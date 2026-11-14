//! Orchestrates the complete tracing pipeline.
//!
//! Pipeline:
//! 1. verify tools (gcc/gdb required; valgrind/addr2line optional)
//! 2. create a unique session temp directory; write the C runtime
//! 3. compile the debug binary (+ instrumentation) and the sanitized binary
//! 4. run GDB/MI2: breakpoint at `main`, step through user code, capture
//!    snapshots, ingest allocation events after every stop, read live heap
//!    contents through GDB within the configured budget
//! 5. resolve allocation caller locations with one batched `addr2line`
//! 6. on crash: run the sanitized binary as a separate diagnostic pass
//! 7. run Valgrind independently and correlate its findings with allocation
//!    ids
//! 8. assemble and return the structured [`DebugSessionTrace`]
//!
//! Partial results are always preserved: a timeout, a step limit or a crash
//! yields a trace containing everything captured up to that point.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::allocation_tracker::{
    apply_caller_locations, resolve_caller_locations, EventFileReader, HeapTimeline,
};
use crate::compiler;
use crate::config::TraceConfig;
use crate::error::{EngineError, Result};
use crate::gdb_mi::{
    capture_state, CaptureOptions, GdbLaunchOptions, GdbSession, StoppedInfo, UserCodeClassifier,
};
use crate::models::{
    CrashReport, DebugSessionTrace, ExecutionSnapshot, ExecutionStatus, HeapBlockInfo,
    ResourceUsage, SanitizerReport, SessionMeta, ValgrindReport,
};
use crate::resource_limits::{CgroupGuard, ExecutionLimits};
use crate::sanitizer;
use crate::valgrind;

/// Cap on heap blocks serialized per snapshot (the timeline itself is capped
/// by `max_heap_allocations`; this bounds per-snapshot JSON size).
const MAX_HEAP_BLOCKS_PER_SNAPSHOT: usize = 512;
/// Per-snapshot cap for the head of captured program output.
const STDOUT_HEAD_CAP: usize = 64 * 1024;
/// Maximum consecutive `-exec-finish` commands through non-user frames.
const MAX_FINISH_CHAIN: usize = 32;
/// Safety margin over the configured wall timeout for the GDB run loop.
const WALL_TIMEOUT_FACTOR: u64 = 4;
const WALL_TIMEOUT_SLACK_SECS: u64 = 30;
/// Maximum wait for one stepping command (the inferior should stop quickly;
/// a hang here is bounded and mapped to the timeout status).
const PER_STEP_TIMEOUT: Duration = Duration::from_secs(10);

const FATAL_SIGNALS: [&str; 7] = [
    "SIGSEGV", "SIGBUS", "SIGILL", "SIGFPE", "SIGABRT", "SIGIOT", "SIGTRAP",
];
const RESOURCE_SIGNALS: [&str; 3] = ["SIGKILL", "SIGXCPU", "SIGXFSZ"];
const BENIGN_SIGNALS: [&str; 9] = [
    "SIGUSR1", "SIGUSR2", "SIGPIPE", "SIGCHLD", "SIGWINCH", "SIGALRM", "SIGVTALRM", "SIGPROF",
    "SIGCONT",
];

/// Entry point: run the full pipeline for a prepared configuration.
pub async fn trace_program(config: TraceConfig) -> Result<DebugSessionTrace> {
    let started = Instant::now();
    let limits = config.limits.clone();
    limits.validate()?;

    let mut trace = DebugSessionTrace::default();
    let mut notes: Vec<String> = Vec::new();

    // ---- tool verification ------------------------------------------------
    let gcc_version = compiler::probe_tool("gcc", "--version").await?; // required
    let gdb_version = compiler::probe_tool("gdb", "--version").await?; // required
    let valgrind_version = match compiler::probe_tool("valgrind", "--version").await {
        Ok(v) => Some(v),
        Err(_) => {
            notes.push("valgrind not found: independent memory analysis skipped".into());
            None
        }
    };
    let addr2line_version = match compiler::probe_tool("addr2line", "--version").await {
        Ok(v) => Some(v),
        Err(_) => {
            notes.push("addr2line not found: allocation call sites not resolved".into());
            None
        }
    };

    // ---- session directory -------------------------------------------------
    let session_dir = make_session_dir()?;
    let tracker_source = crate::allocation_tracker::write_runtime_source(&session_dir)?;

    // ---- compilation -------------------------------------------------------
    let (debug_build, asan_build) =
        compiler::compile_project(&config.build, &session_dir, Some(tracker_source)).await?;
    trace.compiled_successfully = debug_build.success;
    trace.compile_errors = debug_build.diagnostics.clone();

    let mut meta = SessionMeta {
        session_dir: session_dir.display().to_string(),
        debug_binary: debug_build.binary_path.display().to_string(),
        sanitized_binary: if asan_build.success {
            Some(asan_build.binary_path.display().to_string())
        } else {
            None
        },
        gcc_version: Some(gcc_version.clone()),
        gdb_version: Some(gdb_version.clone()),
        valgrind_version: valgrind_version.clone(),
        addr2line_version: addr2line_version.clone(),
        cgroup_enforced: false,
        build: config.build.clone(),
        program_args: config.program_args.clone(),
        working_directory: config
            .inferior_working_directory
            .as_ref()
            .or(config.build.working_directory.as_ref())
            .map(|p| p.display().to_string()),
    };

    if !debug_build.success {
        trace.execution_status = ExecutionStatus::CompileFailed;
        notes.push("compilation failed; no execution or memory analysis was run".into());
        trace.analysis_notes = notes;
        trace.session_meta = Some(meta);
        if !config.keep_temp {
            let _ = std::fs::remove_dir_all(&session_dir);
        }
        return Ok(trace);
    }
    if !asan_build.success {
        let reason = asan_build
            .diagnostics
            .iter()
            .find(|d| !d.message.is_empty())
            .map(|d| d.message.clone())
            .unwrap_or_else(|| asan_build.raw_stderr.lines().last().unwrap_or_default().to_string());
        notes.push(format!(
            "sanitized binary could not be built ({reason}); sanitizer diagnostics disabled"
        ));
    }

    // ---- GDB execution + heap instrumentation -------------------------------
    let classifier = UserCodeClassifier::from_build(&config.build);
    let event_file = session_dir.join("alloc_events.txt");
    let mut timeline = HeapTimeline::new(limits.max_heap_allocations);
    let mut reader = EventFileReader::new(event_file.clone());

    let phase = match run_gdb_phase(
        &config,
        &limits,
        &classifier,
        &debug_build.binary_path,
        &event_file,
        &mut reader,
        &mut timeline,
        &mut notes,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            notes.push(format!("gdb phase failed: {e}"));
            GdbPhaseResult {
                status: ExecutionStatus::DebuggerFailure,
                crash: None,
                exit_code: None,
                inferior_pid: None,
                commands_issued: 0,
                limits_triggered: Vec::new(),
                program_output_bytes: 0,
                program_output_truncated: false,
                snapshots: Vec::new(),
                cgroup_enforced: false,
            }
        }
    };

    // ---- caller location resolution (batched addr2line) ---------------------
    if addr2line_version.is_some() {
        let mut callers: Vec<u64> = timeline
            .blocks()
            .iter()
            .map(|b| b.alloc_caller)
            .filter(|c| *c != 0)
            .collect();
        callers.extend(timeline.event_callers().iter().copied().filter(|c| *c != 0));
        if !callers.is_empty() {
            match resolve_caller_locations(&debug_build.binary_path, &callers).await {
                Ok(resolved) => {
                    if resolved.is_empty() {
                        notes.push("addr2line resolved no allocation call sites".into());
                    }
                    apply_caller_locations(&mut timeline, &resolved);
                }
                Err(e) => notes.push(format!("addr2line failed: {e}")),
            }
        }
    }

    // ---- crash → sanitizer diagnostic pass ----------------------------------
    let status = phase.status.clone();
    let mut crash: Option<CrashReport> = phase.crash.clone();
    let mut sanitizer_report: Option<SanitizerReport> = None;
    if status == ExecutionStatus::Crashed && asan_build.success {
        let report = sanitizer::run_sanitized(
            &asan_build.binary_path,
            &config.program_args,
            config
                .inferior_working_directory
                .as_deref()
                .or(config.build.working_directory.as_deref()),
            &limits,
        )
        .await;
        notes.push(format!("sanitizer pass: {}", report.reproduction));
        if report.reproduction == "reproduced" {
            let summary = report
                .message
                .clone()
                .or_else(|| report.error_category.clone())
                .filter(|s| !s.is_empty());
            if let Some(c) = crash.as_mut() {
                c.asan_summary = summary;
            }
        }
        sanitizer_report = Some(report);
    }

    // ---- Valgrind (independent analysis) ------------------------------------
    let mut valgrind_report: Option<ValgrindReport> = None;
    if valgrind_version.is_some() {
        let vg_xml = session_dir.join("valgrind.xml");
        let vg_events = session_dir.join("valgrind_alloc_events.txt");
        match valgrind::run_valgrind(
            &debug_build.binary_path,
            &config.program_args,
            config
                .inferior_working_directory
                .as_deref()
                .or(config.build.working_directory.as_deref()),
            &limits,
            &vg_xml,
            &vg_events,
        )
        .await
        {
            Ok(run) => {
                if let Some(mut report) = run.report {
                    if run.timed_out {
                        notes.push("valgrind exceeded its timeout: XML may be incomplete".into());
                    }
                    // Correlate valgrind sites with allocation ids by (file, line).
                    let sites: Vec<(String, u32, u64)> = timeline
                        .blocks()
                        .iter()
                        .filter_map(|b| {
                            let file = b.caller_file.clone()?;
                            let line = b.caller_line?;
                            Some((file, line, timeline.global_id(b.pid, b.allocation_id)))
                        })
                        .collect();
                    valgrind::correlate_with_allocations(&mut report, &sites);
                    valgrind_report = Some(report);
                } else {
                    notes.push("valgrind XML could not be parsed (execution terminated?)".into());
                }
            }
            Err(e) => notes.push(format!("valgrind failed: {e}")),
        }
    }

    // ---- assemble ------------------------------------------------------------
    let parent_pid = infer_parent_pid(&timeline, phase.inferior_pid);
    trace.snapshots = backfill_snapshots(&timeline, phase.snapshots, parent_pid);
    trace.heap_events = final_events(&timeline);
    trace.heap_state = build_heap_state(&timeline, &trace.snapshots, parent_pid);
    trace.crash_report = crash;
    trace.sanitizer_report = sanitizer_report;
    trace.valgrind_report = valgrind_report;

    if timeline.blocks_capped {
        notes.push(format!(
            "allocation block cap reached ({}); later allocations recorded as events only",
            limits.max_heap_allocations
        ));
    }

    trace.resource_usage = ResourceUsage {
        wall_time_ms: started.elapsed().as_millis() as u64,
        gdb_commands_issued: phase.commands_issued,
        snapshots_captured: trace.snapshots.len(),
        allocation_events_ingested: trace.heap_events.len(),
        program_output_bytes: phase.program_output_bytes,
        program_output_truncated: phase.program_output_truncated,
        limits_triggered: phase.limits_triggered.clone(),
        inferior_exit_code: phase.exit_code,
    };
    trace.execution_status = status;
    trace.analysis_notes = notes;
    meta.cgroup_enforced = phase.cgroup_enforced;
    trace.session_meta = Some(meta);

    if !config.keep_temp {
        let _ = std::fs::remove_dir_all(&session_dir);
    }
    Ok(trace)
}

// ---------------------------------------------------------------------------
// GDB phase
// ---------------------------------------------------------------------------

/// Everything the GDB execution phase produced.
pub struct GdbPhaseResult {
    pub status: ExecutionStatus,
    pub crash: Option<CrashReport>,
    pub exit_code: Option<i32>,
    pub inferior_pid: Option<u32>,
    pub commands_issued: u64,
    pub limits_triggered: Vec<String>,
    pub program_output_bytes: u64,
    pub program_output_truncated: bool,
    pub snapshots: Vec<ExecutionSnapshot>,
    pub cgroup_enforced: bool,
}

/// Run the debug binary under GDB/MI2, stepping through user code and
/// capturing one snapshot per stop, with live heap contents read through the
/// debugger at each snapshot. Allocation events are ingested after every
/// stop. Never loses partial results: any stop condition (exit, crash,
/// timeout, step limit) terminates the loop gracefully.
#[allow(clippy::too_many_arguments)]
pub async fn run_gdb_phase(
    config: &TraceConfig,
    limits: &ExecutionLimits,
    classifier: &UserCodeClassifier,
    binary: &Path,
    event_file: &Path,
    reader: &mut EventFileReader,
    timeline: &mut HeapTimeline,
    notes: &mut Vec<String>,
) -> Result<GdbPhaseResult> {
    let timeout_secs = limits.timeout_seconds.max(1);
    let wall_deadline = Instant::now()
        + Duration::from_secs(timeout_secs * WALL_TIMEOUT_FACTOR + WALL_TIMEOUT_SLACK_SECS);

    let mut result = GdbPhaseResult {
        status: ExecutionStatus::ExecutionFailure,
        crash: None,
        exit_code: None,
        inferior_pid: None,
        commands_issued: 0,
        limits_triggered: Vec::new(),
        program_output_bytes: 0,
        program_output_truncated: false,
        snapshots: Vec::new(),
        cgroup_enforced: false,
    };

    // Best-effort cgroup v2 enforcement for the inferior tree.
    let cgroup = CgroupGuard::create(limits);

    let inferior_dir = config
        .inferior_working_directory
        .clone()
        .or_else(|| config.build.working_directory.clone());
    let launch_opts = GdbLaunchOptions {
        binary: binary.to_path_buf(),
        working_directory: inferior_dir.clone(),
        program_args: config.program_args.clone(),
        limits: limits.clone(),
        // The inferior inherits GDB's environment; the C runtime reads
        // C_TRACE_EVENT_FILE to find its dedicated event trace file.
        env: vec![(
            "C_TRACE_EVENT_FILE".into(),
            event_file.display().to_string(),
        )],
    };

    let mut session = match GdbSession::launch(&launch_opts).await {
        Ok(s) => s,
        Err(e) => {
            notes.push(format!("could not launch gdb: {e}"));
            result.status = ExecutionStatus::DebuggerFailure;
            return Ok(result);
        }
    };
    result.commands_issued = session.commands_issued.load(std::sync::atomic::Ordering::SeqCst);

    // Breakpoint at main; without it we can still run to completion (no
    // stepping, no snapshots before exit).
    let have_breakpoint = match session.insert_breakpoint("main").await {
        Ok(_) => true,
        Err(e) => {
            notes.push(format!("no breakpoint at main ({e}); running to completion"));
            false
        }
    };

    // Run the inferior; the first stop is the breakpoint (or exit).
    let run_timeout = remaining(wall_deadline).min(Duration::from_secs(timeout_secs * 2 + 10));
    let mut stop = match session.run(run_timeout).await {
        Ok(stop) => stop,
        Err(e) => {
            notes.push(format!("gdb run failed: {e}"));
            if remaining(wall_deadline).is_zero() {
                result.status = ExecutionStatus::Timeout;
                result.limits_triggered.push("timeout_seconds".into());
            } else {
                result.status = ExecutionStatus::ExecutionFailure;
            }
            terminate(&mut session, cgroup.as_ref()).await;
            finish_phase(&mut result, &session, timeline, reader, notes);
            return Ok(result);
        }
    };
    result.inferior_pid = session.inferior_pid;
    if let (Some(cg), Some(pid)) = (cgroup.as_ref(), session.inferior_pid) {
        result.cgroup_enforced = cg.attach(pid);
        if !result.cgroup_enforced {
            notes.push(
                "cgroup v2 enforcement unavailable (not delegated); using rlimits only".into(),
            );
        }
    }

    let capture_opts = CaptureOptions {
        classifier: classifier.clone(),
        max_variables: limits.max_variables_per_snapshot,
        max_memory_read: limits.max_memory_read_bytes,
        memory_budget: limits.max_snapshot_memory_bytes,
    };

    let mut steps: usize = 0;
    let mut finish_chain: usize = 0;
    let mut snapshot_cap_noted = false;
    let mut memory_cap_noted = false;
    let mut read_cap_noted = false;

    loop {
        // Ingest allocation events emitted since the last stop. The inferior
        // is stopped here, so the event file is complete up to now.
        for ev in reader.read_new() {
            timeline.ingest(&ev);
        }
        let dbg = std::env::var_os("C_TRACE_DEBUG").is_some();
        if dbg {
            eprintln!(
                "[trace] loop enter: reason={:?} signal={:?} frame={:?} raw={:?}",
                stop.reason,
                stop.signal_name,
                stop.frame.as_ref().map(|f| (f.func.clone(), f.line)),
                stop.raw
            );
        }

        // --- classify the stop ---------------------------------------------------
        if stop.is_exit() {
            result.exit_code = exit_code_from(&mut session, &stop);
            result.status = ExecutionStatus::Completed;
            break;
        }
        if stop.is_signal() {
            match classify_signal(stop.signal_name.as_deref()) {
                SignalClass::Fatal => {
                    // Capture the crash state (registers are still valid).
                    let state = capture_or_empty(&mut session, &capture_opts).await;
                    let pid = inferior_pid_of(&session);
                    push_snapshot(
                        &mut result,
                        &mut session,
                        &stop,
                        &state,
                        timeline,
                        pid,
                        limits,
                        &mut snapshot_cap_noted,
                        &mut memory_cap_noted,
                        &mut read_cap_noted,
                        notes,
                    )
                    .await;
                    result.crash = Some(crash_from(&stop, &state));
                    result.status = ExecutionStatus::Crashed;
                    break;
                }
                SignalClass::Resource => {
                    result.status = ExecutionStatus::ResourceLimitReached;
                    result
                        .limits_triggered
                        .push(stop.signal_name.clone().unwrap_or_default());
                    break;
                }
                SignalClass::Benign => {
                    // Resume execution; counts as a step so signal storms are
                    // bounded by max_steps.
                    steps += 1;
                    if steps > limits.max_steps {
                        result.status = ExecutionStatus::StepLimitReached;
                        break;
                    }
                    if Instant::now() >= wall_deadline {
                        result.status = ExecutionStatus::Timeout;
                        result.limits_triggered.push("timeout_seconds".into());
                        break;
                    }
                    stop = match session.exec_continue(step_timeout(wall_deadline)).await {
                        Ok(s) => s,
                        Err(e) => {
                            notes.push(format!("continue after benign signal failed: {e}"));
                            result.status = ExecutionStatus::ExecutionFailure;
                            break;
                        }
                    };
                    continue;
                }
            }
        }

        // --- capture snapshot (exit stops broke out above) ------------------------
        let state = capture_or_empty(&mut session, &capture_opts).await;
        let pid = inferior_pid_of(&session);
        push_snapshot(
            &mut result,
            &mut session,
            &stop,
            &state,
            timeline,
            pid,
            limits,
            &mut snapshot_cap_noted,
            &mut memory_cap_noted,
            &mut read_cap_noted,
            notes,
        )
        .await;

        // --- resource checks -------------------------------------------------------
        if Instant::now() >= wall_deadline {
            result.status = ExecutionStatus::Timeout;
            result.limits_triggered.push("timeout_seconds".into());
            break;
        }
        if steps >= limits.max_steps {
            result.limits_triggered.push("max_steps".into());
            result.status = ExecutionStatus::StepLimitReached;
            break;
        }

        // --- decide the next stepping command ---------------------------------------
        let dbg = std::env::var_os("C_TRACE_DEBUG").is_some();
        let is_user = state.is_user_code;
        if dbg {
            eprintln!(
                "[trace] step={} finish_chain={} is_user={} stop={:?} frame={:?}",
                steps,
                finish_chain,
                is_user,
                stop.reason,
                stop.frame.as_ref().map(|f| (f.func.clone(), f.file.clone(), f.line))
            );
        }
        let next_stop = if is_user || !have_breakpoint {
            // User code: single-step (this enters calls too, which is how we
            // visit helper functions in the user's project).
            steps += 1;
            finish_chain = 0;
            match session.exec_step(step_timeout(wall_deadline)).await {
                Ok(s) => s,
                Err(EngineError::GdbCommandTimeout { .. }) => {
                    // The inferior hung inside a step (blocking syscall, long
                    // loop): preserve partials and report the timeout.
                    debug_dump_console(&session, "exec-step timed out");
                    result.status = ExecutionStatus::Timeout;
                    result.limits_triggered.push("timeout_seconds".into());
                    break;
                }
                Err(e) => {
                    // Transient GDB states usually resolve on a plain
                    // continue; try once, bounded.
                    match session.exec_continue(step_timeout(wall_deadline)).await {
                        Ok(s) => {
                            notes.push(format!("recovered from `{e}` via continue"));
                            s
                        }
                        Err(_) => {
                            debug_dump_console(&session, "exec-step failed");
                            notes.push(format!("step failed: {e}"));
                            result.status = ExecutionStatus::ExecutionFailure;
                            break;
                        }
                    }
                }
            }
        } else {
            // Non-user frame: run until this frame returns so we come back
            // into user code without single-stepping the standard library.
            // Exception: GDB occasionally reports a stop with neither a
            // reason nor a signal — e.g. when it failed to insert the
            // temporary breakpoint of `-exec-finish` while unwinding the
            // outermost libc frame ("Cannot access memory at address
            // 0x101"). Such stops cannot be finished; they must simply be
            // continued (this resumes the (almost always imminent) exit).
            let no_reason_stop = stop.reason.is_none() && stop.signal_name.is_none();
            if finish_chain >= MAX_FINISH_CHAIN || no_reason_stop {
                steps += 1;
                if no_reason_stop {
                    finish_chain = 0;
                }
                match session.exec_continue(step_timeout(wall_deadline)).await {
                    Ok(s) => s,
                    Err(e) => {
                        notes.push(format!("continue failed: {e}"));
                        result.status = ExecutionStatus::ExecutionFailure;
                        break;
                    }
                }
            } else {
                finish_chain += 1;
                match session.exec_finish(step_timeout(wall_deadline)).await {
                    Ok(s) => s,
                    Err(EngineError::GdbCommandError { message, .. })
                        if message.contains("finish not meaningful") =>
                    {
                        // Outermost frame: the program is finishing; continue.
                        steps += 1;
                        match session.exec_continue(step_timeout(wall_deadline)).await {
                            Ok(s) => s,
                            Err(e) => {
                                notes.push(format!("continue failed: {e}"));
                                result.status = ExecutionStatus::ExecutionFailure;
                                break;
                            }
                        }
                    }
                    Err(EngineError::GdbCommandTimeout { .. }) => {
                        debug_dump_console(&session, "exec-finish timed out");
                        result.status = ExecutionStatus::Timeout;
                        result.limits_triggered.push("timeout_seconds".into());
                        break;
                    }
                    Err(e) => {
                        // Transient GDB states (an execution command aborted
                        // right after a failed breakpoint insert) usually
                        // resolve on a plain continue; try once, bounded.
                        match session.exec_continue(step_timeout(wall_deadline)).await {
                            Ok(s) => {
                                notes.push(format!("recovered from `{e}` via continue"));
                                s
                            }
                            Err(_) => {
                                debug_dump_console(&session, "exec-finish failed");
                                notes.push(format!("finish failed: {e}"));
                                result.status = ExecutionStatus::ExecutionFailure;
                                break;
                            }
                        }
                    }
                }
            }
        };
        // The finish chain resets once execution is back in user code.
        if let Some(frame) = &next_stop.frame {
            if classifier.is_user_file(frame) {
                finish_chain = 0;
            }
        }
        stop = next_stop;
    }

    // --- termination -----------------------------------------------------------
    match result.status {
        // Normal and crash endings: GDB shuts down gracefully.
        ExecutionStatus::Completed | ExecutionStatus::Crashed => {
            terminate(&mut session, cgroup.as_ref()).await;
        }
        // Limit endings: hard-kill the inferior tree so nothing survives.
        ExecutionStatus::Timeout
        | ExecutionStatus::StepLimitReached
        | ExecutionStatus::ResourceLimitReached
        | ExecutionStatus::ExecutionFailure
        | ExecutionStatus::DebuggerFailure => {
            if let Some(cg) = cgroup.as_ref() {
                cg.kill_all();
            }
            session.kill().await;
        }
        ExecutionStatus::CompileFailed => {
            terminate(&mut session, cgroup.as_ref()).await;
        }
    }

    // Post-mortem events (destructors, exit handlers) may have landed after
    // the final stop.
    for ev in reader.read_new() {
        timeline.ingest(&ev);
    }
    finish_phase(&mut result, &session, timeline, reader, notes);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Phase helpers
// ---------------------------------------------------------------------------

enum SignalClass {
    Fatal,
    Resource,
    Benign,
}

fn classify_signal(name: Option<&str>) -> SignalClass {
    match name {
        Some(n) if FATAL_SIGNALS.contains(&n) => SignalClass::Fatal,
        Some(n) if RESOURCE_SIGNALS.contains(&n) => SignalClass::Resource,
        Some(n) if BENIGN_SIGNALS.contains(&n) => SignalClass::Benign,
        // Unknown signal: conservatively fatal so we stop and report it.
        _ => SignalClass::Fatal,
    }
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

fn step_timeout(wall_deadline: Instant) -> Duration {
    remaining(wall_deadline)
        .min(PER_STEP_TIMEOUT)
        .max(Duration::from_secs(1))
}

fn inferior_pid_of(session: &GdbSession) -> Option<u32> {
    session.inferior_pid
}

/// Debug aid: print the tail of GDB's raw MI stream (only when
/// C_TRACE_DEBUG=1) so live protocol misbehaviour can be diagnosed.
fn debug_dump_console(session: &GdbSession, why: &str) {
    if !std::env::var_os("C_TRACE_DEBUG").is_some() {
        return;
    }
    let log = session.console_log();
    let tail_start = log.len().saturating_sub(6000);
    eprintln!("[trace] console tail at {why}:\n{}", &log[tail_start..]);
}

/// Graceful shutdown (normal end): cgroup kill is a no-op safety net and GDB
/// exits cleanly, closing the pty.
async fn terminate(session: &mut GdbSession, cgroup: Option<&CgroupGuard>) {    if let Some(cg) = cgroup {
        cg.kill_all();
    }
    session.kill().await;
}

/// Finalize a phase result: output counters, cap notes.
fn finish_phase(
    result: &mut GdbPhaseResult,
    session: &GdbSession,
    timeline: &HeapTimeline,
    reader: &EventFileReader,
    notes: &mut Vec<String>,
) {
    result.commands_issued = session.commands_issued.load(std::sync::atomic::Ordering::SeqCst);
    let (_, total, truncated) = session.program_output();
    result.program_output_bytes = total;
    result.program_output_truncated = truncated;
    if truncated {
        result.limits_triggered.push("max_output_bytes".into());
    }
    if timeline.blocks_capped && !result.limits_triggered.contains(&"max_heap_allocations".into()) {
        result.limits_triggered.push("max_heap_allocations".into());
    }
    if timeline.events_capped {
        notes.push("allocation event history cap reached; later events dropped".into());
    }
    if !reader.parse_errors.is_empty() {
        notes.push(format!(
            "{} allocation event line(s) failed to parse",
            reader.parse_errors.len()
        ));
    }
}

/// Capture GDB state, degrading to an empty capture on failure (e.g. dead
/// inferior) instead of losing the stop.
async fn capture_or_empty(
    session: &mut GdbSession,
    opts: &CaptureOptions,
) -> crate::gdb_mi::CapturedState {
    capture_state(session, opts)
        .await
        .unwrap_or_default()
}/// Build and append one snapshot (with live heap reads inside the budget).
#[allow(clippy::too_many_arguments)]
async fn push_snapshot(
    result: &mut GdbPhaseResult,
    session: &mut GdbSession,
    stop: &StoppedInfo,
    state: &crate::gdb_mi::CapturedState,
    timeline: &HeapTimeline,
    pid: Option<u32>,
    limits: &ExecutionLimits,
    snapshot_cap_noted: &mut bool,
    memory_cap_noted: &mut bool,
    read_cap_noted: &mut bool,
    notes: &mut Vec<String>,
) {
    if result.snapshots.len() >= limits.max_snapshot_count {
        if !*snapshot_cap_noted {
            result.limits_triggered.push("max_snapshot_count".into());
            *snapshot_cap_noted = true;
        }
        return;
    }

    // Live heap reads within the remaining per-snapshot budget (the budget is
    // shared with the variable raw-byte reads performed by capture_state).
    let mut heap_blocks: Vec<HeapBlockInfo> = Vec::new();
    let mut budget_left = limits
        .max_snapshot_memory_bytes
        .saturating_sub(state.memory_used);
    let mut memory_truncated = budget_left == 0;
    if let Some(pid) = pid {
        let alive = timeline.alive_blocks(pid);
        let over_block_cap = alive.len() > MAX_HEAP_BLOCKS_PER_SNAPSHOT;
        for block in alive.into_iter().take(MAX_HEAP_BLOCKS_PER_SNAPSHOT) {
            let addr = format!("0x{:x}", block.addr);
            let read_size = block
                .size
                .min(limits.max_memory_read_bytes as u64)
                .min(budget_left as u64) as usize;
            let mut info = HeapBlockInfo {
                allocation_id: block.allocation_id,
                address: addr.clone(),
                size_bytes: block.size,
                allocation_type: block.alloc_type.to_string(),
                is_freed: false,
                allocation_file: block.caller_file.clone(),
                allocation_line: block.caller_line,
                raw_bytes_hex: None,
                memory_read_error: None,
                realloc_from_id: block.realloc_from_id,
            };
            if block.size as usize > limits.max_memory_read_bytes && !*read_cap_noted {
                result.limits_triggered.push("max_memory_read_bytes".into());
                *read_cap_noted = true;
            }
            if read_size == 0 {
                info.memory_read_error = Some("snapshot memory budget exhausted".into());
                memory_truncated = true;
                heap_blocks.push(info);
                continue;
            }
            match session.read_memory_bytes(&addr, read_size as u64).await {
                Ok(hex) => info.raw_bytes_hex = Some(hex),
                Err(e) => info.memory_read_error = Some(format!("memory read failed: {e}")),
            }
            budget_left = budget_left.saturating_sub(read_size);
            if read_size < block.size as usize {
                memory_truncated = true; // partial capture
            }
            heap_blocks.push(info);
        }
        if over_block_cap {
            memory_truncated = true;
            notes.push(format!(
                "snapshot {} heap truncated to {} blocks",
                result.snapshots.len(),
                MAX_HEAP_BLOCKS_PER_SNAPSHOT
            ));
        }
    }
    if memory_truncated && !*memory_cap_noted {
        result.limits_triggered.push("max_snapshot_memory_bytes".into());
        *memory_cap_noted = true;
    }

    // Program output accumulated so far (head-truncated per snapshot).
    let (output, _, output_truncated) = session.program_output();
    let stdout_head = char_safe_head(&output, STDOUT_HEAD_CAP);

    let frame = state
        .current
        .clone()
        .or_else(|| stop.frame.clone())
        .unwrap_or_default();
    result.snapshots.push(ExecutionSnapshot {
        step_index: result.snapshots.len(),
        file: frame.file.unwrap_or_default(),
        line: frame.line.unwrap_or(0),
        func: frame.func,
        call_stack: state.frames.clone(),
        variables: state.variables.clone(),
        heap_blocks,
        stdout_so_far: stdout_head,
        stopped_reason: stop.reason.clone(),
        signal: stop.signal_name.clone(),
        memory_truncated,
        stdout_truncated: output_truncated,
    });
}

fn char_safe_head(s: &str, cap_bytes: usize) -> String {
    if s.len() <= cap_bytes {
        return s.to_string();
    }
    let mut cut = cap_bytes;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_string()
}

fn crash_from(stop: &StoppedInfo, state: &crate::gdb_mi::CapturedState) -> CrashReport {
    let frame = stop.frame.clone().or_else(|| state.current.clone());
    CrashReport {
        signal_name: stop
            .signal_name
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        file: frame.as_ref().and_then(|f| f.file.clone()),
        line: frame.as_ref().and_then(|f| f.line),
        func: frame
            .as_ref()
            .map(|f| f.func.clone())
            .filter(|s| !s.is_empty()),
        asan_summary: None,
        backtrace: state.frames.clone(),
        detail: stop.signal_meaning.clone(),
    }
}

/// Inferior exit code: from the stop record, else from the
/// `=thread-group-exited` notification in the backlog. MI emits the value as
/// decimal in current versions but hex in some older ones; both accepted.
fn exit_code_from(session: &mut GdbSession, stop: &StoppedInfo) -> Option<i32> {
    if let Some(code) = &stop.exit_code {
        return parse_mi_exit_code(code);
    }
    for record in session.take_backlog() {
        if let crate::mi_parser::MiRecord::Notify { class, results } = &record {
            if class == "thread-group-exited" {
                for r in results {
                    if r.variable == "exit-code" {
                        if let Some(v) = crate::mi_parser::as_const(&r.value) {
                            if let Some(c) = parse_mi_exit_code(v) {
                                return Some(c);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn parse_mi_exit_code(text: &str) -> Option<i32> {
    if let Ok(v) = text.parse::<i32>() {
        return Some(v);
    }
    u32::from_str_radix(text, 16).ok().map(|v| v as i32)
}

// ---------------------------------------------------------------------------
// Timeline helpers
// ---------------------------------------------------------------------------

fn infer_parent_pid(timeline: &HeapTimeline, inferior_pid: Option<u32>) -> Option<u32> {
    if let Some(pid) = inferior_pid {
        return Some(pid);
    }
    let pids = timeline.pids_seen();
    if pids.len() == 1 {
        Some(pids[0])
    } else {
        None
    }
}

/// Fill snapshot heap blocks with resolved allocation locations and global
/// (pid-widened) ids.
fn backfill_snapshots(
    timeline: &HeapTimeline,
    mut snapshots: Vec<ExecutionSnapshot>,
    parent_pid: Option<u32>,
) -> Vec<ExecutionSnapshot> {
    let Some(parent_pid) = parent_pid else {
        return snapshots;
    };
    for snap in snapshots.iter_mut() {
        for block in snap.heap_blocks.iter_mut() {
            if let Some(state) = timeline.block_by_id(parent_pid, block.allocation_id) {
                block.allocation_file = state.caller_file.clone();
                block.allocation_line = state.caller_line;
                block.realloc_from_id = state
                    .realloc_from_id
                    .map(|id| timeline.global_id(parent_pid, id));
                block.allocation_id = timeline.global_id(parent_pid, block.allocation_id);
            }
        }
    }
    snapshots
}

/// Serialized heap events with pid-widened ids when multiple processes ran.
fn final_events(timeline: &HeapTimeline) -> Vec<crate::models::HeapEvent> {
    timeline
        .events()
        .iter()
        .map(|e| {
            let mut e = e.clone();
            e.allocation_id = timeline.global_id(e.pid, e.allocation_id);
            if let Some(old) = e.old_allocation_id {
                e.old_allocation_id = Some(timeline.global_id(e.pid, old));
            }
            e
        })
        .collect()
}

/// Final heap state: every reconstructed block with its end-of-run status.
/// `raw_bytes_hex` carries the last bytes captured while the block was alive
/// (from any snapshot); freed blocks keep that historical capture, live
/// blocks the most recent one.
fn build_heap_state(
    timeline: &HeapTimeline,
    snapshots: &[ExecutionSnapshot],
    parent_pid: Option<u32>,
) -> Vec<HeapBlockInfo> {
    let mut last_contents: HashMap<u64, String> = HashMap::new();
    for snap in snapshots {
        for b in &snap.heap_blocks {
            if let Some(hex) = &b.raw_bytes_hex {
                last_contents.insert(b.allocation_id, hex.clone());
            }
        }
    }
    timeline
        .blocks()
        .iter()
        .map(|b| {
            let global = parent_pid
                .map(|p| timeline.global_id(p, b.allocation_id))
                .unwrap_or(b.allocation_id);
            HeapBlockInfo {
                allocation_id: global,
                address: format!("0x{:x}", b.addr),
                size_bytes: b.size,
                allocation_type: b.alloc_type.to_string(),
                is_freed: b.freed,
                allocation_file: b.caller_file.clone(),
                allocation_line: b.caller_line,
                raw_bytes_hex: last_contents.get(&global).cloned(),
                memory_read_error: None,
                realloc_from_id: b.realloc_from_id.map(|id| timeline.global_id(b.pid, id)),
            }
        })
        .collect()
}

fn make_session_dir() -> Result<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("c-trace-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation_tracker::parse_event_line;

    #[test]
    fn signal_classification() {
        assert!(matches!(classify_signal(Some("SIGSEGV")), SignalClass::Fatal));
        assert!(matches!(classify_signal(Some("SIGABRT")), SignalClass::Fatal));
        assert!(matches!(
            classify_signal(Some("SIGXCPU")),
            SignalClass::Resource
        ));
        assert!(matches!(
            classify_signal(Some("SIGKILL")),
            SignalClass::Resource
        ));
        assert!(matches!(classify_signal(Some("SIGUSR1")), SignalClass::Benign));
        assert!(matches!(classify_signal(Some("SIGPIPE")), SignalClass::Benign));
        assert!(matches!(classify_signal(None), SignalClass::Fatal));
        assert!(matches!(
            classify_signal(Some("SIGUNKNOWN999")),
            SignalClass::Fatal
        ));
    }

    #[test]
    fn exit_code_parsing() {
        assert_eq!(parse_mi_exit_code("0"), Some(0));
        assert_eq!(parse_mi_exit_code("42"), Some(42));
        assert_eq!(parse_mi_exit_code("ff"), Some(255));
        assert_eq!(parse_mi_exit_code("bogus"), None);
    }

    #[test]
    fn char_safe_head_respects_boundaries() {
        let s = "aé".repeat(1000);
        let head = char_safe_head(&s, 1001);
        assert!(head.len() <= 1001);
        assert!(head.starts_with("aé"));
        assert_eq!(char_safe_head("short", 100), "short");
    }

    #[test]
    fn heap_state_uses_last_alive_contents() {
        let mut timeline = HeapTimeline::new(100);
        timeline.ingest(
            &parse_event_line("102772 1 malloc 1 7f0000000000 4 0 0 401000 ok").unwrap(),
        );
        let snap = ExecutionSnapshot {
            step_index: 0,
            file: "x.c".into(),
            line: 1,
            func: "main".into(),
            call_stack: vec![],
            variables: vec![],
            heap_blocks: vec![HeapBlockInfo {
                allocation_id: 1,
                address: "0x7f0000000000".into(),
                size_bytes: 4,
                allocation_type: "malloc".into(),
                is_freed: false,
                allocation_file: None,
                allocation_line: None,
                raw_bytes_hex: Some("deadbeef".into()),
                memory_read_error: None,
                realloc_from_id: None,
            }],
            stdout_so_far: String::new(),
            stopped_reason: None,
            signal: None,
            memory_truncated: false,
            stdout_truncated: false,
        };
        timeline.ingest(
            &parse_event_line("102772 2 free 1 7f0000000000 4 0 0 401000 ok").unwrap(),
        );
        let state = build_heap_state(&timeline, &[snap], Some(102772));
        assert_eq!(state.len(), 1);
        assert!(state[0].is_freed);
        assert_eq!(state[0].raw_bytes_hex.as_deref(), Some("deadbeef"));
        assert_eq!(state[0].allocation_id, 1);
    }

    #[test]
    fn event_ids_widen_only_for_multi_pid() {
        let mut timeline = HeapTimeline::new(100);
        timeline.ingest(
            &parse_event_line("102772 1 malloc 1 7f0000000000 4 0 0 401000 ok").unwrap(),
        );
        let events = final_events(&timeline);
        assert_eq!(events[0].allocation_id, 1);

        timeline.ingest(
            &parse_event_line("102900 1 malloc 1 7f0000000010 4 0 0 401000 ok").unwrap(),
        );
        let events = final_events(&timeline);
        assert_eq!(events[0].allocation_id, (102772u64 << 32) | 1);
        assert_eq!(events[1].allocation_id, (102900u64 << 32) | 1);
    }
}

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
    ResourceUsage, SanitizerReport, SessionMeta, StageResult, StageStatus, ValgrindReport,
};
use crate::resource_limits::{CgroupGuard, ExecutionLimits};
use crate::sanitizer;
use crate::valgrind;

/// Cap on heap blocks serialized per snapshot (the timeline itself is capped
/// by `max_heap_allocations`; this bounds per-snapshot JSON size).
const MAX_HEAP_BLOCKS_PER_SNAPSHOT: usize = 512;

/// Run the ASan/UBSan reproduction pass once and record its stage result.
///
/// V2.1 (F-02): called either before Valgrind (trigger already satisfied by a
/// crash, or mode `always`) or after it (trigger depends on Valgrind
/// findings). Respects the overall session deadline and engine cancellation;
/// returns `None` without a stage result only when the caller decides the
/// pass was not requested at all.
#[allow(clippy::too_many_arguments)]
async fn run_sanitizer_pass(
    asan_binary: &Path,
    program_args: &[String],
    working_directory: Option<&Path>,
    limits: &ExecutionLimits,
    crash: &mut Option<CrashReport>,
    started: Instant,
    sanitizer_started: Instant,
    trace: &mut DebugSessionTrace,
    notes: &mut Vec<String>,
) -> Option<SanitizerReport> {
    if Instant::now() >= crate::supervisor::deadline() || crate::supervisor::cancelled() {
        trace.stage_results.sanitizer = skipped_stage(if crate::supervisor::cancelled() {
            "session interrupted"
        } else {
            "overall session deadline exhausted"
        });
        return None;
    }
    let mut report =
        sanitizer::run_sanitized(asan_binary, program_args, working_directory, limits).await;
    if report.reproduction == "reproduced"
        && !crash
            .as_ref()
            .is_some_and(|c| sanitizer::matches_crash(&report, c))
    {
        report.reproduction = "diagnostic_observed".into();
    }
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
    trace.stage_results.sanitizer = crate::supervisor::stage(
        started,
        sanitizer_started,
        if report.cancelled {
            StageStatus::Cancelled
        } else if report.timed_out {
            StageStatus::TimedOut
        } else if report.reproduction == "inconclusive" || report.reproduction == "skipped" {
            StageStatus::Failed
        } else {
            StageStatus::Completed
        },
        report.exit_code,
        Some(report.reproduction.clone()),
        report.output_complete,
        report.sanitizer.is_some(),
    );
    Some(report)
}
/// Per-snapshot cap for the head of captured program output.
const STDOUT_HEAD_CAP: usize = 64 * 1024;
/// Maximum consecutive `-exec-finish` commands through non-user frames.
const MAX_FINISH_CHAIN: usize = 32;
/// Maximum wait for one stepping command (the inferior should stop quickly;
/// a hang here is bounded and mapped to the timeout status).
const PER_STEP_TIMEOUT: Duration = Duration::from_secs(10);

const FATAL_SIGNALS: [&str; 7] = [
    "SIGSEGV", "SIGBUS", "SIGILL", "SIGFPE", "SIGABRT", "SIGIOT", "SIGTRAP",
];
const RESOURCE_SIGNALS: [&str; 2] = ["SIGXCPU", "SIGXFSZ"];
const BENIGN_SIGNALS: [&str; 9] = [
    "SIGUSR1",
    "SIGUSR2",
    "SIGPIPE",
    "SIGCHLD",
    "SIGWINCH",
    "SIGALRM",
    "SIGVTALRM",
    "SIGPROF",
    "SIGCONT",
];

/// Entry point: run the full pipeline for a prepared configuration.
pub async fn trace_program(config: TraceConfig) -> Result<DebugSessionTrace> {
    config.limits.validate()?;
    let tracing_configuration = config.tracing.clone();
    let execution_limits = config.limits.clone();
    crate::supervisor::scope(config.limits.timeout_seconds.max(1), async {
        match trace_program_inner(config).await {
            Err(EngineError::Cancelled) => Ok(DebugSessionTrace {
                tracing_configuration,
                execution_limits,
                execution_status: ExecutionStatus::Interrupted,
                execution: crate::models::ExecutionOutcome {
                    outcome: crate::models::ProgramOutcome::Interrupted,
                    reason: Some("session interrupted before execution".into()),
                    ..Default::default()
                },
                stage_results: crate::models::StageResults {
                    compiler: StageResult {
                        status: Some(StageStatus::Cancelled),
                        reason: Some("session interrupted".into()),
                        output_complete: Some(false),
                        ..Default::default()
                    },
                    debugger: skipped_stage("session interrupted"),
                    valgrind: skipped_stage("session interrupted"),
                    sanitizer: skipped_stage("session interrupted"),
                    allocation_tracker: skipped_stage("session interrupted"),
                },
                ..Default::default()
            }),
            result => result,
        }
    })
    .await
}

async fn trace_program_inner(config: TraceConfig) -> Result<DebugSessionTrace> {
    let started = Instant::now();
    let session_deadline = crate::supervisor::deadline();
    let limits = config.limits.clone();
    limits.validate()?;
    if !config.tracing.retain_instruction_events && config.tracing.journal.is_none() {
        return Err(EngineError::Config(
            "journal-only recording requires a journal path".into(),
        ));
    }
    for expression in config
        .tracing
        .globals
        .iter()
        .chain(config.tracing.memory_regions.iter().map(|r| &r.address))
    {
        if expression.contains('=')
            || expression.contains(';')
            || expression.contains("++")
            || expression.contains("--")
            || expression.contains('{')
            || expression.contains('}')
        {
            return Err(EngineError::Config(
                "inspection expressions must not contain side effects".into(),
            ));
        }
    }

    let mut trace = DebugSessionTrace {
        tracing_configuration: config.tracing.clone(),
        execution_limits: limits.clone(),
        ..Default::default()
    };
    let mut notes: Vec<String> = Vec::new();

    // ---- tool verification ------------------------------------------------
    // V2.1 (F-05): `--gdb-path`/`--valgrind-path` override PATH resolution for
    // the probe and the actual launch; unset paths resolve via PATH (execvp
    // semantics), so tool absence is detectable by config/CLI override.
    let gdb_executable = config.gdb_path.as_deref().unwrap_or("gdb");
    let valgrind_executable = config.valgrind_path.as_deref().unwrap_or("valgrind");
    let gcc_version = compiler::probe_tool("gcc", "--version").await?; // required
    let gdb_version = compiler::probe_tool(gdb_executable, "--version").await?; // required
    let valgrind_version = match compiler::probe_tool(valgrind_executable, "--version").await {
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
    let compiler_started = Instant::now();
    let build_deadline = crate::supervisor::stage_deadline(limits.compiler_timeout_seconds);
    let build = crate::supervisor::scope_until(
        build_deadline,
        compiler::compile_project(&config.build, &session_dir, Some(tracker_source)),
    )
    .await;
    let (debug_build, asan_build) = match build {
        Ok(build) => build,
        Err(error) => {
            let reason = error.to_string();
            let timed_out = Instant::now() >= build_deadline || reason.contains("deadline");
            trace.execution_status = if crate::supervisor::cancelled() {
                ExecutionStatus::Interrupted
            } else if timed_out {
                ExecutionStatus::Timeout
            } else {
                ExecutionStatus::ExecutionFailure
            };
            trace.execution.outcome = if crate::supervisor::cancelled() {
                crate::models::ProgramOutcome::Interrupted
            } else if timed_out {
                crate::models::ProgramOutcome::TimedOut
            } else {
                crate::models::ProgramOutcome::InternalFailure
            };
            trace.execution.reason = Some(reason.clone());
            trace.stage_results.compiler = crate::supervisor::stage(
                started,
                compiler_started,
                if crate::supervisor::cancelled() {
                    StageStatus::Cancelled
                } else if timed_out {
                    StageStatus::TimedOut
                } else {
                    StageStatus::Failed
                },
                None,
                Some(reason.clone()),
                false,
                false,
            );
            trace.analysis_notes.push(reason);
            trace.stage_results.debugger = skipped_stage("compilation did not complete");
            trace.stage_results.valgrind = skipped_stage("compilation did not complete");
            trace.stage_results.sanitizer = skipped_stage("compilation did not complete");
            trace.stage_results.allocation_tracker = skipped_stage("program not started");
            trace.resource_usage.wall_time_ms = started.elapsed().as_millis() as u64;
            if timed_out {
                trace
                    .resource_usage
                    .limits_triggered
                    .push("compiler_deadline".into());
            }
            if !config.keep_temp {
                let _ = std::fs::remove_dir_all(&session_dir);
            }
            return Ok(trace);
        }
    };
    trace.stage_results.compiler = crate::supervisor::stage(
        started,
        compiler_started,
        if debug_build.success {
            StageStatus::Completed
        } else {
            StageStatus::Failed
        },
        Some(debug_build.exit_code),
        (!debug_build.success).then(|| {
            debug_build
                .diagnostics
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_else(|| debug_build.raw_stderr.clone())
        }),
        true,
        false,
    );
    trace.compiled_successfully = debug_build.success;
    trace.compile_errors = debug_build.diagnostics.clone();

    let mut meta = SessionMeta {
        sanitizer_variant: asan_build.sanitizer_variant.clone(),
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
        trace.stage_results.debugger = skipped_stage("compilation failed");
        trace.stage_results.valgrind = skipped_stage("compilation failed");
        trace.stage_results.sanitizer = skipped_stage("compilation failed");
        trace.stage_results.allocation_tracker = skipped_stage("program not started");
        trace.execution_status = ExecutionStatus::CompileFailed;
        trace.execution.outcome = crate::models::ProgramOutcome::CompileFailed;
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
            .unwrap_or_else(|| {
                asan_build
                    .raw_stderr
                    .lines()
                    .last()
                    .unwrap_or_default()
                    .to_string()
            });
        notes.push(format!(
            "sanitized binary could not be built ({reason}); sanitizer diagnostics disabled"
        ));
    } else if asan_build.sanitizer_variant.as_deref() != Some("address,undefined") {
        notes.push(format!(
            "sanitizer build degraded to {}; combined runtime unavailable",
            asan_build.sanitizer_variant.as_deref().unwrap_or("unknown")
        ));
    }

    // ---- GDB execution + heap instrumentation -------------------------------
    let classifier = UserCodeClassifier::from_build(&config.build);
    let event_file = session_dir.join("alloc_events.txt");
    let mut timeline = HeapTimeline::new(limits.max_heap_allocations);
    let mut reader = EventFileReader::new(event_file.clone());

    let debugger_started = Instant::now();
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
            let mut status = match &e {
                EngineError::LimitReached("max_output_bytes") => {
                    ExecutionStatus::OutputLimitReached
                }
                EngineError::LimitReached(_) => ExecutionStatus::StepLimitReached,
                _ => ExecutionStatus::ExecutionFailure,
            };
            if crate::supervisor::cancelled() {
                status = ExecutionStatus::Interrupted;
            }
            let mut recording = crate::models::RecordingData::default();
            recording.coverage.truncation_reasons.push(e.to_string());
            GdbPhaseResult {
                status,
                termination_signal: None,
                crash: None,
                exit_code: None,
                inferior_pid: None,
                commands_issued: 0,
                limits_triggered: match &e {
                    EngineError::LimitReached(name) => vec![(*name).to_string()],
                    _ => Vec::new(),
                },
                program_output_bytes: 0,
                program_output_truncated: false,
                snapshots: Vec::new(),
                cgroup_enforced: false,
                recording,
                executable_bias: None,
            }
        }
    };
    trace.stage_results.debugger = crate::supervisor::stage(
        started,
        debugger_started,
        match phase.status {
            ExecutionStatus::Interrupted if crate::supervisor::cancelled() => {
                StageStatus::Cancelled
            }
            ExecutionStatus::Timeout => StageStatus::TimedOut,
            ExecutionStatus::DebuggerFailure | ExecutionStatus::ExecutionFailure => {
                StageStatus::Failed
            }
            _ => StageStatus::Completed,
        },
        phase.exit_code,
        phase
            .limits_triggered
            .first()
            .cloned()
            .or_else(|| crate::supervisor::cancelled().then(|| "session interrupted".into()))
            .or_else(|| {
                matches!(
                    phase.status,
                    ExecutionStatus::DebuggerFailure | ExecutionStatus::ExecutionFailure
                )
                .then(|| notes.last().cloned())
                .flatten()
            }),
        matches!(
            phase.status,
            ExecutionStatus::Completed | ExecutionStatus::Crashed
        ),
        !phase.snapshots.is_empty(),
    );
    // V2.1 (F-01): the tracker "completed" only if the program reached a
    // terminal state (or its telemetry was fully ingested). Interrupted runs
    // (timeout, cancellation, limits) previously reported
    // status=completed/output_complete=false/partial_results_available=false,
    // which hid both the interruption and the partially collected events.
    let tracker_terminal = matches!(
        phase.status,
        ExecutionStatus::Completed | ExecutionStatus::Crashed
    );
    let (tracker_status, tracker_reason) = if !reader.parse_errors.is_empty() {
        (StageStatus::Failed, reader.parse_errors.first().cloned())
    } else if crate::supervisor::cancelled() {
        (
            StageStatus::Cancelled,
            Some("session cancelled before allocation telemetry was fully ingested".into()),
        )
    } else if matches!(phase.status, ExecutionStatus::Timeout) {
        (
            StageStatus::TimedOut,
            Some(
                "program timed out; allocation events up to the last observed stop were ingested"
                    .into(),
            ),
        )
    } else if tracker_terminal {
        (StageStatus::Completed, None)
    } else if matches!(
        phase.status,
        ExecutionStatus::StepLimitReached
            | ExecutionStatus::OutputLimitReached
            | ExecutionStatus::ResourceLimitReached
    ) {
        (
            StageStatus::Failed,
            Some(format!(
                "program terminated by a resource limit ({:?}); allocation events up to the last observed stop were ingested",
                phase.status
            )),
        )
    } else {
        (
            StageStatus::Failed,
            Some(format!(
                "program did not reach a terminal state ({:?}); allocation coverage may be incomplete",
                phase.status
            )),
        )
    };
    trace.stage_results.allocation_tracker = crate::supervisor::stage(
        started,
        debugger_started,
        tracker_status,
        None,
        tracker_reason,
        tracker_terminal && reader.parse_errors.is_empty(),
        !timeline.events().is_empty(),
    );
    if phase.inferior_pid.is_none() {
        trace.stage_results.allocation_tracker = skipped_stage("inferior was not started");
    } else if !event_file.is_file() {
        trace.stage_results.allocation_tracker = crate::supervisor::stage(
            started,
            debugger_started,
            StageStatus::Failed,
            None,
            Some("allocation telemetry file unavailable".into()),
            false,
            false,
        );
    }

    // ---- caller location resolution (batched addr2line) ---------------------
    let source_resolution_started = Instant::now();
    if addr2line_version.is_some()
        && Instant::now() < session_deadline
        && !crate::supervisor::cancelled()
    {
        let mut callers: Vec<u64> = timeline
            .blocks()
            .iter()
            .map(|b| b.alloc_caller)
            .filter(|c| *c != 0)
            .collect();
        callers.extend(timeline.event_callers().iter().copied().filter(|c| *c != 0));
        if !callers.is_empty() {
            let normalized: Vec<u64> = callers
                .iter()
                .filter_map(|pc| pc.checked_sub(phase.executable_bias?))
                .collect();
            match tokio::time::timeout_at(
                session_deadline.into(),
                resolve_caller_locations(&debug_build.binary_path, &normalized),
            )
            .await
            {
                Ok(Ok(resolved)) => {
                    if resolved.is_empty() {
                        notes.push("addr2line resolved no allocation call sites".into());
                    }
                    let relocated = resolved
                        .into_iter()
                        .filter_map(|(pc, info)| {
                            Some((pc.checked_add(phase.executable_bias?)?, info))
                        })
                        .collect();
                    apply_caller_locations(&mut timeline, &relocated);
                }
                Ok(Err(e)) => notes.push(format!("addr2line failed: {e}")),
                Err(_) => {
                    notes.push("source resolution deadline exhausted; raw PCs retained".into())
                }
            }
        }
    }

    let source_resolution_time_us = source_resolution_started.elapsed().as_micros() as u64;
    let status = phase.status.clone();
    let mut crash: Option<CrashReport> = phase.crash.clone();
    let mut sanitizer_report: Option<SanitizerReport> = None;

    // ---- sanitizer / Valgrind stage ordering (V2.1, fixes F-02) --------------
    // The sanitizer trigger is configurable. When the trigger is already
    // satisfied without Valgrind (a crash, or mode `always`), the sanitizer
    // runs FIRST — the V2 schedule — so it cannot be crowded out of the
    // session deadline by the heavier Valgrind stage. Only when the sanitizer
    // decision depends on Valgrind findings (`on_crash_or_valgrind_error` with
    // a clean exit) does Valgrind run first. The two stages instrument
    // separate processes and never run together.
    let sanitizer_satisfied_without_valgrind = match config.sanitizer_trigger {
        crate::config::SanitizerTrigger::Never => false,
        crate::config::SanitizerTrigger::OnCrash
        | crate::config::SanitizerTrigger::OnCrashOrValgrindError => {
            status == ExecutionStatus::Crashed
        }
        crate::config::SanitizerTrigger::Always => true,
    };
    let sanitizer_first = sanitizer_satisfied_without_valgrind && asan_build.success;
    let sanitizer_started = Instant::now();
    let mut sanitizer_ran = false;
    if sanitizer_first {
        sanitizer_report = run_sanitizer_pass(
            &asan_build.binary_path,
            &config.program_args,
            config
                .inferior_working_directory
                .as_deref()
                .or(config.build.working_directory.as_deref()),
            &limits,
            &mut crash,
            started,
            sanitizer_started,
            &mut trace,
            &mut notes,
        )
        .await;
        sanitizer_ran = sanitizer_report.is_some();
    }

    // ---- Valgrind (independent analysis) ------------------------------------
    let mut valgrind_report: Option<ValgrindReport> = None;
    let valgrind_started = Instant::now();
    if valgrind_version.is_some()
        && Instant::now() < session_deadline
        && !crate::supervisor::cancelled()
    {
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
            config.valgrind_path.as_deref(),
        )
        .await
        {
            Ok(run) => {
                if let Some(mut report) = run.report {
                    let output_complete =
                        report.output_complete && !run.timed_out && !run.cancelled;
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
                    trace.stage_results.valgrind = crate::supervisor::stage(
                        started,
                        valgrind_started,
                        if run.cancelled {
                            StageStatus::Cancelled
                        } else if run.timed_out {
                            StageStatus::TimedOut
                        } else if !output_complete {
                            StageStatus::Failed
                        } else {
                            StageStatus::Completed
                        },
                        run.exit_code,
                        valgrind_report.as_ref().and_then(|r| r.parse_error.clone()),
                        output_complete,
                        true,
                    );
                    if run.timed_out {
                        trace.stage_results.valgrind.status = Some(StageStatus::TimedOut);
                        trace.stage_results.valgrind.output_complete = Some(false);
                        trace.stage_results.valgrind.reason =
                            Some("Valgrind stage timed out".into());
                    }
                } else {
                    notes.push("valgrind XML could not be parsed (execution terminated?)".into());
                    trace.stage_results.valgrind = crate::supervisor::stage(
                        started,
                        valgrind_started,
                        if run.cancelled {
                            StageStatus::Cancelled
                        } else if run.timed_out {
                            StageStatus::TimedOut
                        } else {
                            StageStatus::Failed
                        },
                        run.exit_code,
                        run.failure_reason
                            .or_else(|| Some("Valgrind XML was missing or invalid".into())),
                        false,
                        false,
                    );
                }
            }
            Err(e) => {
                notes.push(format!("valgrind failed: {e}"));
                trace.stage_results.valgrind = crate::supervisor::stage(
                    started,
                    valgrind_started,
                    if crate::supervisor::remaining().is_zero() {
                        StageStatus::TimedOut
                    } else {
                        StageStatus::Failed
                    },
                    None,
                    Some(e.to_string()),
                    false,
                    false,
                );
            }
        }
    } else if valgrind_version.is_some() {
        trace.stage_results.valgrind = skipped_stage(if crate::supervisor::cancelled() {
            "session interrupted"
        } else {
            "overall session deadline exhausted"
        });
    } else {
        trace.stage_results.valgrind = skipped_stage("Valgrind is unavailable");
    }

    let valgrind_had_errors = valgrind_report
        .as_ref()
        .is_some_and(|r| !r.errors.is_empty() || !r.detailed_errors.is_empty());

    // ---- sanitizer diagnostic pass, when it depends on Valgrind findings ----
    let sanitizer_requested = sanitizer_satisfied_without_valgrind
        || match config.sanitizer_trigger {
            crate::config::SanitizerTrigger::OnCrashOrValgrindError => valgrind_had_errors,
            crate::config::SanitizerTrigger::Always => true,
            _ => false,
        };
    let sanitizer_skip_reason = match config.sanitizer_trigger {
        crate::config::SanitizerTrigger::Never => {
            Some("sanitizer trigger mode is 'never'".to_owned())
        }
        crate::config::SanitizerTrigger::OnCrash if status != ExecutionStatus::Crashed => {
            Some("trigger mode 'on_crash' and the program did not crash".to_owned())
        }
        crate::config::SanitizerTrigger::OnCrashOrValgrindError
            if status != ExecutionStatus::Crashed && !valgrind_had_errors =>
        {
            Some(
                "trigger mode 'on_crash_or_valgrind_error': no crash and Valgrind reported no memory errors"
                    .to_owned(),
            )
        }
        _ => None,
    };
    if sanitizer_first {
        // Already attempted above (or skipped by deadline there).
    } else if sanitizer_requested
        && asan_build.success
        && Instant::now() < session_deadline
        && !crate::supervisor::cancelled()
    {
        sanitizer_report = run_sanitizer_pass(
            &asan_build.binary_path,
            &config.program_args,
            config
                .inferior_working_directory
                .as_deref()
                .or(config.build.working_directory.as_deref()),
            &limits,
            &mut crash,
            started,
            sanitizer_started,
            &mut trace,
            &mut notes,
        )
        .await;
        sanitizer_ran = sanitizer_report.is_some();
    } else if sanitizer_requested && asan_build.success {
        trace.stage_results.sanitizer = skipped_stage(if crate::supervisor::cancelled() {
            "session interrupted"
        } else {
            "overall session deadline exhausted"
        });
    } else {
        let reason = sanitizer_skip_reason
            .unwrap_or_else(|| "no crash or sanitized build unavailable".to_owned());
        trace.stage_results.sanitizer = skipped_stage(&reason);
    }
    let _ = sanitizer_ran;

    // ---- assemble ------------------------------------------------------------
    let parent_pid = infer_parent_pid(&timeline, phase.inferior_pid);
    trace.snapshots = backfill_snapshots(&timeline, phase.snapshots, parent_pid);
    trace.heap_events = final_events(&timeline);
    trace.heap_state = build_heap_state(&timeline, &trace.snapshots, parent_pid);
    trace.recording = phase.recording;
    trace.recording.profiling.source_resolution_time_us = source_resolution_time_us;
    trace.recording.profiling.engine_peak_rss_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|line| {
                line.strip_prefix("VmHWM:")
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse().ok())
            })
        });
    trace.recording.allocation_lifetimes = timeline
        .blocks()
        .iter()
        .map(|b| crate::models::AllocationLifetime {
            allocation_id: timeline.global_id(b.pid, b.allocation_id),
            pid: b.pid,
            allocation_sequence: b.alloc_seq,
            free_sequence: (b.free_seq != 0).then_some(b.free_seq),
            raw_caller_address: format!("0x{:x}", b.alloc_caller),
            module: Some(debug_build.binary_path.display().to_string()),
            resolution: if b.caller_file.is_some() {
                "resolved"
            } else {
                "unresolved"
            }
            .into(),
        })
        .collect();
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
    use crate::models::ProgramOutcome;
    trace.execution = crate::models::ExecutionOutcome {
        outcome: match trace.execution_status {
            ExecutionStatus::Completed => ProgramOutcome::Completed,
            ExecutionStatus::Crashed => ProgramOutcome::Crashed,
            ExecutionStatus::Timeout => ProgramOutcome::TimedOut,
            ExecutionStatus::StepLimitReached if phase.inferior_pid.is_none() => {
                ProgramOutcome::NotStarted
            }
            ExecutionStatus::StepLimitReached => ProgramOutcome::StepLimitReached,
            // V2.1 (F-04): output-limit termination has its own status instead
            // of being mislabeled as a step limit.
            ExecutionStatus::OutputLimitReached => ProgramOutcome::OutputLimitReached,
            ExecutionStatus::ResourceLimitReached => ProgramOutcome::Interrupted,
            ExecutionStatus::Interrupted => ProgramOutcome::Interrupted,
            ExecutionStatus::CompileFailed => ProgramOutcome::CompileFailed,
            ExecutionStatus::DebuggerFailure => ProgramOutcome::DebuggerFailed,
            ExecutionStatus::ExecutionFailure => ProgramOutcome::InternalFailure,
        },
        exit_code: trace.resource_usage.inferior_exit_code,
        signal: phase
            .termination_signal
            .or_else(|| trace.crash_report.as_ref().map(|r| r.signal_name.clone())),
        instruction_address: trace
            .crash_report
            .as_ref()
            .and_then(|c| c.instruction_address.clone()),
        reason: trace
            .resource_usage
            .limits_triggered
            .first()
            .cloned()
            .or_else(|| trace.stage_results.debugger.reason.clone()),
    };
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
    pub termination_signal: Option<String>,
    pub executable_bias: Option<u64>,
    pub recording: crate::models::RecordingData,
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

impl GdbPhaseResult {
    /// Record a triggered limit exactly once (V2.1, fixes F-03: the audit
    /// observed duplicate entries such as two `max_output_bytes` entries when
    /// the in-loop and post-loop paths both recorded the same limit).
    pub fn push_limit_once(&mut self, name: &str) {
        if !self.limits_triggered.iter().any(|n| n == name) {
            self.limits_triggered.push(name.to_owned());
        }
    }
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
    let mut runtime_classifier = classifier.clone();
    runtime_classifier.instrumentation_file = binary.parent().map(|dir| {
        let source = dir.join("alloc_tracker.c");
        std::fs::canonicalize(&source)
            .unwrap_or(source)
            .display()
            .to_string()
    });
    let classifier = &runtime_classifier;
    let timeout_secs = limits.timeout_seconds.max(1);
    let wall_deadline = crate::supervisor::stage_deadline(limits.debugger_timeout_seconds);
    let mut recorder = crate::recorder::Recorder::new(&config.tracing, limits)?;
    let dual = config.tracing.mode == crate::config::TraceMode::Dual;

    let mut result = GdbPhaseResult {
        termination_signal: None,
        recording: Default::default(),
        executable_bias: None,
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
    if crate::supervisor::remaining().is_zero() {
        result.status = ExecutionStatus::Timeout;
        result.push_limit_once("overall_session_deadline");
        recorder.truncate("overall_session_deadline");
        result.recording = recorder.finish();
        return Ok(result);
    }

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
        gdb_path: config.gdb_path.clone(),
    };

    let mut session = match GdbSession::launch(&launch_opts).await {
        Ok(s) => s,
        Err(e) => {
            notes.push(format!("could not launch gdb: {e}"));
            result.status = if Instant::now() >= wall_deadline {
                ExecutionStatus::Timeout
            } else {
                ExecutionStatus::DebuggerFailure
            };
            if let EngineError::LimitReached(name) = &e {
                result.status = if *name == "max_output_bytes" {
                    ExecutionStatus::OutputLimitReached
                } else {
                    ExecutionStatus::StepLimitReached
                };
                result.push_limit_once(name);
                if *name == "max_gdb_commands" {
                    result.commands_issued = limits.max_gdb_commands;
                }
            }
            if result.status == ExecutionStatus::Timeout {
                result.push_limit_once("session_or_debugger_deadline");
            }
            recorder.truncate(&e.to_string());
            if crate::supervisor::cancelled() {
                result.status = ExecutionStatus::Interrupted;
            }
            result.recording = recorder.finish();
            return Ok(result);
        }
    };
    result.commands_issued = session
        .commands_issued
        .load(std::sync::atomic::Ordering::SeqCst);

    // Breakpoint at main; without it we can still run to completion (no
    // stepping, no snapshots before exit).
    let have_breakpoint = match session.insert_breakpoint("main").await {
        Ok(_) => true,
        Err(e) => {
            notes.push(format!(
                "no breakpoint at main ({e}); running to completion"
            ));
            false
        }
    };
    // Entry-address breakpoints catch callbacks from excluded library intervals.
    let entry_errors = session.install_user_entries(classifier).await;
    if !entry_errors.is_empty() {
        recorder.truncate("callback_entry_breakpoints_incomplete");
        notes.extend(entry_errors);
    }

    // Run the inferior; the first stop is the breakpoint (or exit).
    let run_timeout = remaining(wall_deadline).min(Duration::from_secs(
        timeout_secs.saturating_mul(2).saturating_add(10),
    ));
    let mut stop = match session.run(run_timeout).await {
        Ok(stop) => stop,
        Err(e) => {
            debug_dump_console(&session, "exec-run failed");
            notes.push(format!("gdb run failed: {e}"));
            result.inferior_pid = session.observed_inferior_pid();
            if remaining(wall_deadline).is_zero() {
                result.status = ExecutionStatus::Timeout;
                result.push_limit_once("timeout_seconds");
            } else {
                result.status = ExecutionStatus::DebuggerFailure;
            }
            if let EngineError::LimitReached(name) = &e {
                result.status = if *name == "max_output_bytes" {
                    ExecutionStatus::OutputLimitReached
                } else {
                    ExecutionStatus::StepLimitReached
                };
                result.push_limit_once(name);
            }
            terminate(&mut session, cgroup.as_ref()).await;
            if crate::supervisor::cancelled() {
                result.status = ExecutionStatus::Interrupted;
            }
            finish_phase(&mut result, &session, timeline, reader, notes);
            recorder.truncate(&e.to_string());
            result.recording = recorder.finish();
            return Ok(result);
        }
    };
    result.inferior_pid = session.inferior_pid;
    if let Some(pid) = session.inferior_pid {
        if let Err(e) = crate::resource_limits::limit_stopped_inferior(pid, limits) {
            notes.push(format!("inferior prlimit enforcement failed: {e}"));
        }
    }
    result.executable_bias = session
        .inferior_pid
        .and_then(|pid| crate::memory::executable_bias(binary, pid));
    if cgroup.is_none() {
        notes.push("cgroup unavailable: per-tree memory/process limits not enforced; inferior rlimits remain active".into());
    }
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
    let debugger_begin = Instant::now();
    let mut stop_id = 0u64;
    let mut last_source = None;
    let mut global_names = config.tracing.globals.clone();
    if config.tracing.discover_globals {
        global_names.extend(
            session
                .discover_globals(classifier, limits.max_globals)
                .await,
        );
    }
    let globals_capped = global_names.len() >= limits.max_globals && limits.max_globals != 0;
    if globals_capped {
        notes.push("global discovery/inspection reached max_globals".into());
    }
    global_names.truncate(limits.max_globals);
    let startup_record = recorder.gap(
        None,
        stop.frame.as_ref().and_then(|f| f.address.clone()),
        "unknown",
        "startup_before_main",
    );
    if !stop
        .frame
        .as_ref()
        .is_some_and(|f| classifier.is_user_file(f))
    {
        recorder.truncate("initial_source_mapping_unavailable");
    }
    // V2.1: per-instruction CPU register capture removed (V2 audit: ~90% of
    // debugger command time; no GUI consumer). Instruction addresses come from
    // GDB stop frames; register payloads are deprecated schema fields.
    if let Err(error) = startup_record {
        result.termination_signal = stop.signal_name.clone();
        result.status = observed_terminal(&stop).unwrap_or(ExecutionStatus::StepLimitReached);
        if result.status == ExecutionStatus::Crashed {
            result.crash = Some(crash_from(&stop, &Default::default()));
        }
        if stop.is_exit() {
            result.exit_code = exit_code_from(&mut session, &stop);
        }
        result.push_limit_once(&error.to_string());
        recorder.truncate(&error.to_string());
        session.kill().await;
        finish_phase(&mut result, &session, timeline, reader, notes);
        result.recording = recorder.finish();
        return Ok(result);
    }
    let mut finish_chain: usize = 0;
    let mut snapshot_cap_noted = false;
    let mut memory_cap_noted = false;
    let mut read_cap_noted = false;

    loop {
        stop_id += 1;
        let terminal_observed = observed_terminal(&stop).is_some() || stop.is_exit();
        if terminal_observed {
            result.termination_signal = stop.signal_name.clone();
        }
        if !terminal_observed
            && session
                .commands_issued
                .load(std::sync::atomic::Ordering::SeqCst)
                >= limits.max_gdb_commands
        {
            result.status = ExecutionStatus::StepLimitReached;
            result.push_limit_once("max_gdb_commands");
            break;
        }
        // Ingest allocation events emitted since the last stop. The inferior
        // is stopped here, so the event file is complete up to now.
        for ev in reader.read_new() {
            timeline.ingest(&ev);
        }
        if let Err(e) = recorder.allocations(timeline.events(), stop_id) {
            result.push_limit_once(&e.to_string());
            recorder.truncate(&e.to_string());
            if !terminal_observed {
                result.status = ExecutionStatus::StepLimitReached;
                break;
            }
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
        if stop.reason.as_deref() == Some("exited-signalled")
            || stop.signal_name.as_deref() == Some("SIGKILL")
        {
            result.status = if stop
                .signal_name
                .as_deref()
                .is_some_and(|s| RESOURCE_SIGNALS.contains(&s))
            {
                ExecutionStatus::ResourceLimitReached
            } else {
                ExecutionStatus::Interrupted
            };
            notes.push(format!(
                "inferior terminated by {:?}; no cause inferred",
                stop.signal_name
            ));
            break;
        }
        if stop.is_signal() {
            match classify_signal(stop.signal_name.as_deref()) {
                SignalClass::Fatal => {
                    // Capture the crash state (registers are still valid).
                    let state = capture_or_empty(&mut session, &capture_opts).await;
                    let pid = inferior_pid_of(&session);
                    let previous_count = result.snapshots.len();
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
                    if let Some(snapshot) = result.snapshots.get_mut(previous_count) {
                        for block in &mut snapshot.heap_blocks {
                            if block.raw_bytes_hex.is_some() {
                                block.observed_at_stop = Some(stop_id);
                            }
                        }
                        snapshot.observation = crate::models::SnapshotObservation {
                            stop_id,
                            instruction_position: recorder.data.instruction_count,
                            instruction_address: stop
                                .frame
                                .as_ref()
                                .and_then(|f| f.address.clone()),
                            thread_id: stop.thread_id.clone(),
                            pid,
                            origin: stop
                                .frame
                                .as_ref()
                                .map(|f| classifier.origin(f))
                                .unwrap_or("unknown")
                                .into(),
                            complete: !snapshot.memory_truncated && state.errors.is_empty(),
                            errors: state.errors.clone(),
                            heap_event_count: timeline.events().len(),
                            ..Default::default()
                        };
                        crate::memory::enrich(&mut snapshot.variables, &[], timeline, pid);
                        if let Err(e) = recorder.checkpoint(snapshot) {
                            recorder.truncate(&e.to_string());
                        }
                    }
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
                        result.push_limit_once("timeout_seconds");
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

        if Instant::now() >= wall_deadline {
            result.status = ExecutionStatus::Timeout;
            result.push_limit_once("session_or_debugger_deadline");
            break;
        }
        if crate::supervisor::cancelled() {
            result.status = ExecutionStatus::Interrupted;
            break;
        }
        let frame = stop.frame.clone().unwrap_or_default();
        let is_user_stop = classifier.is_user_file(&frame);
        let source_key = (frame.file.clone(), frame.line, frame.func.clone());
        let checkpoint = is_user_stop
            && (!dual
                || last_source.as_ref() != Some(&source_key)
                || stop.reason.as_deref() == Some("breakpoint-hit"));
        if checkpoint
            && result.snapshots.len()
                >= limits.max_source_checkpoints.min(limits.max_snapshot_count)
        {
            result.status = ExecutionStatus::StepLimitReached;
            result.push_limit_once("max_source_checkpoints");
            break;
        }
        // Rich dynamic state is read only at source transitions, while all-stop.
        let capture_start = Instant::now();
        let state = if checkpoint || !is_user_stop {
            capture_or_empty(&mut session, &capture_opts).await
        } else {
            crate::gdb_mi::CapturedState {
                current: Some(frame.clone()),
                is_user_code: is_user_stop,
                ..Default::default()
            }
        };
        recorder.data.profiling.capture_time_us += capture_start.elapsed().as_micros() as u64;
        let pid = inferior_pid_of(&session);
        if checkpoint {
            for var in &state.variables {
                if var.scope.as_deref() == Some("static") && global_names.len() < limits.max_globals
                {
                    let name = format!("{}::{}", frame.func, var.name);
                    session.symbol_scopes.insert(name.clone(), "static".into());
                    if !global_names.contains(&name) {
                        global_names.push(name);
                    }
                }
            }
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
            if let Some(snapshot) = result.snapshots.last_mut() {
                for block in &mut snapshot.heap_blocks {
                    if block.raw_bytes_hex.is_some() {
                        block.observed_at_stop = Some(stop_id);
                    }
                }
                snapshot.observation = crate::models::SnapshotObservation {
                    stop_id,
                    instruction_position: recorder.data.instruction_count,
                    instruction_address: frame.address.clone(),
                    thread_id: stop.thread_id.clone(),
                    pid,
                    origin: classifier.origin(&frame).into(),
                    globals: Vec::new(),
                    complete: !snapshot.memory_truncated && state.errors.is_empty(),
                    errors: state.errors.clone(),
                    heap_event_count: timeline.events().len(),
                };
                let mut global_budget = limits
                    .max_snapshot_memory_bytes
                    .saturating_sub(state.memory_used);
                global_budget = global_budget.saturating_sub(
                    snapshot
                        .heap_blocks
                        .iter()
                        .filter_map(|b| b.raw_bytes_hex.as_ref())
                        .map(|s| s.len() / 2)
                        .sum(),
                );
                for name in &global_names {
                    let mut global_limits = limits.clone();
                    global_limits.max_memory_read_bytes =
                        global_limits.max_memory_read_bytes.min(global_budget);
                    let global =
                        crate::recorder::capture_global(&mut session, name, &global_limits).await;
                    global_budget = global_budget
                        .saturating_sub(global.raw_bytes_hex.as_ref().map_or(0, |s| s.len() / 2));
                    snapshot.observation.globals.push(global);
                }
                crate::memory::enrich(
                    &mut snapshot.variables,
                    &snapshot.observation.globals,
                    timeline,
                    pid,
                );
                let symbols = snapshot.observation.globals.clone();
                crate::memory::enrich(&mut snapshot.observation.globals, &symbols, timeline, pid);
                snapshot.observation.complete &= snapshot
                    .variables
                    .iter()
                    .chain(&snapshot.observation.globals)
                    .all(|v| {
                        v.read_status == crate::models::DataValidity::Valid
                            && v.read_error.is_none()
                            && v.bytes_status != crate::models::DataValidity::Truncated
                    });
                if globals_capped {
                    snapshot.observation.complete = false;
                    snapshot
                        .observation
                        .errors
                        .push("max_globals: additional global symbols may be omitted".into());
                }
                if let Err(e) = recorder.checkpoint(snapshot) {
                    result.status = ExecutionStatus::StepLimitReached;
                    result.push_limit_once(&e.to_string());
                    break;
                }
            }
            last_source = Some(source_key);
        }

        // --- resource checks -------------------------------------------------------
        if Instant::now() >= wall_deadline {
            result.status = ExecutionStatus::Timeout;
            result.push_limit_once("timeout_seconds");
            break;
        }
        if session
            .commands_issued
            .load(std::sync::atomic::Ordering::SeqCst)
            >= limits.max_gdb_commands
        {
            result.status = ExecutionStatus::StepLimitReached;
            result.push_limit_once("max_gdb_commands");
            break;
        }
        if (!dual && steps >= limits.max_steps)
            || (dual && result.snapshots.len() >= limits.max_steps)
        {
            result.push_limit_once("max_steps");
            result.status = ExecutionStatus::StepLimitReached;
            break;
        }

        // --- decide the next stepping command ---------------------------------------
        let dbg = std::env::var_os("C_TRACE_DEBUG").is_some();
        let is_user = is_user_stop;
        if dbg {
            eprintln!(
                "[trace] step={} finish_chain={} is_user={} stop={:?} frame={:?}",
                steps,
                finish_chain,
                is_user,
                stop.reason,
                stop.frame
                    .as_ref()
                    .map(|f| (f.func.clone(), f.file.clone(), f.line))
            );
        }
        if dual && recorder.data.instruction_count >= limits.max_instruction_events as u64 {
            result.status = ExecutionStatus::StepLimitReached;
            result.push_limit_once("max_instruction_events");
            break;
        }
        if session.program_output().2 {
            result.status = ExecutionStatus::OutputLimitReached;
            result.push_limit_once("max_output_bytes");
            break;
        }
        let instruction_id = if dual && is_user {
            match recorder.disassemble(&mut session, &stop).await {
                Ok(id) => id,
                Err(e) => {
                    notes.push(format!("disassembly unavailable: {e}"));
                    None
                }
            }
        } else {
            None
        };
        let step_start = Instant::now();
        let next_stop = if is_user || !have_breakpoint {
            // User code: single-step (this enters calls too, which is how we
            // visit helper functions in the user's project).
            steps += 1;
            finish_chain = 0;
            let stepped = if dual {
                session.exec_instruction(step_timeout(wall_deadline)).await
            } else {
                session.exec_step(step_timeout(wall_deadline)).await
            };
            match stepped {
                Ok(s) => s,
                Err(EngineError::GdbCommandTimeout { .. }) => {
                    // The inferior hung inside a step (blocking syscall, long
                    // loop): preserve partials and report the timeout.
                    debug_dump_console(&session, "exec-step timed out");
                    result.status = if Instant::now() >= wall_deadline {
                        ExecutionStatus::Timeout
                    } else {
                        ExecutionStatus::DebuggerFailure
                    };
                    result.push_limit_once(if Instant::now() >= wall_deadline {
                        "session_or_debugger_deadline"
                    } else {
                        "debugger_stop_wait_timeout"
                    });
                    if dual && is_user {
                        let _ = recorder.event(crate::models::InstructionEvent {
                            stop_id_before: stop_id,
                            stop_id_after: None,
                            event_id: recorder.data.instruction_count,
                            instruction_id,
                            address: frame.address.clone().unwrap_or_default(),
                            next_address: None,
                            thread_id: stop.thread_id.clone(),
                            pid,
                            source: crate::models::SourceContext {
                                file: frame.file.clone(),
                                line: frame.line,
                                function: Some(frame.func.clone()),
                            },
                            checkpoint_id: result.snapshots.last().map(|s| s.step_index),
                            outcome: "unknown".into(),
                            stopped_reason: Some("debugger_stop_wait_timeout".into()),
                            signal: None,
                            registers: None,
                            memory: Vec::new(),
                        });
                    }
                    break;
                }
                Err(e) => {
                    // Do not silently run past user instructions after a failed step.
                    debug_dump_console(&session, "exec-step failed");
                    notes.push(format!("step failed: {e}"));
                    result.status = if Instant::now() >= wall_deadline {
                        ExecutionStatus::Timeout
                    } else {
                        ExecutionStatus::DebuggerFailure
                    };
                    break;
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
                        result.status = if Instant::now() >= wall_deadline {
                            ExecutionStatus::Timeout
                        } else {
                            ExecutionStatus::DebuggerFailure
                        };
                        result.push_limit_once(if Instant::now() >= wall_deadline {
                            "session_or_debugger_deadline"
                        } else {
                            "debugger_stop_wait_timeout"
                        });
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
        recorder.data.profiling.step_time_us += step_start.elapsed().as_micros() as u64;
        if dual && is_user {
            let event = crate::models::InstructionEvent {
                stop_id_before: stop_id,
                stop_id_after: Some(stop_id + 1),
                event_id: recorder.data.instruction_count,
                instruction_id,
                address: frame.address.clone().unwrap_or_default(),
                next_address: next_stop.frame.as_ref().and_then(|f| f.address.clone()),
                thread_id: stop.thread_id.clone(),
                pid,
                source: crate::models::SourceContext {
                    file: frame.file.clone(),
                    line: frame.line,
                    function: Some(frame.func.clone()),
                },
                checkpoint_id: result.snapshots.last().map(|s| s.step_index),
                outcome: crate::recorder::event_outcome(&stop, &next_stop).into(),
                stopped_reason: next_stop.reason.clone(),
                signal: next_stop.signal_name.clone(),
                registers: None,
                memory: if next_stop.is_exit() {
                    Vec::new()
                } else {
                    recorder.selected_memory(&mut session).await
                },
            };
            if let Err(e) = recorder.event(event) {
                result.termination_signal = next_stop.signal_name.clone();
                result.status =
                    observed_terminal(&next_stop).unwrap_or(ExecutionStatus::StepLimitReached);
                if next_stop.is_exit() {
                    result.exit_code = exit_code_from(&mut session, &next_stop);
                }
                if result.status == ExecutionStatus::Crashed {
                    result.crash = Some(crash_from(&next_stop, &Default::default()));
                }
                recorder.truncate(&e.to_string());
                result.push_limit_once(&e.to_string());
                break;
            }
        } else {
            if let Err(e) = recorder.gap(
                frame.address.clone(),
                next_stop.frame.as_ref().and_then(|f| f.address.clone()),
                classifier.origin(&frame),
                if dual {
                    "excluded_code"
                } else {
                    "source_only_stepping"
                },
            ) {
                result.status = ExecutionStatus::StepLimitReached;
                result.push_limit_once(&e.to_string());
                break;
            }
        }
        // The finish chain resets once execution is back in user code.
        if let Some(frame) = &next_stop.frame {
            if classifier.is_user_file(frame) {
                finish_chain = 0;
            }
        }
        stop = next_stop;
    }

    // Preserve observed native termination; only normalize nonterminal failures.
    if !matches!(
        result.status,
        ExecutionStatus::Completed
            | ExecutionStatus::Crashed
            | ExecutionStatus::Interrupted
            | ExecutionStatus::ResourceLimitReached
    ) {
        if crate::supervisor::cancelled() {
            result.status = ExecutionStatus::Interrupted;
        } else if session.program_output().2 {
            result.status = ExecutionStatus::OutputLimitReached;
            result.push_limit_once("max_output_bytes");
        } else if Instant::now() >= wall_deadline {
            result.status = ExecutionStatus::Timeout;
            result.push_limit_once(if crate::supervisor::remaining().is_zero() {
                "overall_session_deadline"
            } else {
                "debugger_stage_deadline"
            });
        }
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
        | ExecutionStatus::OutputLimitReached
        | ExecutionStatus::ResourceLimitReached
        | ExecutionStatus::ExecutionFailure
        | ExecutionStatus::DebuggerFailure => {
            if let Some(cg) = cgroup.as_ref() {
                cg.kill_all();
            }
            session.kill().await;
        }
        ExecutionStatus::CompileFailed | ExecutionStatus::Interrupted => {
            terminate(&mut session, cgroup.as_ref()).await;
        }
    }

    // Post-mortem events (destructors, exit handlers) may have landed after
    // the final stop.
    for ev in reader.read_new() {
        timeline.ingest(&ev);
    }
    if let Err(e) = recorder.allocations(timeline.events(), stop_id) {
        recorder.truncate(&e.to_string());
    }
    finish_phase(&mut result, &session, timeline, reader, notes);
    if session.multi_threaded() {
        recorder.truncate("multiple_threads_observed_instruction_coverage_is_selected_thread_only");
    }
    if !matches!(
        result.status,
        ExecutionStatus::Completed | ExecutionStatus::Crashed
    ) {
        let _ = recorder.gap(
            stop.frame.as_ref().and_then(|f| f.address.clone()),
            None,
            "unknown",
            "recording_terminated",
        );
        recorder.truncate("execution_did_not_reach_observed_termination");
        for reason in &result.limits_triggered {
            recorder.truncate(reason);
        }
    }
    recorder.data.profiling.debugger_time_us = debugger_begin.elapsed().as_micros() as u64;
    recorder.data.profiling.command_time_us = session.command_time_us;
    recorder.data.profiling.mi_parse_time_us = session.parse_time_us();
    recorder.data.profiling.memory_read_time_us = session.memory_read_time_us;
    recorder.data.internal_diagnostics = session.console_log().lines().map(str::to_owned).collect();
    result.recording = recorder.finish();
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

fn observed_terminal(stop: &StoppedInfo) -> Option<ExecutionStatus> {
    if stop.signal_name.as_deref() == Some("SIGKILL")
        || stop.reason.as_deref() == Some("exited-signalled")
    {
        Some(ExecutionStatus::Interrupted)
    } else if stop.is_signal() {
        match classify_signal(stop.signal_name.as_deref()) {
            SignalClass::Fatal => Some(ExecutionStatus::Crashed),
            SignalClass::Resource => Some(ExecutionStatus::ResourceLimitReached),
            SignalClass::Benign => None,
        }
    } else if stop.is_exit() {
        Some(ExecutionStatus::Completed)
    } else {
        None
    }
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

fn step_timeout(wall_deadline: Instant) -> Duration {
    remaining(wall_deadline).min(PER_STEP_TIMEOUT)
}

fn skipped_stage(reason: &str) -> StageResult {
    StageResult {
        status: Some(StageStatus::Skipped),
        reason: Some(reason.into()),
        output_complete: Some(false),
        ..Default::default()
    }
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
    let mut tail_start = log.len().saturating_sub(6000);
    while !log.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    eprintln!("[trace] console tail at {why}:\n{}", &log[tail_start..]);
}

/// Graceful shutdown (normal end): cgroup kill is a no-op safety net and GDB
/// exits cleanly, closing the pty.
async fn terminate(session: &mut GdbSession, cgroup: Option<&CgroupGuard>) {
    if let Some(cg) = cgroup {
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
    result.commands_issued = session
        .commands_issued
        .load(std::sync::atomic::Ordering::SeqCst);
    let (_, total, truncated) = session.program_output();
    result.program_output_bytes = total;
    result.program_output_truncated = truncated;
    if truncated {
        result.push_limit_once("max_output_bytes");
    }
    if timeline.blocks_capped
        && !result
            .limits_triggered
            .contains(&"max_heap_allocations".into())
    {
        result.push_limit_once("max_heap_allocations");
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
    match capture_state(session, opts).await {
        Ok(state) => state,
        Err(e) => crate::gdb_mi::CapturedState {
            errors: vec![e.to_string()],
            ..Default::default()
        },
    }
}
/// Build and append one snapshot (with live heap reads inside the budget).
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
            result.push_limit_once("max_snapshot_count");
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
    let mut memory_truncated = budget_left == 0
        || state
            .variables
            .iter()
            .any(|v| v.bytes_status == crate::models::DataValidity::Truncated);
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
                observed_at_stop: None,
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
                result.push_limit_once("max_memory_read_bytes");
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
        result.push_limit_once("max_snapshot_memory_bytes");
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
        observation: Default::default(),
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
        stdout_truncated: output_truncated || output.len() > STDOUT_HEAD_CAP,
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
        instruction_address: frame.as_ref().and_then(|f| f.address.clone()),
        signal_name: stop.signal_name.clone().unwrap_or_else(|| "unknown".into()),
        file: frame.as_ref().and_then(|f| f.file.clone()),
        line: frame.as_ref().and_then(|f| f.line),
        func: frame
            .as_ref()
            .map(|f| f.func.clone())
            .filter(|s| !s.is_empty()),
        asan_summary: None,
        backtrace: if state.frames.is_empty() {
            stop.frame.clone().into_iter().collect()
        } else {
            state.frames.clone()
        },
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
    if let Some(hex) = text.strip_prefix("0x") {
        return i32::from_str_radix(hex, 16).ok();
    }
    if text.starts_with('0') && text.len() > 1 {
        return i32::from_str_radix(text, 8).ok();
    }
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
    _parent_pid: Option<u32>,
) -> Vec<HeapBlockInfo> {
    let mut last_contents: HashMap<u64, (String, Option<u64>)> = HashMap::new();
    for snap in snapshots {
        for b in &snap.heap_blocks {
            if let Some(hex) = &b.raw_bytes_hex {
                last_contents.insert(b.allocation_id, (hex.clone(), b.observed_at_stop));
            }
        }
    }
    timeline
        .blocks()
        .iter()
        .map(|b| {
            let global = timeline.global_id(b.pid, b.allocation_id);
            HeapBlockInfo {
                observed_at_stop: last_contents.get(&global).and_then(|(_, stop)| *stop),
                allocation_id: global,
                address: format!("0x{:x}", b.addr),
                size_bytes: b.size,
                allocation_type: b.alloc_type.to_string(),
                is_freed: b.freed,
                allocation_file: b.caller_file.clone(),
                allocation_line: b.caller_line,
                raw_bytes_hex: last_contents.get(&global).map(|(bytes, _)| bytes.clone()),
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
        assert!(matches!(
            classify_signal(Some("SIGSEGV")),
            SignalClass::Fatal
        ));
        assert!(matches!(
            classify_signal(Some("SIGABRT")),
            SignalClass::Fatal
        ));
        assert!(matches!(
            classify_signal(Some("SIGXCPU")),
            SignalClass::Resource
        ));
        assert!(matches!(
            classify_signal(Some("SIGKILL")),
            SignalClass::Fatal
        ));
        assert!(matches!(
            classify_signal(Some("SIGUSR1")),
            SignalClass::Benign
        ));
        assert!(matches!(
            classify_signal(Some("SIGPIPE")),
            SignalClass::Benign
        ));
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
        assert_eq!(parse_mi_exit_code("052"), Some(42));
        assert_eq!(parse_mi_exit_code("0x2a"), Some(42));
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
        timeline
            .ingest(&parse_event_line("102772 1 malloc 1 7f0000000000 4 0 0 401000 ok").unwrap());
        let snap = ExecutionSnapshot {
            observation: Default::default(),
            step_index: 0,
            file: "x.c".into(),
            line: 1,
            func: "main".into(),
            call_stack: vec![],
            variables: vec![],
            heap_blocks: vec![HeapBlockInfo {
                observed_at_stop: None,
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
        timeline.ingest(&parse_event_line("102772 2 free 1 7f0000000000 4 0 0 401000 ok").unwrap());
        let state = build_heap_state(&timeline, &[snap], Some(102772));
        assert_eq!(state.len(), 1);
        assert!(state[0].is_freed);
        assert_eq!(state[0].raw_bytes_hex.as_deref(), Some("deadbeef"));
        assert_eq!(state[0].allocation_id, 1);
    }

    #[test]
    fn event_ids_widen_only_for_multi_pid() {
        let mut timeline = HeapTimeline::new(100);
        timeline
            .ingest(&parse_event_line("102772 1 malloc 1 7f0000000000 4 0 0 401000 ok").unwrap());
        let events = final_events(&timeline);
        assert_eq!(events[0].allocation_id, 1);

        timeline
            .ingest(&parse_event_line("102900 1 malloc 1 7f0000000010 4 0 0 401000 ok").unwrap());
        let events = final_events(&timeline);
        assert_eq!(events[0].allocation_id, (102772u64 << 32) | 1);
        assert_eq!(events[1].allocation_id, (102900u64 << 32) | 1);
    }
}

use c_trace_engine::config::TraceMode;
use c_trace_engine::models::{DataValidity, DebugSessionTrace, ExecutionStatus, StageStatus};
use c_trace_engine::{processor, trace::trace_program, TraceConfig};
static RECORDING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[tokio::test]
async fn sigint_preserves_partial_source_trace_and_reaps_debugger_tree() {
    let Some(_permit) = tools().await else {
        return;
    };
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let journal = std::env::temp_dir().join(format!(
        "cte-interrupt-{}-{stamp}.jsonl",
        std::process::id()
    ));
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_c-trace-engine"))
        .arg(config("spin_same_line.c").build.source_file)
        .args(["--trace-mode", "source", "--timeout", "30", "--journal"])
        .arg(&journal)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let mut ready = false;
    for _ in 0..500 {
        if let Ok(text) = std::fs::read_to_string(&journal) {
            ready = text
                .lines()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .any(|v| v["kind"] == "checkpoint" && v["data"]["line"] == 3);
        }
        if ready || child.try_wait().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let tree = c_trace_engine::supervisor::descendants(pid);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let start = Instant::now();
    unsafe {
        libc::kill(pid as i32, libc::SIGINT);
    }
    let output = tokio::time::timeout(Duration::from_secs(5), child.wait_with_output()).await;
    let survivors: Vec<_> = tree
        .into_iter()
        .filter(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists())
        .collect();
    for pid in &survivors {
        c_trace_engine::resource_limits::kill_process_group(*pid);
    }
    assert!(
        survivors.is_empty(),
        "debugger descendants survived interruption: {survivors:?}"
    );
    let output = output.unwrap().unwrap();
    assert!(
        ready,
        "inferior never reached fixture loop: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(start.elapsed() < Duration::from_secs(4));
    let trace: DebugSessionTrace = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(trace.execution_status, ExecutionStatus::Interrupted);
    assert_eq!(
        trace.stage_results.debugger.status,
        Some(StageStatus::Cancelled)
    );
    assert_eq!(
        trace.stage_results.valgrind.status,
        Some(StageStatus::Skipped)
    );
    assert!(!trace.snapshots.is_empty());
    assert!(trace.recording.storage.finalized);
    std::fs::remove_file(journal).unwrap();
}

#[tokio::test]
async fn lexical_shadowing_never_pairs_an_outer_value_with_the_inner_address() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("shadowed.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    let locals: Vec<_> = t
        .snapshots
        .iter()
        .flat_map(|s| &s.variables)
        .filter(|v| v.name == "value")
        .collect();
    assert!(locals.iter().any(|v| v.value == "20"));
    assert!(locals.iter().any(|v| v.value == "30"));
    for snapshot in &t.snapshots {
        let shadowed: Vec<_> = snapshot
            .variables
            .iter()
            .filter(|v| v.name == "value")
            .collect();
        if shadowed.len() > 1 {
            let addresses: Vec<_> = shadowed.iter().filter_map(|v| v.address.as_ref()).collect();
            if addresses.len() == shadowed.len() {
                assert_ne!(addresses[0], addresses[1]);
                assert_ne!(shadowed[0].lexical_scope_id, shadowed[1].lexical_scope_id);
            } else {
                assert!(shadowed.iter().all(|v| v
                    .read_error
                    .as_deref()
                    .is_some_and(|e| e.contains("MI name collision"))));
            }
        }
    }
}

#[tokio::test]
async fn native_memory_errors_and_nonzero_exit_preserve_execution_outcomes() {
    let Some(_permit) = tools().await else {
        return;
    };
    for (fixture, expected_kind) in [
        ("double_free.c", "InvalidFree"),
        ("invalid_write.c", "InvalidWrite"),
        ("uninit.c", "UninitCondition"),
        ("leaky.c", "Leak_DefinitelyLost"),
    ] {
        let mut c = config(fixture);
        c.tracing.mode = TraceMode::Source;
        let t = trace_program(c).await.unwrap();
        if t.session_meta.as_ref().unwrap().valgrind_version.is_some() {
            assert!(
                t.valgrind_report
                    .as_ref()
                    .unwrap()
                    .detailed_errors
                    .iter()
                    .any(|e| e.kind == expected_kind),
                "{fixture}: native report missing {expected_kind}"
            );
        }
        if fixture == "double_free.c" {
            assert_eq!(t.execution_status, ExecutionStatus::Crashed);
        }
    }
    let t = run("exit_status.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    assert_eq!(t.resource_usage.inferior_exit_code, Some(42));
    let t = run("allocator_edges.c").await;
    assert_eq!(
        t.execution_status,
        ExecutionStatus::Completed,
        "{:?}",
        t.analysis_notes
    );
    assert_eq!(t.resource_usage.inferior_exit_code, Some(0));
    assert!(t
        .heap_events
        .iter()
        .any(|e| e.size_bytes.is_none()
            && e.detail.as_deref().is_some_and(|d| d.contains("overflow"))));
}
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn config(name: &str) -> TraceConfig {
    let mut c = TraceConfig::default();
    c.build.source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/c")
        .join(name);
    c.limits.timeout_seconds = 30;
    c.limits.max_steps = 20_000;
    c
}

async fn tools() -> Option<tokio::sync::SemaphorePermit<'static>> {
    let permit = RECORDING.acquire().await.ok()?;
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("SKIPPED V2 test: required tool {tool} unavailable");
            return None;
        }
    }
    Some(permit)
}

async fn run(name: &str) -> DebugSessionTrace {
    trace_program(config(name)).await.unwrap()
}

#[tokio::test]
async fn fault_is_an_attempt_at_the_actual_pc_and_keeps_native_diagnostics() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("null_write.c").await;
    processor::validate(&t).unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Crashed);
    let last = t.recording.instruction_events.last().unwrap();
    assert_eq!(last.outcome, "faulted");
    assert_eq!(last.signal.as_deref(), Some("SIGSEGV"));
    assert_eq!(last.next_address.as_ref(), Some(&last.address));
    assert_eq!(
        t.execution.instruction_address.as_ref(),
        Some(&last.address)
    );
    assert!(last
        .source
        .file
        .as_deref()
        .unwrap()
        .ends_with("null_write.c"));
    assert!(t
        .snapshots
        .iter()
        .flat_map(|s| &s.variables)
        .any(|v| v.name == "ptr"
            && v.points_to.as_deref() == Some("0x0")
            && v.target.as_ref().is_some_and(|p| p.kind == "null")));
    assert_eq!(
        t.stage_results.debugger.status,
        Some(StageStatus::Completed)
    );
    if t.session_meta.as_ref().unwrap().valgrind_version.is_some() {
        assert!(t
            .valgrind_report
            .as_ref()
            .unwrap()
            .detailed_errors
            .iter()
            .any(|e| e.kind == "InvalidWrite"));
    }
    assert!(t.stage_results.sanitizer.status.is_some());
    let roundtrip: DebugSessionTrace =
        serde_json::from_slice(&serde_json::to_vec(&t).unwrap()).unwrap();
    processor::validate(&roundtrip).unwrap();
}

#[tokio::test]
async fn deadline_includes_queries_and_does_not_start_valgrind_after_expiry() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("spin_same_line.c");
    c.limits.timeout_seconds = 3;
    c.limits.max_source_checkpoints = 100_000;
    let start = Instant::now();
    let t = trace_program(c).await.unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(6),
        "cleanup grace exceeded"
    );
    assert_eq!(t.execution_status, ExecutionStatus::Timeout);
    assert!(!t.resource_usage.limits_triggered.is_empty());
    assert_eq!(t.stage_results.valgrind.status, Some(StageStatus::Skipped));
    assert!(!t.recording.coverage.complete_for_scope);
    assert!(!t.recording.instruction_events.is_empty());
    processor::validate(&t).unwrap();
    let pids: HashSet<_> = t
        .recording
        .instruction_events
        .iter()
        .filter_map(|e| e.pid)
        .collect();
    tokio::time::sleep(Duration::from_millis(100)).await;
    for pid in pids {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status"));
        assert!(
            status.is_err() || status.unwrap().contains("State:\tZ"),
            "live inferior survived deadline"
        );
    }
}

#[tokio::test]
async fn source_mode_same_line_stall_is_also_bounded() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("spin_same_line.c");
    c.tracing.mode = TraceMode::Source;
    c.limits.timeout_seconds = 3;
    let start = Instant::now();
    let t = trace_program(c).await.unwrap();
    assert!(start.elapsed() < Duration::from_secs(6));
    assert_eq!(t.execution_status, ExecutionStatus::Timeout);
    assert!(!t.recording.coverage.instruction_capture_enabled);
    assert!(t.recording.instruction_events.is_empty());
}

#[tokio::test]
async fn instruction_limit_is_not_a_crash_and_journal_recovers_order() {
    let Some(_permit) = tools().await else {
        return;
    };
    let path = std::env::temp_dir().join(format!("cte-v2-journal-{}.jsonl", std::process::id()));
    let mut c = config("finite_loop.c");
    c.limits.max_instruction_events = 8;
    c.tracing.journal = Some(path.clone());
    c.tracing.retain_instruction_events = false;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::StepLimitReached);
    assert_eq!(t.recording.instruction_count, 8);
    assert!(t.recording.instruction_events.is_empty());
    assert!(t
        .recording
        .coverage
        .truncation_reasons
        .iter()
        .any(|r| r == "max_instruction_events"));
    assert!(t.recording.storage.finalized);
    let lines = std::fs::read_to_string(&path).unwrap();
    let records: Vec<serde_json::Value> = lines
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let ids: Vec<_> = records
        .iter()
        .filter(|r| r["kind"] == "instruction")
        .map(|r| r["data"]["event_id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, (0..8).collect::<Vec<_>>());
    assert_eq!(records.last().unwrap()["kind"], "finalized");
    std::fs::remove_file(path).unwrap();
    for commands in [1, 70] {
        let mut c = config("finite_loop.c");
        c.limits.max_gdb_commands = commands;
        let capped = trace_program(c).await.unwrap();
        assert_eq!(capped.execution_status, ExecutionStatus::StepLimitReached);
        assert!(capped
            .resource_usage
            .limits_triggered
            .iter()
            .any(|r| r == "max_gdb_commands"));
        assert!(capped.resource_usage.gdb_commands_issued <= commands);
    }
}

#[tokio::test]
async fn loop_events_are_dynamic_cached_and_registers_replay() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("finite_loop.c").await;
    processor::validate(&t).unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    let data = &t.recording;
    assert!(data.instruction_count > data.disassembly_table.len() as u64);
    assert!(data.profiling.disassembly_cache_hits > data.profiling.disassembly_queries);
    let unique: HashSet<_> = data.instruction_events.iter().map(|e| &e.address).collect();
    assert!(unique.len() < data.instruction_events.len());
    let event = data
        .instruction_events
        .iter()
        .rfind(|e| e.registers.as_ref().is_some_and(|r| r.complete))
        .unwrap();
    let replay = processor::replay_registers(&t, event.event_id).unwrap();
    assert!(!replay.is_empty());
    assert!(replay
        .values()
        .all(|r| r.value.is_some() || r.availability != DataValidity::Valid));
    assert!(t
        .snapshots
        .iter()
        .all(|s| s.observation.origin == "user_code"));
}

#[tokio::test]
async fn callbacks_and_recursion_and_same_line_functions_remain_observable() {
    let Some(_permit) = tools().await else {
        return;
    };
    for (fixture, required) in [
        ("callback.c", "compare"),
        ("recursive.c", "recurse"),
        ("same_line.c", "b"),
        ("nested_loops.c", "main"),
    ] {
        let t = run(fixture).await;
        processor::validate(&t).unwrap();
        assert_eq!(
            t.execution_status,
            ExecutionStatus::Completed,
            "{fixture}: {:?}",
            t.analysis_notes
        );
        assert!(
            t.snapshots.iter().any(|s| s.func == required),
            "{fixture}: missing {required}"
        );
        if fixture == "recursive.c" {
            let identities: HashSet<_> = t
                .snapshots
                .iter()
                .filter(|s| s.func == "recurse")
                .flat_map(|s| &s.variables)
                .filter(|v| v.name == "value")
                .filter_map(|v| v.identity.clone())
                .collect();
            assert!(identities.len() >= 3);
        }
    }
}

#[tokio::test]
async fn pointers_globals_and_pie_allocation_locations_are_evidence_based() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("pointer_to_pointer.c").await;
    assert!(t
        .snapshots
        .iter()
        .flat_map(|s| &s.variables)
        .any(|v| v.name == "link" && v.target.as_ref().is_some_and(|p| p.kind == "stack")));
    let t = run("globals.c").await;
    assert!(t
        .snapshots
        .iter()
        .flat_map(|s| &s.observation.globals)
        .any(|v| v.name.contains("global_value") && v.read_status == DataValidity::Valid));
    let mut c = config("pointer_demo.c");
    c.build.compiler_flags = vec!["-fPIE".into()];
    c.build.linker_flags = vec!["-pie".into()];
    let t = trace_program(c).await.unwrap();
    assert_eq!(
        t.execution_status,
        ExecutionStatus::Completed,
        "{:?}",
        t.analysis_notes
    );
    assert!(t.heap_state.iter().any(|b| b.allocation_file.is_some()));
    assert!(t
        .recording
        .allocation_lifetimes
        .iter()
        .all(|a| !a.raw_caller_address.is_empty()));
    assert!(t
        .snapshots
        .iter()
        .all(|s| !s.func.starts_with("__wrap_") && !s.file.ends_with("alloc_tracker.c")));
    assert!(t
        .recording
        .trace_gaps
        .iter()
        .any(|g| g.origin == "instrumentation_runtime"));
}

#[tokio::test]
async fn allocation_identity_survives_move_and_reuse() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("realloc_move.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    assert!(
        t.heap_state.iter().any(|b| b.realloc_from_id.is_some()),
        "fixture should force a move on supported glibc"
    );
    let t = run("address_reuse.c").await;
    let ids: HashSet<_> = t.heap_state.iter().map(|b| b.allocation_id).collect();
    assert_eq!(ids.len(), 2);
    assert!(t.heap_state.iter().all(|b| b.is_freed));
}

#[tokio::test]
async fn selected_memory_output_caps_and_missing_mappings_are_explicit() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("globals.c");
    c.tracing
        .memory_regions
        .push(c_trace_engine::config::MemoryRegion {
            address: "&global_value".into(),
            size: 4,
        });
    let t = trace_program(c).await.unwrap();
    assert!(t
        .recording
        .instruction_events
        .iter()
        .flat_map(|e| &e.memory)
        .any(|m| m.status == DataValidity::Valid && m.address.is_some()));
    assert!(t
        .snapshots
        .iter()
        .filter(|s| s.func == "main")
        .flat_map(|s| &s.observation.globals)
        .any(|v| v.name == "update::calls" && v.value == "2"));
    let mut c = config("big_output.c");
    c.limits.max_output_bytes = 64;
    let t = trace_program(c).await.unwrap();
    assert!(t.resource_usage.program_output_truncated);
    assert_eq!(
        t.execution.outcome,
        c_trace_engine::models::ProgramOutcome::OutputLimitReached
    );
    let mut c = config("hello.c");
    c.build.compiler_flags.push("-g0".into());
    let t = trace_program(c).await.unwrap();
    assert!(!t.recording.coverage.complete_for_scope);
    assert!(t
        .recording
        .coverage
        .truncation_reasons
        .iter()
        .any(|r| r == "initial_source_mapping_unavailable"));
}

#[tokio::test]
async fn all_stop_thread_limitations_are_reported() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("threads.c");
    c.build.linker_flags.push("-pthread".into());
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.recording.coverage.stop_model, "gdb_all_stop");
    assert!(t
        .recording
        .coverage
        .truncation_reasons
        .iter()
        .any(|r| r.contains("multiple_threads")));
}

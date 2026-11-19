use c_trace_engine::config::TraceMode;
use c_trace_engine::models::{
    DataValidity, DebugSessionTrace, ExecutionSnapshot, ExecutionStatus, PointerTarget,
    ScopeStatus, StageStatus, VariableInfo,
};
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
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[tokio::test]
async fn crash_globals_and_snapshot_cap_do_not_claim_complete_coverage() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("phase8r3_crash.c");
    c.tracing.discover_globals = false;
    c.tracing.globals = vec!["observed_global".into()];
    let trace = trace_program(c.clone()).await.unwrap();
    assert_eq!(trace.execution_status, ExecutionStatus::Crashed);
    let crash = trace.snapshots.last().expect("crash checkpoint");
    assert_eq!(crash.signal.as_deref(), Some("SIGSEGV"));
    assert_eq!(crash.observation.globals.len(), 1);
    assert_eq!(crash.observation.globals[0].value, "123");

    c.limits.max_snapshot_count = 2;
    c.tracing.mode = TraceMode::Dual; // fatal stop occurs after two ordinary checkpoints
    let capped = trace_program(c).await.unwrap();
    assert_eq!(capped.execution_status, ExecutionStatus::Crashed);
    assert_eq!(capped.snapshots.len(), 2);
    assert!(!capped.recording.coverage.complete_for_scope);
    assert!(capped
        .recording
        .coverage
        .truncation_reasons
        .iter()
        .any(|reason| reason.contains("max_snapshot_count")));
}

#[tokio::test]
async fn requested_global_query_failure_and_budget_exhaustion_are_incomplete() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("phase8r3_crash.c");
    c.tracing.discover_globals = false;
    c.tracing.globals = vec!["missing_requested_global".into()];
    let failed = trace_program(c.clone()).await.unwrap();
    assert_eq!(failed.execution_status, ExecutionStatus::Crashed);
    let crash = failed.snapshots.last().unwrap();
    assert_eq!(crash.observation.globals.len(), 1);
    assert!(crash.observation.globals[0].read_error.is_some());
    assert!(!crash.observation.complete);
    assert!(!crash.observation.errors.is_empty());

    c.tracing.globals = vec!["observed_global".into()];
    c.limits.max_snapshot_memory_bytes = 1;
    let limited = trace_program(c).await.unwrap();
    assert_eq!(limited.execution_status, ExecutionStatus::Crashed);
    let crash = limited.snapshots.last().unwrap();
    assert_eq!(crash.observation.globals.len(), 1);
    assert!(!crash.observation.complete);
    assert!(crash.observation.globals[0]
        .memory_evidence
        .as_ref()
        .is_some_and(|e| e.status != "full"));
}

#[tokio::test]
async fn exact_fit_inferior_output_stays_complete() {
    let Some(_permit) = tools().await else { return };
    // Independent POSIX write oracle: four 16-byte writes, no newline or
    // terminal escape processing, exactly 64 output bytes.
    let mut c = config("phase8b_exact_output.c");
    c.limits.max_output_bytes = 64;
    let trace = trace_program(c).await.unwrap();
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert!(!trace.resource_usage.program_output_truncated);
    assert_eq!(trace.resource_usage.program_output_bytes, 64);
}

#[tokio::test]
async fn blocked_inferior_stop_wait_is_timeout_not_debugger_failure() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("phase8b_blocked.c");
    c.limits.timeout_seconds = 25; // exceeds the fixed 10-second stop wait
    c.tracing.mode = TraceMode::Source;
    let trace = trace_program(c).await.unwrap();
    assert_eq!(
        trace.execution_status,
        ExecutionStatus::Timeout,
        "the step was accepted; a blocked inferior is not a failed MI command: {:?}",
        trace.analysis_notes
    );
    assert!(trace
        .resource_usage
        .limits_triggered
        .iter()
        .any(|reason| reason == "debugger_stop_wait_timeout"));
    assert!(!trace.snapshots.is_empty());
    assert!(!trace.recording.coverage.complete_for_scope);
}

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
async fn phase2_probe_reuses_metadata_but_refreshes_array_struct_and_global_values() {
    let Some(_permit) = tools().await else { return };
    let t = run("aggregate.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    processor::validate(&t).unwrap();
    let profiling = &t.recording.profiling;
    assert!(profiling.saved_variable_metadata_queries.unwrap_or(0) > 0);
    assert!(
        profiling
            .gdb_command_categories
            .as_ref()
            .and_then(|counts| counts.get("lexical_probe"))
            .copied()
            .unwrap_or(0)
            > 0
    );
    let locals: Vec<_> = t.snapshots.iter().flat_map(|s| &s.variables).collect();
    let paths: Vec<_> = locals.iter().filter(|v| v.name == "local").collect();
    assert!(paths.len() > 2);
    assert!(paths.windows(2).any(|pair| pair[0].value != pair[1].value));
    assert!(paths.iter().all(|v| v.size_bytes == Some(48)));
    let global_values: Vec<_> = t
        .snapshots
        .iter()
        .flat_map(|s| &s.observation.globals)
        .filter(|v| v.name.ends_with("::g_path"))
        .map(|v| v.value.as_str())
        .collect();
    assert!(global_values.len() > 2);
    assert!(global_values.windows(2).any(|pair| pair[0] != pair[1]));
    let store = c_trace_engine::trace_parser::TraceStore::from_trace(t.clone()).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
    assert_eq!(store.raw().trace().snapshots.len(), t.snapshots.len());
    assert_eq!(
        store.semantic_input().raw().trace().snapshots.len(),
        t.snapshots.len()
    );
}

#[tokio::test]
async fn phase2_vla_size_changes_across_distinct_activations() {
    let Some(_permit) = tools().await else { return };
    let t = run("vla_metadata.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    let arrays: Vec<_> = t
        .snapshots
        .iter()
        .flat_map(|s| &s.variables)
        .filter(|v| v.name == "items" && v.size_bytes.is_some())
        .collect();
    let sizes: HashSet<_> = arrays.iter().filter_map(|v| v.size_bytes).collect();
    assert!(sizes.contains(&12) && sizes.contains(&20), "{sizes:?}");
    let activations: HashSet<_> = arrays.iter().filter_map(|v| v.activation_id).collect();
    assert!(activations.len() >= 2);
}

#[tokio::test]
async fn phase2_global_cache_distinguishes_same_name_in_two_translation_units() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("two_statics_main.c");
    c.build
        .additional_sources
        .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/c/two_statics_other.c"));
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    let globals: Vec<_> = t
        .snapshots
        .iter()
        .flat_map(|s| &s.observation.globals)
        .filter(|v| v.name.ends_with("::shared"))
        .collect();
    let names: HashSet<_> = globals.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        names.len(),
        2,
        "same-name statics need separate qualified identities: {names:?}"
    );
    assert!(globals.iter().any(|v| v.value == "11"));
    assert!(globals.iter().any(|v| v.value == "20" || v.value == "22"));
    assert!(globals.iter().any(|v| v.value == "23"));
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
    // The deadline begins before GCC/GDB startup. Leave enough time for at
    // least one real stop on a busy host; still require a runtime timeout,
    // partial instruction evidence and no subsequent Valgrind pass.
    c.limits.timeout_seconds = 8;
    c.limits.max_steps = 100_000;
    c.limits.max_source_checkpoints = 100_000;
    let start = Instant::now();
    let t = trace_program(c).await.unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(11),
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
async fn loop_events_are_dynamic_and_registers_are_not_collected() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("finite_loop.c").await;
    processor::validate(&t).unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    let data = &t.recording;
    // Static disassembly caching still effective; dynamic execution preserved.
    assert!(data.instruction_count > data.disassembly_table.len() as u64);
    assert!(data.profiling.disassembly_cache_hits > data.profiling.disassembly_queries);
    let unique: HashSet<_> = data.instruction_events.iter().map(|e| &e.address).collect();
    assert!(unique.len() < data.instruction_events.len());

    // --- V2.1 negative register assertions ---------------------------------
    // 1. No register payload was collected or serialized on any event.
    assert!(
        data.instruction_events
            .iter()
            .all(|e| e.registers.is_none()),
        "per-instruction register payloads must be absent in V2.1"
    );
    assert!(data.initial_registers.is_none());
    // 2. The register-query time metric is zero: proves the expensive
    //    `-data-list-register-values` polling was removed, not just hidden.
    assert_eq!(
        data.profiling.register_time_us, 0,
        "register_time_us must be 0 in V2.1 (no register queries issued)"
    );
    // 3. The serialized event JSON contains no "registers" key at all.
    let first_event = data
        .instruction_events
        .first()
        .expect("loop produces instruction events");
    let serialized = serde_json::to_value(first_event).unwrap();
    assert!(
        serialized.get("registers").is_none(),
        "V2.1 events must not serialize a registers key: {serialized}"
    );
    let whole_trace = serde_json::to_value(&t).unwrap();
    assert!(
        whole_trace
            .pointer("/recording/initial_registers")
            .is_none(),
        "V2.1 traces must not serialize initial_registers"
    );
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
    // V2.1 (F-04): output-limit termination has its own execution status
    // instead of being mislabeled as a step limit.
    assert_eq!(t.execution_status, ExecutionStatus::OutputLimitReached);
    assert_eq!(
        t.execution.outcome,
        c_trace_engine::models::ProgramOutcome::OutputLimitReached
    );
    assert!(t
        .resource_usage
        .limits_triggered
        .iter()
        .any(|r| r == "max_output_bytes"));
    // The triggered limit must be recorded exactly once (F-03).
    assert_eq!(
        t.resource_usage
            .limits_triggered
            .iter()
            .filter(|r| *r == "max_output_bytes")
            .count(),
        1
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

// ---------------------------------------------------------------------------
// V2.1 regressions: F-01 honest interrupted allocation-tracker metadata
// ---------------------------------------------------------------------------

#[tokio::test]
async fn allocation_tracker_metadata_is_honest_when_timed_out() {
    let Some(_permit) = tools().await else {
        return;
    };
    // many_allocs.c allocates continuously. Five seconds leaves enough room
    // for compilation/GDB startup and at least one verified event on slower
    // hosts, but not for 5,000 source-stepped allocation/free pairs.
    let mut c = config("many_allocs.c");
    c.limits.timeout_seconds = 5;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Timeout);
    let tracker = &t.stage_results.allocation_tracker;
    // V2.1 (F-01): an interrupted run must NOT claim status=completed with
    // output_complete=false and partial_results_available=false.
    assert_eq!(tracker.status, Some(StageStatus::TimedOut));
    assert!(
        tracker.reason.is_some(),
        "interrupted allocation tracker needs a reason"
    );
    assert_eq!(tracker.output_complete, Some(false));
    assert_eq!(
        tracker.partial_results_available,
        Some(true),
        "allocation events ingested before the timeout are available"
    );
    assert!(!t.heap_events.is_empty());
}

#[tokio::test]
async fn allocation_tracker_metadata_is_honest_at_step_limit() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("many_allocs.c");
    c.limits.max_steps = 50;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::StepLimitReached);
    let tracker = &t.stage_results.allocation_tracker;
    assert_ne!(tracker.status, Some(StageStatus::Completed));
    assert!(
        tracker
            .reason
            .as_deref()
            .is_some_and(|r| r.contains("resource limit")),
        "reason should name the resource-limit interruption: {:?}",
        tracker.reason
    );
    assert_eq!(tracker.output_complete, Some(false));
    assert_eq!(tracker.partial_results_available, Some(true));
    assert!(!t.heap_events.is_empty());
}

#[tokio::test]
async fn allocation_tracker_reports_completed_for_terminal_runs() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("pointer_demo.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    let tracker = &t.stage_results.allocation_tracker;
    assert_eq!(tracker.status, Some(StageStatus::Completed));
    assert_eq!(tracker.output_complete, Some(true));
}

// ---------------------------------------------------------------------------
// V2.1 regressions: F-02 sanitizer trigger modes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sanitizer_trigger_never_skips_the_pass() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("null_deref.c");
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Crashed);
    assert!(t.sanitizer_report.is_none());
    assert_eq!(t.stage_results.sanitizer.status, Some(StageStatus::Skipped));
    assert!(t
        .stage_results
        .sanitizer
        .reason
        .as_deref()
        .is_some_and(|r| r.contains("never")));
}

#[tokio::test]
async fn sanitizer_trigger_on_crash_runs_on_crash_and_skips_clean_runs() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("null_deref.c");
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::OnCrash;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Crashed);
    // V2 behavior restored: the pass runs on crash.
    assert!(
        t.sanitizer_report.is_some(),
        "on_crash must run the sanitizer pass for a crashing program; stage: {:?}",
        t.stage_results.sanitizer
    );
    // A clean, memory-safe program must not trigger the pass.
    let mut c = config("hello.c");
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::OnCrash;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    assert!(t.sanitizer_report.is_none());
    assert!(t
        .stage_results
        .sanitizer
        .reason
        .as_deref()
        .is_some_and(|r| r.contains("on_crash")));
}

#[tokio::test]
async fn sanitizer_trigger_on_crash_or_valgrind_error_covers_noncrashing_valgrind_findings() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("use_after_free.c");
    c.limits.timeout_seconds = 12;
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::OnCrashOrValgrindError;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    // Valgrind finds the UAF even though the program exits cleanly; the
    // sanitizer pass must therefore run (F-02) rather than be skipped.
    let vg_had_errors = t
        .valgrind_report
        .as_ref()
        .is_some_and(|r| !r.errors.is_empty() || !r.detailed_errors.is_empty());
    if !vg_had_errors {
        // Tool unavailable on this machine: the valgrind stage must explain it.
        assert_eq!(t.stage_results.valgrind.status, Some(StageStatus::Skipped));
        return;
    }
    assert!(
        t.sanitizer_report.is_some(),
        "default trigger must run ASan after non-crashing Valgrind errors; stage: {:?} notes: {:?}",
        t.stage_results.sanitizer,
        t.analysis_notes
    );
    // A normal memory-safe program must not trigger the pass.
    let mut c = config("hello.c");
    c.limits.timeout_seconds = 12;
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::OnCrashOrValgrindError;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    if t.stage_results.valgrind.status == Some(StageStatus::Skipped) {
        return; // valgrind unavailable: cannot have reported errors
    }
    assert!(t.sanitizer_report.is_none());
    assert!(t
        .stage_results
        .sanitizer
        .reason
        .as_deref()
        .is_some_and(|r| r.contains("on_crash_or_valgrind_error")));
}

#[tokio::test]
async fn sanitizer_trigger_always_runs_even_for_clean_safe_programs() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("hello.c");
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Always;
    let t = trace_program(c).await.unwrap();
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    assert!(
        t.sanitizer_report.is_some(),
        "always must run the sanitizer pass; stage: {:?}",
        t.stage_results.sanitizer
    );
}

// ---------------------------------------------------------------------------
// V2.1 regressions: F-05 explicit tool-path overrides
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gdb_path_override_allows_failure_injection() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("hello.c");
    c.gdb_path = Some("/nonexistent/cannot-be-gdb".into());
    let result = trace_program(c).await;
    match result {
        Err(c_trace_engine::error::EngineError::MissingTool { tool, .. }) => {
            assert!(tool.contains("cannot-be-gdb") || tool == "gdb");
        }
        other => panic!("expected MissingTool for a bogus gdb path, got: {other:?}"),
    }
}

#[tokio::test]
async fn valgrind_path_override_allows_failure_injection() {
    let Some(_permit) = tools().await else {
        return;
    };
    let mut c = config("pointer_demo.c");
    c.valgrind_path = Some("/nonexistent/cannot-be-valgrind".into());
    let t = trace_program(c).await.unwrap();
    // GDB execution is unaffected; Valgrind discovery fails and is skipped
    // with an explanatory note (never silently).
    assert_eq!(t.execution_status, ExecutionStatus::Completed);
    assert_eq!(t.stage_results.valgrind.status, Some(StageStatus::Skipped));
    assert!(t.valgrind_report.is_none());
    assert!(t
        .analysis_notes
        .iter()
        .any(|n| n.contains("valgrind not found")));
}

// ---------------------------------------------------------------------------
// V2.2 Phase 1: pointer-to-variable identity and scope liveness evidence.
// ---------------------------------------------------------------------------

fn variable<'a>(s: &'a ExecutionSnapshot, name: &str) -> Option<&'a VariableInfo> {
    s.variables
        .iter()
        .find(|v| v.name == name)
        .or_else(|| s.observation.globals.iter().find(|v| v.name == name))
}

fn target<'a>(s: &'a ExecutionSnapshot, name: &str) -> Option<&'a PointerTarget> {
    variable(s, name)?.target.as_ref()
}

#[tokio::test]
async fn pointer_identities_follow_reassignment_aliases_and_stay_evidence_based() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("pointer_identity.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);

    // A: the first stack target of `p` reuses `x`'s recorder-assigned
    // identity — nothing is invented from the name or the address.
    let first = t
        .snapshots
        .iter()
        .find(|s| target(s, "p").is_some_and(|t| t.kind == "stack"))
        .expect("p must resolve to a stack variable once initialized");
    let x = variable(first, "x").expect("x recorded beside its pointer");
    let p = target(first, "p").unwrap();
    assert_eq!(p.base_address.as_deref(), x.address.as_deref());
    assert_eq!(p.offset_bytes, Some(0));
    assert_eq!(p.variable_identity.as_deref(), x.identity.as_deref());

    // C: the alias `q` resolves to the same identity at the same stop.
    let aliased = t
        .snapshots
        .iter()
        .find(|s| {
            target(s, "p").and_then(|t| t.variable_identity.as_deref())
                == variable(s, "x").and_then(|v| v.identity.as_deref())
                && target(s, "q").and_then(|t| t.variable_identity.as_deref())
                    == variable(s, "x").and_then(|v| v.identity.as_deref())
        })
        .expect("p and q must both resolve to x while aliased");

    // B: after reassignment p resolves to `y`, and `q` keeps `x` — the
    // association follows the recorded pointer value, never a stale one.
    let x_id = variable(aliased, "x").unwrap().identity.clone().unwrap();
    let y_id = variable(aliased, "y")
        .expect("y recorded")
        .identity
        .clone()
        .unwrap();
    assert_ne!(x_id, y_id);
    let reassigned = t
        .snapshots
        .iter()
        .filter(|s| {
            target(s, "p").is_some_and(|t| t.kind == "stack")
                && target(s, "q").is_some_and(|t| t.kind == "stack")
        })
        .find(|s| target(s, "p").unwrap().variable_identity.as_deref() == Some(y_id.as_str()))
        .expect("p must resolve to y after reassignment");
    assert_eq!(
        target(reassigned, "q")
            .unwrap()
            .variable_identity
            .as_deref(),
        Some(x_id.as_str()),
        "q must keep its alias target"
    );

    // Evidence-based invariant: whenever a stack target carries an identity,
    // that identity belongs to a variable recorded in the same snapshot whose
    // address matches the target base. No cross-snapshot or invented joins.
    for s in &t.snapshots {
        for v in &s.variables {
            let Some(t) = v.target.as_ref() else {
                continue;
            };
            if t.kind != "stack" {
                continue;
            }
            if let Some(id) = &t.variable_identity {
                let owner = s.variables.iter().find(|o| {
                    o.address.as_deref() == t.base_address.as_deref()
                        && o.identity.as_deref() == Some(id.as_str())
                });
                assert!(
                    owner.is_some(),
                    "unbacked pointer identity {id} for {} at line {}",
                    v.name,
                    s.line
                );
            }
        }
    }

    // D: null pointers keep no identity at any stop.
    for s in &t.snapshots {
        if let Some(t) = target(s, "nul") {
            assert_eq!(t.kind, "null");
            assert_eq!(t.variable_identity, None);
        }
    }

    // I: an interior pointer stays attributed to the container with its
    // offset preserved; no element identity is fabricated.
    let interior = t
        .snapshots
        .iter()
        .find(|s| target(s, "mid").is_some_and(|t| t.kind == "stack"))
        .expect("mid must resolve to its array");
    let a = variable(interior, "a").expect("array recorded");
    let mid = target(interior, "mid").unwrap();
    assert_eq!(mid.base_address.as_deref(), a.address.as_deref());
    assert_eq!(mid.offset_bytes, Some(4));
    assert_eq!(mid.variable_identity.as_deref(), a.identity.as_deref());

    // J: a heap address match identifies a region, not the pointer's origin;
    // no local identity is assigned to heap memory.
    let heap = t
        .snapshots
        .iter()
        .find(|s| target(s, "hp").is_some_and(|t| t.kind == "heap"))
        .expect("hp must resolve to its heap allocation");
    let hp = target(heap, "hp").unwrap();
    assert!(hp.address_match_allocation_id.is_some());
    assert_eq!(hp.allocation_id, None);
    assert_eq!(hp.variable_identity, None);

    // Globals cross-enriched against frame-0 pointers reuse the recorder's
    // `symbol:` identity (globals are recorded under their file-qualified
    // name).
    let global = t
        .snapshots
        .iter()
        .find(|s| target(s, "gp").is_some_and(|t| t.kind == "global"))
        .expect("gp must resolve to the global");
    let g = global
        .observation
        .globals
        .iter()
        .find(|v| v.name.ends_with("::g_value"))
        .expect("g_value recorded as a global");
    assert!(g
        .identity
        .as_deref()
        .is_some_and(|i| i.starts_with("symbol:")));
    assert_eq!(
        target(global, "gp").unwrap().variable_identity.as_deref(),
        g.identity.as_deref()
    );
}

#[tokio::test]
async fn scope_liveness_tracks_lexical_blocks_without_conflating_initialization() {
    let Some(_permit) = tools().await else {
        return;
    };
    let t = run("scope_liveness.c").await;
    assert_eq!(t.execution_status, ExecutionStatus::Completed);

    // Every variable with a defining block range carries a scope status, and
    // an Active status is backed by the frame-0 program counter actually
    // lying inside the half-open block range.
    for s in &t.snapshots {
        let pc = s
            .call_stack
            .first()
            .and_then(|f| f.address.as_deref())
            .and_then(|a| a.strip_prefix("0x"))
            .and_then(|a| u64::from_str_radix(a, 16).ok());
        for v in &s.variables {
            if let Some(id) = v
                .lexical_scope_id
                .as_deref()
                .and_then(|id| id.strip_prefix("block:"))
            {
                let Some(status) = v.scope_status else {
                    panic!("block-scoped variable without status: {}", v.name);
                };
                let Some((range, _)) = id.split_once(':') else {
                    continue;
                };
                let Some((start, end)) = range.split_once('-') else {
                    continue;
                };
                let (Ok(start), Ok(end)) =
                    (u64::from_str_radix(start, 16), u64::from_str_radix(end, 16))
                else {
                    continue;
                };
                if status == ScopeStatus::Active {
                    let pc = pc.expect("active status implies a frame address");
                    assert!(
                        start <= pc && pc < end,
                        "{} active at line {} but pc {pc:#x} outside {id}",
                        v.name,
                        s.line
                    );
                }
            }
        }
    }

    // E: block-scoped `b` is absent before its block, Active while listed,
    // and absent again after the block exits.
    let first_b = t
        .snapshots
        .iter()
        .position(|s| variable(s, "b").is_some())
        .expect("b must be recorded inside its block");
    assert!(
        t.snapshots[..first_b]
            .iter()
            .all(|s| variable(s, "b").is_none()),
        "b must not be listed before its block is active"
    );
    for s in t.snapshots.iter().skip(first_b) {
        if let Some(b) = variable(s, "b") {
            assert_eq!(b.scope_status, Some(ScopeStatus::Active));
        }
    }
    let last_main = t
        .snapshots
        .iter()
        .rev()
        .find(|s| s.func == "main")
        .expect("main frames recorded");
    assert!(
        variable(last_main, "b").is_none(),
        "b is out of scope at exit"
    );

    // Scope status never implies initialization: main-body locals are listed
    // (and Active) at the first stop, before their declaration lines have
    // executed. Their value/byte validity stays recorded separately.
    let entry = t.snapshots.iter().find(|s| s.func == "main").unwrap();
    let before_decl = entry
        .variables
        .iter()
        .filter(|v| v.declaration_line.is_some_and(|l| l > entry.line))
        .count();
    assert!(
        before_decl > 0,
        "expected function-body locals listed before their declaration lines"
    );
    for v in &entry.variables {
        assert_eq!(v.scope_status, Some(ScopeStatus::Active));
    }

    // F: the two `x` declarations keep distinct identities; pointer
    // resolution follows addresses and never selects the shadowed outer `x`.
    let dual: Vec<&ExecutionSnapshot> = t
        .snapshots
        .iter()
        .filter(|s| s.variables.iter().filter(|v| v.name == "x").count() == 2)
        .collect();
    assert!(!dual.is_empty(), "shadowed x pair must be recorded");
    for s in &dual {
        let xs: Vec<&VariableInfo> = s.variables.iter().filter(|v| v.name == "x").collect();
        let ids: HashSet<&str> = xs.iter().filter_map(|v| v.identity.as_deref()).collect();
        assert_eq!(ids.len(), 2, "shadowed x identities must differ");
        let inner = xs
            .iter()
            .max_by_key(|v| v.declaration_line.unwrap_or(0))
            .unwrap();
        let outer = xs
            .iter()
            .min_by_key(|v| v.declaration_line.unwrap_or(0))
            .unwrap();
        assert_ne!(inner.lexical_scope_id, outer.lexical_scope_id);
    }
    let inner_of = |s: &ExecutionSnapshot| {
        s.variables
            .iter()
            .filter(|v| v.name == "x")
            .max_by_key(|v| v.declaration_line.unwrap_or(0))
            .and_then(|v| v.identity.clone())
            .unwrap()
    };
    let p_inner = t
        .snapshots
        .iter()
        .find(|s| target(s, "p").is_some_and(|t| t.variable_identity == Some(inner_of(s))))
        .expect("p must resolve to the inner x by address");
    assert_eq!(
        target(p_inner, "p").unwrap().variable_identity.as_deref(),
        Some(inner_of(p_inner).as_str())
    );
    // Later `p = &b` resolves to `b`'s identity instead — no stale x.
    let p_b = t
        .snapshots
        .iter()
        .filter(|s| variable(s, "b").is_some())
        .find(|s| {
            target(s, "p").and_then(|t| t.variable_identity.as_deref())
                == variable(s, "b").and_then(|v| v.identity.as_deref())
        })
        .expect("p must resolve to b after reassignment");
    assert_ne!(
        target(p_b, "p").unwrap().variable_identity.as_deref(),
        Some(inner_of(p_b).as_str())
    );

    // G: recursive activations carry distinct activation ids, and each
    // helper local's identity encodes its own activation.
    let acts: HashSet<u64> = t
        .snapshots
        .iter()
        .filter(|s| s.func == "helper")
        .flat_map(|s| &s.variables)
        .filter(|v| v.name == "local")
        .filter_map(|v| v.activation_id)
        .collect();
    assert!(acts.len() >= 4, "distinct helper activations: {acts:?}");
    for s in t.snapshots.iter().filter(|s| s.func == "helper") {
        for v in &s.variables {
            if let (Some(id), Some(act)) = (v.identity.as_deref(), v.activation_id) {
                if v.name == "local" {
                    assert!(
                        id.starts_with(&format!("activation:{act}:")),
                        "identity {id} does not encode activation {act}"
                    );
                }
            }
        }
    }

    // H: recursion reuses the same stack address across activations, but the
    // recorded identity of the reused slot always belongs to the *current*
    // activation; and a dangling pointer into a returned frame resolves to
    // nothing (no stale identity, no invented match).
    let mut by_address: HashMap<String, HashSet<u64>> = HashMap::new();
    for s in t.snapshots.iter().filter(|s| s.func == "helper") {
        if let Some(local) = variable(s, "local") {
            if let (Some(addr), Some(act)) = (local.address.clone(), local.activation_id) {
                by_address.entry(addr).or_default().insert(act);
            }
        }
    }
    assert!(
        by_address.values().any(|acts| acts.len() >= 2),
        "expected stack address reuse across helper activations: {by_address:?}"
    );
    for s in t.snapshots.iter().filter(|s| s.func == "main") {
        if let Some(t) = target(s, "self") {
            // Null before the call; dangling into a dead frame afterwards.
            if t.kind == "null" {
                assert_eq!(t.variable_identity, None);
            } else {
                assert_eq!(
                    t.variable_identity, None,
                    "dangling self must not acquire a variable identity"
                );
            }
        }
    }
    assert!(
        t.snapshots
            .iter()
            .filter(|s| s.func == "main")
            .any(|s| target(s, "self").is_some_and(|t| t.kind == "stack")),
        "expected self to observe its dead-frame stack region"
    );
}

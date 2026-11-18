//! Bounded stack-overflow evidence from one real source-mode GDB recording.
use c_trace_engine::config::{SanitizerTrigger, TraceMode};
use c_trace_engine::models::ExecutionStatus;
use c_trace_engine::trace::trace_program;
use c_trace_engine::trace_parser::TraceStore;
use c_trace_engine::TraceConfig;
use std::collections::HashSet;
use std::path::PathBuf;

#[tokio::test]
async fn deep_recursion_reports_lost_activation_continuity_without_false_id_reuse() {
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("SKIPPED deep recursion test: required tool {tool} unavailable");
            return;
        }
    }
    let mut config = TraceConfig::default();
    config.build.source_file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/c/runtime_memory_deep.c");
    config.tracing.mode = TraceMode::Source;
    config.tracing.discover_globals = false;
    config.limits.max_steps = 1600;
    config.limits.max_source_checkpoints = 1600;
    config.limits.max_snapshot_count = 1600;
    config.limits.max_trace_memory_bytes = 64 * 1024 * 1024;
    config.limits.timeout_seconds = 90;
    config.limits.debugger_timeout_seconds = 90;
    config.limits.max_caller_frames = 3;
    config.limits.max_caller_variables = 8;
    config.limits.max_variables_per_snapshot = 8;
    config.sanitizer_trigger = SanitizerTrigger::Never;
    config.valgrind_path = Some("/nonexistent/phase4-no-valgrind".into());
    let trace = trace_program(config).await.unwrap();
    assert!(trace.compiled_successfully, "{:?}", trace.compile_errors);
    assert_eq!(
        trace.execution_status,
        ExecutionStatus::Completed,
        "{:?}",
        trace.resource_usage.limits_triggered
    );
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let overflow = |snapshot: &c_trace_engine::models::ExecutionSnapshot| {
        snapshot
            .observation
            .errors
            .iter()
            .any(|error| error.contains("activation continuity lost"))
    };
    let first = trace
        .snapshots
        .iter()
        .position(overflow)
        .expect("205 recursive calls exceed frame bound");
    assert!(first > 0);
    let previous_ids: HashSet<_> = trace.snapshots[first - 1]
        .variables
        .iter()
        .filter_map(|var| var.activation_id)
        .collect();
    assert!(!previous_ids.is_empty());
    let mut preserved_bytes = false;
    let mut recovered = false;
    for snapshot in trace.snapshots.iter().skip(first) {
        assert!(snapshot.call_stack.len() <= 200);
        if overflow(snapshot) {
            assert_eq!(snapshot.call_stack.len(), 200);
            assert!(!snapshot.observation.complete);
            assert!(snapshot.caller_frames.is_empty());
            for var in &snapshot.variables {
                assert!(var.activation_id.is_none());
                if var.scope.as_deref() != Some("static") {
                    assert!(var.identity.is_none());
                }
                preserved_bytes |= var.address.is_some() && var.raw_bytes_hex.is_some();
            }
        } else if snapshot.func == "deep" {
            for id in snapshot
                .variables
                .iter()
                .filter_map(|var| var.activation_id)
            {
                assert!(
                    !previous_ids.contains(&id),
                    "incomplete recursive path reused activation {id}"
                );
                recovered = true;
            }
        }
    }
    assert!(preserved_bytes);
    assert!(recovered);
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

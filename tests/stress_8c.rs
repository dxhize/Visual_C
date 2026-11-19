//! Bounded integration stress: competing limits, retained partial evidence,
//! journal/export scope, and repeated session ownership. No unbounded fork.
use c_trace_engine::config::{SanitizerTrigger, TraceMode};
use c_trace_engine::models::ExecutionStatus;
use c_trace_engine::{trace::trace_program, TraceConfig};
use std::path::PathBuf;
use std::time::Duration;

fn config(fixture: &str) -> TraceConfig {
    let mut c = TraceConfig::default();
    c.build.source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/c")
        .join(fixture);
    c.tracing.mode = TraceMode::Source;
    c.sanitizer_trigger = SanitizerTrigger::Never;
    c.valgrind_path = Some("/nonexistent/8c-valgrind".into());
    c.limits.timeout_seconds = 25;
    c
}

async fn tools() -> bool {
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return false;
        }
    }
    true
}

#[tokio::test]
async fn step_limit_and_session_deadline_win_only_when_they_actually_fire() {
    if !tools().await {
        return;
    }
    for _ in 0..2 {
        let mut steps = config("infinite_loop.c");
        steps.limits.max_steps = 8;
        let result = tokio::time::timeout(Duration::from_secs(28), trace_program(steps))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.execution_status, ExecutionStatus::StepLimitReached);
        assert!(result
            .resource_usage
            .limits_triggered
            .iter()
            .any(|l| l == "max_steps"));
        assert!(!result.snapshots.is_empty());
        assert!(!result.recording.coverage.complete_for_scope);

        let mut deadline = config("infinite_loop.c");
        deadline.limits.timeout_seconds = 5;
        deadline.limits.max_steps = 200_000;
        let timed = tokio::time::timeout(Duration::from_secs(9), trace_program(deadline))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(timed.execution_status, ExecutionStatus::Timeout);
        assert!(!timed.recording.coverage.complete_for_scope);
        assert!(timed
            .resource_usage
            .limits_triggered
            .iter()
            .any(|l| l.contains("timeout") || l.contains("deadline")));
    }
}

#[tokio::test]
async fn gdb_exits_after_version_probe_without_false_observations() {
    if !tools().await {
        return;
    }
    let dir = std::env::temp_dir().join(format!("cte-8c-gdb-death-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("fake-gdb");
    std::fs::write(
        &script,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'GNU gdb 18'; exit 0; fi\nexit 7\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions).unwrap();
    let mut c = config("hello.c");
    c.gdb_path = Some(script.to_string_lossy().into_owned());
    let trace = tokio::time::timeout(Duration::from_secs(25), trace_program(c))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(trace.execution_status, ExecutionStatus::DebuggerFailure);
    assert!(trace.snapshots.is_empty());
    assert!(!trace.recording.coverage.complete_for_scope);
    assert_eq!(
        trace.stage_results.debugger.status,
        Some(c_trace_engine::models::StageStatus::Failed)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn repeated_short_sessions_release_owned_processes_and_large_buffers() {
    if !tools().await {
        return;
    }
    let rss = || -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let mut samples = Vec::new();
    for _ in 0..4 {
        let result =
            tokio::time::timeout(Duration::from_secs(25), trace_program(config("hello.c")))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(result.execution_status, ExecutionStatus::Completed);
        assert_eq!(result.resource_usage.inferior_exit_code, Some(0));
        assert!(!result.snapshots.is_empty());
        drop(result);
        tokio::time::sleep(Duration::from_millis(100)).await;
        samples.push(rss());
    }
    // Allocator arenas and one-time tool caches need not return to baseline;
    // a persistent 16 MiB growth after a warm session would be suspicious.
    eprintln!("8C repeated-session RSS KiB: {samples:?}");
    assert!(
        samples[3].saturating_sub(samples[0]) < 16 * 1024,
        "RSS KiB across four independent sessions: {samples:?}"
    );
}

#[tokio::test]
async fn journal_budget_stops_capture_but_does_not_bound_final_json() {
    if !tools().await {
        return;
    }
    let mut c = config("hello.c");
    c.tracing.mode = TraceMode::Dual;
    c.limits.max_trace_bytes = 2048;
    let limit = c.limits.max_trace_bytes;
    let result = tokio::time::timeout(Duration::from_secs(28), trace_program(c))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.execution_status, ExecutionStatus::StepLimitReached);
    assert!(result
        .resource_usage
        .limits_triggered
        .iter()
        .any(|l| l == "max_trace_bytes"));
    assert!(!result.recording.coverage.complete_for_scope);
    assert!(result.recording.storage.bytes_written <= limit as u64);
    // The independent export budget rejects this still-complete-in-memory
    // trace without touching stdout, while a larger export preserves it.
    let actual = serde_json::to_vec_pretty(&result).unwrap().len() + 1;
    assert!(actual > limit);
    let mut stdout = Vec::new();
    assert!(
        c_trace_engine::json_export::export_json(&result, true, limit, None, &mut stdout)
            .unwrap_err()
            .to_string()
            .contains("max_json_export_bytes")
    );
    assert!(stdout.is_empty());
    c_trace_engine::json_export::export_json(&result, true, actual, None, &mut stdout).unwrap();
    assert_eq!(stdout.len(), actual);
    let round_trip: c_trace_engine::DebugSessionTrace = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(
        serde_json::to_value(&round_trip).unwrap(),
        serde_json::to_value(&result).unwrap()
    );
}

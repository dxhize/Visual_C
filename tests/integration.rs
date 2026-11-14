//! Integration tests: the engine against real tools (gcc, gdb, valgrind,
//! addr2line, ASan when available).
//!
//! Every test verifies its required tools first and exits early with a
//! printed reason when one is missing — the suite still passes on machines
//! without Valgrind etc., but tells the developer exactly what was skipped.
//!
//! Run with: `cargo test --test integration`

use std::path::PathBuf;

use c_trace_engine::config::{BuildConfig, TraceConfig};
use c_trace_engine::models::ExecutionStatus;
use c_trace_engine::resource_limits::ExecutionLimits;
use c_trace_engine::trace;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir().join("fixtures").join("c").join(name)
}

/// Probe a tool by running `tool --version`; returns the version line.
async fn tool_version(tool: &str) -> Option<String> {
    let out = tokio::process::Command::new(tool)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|l| l.to_string())
}

async fn require(tools: &[&str]) -> bool {
    for tool in tools {
        match tool_version(tool).await {
            Some(v) => println!("using {tool}: {v}"),
            None => {
                println!(
                    "SKIPPED: required tool `{tool}` is not installed on this machine"
                );
                return false;
            }
        }
    }
    true
}

fn config_for(source: &std::path::Path, extra: impl FnOnce(&mut TraceConfig)) -> TraceConfig {
    let build = BuildConfig {
        source_file: source.to_path_buf(),
        additional_sources: Vec::new(),
        object_files: Vec::new(),
        include_dirs: Vec::new(),
        defines: Vec::new(),
        compiler_flags: Vec::new(),
        linker_flags: Vec::new(),
        library_dirs: Vec::new(),
        libraries: Vec::new(),
        output_name: None,
        working_directory: None,
    };
    let mut config = TraceConfig {
        build,
        program_args: Vec::new(),
        inferior_working_directory: None,
        limits: ExecutionLimits::default(),
        output_file: None,
        pretty: true,
        keep_temp: false,
    };
    extra(&mut config);
    config
}

fn bounded(config: &mut TraceConfig) {
    config.limits.timeout_seconds = 5;
    config.limits.max_steps = 400;
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compiles_sample_fixture() {
    if !require(&["gcc"]).await {
        return;
    }
    let config = config_for(&fixture("hello.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    assert!(trace.compiled_successfully, "compilation should succeed");
    assert!(
        trace.compile_errors.is_empty(),
        "unexpected diagnostics: {:?}",
        trace.compile_errors
    );
}

#[tokio::test]
async fn compile_failure_emits_partial_trace() {
    if !require(&["gcc"]).await {
        return;
    }
    let broken = std::env::temp_dir().join("cte-broken-integration.c");
    std::fs::write(&broken, "int main(void { return 0; }\n").unwrap();
    let config = config_for(&broken, bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    assert_eq!(trace.execution_status, ExecutionStatus::CompileFailed);
    assert!(!trace.compiled_successfully);
    assert!(
        !trace.compile_errors.is_empty(),
        "the trace must carry the compiler diagnostics"
    );
    assert!(trace.snapshots.is_empty());
    let _ = std::fs::remove_file(&broken);
}

#[tokio::test]
async fn sanitizer_variant_fallback_reported() {
    // On systems with only libasan installed (no libubsan) the combined
    // variant fails to link and the engine degrades — the trace must note
    // it instead of failing.
    if !require(&["gcc"]).await {
        return;
    }
    let config = config_for(&fixture("hello.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    let meta = trace.session_meta.expect("meta");
    if meta.sanitized_binary.is_none() {
        let noted = trace
            .analysis_notes
            .iter()
            .any(|n| n.contains("sanitized binary could not be built"));
        assert!(noted, "missing sanitized binary must be explained: {:?}", trace.analysis_notes);
    }
}

// ---------------------------------------------------------------------------
// End-to-end execution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn end_to_end_sample_records_execution() {
    if !require(&["gcc", "gdb"]).await {
        return;
    }
    let config = config_for(&fixture("hello.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    assert!(
        !trace.snapshots.is_empty(),
        "at least the main-breakpoint snapshot is expected"
    );
    // The snapshot chain must reach user code.
    assert!(
        trace.snapshots.iter().any(|s| s.func == "main"),
        "expected a snapshot stopped in main"
    );
}

#[tokio::test]
async fn heap_events_follow_malloc_and_free() {
    if !require(&["gcc", "gdb"]).await {
        return;
    }
    let config = config_for(&fixture("pointer_demo.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    let kinds: Vec<String> = trace
        .heap_events
        .iter()
        .map(|e| format!("{:?}", e.event_type).to_lowercase())
        .collect();
    assert!(kinds.contains(&"allocated".into()), "events: {kinds:?}");
    assert!(kinds.contains(&"freed".into()), "events: {kinds:?}");
    // Heap reconstruction: exactly one block, freed at the end.
    assert_eq!(trace.heap_state.len(), 1);
    assert!(trace.heap_state[0].is_freed);
    // Contents captured while alive must exist (engine reads through GDB).
    assert!(
        trace.heap_state[0].raw_bytes_hex.is_some(),
        "live block contents should have been captured"
    );
}

#[tokio::test]
async fn realloc_chain_recorded() {
    if !require(&["gcc", "gdb"]).await {
        return;
    }
    let config = config_for(&fixture("realloc_demo.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    let kinds: Vec<String> = trace
        .heap_events
        .iter()
        .map(|e| format!("{:?}", e.event_type).to_lowercase())
        .collect();
    assert!(kinds.contains(&"reallocated".into()), "events: {kinds:?}");
}

#[tokio::test]
async fn failed_allocation_recorded() {
    if !require(&["gcc", "gdb"]).await {
        return;
    }
    let config = config_for(&fixture("malloc_fail.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    assert!(
        trace
            .heap_events
            .iter()
            .any(|e| format!("{:?}", e.event_type).to_lowercase() == "allocationfailed"),
        "huge malloc must surface as allocation_failed: {:?}",
        trace.heap_events
    );
}

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

#[tokio::test]
async fn step_limit_bounds_execution() {
    if !require(&["gcc", "gdb"]).await {
        return;
    }
    let config = config_for(&fixture("many_allocs.c"), |c| {
        bounded(c);
        c.limits.max_steps = 50;
    });
    let trace = trace::trace_program(config).await.expect("trace");
    assert_eq!(trace.execution_status, ExecutionStatus::StepLimitReached);
    assert!(
        trace
            .resource_usage
            .limits_triggered
            .iter()
            .any(|l| l == "max_steps"),
        "limits_triggered: {:?}",
        trace.resource_usage.limits_triggered
    );
    // Partial results are preserved.
    assert!(!trace.snapshots.is_empty());
    assert!(
        !trace.heap_events.is_empty(),
        "events captured before the limit must survive"
    );
}

#[tokio::test]
async fn crash_is_reported_with_sanitizer_pass() {
    if !require(&["gcc", "gdb"]).await {
        return;
    }
    let config = config_for(&fixture("null_deref.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    assert_eq!(trace.execution_status, ExecutionStatus::Crashed);
    let crash = trace.crash_report.clone().expect("crash report");
    assert_eq!(crash.signal_name, "SIGSEGV");
    assert!(
        crash.file.as_deref().unwrap_or("").contains("null_deref.c"),
        "crash location: {:?}",
        (crash.file, crash.line)
    );
    let sanitizer = trace.sanitizer_report.expect("sanitizer report");
    // The sanitized binary may be impossible to build on some systems; the
    // reproduction field must reflect reality either way.
    if sanitizer.reproduction != "reproduced" {
        println!("note: sanitizer pass did not reproduce: {}", sanitizer.reproduction);
    }
}

// ---------------------------------------------------------------------------
// Valgrind (independent analysis)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn valgrind_flags_use_after_free() {
    if !require(&["gcc", "valgrind"]).await {
        return;
    }
    let config = config_for(&fixture("use_after_free.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    let report = trace.valgrind_report.expect("valgrind report");
    assert!(
        !report.errors.is_empty() || !report.detailed_errors.is_empty(),
        "use_after_free must produce at least one valgrind error"
    );
}

#[tokio::test]
async fn valgrind_flags_leak() {
    if !require(&["gcc", "valgrind"]).await {
        return;
    }
    let config = config_for(&fixture("leaky.c"), bounded);
    let trace = trace::trace_program(config).await.expect("trace");
    let report = trace.valgrind_report.expect("valgrind report");
    assert!(
        report.definitely_lost_bytes > 0
            || report.possibly_lost_bytes > 0
            || report.still_reachable_bytes > 0
            || !report.errors.is_empty(),
        "leaky.c must surface a leak finding: {:?}",
        (
            report.definitely_lost_bytes,
            report.possibly_lost_bytes,
            report.still_reachable_bytes
        )
    );
}

// ---------------------------------------------------------------------------
// Multi-file projects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multi_file_project_steps_through_helper() {
    if !require(&["gcc", "gdb"]).await {
        return;
    }
    let dir = manifest_dir().join("fixtures").join("c").join("multi_file");
    let mut config = config_for(&dir.join("main.c"), bounded);
    config.build.additional_sources.push(dir.join("helper.c"));
    let trace = trace::trace_program(config).await.expect("trace");
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    let files: Vec<String> = trace
        .snapshots
        .iter()
        .filter_map(|s| s.file.split('/').next_back().map(|f| f.to_string()))
        .collect();
    assert!(
        files.iter().any(|f| f == "helper.c"),
        "stepping must visit helper.c: {files:?}"
    );
}

//! Independent GCC/GDB/OS oracles for a defined-behavior C program.
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use c_trace_engine::models::{ExecutionStatus, HeapEventType};
use c_trace_engine::{trace::trace_program, TraceConfig};

#[tokio::test]
async fn native_stdout_and_gdb_breakpoint_agree_with_runtime_snapshot_and_heap_log() {
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/c/phase8b_defined.c");
    let dir = std::env::temp_dir().join(format!("cte-8b-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let binary = dir.join("native");
    let compiled = Command::new("gcc")
        .args(["-g", "-O0", "-fno-pie", "-no-pie"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let native = Command::new(&binary).output().unwrap();
    assert!(native.status.success());
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "oracle global=41 alias=7 heap2=23 sum=10 result=88\n"
    );
    // GDB's own breakpoint values, independent of the engine's MI session.
    let ground = Command::new("gdb")
        .args([
            "-nx",
            "-q",
            "--batch",
            "-ex",
            "break checkpoint",
            "-ex",
            "run",
            "-ex",
            "p *alias",
            "-ex",
            "p heap[2]",
            "-ex",
            "p sum",
            "-ex",
            "p oracle_global",
            "-ex",
            "up",
            "-ex",
            "p packet.items[1]",
        ])
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        ground.status.success(),
        "{}",
        String::from_utf8_lossy(&ground.stderr)
    );
    let text = String::from_utf8_lossy(&ground.stdout);
    let observations: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once(" = ")?;
            (key.starts_with('$') && key[1..].parse::<usize>().is_ok())
                .then(|| value.trim().parse::<i32>().ok())
                .flatten()
        })
        .collect();
    assert_eq!(observations, [7, 23, 10, 41, 7], "{text}");

    // A separate, independently produced raw wrapper log establishes event
    // kinds and sizes, not process-specific addresses (ASLR differs by run).
    let wrapped = dir.join("wrapped");
    let wrapped_build = Command::new("gcc")
        .args(["-g", "-O0", "-pthread"])
        .arg(&source)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("c_runtime/alloc_tracker.c"))
        .args([
            "-Wl,--wrap=malloc",
            "-Wl,--wrap=calloc",
            "-Wl,--wrap=realloc",
            "-Wl,--wrap=free",
        ])
        .arg("-o")
        .arg(&wrapped)
        .output()
        .unwrap();
    assert!(
        wrapped_build.status.success(),
        "{}",
        String::from_utf8_lossy(&wrapped_build.stderr)
    );
    let raw = Command::new(&wrapped)
        .env("C_TRACE_EVENT_FILE", dir.join("events"))
        .env("C_TRACE_EVENT_STATUS_FILE", dir.join("ack"))
        .output()
        .unwrap();
    assert!(raw.status.success());
    assert_eq!(raw.stdout, native.stdout);
    let rows = std::fs::read_to_string(dir.join("events")).unwrap();
    let parsed: Vec<_> = rows
        .lines()
        .map(c_trace_engine::allocation_tracker::parse_event_line)
        .collect::<Result<_, _>>()
        .unwrap();
    let raw_shape: Vec<_> = parsed
        .iter()
        .map(|e| (format!("{:?}", e.kind), e.size))
        .collect();
    assert_eq!(
        raw_shape,
        [
            ("Malloc".into(), 12),
            ("Realloc".into(), 16),
            ("Free".into(), 16)
        ]
    );
    assert_eq!(parsed.iter().map(|e| e.seq).collect::<Vec<_>>(), [1, 2, 3]);
    let ack = std::fs::read_to_string(dir.join("ack")).unwrap();
    assert_eq!(
        ack.split_whitespace().skip(1).collect::<Vec<_>>(),
        ["3", "0"]
    );

    let mut config = TraceConfig::default();
    config.build.source_file = source;
    config.tracing.discover_globals = false;
    config.tracing.globals = vec!["oracle_global".into()];
    config.limits.timeout_seconds = 40;
    config.limits.max_steps = 2000;
    config.limits.max_caller_frames = 2;
    config.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
    let trace = tokio::time::timeout(Duration::from_secs(45), trace_program(config))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let stop = trace
        .snapshots
        .iter()
        .find(|s| {
            s.func == "checkpoint"
                && s.variables
                    .iter()
                    .any(|v| v.name == "sum" && v.value == "10")
        })
        .expect("checkpoint stop");
    assert!(stop
        .observation
        .globals
        .iter()
        .any(|v| v.name == "oracle_global" && v.value == "41"));
    assert!(stop
        .variables
        .iter()
        .any(|v| v.name == "sum" && v.value == "10"));
    assert!(stop
        .variables
        .iter()
        .any(|v| v.name == "alias" && v.points_to.is_some()));
    assert!(stop
        .caller_frames
        .iter()
        .any(|frame| frame.frame.func == "main"));
    // Raw free is after this stop; terminal replay must not mark it freed.
    assert_eq!(stop.observation.heap_event_count, 2);
    assert_eq!(
        stop.variables
            .iter()
            .find(|v| v.name == "heap")
            .and_then(|v| v.target.as_ref())
            .and_then(|target| target.active),
        Some(true),
        "live pointer at checkpoint despite a later free"
    );
    assert_eq!(
        trace
            .heap_events
            .iter()
            .map(|e| e.event_type.clone())
            .collect::<Vec<_>>(),
        [
            HeapEventType::Allocated,
            HeapEventType::Reallocated,
            HeapEventType::Freed
        ]
    );
    assert_eq!(
        trace
            .heap_events
            .iter()
            .map(|e| e.size_bytes.unwrap())
            .collect::<Vec<_>>(),
        [12, 16, 16]
    );
    assert!(trace
        .recording
        .heap_history
        .as_ref()
        .is_some_and(|h| h.complete));
    let _ = std::fs::remove_dir_all(dir);
}

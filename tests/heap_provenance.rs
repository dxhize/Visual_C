//! Heap provenance checked against real GCC/GDB output and old JSON traces.

use std::path::PathBuf;

use c_trace_engine::allocation_tracker::{parse_event_line, verify_event_status, HeapTimeline};
use c_trace_engine::config::{SanitizerTrigger, TraceMode};
use c_trace_engine::models::{
    DebugSessionTrace, ExecutionStatus, HeapCheckpointInterval, HeapEvent, HeapEventAttribution,
    HeapEventType,
};
use c_trace_engine::trace::trace_program;
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};
use c_trace_engine::TraceConfig;

static RECORDING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

async fn tools() -> Option<tokio::sync::SemaphorePermit<'static>> {
    let permit = RECORDING.acquire().await.ok()?;
    for tool in ["gcc", "gdb", "addr2line"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("SKIPPED heap provenance: {tool} unavailable");
            return None;
        }
    }
    Some(permit)
}

fn config(name: &str) -> TraceConfig {
    let mut config = TraceConfig::default();
    config.build.source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/c")
        .join(name);
    config.tracing.mode = TraceMode::Source;
    config.limits.timeout_seconds = 60;
    config.limits.max_steps = 2_000;
    config.limits.max_snapshot_count = 2_000;
    config.limits.max_source_checkpoints = 2_000;
    config.limits.max_caller_frames = 2;
    config.sanitizer_trigger = SanitizerTrigger::Never;
    config.valgrind_path = Some("/nonexistent/heap-provenance-no-valgrind".into());
    config
}

#[test]
fn terminal_event_status_rejects_missing_damaged_and_unacknowledged_pids() {
    let path = std::env::temp_dir().join(format!(
        "cte-event-status-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut timeline = HeapTimeline::new(8);
    timeline.ingest(&parse_event_line("100 1 malloc 1 1000 8 0 0 401000 ok").unwrap());
    timeline.ingest(&parse_event_line("200 1 malloc 1 2000 8 0 0 401000 ok").unwrap());
    assert!(
        timeline.history_complete(),
        "the event stream alone cannot reveal a failed final write"
    );
    assert!(!verify_event_status(&path, &timeline, Some(100)).is_empty());
    for (input, expected) in [
        ("100 1 0\n", "200"),
        ("100 1 0\n200 1 0", "newline"),
        ("100 1 0\n200 nope 0\n", "invalid"),
        ("100 1 0\n200 1 0\n200 1 0\n", "duplicate"),
        ("100 1 0\n200 1 1\n", "failure"),
        ("100 1 0\n200 2 0\n", "sequence"),
        ("100 1 0\n200 1 0\n300 1 0\n", "unobserved"),
    ] {
        std::fs::write(&path, input).unwrap();
        let errors = verify_event_status(&path, &timeline, Some(100));
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "{input:?}: {errors:?}"
        );
    }
    std::fs::write(&path, "100 2 1\n200 1 0\n").unwrap();
    let errors = verify_event_status(&path, &timeline, Some(100));
    assert!(errors.iter().any(|error| error.contains("writer failure")));
    timeline.mark_transport_incomplete();
    assert!(!timeline.history_complete());
    std::fs::write(&path, "100 1 0\n200 1 0\n").unwrap();
    assert!(verify_event_status(&path, &timeline, Some(100)).is_empty());
    let empty_timeline = HeapTimeline::new(8);
    assert!(!verify_event_status(&path, &empty_timeline, Some(100)).is_empty());
    std::fs::write(&path, "100 0 0\n").unwrap();
    assert!(verify_event_status(&path, &empty_timeline, Some(100)).is_empty());
    std::fs::write(&path, vec![b'X'; 1024 * 1024 + 1]).unwrap();
    assert!(verify_event_status(&path, &empty_timeline, Some(100))
        .iter()
        .any(|error| error.contains("1 MiB")));
    std::fs::remove_file(path).unwrap();
}

async fn run(name: &str) -> DebugSessionTrace {
    let trace = trace_program(config(name)).await.unwrap();
    assert!(trace.compiled_successfully, "{:?}", trace.compile_errors);
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let history = trace.recording.heap_history.as_ref().unwrap();
    assert!(history.complete, "{history:?}");
    let json: DebugSessionTrace =
        serde_json::from_slice(&serde_json::to_vec(&trace).unwrap()).unwrap();
    assert!(json.recording.heap_history.unwrap().complete);
    assert!(TraceStore::from_trace(trace.clone())
        .unwrap()
        .validate_deep()
        .is_structurally_sound());
    trace
}

#[tokio::test]
async fn observed_forks_never_claim_complete_multiprocess_coverage() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("phase8r2_basic.c");
    c.program_args = vec!["forks".into()];
    let trace = trace_program(c).await.unwrap();
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    let history = serde_json::to_value(&trace.recording.heap_history).unwrap();
    assert_eq!(history["process_scope"], "verified_observed_pids_only");
    assert_eq!(history["unobserved_process_coverage"], "unknown");
}

#[tokio::test]
async fn parent_observed_silent_repeated_and_incomplete_children_are_scoped() {
    let Some(_permit) = tools().await else { return };
    for (mode, observed_pids, complete) in [
        ("parent", 1, true),
        ("observed", 2, true),
        ("repeated", 4, true),
        ("silent", 1, true),
        ("incomplete", 2, false),
    ] {
        let mut c = config("phase8dr_fork.c");
        c.program_args = vec![mode.into()];
        let trace = trace_program(c).await.unwrap();
        assert_eq!(trace.execution_status, ExecutionStatus::Completed, "{mode}");
        let coverage = trace.recording.heap_history.as_ref().unwrap();
        assert_eq!(
            coverage.process_scope.as_deref(),
            Some("verified_observed_pids_only")
        );
        assert_eq!(
            coverage.unobserved_process_coverage.as_deref(),
            Some("unknown")
        );
        assert_eq!(
            coverage.observed_event_pids.len(),
            observed_pids,
            "{mode}: {coverage:?}"
        );
        assert_eq!(coverage.complete, complete, "{mode}: {coverage:?}");
        let followed = coverage.followed_inferior_pid.unwrap();
        assert!(coverage.observed_event_pids.contains(&followed));
        assert!(
            trace
                .heap_events
                .iter()
                .filter(|e| e.pid == followed)
                .count()
                >= 4
        );
        assert!(
            trace
                .snapshots
                .iter()
                .all(|s| s.observation.pid == Some(followed)),
            "GDB snapshots belong to the followed inferior only: {mode}"
        );
        for pid in &coverage.observed_event_pids {
            let events: Vec<_> = trace.heap_events.iter().filter(|e| e.pid == *pid).collect();
            assert!(!events.is_empty());
            assert_eq!(events[0].sequence, 1, "{mode} pid {pid}");
            assert!(events.iter().all(|e| e.pid == *pid));
        }
        // A silent fork is intentionally indistinguishable from parent-only
        // logs; neither case may claim complete *multiprocess* coverage.
        if mode == "silent" {
            assert_eq!(coverage.observed_event_pids, vec![followed]);
        }
        let loaded: DebugSessionTrace =
            serde_json::from_slice(&serde_json::to_vec(&trace).unwrap()).unwrap();
        assert_eq!(
            loaded
                .recording
                .heap_history
                .unwrap()
                .observed_event_pids
                .len(),
            observed_pids
        );
    }
}

fn line_of(source: &str, needle: &str) -> u32 {
    let matches: Vec<_> = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .collect();
    assert_eq!(matches.len(), 1, "ambiguous fixture expression: {needle}");
    (matches[0].0 + 1) as u32
}

fn at_source(trace: &DebugSessionTrace, line: u32) -> &HeapEvent {
    let matches: Vec<_> = trace
        .heap_events
        .iter()
        .filter(|event| event.source_line == Some(line))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "heap events for fixture line {line}: {matches:?}"
    );
    matches[0]
}

fn assert_callsite(trace: &DebugSessionTrace, event: &HeapEvent, file: &str) {
    assert!(
        event
            .source_file
            .as_deref()
            .is_some_and(|path| path.ends_with(file)),
        "{event:?}"
    );
    assert!(event
        .raw_caller_address
        .as_deref()
        .is_some_and(|pc| pc.starts_with("0x")));
    let attribution = event
        .attribution
        .as_ref()
        .expect("explicit heap attribution");
    assert_eq!(
        attribution.call_site_resolution,
        "resolved_return_address_minus_one"
    );
    assert_eq!(attribution.call_site_function.as_deref(), Some("main"));
    assert!(attribution.history_reliable);
    if let Some(interval) = &attribution.checkpoint_interval {
        if let Some(index) = interval.before_checkpoint {
            assert_eq!(
                interval.before_stop,
                Some(trace.snapshots[index].observation.stop_id)
            );
        }
        if let Some(index) = interval.after_checkpoint {
            assert_eq!(
                interval.after_stop,
                Some(trace.snapshots[index].observation.stop_id)
            );
        }
        if let (Some(before), Some(after)) = (interval.before_stop, interval.after_stop) {
            assert!(
                before < after,
                "event is interval evidence, not exact stop evidence"
            );
        }
    }
}

#[tokio::test]
async fn allocation_reuse_caller_stale_pointers_and_heap_bytes() {
    let Some(_permit) = tools().await else { return };
    let trace = run("heap_provenance.c").await;
    let source = include_str!("../fixtures/c/heap_provenance.c");
    for event in &trace.heap_events {
        assert_callsite(&trace, event, "heap_provenance.c");
    }
    let first = at_source(&trace, line_of(source, "first = malloc(32);"));
    let freed = at_source(&trace, line_of(source, "free(first);"));
    let second = at_source(&trace, line_of(source, "second = malloc(32);"));
    let calloc = at_source(&trace, line_of(source, "zeros = calloc(8, 4);"));
    let free_null = at_source(&trace, line_of(source, "free(null_pointer);"));
    assert_eq!(first.event_type, HeapEventType::Allocated);
    assert_eq!(calloc.event_type, HeapEventType::Allocated);
    assert_eq!(calloc.size_bytes, Some(32));
    assert_eq!(calloc.calloc_count, Some(8));
    assert_eq!(calloc.calloc_element_size, Some(4));
    assert_eq!(freed.event_type, HeapEventType::Freed);
    assert_eq!(freed.allocation_id, first.allocation_id);
    assert_eq!(
        second.address, first.address,
        "fixture must observe actual glibc reuse"
    );
    assert_ne!(second.allocation_id, first.allocation_id);
    assert_eq!(free_null.event_type, HeapEventType::Freed);
    assert_eq!(free_null.allocation_id, 0);
    assert!(free_null
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("free(NULL)")));

    let mut live_first = false;
    let mut freed_stale = false;
    let mut reused_stale = false;
    let mut caller_pointer = false;
    let mut calloc_zero = false;
    let mut calloc_updated = false;
    for snapshot in &trace.snapshots {
        for block in &snapshot.heap_blocks {
            assert_eq!(block.observed_at_stop, Some(snapshot.observation.stop_id));
            assert_eq!(block.lifetime_status.as_deref(), Some("observed_active"));
            if block.allocation_id == first.allocation_id {
                live_first |= block
                    .raw_bytes_hex
                    .as_deref()
                    .is_some_and(|hex| hex.starts_with("5a"));
            }
            if block.allocation_id == calloc.allocation_id {
                calloc_zero |= block
                    .raw_bytes_hex
                    .as_deref()
                    .is_some_and(|hex| hex.starts_with("00000000"));
                calloc_updated |= block
                    .raw_bytes_hex
                    .as_deref()
                    .is_some_and(|hex| hex.starts_with("71"));
            }
        }
        for variable in snapshot.variables.iter().chain(
            snapshot
                .caller_frames
                .iter()
                .flat_map(|frame| frame.variables.iter()),
        ) {
            if variable.name == "stale" && variable.points_to == first.address {
                if let Some(target) = &variable.target {
                    if snapshot.observation.heap_event_count >= 2
                        && snapshot.observation.heap_event_count < 3
                    {
                        freed_stale |= target.kind == "heap" && target.active == Some(false);
                    }
                    if snapshot.observation.heap_event_count >= 3 {
                        reused_stale |= target.address_match_allocation_id
                            == Some(second.allocation_id)
                            && target.allocation_id.is_none()
                            && target.provenance_status.as_deref() == Some("ambiguous_reuse");
                    }
                }
            }
            if variable.name == "candidate" && variable.points_to == second.address {
                caller_pointer |= snapshot.caller_frames.iter().any(|frame| {
                    frame.frame.func == "main"
                        && frame
                            .variables
                            .iter()
                            .any(|v| v.name == "second" && v.points_to == second.address)
                });
            }
        }
    }
    assert!(
        live_first && calloc_zero && calloc_updated,
        "live heap bytes must refresh"
    );
    assert!(
        freed_stale,
        "stale pointer remains distinguishable before reuse"
    );
    assert!(
        reused_stale,
        "numeric reuse must not prove ownership of stale pointer"
    );
    assert!(
        caller_pointer,
        "callee and caller pointer observations must coexist"
    );
}

#[tokio::test]
async fn realloc_same_move_failure_zero_and_source_sites() {
    let Some(_permit) = tools().await else { return };
    let trace = run("heap_provenance_realloc.c").await;
    let source = include_str!("../fixtures/c/heap_provenance_realloc.c");
    for event in &trace.heap_events {
        assert_callsite(&trace, event, "heap_provenance_realloc.c");
    }
    let initial = at_source(&trace, line_of(source, "block = malloc(256);"));
    let same = at_source(&trace, line_of(source, "same = realloc(block, 128);"));
    let failed = at_source(
        &trace,
        line_of(source, "failed = realloc(same, impossible);"),
    );
    let moved = at_source(
        &trace,
        line_of(source, "moved = realloc(same, 1024 * 1024);"),
    );
    let zero = at_source(&trace, line_of(source, "zero_result = realloc(zeroed, 0);"));
    let from_null = at_source(
        &trace,
        line_of(source, "fresh = realloc(null_pointer, 24);"),
    );
    let malloc_zero = at_source(&trace, line_of(source, "zero_size = malloc(0);"));
    let failed_malloc = at_source(
        &trace,
        line_of(source, "failed_malloc = malloc(impossible);"),
    );
    let overflow = at_source(&trace, line_of(source, "overflow = calloc(impossible, 2);"));
    assert_eq!(same.event_type, HeapEventType::Reallocated);
    assert_eq!(same.address, initial.address);
    assert_ne!(same.allocation_id, initial.allocation_id);
    assert_eq!(same.old_allocation_id, Some(initial.allocation_id));
    assert_eq!(same.size_bytes, Some(128));
    assert_eq!(failed.event_type, HeapEventType::AllocationFailed);
    assert_eq!(failed.old_allocation_id, Some(same.allocation_id));
    assert_eq!(failed.heap_usage_after, same.heap_usage_after);
    assert_eq!(moved.event_type, HeapEventType::Reallocated);
    assert_eq!(moved.old_allocation_id, Some(same.allocation_id));
    assert_ne!(moved.allocation_id, same.allocation_id);
    assert_ne!(moved.address, same.address);
    assert_eq!(moved.size_bytes, Some(1024 * 1024));
    let old = trace
        .heap_state
        .iter()
        .find(|block| block.allocation_id == initial.allocation_id)
        .unwrap();
    let new = trace
        .heap_state
        .iter()
        .find(|block| block.allocation_id == moved.allocation_id)
        .unwrap();
    let middle = trace
        .heap_state
        .iter()
        .find(|block| block.allocation_id == same.allocation_id)
        .unwrap();
    assert!(old.is_freed && new.is_freed);
    assert!(middle.is_freed);
    assert_eq!(middle.realloc_from_id, Some(initial.allocation_id));
    assert_eq!(new.realloc_from_id, Some(same.allocation_id));
    assert!(trace
        .snapshots
        .iter()
        .flat_map(|s| &s.heap_blocks)
        .any(|block| {
            block.allocation_id == moved.allocation_id
                && block
                    .raw_bytes_hex
                    .as_deref()
                    .is_some_and(|hex| hex.starts_with("4b"))
        }));
    assert_eq!(zero.event_type, HeapEventType::Reallocated);
    assert!(zero
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("glibc freed")));
    assert_eq!(zero.address, None);
    assert_eq!(from_null.event_type, HeapEventType::Reallocated);
    assert_eq!(from_null.old_allocation_id, None);
    assert_eq!(from_null.size_bytes, Some(24));
    assert!(matches!(
        malloc_zero.event_type,
        HeapEventType::Allocated | HeapEventType::AllocationFailed
    ));
    assert_eq!(malloc_zero.size_bytes, Some(0));
    assert_eq!(failed_malloc.event_type, HeapEventType::AllocationFailed);
    assert_eq!(failed_malloc.address, None);
    assert_eq!(overflow.event_type, HeapEventType::AllocationFailed);
    assert_eq!(overflow.size_bytes, None);
    assert_eq!(overflow.calloc_count, Some(usize::MAX as u64));
    assert_eq!(overflow.calloc_element_size, Some(2));
    assert!(overflow
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("overflow")));
}

#[test]
fn old_json_and_partial_history_preserve_borrowed_interval_evidence() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/v21_heap.json");
    let original = std::fs::read(path).unwrap();
    let store = TraceStore::from_json_bytes(&original, TraceLoadOptions::default()).unwrap();
    assert!(store
        .trace()
        .heap_events
        .iter()
        .all(|event| event.attribution.is_none()));
    assert!(store
        .trace()
        .heap_events
        .iter()
        .all(|event| event.calloc_count.is_none() && event.heap_usage_complete.is_none()));
    assert!(store
        .trace()
        .heap_state
        .iter()
        .all(|block| block.lifetime_status.is_none()));
    assert_eq!(
        serde_json::to_value(store.trace()).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&original).unwrap()
    );

    // Mutate only a real trace's counters: both events now belong to one
    // retained boundary interval, not to an invented exact event stop.
    let mut trace = store.trace().clone();
    let last = trace.snapshots.len() - 1;
    for snapshot in &mut trace.snapshots[..last] {
        snapshot.observation.heap_event_count = 0;
    }
    trace.snapshots[last].observation.heap_event_count = trace.heap_events.len();
    let boundary = trace.snapshots[last].observation.stop_id;
    let first_stop = trace.snapshots[0].observation.stop_id;
    trace.heap_events[0].attribution = Some(HeapEventAttribution {
        call_site_resolution: "resolved_return_address_minus_one".into(),
        call_site_function: Some("main".into()),
        checkpoint_interval: Some(HeapCheckpointInterval {
            before_checkpoint: Some(0),
            after_checkpoint: Some(last),
            before_stop: Some(first_stop),
            after_stop: Some(boundary),
        }),
        history_reliable: true,
    });
    trace.heap_events[1].old_allocation_id = Some(u64::MAX - 1);
    trace.heap_events[1].attribution = Some(HeapEventAttribution {
        call_site_resolution: "unavailable".into(),
        call_site_function: None,
        checkpoint_interval: None,
        history_reliable: false,
    });
    let bytes = serde_json::to_vec(&trace).unwrap();
    let store = TraceStore::from_json_bytes(&bytes, TraceLoadOptions::default()).unwrap();
    let events = store
        .semantic_input()
        .checkpoint_context(last)
        .unwrap()
        .heap_events_in_snapshot_interval()
        .unwrap();
    assert_eq!(events.len(), 2);
    assert!(std::ptr::eq(
        events.as_ptr(),
        store.trace().heap_events.as_ptr()
    ));
    assert_eq!(store.heap_events_at_stop(boundary).unwrap().len(), 2);
    assert!(
        store.validate_deep().is_structurally_sound(),
        "missing history is unavailable, not corrupt"
    );
    assert!(store
        .validate_deep()
        .findings()
        .iter()
        .any(|finding| finding.code == "REALLOC_ORIGIN_UNRESOLVED"));
    let coarse = store.trace().heap_events[0].attribution.as_ref().unwrap();
    let interval = coarse.checkpoint_interval.as_ref().unwrap();
    assert_eq!(
        (interval.before_stop, interval.after_stop),
        (Some(first_stop), Some(boundary))
    );
    assert!(
        boundary > first_stop + 1,
        "skipped GDB stops cannot be assigned exactly"
    );
    let partial = store.trace().heap_events[1].attribution.as_ref().unwrap();
    assert!(!partial.history_reliable);
    assert!(partial.checkpoint_interval.is_none());
}

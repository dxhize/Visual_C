//! TP8-F01: checked-in engine V2.2 realloc trace and independent live engine
//! oracles; no untrusted import is rewritten by the parser.
use c_trace_engine::models::{HeapEventType, HeapHistoryCoverage};
use c_trace_engine::trace_parser::EvidenceLocation;
use c_trace_engine::trace_parser::{EvidenceConfidence as C, TraceLoadOptions, TraceStore};

fn fixture() -> c_trace_engine::models::DebugSessionTrace {
    TraceStore::open(
        format!(
            "{}/tests/fixtures/stage1/realloc_move.json",
            env!("CARGO_MANIFEST_DIR")
        ),
        TraceLoadOptions::default(),
    )
    .unwrap()
    .trace()
    .clone()
}
fn scoped(
    mut trace: c_trace_engine::models::DebugSessionTrace,
) -> c_trace_engine::models::DebugSessionTrace {
    let pid = trace.heap_events[0].pid;
    trace.recording.heap_history = Some(HeapHistoryCoverage {
        complete: true,
        process_scope: Some("verified_observed_pids_only".into()),
        unobserved_process_coverage: Some("unknown".into()),
        followed_inferior_pid: Some(pid),
        observed_event_pids: vec![pid],
        ..Default::default()
    });
    trace
}
#[test]
fn original_fabricated_existing_predecessor_must_never_be_definite() {
    let trace = scoped(fixture());
    assert_eq!(
        trace.heap_events[0].event_type,
        c_trace_engine::models::HeapEventType::Allocated
    );
    assert_eq!(trace.heap_events[0].old_allocation_id, None);
    assert_eq!(
        trace.heap_events[2].event_type,
        c_trace_engine::models::HeapEventType::Reallocated
    );
    assert_eq!(trace.heap_events[2].old_allocation_id, Some(1));
    let mut forged = trace.clone();
    forged.heap_state[0].realloc_from_id = Some(2);
    let store = TraceStore::from_trace(forged).unwrap(); // schema-only gate
    let view = store.validated_semantic();
    assert!(
        !view.report().is_structurally_sound(),
        "{:?}",
        view.report()
    );
    assert_eq!(view.heap(1).old_id, Some(2)); // raw evidence, never rewritten
    assert_eq!(view.heap(1).realloc_confidence, C::Contradictory);
    let healthy = TraceStore::from_trace(trace).unwrap();
    assert!(healthy.validate_deep().is_structurally_sound());
    assert_eq!(healthy.validated_semantic().heap(3).old_id, Some(1));
    assert_eq!(
        healthy.validated_semantic().heap(3).realloc_confidence,
        C::Definite
    );
}

#[test]
fn self_cycle_wrong_pid_sequence_and_event_type_are_not_accepted() {
    let source = scoped(fixture());
    for (name, modify) in [
        ("self", 0_u8),
        ("cycle", 1),
        ("wrong-pid", 2),
        ("wrong-order", 3),
        ("wrong-predecessor", 4),
        ("three-node-cycle", 5),
        ("vector-order", 6),
        ("event-chain-cycle", 7),
    ] {
        let mut trace = source.clone();
        match modify {
            0 => trace.heap_state[2].realloc_from_id = Some(3),
            1 => trace.heap_state[0].realloc_from_id = Some(3), // 1 ← 3 ← 1
            2 => trace.heap_events[2].pid += 1,
            3 => trace.heap_events[2].sequence = 1,
            4 => trace.heap_events[2].old_allocation_id = Some(2),
            5 => {
                trace.heap_state[0].realloc_from_id = Some(3);
                trace.heap_state[1].realloc_from_id = Some(1);
                trace.heap_state[2].realloc_from_id = Some(2);
            }
            6 => trace.heap_events.swap(0, 2), // raw vector contradicts PID-local sequence
            7 => {
                trace.heap_events[0].event_type = HeapEventType::Reallocated;
                trace.heap_events[0].old_allocation_id = Some(3);
            } // event 1 claims to follow event 3
            _ => unreachable!(),
        }
        let store = TraceStore::from_trace(trace).unwrap();
        let view = store.validated_semantic();
        assert!(
            !view.report().is_structurally_sound(),
            "{name}: {:?}",
            view.report()
        );
        if matches!(modify, 0 | 1 | 5) {
            assert!(
                view.report()
                    .findings()
                    .iter()
                    .any(|f| f.code == "REALLOC_LINK_CYCLE"),
                "{name}: expected actual cycle finding"
            );
        }
        if modify == 6 {
            assert!(
                view.report()
                    .findings()
                    .iter()
                    .any(|f| f.code == "HEAP_EVENT_SEQUENCE_ORDER"),
                "{name}: expected vector-order finding"
            );
        }
        if modify == 7 {
            assert!(
                view.report()
                    .findings()
                    .iter()
                    .any(|f| f.code == "REALLOC_EVENT_ORIGIN_CONTRADICTORY"),
                "{name}: expected raw event direction finding"
            );
        }
        let id = if modify == 1 || modify == 5 { 1 } else { 3 };
        assert_ne!(view.heap(id).realloc_confidence, C::Definite, "{name}");
    }
}

#[test]
fn missing_failed_incomplete_and_legacy_are_not_definite() {
    let trace = scoped(fixture());
    let mut missing = trace.clone();
    missing.heap_state[2].realloc_from_id = Some(999_999);
    missing.heap_events[2].old_allocation_id = Some(999_999);
    let store = TraceStore::from_trace(missing).unwrap();
    assert_eq!(
        store.validated_semantic().heap(3).realloc_confidence,
        C::Unavailable
    );
    assert!(store
        .validate_deep()
        .findings()
        .iter()
        .any(|f| f.code == "REALLOC_ORIGIN_UNRESOLVED"));

    let mut missing_creation = trace.clone();
    missing_creation.heap_events.remove(0); // old lifetime metadata exists, old event does not
    assert_ne!(
        TraceStore::from_trace(missing_creation)
            .unwrap()
            .validated_semantic()
            .heap(3)
            .realloc_confidence,
        C::Definite
    );

    let mut failed = trace.clone();
    failed.heap_events[2].event_type = HeapEventType::AllocationFailed;
    assert_ne!(
        TraceStore::from_trace(failed)
            .unwrap()
            .validated_semantic()
            .heap(3)
            .realloc_confidence,
        C::Definite
    );

    let mut partial_bad = trace.clone();
    partial_bad
        .recording
        .heap_history
        .as_mut()
        .unwrap()
        .complete = false;
    partial_bad.heap_state[0].realloc_from_id = Some(2);
    let partial_bad = TraceStore::from_trace(partial_bad).unwrap();
    assert!(!partial_bad.validate_deep().is_structurally_sound());
    assert_eq!(
        partial_bad.validated_semantic().heap(1).realloc_confidence,
        C::Contradictory
    );

    let mut partial = trace;
    partial.recording.heap_history.as_mut().unwrap().complete = false;
    let partial = TraceStore::from_trace(partial).unwrap();
    assert_eq!(
        partial.validated_semantic().heap(3).realloc_confidence,
        C::Candidate
    );
    assert!(partial.validate_deep().is_partial());
    let legacy = TraceStore::from_trace(fixture()).unwrap();
    assert!(legacy.validate_deep().is_structurally_sound());
    assert_eq!(
        legacy.validated_semantic().heap(3).realloc_confidence,
        C::Candidate
    );
}

#[test]
fn tp7_interval_and_heap_references_inherit_only_verified_confidence() {
    let store = TraceStore::from_trace(scoped(fixture())).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let reference = store.evidence_ref(EvidenceLocation::HeapEvent(2)).unwrap();
    assert_eq!(
        view.heap_reference(&reference).unwrap().realloc_confidence,
        C::Definite
    );
    let mut found = false;
    for index in 0..store.trace().snapshots.len() {
        let summary = view.checkpoint_summary(index).unwrap();
        let len = summary
            .checkpoint
            .context
            .heap_events_in_snapshot_interval()
            .unwrap()
            .len();
        for item in summary.heap_interval_page(0..len).unwrap() {
            let item = item.unwrap();
            if item.position == 2 {
                assert_eq!(
                    item.allocation(&view).unwrap().realloc_confidence,
                    C::Definite
                );
                found = true;
            }
        }
    }
    assert!(
        found,
        "the recorded realloc must be in a retained snapshot interval"
    );
}

#[tokio::test]
async fn fresh_engine_in_place_and_moved_realloc_are_correlated_by_events_not_addresses() {
    use c_trace_engine::{trace::trace_program, TraceConfig};
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped live F01: {tool} unavailable");
            return;
        }
    }
    for name in [
        "phase8b_defined.c",
        "realloc_move.c",
        "heap_provenance.c",
        "heap_provenance_realloc.c",
    ] {
        let mut config = TraceConfig::default();
        config.build.source_file =
            format!("{}/fixtures/c/{name}", env!("CARGO_MANIFEST_DIR")).into();
        config.limits.max_steps = 400;
        config.limits.timeout_seconds = 40;
        config.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
        let store = TraceStore::from_trace(trace_program(config).await.unwrap()).unwrap();
        let view = store.validated_semantic();
        assert!(
            view.report().is_structurally_sound(),
            "{name}: {:?}",
            view.report()
        );
        let mut count = 0;
        for block in store
            .heap_state()
            .iter()
            .filter(|b| b.realloc_from_id.is_some())
        {
            let event = store
                .heap_events()
                .iter()
                .find(|e| {
                    e.event_type == HeapEventType::Reallocated
                        && e.allocation_id == block.allocation_id
                })
                .expect("engine realloc event");
            assert_eq!(event.old_allocation_id, block.realloc_from_id);
            let heap = view.heap(block.allocation_id);
            assert_eq!(
                heap.realloc_confidence,
                C::Definite,
                "{name}: event={event:?}"
            );
            let old = store
                .heap_events()
                .iter()
                .find(|e| {
                    e.allocation_id == block.realloc_from_id.unwrap()
                        && matches!(
                            e.event_type,
                            HeapEventType::Allocated | HeapEventType::Reallocated
                        )
                        && e.sequence == heap.old_lifetime.unwrap().allocation_sequence
                })
                .expect("old lifetime's real creation event (malloc or realloc)");
            eprintln!(
                "{name}: old_address={:?}, new_address={:?}, confidence={:?}",
                old.address, event.address, heap.realloc_confidence
            );
            count += 1;
        }
        if name == "heap_provenance.c" {
            // Observed address reuse remains a different ID, never an inferred
            // predecessor for a malloc (nor proof that a stale pointer owns it).
            for (i, first) in store.heap_events().iter().enumerate() {
                for later in store.heap_events().iter().skip(i + 1) {
                    if first.address.is_some()
                        && first.address == later.address
                        && first.allocation_id != later.allocation_id
                        && later.event_type == HeapEventType::Allocated
                    {
                        assert_eq!(
                            view.heap(later.allocation_id).realloc_confidence,
                            C::Unavailable
                        );
                    }
                }
            }
        } else {
            assert!(count > 0, "{name}: expected successful realloc");
            if name == "heap_provenance_realloc.c" {
                let terminal: Vec<_> = store
                    .heap_events()
                    .iter()
                    .filter(|e| {
                        e.event_type == HeapEventType::Reallocated
                            && e.old_allocation_id == Some(e.allocation_id)
                            && e.address.is_none()
                            && e.size_bytes == Some(0)
                    })
                    .collect();
                assert!(
                    !terminal.is_empty(),
                    "expected the engine's realloc(ptr, 0) terminal-free event"
                );
                for event in terminal {
                    assert!(
                        view.heap(event.allocation_id).old_id.is_none(),
                        "terminal-free is not a new predecessor link"
                    );
                }
            }
        }
    }
}

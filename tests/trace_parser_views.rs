//! TP-3: borrowed dual-consumer access over real V2.1 Stage 1 evidence.
use std::path::PathBuf;
use std::sync::Arc;

use c_trace_engine::models::{
    DataValidity, DebugSessionTrace, ExecutionSnapshot, ExecutionStatus, FrameVariables,
    MemoryEvidence, PointerTarget, ScopeStatus, SnapshotObservation, StackFrameInfo, VariableInfo,
};
use c_trace_engine::trace_parser::{
    Evidence, EvidenceLocation as L, QueryError, TraceLoadOptions, TraceStore,
};

fn load(name: &str) -> TraceStore {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name);
    TraceStore::open(path, TraceLoadOptions::default()).unwrap()
}

fn caller_trace() -> DebugSessionTrace {
    let frame = StackFrameInfo {
        level: 1,
        func: "main".into(),
        address: Some("0x4000".into()),
        ..Default::default()
    };
    let mut trace = DebugSessionTrace::default();
    trace.recording.source_checkpoints = vec![0];
    trace.snapshots.push(ExecutionSnapshot {
        observation: SnapshotObservation {
            stop_id: 1,
            ..Default::default()
        },
        call_stack: vec![StackFrameInfo::default(), frame.clone()],
        variables: vec![VariableInfo {
            name: "local".into(),
            activation_id: Some(2),
            identity: Some("activation:2:local".into()),
            ..Default::default()
        }],
        caller_frames: vec![FrameVariables {
            frame,
            activation_id: 1,
            variables: vec![VariableInfo {
                name: "caller".into(),
                activation_id: Some(1),
                identity: Some("activation:1:caller".into()),
                read_status: DataValidity::Valid,
                raw_bytes_hex: Some("01000000".into()),
                memory_evidence: Some(MemoryEvidence {
                    extent_bytes: Some(4),
                    requested_bytes: 4,
                    captured_bytes: 4,
                    status: "full".into(),
                    reason: None,
                    limit: None,
                }),
                target: Some(PointerTarget {
                    variable_identity: Some("activation:2:local".into()),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    trace
}

#[test]
fn phase4_caller_access_and_evidence_borrow_canonical_records() {
    let bytes = serde_json::to_vec(&caller_trace()).unwrap();
    let store = TraceStore::from_json_bytes(&bytes, TraceLoadOptions::default()).unwrap();
    let clone = store.clone();
    let context = store.semantic_input().checkpoint_context(0).unwrap();
    let snapshot = &store.trace().snapshots[0];
    assert_eq!(context.variables().len(), 1, "frame 0 remains separate");
    assert_eq!(
        context.caller_frames().as_ptr(),
        snapshot.caller_frames.as_ptr()
    );
    assert_eq!(
        store.raw().caller_frames(0).unwrap().as_ptr(),
        snapshot.caller_frames.as_ptr()
    );
    let frame = context.caller_frame_evidence(0).unwrap();
    let var = context.caller_variable_evidence(0, 0).unwrap();
    match clone.resolve_evidence(&frame).unwrap() {
        Evidence::SnapshotCallerFrame(caller) => {
            assert!(std::ptr::eq(caller, &snapshot.caller_frames[0]))
        }
        other => panic!("wrong evidence: {other:?}"),
    }
    match context.snapshot().caller_frames[0].variables[0]
        .raw_bytes_hex
        .as_deref()
    {
        Some("01000000") => {}
        other => panic!("lost recorded bytes: {other:?}"),
    }
    match store.resolve_evidence(&var).unwrap() {
        Evidence::SnapshotCallerVariable(info) => {
            assert!(std::ptr::eq(info, &snapshot.caller_frames[0].variables[0]))
        }
        other => panic!("wrong evidence: {other:?}"),
    }
    assert!(matches!(
        context.caller_frame_evidence(1),
        Err(QueryError::InvalidReference { .. })
    ));
    assert!(matches!(
        context.caller_variable_evidence(0, 1),
        Err(QueryError::InvalidReference { .. })
    ));
    assert!(matches!(
        context.caller_variable_evidence(1, 0),
        Err(QueryError::InvalidReference { .. })
    ));
    let independent = TraceStore::from_trace(caller_trace()).unwrap();
    assert!(matches!(
        independent.resolve_evidence(&var),
        Err(QueryError::ForeignTrace)
    ));
    assert!(store.raw().caller_frames(1).is_err());
    let old = load("v21_dual.json");
    assert!(old
        .semantic_input()
        .checkpoint_context(0)
        .unwrap()
        .caller_frames()
        .is_empty());
    assert!(old.raw().caller_frames(0).unwrap().is_empty());
}

#[test]
fn phase4_deep_validation_checks_caller_associations_and_activations() {
    let trace = caller_trace();
    assert!(TraceStore::from_trace(trace.clone())
        .unwrap()
        .validate_deep()
        .is_structurally_sound());
    for (case, code) in [
        "CALLER_FRAME_ASSOCIATION_INVALID",
        "CALLER_FRAME_ASSOCIATION_INVALID",
        "CALLER_ACTIVATION_INVALID",
        "CALLER_VARIABLE_ACTIVATION_MISMATCH",
        "CALLER_IDENTITY_ACTIVATION_MISMATCH",
        "CALLER_MEMORY_EVIDENCE_INVALID",
        "CALLER_ACTIVATION_INVALID",
        "CALLER_FRAME_ASSOCIATION_INVALID",
    ]
    .into_iter()
    .enumerate()
    {
        let mut trace = trace.clone();
        let snapshot = &mut trace.snapshots[0];
        let caller = &mut snapshot.caller_frames[0];
        match case {
            0 => caller.frame.level = 0,
            1 => caller.frame.address = Some("0x9999".into()),
            2 => caller.activation_id = 0,
            3 => caller.variables[0].activation_id = Some(9),
            4 => caller.variables[0].identity = Some("activation:9:caller".into()),
            5 => {
                caller.variables[0]
                    .memory_evidence
                    .as_mut()
                    .unwrap()
                    .captured_bytes = 8
            }
            6 => caller.activation_id = 2,
            _ => {
                let duplicate = caller.clone();
                snapshot.caller_frames.push(duplicate);
            }
        }
        let report = TraceStore::from_trace(trace).unwrap().validate_deep();
        assert!(!report.is_structurally_sound(), "case {case}: {report:?}");
        assert!(
            report.findings().iter().any(|finding| finding.code == code),
            "case {case}: {report:?}"
        );
    }
}

#[test]
fn phase4_partial_callers_and_missing_references_are_unavailable_not_corrupt() {
    let mut trace = caller_trace();
    let caller = &mut trace.snapshots[0].caller_frames[0];
    caller.errors.push("memory budget exhausted".into());
    caller.variables[0].activation_id = None;
    caller.variables[0].read_status = DataValidity::Unavailable;
    caller.variables[0]
        .target
        .as_mut()
        .unwrap()
        .variable_identity = Some("activation:99:dead".into());
    let report = TraceStore::from_trace(trace).unwrap().validate_deep();
    assert!(report.is_structurally_sound(), "{report:?}");
    for code in [
        "CALLER_CAPTURE_INCOMPLETE",
        "CALLER_VARIABLE_ACTIVATION_UNRECORDED",
        "CALLER_VARIABLE_OBSERVATION_UNAVAILABLE",
        "POINTER_VARIABLE_REFERENCE_UNRESOLVED",
    ] {
        assert!(
            report.findings().iter().any(|finding| finding.code == code
                && finding.severity
                    == c_trace_engine::trace_parser::ValidationSeverity::UnavailableEvidence),
            "{report:?}"
        );
    }
}

#[test]
fn phase4_pointer_validation_checks_cross_frame_identity_extent_and_offset() {
    let mut trace = caller_trace();
    let snapshot = &mut trace.snapshots[0];
    snapshot.caller_frames[0].variables[0].address = Some("0x1000".into());
    snapshot.caller_frames[0].variables[0].size_bytes = Some(4);
    snapshot.variables[0].points_to = Some("0x1002".into());
    snapshot.variables[0].target = Some(PointerTarget {
        kind: "stack".into(),
        variable_identity: Some("activation:1:caller".into()),
        base_address: Some("0x1000".into()),
        offset_bytes: Some(2),
        ..Default::default()
    });
    assert!(TraceStore::from_trace(trace.clone())
        .unwrap()
        .validate_deep()
        .is_structurally_sound());
    for (case, code) in [
        "POINTER_VARIABLE_OFFSET_MISMATCH",
        "POINTER_VARIABLE_EXTENT_MISMATCH",
        "POINTER_VARIABLE_REFERENCE_INACTIVE",
        "POINTER_VARIABLE_REFERENCE_INVALID",
    ]
    .into_iter()
    .enumerate()
    {
        let mut trace = trace.clone();
        let snapshot = &mut trace.snapshots[0];
        match case {
            0 => snapshot.variables[0].target.as_mut().unwrap().offset_bytes = Some(9),
            1 => snapshot.variables[0].points_to = Some("0x2000".into()),
            2 => snapshot.caller_frames[0].variables[0].scope_status = Some(ScopeStatus::Inactive),
            _ => snapshot.variables[0].target.as_mut().unwrap().allocation_id = Some(9),
        }
        let report = TraceStore::from_trace(trace).unwrap().validate_deep();
        let finding = report
            .findings()
            .iter()
            .find(|finding| finding.code == code)
            .unwrap();
        assert_eq!(finding.location.to_string(), "snapshots[0].variables[0]");
        assert!(!report.is_structurally_sound(), "{report:?}");
    }
}

#[test]
fn a_views_are_borrowed_and_share_both_arcs_across_clones() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<TraceStore>();
    let store = load("v21_dual.json");
    let clone = store.clone();
    let trace = store.trace() as *const _;
    let indexes = store.indexes() as *const _;
    for view in [store.raw(), store.semantic_input().raw(), clone.raw()] {
        assert_eq!(view.trace() as *const _, trace);
        assert_eq!(view.indexes() as *const _, indexes);
    }
    for view in [store.semantic_input(), clone.semantic_input()] {
        assert_eq!(view.store().trace() as *const _, trace);
        assert_eq!(view.store().indexes() as *const _, indexes);
    }
    assert!(Arc::ptr_eq(&store.shared(), &clone.shared()));
    assert_eq!(
        std::mem::size_of_val(&store.raw()),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of_val(&store.semantic_input()),
        std::mem::size_of::<usize>()
    );
}

#[test]
fn b_raw_view_reuses_tp2_ordering_and_preserves_evidence() {
    let store = load("v21_dual.json");
    let raw = store.raw();
    assert_eq!(raw.source_checkpoints(), store.source_checkpoints());
    assert_eq!(raw.execution_status(), store.execution_status());
    assert_eq!(raw.coverage().scope, store.coverage().scope);
    assert_eq!(
        raw.stage_results().debugger.status,
        store.stage_results().debugger.status
    );
    assert_eq!(
        raw.instruction_range(0..5).unwrap().as_ptr(),
        store.trace().recording.instruction_events.as_ptr()
    );
    let first = raw.instruction_event(0).unwrap();
    assert_eq!(raw.disassembly_for_event(0).unwrap().address, first.address);
    let id = first.instruction_id.unwrap();
    assert_eq!(
        raw.instruction_execution_positions(id).unwrap(),
        store.instruction_execution_positions(id).unwrap()
    );
    assert_eq!(raw.heap_events_for_allocation(1).len(), 2);
    let snapshot = raw.snapshot(0).unwrap();
    assert!(std::ptr::eq(snapshot, &store.trace().snapshots[0]));
    assert_eq!(raw.crash_report().is_some(), store.crash_report().is_some());
    // Availability metadata is not normalized or filled in by the view.
    let heap = load("v21_heap.json");
    let heap_raw = heap.raw();
    let block = heap_raw.heap_block(1).unwrap();
    assert_eq!(
        block.raw_bytes_hex,
        heap.trace().heap_state[0].raw_bytes_hex
    );
    assert_eq!(
        block.memory_read_error,
        heap.trace().heap_state[0].memory_read_error
    );
}

#[test]
fn c_checkpoint_context_uses_explicit_relationships_only() {
    let store = load("v21_dual.json");
    let semantic = store.semantic_input();
    let (index, snap) = store
        .trace()
        .snapshots
        .iter()
        .enumerate()
        .find(|(_, snap)| snap.call_stack.len() > 1 && !snap.variables.is_empty())
        .unwrap();
    let context = semantic.checkpoint_context(index).unwrap();
    assert_eq!(context.snapshot_index(), index);
    assert!(context.source_checkpoint_position().is_none());
    assert!(std::ptr::eq(context.snapshot(), snap));
    assert_eq!(context.frames().as_ptr(), snap.call_stack.as_ptr());
    assert_eq!(context.variables().as_ptr(), snap.variables.as_ptr());
    assert_eq!(context.globals().len(), snap.observation.globals.len());
    assert_eq!(
        context.source_location(),
        Some((snap.file.as_str(), snap.line))
    );
    assert_eq!(context.stop_id(), Some(snap.observation.stop_id));
    assert_eq!(
        context.instruction_position(),
        snap.observation.instruction_position
    );
    assert_eq!(context.observation_complete(), snap.observation.complete);
    assert_eq!(
        context.events_at_checkpoint().unwrap().len(),
        store.events_at_checkpoint(index).unwrap().len()
    );
    assert_eq!(
        context.heap_events_in_snapshot_interval().unwrap().len(),
        store
            .heap_events_at_stop(snap.observation.stop_id)
            .unwrap()
            .len()
    );
    let evidence = context.variable_evidence(0).unwrap();
    match semantic.resolve_evidence(&evidence).unwrap() {
        Evidence::SnapshotVariable(info) => assert!(std::ptr::eq(info, &snap.variables[0])),
        other => panic!("wrong evidence: {other:?}"),
    };

    let heap = load("v21_heap.json");
    let (i, _) = heap
        .trace()
        .snapshots
        .iter()
        .enumerate()
        .find(|(_, snap)| !snap.heap_blocks.is_empty())
        .unwrap();
    let context = heap.semantic_input().checkpoint_context(i).unwrap();
    let reference = context.heap_block_evidence(0).unwrap();
    assert!(matches!(
        heap.resolve_evidence(&reference),
        Ok(Evidence::SnapshotHeapBlock(_))
    ));
    assert!(context.heap_blocks()[0].allocation_id > 0);
    assert_eq!(context.coverage().scope, heap.coverage().scope);
}

#[test]
fn c_phase1_scope_status_and_pointer_identity_flow_through_views() {
    // V2.1 traces deserialize with `scope_status` defaulting to None: the
    // additive field changes no existing meaning and needs no version bump.
    let store = load("v21_dual.json");
    assert!(store
        .trace()
        .snapshots
        .iter()
        .flat_map(|s| s.variables.iter())
        .all(|v| v.scope_status.is_none()));

    // Phase 1 evidence injected into an otherwise V2.1 trace round-trips
    // through TP-1 parsing, TP-2 indexes and the TP-3 checkpoint context
    // over the same shared `Arc<DebugSessionTrace>` models — no parser or
    // view changes are involved.
    let mut trace: DebugSessionTrace = store.trace().clone();
    let position = trace
        .snapshots
        .iter()
        .position(|s| s.variables.len() >= 2 && s.variables[0].identity.is_some())
        .unwrap();
    let snapshot = &mut trace.snapshots[position];
    snapshot.variables[0].scope_status = Some(ScopeStatus::Active);
    snapshot.variables[1].scope_status = Some(ScopeStatus::Inactive);
    snapshot.variables[1].target = Some(PointerTarget {
        kind: "stack".into(),
        base_address: snapshot.variables[0].address.clone(),
        offset_bytes: Some(0),
        variable_identity: snapshot.variables[0].identity.clone(),
        ..Default::default()
    });
    let edited = TraceStore::from_trace(trace).unwrap();
    let report = edited.validate_deep();
    assert!(
        report.is_structurally_sound(),
        "Phase 1 fields must not introduce corruption findings: {report:?}"
    );
    let context = edited
        .semantic_input()
        .checkpoint_context(position)
        .unwrap();
    let vars = context.variables();
    assert_eq!(vars[0].scope_status, Some(ScopeStatus::Active));
    assert_eq!(vars[1].scope_status, Some(ScopeStatus::Inactive));
    let target = vars[1].target.as_ref().unwrap();
    assert_eq!(target.kind, "stack");
    assert_eq!(target.base_address.as_deref(), vars[0].address.as_deref());
    assert_eq!(
        target.variable_identity.as_deref(),
        vars[0].identity.as_deref()
    );
}

#[test]
fn c_missing_source_and_stop_never_turn_into_invented_joins() {
    let store = load("v21_loop.json");
    let mut trace: DebugSessionTrace = store.trace().clone();
    trace.snapshots[0].file.clear();
    trace.snapshots[0].line = 0;
    trace.snapshots[0].observation.stop_id = 0; // older/default observation
    trace.snapshots[0].observation.complete = false;
    trace.snapshots[0].memory_truncated = true;
    trace.snapshots[0].variables[0].raw_bytes_hex = None;
    trace.snapshots[0].variables[0].read_status = DataValidity::Unavailable;
    let edited = TraceStore::from_trace(trace).unwrap();
    let context = edited.semantic_input().checkpoint_context(0).unwrap();
    assert_eq!(context.source_location(), None);
    assert_eq!(context.stop_id(), None);
    assert!(!context.observation_complete());
    assert!(context.snapshot().memory_truncated);
    assert_eq!(
        context.variables()[0].read_status,
        DataValidity::Unavailable
    );
    assert!(context.variables()[0].raw_bytes_hex.is_none());
    assert!(matches!(
        context.heap_events_in_snapshot_interval(),
        Err(QueryError::Unavailable { .. })
    ));
    assert!(edited
        .semantic_input()
        .checkpoint_context(edited.trace().snapshots.len())
        .is_err());
    assert!(store.raw().snapshot_at_stop(2).unwrap().is_none());
}

#[test]
fn c_coarse_heap_boundary_does_not_assert_exact_event_stop() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stage1/realloc_move.json");
    let store = TraceStore::open(path, TraceLoadOptions::default()).unwrap();
    let context = store.semantic_input().checkpoint_context(6).unwrap();
    assert_eq!(context.stop_id(), Some(24));
    let events = context.heap_events_in_snapshot_interval().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].allocation_id, 3);
    assert_eq!(events[0].old_allocation_id, Some(1));
    // Source-mode and incomplete traces must remain usable.
    let partial = load("v21_bounded_infinite.json");
    assert_eq!(partial.execution_status(), &ExecutionStatus::Timeout);
    assert_eq!(partial.semantic_input().snapshots().count(), 3);
}

#[test]
fn d_evidence_categories_validate_positions_and_trace_identity() {
    let store = load("v21_dual.json");
    let clone = store.clone();
    let locations = [
        L::Snapshot(0),
        L::InstructionEvent(0),
        L::Disassembly(0),
        L::HeapEvent(0),
        L::HeapState(0),
        L::AllocationLifetime(0),
        L::TraceGap(0),
        L::SnapshotFrame {
            snapshot: 0,
            frame: 0,
        },
    ];
    for loc in locations {
        let reference = store.raw().evidence_ref(loc).unwrap();
        assert_eq!(reference.location(), loc);
        assert!(clone.semantic_input().resolve_evidence(&reference).is_ok());
        assert!(store.raw().resolve_evidence(&reference).is_ok());
        // Independently loading identical JSON is not the same trace.
        assert!(matches!(
            load("v21_dual.json").resolve_evidence(&reference),
            Err(QueryError::ForeignTrace)
        ));
    }
    let snap = store.evidence_ref(L::Snapshot(0)).unwrap();
    let event = store.evidence_ref(L::InstructionEvent(0)).unwrap();
    assert!(matches!(
        store.resolve_evidence(&snap),
        Ok(Evidence::Snapshot(_))
    ));
    assert!(matches!(
        store.resolve_evidence(&event),
        Ok(Evidence::InstructionEvent(_))
    ));
    assert!(matches!(
        store.evidence_ref(L::InstructionEvent(usize::MAX)),
        Err(QueryError::InvalidReference { .. })
    ));
    assert!(matches!(
        store.evidence_ref(L::SnapshotVariable {
            snapshot: 0,
            variable: usize::MAX
        }),
        Err(QueryError::InvalidReference { .. })
    ));
    assert!(matches!(
        store.evidence_ref(L::SnapshotFrame {
            snapshot: usize::MAX,
            frame: 0
        }),
        Err(QueryError::InvalidReference { .. })
    ));
    // A weak reference does not own a parsed trace; after it is dropped,
    // an unrelated load still cannot resolve the old position.
    let expired = {
        let transient = load("v21_dual.json");
        transient.evidence_ref(L::Snapshot(0)).unwrap()
    };
    assert!(matches!(
        store.resolve_evidence(&expired),
        Err(QueryError::ForeignTrace)
    ));
}

#[test]
fn d_nested_memory_and_optional_reports_preserve_availability() {
    let base = load("v21_dual.json");
    let mut trace = base.trace().clone();
    let memory = c_trace_engine::models::MemoryObservation {
        expression: "&x".into(),
        address: Some("0x1000".into()),
        requested_bytes: 8,
        raw_bytes_hex: None,
        status: DataValidity::Unreadable,
        error: Some("read failed".into()),
    };
    trace.recording.instruction_events[0].memory.push(memory);
    let store = TraceStore::from_trace(trace).unwrap();
    let reference = store
        .evidence_ref(L::EventMemory {
            event: 0,
            observation: 0,
        })
        .unwrap();
    match store.resolve_evidence(&reference).unwrap() {
        Evidence::EventMemory(observation) => {
            assert_eq!(observation.status, DataValidity::Unreadable);
            assert!(observation.raw_bytes_hex.is_none());
            assert_eq!(observation.error.as_deref(), Some("read failed"));
        }
        other => panic!("wrong evidence {other:?}"),
    }
    assert!(matches!(
        store.evidence_ref(L::EventMemory {
            event: 0,
            observation: 1
        }),
        Err(QueryError::InvalidReference { .. })
    ));
    let crash = load("v21_crash.json");
    assert!(matches!(
        crash.resolve_evidence(&crash.evidence_ref(L::CrashReport).unwrap()),
        Ok(Evidence::CrashReport(_))
    ));
    assert!(matches!(
        crash.resolve_evidence(&crash.evidence_ref(L::ValgrindError(0)).unwrap()),
        Ok(Evidence::ValgrindError(_))
    ));
    let normal = load("v21_normal.json");
    assert!(matches!(
        normal.evidence_ref(L::CrashReport),
        Err(QueryError::Unavailable { .. })
    ));
}

#[test]
fn e_bounded_ranges_and_explicit_checkpoint_traversal() {
    let base = load("v21_loop.json");
    let raw = base.raw();
    assert!(raw.instruction_range(0..0).unwrap().is_empty());
    assert_eq!(raw.instruction_range(7..8).unwrap()[0].event_id, 7);
    assert_eq!(
        raw.instruction_range(0..raw.retained_event_count())
            .unwrap()
            .len(),
        raw.retained_event_count()
    );
    assert!(matches!(
        raw.instruction_range(0..usize::MAX),
        Err(QueryError::InvalidRange { .. })
    ));
    assert_eq!(raw.snapshots_in_range(0..2).unwrap().len(), 2);

    let mut trace = base.trace().clone();
    trace.recording.source_checkpoints = vec![0, 4, 9, 9, 15];
    let store = TraceStore::from_trace(trace).unwrap();
    let semantic = store.semantic_input();
    assert_eq!(semantic.snapshots().count(), store.trace().snapshots.len());
    let collected: Vec<_> = semantic
        .source_checkpoints()
        .enumerate()
        .map(|(position, context)| {
            let context = context.unwrap();
            assert_eq!(context.source_checkpoint_position(), Some(position));
            context.snapshot_index()
        })
        .collect();
    assert_eq!(collected, [0, 4, 9, 9, 15]);
    assert_eq!(
        semantic
            .checkpoint_context(2)
            .unwrap()
            .source_checkpoint_position(),
        None
    );

    let mut trace = base.trace().clone();
    trace.recording.source_checkpoints = vec![usize::MAX];
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(matches!(
        store.semantic_input().source_checkpoints().next(),
        Some(Err(QueryError::OutOfRange { .. }))
    ));
}

#[test]
fn f_g_parallel_readers_share_raw_and_semantic_observations() {
    let store = Arc::new(load("v21_dual.json"));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let store = Arc::clone(&store);
            std::thread::spawn(move || {
                let raw = store.raw();
                let semantic = store.semantic_input();
                let count: usize = semantic
                    .source_checkpoints()
                    .map(|ctx| ctx.unwrap().events_at_checkpoint().unwrap().len())
                    .sum();
                let evidence = raw.evidence_ref(L::InstructionEvent(3)).unwrap();
                let event_id = match semantic.resolve_evidence(&evidence).unwrap() {
                    Evidence::InstructionEvent(e) => e.event_id,
                    other => panic!("{other:?}"),
                };
                (
                    count,
                    event_id,
                    raw.instruction_range(10..13).unwrap().len(),
                )
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    for result in &results[1..] {
        assert_eq!(result, &results[0]);
    }
    assert_eq!(results[0].1, 3);
    assert_eq!(results[0].2, 3);
}

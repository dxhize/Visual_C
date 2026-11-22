//! SE-1: object identity must follow verified runtime identity, never address,
//! source name, pointer candidate or independent tool evidence.
use c_trace_engine::models::{
    DataValidity, DebugSessionTrace, ExecutionSnapshot, FrameVariables, HeapEventType,
    MemoryEvidence, PointerTarget, ScopeStatus, SnapshotObservation, StackFrameInfo, VariableInfo,
};
use c_trace_engine::semantic::{
    CheckpointPresence, HeapLifetimeStatus, ObjectKind, ObjectRegistry, RegistryError, Visibility,
};
use c_trace_engine::trace_parser::{
    Evidence, EvidenceConfidence as C, EvidenceLocation as L, QueryError, TraceStore,
};

fn local(name: &str, activation: Option<u64>, identity: Option<&str>, addr: &str) -> VariableInfo {
    VariableInfo {
        name: name.into(),
        scope: Some("stack".into()),
        activation_id: activation,
        identity: identity.map(str::to_owned),
        address: Some(addr.into()),
        size_bytes: Some(4),
        read_status: DataValidity::Valid,
        value: "7".into(),
        scope_status: Some(ScopeStatus::Active),
        ..Default::default()
    }
}

fn global(name: &str, identity: &str, scope: &str) -> VariableInfo {
    VariableInfo {
        name: name.into(),
        identity: Some(identity.into()),
        scope: Some(scope.into()),
        ..Default::default()
    }
}

fn frame(level: u32, func: &str) -> StackFrameInfo {
    StackFrameInfo {
        level,
        func: func.into(),
        ..Default::default()
    }
}

fn snap(index: usize, pid: Option<u32>, variables: Vec<VariableInfo>) -> ExecutionSnapshot {
    ExecutionSnapshot {
        step_index: index,
        file: "scope.c".into(),
        line: 7,
        func: "recurse".into(),
        observation: SnapshotObservation {
            stop_id: index as u64 + 1,
            pid,
            complete: true,
            ..Default::default()
        },
        call_stack: vec![frame(0, "recurse")],
        variables,
        ..Default::default()
    }
}

fn synthetic() -> DebugSessionTrace {
    let a = local(
        "value",
        Some(1),
        Some("activation:1:block:A:7:value"),
        "0x1000",
    );
    let b = local(
        "value",
        Some(2),
        Some("activation:2:block:A:7:value"),
        "0x1000",
    );
    let mut t = DebugSessionTrace {
        snapshots: vec![
            snap(0, Some(42), vec![a.clone()]),
            snap(1, Some(42), vec![b]),
            snap(2, Some(42), vec![a]),
        ],
        ..Default::default()
    };
    t.snapshots[0].observation.globals = vec![
        global("same", "symbol:'a.c'::same", "static"),
        global("same", "symbol:'b.c'::same", "static"),
        global("same", "symbol:same", "global"),
    ];
    t.snapshots[1].observation.globals = t.snapshots[0].observation.globals.clone();
    t.snapshots[2].observation.globals = t.snapshots[0].observation.globals.clone();
    t
}

#[test]
fn verified_activations_shadowing_static_and_same_address_remain_distinct() {
    let store = TraceStore::from_trace(synthetic()).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let registry = ObjectRegistry::build(&view).unwrap();
    let first = registry.objects_at_checkpoint(0).unwrap().next().unwrap();
    let second = registry.objects_at_checkpoint(1).unwrap().next().unwrap();
    assert_eq!(first.kind, ObjectKind::Stack);
    assert_eq!(first.activation_id, Some(1));
    assert_eq!(second.activation_id, Some(2));
    assert_eq!(first.first_address, second.first_address);
    assert_ne!(
        first.id.evidence().location(),
        second.id.evidence().location()
    );
    assert_eq!(registry.observations(&first.id).unwrap().count(), 2);
    assert_eq!(registry.observation_at(&first.id, 1).unwrap().count(), 0);
    assert_eq!(registry.observation_at(&first.id, 2).unwrap().count(), 1);
    assert_eq!(
        registry.presence_at(&first.id, 1).unwrap(),
        CheckpointPresence::NotObserved {
            snapshot_complete: true
        }
    );
    let globals: Vec<_> = registry
        .objects_at_checkpoint(0)
        .unwrap()
        .filter(|o| o.kind != ObjectKind::Stack)
        .collect();
    assert_eq!(globals.len(), 3);
    assert_ne!(
        globals[0].id.evidence().location(),
        globals[1].id.evidence().location()
    );
    assert_eq!(globals[0].kind, ObjectKind::StaticLocal);
    assert_eq!(globals[1].kind, ObjectKind::StaticLocal);
    assert_eq!(globals[2].kind, ObjectKind::Global);
    for g in globals {
        assert_eq!(registry.observations(&g.id).unwrap().count(), 3);
    }
    assert_eq!(registry.observations_at_checkpoint(0).unwrap().count(), 4);
    let first_obs = registry
        .observation_at(&first.id, 0)
        .unwrap()
        .next()
        .unwrap();
    assert_eq!(first_obs.record.visibility, Visibility::InScope);
    assert!(
        matches!(store.resolve_evidence(&first_obs.evidence().unwrap()).unwrap(), Evidence::SnapshotVariable(v) if std::ptr::eq(v,&store.trace().snapshots[0].variables[0]))
    );
}

#[test]
fn missing_activation_fallback_identity_partial_capture_and_scope_do_not_merge() {
    let mut t = synthetic();
    t.snapshots[1].variables[0].activation_id = None;
    t.snapshots[1].variables[0].identity = Some("activation:2:value:0x1000".into());
    t.snapshots[1].variables[0].scope_status = Some(ScopeStatus::Inactive);
    t.snapshots[1].variables[0].read_status = DataValidity::Unreadable;
    t.snapshots[1].variables[0].memory_evidence = Some(MemoryEvidence {
        requested_bytes: 4,
        captured_bytes: 0,
        status: "unreadable".into(),
        ..Default::default()
    });
    t.snapshots[1].observation.complete = false;
    t.snapshots[2].variables.clear();
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let missing = r
        .objects_at_checkpoint(1)
        .unwrap()
        .find(|o| o.kind == ObjectKind::Stack)
        .unwrap();
    assert_eq!(missing.identity_confidence, C::Unavailable);
    assert_eq!(missing.activation_id, None);
    let obs = r.observation_at(&missing.id, 1).unwrap().next().unwrap();
    assert_eq!(obs.record.visibility, Visibility::OutOfScope);
    assert!(!obs.record.snapshot_complete);
    assert_eq!(obs.variable().unwrap().unwrap().memory.valid_range, None);
    let first = r.objects_at_checkpoint(0).unwrap().next().unwrap();
    assert_eq!(
        r.presence_at(&first.id, 2).unwrap(),
        CheckpointPresence::NotObserved {
            snapshot_complete: true
        }
    );
    assert_eq!(r.observations(&missing.id).unwrap().count(), 1);
}

#[test]
fn caller_frames_have_distinct_activations_and_foreign_references_fail() {
    let mut t = synthetic();
    let caller = frame(1, "recurse");
    t.snapshots[0].caller_frames = vec![FrameVariables {
        frame: caller.clone(),
        activation_id: 3,
        variables: vec![local(
            "value",
            Some(3),
            Some("activation:3:block:A:7:value"),
            "0x1000",
        )],
        ..Default::default()
    }];
    t.snapshots[0].call_stack.push(caller);
    let store = TraceStore::from_trace(t.clone()).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let reference = store
        .evidence_ref(L::SnapshotCallerFrame {
            snapshot: 0,
            frame: 0,
        })
        .unwrap();
    let callers: Vec<_> = r.stack_objects(&reference).unwrap().collect();
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].activation_id, Some(3));
    let own = store
        .evidence_ref(L::SnapshotFrame {
            snapshot: 0,
            frame: 0,
        })
        .unwrap();
    assert_eq!(r.stack_objects(&own).unwrap().count(), 1);
    let foreign = TraceStore::from_trace(t).unwrap();
    let other_view = foreign.validated_semantic();
    let other = ObjectRegistry::build(&other_view).unwrap();
    assert!(matches!(
        other.object(&callers[0].id),
        Err(RegistryError::Evidence(QueryError::ForeignTrace))
    ));
    assert!(matches!(
        other.stack_objects(&reference),
        Err(RegistryError::Evidence(QueryError::ForeignTrace))
    ));
    assert!(matches!(
        r.objects_at_checkpoint(99),
        Err(RegistryError::OutOfRange { .. })
    ));
    assert!(matches!(
        r.observation_at(&callers[0].id, 99),
        Err(RegistryError::OutOfRange { .. })
    ));
}

#[test]
fn same_name_declaration_and_address_cannot_merge_without_native_identity() {
    let mut t = synthetic();
    t.snapshots[0].variables.push(local(
        "shadow",
        Some(1),
        Some("activation:1:shadow:0x1000"),
        "0x1000",
    ));
    t.snapshots[1].variables.push(local(
        "shadow",
        Some(2),
        Some("activation:2:shadow:0x1000"),
        "0x1000",
    ));
    t.snapshots[0].variables[1].declaration_ref = Some("debug:u0:o55".into());
    t.snapshots[1].variables[1].declaration_ref = Some("debug:u0:o55".into());
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let a = r
        .objects_at_checkpoint(0)
        .unwrap()
        .find(|o| o.name == Some("shadow"))
        .unwrap();
    let b = r
        .objects_at_checkpoint(1)
        .unwrap()
        .find(|o| o.name == Some("shadow"))
        .unwrap();
    assert_eq!(a.first_address, b.first_address);
    assert_eq!(a.declaration_ref, b.declaration_ref);
    assert_eq!(a.identity_confidence, C::Unavailable);
    assert_eq!(b.identity_confidence, C::Unavailable);
    assert_ne!(a.id.evidence().location(), b.id.evidence().location());
}

#[test]
fn structurally_invalid_imports_are_rejected_not_reinterpreted() {
    let text = include_str!("fixtures/stage1/realloc_move.json");
    let mut base: DebugSessionTrace = serde_json::from_str(text).unwrap();
    // TP8-F01: an existing, but wrong, predecessor (block 1 was Allocated).
    let wrong = base
        .heap_state
        .iter()
        .find(|b| b.realloc_from_id.is_some())
        .unwrap()
        .allocation_id;
    let first = base
        .heap_state
        .iter()
        .position(|b| b.allocation_id != wrong)
        .unwrap();
    base.heap_state[first].realloc_from_id = Some(wrong);
    let store = TraceStore::from_trace(base).unwrap();
    let view = store.validated_semantic();
    assert!(!view.report().is_structurally_sound());
    assert!(view
        .report()
        .findings()
        .iter()
        .any(|f| f.code == "REALLOC_LINK_CONTRADICTORY"));
    assert!(matches!(
        ObjectRegistry::build(&view),
        Err(RegistryError::Unsound { .. })
    ));

    let mut t = synthetic();
    t.snapshots[0].variables[0].points_to = Some("0x1000".into());
    t.snapshots[0].variables[0].target = Some(PointerTarget {
        kind: "heap".into(),
        provenance_status: Some("address_only".into()),
        allocation_id: Some(1),
        address_match_allocation_id: Some(1),
        ..Default::default()
    });
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(!view.report().is_structurally_sound());
    assert!(matches!(
        ObjectRegistry::build(&view),
        Err(RegistryError::Unsound { .. })
    ));

    let mut t = synthetic();
    t.recording.heap_history = Some(c_trace_engine::models::HeapHistoryCoverage {
        complete: true,
        process_scope: Some("verified_observed_pids_only".into()),
        unobserved_process_coverage: Some("complete_all_processes".into()),
        observed_event_pids: vec![42],
        ..Default::default()
    });
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(!view.report().is_structurally_sound());
    assert!(matches!(
        ObjectRegistry::build(&view),
        Err(RegistryError::Unsound { .. })
    ));
}

#[test]
fn partial_or_legacy_heap_history_preserves_events_without_inventing_predecessors() {
    let trace: DebugSessionTrace =
        serde_json::from_str(include_str!("fixtures/stage1/realloc_move.json")).unwrap();
    let store = TraceStore::from_trace(trace).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let child = r
        .objects()
        .iter()
        .find(|o| {
            o.kind == ObjectKind::Heap
                && r.heap_lifetime(&o.id)
                    .unwrap()
                    .unwrap()
                    .recorded_predecessor
                    .is_some()
        })
        .unwrap();
    let relation = r.heap_lifetime(&child.id).unwrap().unwrap();
    assert!(matches!(
        relation.realloc_confidence,
        C::Candidate | C::Definite
    ));
    let old = r.realloc_predecessor(&child.id).unwrap().unwrap();
    assert_eq!(
        Some(old.0.allocation_id.unwrap()),
        relation.recorded_predecessor
    );
    assert_eq!(old.1, relation.realloc_confidence);
    for heap in r.objects().iter().filter(|o| o.kind == ObjectKind::Heap) {
        let id = heap.allocation_id.unwrap();
        let indexed: Vec<_> = r
            .heap_events(&heap.id)
            .unwrap()
            .unwrap()
            .events()
            .map(|e| (e.sequence, e.allocation_id))
            .collect();
        let raw: Vec<_> = store
            .heap_events()
            .iter()
            .filter(|e| e.allocation_id == id)
            .map(|e| (e.sequence, e.allocation_id))
            .collect();
        assert_eq!(indexed, raw);
    }
    let event = store.evidence_ref(L::HeapEvent(0)).unwrap();
    let obj = r.heap_object(&event).unwrap().unwrap();
    assert_eq!(
        obj.allocation_id,
        Some(store.heap_events()[0].allocation_id)
    );
    let foreign = TraceStore::from_trace(store.trace().clone()).unwrap();
    let ref_other = foreign.evidence_ref(L::HeapEvent(0)).unwrap();
    assert!(matches!(
        r.heap_object(&ref_other),
        Err(RegistryError::Evidence(QueryError::ForeignTrace))
    ));
}

#[tokio::test]
async fn live_engine_recursion_realloc_zero_reuse_and_process_scope_oracles() {
    use c_trace_engine::{trace::trace_program, TraceConfig};
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped live SE-1: {tool} unavailable");
            return;
        }
    }
    for name in [
        "recursive.c",
        "phase8b_defined.c",
        "realloc_move.c",
        "calloc_demo.c",
        "heap_provenance_realloc.c",
        "heap_provenance.c",
        "phase8dr_fork.c",
    ] {
        let mut config = TraceConfig::default();
        config.build.source_file =
            format!("{}/fixtures/c/{name}", env!("CARGO_MANIFEST_DIR")).into();
        config.limits.max_steps = 400;
        config.limits.timeout_seconds = 48;
        config.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
        if name == "recursive.c" {
            config.limits.max_caller_frames = 3;
        }
        if name == "phase8dr_fork.c" {
            config.program_args = vec!["observed".into()];
        }
        let store = TraceStore::from_trace(trace_program(config).await.unwrap()).unwrap();
        let view = store.validated_semantic();
        assert!(
            view.report().is_structurally_sound(),
            "{name}: {:?}",
            view.report()
        );
        let r = ObjectRegistry::build(&view).unwrap();
        assert!(
            r.observation_count() >= store.trace().snapshots.len(),
            "{name}: must preserve checkpoint observations"
        );
        if name == "recursive.c" {
            assert!(
                r.objects()
                    .iter()
                    .any(|o| o.kind == ObjectKind::Stack
                        && r.observations(&o.id).unwrap().count() > 1)
            );
            let recursive: Vec<_> = r
                .objects()
                .iter()
                .filter(|o| o.name == Some("value") && o.kind == ObjectKind::Stack)
                .collect();
            assert!(
                recursive
                    .iter()
                    .filter_map(|o| o.activation_id)
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    >= 3
            );
            assert!(store
                .trace()
                .snapshots
                .iter()
                .any(|s| !s.caller_frames.is_empty()));
            assert!(r.objects().iter().any(|o| o.kind == ObjectKind::Stack
                && r.observations(&o.id)
                    .unwrap()
                    .any(|obs| matches!(obs.record.location, L::SnapshotCallerVariable { .. }))));
        }
        if name == "phase8b_defined.c" {
            let child = r
                .objects()
                .iter()
                .find(|o| {
                    o.kind == ObjectKind::Heap
                        && r.heap_lifetime(&o.id)
                            .unwrap()
                            .unwrap()
                            .recorded_predecessor
                            .is_some()
                })
                .unwrap();
            assert_eq!(
                r.heap_lifetime(&child.id)
                    .unwrap()
                    .unwrap()
                    .realloc_confidence,
                C::Definite
            );
            let old = r.realloc_predecessor(&child.id).unwrap().unwrap().0;
            assert_eq!(
                old.first_address, child.first_address,
                "same address, distinct lifetimes"
            );
            assert_ne!(old.allocation_id, child.allocation_id);
        }
        if name == "realloc_move.c" {
            let moved = r
                .objects()
                .iter()
                .find(|o| {
                    o.kind == ObjectKind::Heap && r.realloc_predecessor(&o.id).unwrap().is_some()
                })
                .unwrap();
            let old = r.realloc_predecessor(&moved.id).unwrap().unwrap();
            assert_eq!(old.1, C::Definite);
            assert_ne!(
                moved.first_address, old.0.first_address,
                "live moved realloc must change the actual address"
            );
            assert_ne!(moved.allocation_id, old.0.allocation_id);
        }
        if name == "calloc_demo.c" {
            let calloc = store
                .heap_events()
                .iter()
                .find(|e| e.event_type == HeapEventType::Allocated && e.calloc_count == Some(16))
                .expect("native calloc event");
            let object = r.heap_by_id(calloc.allocation_id).unwrap();
            let lifetime = r.heap_lifetime(&object.id).unwrap().unwrap();
            assert_eq!(
                lifetime.record.unwrap().allocation_sequence,
                calloc.sequence
            );
            assert_eq!(lifetime.status, HeapLifetimeStatus::RecordedFreed);
            assert!(r
                .heap_events(&object.id)
                .unwrap()
                .unwrap()
                .events()
                .any(|e| e.event_type == HeapEventType::Freed));
        }
        if name == "heap_provenance_realloc.c" {
            let terminal = store
                .heap_events()
                .iter()
                .find(|e| {
                    e.event_type == HeapEventType::Reallocated
                        && e.old_allocation_id == Some(e.allocation_id)
                        && e.address.is_none()
                        && e.size_bytes == Some(0)
                })
                .unwrap();
            let terminal_object = r.heap_by_id(terminal.allocation_id).unwrap();
            assert!(r
                .realloc_predecessor(&terminal_object.id)
                .unwrap()
                .is_none());
            assert_eq!(
                r.heap_lifetime(&terminal_object.id)
                    .unwrap()
                    .unwrap()
                    .status,
                HeapLifetimeStatus::RecordedFreed
            );
            let event_position = store
                .heap_events()
                .iter()
                .position(|e| std::ptr::eq(e, terminal))
                .unwrap();
            let first_after = store
                .trace()
                .snapshots
                .iter()
                .find(|s| s.observation.heap_event_count > event_position)
                .expect("retained checkpoint after terminal-free");
            assert!(matches!(
                r.presence_at(&terminal_object.id, first_after.step_index)
                    .unwrap(),
                CheckpointPresence::NotObserved { .. }
            ));
            for failed in store.heap_events().iter().filter(|e| {
                e.event_type == HeapEventType::AllocationFailed && e.old_allocation_id.is_none()
            }) {
                assert!(r.heap_by_id(failed.allocation_id).is_none());
            }
            assert!(r
                .objects()
                .iter()
                .filter(|o| o.kind == ObjectKind::Heap)
                .any(
                    |o| r.heap_lifetime(&o.id).unwrap().unwrap().realloc_confidence == C::Definite
                ));
        }
        if name == "heap_provenance.c" {
            for (i, a) in store.heap_events().iter().enumerate() {
                for b in store.heap_events().iter().skip(i + 1) {
                    if a.address.is_some()
                        && a.address == b.address
                        && a.allocation_id != b.allocation_id
                        && b.event_type == HeapEventType::Allocated
                    {
                        let ao = r.heap_by_id(a.allocation_id).unwrap();
                        let bo = r.heap_by_id(b.allocation_id).unwrap();
                        assert_ne!(ao.allocation_id, bo.allocation_id);
                        assert!(r.realloc_predecessor(&bo.id).unwrap().is_none());
                    }
                }
            }
        }
        if name == "phase8dr_fork.c" {
            let pids: std::collections::HashSet<_> = r
                .objects()
                .iter()
                .filter(|o| o.kind == ObjectKind::Heap)
                .filter_map(|o| o.pid)
                .collect();
            assert_eq!(pids.len(), 2);
            assert_eq!(
                view.process_coverage().unobserved_descendants,
                C::Unavailable
            );
            assert!(store
                .trace()
                .snapshots
                .iter()
                .all(|s| s.observation.pid == store.trace().snapshots[0].observation.pid));
        }
    }
}

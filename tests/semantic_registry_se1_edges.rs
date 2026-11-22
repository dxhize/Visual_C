//! Additional identity-boundary regressions: duplicate native IDs, static
//! storage across activations, legacy and partial histories, sparse stops.
use c_trace_engine::models::{
    DebugSessionTrace, ExecutionSnapshot, HeapHistoryCoverage, ScopeStatus, SnapshotObservation,
    StackFrameInfo, VariableInfo,
};
use c_trace_engine::semantic::{
    CheckpointPresence, ObjectKind, ObjectRegistry, RegistryError, Visibility,
};
use c_trace_engine::trace_parser::{EvidenceConfidence as C, TraceStore};

fn var(name: &str, identity: &str, activation: Option<u64>, scope: &str) -> VariableInfo {
    VariableInfo {
        name: name.into(),
        identity: Some(identity.into()),
        activation_id: activation,
        scope: Some(scope.into()),
        address: Some("0x1000".into()),
        scope_status: Some(ScopeStatus::Active),
        ..Default::default()
    }
}

fn trace() -> DebugSessionTrace {
    let mut t = DebugSessionTrace::default();
    for i in 0..3 {
        t.snapshots.push(ExecutionSnapshot {
            step_index: i,
            file: "scope.c".into(),
            line: 7,
            func: "call".into(),
            variables: vec![var(
                "value",
                "activation:1:block:400-500:7:value",
                Some(1),
                "stack",
            )],
            observation: SnapshotObservation {
                stop_id: i as u64 + 1,
                pid: Some(42),
                complete: true,
                ..Default::default()
            },
            call_stack: vec![StackFrameInfo {
                level: 0,
                func: "call".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
    }
    t
}

#[test]
fn duplicated_identity_at_one_stop_never_resolves_by_first_match() {
    let mut t = trace();
    let duplicate = t.snapshots[0].variables[0].clone();
    t.snapshots[0].variables.push(duplicate);
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    if !view.report().is_structurally_sound() {
        assert!(matches!(
            ObjectRegistry::build(&view),
            Err(RegistryError::Unsound { .. })
        ));
        return;
    }
    let r = ObjectRegistry::build(&view).unwrap();
    let first: Vec<_> = r.objects_at_checkpoint(0).unwrap().collect();
    assert_eq!(first.len(), 2);
    assert!(first
        .iter()
        .all(|o| o.identity_confidence == C::Unavailable));
    assert_ne!(
        first[0].id.evidence().location(),
        first[1].id.evidence().location()
    );
    assert_eq!(r.observations(&first[0].id).unwrap().count(), 1);
    assert_eq!(r.observations(&first[1].id).unwrap().count(), 1);
    assert_eq!(r.observation_at(&first[0].id, 2).unwrap().count(), 0);
}

#[test]
fn function_static_has_static_storage_identity_not_recursive_stack_identity() {
    let mut t = trace();
    for (i, s) in t.snapshots.iter_mut().enumerate() {
        s.variables.push(var(
            "cache",
            "static:block:400-500:9:cache",
            Some(if i == 1 { 2 } else { 1 }),
            "static",
        ));
        s.observation
            .globals
            .push(var("cache", "symbol:'other-unit.c'::cache", None, "static"));
    }
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let persistent = r
        .objects()
        .iter()
        .find(|o| o.runtime_identity == Some("static:block:400-500:9:cache"))
        .unwrap();
    assert_eq!(persistent.kind, ObjectKind::StaticLocal);
    assert_eq!(persistent.activation_id, None);
    assert_eq!(persistent.identity_confidence, C::Definite);
    assert_eq!(r.observations(&persistent.id).unwrap().count(), 3);
    let unrelated = r
        .objects()
        .iter()
        .find(|o| o.runtime_identity == Some("symbol:'other-unit.c'::cache"))
        .unwrap();
    assert_eq!(r.observations(&unrelated.id).unwrap().count(), 3);
    assert_ne!(
        persistent.id.evidence().location(),
        unrelated.id.evidence().location()
    );
}

#[test]
fn partial_history_and_unobserved_checkpoint_are_not_a_dead_object() {
    let mut t: DebugSessionTrace =
        serde_json::from_str(include_str!("fixtures/stage1/realloc_move.json")).unwrap();
    t.recording.heap_history = Some(HeapHistoryCoverage {
        complete: false,
        ..Default::default()
    });
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let successor = r
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
    let lifetime = r.heap_lifetime(&successor.id).unwrap().unwrap();
    assert_eq!(lifetime.identity_confidence, C::Candidate);
    assert_eq!(lifetime.realloc_confidence, C::Candidate);
    assert_eq!(
        r.realloc_predecessor(&successor.id).unwrap().unwrap().1,
        C::Candidate
    );
    assert!(matches!(
        r.presence_at(&successor.id, 0).unwrap(),
        CheckpointPresence::NotObserved { .. }
    ));
    assert!(matches!(
        r.observation_at(&successor.id, store.trace().snapshots.len()),
        Err(RegistryError::OutOfRange { .. })
    ));
    assert!(r.estimated_bytes() > r.observation_count());
}

#[test]
fn missing_metadata_and_capture_gaps_never_create_or_destroy_objects() {
    let mut t = trace();
    t.snapshots[1].variables.clear();
    t.snapshots[1].observation.complete = false;
    t.snapshots[2].variables[0].scope_status = Some(ScopeStatus::Unknown);
    t.recording
        .trace_gaps
        .push(c_trace_engine::models::TraceGap {
            instruction_position: 0,
            start_address: None,
            end_address: None,
            origin: "user_code".into(),
            reason: "excluded code".into(),
        });
    t.recording.heap_history = Some(HeapHistoryCoverage {
        complete: false,
        ..Default::default()
    });
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let id = &r.objects_at_checkpoint(0).unwrap().next().unwrap().id;
    assert_eq!(r.observations(id).unwrap().count(), 2);
    assert_eq!(
        r.presence_at(id, 1).unwrap(),
        CheckpointPresence::NotObserved {
            snapshot_complete: false
        }
    );
    assert_eq!(
        r.observation_at(id, 2)
            .unwrap()
            .next()
            .unwrap()
            .record
            .visibility,
        Visibility::UnknownScope
    );
    assert_eq!(r.objects_at_checkpoint(1).unwrap().count(), 0);
    assert_eq!(r.observation_at(id, 1).unwrap().count(), 0);
    let legacy = TraceStore::open("examples/v21_normal.json", Default::default()).unwrap();
    let view = legacy.validated_semantic();
    assert!(view.report().is_structurally_sound());
    let r = ObjectRegistry::build(&view).unwrap();
    assert_eq!(
        r.objects_at_checkpoint(0).unwrap().count(),
        r.observations_at_checkpoint(0).unwrap().count()
    );
}

#[test]
fn two_observations_borrow_distinct_bytes_without_decoding_initialization() {
    use c_trace_engine::models::{DataValidity, MemoryEvidence};
    let mut t = trace();
    t.snapshots[0].variables[0].raw_bytes_hex = Some("07".into());
    t.snapshots[0].variables[0].bytes_status = DataValidity::Truncated;
    t.snapshots[0].variables[0].memory_evidence = Some(MemoryEvidence {
        extent_bytes: Some(4),
        requested_bytes: 4,
        captured_bytes: 1,
        status: "partial".into(),
        limit: Some("max_snapshot_memory_bytes".into()),
        ..Default::default()
    });
    t.snapshots[1].variables[0].value = "9".into();
    t.snapshots[1].variables[0].raw_bytes_hex = None;
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let id = &r.objects_at_checkpoint(0).unwrap().next().unwrap().id;
    assert_eq!(r.observations(id).unwrap().count(), 3);
    let first = r.observation_at(id, 0).unwrap().next().unwrap();
    let second = r.observation_at(id, 1).unwrap().next().unwrap();
    assert_eq!(
        first.variable().unwrap().unwrap().memory.valid_range,
        Some(0..1)
    );
    assert_eq!(
        first.variable().unwrap().unwrap().memory.unobserved_range,
        Some(1..4)
    );
    assert!(std::ptr::eq(
        first.variable().unwrap().unwrap().info,
        &store.trace().snapshots[0].variables[0]
    ));
    assert_eq!(second.variable().unwrap().unwrap().info.value, "9");
    assert_eq!(second.variable().unwrap().unwrap().memory.valid_range, None);
    assert_eq!(first.record.visibility, Visibility::InScope);
}

#[test]
fn duplicate_static_type_identity_is_an_import_error_not_a_merge() {
    use c_trace_engine::dwarf::{TypeEntry, TypeRegistry};
    let mut t = trace();
    let ty = TypeEntry {
        id: "duplicate".into(),
        unit_id: "u".into(),
        die_offset: 1,
        kind: "base".into(),
        name: Some("int".into()),
        byte_size: Some(4),
        encoding: None,
        referenced_type: None,
        members: vec![],
        dimensions: vec![],
        enumerators: vec![],
        declaration_only: false,
        runtime_dependent: false,
    };
    t.type_registry = Some(TypeRegistry {
        types: vec![ty.clone(), ty],
        ..Default::default()
    });
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(
        view.report()
            .findings()
            .iter()
            .any(|f| f.code == "DWARF_DUPLICATE_ID"),
        "{:?}",
        view.report()
    );
    assert!(matches!(
        ObjectRegistry::build(&view),
        Err(RegistryError::Unsound { .. })
    ));
}

#[test]
fn missing_record_in_memory_truncated_stop_is_not_a_complete_snapshot() {
    let mut t = trace();
    t.snapshots[1].variables.clear();
    t.snapshots[1].memory_truncated = true;
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let id = &r.objects_at_checkpoint(0).unwrap().next().unwrap().id;
    assert_eq!(
        r.presence_at(id, 1).unwrap(),
        CheckpointPresence::NotObserved {
            snapshot_complete: false
        }
    );
    assert_eq!(r.observations(id).unwrap().count(), 2);
}

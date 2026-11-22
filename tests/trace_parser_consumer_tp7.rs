//! TP-7 composed evidence: canonical identity, bounded pages, no inferred joins.
use c_trace_engine::models::{
    DataValidity, DebugSessionTrace, ExecutionSnapshot, FrameVariables, HeapEvent, HeapEventType,
    InstructionEvent, MemoryEvidence, PointerTarget, ScopeStatus, SnapshotObservation,
    SourceContext, StackFrameInfo, TraceGap, ValgrindError, ValgrindReport, ValgrindSiteCandidate,
    VariableInfo,
};
use c_trace_engine::trace_parser::consumer::{VariableScope as S, MAX_CONSUMER_PAGE};
use c_trace_engine::trace_parser::{
    Evidence, EvidenceConfidence as C, EvidenceLocation as L, QueryError, TraceStore,
};

fn var(name: &str, identity: &str, activation: u64) -> VariableInfo {
    VariableInfo {
        name: name.into(),
        identity: Some(identity.into()),
        activation_id: Some(activation),
        scope_status: Some(ScopeStatus::Active),
        read_status: DataValidity::Valid,
        value: "7".into(),
        ..Default::default()
    }
}

fn trace() -> DebugSessionTrace {
    let caller = StackFrameInfo {
        level: 1,
        func: "main".into(),
        address: Some("0x4000".into()),
        ..Default::default()
    };
    let mut t = DebugSessionTrace::default();
    t.recording.source_checkpoints = vec![0];
    let mut first = var("shadow", "activation:1:outer", 1);
    first.address = Some("0x1000".into());
    first.size_bytes = Some(4);
    let mut second = var("shadow", "activation:1:inner", 1);
    second.address = Some("0x2000".into());
    second.size_bytes = Some(4);
    second.raw_bytes_hex = Some("07000000".into());
    second.bytes_status = DataValidity::Valid;
    second.memory_evidence = Some(MemoryEvidence {
        requested_bytes: 4,
        captured_bytes: 4,
        status: "full".into(),
        ..Default::default()
    });
    let mut alias = var("alias", "activation:1:alias", 1);
    alias.points_to = Some("0x2000".into());
    alias.target = Some(PointerTarget {
        kind: "variable".into(),
        variable_identity: second.identity.clone(),
        ..Default::default()
    });
    let mut global = var("g", "global:g", 0);
    global.activation_id = None;
    t.snapshots.push(ExecutionSnapshot {
        file: "scope.c".into(),
        line: 12,
        func: "run".into(),
        observation: SnapshotObservation {
            stop_id: 7,
            pid: Some(42),
            globals: vec![global],
            complete: true,
            ..Default::default()
        },
        call_stack: vec![
            StackFrameInfo {
                level: 0,
                func: "run".into(),
                full_file: Some("/src/scope.c".into()),
                ..Default::default()
            },
            caller.clone(),
        ],
        variables: vec![first, second, alias],
        caller_frames: vec![FrameVariables {
            frame: caller,
            activation_id: 2,
            variables: vec![var("shadow", "activation:2:caller", 2)],
            ..Default::default()
        }],
        ..Default::default()
    });
    t
}

#[test]
fn checkpoint_composes_scoped_variables_without_losing_raw_identity() {
    let t = trace();
    let store = TraceStore::from_trace(t.clone()).unwrap();
    let clone = store.clone();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let summary = view.checkpoint_summary(0).unwrap();
    assert_eq!(summary.snapshot_index(), 0);
    assert_eq!(summary.stop_id(), Some(7));
    assert_eq!(summary.attempted_instruction_position(), 0);
    assert_eq!(summary.pid(), Some(42));
    assert!(summary.observation_complete());
    assert_eq!(summary.source_location(), Some(("scope.c", 12, "run")));
    assert!(
        matches!(clone.resolve_evidence(&summary.reference).unwrap(), Evidence::Snapshot(s) if std::ptr::eq(s, &store.trace().snapshots[0]))
    );
    let locals: Vec<_> = summary
        .variables_page(S::Local, 0..3)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(locals[0].info.name, locals[1].info.name);
    assert_ne!(locals[0].info.identity, locals[1].info.identity);
    assert_eq!(locals[1].memory.valid_range, Some(0..4));
    assert_eq!(locals[1].memory.confidence, C::Definite);
    assert!(std::ptr::eq(
        locals[1].info,
        &store.trace().snapshots[0].variables[1]
    ));
    assert_eq!(
        view.variable_target(&locals[2])
            .unwrap()
            .record
            .unwrap()
            .identity,
        locals[1].info.identity
    );
    assert_eq!(
        view.variable_target(&locals[2]).unwrap().confidence,
        C::Definite
    );
    assert!(locals[2]
        .pointer
        .as_ref()
        .unwrap()
        .definitive_allocation
        .is_none());
    let global = summary.variable(S::Global, 0).unwrap();
    assert_eq!(global.info.identity.as_deref(), Some("global:g"));
    let frame = summary.caller_frame(0).unwrap();
    assert_eq!(frame.frame.activation_id, 2);
    assert_eq!(frame.position, 0);
    assert_eq!(frame.confidence, C::Definite);
    assert!(matches!(
        clone.resolve_evidence(&frame.reference).unwrap(),
        Evidence::SnapshotCallerFrame(_)
    ));
    assert_eq!(
        summary
            .variable(S::Caller(0), 0)
            .unwrap()
            .info
            .identity
            .as_deref(),
        Some("activation:2:caller")
    );
    for (id, expected_len) in [(1, 3), (2, 1)] {
        let activation = summary.frame_activation(id).unwrap().unwrap();
        assert_eq!(activation.activation_id, id);
        assert_eq!(activation.confidence, C::Definite);
        assert_eq!(
            activation.variables_page(0..expected_len).unwrap().count(),
            expected_len
        );
    }
    assert!(summary.frame_activation(0).unwrap().is_none());
    assert!(summary.frame_activation(99).unwrap().is_none());
    assert!(summary.first_location_findings().next().is_none());
    let other = TraceStore::from_trace(t).unwrap();
    assert!(matches!(
        other.resolve_evidence(&summary.reference),
        Err(QueryError::ForeignTrace)
    ));
    let foreign = other
        .evidence_ref(L::SnapshotVariable {
            snapshot: 0,
            variable: 1,
        })
        .unwrap();
    assert!(matches!(
        view.variable(&foreign),
        Err(QueryError::ForeignTrace)
    ));
}

#[test]
fn large_pages_have_explicit_bounds_and_preserve_order_and_identity() {
    let mut t = trace();
    t.snapshots[0].variables = (0..310)
        .map(|i| var(&format!("v{i}"), &format!("activation:1:v{i}"), 1))
        .collect();
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    let summary = view.checkpoint_summary(0).unwrap();
    let all = summary.frame_activation(1).unwrap().unwrap();
    for range in [
        0..MAX_CONSUMER_PAGE,
        MAX_CONSUMER_PAGE..(2 * MAX_CONSUMER_PAGE),
        256..310,
    ] {
        let items: Vec<_> = all
            .variables_page(range.clone())
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(items.len(), range.end - range.start);
        for (offset, item) in items.iter().enumerate() {
            assert_eq!(
                item.reference.location(),
                L::SnapshotVariable {
                    snapshot: 0,
                    variable: range.start + offset
                }
            );
            assert!(std::ptr::eq(
                item.info,
                &store.trace().snapshots[0].variables[range.start + offset]
            ));
        }
    }
    assert!(matches!(
        summary.variables_page(S::Local, 0..129),
        Err(QueryError::LimitExceeded {
            requested: 129,
            max: 128,
            ..
        })
    ));
    assert!(matches!(
        summary.variables_page(S::Local, 0..311),
        Err(QueryError::InvalidRange { .. })
    ));
    let reversed = std::ops::Range { start: 13, end: 12 };
    assert!(matches!(
        summary.variables_page(S::Local, reversed),
        Err(QueryError::InvalidRange { .. })
    ));
    assert_eq!(
        summary.variables_page(S::Local, 310..310).unwrap().count(),
        0
    );
    assert!(matches!(
        summary.source_expressions_page(0..0),
        Err(QueryError::Unavailable { .. })
    ));
    assert!(matches!(
        view.valgrind_errors_page(0..0),
        Err(QueryError::Unavailable { .. })
    ));
}

#[test]
fn checkpoint_tagged_events_gap_windows_and_missing_stops_stay_distinct() {
    let mut t = trace();
    t.snapshots[0].observation.instruction_position = 1;
    t.snapshots.push(ExecutionSnapshot {
        step_index: 1,
        file: "scope.c".into(),
        line: 14,
        observation: SnapshotObservation {
            stop_id: 11,
            instruction_position: 2,
            pid: Some(42),
            complete: false,
            ..Default::default()
        },
        ..Default::default()
    });
    t.recording.instruction_count = 2;
    for (id, stop) in [(0, 7), (1, 9)] {
        t.recording.instruction_events.push(InstructionEvent {
            event_id: id,
            stop_id_before: stop,
            stop_id_after: Some(stop + 1),
            checkpoint_id: Some(0),
            address: "0x4000".into(),
            next_address: None,
            instruction_id: None,
            thread_id: None,
            pid: Some(42),
            source: SourceContext {
                file: Some("scope.c".into()),
                line: Some(12),
                function: Some("run".into()),
            },
            outcome: "completed".into(),
            stopped_reason: None,
            signal: None,
            registers: None,
            memory: vec![],
        });
    }
    for pos in [0, 1, 2] {
        t.recording.trace_gaps.push(TraceGap {
            instruction_position: pos,
            origin: "retention".into(),
            reason: "test".into(),
            start_address: None,
            end_address: None,
        });
    }
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let first = view.checkpoint_summary(0).unwrap();
    let second = view.checkpoint_summary(1).unwrap();
    assert_eq!(first.stop_id(), Some(7));
    assert_eq!(second.stop_id(), Some(11));
    assert_eq!(
        store.snapshot_at_stop(8).unwrap().map(|s| s.step_index),
        None
    ); // no invented stop
    assert_eq!(
        first
            .instruction_events_page(0..2)
            .unwrap()
            .map(|r| r.unwrap().position)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        first
            .instruction_events_page(0..2)
            .unwrap()
            .map(|r| r.unwrap().event.stop_id_before)
            .collect::<Vec<_>>(),
        vec![7, 9]
    );
    assert_eq!(second.instruction_events_page(0..0).unwrap().count(), 0);
    assert_eq!(
        first
            .gaps_in_attempted_window(0..2)
            .unwrap()
            .map(|r| r.unwrap().gap.instruction_position)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        second
            .gaps_in_attempted_window(0..1)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .gap
            .instruction_position,
        2
    );
    assert!(!second.observation_complete());
    assert_eq!(first.coverage().0.retained, 2);
    assert!(matches!(
        first.coverage().1.scope,
        c_trace_engine::trace_parser::views::HeapProcessScope::Unknown
    ));
    assert!(first.heap_interval_page(0..0).is_ok());
}

#[test]
fn independent_diagnostics_are_session_scoped_and_bounded() {
    let mut t = trace();
    t.execution_status = c_trace_engine::models::ExecutionStatus::Crashed;
    t.valgrind_report = Some(ValgrindReport {
        detailed_errors: (0..260)
            .map(|i| ValgrindError {
                kind: "InvalidRead".into(),
                related_allocation_ids: vec![i],
                source_site_candidates: vec![ValgrindSiteCandidate {
                    file: "scope.c".into(),
                    line: 12,
                    evidence: "same_source_site_independent_run_not_allocation_identity".into(),
                }],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    let summary = view.checkpoint_summary(0).unwrap();
    assert!(summary.session_diagnostics().crash.is_none()); // missing report does not negate crash
    assert!(summary.session_report_refs().crash.is_none());
    assert!(matches!(
        store
            .resolve_evidence(&summary.session_report_refs().valgrind.unwrap())
            .unwrap(),
        Evidence::ValgrindReport(_)
    ));
    assert_eq!(
        summary
            .session_diagnostics()
            .valgrind
            .unwrap()
            .detailed_errors
            .len(),
        260
    );
    assert!(matches!(
        view.valgrind_errors_page(0..129),
        Err(QueryError::LimitExceeded { .. })
    ));
    for range in [0..128, 128..256, 256..260] {
        for (offset, result) in view
            .valgrind_errors_page(range.clone())
            .unwrap()
            .enumerate()
        {
            let item = result.unwrap();
            assert_eq!(
                item.reference.location(),
                L::ValgrindError(range.start + offset)
            );
            assert_eq!(item.source_confidence, C::Candidate);
            assert!(item.primary_allocation_id.is_none());
            assert!(std::ptr::eq(
                item.error,
                &store.valgrind_report().unwrap().detailed_errors[range.start + offset]
            ));
        }
    }
}

#[test]
fn corrupt_import_findings_remain_visible_and_confidence_is_not_promoted() {
    let mut t = trace();
    let bad = &mut t.snapshots[0].variables[2];
    bad.target.as_mut().unwrap().kind = "heap".into();
    bad.target.as_mut().unwrap().variable_identity = None;
    bad.target.as_mut().unwrap().provenance_status = Some("address_only".into());
    bad.target.as_mut().unwrap().address_match_allocation_id = Some(9);
    bad.target.as_mut().unwrap().allocation_id = Some(9);
    let store = TraceStore::from_trace(t).unwrap(); // schema-only gate succeeds
    let view = store.validated_semantic();
    let summary = view.checkpoint_summary(0).unwrap();
    assert_eq!(summary.checkpoint.confidence, C::Contradictory);
    assert!(summary
        .first_location_findings()
        .any(|f| f.code == "POINTER_HEAP_PROVENANCE_CONTRADICTORY"));
    let pointer = summary.variable(S::Local, 2).unwrap();
    assert_eq!(pointer.pointer.unwrap().confidence, C::Contradictory);
    assert!(summary.session_diagnostics().crash.is_none());
    assert!(matches!(
        summary.coverage().1.scope,
        c_trace_engine::trace_parser::views::HeapProcessScope::Unknown
    ));
}

#[test]
fn large_event_pages_do_not_merge_stop_ids_or_unbounded_results() {
    let mut t = trace();
    t.recording.instruction_count = 300;
    t.snapshots[0].observation.instruction_position = 300;
    for i in 0..300 {
        t.recording.instruction_events.push(InstructionEvent {
            event_id: i,
            stop_id_before: 7 + i,
            stop_id_after: Some(8 + i),
            checkpoint_id: Some(0),
            address: "0x4000".into(),
            next_address: None,
            instruction_id: None,
            thread_id: None,
            pid: Some(42),
            source: SourceContext::default(),
            outcome: "completed".into(),
            stopped_reason: None,
            signal: None,
            registers: None,
            memory: vec![],
        });
    }
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let summary = view.checkpoint_summary(0).unwrap();
    assert!(matches!(
        summary.instruction_events_page(0..129),
        Err(QueryError::LimitExceeded { .. })
    ));
    for range in [0..128, 128..256, 256..300] {
        for (offset, result) in summary
            .instruction_events_page(range.clone())
            .unwrap()
            .enumerate()
        {
            let entry = result.unwrap();
            assert_eq!(entry.position, range.start + offset);
            assert_eq!(entry.event.stop_id_before, entry.position as u64 + 7);
            assert_eq!(
                entry.reference.location(),
                L::InstructionEvent(entry.position)
            );
            assert_eq!(entry.tag_confidence, C::Definite); // recorded tag, not exact stop
        }
    }
}

#[test]
fn legacy_v21_checkpoint_remains_borrowed_with_unknown_new_metadata() {
    let store = TraceStore::open(
        format!("{}/examples/v21_dual.json", env!("CARGO_MANIFEST_DIR")),
        Default::default(),
    )
    .unwrap();
    let view = store.validated_semantic();
    let summary = view.checkpoint_summary(0).unwrap();
    assert!(matches!(
        summary.coverage().1.scope,
        c_trace_engine::trace_parser::views::HeapProcessScope::Unknown
    ));
    assert!(matches!(
        summary.source_expressions_page(0..0),
        Err(QueryError::Unavailable { .. })
    ));
    assert!(std::ptr::eq(
        &store.trace().snapshots[0],
        match store.resolve_evidence(&summary.reference).unwrap() {
            Evidence::Snapshot(x) => x,
            _ => unreachable!(),
        }
    ));
    assert_eq!(
        summary.attempted_instruction_position(),
        store.trace().snapshots[0].observation.instruction_position
    );
}

#[tokio::test]
async fn live_recursive_checkpoint_and_ast_candidates_are_source_scoped() {
    use c_trace_engine::{trace::trace_program, TraceConfig};
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped live TP-7: {tool} unavailable");
            return;
        }
    }
    let mut c = TraceConfig::default();
    c.build.source_file = format!("{}/fixtures/c/recursive.c", env!("CARGO_MANIFEST_DIR")).into();
    c.limits.max_caller_frames = 3;
    c.limits.max_steps = 350;
    c.limits.timeout_seconds = 40;
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
    let store = TraceStore::from_trace(trace_program(c).await.unwrap()).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let mut observed_callers = false;
    for index in 0..store.trace().snapshots.len() {
        let summary = view.checkpoint_summary(index).unwrap();
        assert_eq!(summary.reference.location(), L::Snapshot(index));
        assert_eq!(
            summary.stop_id(),
            Some(store.trace().snapshots[index].observation.stop_id)
        );
        if !summary.checkpoint.context.caller_frames().is_empty() {
            let caller = summary.caller_frame(0).unwrap();
            let activation = summary
                .frame_activation(caller.frame.activation_id)
                .unwrap()
                .unwrap();
            assert_eq!(activation.scope, S::Caller(0));
            if let Some(info) = caller.frame.variables.first() {
                let item = activation
                    .variables_page(0..1)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap();
                assert!(std::ptr::eq(item.info, info));
                assert_eq!(item.info.activation_id, Some(caller.frame.activation_id));
            }
            observed_callers = true;
        }
    }
    assert!(observed_callers, "expected captured recursive caller frame");
    if c_trace_engine::compiler::probe_tool("clang", "--version")
        .await
        .is_ok()
    {
        let mut c = TraceConfig::default();
        c.build.source_file =
            format!("{}/fixtures/c/phase8b_macro.c", env!("CARGO_MANIFEST_DIR")).into();
        c.clang_ast = true;
        c.limits.max_steps = 250;
        c.limits.timeout_seconds = 40;
        c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
        let store = TraceStore::from_trace(trace_program(c).await.unwrap()).unwrap();
        let view = store.validated_semantic();
        assert!(view.report().is_structurally_sound(), "{:?}", view.report());
        let registry = store
            .trace()
            .source_metadata
            .as_ref()
            .expect("Clang available");
        assert!(!registry.heap_call_ambiguities.is_empty());
        let mut matched = false;
        for index in 0..store.trace().snapshots.len() {
            let summary = view.checkpoint_summary(index).unwrap();
            let Some((_, line, _)) = summary.source_location() else {
                continue;
            };
            let expected = registry
                .expressions
                .iter()
                .filter(|expr| expr.span.start_line <= line && line <= expr.span.end_line)
                .count();
            if expected == 0 {
                continue;
            }
            let count = expected.min(MAX_CONSUMER_PAGE);
            for source in summary.source_expressions_page(0..count).unwrap() {
                assert_eq!(source.source_match_confidence, C::Candidate);
                assert_eq!(source.expression.execution_confidence, C::Unavailable);
                assert!(source.expression.expression.unwrap().span.start_line <= line);
                matched = true;
            }
        }
        assert!(
            matched,
            "expected exact recorded frame fullname to match AST source"
        );
        let ambiguous =
            view.source_expression(&registry.heap_call_ambiguities[0].expression_ids[0]);
        assert_eq!(ambiguous.heap_call_confidence, C::Ambiguous);
        assert_eq!(ambiguous.execution_confidence, C::Unavailable);
    }
}

#[tokio::test]
async fn live_heap_reuse_realloc_crash_and_fork_keep_independent_scopes() {
    use c_trace_engine::{trace::trace_program, TraceConfig};
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped live TP-7: {tool} unavailable");
            return;
        }
    }
    for name in ["heap_provenance.c", "realloc_move.c"] {
        let mut c = TraceConfig::default();
        c.build.source_file = format!("{}/fixtures/c/{name}", env!("CARGO_MANIFEST_DIR")).into();
        c.limits.max_steps = 400;
        c.limits.timeout_seconds = 40;
        c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
        let store = TraceStore::from_trace(trace_program(c).await.unwrap()).unwrap();
        let view = store.validated_semantic();
        assert!(
            view.report().is_structurally_sound(),
            "{name}: {:?}",
            view.report()
        );
        let mut events = 0;
        let mut candidate = 0;
        for index in 0..store.trace().snapshots.len() {
            let summary = view.checkpoint_summary(index).unwrap();
            let end = summary
                .checkpoint
                .context
                .heap_events_in_snapshot_interval()
                .unwrap()
                .len();
            for item in summary.heap_interval_page(0..end).unwrap() {
                let item = item.unwrap();
                assert_eq!(item.reference.location(), L::HeapEvent(item.position));
                assert!(std::ptr::eq(
                    item.event,
                    &store.heap_events()[item.position]
                ));
                assert_eq!(item.interval_confidence, C::Candidate);
                assert_eq!(
                    item.allocation(&view).unwrap().allocation_id,
                    item.event.allocation_id
                );
                events += 1;
            }
            for item in summary
                .variables_page(S::Local, 0..summary.checkpoint.context.variables().len())
                .unwrap()
            {
                let item = item.unwrap();
                if let Some(p) = item.pointer {
                    if p.address_candidate.is_some() {
                        candidate += 1;
                        assert!(p.definitive_allocation.is_none());
                    }
                }
            }
        }
        assert_eq!(events, store.heap_events().len());
        if !store.heap_events().is_empty() {
            let allocation = store.evidence_ref(L::HeapEvent(0)).unwrap();
            let id = store.heap_events()[0].allocation_id;
            let len = store.heap_event_positions_for_allocation(id).len();
            for entry in view.heap_events_page(&allocation, 0..len).unwrap() {
                let entry = entry.unwrap();
                assert!(std::ptr::eq(
                    entry.event,
                    &store.heap_events()[entry.position]
                ));
                assert_eq!(entry.event.allocation_id, id);
                assert_eq!(entry.reference.location(), L::HeapEvent(entry.position));
                assert_eq!(entry.observed_confidence, C::Definite);
            }
        }
        assert!(candidate > 0, "{name}: expected numeric address candidates");
        if name == "realloc_move.c" {
            assert!(store
                .heap_state()
                .iter()
                .any(|b| b.realloc_from_id.is_some()));
            for block in store
                .heap_state()
                .iter()
                .filter(|b| b.realloc_from_id.is_some())
            {
                assert_eq!(view.heap(block.allocation_id).old_id, block.realloc_from_id);
            }
        }
    }
    let mut c = TraceConfig::default();
    c.build.source_file =
        format!("{}/fixtures/c/phase8r3_crash.c", env!("CARGO_MANIFEST_DIR")).into();
    c.limits.timeout_seconds = 40;
    c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
    let store = TraceStore::from_trace(trace_program(c).await.unwrap()).unwrap();
    let view = store.validated_semantic();
    let summary = view
        .checkpoint_summary(store.trace().snapshots.len() - 1)
        .unwrap();
    assert!(summary.session_diagnostics().crash.is_some());
    assert!(matches!(
        store
            .resolve_evidence(&summary.session_report_refs().crash.unwrap())
            .unwrap(),
        Evidence::CrashReport(_)
    ));
    assert!(matches!(
        store
            .resolve_evidence(&store.evidence_ref(L::CrashReport).unwrap())
            .unwrap(),
        Evidence::CrashReport(_)
    ));
    // A Valgrind pass is independent and may be unavailable; never attach it
    // as an exact cause for this checkpoint.
    if let Some(report) = store.valgrind_report() {
        for item in view
            .valgrind_errors_page(0..report.detailed_errors.len().min(MAX_CONSUMER_PAGE))
            .unwrap()
        {
            assert!(item.unwrap().primary_allocation_id.is_none());
        }
    }
    for (mode, count, complete) in [
        ("observed", 2, true),
        ("silent", 1, true),
        ("incomplete", 2, false),
    ] {
        let mut c = TraceConfig::default();
        c.build.source_file =
            format!("{}/fixtures/c/phase8dr_fork.c", env!("CARGO_MANIFEST_DIR")).into();
        c.program_args = vec![mode.into()];
        c.limits.max_steps = 300;
        c.limits.timeout_seconds = 40;
        c.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
        let store = TraceStore::from_trace(trace_program(c).await.unwrap()).unwrap();
        let view = store.validated_semantic();
        assert!(
            view.report().is_structurally_sound(),
            "{mode}: {:?}",
            view.report()
        );
        let summary = view.checkpoint_summary(0).unwrap();
        let (execution, coverage) = summary.coverage();
        let c_trace_engine::trace_parser::views::HeapProcessScope::VerifiedObservedPidsOnly {
            observed_event_pids,
            observed_history_complete,
            ..
        } = coverage.scope
        else {
            panic!("{mode}: missing scope")
        };
        assert_eq!(observed_event_pids.len(), count);
        assert_eq!(observed_history_complete, complete);
        assert_eq!(coverage.unobserved_descendants, C::Unavailable);
        assert!(!execution.coverage.scope.is_empty());
        if !complete {
            assert!(view.report().is_partial());
        }
    }
}

#[test]
fn large_untracked_allocation_history_pages_without_claiming_identity() {
    let mut t = trace();
    t.snapshots[0].observation.heap_event_count = 130;
    t.heap_events = (1..=130)
        .map(|sequence| HeapEvent {
            raw_caller_address: None,
            sequence,
            pid: 42,
            allocation_id: 0,
            event_type: HeapEventType::AllocationFailed,
            address: None,
            size_bytes: None,
            calloc_count: None,
            calloc_element_size: None,
            source_file: None,
            source_line: None,
            old_allocation_id: None,
            heap_usage_after: 0,
            heap_usage_complete: None,
            detail: None,
            attribution: None,
        })
        .collect();
    let other = TraceStore::from_trace(t.clone()).unwrap();
    let store = TraceStore::from_trace(t).unwrap();
    let view = store.validated_semantic();
    let reference = store.evidence_ref(L::HeapEvent(0)).unwrap();
    assert!(matches!(
        view.heap_events_page(&other.evidence_ref(L::HeapEvent(0)).unwrap(), 0..1),
        Err(QueryError::ForeignTrace)
    ));
    assert!(matches!(
        view.heap_events_page(&reference, 0..129),
        Err(QueryError::LimitExceeded { max: 128, .. })
    ));
    let mut positions = Vec::new();
    for range in [0..128, 128..130] {
        for item in view.heap_events_page(&reference, range).unwrap() {
            let item = item.unwrap();
            assert_eq!(item.event.allocation_id, 0);
            assert_eq!(item.history_confidence, C::Unavailable);
            assert_eq!(item.observed_confidence, C::Definite);
            positions.push(item.position);
        }
    }
    assert_eq!(positions, (0..130).collect::<Vec<_>>());
}

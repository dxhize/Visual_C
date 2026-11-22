//! TP-6 evidence boundary: no inferred execution, pointer origin or child census.
use c_trace_engine::models::{
    AllocationLifetime, DataValidity, DebugSessionTrace, ExecutionSnapshot, FrameVariables,
    HeapBlockInfo, HeapHistoryCoverage, MemoryEvidence, PointerTarget, ScopeStatus,
    SnapshotObservation, StackFrameInfo, ValgrindError, ValgrindReport, ValgrindSiteCandidate,
    VariableInfo,
};
use c_trace_engine::trace_parser::semantic::EvidenceConfidence as C;
use c_trace_engine::trace_parser::views::HeapProcessScope;
use c_trace_engine::trace_parser::{
    EvidenceLocation as L, QueryError, TraceLoadOptions, TraceStore,
};

fn base() -> DebugSessionTrace {
    let caller = StackFrameInfo {
        level: 1,
        func: "main".into(),
        address: Some("0x4000".into()),
        ..Default::default()
    };
    let mut trace = DebugSessionTrace::default();
    trace.recording.source_checkpoints.push(0);
    trace.snapshots.push(ExecutionSnapshot {
        observation: SnapshotObservation {
            stop_id: 1,
            pid: Some(42),
            complete: true,
            ..Default::default()
        },
        file: "scope.c".into(),
        line: 12,
        func: "run".into(),
        call_stack: vec![StackFrameInfo::default(), caller.clone()],
        caller_frames: vec![FrameVariables {
            frame: caller,
            activation_id: 3,
            ..Default::default()
        }],
        ..Default::default()
    });
    trace
}

fn variable(name: &str, id: &str, activation: u64) -> VariableInfo {
    VariableInfo {
        name: name.into(),
        identity: Some(id.into()),
        activation_id: Some(activation),
        read_status: DataValidity::Valid,
        value: "7".into(),
        scope_status: Some(ScopeStatus::Active),
        ..Default::default()
    }
}

#[test]
fn variables_memory_and_trace_local_identity_are_separate() {
    let mut trace = base();
    let mut first = variable("shadow", "activation:1:outer", 1);
    first.bytes_status = DataValidity::Truncated;
    first.raw_bytes_hex = Some("00010203".into());
    first.memory_evidence = Some(MemoryEvidence {
        requested_bytes: 8,
        captured_bytes: 4,
        extent_bytes: Some(8),
        status: "truncated".into(),
        reason: Some("capture limit".into()),
        ..Default::default()
    });
    trace.snapshots[0].variables.push(first);
    let mut inner = variable("shadow", "activation:2:inner", 2);
    inner.read_status = DataValidity::Unavailable;
    inner.value.clear();
    inner.memory_evidence = Some(MemoryEvidence {
        requested_bytes: 8,
        captured_bytes: 0,
        status: "unreadable".into(),
        reason: Some("GDB read failed".into()),
        ..Default::default()
    });
    trace.snapshots[0].variables.push(inner);
    let mut global = variable("global", "global:g", 0);
    global.activation_id = None;
    global.raw_bytes_hex = Some("07000000".into());
    global.bytes_status = DataValidity::Valid;
    global.memory_evidence = Some(MemoryEvidence {
        requested_bytes: 4,
        captured_bytes: 4,
        status: "full".into(),
        ..Default::default()
    });
    trace.snapshots[0].observation.globals.push(global);
    let mut caller = variable("caller", "activation:3:caller", 3);
    caller.raw_bytes_hex = Some("01020304".into());
    caller.bytes_status = DataValidity::Valid;
    caller.memory_evidence = Some(MemoryEvidence {
        requested_bytes: 4,
        captured_bytes: 4,
        status: "full".into(),
        ..Default::default()
    });
    trace.snapshots[0].caller_frames[0].variables.push(caller);
    let store = TraceStore::from_trace(trace.clone()).unwrap();
    let clone = store.clone();
    let evidence = store.semantic_input().validated();
    assert!(
        evidence.report().is_structurally_sound(),
        "{:?}",
        evidence.report()
    );
    let ctx = evidence.checkpoint(0).unwrap();
    assert_eq!(ctx.location(), Some(("scope.c", 12, "run")));
    assert_eq!(ctx.observation.stop_id, 1);
    for (loc, expected, confidence) in [
        (
            L::SnapshotVariable {
                snapshot: 0,
                variable: 0,
            },
            "activation:1:outer",
            C::Candidate,
        ),
        (
            L::SnapshotVariable {
                snapshot: 0,
                variable: 1,
            },
            "activation:2:inner",
            C::Unavailable,
        ),
        (
            L::SnapshotGlobal {
                snapshot: 0,
                variable: 0,
            },
            "global:g",
            C::Definite,
        ),
        (
            L::SnapshotCallerVariable {
                snapshot: 0,
                frame: 0,
                variable: 0,
            },
            "activation:3:caller",
            C::Definite,
        ),
    ] {
        let reference = clone.evidence_ref(loc).unwrap();
        let v = evidence.variable(&reference).unwrap();
        assert_eq!(v.info.identity.as_deref(), Some(expected));
        assert_eq!(v.memory.confidence, confidence);
        assert_eq!(v.type_confidence, C::Unavailable);
        assert!(std::ptr::eq(
            v.info,
            match clone.resolve_evidence(&reference).unwrap() {
                c_trace_engine::trace_parser::Evidence::SnapshotVariable(x)
                | c_trace_engine::trace_parser::Evidence::SnapshotGlobal(x)
                | c_trace_engine::trace_parser::Evidence::SnapshotCallerVariable(x) => x,
                _ => panic!("wrong evidence"),
            }
        ));
        match loc {
            L::SnapshotVariable { variable: 0, .. } => {
                assert_eq!(v.memory.valid_range, Some(0..4));
                assert_eq!(v.memory.unobserved_range, Some(4..8));
                assert_eq!(
                    v.memory.observation.unwrap().reason.as_deref(),
                    Some("capture limit")
                );
            }
            L::SnapshotVariable { variable: 1, .. } => {
                assert_eq!(v.value_confidence, C::Unavailable);
                assert!(v.memory.valid_range.is_none());
                assert_eq!(v.memory.unobserved_range, Some(0..8));
            }
            _ => assert_eq!(v.memory.valid_range, Some(0..4)),
        }
    }
    let other = TraceStore::from_trace(trace).unwrap();
    let foreign = other
        .evidence_ref(L::SnapshotVariable {
            snapshot: 0,
            variable: 0,
        })
        .unwrap();
    assert!(matches!(
        evidence.variable(&foreign),
        Err(QueryError::ForeignTrace)
    ));
}

#[test]
fn pointers_candidates_aliases_and_bad_imports_never_become_definite_heap_origin() {
    let mut trace = base();
    let target = variable("pointee", "activation:1:target", 1);
    trace.snapshots[0].variables.push(target);
    for (status, expected) in [
        ("address_only", C::Candidate),
        ("known_freed_address", C::Candidate),
        ("ambiguous_reuse", C::Ambiguous),
        ("history_incomplete", C::Unavailable),
    ] {
        let mut p = variable("alias", "activation:1:alias", 1);
        p.points_to = Some("0x7000".into());
        p.target = Some(PointerTarget {
            kind: "heap".into(),
            base_address: Some("0x7000".into()),
            address_match_allocation_id: Some(8),
            provenance_status: Some(status.into()),
            ..Default::default()
        });
        trace.snapshots[0].variables.push(p);
        let store = TraceStore::from_trace(trace.clone()).unwrap();
        let view = store.validated_semantic();
        let r = store
            .evidence_ref(L::SnapshotVariable {
                snapshot: 0,
                variable: 1,
            })
            .unwrap();
        let v = view.variable(&r).unwrap();
        let p = v.pointer.unwrap();
        assert_eq!(p.confidence, expected);
        assert_eq!(p.address, Some("0x7000"));
        assert_eq!(p.address_candidate, Some(8));
        assert_eq!(p.definitive_allocation, None);
        trace.snapshots[0].variables.pop();
    }
    let mut p = variable("alias", "activation:1:alias", 1);
    p.points_to = Some("0x1000".into());
    p.target = Some(PointerTarget {
        kind: "variable".into(),
        variable_identity: Some("activation:1:target".into()),
        ..Default::default()
    });
    trace.snapshots[0].variables.push(p);
    let store = TraceStore::from_trace(trace.clone()).unwrap();
    let view = store.validated_semantic();
    let p = view
        .variable(
            &store
                .evidence_ref(L::SnapshotVariable {
                    snapshot: 0,
                    variable: 1,
                })
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        view.variable_target(&p)
            .unwrap()
            .record
            .unwrap()
            .identity
            .as_deref(),
        Some("activation:1:target")
    );
    assert_eq!(view.variable_target(&p).unwrap().confidence, C::Definite);
    trace.snapshots[0].variables[1]
        .target
        .as_mut()
        .unwrap()
        .provenance_status = Some("address_only".into());
    trace.snapshots[0].variables[1]
        .target
        .as_mut()
        .unwrap()
        .allocation_id = Some(8);
    let bad = TraceStore::from_trace(trace).unwrap(); // schema gate does not validate
    let bad_view = bad.validated_semantic();
    assert!(!bad_view.report().is_structurally_sound());
    let p = bad_view
        .variable(
            &bad.evidence_ref(L::SnapshotVariable {
                snapshot: 0,
                variable: 1,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(p.pointer.as_ref().unwrap().confidence, C::Contradictory);
    assert_eq!(
        bad_view.variable_target(&p).unwrap().confidence,
        C::Contradictory
    );
    assert!(p.pointer.unwrap().definitive_allocation.is_none());
}

#[test]
fn lifetimes_coverage_static_calls_and_independent_diagnostics_keep_scope() {
    let mut trace = base();
    trace.recording.heap_history = Some(HeapHistoryCoverage {
        complete: true,
        process_scope: Some("verified_observed_pids_only".into()),
        unobserved_process_coverage: Some("unknown".into()),
        followed_inferior_pid: Some(42),
        observed_event_pids: vec![42],
        ..Default::default()
    });
    trace
        .recording
        .allocation_lifetimes
        .push(AllocationLifetime {
            allocation_id: 11,
            pid: 42,
            allocation_sequence: 1,
            ..Default::default()
        });
    trace
        .recording
        .allocation_lifetimes
        .push(AllocationLifetime {
            allocation_id: 12,
            pid: 42,
            allocation_sequence: 2,
            ..Default::default()
        });
    trace.heap_state.push(HeapBlockInfo {
        allocation_id: 12,
        realloc_from_id: Some(11),
        address: "0x7000".into(),
        ..Default::default()
    });
    trace.valgrind_report = Some(ValgrindReport {
        detailed_errors: vec![ValgrindError {
            kind: "InvalidRead".into(),
            related_allocation_ids: vec![11],
            source_site_candidates: vec![ValgrindSiteCandidate {
                file: "scope.c".into(),
                line: 12,
                evidence: "same_source_site_independent_run_not_allocation_identity".into(),
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    let store = TraceStore::from_trace(trace.clone()).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let heap = view.heap(12);
    assert_eq!(heap.identity_confidence, C::Definite);
    let reference = store.evidence_ref(L::AllocationLifetime(1)).unwrap();
    assert_eq!(view.heap_reference(&reference).unwrap().allocation_id, 12);
    let foreign = TraceStore::from_trace(trace.clone()).unwrap();
    let foreign_reference = foreign.evidence_ref(L::AllocationLifetime(1)).unwrap();
    assert!(matches!(
        view.heap_reference(&foreign_reference),
        Err(QueryError::ForeignTrace)
    ));
    // Both recorded IDs exist, but there is no realloc event. Even a scoped
    // complete-history annotation cannot establish a predecessor by itself.
    assert_eq!(heap.realloc_confidence, C::Unavailable);
    assert_eq!(heap.old_id, Some(11));
    assert_eq!(heap.old_lifetime.unwrap().pid, 42);
    assert_eq!(heap.events().count(), 0); // no events invented from final state
    let diag = view.valgrind_error(0).unwrap();
    assert_eq!(diag.source_confidence, C::Candidate);
    assert_eq!(diag.primary_allocation_id, None); // even with imported legacy IDs
    assert_eq!(diag.error.source_site_candidates[0].line, 12);
    assert_eq!(
        view.diagnostics().valgrind.unwrap().detailed_errors.len(),
        1
    );
    let coverage = view.process_coverage();
    assert!(matches!(
        coverage.scope,
        HeapProcessScope::VerifiedObservedPidsOnly {
            observed_event_pids: [42],
            observed_history_complete: true,
            ..
        }
    ));
    assert_eq!(coverage.unobserved_descendants, C::Unavailable);
    trace.recording.heap_history.as_mut().unwrap().complete = false;
    assert_eq!(
        TraceStore::from_trace(trace.clone())
            .unwrap()
            .validated_semantic()
            .heap(12)
            .identity_confidence,
        C::Candidate
    );
    trace.recording.heap_history.as_mut().unwrap().process_scope = None;
    let legacy = TraceStore::from_trace(trace).unwrap();
    assert!(matches!(
        legacy.validated_semantic().process_coverage().scope,
        HeapProcessScope::Unknown
    ));
    let old = TraceStore::open(
        format!("{}/examples/v21_normal.json", env!("CARGO_MANIFEST_DIR")),
        TraceLoadOptions::default(),
    )
    .unwrap();
    let old_view = old.validated_semantic();
    assert!(matches!(
        old_view.process_coverage().scope,
        HeapProcessScope::Unknown
    ));
    assert_eq!(
        old_view.source_expression("missing").execution_confidence,
        C::Unavailable
    );
}

#[tokio::test]
async fn live_v22_ast_on_off_and_reused_heap_evidence_remain_uncertain() {
    use c_trace_engine::{trace::trace_program, TraceConfig};
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped live V2.2 test: {tool} unavailable");
            return;
        }
    }
    let source = format!("{}/fixtures/c/phase8b_macro.c", env!("CARGO_MANIFEST_DIR"));
    let clang = c_trace_engine::compiler::probe_tool("clang", "--version")
        .await
        .is_ok();
    for ast in [false, true] {
        let mut config = TraceConfig::default();
        config.build.source_file = source.clone().into();
        config.clang_ast = ast;
        config.limits.max_steps = 250;
        config.limits.timeout_seconds = 35;
        config.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
        let trace = trace_program(config).await.unwrap();
        let store = TraceStore::from_trace(trace).unwrap();
        let view = store.validated_semantic();
        assert!(view.report().is_structurally_sound(), "{:?}", view.report());
        assert!(view.execution().retained > 0);
        assert!(view.process_coverage().unobserved_descendants == C::Unavailable);
        assert!(!store.heap_events().is_empty());
        if let Some(registry) = store.type_registry() {
            if let Some(entry) = registry.types.first() {
                assert!(std::ptr::eq(store.type_entry(&entry.id).unwrap(), entry));
                assert!(std::ptr::eq(
                    view.store().type_entry(&entry.id).unwrap(),
                    entry
                ));
                let mut invalid = store.trace().clone();
                invalid
                    .type_registry
                    .as_mut()
                    .unwrap()
                    .types
                    .push(entry.clone());
                let imported = TraceStore::from_trace(invalid).unwrap();
                let rejected = imported.validated_semantic();
                assert!(rejected
                    .report()
                    .findings()
                    .iter()
                    .any(|f| f.code == "DWARF_DUPLICATE_ID"));
                assert_eq!(rejected.checkpoint(0).unwrap().confidence, C::Contradictory);
            }
        }
        for snapshot in view.store().semantic_input().snapshots() {
            for i in 0..snapshot.variables().len() {
                let var = view
                    .variable(&snapshot.variable_evidence(i).unwrap())
                    .unwrap();
                if let Some(id) = var.info.type_ref.as_deref() {
                    assert!(std::ptr::eq(
                        var.type_entry.unwrap(),
                        store.type_entry(id).unwrap()
                    ));
                    assert_eq!(var.type_confidence, C::Definite);
                }
                if let Some(pointer) = var.pointer {
                    assert_eq!(pointer.definitive_allocation, None);
                }
            }
        }
        if ast && clang {
            let registry = view
                .store()
                .trace()
                .source_metadata
                .as_ref()
                .expect("available Clang must retain AST");
            assert!(!registry.heap_call_candidates.is_empty());
            if let Some(declaration) = registry.declarations.first() {
                assert!(std::ptr::eq(
                    store.source_declaration(&declaration.id).unwrap(),
                    declaration
                ));
            }
            assert!(!registry.heap_call_ambiguities.is_empty());
            let candidate = view.source_expression(&registry.heap_call_candidates[0].expression_id);
            assert_eq!(candidate.heap_call_confidence, C::Candidate);
            assert_eq!(candidate.execution_confidence, C::Unavailable);
            assert_eq!(candidate.heap_call_candidates().count(), 1);
            let call = candidate.call_sites().next().unwrap();
            assert!(std::ptr::eq(
                call.event,
                &view.store().heap_events()[call.event_position]
            ));
            assert_eq!(call.confidence, C::Candidate);
            let ambiguous =
                view.source_expression(&registry.heap_call_ambiguities[0].expression_ids[0]);
            assert_eq!(ambiguous.heap_call_confidence, C::Ambiguous);
            assert_eq!(ambiguous.execution_confidence, C::Unavailable);
            assert_eq!(
                ambiguous.ambiguous_call_sites().next().unwrap().confidence,
                C::Ambiguous
            );
        } else if !ast {
            assert!(view.store().trace().source_metadata.is_none());
            assert_eq!(
                view.source_expression("missing").execution_confidence,
                C::Unavailable
            );
        }
    }
}

#[test]
fn crash_gap_and_unsupported_fork_claim_are_not_execution_proof() {
    let mut trace = base();
    trace.execution_status = c_trace_engine::models::ExecutionStatus::Crashed;
    trace.recording.instruction_count = 1;
    trace
        .recording
        .trace_gaps
        .push(c_trace_engine::models::TraceGap {
            instruction_position: 1,
            origin: "recording".into(),
            reason: "deadline".into(),
            start_address: None,
            end_address: None,
        });
    trace.recording.heap_history = Some(HeapHistoryCoverage {
        complete: false,
        process_scope: Some("verified_observed_pids_only".into()),
        unobserved_process_coverage: Some("unknown".into()),
        followed_inferior_pid: Some(42),
        observed_event_pids: vec![42, 43],
        ..Default::default()
    });
    let store = TraceStore::from_trace(trace.clone()).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    assert!(view.report().is_partial());
    assert_eq!(view.execution().attempted, 1);
    assert_eq!(view.execution().retained, 0);
    assert_eq!(view.execution().gaps[0].instruction_position, 1);
    assert!(view.diagnostics().crash.is_none()); // absent report != no crash
    assert!(matches!(
        view.process_coverage().scope,
        HeapProcessScope::VerifiedObservedPidsOnly {
            observed_event_pids: [42, 43],
            observed_history_complete: false,
            ..
        }
    ));
    assert_eq!(
        view.process_coverage().unobserved_descendants,
        C::Unavailable
    );
    trace
        .recording
        .heap_history
        .as_mut()
        .unwrap()
        .unobserved_process_coverage = Some("complete_all_processes".into());
    let imported = TraceStore::from_trace(trace).unwrap();
    let view = imported.validated_semantic();
    assert!(!view.report().is_structurally_sound());
    assert_eq!(
        view.process_coverage().unobserved_descendants,
        C::Contradictory
    );
    assert_eq!(view.checkpoint(0).unwrap().confidence, C::Contradictory);
}

#[tokio::test]
async fn live_v22_reused_address_keeps_candidates_distinct_from_lifetimes() {
    use c_trace_engine::{trace::trace_program, TraceConfig};
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped live V2.2 test: {tool} unavailable");
            return;
        }
    }
    let mut config = TraceConfig::default();
    config.build.source_file = format!(
        "{}/fixtures/c/heap_provenance.c",
        env!("CARGO_MANIFEST_DIR")
    )
    .into();
    config.limits.timeout_seconds = 40;
    config.limits.max_steps = 400;
    config.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
    let store = TraceStore::from_trace(trace_program(config).await.unwrap()).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let mut reuse = 0;
    let mut freed = 0;
    for context in store.semantic_input().snapshots() {
        for index in 0..context.variables().len() {
            let v = view
                .variable(&context.variable_evidence(index).unwrap())
                .unwrap();
            let Some(pointer) = v.pointer else { continue };
            match pointer.provenance_status {
                Some("ambiguous_reuse") => {
                    reuse += 1;
                    assert_eq!(pointer.confidence, C::Ambiguous);
                }
                Some("known_freed_address") => {
                    freed += 1;
                    assert_eq!(pointer.confidence, C::Candidate);
                }
                _ => {}
            }
            assert_eq!(pointer.definitive_allocation, None);
        }
    }
    assert!(reuse > 0 && freed > 0, "no observed reuse/freed candidates");
    for lifetime in store.allocation_lifetimes() {
        let heap = view.heap(lifetime.allocation_id);
        assert_eq!(heap.lifetime.unwrap().pid, lifetime.pid);
        assert_eq!(
            heap.events()
                .filter(|event| event.pid == lifetime.pid)
                .count(),
            heap.events().count()
        );
    }
}

#[tokio::test]
async fn live_v22_observed_and_silent_children_never_certify_descendants() {
    use c_trace_engine::{trace::trace_program, TraceConfig};
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped live fork evidence: {tool} unavailable");
            return;
        }
    }
    for (mode, pid_count, complete) in [
        ("observed", 2, true),
        ("silent", 1, true),
        ("incomplete", 2, false),
    ] {
        let mut config = TraceConfig::default();
        config.build.source_file =
            format!("{}/fixtures/c/phase8dr_fork.c", env!("CARGO_MANIFEST_DIR")).into();
        config.program_args = vec![mode.into()];
        config.limits.timeout_seconds = 40;
        config.limits.max_steps = 300;
        config.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
        let store = TraceStore::from_trace(trace_program(config).await.unwrap()).unwrap();
        let view = store.validated_semantic();
        assert!(
            view.report().is_structurally_sound(),
            "{mode}: {:?}",
            view.report()
        );
        let coverage = view.process_coverage();
        let HeapProcessScope::VerifiedObservedPidsOnly {
            observed_event_pids,
            followed_inferior_pid,
            observed_history_complete,
        } = coverage.scope
        else {
            panic!("{mode}: missing observed-PID scope");
        };
        assert_eq!(observed_event_pids.len(), pid_count, "{mode}");
        assert!(
            observed_event_pids.contains(&followed_inferior_pid.unwrap()),
            "{mode}"
        );
        assert_eq!(observed_history_complete, complete, "{mode}");
        assert_eq!(coverage.unobserved_descendants, C::Unavailable, "{mode}");
        assert_eq!(
            coverage
                .history
                .unwrap()
                .unobserved_process_coverage
                .as_deref(),
            Some("unknown")
        );
        if !complete {
            assert!(
                view.report().is_partial(),
                "missing child ACK must downgrade history"
            );
        }
    }
}

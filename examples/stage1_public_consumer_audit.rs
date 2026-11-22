//! Stage 1 freeze audit: minimal read-only Stage 2-shaped consumer using only
//! public Trace Parser APIs. Usage: cargo run --release --example
//! stage1_public_consumer_audit -- /path/to/engine-output.json
use c_trace_engine::trace_parser::{
    EvidenceConfidence, EvidenceLocation, QueryError, TraceLoadOptions, TraceStore, VariableScope,
    MAX_CONSUMER_PAGE,
};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("supply one engine JSON document")?;
    let store = TraceStore::open(&path, TraceLoadOptions::default())?;
    let view = store.validated_semantic(); // exactly once; loading alone is not validation
    let findings: Vec<_> = view.report().findings().iter().map(|f| f.code).collect();
    println!(
        "file={path} schema={} sound={} partial={} findings={findings:?}",
        store.trace().schema_version,
        view.report().is_structurally_sound(),
        view.report().is_partial()
    );
    let (execution, processes) = (view.execution(), view.process_coverage());
    println!("outcome={:?} attempted={} retained={} gaps={} child_scope={:?} unobserved_descendants={:?}",
        execution.status, execution.attempted, execution.retained, execution.gaps.len(),
        processes.scope, processes.unobserved_descendants);
    assert_eq!(
        execution.retained,
        store.trace().recording.instruction_events.len()
    );
    let mut locals = 0;
    let mut globals = 0;
    let mut caller_variables = 0;
    let mut page_events = 0;
    let mut heap_intervals = 0;
    let mut pointer_candidates = 0;
    let mut pointers_ambiguous = 0;
    let mut realloc_definite = 0;
    for index in 0..store.trace().snapshots.len().min(6) {
        let summary = view.checkpoint_summary(index)?;
        assert_eq!(
            summary.reference.location(),
            EvidenceLocation::Snapshot(index)
        );
        assert_eq!(
            summary.attempted_instruction_position(),
            store.trace().snapshots[index]
                .observation
                .instruction_position
        );
        for scope in [VariableScope::Local, VariableScope::Global] {
            let len = match scope {
                VariableScope::Local => summary.checkpoint.context.variables().len(),
                _ => summary.checkpoint.context.globals().len(),
            };
            for start in (0..len).step_by(MAX_CONSUMER_PAGE) {
                for item in
                    summary.variables_page(scope, start..(start + MAX_CONSUMER_PAGE).min(len))?
                {
                    let var = item?;
                    match scope {
                        VariableScope::Local => locals += 1,
                        _ => globals += 1,
                    };
                    if let Some(pointer) = &var.pointer {
                        if pointer.address_candidate.is_some() {
                            pointer_candidates += 1;
                        }
                        if pointer.confidence == EvidenceConfidence::Ambiguous {
                            pointers_ambiguous += 1;
                        }
                        // Containment is never a provenance claim, even for exact addresses.
                        assert!(pointer.definitive_allocation.is_none());
                        if var
                            .info
                            .target
                            .as_ref()
                            .and_then(|t| t.variable_identity.as_ref())
                            .is_some()
                        {
                            let resolved = view.variable_target(&var)?;
                            assert!(matches!(
                                resolved.confidence,
                                EvidenceConfidence::Definite
                                    | EvidenceConfidence::Candidate
                                    | EvidenceConfidence::Ambiguous
                                    | EvidenceConfidence::Unavailable
                                    | EvidenceConfidence::Contradictory
                            ));
                        }
                    }
                    if var.memory.valid_range.is_some() {
                        assert!(var.memory.observation.is_some());
                    }
                }
            }
        }
        for (frame, call) in summary
            .checkpoint
            .context
            .caller_frames()
            .iter()
            .enumerate()
        {
            let recorded = summary.caller_frame(frame)?;
            assert!(std::ptr::eq(recorded.frame, call));
            if let Some(activation) = summary.frame_activation(call.activation_id)? {
                for start in (0..activation.variables.len()).step_by(MAX_CONSUMER_PAGE) {
                    for item in activation.variables_page(
                        start..(start + MAX_CONSUMER_PAGE).min(activation.variables.len()),
                    )? {
                        let var = item?;
                        caller_variables += 1;
                        assert_eq!(var.info.activation_id, Some(call.activation_id));
                    }
                }
            }
        }
        let event_len = summary.checkpoint.context.events_at_checkpoint()?.len();
        for start in (0..event_len).step_by(MAX_CONSUMER_PAGE) {
            for item in summary
                .instruction_events_page(start..(start + MAX_CONSUMER_PAGE).min(event_len))?
            {
                let item = item?;
                page_events += 1;
                assert_eq!(
                    item.reference.location(),
                    EvidenceLocation::InstructionEvent(item.position)
                );
                assert_eq!(item.event.checkpoint_id, Some(index));
            }
        }
        let heap_len = match summary
            .checkpoint
            .context
            .heap_events_in_snapshot_interval()
        {
            Ok(events) => events.len(),
            Err(QueryError::Unavailable { .. }) => 0,
            Err(error) => return Err(error.into()),
        };
        for start in (0..heap_len).step_by(MAX_CONSUMER_PAGE) {
            for item in
                summary.heap_interval_page(start..(start + MAX_CONSUMER_PAGE).min(heap_len))?
            {
                let item = item?;
                heap_intervals += 1;
                let allocation = item.allocation(&view)?;
                assert_eq!(allocation.allocation_id, item.event.allocation_id);
                if allocation.realloc_confidence == EvidenceConfidence::Definite {
                    realloc_definite += 1;
                }
            }
        }
        let refs = summary.session_report_refs();
        if let Some(reference) = refs.crash {
            assert_eq!(reference.location(), EvidenceLocation::CrashReport);
        }
        let _ = (
            summary.coverage(),
            summary.session_diagnostics(),
            summary.first_location_findings().count(),
        );
        // Optional AST evidence: no match is not a negative execution claim.
        match summary.source_expressions_page(0..0) {
            Ok(page) => assert_eq!(page.count(), 0),
            Err(QueryError::Unavailable { .. }) => {}
            Err(error) => return Err(error.into()),
        };
    }
    let diag = view.diagnostics();
    let valgrind = diag
        .valgrind
        .map_or(0, |report| report.detailed_errors.len());
    for start in (0..valgrind).step_by(MAX_CONSUMER_PAGE) {
        for item in view.valgrind_errors_page(start..(start + MAX_CONSUMER_PAGE).min(valgrind))? {
            assert_eq!(item?.primary_allocation_id, None); // independently run tool
        }
    }
    // A trace-scoped allocation reference, not a numeric address or Valgrind ID.
    if !store.heap_events().is_empty() {
        let reference = store.evidence_ref(EvidenceLocation::HeapEvent(0))?;
        let linked = view.heap_reference(&reference)?;
        let len = store
            .heap_event_positions_for_allocation(linked.allocation_id)
            .len();
        for start in (0..len).step_by(MAX_CONSUMER_PAGE) {
            for item in
                view.heap_events_page(&reference, start..(start + MAX_CONSUMER_PAGE).min(len))?
            {
                assert_eq!(item?.event.allocation_id, linked.allocation_id);
            }
        }
    }
    println!("sampled_checkpoints={} locals={locals} globals={globals} caller_variables={caller_variables} instruction_events={page_events} heap_interval_events={heap_intervals} pointer_candidates={pointer_candidates} ambiguous_pointers={pointers_ambiguous} definite_realloc_items={realloc_definite} crash={} sanitizer={} valgrind_errors={valgrind}",
        store.trace().snapshots.len().min(6),diag.crash.is_some(),diag.sanitizer.is_some());
    Ok(())
}

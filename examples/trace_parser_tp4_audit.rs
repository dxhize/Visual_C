//! Read-only TP-4 compatibility probe. Usage: cargo run --release --example
//! trace_parser_tp4_audit -- TRACE.json [TRACE.json ...]
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore, ValidationSeverity};
use serde_json::Value;

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        let store = TraceStore::from_json_bytes(&bytes, TraceLoadOptions::default()).unwrap();
        let trace = store.trace();
        let typed = serde_json::to_value(trace).unwrap();
        for field in [
            "heap_history",
            "evidence_fusion",
            "source_metadata",
            "type_registry",
            "valgrind_report",
            "execution_limits",
        ] {
            if raw.get(field).is_some() {
                assert_eq!(
                    raw.get(field),
                    typed.get(field),
                    "{path}: {field} lost in model round-trip"
                );
            }
        }
        assert_eq!(
            raw["snapshots"].as_array().unwrap().len(),
            trace.snapshots.len()
        );
        assert_eq!(
            raw["heap_events"].as_array().unwrap().len(),
            store.heap_events().len()
        );
        assert_eq!(
            raw["instruction_events"].as_array().unwrap().len(),
            store.retained_event_count()
        );
        let report = store.validate_deep();
        let codes: Vec<_> = report
            .findings()
            .iter()
            .map(|f| format!("{:?}:{}:{}", f.severity, f.code, f.occurrences))
            .collect();
        if path.ends_with("/reuse.json") {
            let mut imported = trace.clone();
            let target = imported
                .snapshots
                .iter_mut()
                .flat_map(|s| &mut s.variables)
                .filter_map(|v| v.target.as_mut())
                .find(|t| {
                    t.provenance_status.as_deref() == Some("address_only")
                        && t.address_match_allocation_id.is_some()
                })
                .unwrap();
            target.allocation_id = target.address_match_allocation_id;
            let mutated = TraceStore::from_trace(imported).unwrap();
            let findings = mutated.validate_deep();
            println!(
                "ADVERSARIAL address_only plus definite_id: structural={:?} codes={:?}",
                findings
                    .findings()
                    .iter()
                    .any(|f| f.severity == ValidationSeverity::StructuralCorruption),
                findings
                    .findings()
                    .iter()
                    .map(|f| f.code)
                    .collect::<Vec<_>>()
            );
        }
        if path.ends_with("/fork-silent.json") {
            let mut imported = trace.clone();
            imported
                .recording
                .heap_history
                .as_mut()
                .unwrap()
                .unobserved_process_coverage = Some("complete_all_processes".into());
            let mutated = TraceStore::from_trace(imported).unwrap();
            println!(
                "ADVERSARIAL false multiprocess coverage: structural={:?}",
                mutated
                    .validate_deep()
                    .findings()
                    .iter()
                    .any(|f| f.severity == ValidationSeverity::StructuralCorruption)
            );
        }
        println!("{path} version={} snapshots={} events={} heap={} caller_frames={} ast={} fork_scope={:?} unknown_process={:?} valgrind_candidates={} definitive_links={} structural={} findings={codes:?}",
            trace.schema_version, trace.snapshots.len(), store.retained_event_count(), store.heap_events().len(),
            trace.snapshots.iter().map(|s| s.caller_frames.len()).sum::<usize>(), trace.source_metadata.is_some(),
            trace.recording.heap_history.as_ref().and_then(|h| h.process_scope.as_deref()),
            trace.recording.heap_history.as_ref().and_then(|h| h.unobserved_process_coverage.as_deref()),
            trace.valgrind_report.as_ref().map(|v| v.detailed_errors.iter().map(|d| d.source_site_candidates.len()).sum::<usize>()).unwrap_or(0),
            trace.valgrind_report.as_ref().map(|v| v.detailed_errors.iter().map(|d| d.related_allocation_ids.len()).sum::<usize>()).unwrap_or(0),
            report.findings().iter().any(|f| f.severity == ValidationSeverity::StructuralCorruption));
    }
}

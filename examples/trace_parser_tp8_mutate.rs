//! Deterministic bounded import challenge over a fresh engine corpus. Writes
//! minimized mutation diffs as full JSON inputs to /tmp/cte-tp8/mutations.
use c_trace_engine::trace_parser::{
    parse_journal_bytes, EvidenceConfidence, QueryError, TraceLoadOptions, TraceStore,
    VariableScope,
};
use serde_json::Value;
use std::fs;

fn challenge(name: &str, original: &Value, f: impl FnOnce(&mut Value)) {
    let mut modified = original.clone();
    f(&mut modified);
    let text = serde_json::to_vec(&modified).unwrap();
    let path = format!("/tmp/cte-tp8/mutations/{name}.json");
    fs::write(&path, &text).unwrap();
    match TraceStore::from_json_bytes(&text, TraceLoadOptions::default()) {
        Err(e) => println!("{name}: import_err={e}"),
        Ok(store) => {
            let v = store.validated_semantic();
            let codes: Vec<_> = v.report().findings().iter().map(|f| f.code).collect();
            let first = v.checkpoint_summary(0).unwrap();
            println!(
                "{name}: sound={} partial={} stop_conf={:?} children={:?} findings={codes:?}",
                v.report().is_structurally_sound(),
                v.report().is_partial(),
                first.checkpoint.confidence,
                v.process_coverage().unobserved_descendants
            );
            if name == "realloc-fabricated-existing" {
                assert!(
                    !v.report().is_structurally_sound(),
                    "TP8-F01 must be structural corruption"
                );
                assert_eq!(
                    v.heap(1).realloc_confidence,
                    EvidenceConfidence::Contradictory
                );
            }
            if name.starts_with("realloc-") {
                let heap = v.heap(1);
                println!("{name}: raw_event_1_type={:?} raw_event_1_old={:?} imported_block_1_old={:?} semantic_old={:?} semantic_realloc_conf={:?}", store.heap_events()[0].event_type, store.heap_events()[0].old_allocation_id, store.heap_block(1).and_then(|b| b.realloc_from_id),heap.old_id,heap.realloc_confidence);
            }
            if !v.report().is_structurally_sound() {
                assert_eq!(
                    first.checkpoint.confidence,
                    EvidenceConfidence::Contradictory
                );
            }
            let n = first.checkpoint.context.variables().len();
            let reversed = std::hint::black_box(1)..std::hint::black_box(0);
            assert!(matches!(
                first.variables_page(VariableScope::Local, reversed),
                Err(QueryError::InvalidRange { .. })
            ));
            assert!(matches!(
                first.variables_page(VariableScope::Local, n..n.saturating_add(1)),
                Err(QueryError::InvalidRange { .. })
            ));
        }
    }
}
fn main() {
    fs::create_dir_all("/tmp/cte-tp8/mutations").unwrap();
    let small: Value =
        serde_json::from_slice(&fs::read("/tmp/cte-tp8/small.json").unwrap()).unwrap();
    let fork: Value = serde_json::from_slice(&fs::read("/tmp/cte-tp8/fork.json").unwrap()).unwrap();
    challenge("address-only-definite", &small, |x| {
        let v = x["snapshots"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .flat_map(|s| s["variables"].as_array_mut().unwrap())
            .find(|v| v["target"]["provenance_status"] == "address_only")
            .expect("real pointer candidate");
        v["target"]["allocation_id"] = serde_json::json!(1);
    });
    challenge("reused-fabricated-id", &small, |x| {
        let v = x["snapshots"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .flat_map(|s| s["variables"].as_array_mut().unwrap())
            .find(|v| v["target"]["provenance_status"] == "address_only")
            .unwrap();
        v["target"]["provenance_status"] = serde_json::json!("ambiguous_reuse");
        v["target"]["allocation_id"] = serde_json::json!(1);
    });
    challenge("false-child-complete", &fork, |x| {
        x["heap_history"]["unobserved_process_coverage"] =
            serde_json::json!("complete_all_processes")
    });
    challenge("false-observed-complete", &fork, |x| {
        x["heap_history"]["complete"] = serde_json::json!(true);
        x["heap_history"]["missing_events"] = serde_json::json!(1);
    });
    challenge("unobserved-event-pid", &fork, |x| {
        x["heap_history"]["observed_event_pids"] = serde_json::json!([])
    });
    challenge("duplicate-heap-id", &small, |x| {
        let b = x["heap_state"][0].clone();
        x["heap_state"].as_array_mut().unwrap().push(b);
    });
    challenge("realloc-self", &small, |x| {
        let id = x["heap_state"][0]["allocation_id"].clone();
        x["heap_state"][0]["realloc_from_id"] = id;
    });
    challenge("realloc-fabricated-existing", &small, |x| {
        x["heap_state"][0]["realloc_from_id"] = x["heap_state"][1]["allocation_id"].clone();
    });
    challenge("realloc-missing", &small, |x| {
        x["heap_state"][0]["realloc_from_id"] = serde_json::json!(99999999)
    });
    challenge("duplicate-dwarf-id", &small, |x| {
        let t = x["type_registry"]["types"][0].clone();
        x["type_registry"]["types"].as_array_mut().unwrap().push(t);
    });
    challenge("duplicate-ast-id", &small, |x| {
        let t = x["source_metadata"]["expressions"][0].clone();
        x["source_metadata"]["expressions"]
            .as_array_mut()
            .unwrap()
            .push(t);
    });
    challenge("invalid-instruction-tag", &small, |x| {
        x["instruction_events"][0]["checkpoint_id"] = serde_json::json!(999999)
    });
    challenge("missing-legacy-scope", &fork, |x| {
        x["heap_history"]
            .as_object_mut()
            .unwrap()
            .remove("process_scope");
        x["heap_history"]
            .as_object_mut()
            .unwrap()
            .remove("unobserved_process_coverage");
    });
    challenge("missing-ast", &small, |x| {
        x["source_metadata"] = Value::Null
    });
    challenge("unsupported-version", &small, |x| {
        x["schema_version"] = serde_json::json!(300)
    });
    challenge("malformed-required", &small, |x| {
        x["snapshots"] = serde_json::json!("not a snapshot list")
    });
    let text = fs::read("/tmp/cte-tp8/small.jsonl").unwrap();
    let complete = parse_journal_bytes(&text, TraceLoadOptions::default()).unwrap();
    println!(
        "journal complete records={} finalized={}",
        complete.records.len(),
        complete.finalized
    );
    let mut torn = text.clone();
    torn.extend_from_slice(b"{\"kind\":\"instruction\",\"data\":{");
    let result = parse_journal_bytes(&torn, TraceLoadOptions::default()).unwrap();
    println!(
        "journal torn records={} torn_tail={} finalized={}",
        result.records.len(),
        result.torn_tail,
        result.finalized
    );
    let mut invalid = text;
    invalid.extend_from_slice(b"{\"kind\":\"instruction\",\"data\":{}}\n");
    println!(
        "journal invalid record={:?}",
        parse_journal_bytes(&invalid, TraceLoadOptions::default()).err()
    );
}

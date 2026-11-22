//! Independent, read-only TP-8 release probe. Usage: cargo run --release --example
//! trace_parser_tp8_audit -- TRACE.json [TRACE.json ...]. No production changes.
use c_trace_engine::models::DebugSessionTrace;
use c_trace_engine::trace_parser::{
    parse_journal, EvidenceLocation, QueryError, TraceLoadOptions, TraceStore, VariableScope,
    MAX_CONSUMER_PAGE,
};
use std::hint::black_box;
use std::time::Instant;

fn rss_kib() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/statm").unwrap();
    stat.split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u64>()
        .unwrap()
        * 4
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path)?;
        let t = Instant::now();
        let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
        let raw_us = t.elapsed().as_micros();
        let t = Instant::now();
        let trace: DebugSessionTrace = serde_json::from_slice(&bytes)?;
        let parse_us = t.elapsed().as_micros();
        let rss_parse = rss_kib();
        let t = Instant::now();
        let store = TraceStore::from_trace(trace)?;
        let index_us = t.elapsed().as_micros();
        let rss_index = rss_kib();
        assert_eq!(store.trace().schema_version, 2);
        assert_eq!(
            raw["snapshots"].as_array().unwrap().len(),
            store.trace().snapshots.len()
        );
        assert_eq!(
            raw["instruction_events"].as_array().unwrap().len(),
            store.trace().recording.instruction_events.len()
        );
        assert_eq!(
            raw["trace_gaps"].as_array().unwrap().len(),
            store.trace_gaps().len()
        );
        assert_eq!(
            raw["heap_events"].as_array().unwrap().len(),
            store.heap_events().len()
        );
        let t = Instant::now();
        let view = store.validated_semantic();
        let validate_us = t.elapsed().as_micros();
        let report = view.report();
        let findings: Vec<_> = report.findings().iter().map(|f| f.code).collect();
        let mut traversed = 0;
        let mut pages = 0;
        let t = Instant::now();
        for (i, snapshot) in store.trace().snapshots.iter().enumerate() {
            let summary = view.checkpoint_summary(i)?;
            assert_eq!(
                summary.stop_id(),
                (snapshot.observation.stop_id != 0).then_some(snapshot.observation.stop_id)
            );
            assert_eq!(
                summary.attempted_instruction_position(),
                snapshot.observation.instruction_position
            );
            assert_eq!(summary.pid(), snapshot.observation.pid);
            for scope in [VariableScope::Local, VariableScope::Global] {
                let records = match scope {
                    VariableScope::Local => &snapshot.variables,
                    _ => &snapshot.observation.globals,
                };
                for start in (0..records.len()).step_by(MAX_CONSUMER_PAGE) {
                    pages += 1;
                    for (offset, item) in summary
                        .variables_page(
                            scope,
                            start..(start + MAX_CONSUMER_PAGE).min(records.len()),
                        )?
                        .enumerate()
                    {
                        assert!(std::ptr::eq(item?.info, &records[start + offset]));
                    }
                }
            }
            // Compare independent raw JSON filter and every TP-7 page's position and borrowed record.
            let expected: Vec<_> = raw["instruction_events"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, e)| e["checkpoint_id"].as_u64() == Some(i as u64))
                .map(|(p, _)| p)
                .collect();
            for start in (0..expected.len()).step_by(MAX_CONSUMER_PAGE) {
                pages += 1;
                for (offset, row) in summary
                    .instruction_events_page(
                        start..(start + MAX_CONSUMER_PAGE).min(expected.len()),
                    )?
                    .enumerate()
                {
                    let row = row?;
                    let p = expected[start + offset];
                    assert_eq!(row.position, p);
                    assert!(std::ptr::eq(
                        row.event,
                        &store.trace().recording.instruction_events[p]
                    ));
                    assert_eq!(
                        row.reference.location(),
                        EvidenceLocation::InstructionEvent(p)
                    );
                    traversed += 1;
                }
            }
        }
        let traverse_us = t.elapsed().as_micros();
        assert_eq!(
            view.process_coverage().unobserved_descendants,
            c_trace_engine::trace_parser::EvidenceConfidence::Unavailable
        );
        let page_limit = if let Some(first) = store.trace().snapshots.first() {
            let summary = view.checkpoint_summary(0)?;
            if first.variables.len() > MAX_CONSUMER_PAGE {
                matches!(
                    summary.variables_page(VariableScope::Local, 0..129),
                    Err(QueryError::LimitExceeded { .. })
                )
            } else {
                true
            }
        } else {
            true
        };
        assert!(page_limit);
        let t = Instant::now();
        let probe_ns = if let Some(first) = store.trace().snapshots.first() {
            let summary = view.checkpoint_summary(0)?;
            let max = first.variables.len().min(128);
            for _ in 0..2000 {
                let mut sum = 0;
                for v in summary.variables_page(VariableScope::Local, 0..max)? {
                    sum += v?.info.value.len();
                }
                black_box(sum);
            }
            t.elapsed().as_nanos() / 2000
        } else {
            0
        };
        let journal = path.strip_suffix(".json").map(|p| format!("{p}.jsonl"));
        let journal_count = if let Some(p) = journal.filter(|p| std::path::Path::new(p).exists()) {
            let j = parse_journal(p, TraceLoadOptions::default())?;
            assert_eq!(
                j.counts.instructions,
                raw["instruction_events"].as_array().unwrap().len()
            );
            j.records.len()
        } else {
            0
        };
        let clones: Vec<_> = (0..4).map(|_| store.clone()).collect();
        assert!(clones
            .iter()
            .all(|s| std::ptr::eq(s.trace(), store.trace())));
        std::thread::scope(|scope| {
            for shared in &clones {
                scope.spawn(move || {
                    let v = shared.validated_semantic();
                    for _ in 0..20 {
                        assert_eq!(
                            v.execution().retained,
                            shared.trace().recording.instruction_events.len()
                        );
                        black_box(shared.indexes().estimated_bytes());
                    }
                });
            }
        });
        println!("path={path} bytes={} snap={} instr={} heap={} gaps={} journal={} raw_parse_us={raw_us} typed_parse_us={parse_us} rss_after_parse_kib={rss_parse} index_us={index_us} index_est_bytes={} rss_after_index_kib={rss_index} validate_us={validate_us} traversed={traversed} pages={pages} traverse_us={traverse_us} variable_page_ns={probe_ns} sound={} partial={} findings={findings:?}", bytes.len(), store.trace().snapshots.len(), store.trace().recording.instruction_events.len(), store.heap_events().len(), store.trace_gaps().len(),journal_count,store.indexes().estimated_bytes(), report.is_structurally_sound(),report.is_partial());
    }
    Ok(())
}

//! Trace Parser TP-1 integration tests.
//!
//! Exercises ingestion against **real Stage 1 output**: the six `examples/v21_*.json`
//! traces produced by this engine (V2.1 recording campaigns) and the two
//! `tests/fixtures/journal/*.jsonl` journals written by `storage::Journal`
//! during real `cargo run -- --journal` sessions. Synthetic inputs (used for
//! invalid-input categories) are mutations of those real files, never
//! hand-invented trace shapes.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use c_trace_engine::config::TraceMode;
use c_trace_engine::models::{ExecutionStatus, HeapEventType};
use c_trace_engine::trace_parser::journal::JournalRecord;
use c_trace_engine::trace_parser::{
    parse_journal, parse_journal_bytes, parse_journal_reader, TraceFormatSelection,
    TraceLoadOptions, TraceParseError, TraceStore,
};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn fixture(name: &str) -> PathBuf {
    examples_dir().join(name)
}

fn journal_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/journal")
        .join(name)
}

fn load(name: &str) -> TraceStore {
    TraceStore::open(fixture(name), TraceLoadOptions::default())
        .unwrap_or_else(|e| panic!("loading {name}: {e}"))
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tp1-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// All six real V2.1 traces ingest, report their recorded statuses, and keep
/// `session_meta` accessible.
#[test]
fn a_normal_traces_ingest_with_recorded_statuses() {
    for (name, status) in [
        ("v21_normal.json", ExecutionStatus::Completed),
        ("v21_dual.json", ExecutionStatus::Completed),
        ("v21_heap.json", ExecutionStatus::Completed),
        ("v21_loop.json", ExecutionStatus::Completed),
        ("v21_crash.json", ExecutionStatus::Crashed),
        ("v21_bounded_infinite.json", ExecutionStatus::Timeout),
    ] {
        let store = load(name);
        assert_eq!(
            store.source().format,
            c_trace_engine::trace_parser::TraceFormat::Json,
            "{name}"
        );
        assert!(store.source().byte_len > 0, "{name}");
        assert_eq!(store.trace().schema_version, 2, "{name}");
        assert!(
            matches!(store.execution_status(), s if std::mem::discriminant(s) == std::mem::discriminant(&status)),
            "{name}: {:?} != {status:?}",
            store.execution_status()
        );
        assert!(store.session_meta().is_some(), "{name}: session_meta");
        assert!(
            store.trace().compiled_successfully,
            "{name}: all six fixtures compiled"
        );
        assert!(
            store.trace().stage_results.compiler.status.is_some(),
            "{name}: stage_results"
        );
    }
}

/// V2-era example files carry `max_register_snapshot_bytes` in
/// `execution_limits` — a field removed from the models in the V2.1 register
/// rework, and `ExecutionLimits` rejects unknown fields. This is the
/// engine's *pre-existing* model contract (Stage 1 never re-loads traces, so
/// nothing else exercised it): the parser surfaces it as a typed error
/// naming the offending field rather than guessing at defaults.
///
/// Schema-version-2 traces that only use *current* fields — including
/// V2.1's deprecated-but-optional register fields — keep parsing (see the
/// `from_trace` unit test and the v21 fixtures above).
#[test]
fn a_legacy_v2_fixtures_surface_the_model_contract() {
    for name in [
        "v2_success.json",
        "v2_dual.json",
        "v2_null_pointer.json",
        "v2_bounded_infinite.json",
    ] {
        let err = TraceStore::open(fixture(name), TraceLoadOptions::default()).unwrap_err();
        assert!(
            matches!(err, TraceParseError::Source { .. }),
            "{name}: {err}"
        );
        let text = err.to_string();
        assert!(
            text.contains("max_register_snapshot_bytes"),
            "{name}: {text}"
        );
    }
}

/// Semantic fidelity: the ingested model is *value-identical* to what Stage 1
/// wrote. Re-serializing the store's trace must equal the original JSON
/// document exactly (field values and structure; whitespace is formatting).
#[test]
fn b_round_trip_is_value_identical_to_stage1_output() {
    for name in [
        "v21_normal.json",
        "v21_dual.json",
        "v21_heap.json",
        "v21_crash.json",
        "v21_bounded_infinite.json",
    ] {
        let original: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture(name)).unwrap()).unwrap();
        let store = load(name);
        let reloaded = serde_json::to_value(store.trace()).unwrap();
        assert_eq!(
            original, reloaded,
            "{name}: ingested trace differs from recorded trace"
        );
    }
}

/// Source/assembly fidelity: checkpoints, instruction events, and
/// disassembly survive; repeated execution of the same address is never
/// collapsed.
#[test]
fn b_source_and_assembly_fidelity() {
    let store = load("v21_dual.json");
    let trace = store.trace();
    assert_eq!(trace.snapshots.len(), 25, "snapshots preserved");
    assert_eq!(
        trace.recording.instruction_events.len(),
        80,
        "instruction events preserved"
    );
    assert_eq!(
        trace.recording.disassembly_table.len(),
        92,
        "disassembly preserved"
    );
    assert_eq!(trace.recording.trace_gaps.len(), 5, "gaps preserved");
    assert_eq!(
        trace.recording.source_checkpoints,
        vec![
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24
        ],
        "source checkpoint references preserved"
    );
    // 80 events over 58 distinct addresses: repeats are preserved as
    // separate events, in recorded order.
    let addresses: Vec<&str> = trace
        .recording
        .instruction_events
        .iter()
        .map(|e| e.address.as_str())
        .collect();
    let distinct = addresses.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(distinct.len(), 58);
    assert_eq!(addresses.len(), 80);

    let bounded_store = load("v21_bounded_infinite.json");
    let bounded = bounded_store.trace();
    // 4146 recorded events over only 7 distinct addresses — a loop, not a
    // merge. Event order is the recorded execution order.
    let addresses: Vec<&str> = bounded
        .recording
        .instruction_events
        .iter()
        .map(|e| e.address.as_str())
        .collect();
    assert_eq!(addresses.len(), 4146);
    assert_eq!(
        addresses
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        7
    );
    let mut counts = std::collections::BTreeMap::new();
    for address in &addresses {
        *counts.entry(*address).or_insert(0usize) += 1;
    }
    assert!(
        counts.values().any(|&c| c > 100),
        "loop-body addresses recur hundreds of times: {counts:?}"
    );
}

/// Memory/heap fidelity: allocation ids and event order unchanged, raw
/// memory bytes hex-exact, and "not captured" stays distinguishable from
/// zero bytes.
#[test]
fn c_memory_and_heap_fidelity() {
    let store = load("v21_dual.json");
    let trace = store.trace();
    // Chronological order exactly as recorded: allocated then freed, id 1.
    let types: Vec<HeapEventType> = trace
        .heap_events
        .iter()
        .map(|e| e.event_type.clone())
        .collect();
    assert!(matches!(types[0], HeapEventType::Allocated));
    assert!(matches!(types[1], HeapEventType::Freed));
    assert!(trace.heap_events.iter().all(|e| e.allocation_id == 1));

    // Raw bytes of the heap block survive ingestion, hex-exact. The
    // allocation happens mid-run, so find the first snapshot with captured
    // block bytes (snapshot 0 has none).
    let original: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("v21_dual.json")).unwrap()).unwrap();
    let block_loc = original["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find_map(|(si, snap)| {
            snap["heap_blocks"]
                .as_array()?
                .iter()
                .position(|b| b["raw_bytes_hex"].as_str().is_some_and(|s| !s.is_empty()))
                .map(|bi| (si, bi))
        })
        .expect("fixture has a heap block with captured bytes");
    let recorded_bytes = original["snapshots"][block_loc.0]["heap_blocks"][block_loc.1]
        ["raw_bytes_hex"]
        .as_str()
        .unwrap();
    let store_block = &trace.snapshots[block_loc.0].heap_blocks[block_loc.1];
    assert_eq!(store_block.raw_bytes_hex.as_deref(), Some(recorded_bytes));
    assert!(!recorded_bytes.is_empty(), "fixture has captured bytes");

    // Variable raw bytes survive too (memory of `arr`, `p`, ...): compare
    // the first variable with captured bytes, wherever it appears.
    let recorded = original["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .find_map(|(si, snap)| {
            snap["variables"]
                .as_array()?
                .iter()
                .position(|v| v["raw_bytes_hex"].is_string())
                .map(|vi| (si, vi))
        })
        .expect("fixture has a variable with captured bytes");
    let store_var = &trace.snapshots[recorded.0].variables[recorded.1];
    let expected = &original["snapshots"][recorded.0]["variables"][recorded.1];
    assert_eq!(store_var.name, expected["name"]);
    assert_eq!(
        store_var.raw_bytes_hex.as_deref(),
        expected["raw_bytes_hex"].as_str()
    );

    // Heap example: the freed block's final state keeps its captured bytes.
    let heap_store = load("v21_heap.json");
    let heap = heap_store.trace();
    assert_eq!(heap.heap_events.len(), 2);
    assert_eq!(heap.heap_state.len(), 1);
    assert!(heap.heap_state[0].is_freed);
    assert!(
        heap.heap_state[0].raw_bytes_hex.is_some(),
        "captured-before-free bytes kept"
    );
}

/// Partial traces load as first-class stores: crash, timeout, and skipped
/// diagnostic stages are recorded data, not ingestion errors.
#[test]
fn d_partial_traces_load() {
    let crash = load("v21_crash.json");
    assert!(matches!(crash.execution_status(), ExecutionStatus::Crashed));
    assert!(
        crash.trace().crash_report.is_some(),
        "crash evidence preserved"
    );
    assert_eq!(crash.trace().recording.instruction_events.len(), 5);

    let timeout = load("v21_bounded_infinite.json");
    assert!(matches!(
        timeout.execution_status(),
        ExecutionStatus::Timeout
    ));
    assert!(
        !timeout.trace().resource_usage.limits_triggered.is_empty(),
        "resource-limit evidence preserved"
    );

    let normal = load("v21_normal.json");
    let sanitizer = &normal.trace().stage_results.sanitizer;
    assert!(
        matches!(
            sanitizer.status,
            Some(c_trace_engine::models::StageStatus::Skipped)
        ),
        "skipped tool stage preserved as data"
    );
}

/// E: every invalid-input category produces a typed structured error.
#[test]
fn e_invalid_inputs() {
    let opts = TraceLoadOptions::default();
    let dual_text = std::fs::read_to_string(fixture("v21_dual.json")).unwrap();

    // Empty / whitespace-only input.
    assert!(matches!(
        TraceStore::from_json_bytes(b"", opts.clone()).unwrap_err(),
        TraceParseError::EmptyInput
    ));

    // Malformed JSON.
    assert!(matches!(
        TraceStore::from_json_bytes(b"{\"schema_version\": 2, \"execution_status", opts.clone())
            .unwrap_err(),
        TraceParseError::MalformedJson { line: 1, .. }
    ));

    // Truncated JSON (cut a real trace mid-document).
    let truncated = &dual_text[..dual_text.len() / 2];
    assert!(matches!(
        TraceStore::from_json_bytes(truncated.as_bytes(), opts.clone()).unwrap_err(),
        TraceParseError::MalformedJson { .. }
    ));

    // Incorrect top-level structure (JSON array instead of trace object).
    assert!(matches!(
        TraceStore::from_json_bytes(b"[]", opts.clone()).unwrap_err(),
        TraceParseError::MalformedJson { .. }
    ));

    // Unsupported schema versions (via mutation of the real trace).
    let mut v3: serde_json::Value = serde_json::from_str(&dual_text).unwrap();
    v3["schema_version"] = serde_json::json!(3);
    assert!(matches!(
        TraceStore::from_json_bytes(v3.to_string().as_bytes(), opts.clone()).unwrap_err(),
        TraceParseError::UnsupportedSchemaVersion { found: 3, .. }
    ));
    v3["schema_version"] = serde_json::json!(1);
    assert!(matches!(
        TraceStore::from_json_bytes(v3.to_string().as_bytes(), opts.clone()).unwrap_err(),
        TraceParseError::UnsupportedSchemaVersion { found: 1, .. }
    ));
    // A *missing* schema_version field is the documented default contract:
    // it means 2 and loads.
    let mut v_missing = v3.clone();
    v_missing.as_object_mut().unwrap().remove("schema_version");
    TraceStore::from_json_bytes(v_missing.to_string().as_bytes(), opts.clone())
        .unwrap_or_else(|e| panic!("missing schema_version should default to 2: {e}"));

    // Invalid required field type.
    let mut bad_status = v3.clone();
    bad_status["schema_version"] = serde_json::json!(2);
    bad_status["execution_status"] = serde_json::json!("bogus_status");
    assert!(matches!(
        TraceStore::from_json_bytes(bad_status.to_string().as_bytes(), opts.clone()).unwrap_err(),
        TraceParseError::MalformedJson { .. }
    ));

    // Oversized input (configurable limit enforced before parsing).
    let tiny = TraceLoadOptions {
        max_bytes: 1024,
        ..opts.clone()
    };
    assert!(matches!(
        TraceStore::from_json_bytes(dual_text.as_bytes(), tiny).unwrap_err(),
        TraceParseError::LimitExceeded { limit: 1024 }
    ));

    // Missing file: error names the path.
    let missing = temp_dir().join("does-not-exist.json");
    let err = TraceStore::open(&missing, opts.clone()).unwrap_err();
    assert!(matches!(err, TraceParseError::Source { .. }), "{err}");
    assert!(err.to_string().contains(missing.to_str().unwrap()), "{err}");

    // Invalid UTF-8.
    assert!(matches!(
        TraceStore::from_json_bytes(&b"{\"a\": \xff}"[..], opts).unwrap_err(),
        TraceParseError::InvalidEncoding { .. }
    ));
}

/// F: the real session journals validate and decode, with per-kind counts
/// matching what the writer produced.
#[test]
fn f_journal_fixtures_ingest() {
    let journal = parse_journal(
        journal_fixture("finite_loop_dual.jsonl"),
        TraceLoadOptions::default(),
    )
    .unwrap_or_else(|e| panic!("finite_loop journal: {e}"));
    assert_eq!(journal.counts.configuration, 1);
    assert_eq!(journal.counts.checkpoints, 29);
    assert_eq!(journal.counts.instructions, 97);
    assert_eq!(journal.counts.disassembly, 46);
    assert_eq!(journal.counts.gaps, 2);
    assert!(journal.finalized, "session reached orderly shutdown");
    assert!(!journal.torn_tail);
    assert_eq!(journal.line_count, 176);
    assert_eq!(
        journal.records.len(),
        journal.counts.configuration
            + journal.counts.checkpoints
            + journal.counts.instructions
            + journal.counts.disassembly
            + journal.counts.gaps
            + journal.counts.finalized
    );
    // Ordering preserved: configuration first, finalized last.
    assert!(matches!(
        journal.records[0],
        JournalRecord::Configuration(_)
    ));
    assert!(matches!(
        journal.records[journal.records.len() - 1],
        JournalRecord::Finalized
    ));
    // The session ran in dual mode (real journal of a dual-mode run).
    assert_eq!(journal.configuration().unwrap().mode, TraceMode::Dual);
    // Decoded record payloads keep their types: snapshots carry files/lines.
    assert!(journal.records.iter().any(|r| matches!(
        r,
        JournalRecord::Checkpoint(snapshot) if snapshot.step_index == 0
    )));

    let calloc = parse_journal(
        journal_fixture("calloc_demo_dual.jsonl"),
        TraceLoadOptions::default(),
    )
    .unwrap_or_else(|e| panic!("calloc journal: {e}"));
    assert_eq!(calloc.counts.allocations, 2, "calloc/free journaled");
    assert!(calloc.records.iter().any(|r| matches!(
        r,
        JournalRecord::Allocation { event, .. }
            if matches!(event.event_type, HeapEventType::Allocated) && event.allocation_id == 1
    )));
    assert!(calloc.finalized);
}

/// F: journal-compatible malformed inputs and explicit format selection.
#[test]
fn f_journal_invalid_inputs_and_format_selection() {
    let text = std::fs::read_to_string(journal_fixture("finite_loop_dual.jsonl")).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    // Malformed record mid-file: typed error naming the line.
    let mut broken = lines.clone();
    broken.insert(1, "{\"kind\":\"checkpoint\""); // invalid JSON, complete line
    let mut bytes = broken.join("\n").into_bytes();
    bytes.push(b'\n');
    let err = parse_journal_bytes(&bytes, TraceLoadOptions::default()).unwrap_err();
    assert!(
        matches!(err, TraceParseError::MalformedJournalRecord { line: 2, .. }),
        "{err}"
    );

    // Unknown record kind: rejected by default, preserved verbatim when allowed.
    let mut unknown = lines.clone();
    unknown.insert(1, "{\"kind\":\"from_the_future\",\"data\":{\"x\":1}}");
    let joined = unknown.join("\n").into_bytes();
    let err = parse_journal_bytes(&joined, TraceLoadOptions::default()).unwrap_err();
    assert!(
        matches!(
            err,
            TraceParseError::UnsupportedJournalRecord { line: 2, ref kind } if kind == "from_the_future"
        ),
        "{err}"
    );
    let allow = TraceLoadOptions {
        allow_unknown_journal_records: true,
        ..TraceLoadOptions::default()
    };
    let journal = parse_journal_bytes(&joined, allow).unwrap();
    assert_eq!(journal.counts.unknown, 1);
    assert!(
        matches!(&journal.records[1], JournalRecord::Unknown { kind, .. } if kind == "from_the_future")
    );

    // Truncated final line: reported, never interpreted.
    let without_last = lines[..lines.len() - 1].join("\n").as_bytes().to_vec();
    let mut torn = without_last.clone();
    torn.push(b'\n');
    torn.extend_from_slice(b"{\"kind\":\"instruction\",\"data\":{\"addr");
    let journal = parse_journal_bytes(&torn, TraceLoadOptions::default()).unwrap();
    assert!(journal.torn_tail);
    assert!(!journal.finalized, "finalized record was truncated away");
    assert_eq!(
        journal.counts.checkpoints, 29,
        "all complete records still decoded"
    );

    // Record limit enforced (path ingestion wraps errors in `Source`).
    let limited = TraceLoadOptions {
        max_journal_records: 5,
        ..TraceLoadOptions::default()
    };
    let err = parse_journal(journal_fixture("finite_loop_dual.jsonl"), limited).unwrap_err();
    assert!(err.to_string().contains("record limit of 5"), "{err}");

    // Journal missing its configuration record (writer invariant). The
    // recorder journals its startup gaps right after configuration, so the
    // next record kind is `gap`.
    let mut no_config = lines.clone();
    no_config.remove(0);
    let mut bytes = no_config.join("\n").into_bytes();
    bytes.push(b'\n');
    let err = parse_journal_bytes(&bytes, TraceLoadOptions::default()).unwrap_err();
    assert!(
        matches!(
            err,
            TraceParseError::JournalMissingConfiguration { line: 1, ref kind } if kind == "gap"
        ),
        "{err}"
    );

    // Empty journal file: valid, empty, nothing fabricated.
    let empty = temp_dir().join("empty.jsonl");
    std::fs::write(&empty, b"").unwrap();
    let journal = parse_journal(&empty, TraceLoadOptions::default()).unwrap();
    assert!(journal.records.is_empty() && !journal.finalized && !journal.torn_tail);

    // Auto format selection: journals are detected from content.
    let auto = TraceLoadOptions {
        format: TraceFormatSelection::Auto,
        ..TraceLoadOptions::default()
    };
    let journal = parse_journal(journal_fixture("finite_loop_dual.jsonl"), auto.clone()).unwrap();
    assert_eq!(journal.counts.checkpoints, 29);
    // ...but cannot become a TraceStore: reconstruction is unsupported, and
    // the error says exactly why (path ingestion wraps the typed error in
    // `Source` naming the file).
    let err =
        TraceStore::open(journal_fixture("finite_loop_dual.jsonl"), auto.clone()).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("full trace reconstruction from a journal is not supported"),
        "{text}"
    );
    assert!(text.contains("parse_journal"), "{text}");

    // A JSON trace is not a journal: pretty traces fail on line 1.
    let err = parse_journal(fixture("v21_dual.json"), auto).unwrap_err();
    assert!(
        err.to_string()
            .contains("malformed journal record on line 1"),
        "{err}"
    );
}

/// F: journal reader ingestion (no filesystem access).
#[test]
fn f_journal_from_reader() {
    let text = std::fs::read_to_string(journal_fixture("calloc_demo_dual.jsonl")).unwrap();
    let journal = parse_journal_reader(Cursor::new(text.as_bytes()), TraceLoadOptions::default())
        .unwrap_or_else(|e| panic!("calloc via reader: {e}"));
    assert_eq!(journal.counts.allocations, 2);
    assert!(journal.finalized);
}

/// G: one parse, many reads — the store never re-parses, and sharing is
/// safe across threads.
#[test]
fn g_ownership_and_repeat_access() {
    let store = load("v21_dual.json");
    // Same underlying Arc on every access: structurally impossible to
    // re-parse or duplicate the trace.
    assert!(Arc::ptr_eq(&store.shared(), &store.shared()));

    // Concurrent readers see the same canonical data.
    let shared = Arc::new(store.clone());
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let store = Arc::clone(&shared);
            std::thread::spawn(move || store.trace().recording.instruction_events.len())
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().unwrap(), 80);
    }

    // Independent loads never mix data.
    let a = load("v21_normal.json");
    let b = load("v21_dual.json");
    assert_eq!(a.trace().recording.instruction_events.len(), 0);
    assert_eq!(b.trace().recording.instruction_events.len(), 80);
    assert!(matches!(a.execution_status(), ExecutionStatus::Completed));
    assert!(matches!(b.execution_status(), ExecutionStatus::Completed));

    // Repeated access returns identical values (no parse, no mutation).
    let first = serde_json::to_value(store.trace()).unwrap();
    for _ in 0..3 {
        assert_eq!(serde_json::to_value(store.trace()).unwrap(), first);
    }
}

/// Loading the same file through a reader yields the same store as through
/// the path (reader ingestion is fully supported).
#[test]
fn g_reader_matches_path_ingestion() {
    let bytes = std::fs::read(fixture("v21_dual.json")).unwrap();
    let via_path = load("v21_dual.json");
    let via_reader =
        TraceStore::from_reader(Cursor::new(bytes), TraceLoadOptions::default()).unwrap();
    assert_eq!(
        serde_json::to_value(via_path.trace()).unwrap(),
        serde_json::to_value(via_reader.trace()).unwrap()
    );
    assert_eq!(via_path.source().byte_len, via_reader.source().byte_len);
}

/// No Stage 1 regression: the trace serializer round-trips through the
/// parser's model without drift (compile the same bytes in and out).
#[test]
fn stage1_serialization_is_stable_through_parser() {
    let text = std::fs::read_to_string(fixture("v21_normal.json")).unwrap();
    let store = TraceStore::from_json_bytes(text.as_bytes(), TraceLoadOptions::default()).unwrap();
    let reprinted = serde_json::to_string_pretty(store.trace()).unwrap();
    let original: serde_json::Value = serde_json::from_str(&text).unwrap();
    let reprinted_value: serde_json::Value = serde_json::from_str(&reprinted).unwrap();
    assert_eq!(original, reprinted_value);
}

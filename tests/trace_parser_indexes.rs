//! Trace Parser TP-2 integration tests: indexes, cross-record resolution,
//! and deep validation over the canonical [`TraceStore`].
//!
//! Exercises the TP-2 query layer against **real Stage 1 output**: the six
//! `examples/v21_*.json` traces (V2.1 recording campaigns), the two fresh
//! `tests/fixtures/stage1/*.json` traces produced by the current engine
//! build, and mutation-based corruption/partiality probes derived from real
//! files (never hand-invented trace shapes). The reference-semantics
//! dictionary under test:
//!
//! * `event_id` / `instruction_id` / `checkpoint_id` / `step_index` /
//!   `source_checkpoints[]` are **0-based vector positions**;
//! * `stop_id` / `stop_id_before` / `stop_id_after` / `observed_at_stop`
//!   are the **1-based GDB stop counter** (holes are normal);
//! * `allocation_id` is a **stable engine identity** (0 = untracked, never
//!   a vector index; reused addresses stay distinct allocations);
//! * addresses are hex strings and never identities.

use std::path::PathBuf;
use std::sync::Arc;

use c_trace_engine::models::{DebugSessionTrace, ExecutionStatus, HeapEventType};
use c_trace_engine::trace_parser::{QueryError, TraceLoadOptions, TraceStore, ValidationSeverity};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn fixture(name: &str) -> PathBuf {
    examples_dir().join(name)
}

fn stage1_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stage1")
        .join(name)
}

fn load(path: impl AsRef<std::path::Path>) -> TraceStore {
    let path = path.as_ref();
    TraceStore::open(path, TraceLoadOptions::default())
        .unwrap_or_else(|e| panic!("loading {}: {e}", path.display()))
}

fn load_example(name: &str) -> TraceStore {
    load(fixture(name))
}

fn all_fixtures() -> Vec<(std::path::PathBuf, &'static str)> {
    let mut paths: Vec<(std::path::PathBuf, &'static str)> = [
        "v21_normal.json",
        "v21_dual.json",
        "v21_heap.json",
        "v21_loop.json",
        "v21_crash.json",
        "v21_bounded_infinite.json",
    ]
    .into_iter()
    .map(|name| (fixture(name), name))
    .collect();
    paths.push((stage1_fixture("loop.json"), "stage1 loop.json"));
    paths.push((
        stage1_fixture("realloc_move.json"),
        "stage1 realloc_move.json",
    ));
    paths
}

/// An owned, mutated copy of a real fixture's trace (mutation-based
/// corruption probes build on recorded data, never invented shapes).
fn owned_trace(store: &TraceStore) -> DebugSessionTrace {
    store.trace().clone()
}

// ---------------------------------------------------------------------------
// A — positional semantics: event_id / instruction_id / step_index are
// vector positions; queries validate bounds instead of clamping or guessing.
// ---------------------------------------------------------------------------

#[test]
fn a_positional_access_is_index_free_and_bounds_checked() {
    let store = load_example("v21_dual.json");
    let trace = store.trace();

    // event_id IS the vector position (recorder-enforced contiguity).
    let first = 0usize;
    let middle = trace.recording.instruction_events.len() / 2;
    let last = trace.recording.instruction_events.len() - 1;
    for index in [first, middle, last] {
        assert_eq!(
            store.instruction_event(index).unwrap().event_id,
            index as u64,
            "event_id must equal the vector position"
        );
        // The query is a plain borrow of the canonical array.
        assert_eq!(
            store.instruction_event(index).unwrap() as *const _,
            &trace.recording.instruction_events[index] as *const _,
        );
    }
    // `instruction_range` returns one subslice in recorded order.
    let range = store.instruction_range(2..7).unwrap();
    assert_eq!(range.len(), 5);
    assert_eq!(range[0].event_id, 2);
    assert_eq!(range[4].event_id, 6);
    // Empty range is valid; inverted / oversized ranges are rejected.
    assert!(store.instruction_range(5..5).unwrap().is_empty());
    assert_eq!(
        store
            .instruction_range(0..store.retained_event_count())
            .unwrap()
            .len(),
        store.retained_event_count()
    );
    let (start, end) = (7usize, 2usize);
    assert!(matches!(
        store.instruction_range(start..end),
        Err(QueryError::InvalidRange { .. })
    ));
    assert!(matches!(
        store.instruction_range(0..trace.recording.instruction_events.len() + 1),
        Err(QueryError::InvalidRange { .. })
    ));
    assert!(matches!(
        store.instruction_event(trace.recording.instruction_events.len()),
        Err(QueryError::OutOfRange {
            collection: "instruction_events",
            ..
        })
    ));

    // instruction_id IS the disassembly position: the entry query returns
    // exactly `disassembly_table[id]`, and the event/disasm address
    // invariant holds through the query layer.
    let entry = store.disassembly_entry(3).unwrap() as *const _;
    assert_eq!(entry, &trace.recording.disassembly_table[3] as *const _);
    assert_eq!(
        store.disassembly_for_event(first).unwrap().address,
        trace.recording.instruction_events[first].address
    );
    assert!(matches!(
        store.disassembly_entry(store.disassembly_len() as u64),
        Err(QueryError::InvalidReference { .. })
    ));

    // snapshot index == step_index; out of range is rejected.
    for index in [0, trace.snapshots.len() - 1] {
        assert_eq!(store.snapshot(index).unwrap().step_index, index);
    }
    assert!(matches!(
        store.snapshot(trace.snapshots.len()),
        Err(QueryError::OutOfRange {
            collection: "snapshots",
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// B — checkpoint mapping: source_checkpoints[] are snapshot indices (an
// indirection, not a renumbering); checkpoint-scoped event runs are
// contiguous and derived by binary search, with zero memory.
// ---------------------------------------------------------------------------

#[test]
fn b_checkpoint_mapping_indirects_into_snapshots() {
    let store = load_example("v21_loop.json");
    let trace = store.trace();

    // The loop fixture journaled every snapshot: the checkpoint list is the
    // identity mapping [0, 1, ..., 28].
    assert_eq!(
        store.source_checkpoints(),
        (0..trace.snapshots.len()).collect::<Vec<_>>()
    );
    for position in [0, 7, trace.snapshots.len() - 1] {
        assert_eq!(store.source_checkpoint(position).unwrap(), position);
        assert_eq!(
            store
                .source_checkpoint_snapshot(position)
                .unwrap()
                .step_index,
            position
        );
    }
    assert!(matches!(
        store.source_checkpoint(trace.snapshots.len()),
        Err(QueryError::OutOfRange {
            collection: "source_checkpoints",
            ..
        })
    ));

    // But the mapping is an *indirection*: drop one journal entry and
    // source_checkpoint(2) must resolve to snapshots[3], never to
    // snapshots[2].
    let mut edited = owned_trace(&store);
    edited.recording.source_checkpoints.remove(2);
    let edited = TraceStore::from_trace(edited).unwrap();
    assert_eq!(edited.source_checkpoint(2).unwrap(), 3);
    assert_eq!(edited.source_checkpoint_snapshot(2).unwrap().step_index, 3);
    // Validation records the journal incompleteness as partial coverage.
    let report = edited.validate_deep();
    assert!(report.is_structurally_sound());
    assert!(report
        .findings()
        .iter()
        .any(|f| f.code == "CHECKPOINT_JOURNAL_INCOMPLETE"
            && f.severity == ValidationSeverity::PartialCoverage));

    // Checkpoint-scoped events: the contiguous run whose checkpoint_id
    // equals the snapshot index, cross-checked against a linear scan.
    for snapshot_index in 0..trace.snapshots.len() {
        let expected: Vec<u64> = trace
            .recording
            .instruction_events
            .iter()
            .filter(|event| event.checkpoint_id == Some(snapshot_index))
            .map(|event| event.event_id)
            .collect();
        let queried: Vec<u64> = store
            .events_at_checkpoint(snapshot_index)
            .unwrap()
            .iter()
            .map(|event| event.event_id)
            .collect();
        assert_eq!(queried, expected, "checkpoint {snapshot_index}");
    }
    assert!(matches!(
        store.events_at_checkpoint(trace.snapshots.len()),
        Err(QueryError::OutOfRange { .. })
    ));
}

// ---------------------------------------------------------------------------
// C — disassembly ↔ executions: one static instruction's repeated executions
// stay distinct, ascending, and equal to a linear scan.
// ---------------------------------------------------------------------------

#[test]
fn c_instruction_executions_resolve_repeated_runs() {
    let store = load_example("v21_loop.json");
    let trace = store.trace();
    let events = &trace.recording.instruction_events;

    // The hottest instruction of the loop body: many positions, all events
    // carrying that id, in ascending order.
    let mut hottest = 0u64;
    let mut hottest_count = 0usize;
    for id in 0..trace.recording.disassembly_table.len() as u64 {
        let count = events
            .iter()
            .filter(|e| e.instruction_id == Some(id))
            .count();
        if count > hottest_count {
            hottest = id;
            hottest_count = count;
        }
    }
    assert!(hottest_count > 1, "the loop body must execute repeatedly");

    let positions = store.instruction_execution_positions(hottest).unwrap();
    assert_eq!(positions.len(), hottest_count);
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "ascending");
    for (slot, &position) in positions.iter().enumerate() {
        let event = &events[position as usize];
        assert_eq!(event.instruction_id, Some(hottest));
        // Same static instruction, distinct executions (event identity).
        assert_eq!(event.event_id as usize, position as usize);
        let _ = slot;
    }
    // The iterator yields the same events without allocating.
    let iterator_len = store.instruction_executions(hottest).unwrap().len();
    assert_eq!(iterator_len, hottest_count);

    // Exhaustive cross-check for every id: the index equals the linear scan.
    for id in 0..trace.recording.disassembly_table.len() as u64 {
        let expected: Vec<u32> = events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.instruction_id == Some(id))
            .map(|(position, _)| position as u32)
            .collect();
        assert_eq!(store.instruction_execution_positions(id).unwrap(), expected);
    }

    // Out-of-table ids are rejected, never silently answered as empty.
    assert!(matches!(
        store.instruction_execution_positions(store.disassembly_len() as u64),
        Err(QueryError::InvalidReference { .. })
    ));
}

// ---------------------------------------------------------------------------
// D — source ↔ assembly: static (which instructions) vs dynamic (which
// executions) are different questions with different answers; file identity
// is the recorded path, verbatim.
// ---------------------------------------------------------------------------

#[test]
fn d_source_mapping_splits_static_and_dynamic() {
    let store = load_example("v21_loop.json");

    let line = 4u32; // the hot loop line of fixtures/c/finite_loop.c
    let file = "fixtures/c/finite_loop.c";
    let static_ids = store.instructions_for_source(file, line).unwrap();
    let dynamic_positions = store.executed_event_positions_for_source(file, line);

    assert!(!static_ids.is_empty(), "line 4 compiled to instructions");
    assert!(!dynamic_positions.is_empty());
    // Dynamic side keeps every visit (loop!); static side deduplicates.
    assert_eq!(dynamic_positions.len(), 48, "recorded visits of line 4");
    assert!(
        static_ids.len() < dynamic_positions.len(),
        "static {} < dynamic {}",
        static_ids.len(),
        dynamic_positions.len()
    );
    assert!(static_ids.windows(2).all(|w| w[0] < w[1]), "deduplicated");
    assert!(
        dynamic_positions.windows(2).all(|w| w[0] < w[1]),
        "ascending event order"
    );
    // Every dynamic position is an event whose instruction_id is on the
    // static side, and whose recorded source is exactly this location.
    for &position in dynamic_positions {
        let event = store.instruction_event(position as usize).unwrap();
        assert_eq!(event.source.file.as_deref(), Some(file));
        assert_eq!(event.source.line, Some(line));
        assert!(static_ids.contains(&(event.instruction_id.unwrap() as u32)));
    }

    // Reverse direction resolves through recorded evidence.
    let reverse: Vec<u32> = store
        .sources_for_instruction(static_ids[0] as u64)
        .unwrap()
        .map(|key| key.line)
        .collect();
    assert!(reverse.contains(&line));

    // File identity is verbatim: a basename-only path is a different
    // (unrecorded) file, and unknown locations are empty results, not
    // errors.
    assert!(store
        .instructions_for_source("finite_loop.c", line)
        .unwrap()
        .is_empty());
    assert!(store
        .instructions_for_source(file, 9999)
        .unwrap()
        .is_empty());
    assert_eq!(
        store.source_files(),
        [Arc::from("fixtures/c/finite_loop.c")]
    );
}

// ---------------------------------------------------------------------------
// E — stop navigation: the 1-based GDB stop counter with holes; snapshots
// and events associate by different recorded fields; heap events associate
// through snapshot boundaries.
// ---------------------------------------------------------------------------

#[test]
fn e_stop_queries_honor_the_stop_counter_and_its_holes() {
    let store = load_example("v21_loop.json");
    let trace = store.trace();

    // Recorded snapshot stops are 1, 3, 4, 8, ... — stop 2 is a hole.
    let recorded_stops: Vec<u64> = trace
        .snapshots
        .iter()
        .map(|s| s.observation.stop_id)
        .collect();
    assert_eq!(&recorded_stops[..4], &[1, 3, 4, 8]);
    assert_eq!(
        store
            .snapshot_at_stop(1)
            .unwrap()
            .map(|s| s.observation.stop_id),
        Some(1)
    );
    assert!(
        store.snapshot_at_stop(2).unwrap().is_none(),
        "hole, not an error"
    );
    assert_eq!(
        store
            .snapshot_at_stop(*recorded_stops.last().unwrap())
            .unwrap()
            .map(|s| s.observation.stop_id),
        Some(*recorded_stops.last().unwrap())
    );

    // Events associate by stop_id_before — including stops with no
    // snapshot: stop 2 has events but no retained snapshot.
    assert!(!store.events_at_stop(2).unwrap().is_empty());
    for stop in recorded_stops.iter().copied() {
        let expected: Vec<u64> = trace
            .recording
            .instruction_events
            .iter()
            .filter(|event| event.stop_id_before == stop)
            .map(|event| event.event_id)
            .collect();
        let queried: Vec<u64> = store
            .events_at_stop(stop)
            .unwrap()
            .iter()
            .map(|event| event.event_id)
            .collect();
        assert_eq!(queried, expected, "stop {stop}");
    }
    // A stop before/after everything: valid, empty.
    assert!(store.events_at_stop(0).unwrap().is_empty());
    assert!(store.events_at_stop(u64::MAX).unwrap().is_empty());

    // Heap-event association follows the recorded cumulative counter. On
    // the fresh realloc_move trace the boundaries are known:
    // stop 8 → [allocated(1)], stop 12 → [allocated(2)], stop 24 →
    // [reallocated(3)], stop 40 → [freed(2)], stops without new heap
    // traffic → empty.
    let store = load(stage1_fixture("realloc_move.json"));
    let slice = store.heap_events_at_stop(8).unwrap();
    assert_eq!(slice.len(), 1);
    assert_eq!(slice[0].allocation_id, 1);
    assert_eq!(slice[0].event_type, HeapEventType::Allocated);
    let slice = store.heap_events_at_stop(24).unwrap();
    assert_eq!(slice.len(), 1);
    assert_eq!(slice[0].event_type, HeapEventType::Reallocated);
    assert!(store.heap_events_at_stop(16).unwrap().is_empty());
    assert!(store.heap_events_at_stop(41).unwrap().is_empty());
    // A stop without a retained snapshot has no boundary evidence.
    assert!(matches!(
        store.heap_events_at_stop(2),
        Err(QueryError::Unavailable { .. })
    ));
    // The boundaries partition the recorded heap events in order.
    let mut concatenated: Vec<u64> = Vec::new();
    let snapshot_stops: Vec<u64> = store
        .trace()
        .snapshots
        .iter()
        .map(|s| s.observation.stop_id)
        .collect();
    for stop in snapshot_stops {
        concatenated.extend(
            store
                .heap_events_at_stop(stop)
                .unwrap()
                .iter()
                .map(|e| e.allocation_id),
        );
    }
    assert_eq!(
        concatenated,
        store
            .heap_events()
            .iter()
            .map(|e| e.allocation_id)
            .collect::<Vec<_>>(),
        "stop boundaries must partition the chronological heap events"
    );

    // Broken stop ordering is corruption: queries refuse to answer.
    let mut corrupted = owned_trace(&store);
    corrupted.snapshots[0].observation.stop_id = 5;
    corrupted.snapshots[1].observation.stop_id = 5;
    let corrupted = TraceStore::from_trace(corrupted).unwrap();
    assert!(matches!(
        corrupted.snapshot_at_stop(5),
        Err(QueryError::OrderingViolation {
            ordering: "snapshot stop_ids",
            ..
        })
    ));
    let report = corrupted.validate_deep();
    assert!(!report.is_structurally_sound());
    assert!(report
        .findings()
        .iter()
        .any(|f| f.code == "SNAPSHOT_STOP_ORDER"));
}

// ---------------------------------------------------------------------------
// F — allocation identity: lookup is by engine id, never by address; the
// realloc chain (3 ← 1) stays resolvable; id 0 stays untracked.
// ---------------------------------------------------------------------------

#[test]
fn f_allocation_lookup_follows_identity_not_address() {
    let store = load(stage1_fixture("realloc_move.json"));

    // Recorded heap history: 1 alloc'd, 2 alloc'd, 3 realloc'd FROM 1,
    // 3 freed, 2 freed.
    assert_eq!(
        store.heap_event_positions_for_allocation(3),
        &[2, 3],
        "allocation 3's realloc + free, in chronological order"
    );
    assert_eq!(store.heap_event_positions_for_allocation(1), &[0]);
    assert_eq!(store.heap_event_positions_for_allocation(2), &[1, 4]);
    // Unknown ids and untracked id 0: valid empty results.
    assert!(store.heap_event_positions_for_allocation(7).is_empty());
    assert!(store.heap_event_positions_for_allocation(0).is_empty());

    // Final states resolve through the state array.
    let block = store.heap_block(3).expect("block 3");
    assert_eq!(block.realloc_from_id, Some(1), "realloc chain recorded");
    assert!(block.is_freed);
    assert!(store.heap_block(1).expect("block 1").is_freed);
    assert!(store.heap_block(7).is_none());
    assert!(store.heap_block(0).is_none(), "id 0 is untracked, no block");

    // Lifetimes resolve too.
    assert_eq!(
        store
            .allocation_lifetime(3)
            .expect("lifetime 3")
            .allocation_id,
        3
    );
    assert!(store.allocation_lifetime(7).is_none());

    // Addresses never key allocations: the v21_dual trace records one
    // allocation's two events at the same address; identity (not address)
    // keeps them together and no other allocation is merged in.
    let dual = load_example("v21_dual.json");
    let positions = dual.heap_event_positions_for_allocation(1);
    assert_eq!(positions, &[0, 1]);
    let addresses: Vec<&str> = dual
        .heap_events_for_allocation(1)
        .map(|event| event.address.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(addresses, ["0x705010", "0x705010"]);
    // An id absent from the trace has no events even though its *address
    // shape* exists in the data — there is no address-keyed API at all.
    assert!(dual.heap_event_positions_for_allocation(999).is_empty());
}

// ---------------------------------------------------------------------------
// G — partial traces validate as sound-but-incomplete; crashed and timed-out
// runs keep every recorded fact queryable.
// ---------------------------------------------------------------------------

#[test]
fn g_partial_traces_validate_sound_and_incomplete() {
    // Timeout trace: scope recorded as incomplete, recording terminated
    // mid-timeline, only 3 of many stops retained.
    let store = load_example("v21_bounded_infinite.json");
    let report = store.validate_deep();
    assert!(
        report.is_structurally_sound(),
        "partial coverage must not be corruption: {report:?}"
    );
    assert!(report.is_partial());
    assert!(
        report
            .findings()
            .iter()
            .any(|f| f.code == "SCOPE_INCOMPLETE"
                && f.severity == ValidationSeverity::PartialCoverage)
    );
    assert_eq!(report.corruption_count(), 0);

    // The retention gap is recorded, not fabricated: the last gap position
    // equals the attempted instruction count.
    let attempted = store.attempted_instruction_count();
    let retained = store.retained_event_count();
    assert_eq!(
        store.trace_gaps().last().unwrap().instruction_position,
        attempted,
        "gap recorded at the attempted-event boundary"
    );
    assert!(
        attempted >= retained as u64,
        "attempted {attempted} >= retained {retained}"
    );

    // Every recorded stop stays queryable despite the truncation.
    for stop in store
        .trace()
        .snapshots
        .iter()
        .map(|s| s.observation.stop_id)
    {
        assert!(store.snapshot_at_stop(stop).unwrap().is_some());
    }
    // Events after the last checkpoint still resolve via binary search.
    let last_checkpoint = *store.source_checkpoints().last().unwrap();
    assert!(!store
        .events_at_checkpoint(last_checkpoint)
        .unwrap()
        .is_empty());
    assert!(store
        .events_at_checkpoint(last_checkpoint)
        .unwrap()
        .windows(2)
        .all(|w| w[0].checkpoint_id <= w[1].checkpoint_id));

    // Crash trace: status recorded, crash report present, validation sound,
    // and the absent Valgrind report is optional absence — not corruption.
    let crash = load_example("v21_crash.json");
    assert_eq!(*crash.execution_status(), ExecutionStatus::Crashed);
    assert!(crash.crash_report().is_some());
    let report = crash.validate_deep();
    assert!(report.is_structurally_sound(), "{report:?}");
    if crash.valgrind_report().is_none() {
        assert!(report
            .findings()
            .iter()
            .any(|f| f.code == "VALGRIND_REPORT_UNRECORDED"
                && f.severity == ValidationSeverity::OptionalAbsence));
    }
}

// ---------------------------------------------------------------------------
// H — referential validation classifies mutations: corruption is typed as
// corruption, recorded partiality stays partiality, and findings aggregate.
// ---------------------------------------------------------------------------

#[test]
fn h_deep_validation_classifies_corruption_and_partiality() {
    let store = load_example("v21_dual.json");

    // Corruption 1: stop_id_after must be exactly before+1 when recorded.
    let mut corrupted = owned_trace(&store);
    let before = corrupted.recording.instruction_events[0].stop_id_before;
    corrupted.recording.instruction_events[0].stop_id_after = Some(before + 7);
    let report = TraceStore::from_trace(corrupted).unwrap().validate_deep();
    assert!(!report.is_structurally_sound());
    let finding = report
        .findings()
        .iter()
        .find(|f| f.code == "STOP_AFTER_NOT_NEXT")
        .expect("STOP_AFTER_NOT_NEXT");
    assert_eq!(finding.severity, ValidationSeverity::StructuralCorruption);

    // Corruption 2: an unresolvable instruction reference.
    let mut corrupted = owned_trace(&store);
    corrupted.recording.instruction_events[0].instruction_id = Some(999_999);
    let report = TraceStore::from_trace(corrupted).unwrap().validate_deep();
    assert!(report
        .findings()
        .iter()
        .any(|f| f.code == "INSTRUCTION_REFERENCE_INVALID"
            && f.severity == ValidationSeverity::StructuralCorruption));

    // Corruption 3: duplicate heap-state identity (aggregated).
    let mut corrupted = owned_trace(&store);
    let duplicate = corrupted.heap_state[0].clone();
    corrupted.heap_state.push(duplicate);
    let report = TraceStore::from_trace(corrupted).unwrap().validate_deep();
    let finding = report
        .findings()
        .iter()
        .find(|f| f.code == "HEAP_STATE_ID_DUPLICATE")
        .expect("HEAP_STATE_ID_DUPLICATE");
    assert_eq!(finding.severity, ValidationSeverity::StructuralCorruption);
    assert_eq!(finding.occurrences, 1, "aggregated: 2 entries, 1 duplicate");

    // Partiality (NOT corruption): a snapshot that never made it into the
    // journal checkpoint list — the trace-memory cap can stop journaling
    // after the snapshot is retained.
    let mut partial = owned_trace(&store);
    partial.recording.source_checkpoints.pop();
    let report = TraceStore::from_trace(partial).unwrap().validate_deep();
    assert!(
        report.is_structurally_sound(),
        "journal incompleteness is recorded partiality: {report:?}"
    );
    assert!(report.is_partial());
    assert!(report
        .findings()
        .iter()
        .any(|f| f.code == "CHECKPOINT_JOURNAL_INCOMPLETE"
            && f.severity == ValidationSeverity::PartialCoverage));

    // Optional absence: Valgrind/sanitizer reports absent on a completed
    // run are noted for consumers, never defects.
    let baseline = store.validate_deep();
    for code in ["VALGRIND_REPORT_UNRECORDED", "SANITIZER_REPORT_UNRECORDED"] {
        let finding = baseline.findings().iter().find(|f| f.code == code);
        assert!(
            finding.is_none() || finding.unwrap().severity == ValidationSeverity::OptionalAbsence,
            "{code} must be optional absence when present"
        );
    }
}

#[test]
fn h_invalid_references_and_missing_evidence_are_not_silenced() {
    let baseline = load_example("v21_dual.json");
    let mut trace = owned_trace(&baseline);
    trace.recording.instruction_events[0].event_id = 99;
    trace.recording.instruction_events[1].checkpoint_id = Some(usize::MAX);
    trace.recording.source_checkpoints[0] = usize::MAX;
    let store = TraceStore::from_trace(trace).unwrap();
    let report = store.validate_deep();
    for code in [
        "ENGINE_STRUCTURAL_VALIDATION",
        "CHECKPOINT_REFERENCE_INVALID",
        "SOURCE_CHECKPOINT_INVALID",
    ] {
        assert!(
            report.findings().iter().any(|finding| {
                finding.code == code && finding.severity == ValidationSeverity::StructuralCorruption
            }),
            "{code} missing: {report:?}"
        );
    }
    assert!(matches!(
        store.source_checkpoint_snapshot(0),
        Err(QueryError::OutOfRange {
            collection: "snapshots",
            ..
        })
    ));

    let mut trace = owned_trace(&baseline);
    trace.recording.instruction_events[0].address = "0xdeadbeef".into();
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(matches!(
        store.disassembly_for_event(0),
        Err(QueryError::InvalidReference { .. })
    ));
    assert!(store.validate_deep().findings().iter().any(|finding| {
        finding.code == "ENGINE_STRUCTURAL_VALIDATION"
            && finding.severity == ValidationSeverity::StructuralCorruption
    }));

    // An id-less event or missing source is unavailable evidence, not a
    // structural failure. A huge invalid id must never alias a valid
    // instruction key after narrowing to u32.
    let mut trace = owned_trace(&baseline);
    trace.recording.instruction_events[0].instruction_id = None;
    trace.recording.instruction_events[0].source.file = None;
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(matches!(
        store.disassembly_for_event(0),
        Err(QueryError::Unavailable { .. })
    ));
    assert!(store.validate_deep().is_structurally_sound());
    assert!(store.validate_deep().findings().iter().any(|finding| {
        finding.code == "SOURCE_CONTEXT_UNRECORDED"
            && finding.severity == ValidationSeverity::UnavailableEvidence
    }));

    let mut trace = owned_trace(&baseline);
    trace.recording.instruction_events[0].instruction_id = Some(1_u64 << 32);
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(matches!(
        store.disassembly_for_event(0),
        Err(QueryError::InvalidReference { .. })
    ));
    assert!(matches!(
        store.instructions_for_source("sample.c", 29),
        Err(QueryError::InvalidReference { .. })
    ));
    assert!(store
        .sources_for_instruction(0)
        .unwrap()
        .all(|key| key.file.as_ref() == "sample.c"));
}

#[test]
fn b_empty_and_sparse_checkpoint_lists_preserve_explicit_mapping() {
    let original = load_example("v21_loop.json");
    let mut trace = owned_trace(&original);
    trace.recording.source_checkpoints = vec![0, 4, 9, 15];
    let store = TraceStore::from_trace(trace).unwrap();
    assert_eq!(store.source_checkpoints(), &[0, 4, 9, 15]);
    assert_eq!(store.source_checkpoint_snapshot(2).unwrap().step_index, 9);
    assert!(store.validate_deep().is_structurally_sound());

    let mut trace = owned_trace(&original);
    trace.recording.source_checkpoints.clear();
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.source_checkpoints().is_empty());
    assert!(store.validate_deep().is_structurally_sound());
    assert!(store.validate_deep().is_partial());
    assert!(matches!(
        store.source_checkpoint(0),
        Err(QueryError::OutOfRange { .. })
    ));
}

#[test]
fn g_instruction_retention_disabled_does_not_invent_events() {
    let original = load_example("v21_loop.json");
    let mut trace = owned_trace(&original);
    let attempted = trace.recording.instruction_count;
    trace.recording.instruction_events.clear();
    trace.recording.coverage.instruction_capture_enabled = false;
    let store = TraceStore::from_trace(trace).unwrap();
    assert_eq!(store.attempted_instruction_count(), attempted);
    assert_eq!(store.retained_event_count(), 0);
    assert!(store.instruction_range(0..0).unwrap().is_empty());
    assert!(matches!(
        store.instruction_event(0),
        Err(QueryError::OutOfRange { .. })
    ));
    let report = store.validate_deep();
    assert!(report.is_structurally_sound(), "{report:?}");
    assert!(report.is_partial());
}

#[test]
fn e_corrupt_heap_counter_returns_error_without_panicking() {
    let original = load(stage1_fixture("realloc_move.json"));
    let mut trace = owned_trace(&original);
    trace.snapshots[2].observation.heap_event_count = trace.heap_events.len() + 1;
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(matches!(
        store.heap_events_at_stop(8),
        Err(QueryError::InvalidReference {
            field: "heap_event_count",
            ..
        }) | Err(QueryError::OrderingViolation { .. })
    ));
    assert!(store.validate_deep().findings().iter().any(|finding| {
        finding.code == "HEAP_COUNT_OVER_RECORDED"
            && finding.severity == ValidationSeverity::StructuralCorruption
    }));
}

#[test]
fn d_same_basename_paths_remain_distinct() {
    let original = load_example("v21_dual.json");
    let mut trace = owned_trace(&original);
    let first = trace.recording.instruction_events[0].clone();
    let mut second = first.clone();
    let id = trace.recording.instruction_events.len() as u64;
    second.event_id = id;
    second.source.file = Some("dir-b/same.c".into());
    second.source.line = Some(42);
    trace.recording.instruction_events[0].source.file = Some("dir-a/same.c".into());
    trace.recording.instruction_events[0].source.line = Some(42);
    trace.recording.instruction_events.push(second);
    trace.recording.instruction_count += 1;
    let store = TraceStore::from_trace(trace).unwrap();
    assert_eq!(
        store.executed_event_positions_for_source("dir-a/same.c", 42),
        &[0]
    );
    assert_eq!(
        store.executed_event_positions_for_source("dir-b/same.c", 42),
        &[id as u32]
    );
    assert!(store
        .executed_event_positions_for_source("same.c", 42)
        .is_empty());
}

// ---------------------------------------------------------------------------
// I — ownership & concurrency: clones share the index set (no rebuild),
// queries are lock-free reads across threads, and the store is Send+Sync.
// ---------------------------------------------------------------------------

#[test]
fn i_clones_share_indexes_and_queries_are_thread_safe() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TraceStore>();

    let store = load_example("v21_dual.json");
    let clone = store.clone();
    // Same Arc allocations: zero duplicate indexes, zero duplicate traces.
    assert!(
        std::ptr::eq(store.indexes(), clone.indexes()),
        "clones must share the index set"
    );
    assert!(std::ptr::eq(store.trace(), clone.trace()));
    // TP-1 behavior preserved: shared() hands out the same Arc.
    let shared: Arc<DebugSessionTrace> = store.shared();
    assert!(Arc::ptr_eq(&shared, &store.shared()));

    // Concurrent readers over one shared store (moved Arc, no locking
    // beyond the Arc refcounts).
    let shared_store = Arc::new(store);
    let handles: Vec<_> = (0..4)
        .map(|worker| {
            let store = Arc::clone(&shared_store);
            std::thread::spawn(move || {
                let mut seen = Vec::new();
                for id in 0..store.disassembly_len() as u64 {
                    if let Ok(events) = store.instruction_executions(id) {
                        seen.push((id, events.len()));
                    }
                }
                let stops: Vec<Option<u64>> = [1, 2, 3, 4, 5]
                    .map(|stop| {
                        store
                            .snapshot_at_stop(stop)
                            .unwrap()
                            .map(|s| s.observation.stop_id)
                    })
                    .to_vec();
                let allocations = store.heap_event_positions_for_allocation(1).to_vec();
                let _ = worker; // deterministic work below
                (seen, stops, allocations)
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    for result in &results[1..] {
        assert_eq!(result, &results[0], "all readers observe identical data");
    }
    assert!(!results[0].0.is_empty());
}

// ---------------------------------------------------------------------------
// J — fresh Stage 1 traces from the current engine build: every index
// cross-checks against the canonical arrays on all eight real fixtures.
// ---------------------------------------------------------------------------

#[test]
fn j_fresh_stage1_traces_index_and_validate() {
    // Fresh traces load and report their recorded statuses.
    let loop_store = load(stage1_fixture("loop.json"));
    assert_eq!(loop_store.trace().schema_version, 2);
    assert_eq!(*loop_store.execution_status(), ExecutionStatus::Completed);
    let realloc_store = load(stage1_fixture("realloc_move.json"));
    assert_eq!(
        *realloc_store.execution_status(),
        ExecutionStatus::Completed
    );
    assert_eq!(realloc_store.trace().heap_events.len(), 5);
    assert_eq!(realloc_store.trace().heap_state.len(), 3);

    // Exhaustive cross-validation on every real fixture: each index query
    // must equal the linear scan over the canonical arrays.
    for (path, name) in all_fixtures() {
        let store = load(&path);
        let trace = store.trace();
        let events = &trace.recording.instruction_events;

        // Executions index == linear scan for every disassembly id.
        for id in 0..trace.recording.disassembly_table.len() as u64 {
            let expected: Vec<u32> = events
                .iter()
                .enumerate()
                .filter(|(_, event)| event.instruction_id == Some(id))
                .map(|(position, _)| position as u32)
                .collect();
            assert_eq!(
                store.instruction_execution_positions(id).unwrap(),
                expected,
                "{name}: executions of instruction {id}"
            );
        }

        // Source index == linear scan for every recorded (file, line) key.
        let mut keys: Vec<(String, u32)> = events
            .iter()
            .filter_map(|event| {
                let file = event.source.file.as_ref()?;
                let line = event.source.line?;
                Some((file.to_string(), line))
            })
            .collect();
        keys.sort();
        keys.dedup();
        for (file, line) in keys {
            let expected_static: Vec<u32> = events
                .iter()
                .filter_map(|event| {
                    if event.source.file.as_deref() == Some(file.as_str())
                        && event.source.line == Some(line)
                    {
                        event.instruction_id.map(|id| id as u32)
                    } else {
                        None
                    }
                })
                .collect();
            let mut static_sorted = expected_static.clone();
            static_sorted.sort_unstable();
            static_sorted.dedup();
            // Set equality (sorted), since the index preserves
            // first-appearance order rather than numeric order.
            assert_eq!(
                {
                    let mut queried = store.instructions_for_source(&file, line).unwrap().to_vec();
                    queried.sort_unstable();
                    queried
                },
                static_sorted,
                "{name}: static mapping of {file}:{line}"
            );
            // First-appearance order: walk the events, dedup on the way.
            let mut first_appearance: Vec<u32> = Vec::new();
            for event in events {
                if event.source.file.as_deref() == Some(file.as_str())
                    && event.source.line == Some(line)
                {
                    if let Some(id) = event.instruction_id.map(|id| id as u32) {
                        if !first_appearance.contains(&id) {
                            first_appearance.push(id);
                        }
                    }
                }
            }
            assert_eq!(
                store.instructions_for_source(&file, line).unwrap(),
                first_appearance,
                "{name}: first-appearance order of {file}:{line}"
            );
            let expected_dynamic: Vec<u32> = events
                .iter()
                .enumerate()
                .filter(|(_, event)| {
                    event.source.file.as_deref() == Some(file.as_str())
                        && event.source.line == Some(line)
                })
                .map(|(position, _)| position as u32)
                .collect();
            assert_eq!(
                store.executed_event_positions_for_source(&file, line),
                expected_dynamic,
                "{name}: dynamic mapping of {file}:{line}"
            );
        }

        // Allocation index == linear scan for every recorded id.
        let ids: Vec<u64> = trace.heap_events.iter().map(|e| e.allocation_id).collect();
        for id in ids {
            let expected: Vec<u32> = trace
                .heap_events
                .iter()
                .enumerate()
                .filter(|(_, event)| event.allocation_id == id)
                .map(|(position, _)| position as u32)
                .collect();
            assert_eq!(
                store.heap_event_positions_for_allocation(id),
                expected,
                "{name}: events of allocation {id}"
            );
        }

        // Deep validation must classify every engine-written trace as
        // structurally sound; partial coverage is allowed and expected on
        // truncated runs.
        let report = store.validate_deep();
        assert!(
            report.is_structurally_sound(),
            "{name} must be sound: {report:?}"
        );
    }
}

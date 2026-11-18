# Trace Parser — TP-1 Ingestion, TP-2 Indexes, TP-3 Consumer Views

TP-1 is the foundational phase of the Trace Parser: the component that
receives Stage 1 (`c-trace-engine`) trace outputs, deserializes them with the
engine's own Serde models, validates their structure, and hands back **one
canonical, immutable, shareable owner of the loaded trace** — the
`TraceStore` — to be shared by every future consumer without re-parsing.

It is *not* a debugging engine, not a Semantic Engine, and not a GUI layer.
It never executes programs, invokes tools, reads live memory, or interprets
what the recorded data *means*. Recorded evidence is preserved verbatim;
deriving C-language meaning from it is Stage 2's job.

Location: `src/trace_parser/` inside the existing `c-trace-engine` crate
(single-crate repository; the models are already in a library target, so no
new crate and no refactoring were needed). Public API lives in
`src/trace_parser/mod.rs`, the error model in `error.rs`, JSON handling in
`json.rs`, journal handling in `journal.rs`, TP-2 indexes in `indexes.rs`,
explicit deep validation in `validation.rs`, and TP-3 views / trace-scoped
references in `views.rs` / `evidence.rs`.

---

## Supported inputs

| input | support | details |
|---|---|---|
| JSON trace (V2.1 / V2) | full | one `DebugSessionTrace` document, exactly as the engine prints to stdout / `--output`. Schema marker is the integer `2`; a trace **without** the field also loads (the model's serde default is 2 — the documented back-compat contract). Any other version is rejected. |
| Session journal (JSONL) | records only | `{"kind":"<record>","data":<value>}` per line, decoded and validated per record. **Full-trace reconstruction from a journal is unsupported** (see below). |
| Other encodings / versions | rejected | Non-UTF-8 input, schema versions ≠ 2, and content that is neither format are typed errors, never guesses. |

Format selection is **content-based** (`TraceFormatSelection::Auto` by
default, or forced with `Json` / `Journal`). File names and extensions are
never consulted, and malformed JSON is never silently reinterpreted as a
journal: journal detection requires every complete line to be a JSON object
carrying a string `kind` and a `data` member, with at most one torn final
line and at least one valid record before it.

### Resource limits (defaults in `TraceLoadOptions`)

* `max_bytes` — 512 MiB input cap. File loads check the size before reading;
  stream loads stop at the cap; over-limit inputs are rejected with a typed
  error, never truncated into a confusing parse failure.
* `max_journal_records` — 2,000,000 records.
* One journal line may never exceed `max_bytes` (bounds per-record buffering).

---

## Public API

```rust
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};

let store = TraceStore::open("examples/v21_normal.json", TraceLoadOptions::default())?;

store.trace();              // &DebugSessionTrace — the canonical data
store.shared();             // Arc<DebugSessionTrace> — cheap handle for future consumers
store.execution_status();   // Completed / Crashed / Timeout / StepLimitReached / ...
store.execution();          // exit code, signal, stop reason
store.session_meta();       // build config + toolchain versions, when recorded
store.source();             // path, exact byte length, detected format
```

* `TraceStore::open(path, options)` — from a file (size pre-checked; errors
  wrapped in `TraceParseError::Source` naming the path).
* `TraceStore::from_reader(reader, options)` — from any byte source; same
  validation, no filesystem access.
* `TraceStore::from_json_bytes(bytes, options)` — explicit JSON from memory
  (unit tests, pre-fetched bytes).
* `TraceStore::from_trace(trace)` — adopt an already-deserialized model
  (in-process engine output); only the schema-version gate runs.
* `parse_journal(path | reader | bytes, options)` — validated journal record
  stream (`JournalTrace`): typed `JournalRecord`s in file order, per-kind
  counts, `finalized` / `torn_tail` completeness flags.

### Ownership and sharing

The trace is held in one `Arc<DebugSessionTrace>` and the indexes in a
separate, shared `Arc<TraceIndexes>`. Loading parses exactly once; every read
borrows into the trace; `Clone` increments both Arcs and **never rebuilds**
indexes. The original input text is **not** retained — only compact
provenance metadata (`TraceSourceMetadata`: path, byte length, format).
There are no `&mut` methods, so cross-thread sharing is safe. Future
consumers (Query/Replay API, Semantic Engine) will each hold `shared()`
handles over the *same* data: one trace, one store, many views.

---

## What the parser deliberately does not do

* No renumbering of events, reassignment of allocation ids, address
  rewriting, reordering, merging of repeated instructions, or invented
  defaults. The round-trip tests assert the loaded model is value-identical
  to what Stage 1 wrote.
* No fabrication of missing observations. A failed memory read stays
  `None`/`not_captured`; a truncated trace stays truncated; skipped tool
  stages stay `Skipped`. Partial traces load as first-class stores.
* No reconstruction of journal fields that Stage 1 computes at session end
  (execution outcome/status, compile diagnostics, `heap_state`, allocation
  lifetimes, crash/Valgrind/sanitizer reports, resource-usage and profiling
  totals, coverage, `session_meta`, analysis notes, stage results) — those
  are never journaled, so requesting a `TraceStore` from journal input
  returns an explicit error that names `parse_journal` as the alternative.
* No GUI pagination, serialization or semantic interpretation (Stage 2).

---

## Journal details (as implemented by `storage::Journal`)

Record kinds and payload types:

| kind | payload |
|---|---|
| `configuration` | `config::TracingConfig` (always the first record) |
| `checkpoint` | `models::ExecutionSnapshot` |
| `allocation` | `{"observed_at_stop": u64, "event": HeapEvent}` |
| `gap` | `models::TraceGap` |
| `disassembly` | `models::DisassembledInstruction` |
| `instruction` | `models::InstructionEvent` |
| `finalized` | `true` (terminal control record) |

Reader policy:

* Every complete line must decode to a known kind — no silent skipping.
* A final line **without a trailing newline** is a suspected torn write
  (append-only, unbuffered writer): accepted only if it fully decodes,
  otherwise reported via `torn_tail: true` and never interpreted.
* Blank lines are tolerated; the first record of a non-empty journal must
  be `configuration`.
* Unknown record kinds are rejected by default; setting
  `allow_unknown_journal_records` preserves them verbatim as
  `JournalRecord::Unknown` (forward compatibility without fabrication).

---

## Known limitations

* **Monolithic JSON memory.** The whole file is read and deserialized at
  once; peak memory is roughly the input plus the parsed structures. There
  is no constant-memory or seek-based parsing (deferred beyond TP-2).
* **V2-era example files.** The four `examples/v2_*.json` files carry
  `max_register_snapshot_bytes` in `execution_limits` — a field removed
  from the models during the V2.1 register rework — and `ExecutionLimits`
  rejects unknown fields. This is Stage 1's pre-existing model contract
  (the engine itself never re-loads traces); the parser surfaces it as a
  typed error naming the field rather than guessing. Schema-version-2
  traces that use current fields parse fine.
* **Journal reconstruction** is explicitly unsupported (see above).
* Deprecated CPU-register fields (`registers`, `initial_registers`) remain
  parse-only exactly as the models define them; V2.1 traces omit them, and
  the parser does not reintroduce register processing.

## Testing

`cargo test` covers: unit tests for detection/decoding edge cases
(`src/trace_parser/*`), and integration tests in `tests/trace_parser.rs`
against **real Stage 1 fixtures** — the six `examples/v21_*.json` traces
(normal, dual, heap, crash, timeout, skipped-sanitizer) and the two journals
in `tests/fixtures/journal/` written by real engine runs. Categories:
normal ingestion, source/assembly fidelity (80 events over 58 addresses;
4146 events over 7 — repeats never collapsed), memory/heap fidelity (ids,
order, hex-exact bytes), partial traces, invalid inputs (empty, malformed,
truncated, wrong structure, bad version, bad field type, oversize, missing
  file, non-UTF-8), journal compatibility, and ownership/repeat access.

---

## TP-2: reference dictionary (verified in Stage 1)

These domains are **not interchangeable**. Index names alone do not establish
the semantics; the recording paths in `src/recorder.rs`, `src/trace.rs`, and
the model declarations in `src/models.rs` do.

| Field | Recorded meaning |
|---|---|
| `event_id` | Zero-based position in `recording.instruction_events`, contiguous when retained. |
| `instruction_id` | Zero-based position in `recording.disassembly_table`, *not* an execution id. |
| Event `checkpoint_id` | Zero-based `snapshots` position of the most recently retained snapshot; may be absent. |
| `source_checkpoints[]` | Snapshot indices, in explicit journaled order; **not** necessarily all snapshots. |
| Snapshot `step_index` | Its zero-based position in `snapshots`. |
| Snapshot `stop_id`; event `stop_id_before` / `stop_id_after`; allocation `observed_at_stop` | One-based processed **GDB stop counter**, with holes; unrelated to event/snapshot positions. `observed_at_stop` occurs on snapshot heap-block observations and journal allocation wrappers, **not** JSON `HeapEvent`. |
| Snapshot `instruction_position`, gap `instruction_position`; `instruction_count` | Attempted instruction-event positions and total attempted count; positions need not resolve in retained `instruction_events`. |
| `allocation_id`; `old_allocation_id`; `realloc_from_id` | Stable engine-assigned allocation identity; 0 means untracked, never a vector position. A realloc can assign a new id. Addresses may be reused across different ids. |
| Heap-event `sequence`; lifetime sequences | Runtime instrumentation counters, distinct from stops. |
| `activation_id`, `lexical_scope_id`; `StackFrameInfo.level` | Existing activation and lexical-scope identities; stack `level` is depth, 0 innermost. They are preserved verbatim, not indexed here. |
| `address`, `next_address`, allocation address | Recorded process addresses (hex strings), not identities; repeated addresses may represent distinct executions or allocations. |

### Index inventory and policy

All indexes hold **positions**, never copies of instruction, snapshot,
disassembly, heap, diagnostic, or byte records. Built **eagerly**, once in
`TraceStore::assemble()` after JSON deserialization or `from_trace()`;
readers borrow through immutable Arcs without locks. Eager construction
avoids per-group `OnceLock` complexity and ensures all indexes are initialized
together. It costs ~32.6 ms on the 94k-event release workload (see below).
No persistent indexes or lazy cache exist.

| Lookup (key → value) | Without index | TP-2 representation; build complexity; estimated incremental memory |
|---|---|---|
| Disassembly id → executed event positions | O(events) repeated scan | CSR: `u32` offsets by dense table id, `u32` event positions; O(events + disassembly). On the 94,306-event trace ~377 KiB (43 entries + all positions). |
| Exact recorded `(file,line)` → static disassembly ids and dynamic event positions; instruction id → source locations | O(events) repeated scan | One interned-path list and `(file-id,line)` map, two CSRs by source key plus reverse CSR by disassembly id; O(events + disassembly). Static ids deduplicated in **first-appearance** order, dynamic event positions preserve **every** visit in recorded order. On the 94k trace ~369 KiB estimated. |
| Allocation id → heap-event positions, final block position, lifetime position | O(heap events + state + lifetimes) | Sparse `HashMap<u64, Vec<u32>>` plus two `HashMap<u64,u32>`; O(total heap records). For 333 events / 167 blocks, ~21 KiB estimate for the whole index set; justified for repeated lookups at this scale, not faster than scanning a 2–5-event trace. No address key. |
| `event_id`, `instruction_id`, snapshot position, source-checkpoint position → canonical record/reference | Already O(1) | **No index.** Direct vector access; source-checkpoint position first reads the explicit `source_checkpoints[]` index, *then* indexes `snapshots`. |
| GDB stop → snapshot; GDB stop / checkpoint → event run | O(snapshots) / O(events) | **No position index.** Verified monotonic ordering flags + binary searches on canonical arrays: O(log snapshots/events), zero per-record memory. Stop-to-snapshot binary search loses to linear scan on three snapshots, but wins on a 1,000-snapshot miss; no extra index is warranted. |
| Stop → heap-event boundary | O(snapshots + heap events) | No index; uses the two neighboring **retained** snapshots' cumulative `observation.heap_event_count` counters. Dropped snapshots coarsen attribution; finer stop attribution cannot be recovered from complete JSON. |

`TraceIndexes::estimated_bytes()` is a rough logical estimate, not a precise
heap accounting tool (hash buckets and vector capacity vary). Measured live
allocator deltas and RSS appear below. Building from untrusted traces retains
only valid positional references in the lookup structures; **explicit deep
validation** detects broken references. Missing source/disassembly evidence
does not cause invented records.

### Raw query API and ordering

All accessors borrow the canonical records; returned slices are half-open and
never clamped. `TraceStore::trace()` / `shared()` still expose original data.

| Methods | Semantics |
|---|---|
| `snapshot`, `snapshots_in_range`, `instruction_event`, `instruction_range`, `disassembly_entry`, `source_checkpoint`, `source_checkpoint_snapshot`, `source_checkpoints` | O(1) direct positional access / borrowed slice; `source_checkpoint(n)` returns the *stored snapshot index* at position `n`. |
| `attempted_instruction_count`, `retained_event_count` | Distinct attempted count and retained vector length; never fill retention holes. |
| `snapshot_at_stop`, `events_at_stop`, `events_at_checkpoint` | Verified-order binary search; missing snapshot at a stop returns `Ok(None)`, missing event run returns an empty borrowed slice. `events_at_stop` matches `stop_id_before` (not `stop_id_after`). |
| `heap_events_at_stop` | Borrowed chronological range using cumulative snapshot heap counts. Missing boundary yields `Unavailable`; malformed counters yield error. Journal wrappers have finer `observed_at_stop` but cannot construct a full store. |
| `instructions_for_source`, `executed_event_positions_for_source`, `executed_events_for_source`, `sources_for_instruction`, `source_files`, `event_source` | **Static** ids vs **dynamic** executions are separate questions. `instructions_for_source` returns `QueryResult<&[u32]>` and errors if recorded ids at that key are invalid; unknown keys return `Ok(&[])`. Source keys compare complete recorded path strings verbatim; identical basenames do not merge. Reverse mapping derives from event evidence (disassembly entries have no source fields). |
| `disassembly_for_event`, `instruction_execution_positions`, `instruction_executions` | One event → one recorded table entry (address checked); one table entry → all its distinct executions, ordered by event position. No synthesized assembly. |
| `heap_event_positions_for_allocation`, `heap_events_for_allocation`, `heap_block`, `allocation_lifetime` | Identity-based positions/borrowed records, preserving chronological order and realloc references. ID 0 yields untracked traffic, not a fabricated block. |
| `trace_gaps`, `heap_events`, `heap_state`, `allocation_lifetimes`, `compile_errors`, `crash_report`, `valgrind_report`, `sanitizer_report`, `stage_results`, `internal_diagnostics`, `coverage`, `storage`, `profiling`, `resource_usage`, `execution_limits`, `tracing_configuration` | Raw evidence and diagnostics as borrowed data, no inference, copying, or cross-tool joins. Variable/frame/byte observations remain accessible via borrowed snapshots. |
| `indexes`, `validate_deep` | Shared index metadata; **explicit** full cross-record audit returning typed findings, not performed during TP-1 ingestion. |

`QueryError` distinguishes `OutOfRange` (positional), `InvalidRange`,
`InvalidReference` (corrupt recorded reference or address mismatch),
`Unavailable` (needed observation absent), and `OrderingViolation` (broken
binary-search precondition). Valid empty result means no recorded match,
**not** fabricated evidence. `TraceParseError` retains the TP-1 ingestion
contract. Ordering of canonical arrays is never changed: events and
allocations follow ingest order, snapshot order is positional, checkpoint
order is explicit, and CSR groups preserve event order. A corrupt ordering
causes dependent range queries to fail rather than silently mis-search.

### Deep validation

`validate_deep()` aggregates findings by `(severity, code)` with first
location, detail and occurrence count; it does not reject the store. It
reuses `processor::validate()` for event-id contiguity, table
ID/address congruence, checkpoint references and attempted/retained counts
where the engine requires them. Additional structural checks cover
`step_index` positional equality, stop/event/checkpoint/gap chronological
order, `stop_id_after` successor, source-checkpoint bounds and order,
snapshot and gap attempted positions, heap-count bounds and monotonicity,
unique heap-state/lifetime ids. Missing journal checkpoint entries, coverage
gaps and incomplete scope are **partial coverage**; absent source mapping,
memory/variable bytes or unresolved realloc origin are **unavailable
evidence**; absent optional diagnostic reports and stop-end addresses are
**optional absence**. Crashes, timeouts, skips and retention caps remain
loadable. No automatic deep-validation rejection was added to TP-1.

### Measured release performance (2026-11-16)

Host: Intel Pentium 3558U @ 1.70 GHz, 7.6 GiB RAM, Linux; `cargo build
--release --lib`, `rustc -O` release harness in
`/tmp/tp1_audit/bench_tp2.rs` (equivalent reproducible harness:
`cargo run --release --example trace_parser_tp2_bench -- TRACE.json`).
Each run reads bytes then measures
`serde_json::from_slice` separately from `TraceStore::from_trace`; all
queries consume a `black_box` result. Direct vector lookup uses 100,000
iterations; scans use 500 iterations on the large trace and 4,000 on small
traces; indexed lookups use 10× those iterations. Timings are per query
from a **single run** (not confidence intervals), with successful and
unsuccessful keys. Full output: `/tmp/tp1_audit/bench_tp2_results.txt`.

| Input | events / heap events / snapshots | parse | eager index build | extra live allocated bytes† | RSS before → after index |
|---|---:|---:|---:|---:|---:|
| `v21_normal.json` (11,622 B) | 0 / 0 / 4 | 0.217 ms | 0.044 ms | 3,192 B | 3,000 → 3,080 KiB |
| `v21_loop.json` (146,580 B) | 97 / 0 / 29 | 1.46 ms | 0.091 ms | 5,268 B | 3,436 → 3,448 KiB |
| `v21_dual.json` (196,713 B) | 80 / 2 / 25 | 1.86 ms | 0.130 ms | 6,532 B | 3,408 → 3,484 KiB |
| fresh `realloc_move.json` (159,706 B) | 37 / 5 / 12 | 1.25 ms | 0.087 ms | 5,656 B | 3,372 → 3,384 KiB |
| fresh source-mode `many_allocs_tp2.json` (3,643,718 B) | 0 / 333 / 1,000 | 30.9 ms | 0.180 ms | 23,064 B | 12,128 → 12,140 KiB |
| `large_spin3.json` (48,654,709 B) | 94,306 / 0 / 3 | 557.1 ms | 34.4 ms | 758,284 B (~0.72 MiB) | 208,024 → 208,100 KiB |

† A counting global allocator measures *live requested bytes* after parsing
minus after index assembly (including Arc and allocator capacity), rather
than page-level RSS. RSS alone understates memory when the allocator reuses
pages committed during deserialization. The large benchmark holds the raw
46.4 MiB input buffer during measurement, which explains why its RSS
exceeds the prior TP-1 post-load ~157 MiB baseline.

| Query (ns/query) | Linear scan | TP-2 | Note |
|---|---:|---:|---|
| Large: instruction hit / miss | 915,155 / 914,642 | 7.1 / 9.5 | CSR dominates repeated full scans. |
| Large: source hit / miss | 4,732,517 / 4,823,655 | 60.9 / 51.8 | Exact `(file,line)`, all visits retained. |
| Loop (97 events): instruction hit / miss | 144.2 / 136.4 | 7.0 / 7.0 | Repeated execution lookup. |
| Dual (80 events): source hit / miss | 515.2 / 493.2 | 63.4 / 52.8 | Small source/assembly workload. |
| 333 heap events: allocation hit / miss | 272.5 / 273.6 | 44.3 / 43.6 | Hash index pays off at moderate cardinality. |
| 5 heap events: allocation hit | 7.1 | 40.4 | Index is *slower* here; tiny traces do not benefit. |
| 1,000 snapshots: stop miss | 1,565.9 | 46.5 | Binary search, no stored index. |
| 3 snapshots: stop hit | 1.9 | 17.0 | Linear wins; binary lookup remains zero-memory. |
| Large: direct `instruction_event` | N/A | 21.4 | Already O(1); no new positional index. |

Index build is ~6.2% of large-trace deserialization time in this run.
The fast cases justify eager CSR construction for repeated execution/source
lookups; ~0.72 MiB added live memory on 94k events is small relative to
deserialization. The heap index is justified by moderately sized allocation
traces but costs a few kilobytes on tiny runs; future workload data may
motivate a size threshold. Stop lookup stores **no** per-stop map because
binary search and direct access already suffice.

Final regression: `cargo test -- --test-threads=1` — **185 tests passed**:
122 library, 12 Stage 1 integration, 13 TP-1 integration, 15 TP-2 integration,
22 remaining integration, one doctest; zero failures. Single-threaded test
execution avoids contention between real debugger subprocess tests in
different binaries. `cargo fmt --check`, `cargo clippy --all-targets --
-D warnings`, and `git diff --check` pass. The TP-2 integration tests in
`tests/trace_parser_indexes.rs` cover positional and range access,
noncontiguous/missing source checkpoints, static vs dynamic source mappings,
stop holes, heap realloc/identity, partial traces and corrupt references,
shared ownership/concurrent reads, and eight real traces including two fresh
Stage 1 fixtures in `tests/fixtures/stage1/`. Existing TP-1 and Stage 1 tests
remain unchanged.

---

## TP-3: two borrowed consumer views

The final ownership diagram is:

```text
V2.1 JSON → TraceStore { Arc<DebugSessionTrace>, Arc<TraceIndexes> }
                  ├─ raw()            → RawTraceView<'_> (one &TraceStore)
                  └─ semantic_input() → SemanticInputView<'_> (one &TraceStore)
```

Neither view allocates or creates a new trace owner; both borrow the exact
same store and TP-2 indexes. The raw view implements
`Deref<Target = TraceStore>` to **delegate**, not reimplement, every existing
TP-1/TP-2 query (bounded instruction/snapshot ranges, source/disassembly
mapping, stop queries, allocation histories, diagnostic and coverage access).
`semantic_input()` additionally offers checkpoint observation contexts,
ordered iteration, and evidence-reference resolution. It is not a Semantic
Engine. A raw consumer never depends on a semantic consumer.

```rust
use c_trace_engine::trace_parser::{
    Evidence, EvidenceLocation, TraceLoadOptions, TraceStore,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = TraceStore::open("examples/v21_dual.json", TraceLoadOptions::default())?;
    let raw = store.raw();
    let events = raw.instruction_range(0..5)?; // half-open borrowed slice
    let entry = raw.disassembly_for_event(0)?;
    let semantic = store.semantic_input();
    let context = semantic.source_checkpoint_context(0)?;
    let original = semantic.resolve_evidence(&context.snapshot_evidence()?)?;
    assert!(matches!(original, Evidence::Snapshot(_)));
    let event_ref = raw.evidence_ref(EvidenceLocation::InstructionEvent(0))?;
    assert!(matches!(semantic.resolve_evidence(&event_ref)?, Evidence::InstructionEvent(_)));
    let _ = (events, entry);
    Ok(())
}
```

### Checkpoint context and availability

`checkpoint_context(snapshot_index)` borrows **any** retained snapshot;
`source_checkpoint_context(position)` follows the *explicit* list and also
reports `Some(position)` as the path by which it was obtained. An ordinary
snapshot context's `source_checkpoint_position()` is `None` meaning
**unasserted**, not proof the snapshot is absent from the list.
`semantic.snapshots()` iterates *all* snapshots in order; the separate
`semantic.source_checkpoints()` iterates the journaled list in its original
order, including repeated positions if present, and returns errors for bad
entries. Both traversals are O(1) per context, with no temporary vectors.

`CheckpointContext` borrows the original snapshot, frames, local variables,
globals and heap-block observations. It exposes optional source `(file,line)`
and optional one-based GDB stop id; legacy/default empty file/zero line and
zero stop are **not** upgraded into identities. It preserves the attempted
instruction position, `observation.complete`, `memory_truncated`, errors,
variable `read_status` / `bytes_status`, raw bytes and overall `coverage`.
`events_at_checkpoint()` returns only events bearing *this* snapshot index
as their recorded `checkpoint_id`; it does not perform a nearest-event join.
`heap_events_in_snapshot_interval()` uses adjacent retained snapshot
heap-event-count boundaries. Its events may have been ingested at **earlier
unretained stops**; it does *not* claim each occurred at this exact stop or
instruction. If stop evidence is unavailable, the query returns
`QueryError::Unavailable`. Creating a context is O(1); querying its event
window is O(log E), its heap interval O(log S). No O(E) scan is hidden in
sequential checkpoint traversal.

### Typed evidence references

`EvidenceLocation` distinguishes original positions in snapshots, instruction
events, disassembly, heap events/final state/lifetimes, gaps, compiler
diagnostics, internal diagnostics, structured Valgrind errors, optional
reports, and nested frame/variable/global/heap-block/memory observations.
`store.evidence_ref(location)` validates the position *before* creating a
small `EvidenceRef`; `store.resolve_evidence(&reference)` returns a typed
borrowed `Evidence<'_>`. Optional absent reports return `Unavailable`;
out-of-range positions return `InvalidReference`. Both views use these same
store methods. A reference holds a **weak identity** of the canonical trace,
not its data: clones of a store can resolve it, but an independent load of
identical JSON returns `QueryError::ForeignTrace`. Weak identity prevents
address reuse after the original store is dropped from turning an old
reference into a valid new one. References are **in-process, trace-local,
non-serializable**: they are not stable cross-session IDs. Allocation IDs
and stop counters are runtime identities, not evidence positions.

### TP-3 performance and cache decision (release, 2026-11-16)

`cargo run --release --example trace_parser_tp3_bench -- TRACE.json [...]`
measures parse and TP-2 indexing separately, then runs **seven rounds** per
TP-3 operation, reporting median / nearest-rank p95 (maximum of seven
samples). `black_box` consumes varying positional queries; a counting
allocator reports live bytes and allocation calls. Hardware: Intel Pentium
3558U @ 1.70 GHz, 7.6 GiB RAM, Linux. Results are *microbenchmarks*, not
confidence intervals or measurements of a future Semantic Engine. Complete
raw output: `/tmp/opencode/tp3_bench_results.txt`,
`/tmp/opencode/tp3_bench_manyalloc.txt` in this audit environment.

| Operation | Dual trace (25 snapshots / 80 events), median / p95 | Large spin trace (3 snapshots / 94,306 events), median / p95 | Many-allocs trace (1,000 snapshots / 333 heap events), median / p95 |
|---|---:|---:|---:|
| `raw()` creation | 2.0 / 2.8 ns | 2.0 / 2.1 ns | 2.0 / 2.1 ns |
| `semantic_input()` creation | 2.0 / 2.4 ns | 2.0 / 2.7 ns | 2.0 / 2.8 ns |
| Varied `checkpoint_context` | 12.9 / 16.1 ns | 12.8 / 13.0 ns | 12.8 / 13.3 ns |
| Traverse **all** source checkpoints | 150.9 / 163.3 ns | 21.2 / 30.9 ns | 5,992 / 6,064 ns |
| Traverse with `events_at_checkpoint` | 2,071 / 2,385 ns | 611 / 646 ns | 13,371 / 13,533 ns |
| Varied evidence resolution | 33.2 / 75.6 ns | 33.3 / 37.5 ns | 33.5 / 35.6 ns |
| Bounded instruction range (≤16) | 13.0 / 13.3 ns | 13.0 / 13.3 ns | N/A (retention disabled) |
| Source→static instruction IDs | 61.7 / 65.8 ns | 61.9 / 65.9 ns | N/A |
| Four parallel readers, 20,000 context queries / batch | 0.369 / 0.468 ms | 0.444 / 0.567 ms | 0.429 / 0.522 ms |

These results include **0 persistent bytes and 0 allocation calls** for
creating the first two views and context. The repeated single-threaded view,
context, traversal, evidence, range and source queries also counted **0
allocations**. Parallel batch measurements include thread-spawn/join
overhead (637 allocator calls for 49 batches): they do **not** indicate
query-layer allocations. Seven samples are few, and nanosecond values are
subject to clock overhead, optimizer effects, system load and branch/cache
state. No cache was introduced: even 1,000 checkpoint contexts plus their
event-window queries take ~13.4 µs per full traversal in this run.

Complete benchmark workloads also covered normal, finite loop, source/
assembly, fresh recursive call stacks (five frames, fixture generated with
`fixtures/c/recursive.c`), heap, crash, timeout, and fresh realloc traces.
`/tmp/tp1_audit/large_spin3.json` and the source-mode many-allocation trace
were generated for the prior TP-2 audit. A fresh-run parse/build of the
94,306-event trace took 614.8 / 32.3 ms, *separate* from view creation; the
audit process retained the raw JSON buffer. No new persistent TP-3 index,
cache, database, or source-string copy is built.

### Concurrent debugger-test reliability

Stage 1 integration binaries spawn real GCC/GDB/Valgrind processes. Their
within-binary semaphores (`tests/v2.rs` recording and
`tests/integration.rs` Valgrind) do **not** serialize subprocesses across
binaries. Short 5-second trace deadlines can expire under simultaneous tool
load; one existing supervisor cancellation test also sleeps a fixed 100 ms
before asserting the shell has emitted a line, leaving a scheduling race.
One TP-3 parallel library run saw that assertion fail; the isolated rerun
and a subsequent complete default-parallel suite passed. A prior TP-2 run
also had transient integration failures under default parallel load, while
isolated and sequential runs passed. These are recorded test-harness timing
risks, **not proof** of parser regressions. No Stage 1 process supervisor or
debugger behavior was changed in TP-3; use `cargo test -- --test-threads=1`
for the stable regression count and keep parallel runs as an additional
signal.

Final TP-3 regression: `cargo test -- --test-threads=1` — **194 passed,
0 failed** (122 library, 12 Stage 1 integration, 13 TP-1 integration,
15 TP-2 integration, 9 TP-3 view/evidence integration, 22 other integration,
one doctest). A separate `cargo test` default-parallel run also passed all
194. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
`git diff --check` pass. TP-3 tests are in `tests/trace_parser_views.rs`;
all TP-1/TP-2/Stage 1 code and existing test files are otherwise unchanged
except the parser's new views and typed query-error variant.

### Stage 2 handoff: recorded input versus missing evidence

The parser exposes variable `activation_id`, `lexical_scope_id`,
declaration and identity fields, stack frames/levels, optional type, size,
raw bytes and pointer target metadata, heap IDs/realloc references, runtime
sequences, read-failure statuses and diagnostic records. A fresh recursive
trace recorded five stack frames; representative V2.1 traces contain
activation/scope IDs and variable bytes where captured. **None** of this is
converted into C object identities or a pointer graph by the parser.

The future Semantic Engine must handle missing DWARF/scopes, optimized-out
or unreadable variables, ambiguous frame identity across stops, unobserved
memory between checkpoints, partial heap history, untracked allocation id 0,
head-truncated output, incomplete source/assembly coverage, skipped tools,
and absent full register snapshots (V2.1 removed per-instruction registers).
Heap state bytes in the finalized trace can be historical (see
`observed_at_stop`), not current at every checkpoint. A type name and raw
bytes alone do not prove object layout, pointer ownership, aliasing,
liveness or state-change causality. Evidence-backed diagnostics need
Stage 2 interpretation and must retain the raw reference and uncertainty.
Neither journal-only reconstruction nor arbitrary process-memory reads are
supported.

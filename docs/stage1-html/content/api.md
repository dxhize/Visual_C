# Public API reference · Trace Engine and Parser

## Level 1 · Intuition

The engine creates a `DebugSessionTrace` from C source; the parser makes that finished recording navigable. **Loading, deep validation and semantic confidence are three separate operations.** Use trace-scoped references for identity, explicit bounded pages for display, and the report for import integrity.

{{diagram:handoff}}

## Level 2 · Ownership and query path

`TraceConfig`/`BuildConfig` → async `trace_program` → JSON export; then `TraceStore::open` → `Arc<DebugSessionTrace>` + `Arc<TraceIndexes>` → `validated_semantic()` → borrowed `CheckpointSummary`/`EvidenceRef`. Cloning a store shares owners; constructing an independent store changes reference identity. All queries borrow canonical records; `source_expression(id)` and bare `heap(id)` are **trace-local string/numeric lookups**, not portable identity tokens. A whole-trace `ValidationReport` is created once per validated view; pages do not revalidate.

## Level 3 · Signatures and contracts

### Record and load

| Public item (path) | Result / contract |
|---|---|
| `trace_program(config: TraceConfig) -> Result<DebugSessionTrace>` (`src/trace.rs`, async) | Runs supervised engine; failure trace is still a structured result when possible |
| `TraceStore::open(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<Self>` | Schema-2 JSON file; 512 MiB default input cap |
| `TraceStore::from_json_bytes(bytes: &[u8], options: TraceLoadOptions) -> Result<Self>` | Explicit one JSON document |
| `TraceStore::from_reader(reader: impl Read, options: TraceLoadOptions) -> Result<Self>` | Bounded stream read |
| `TraceStore::from_trace(trace: DebugSessionTrace) -> Result<Self>` | **Schema-only** gate; does not run `validate_deep` |
| `parse_journal(path, options) -> Result<JournalTrace>` | Ordered JSONL **records only**, no reconstructed TraceStore |
| `store.trace() -> &DebugSessionTrace`, `store.shared() -> Arc<DebugSessionTrace>` | Canonical immutable owner; clone shares indexes |

### Raw and referenced navigation

| Public item | Use |
|---|---|
| `store.snapshot(index)`, `instruction_event(index)`, `instruction_range(range)` | Original position/half-open borrowed records; raw TP-2 ranges are **not** TP-7 pages |
| `store.events_at_checkpoint(snapshot_index)`, `snapshot_at_stop(stop_id)` | Explicit tag run and verified-order stop lookup; stop ID is not snapshot index |
| `store.instruction_executions(instruction_id)`, `executed_events_for_source(file,line)` | Preserve repeated dynamic visits; exact recorded source strings |
| `store.heap_events_for_allocation(id)`, `heap_block(id)`, `allocation_lifetime(id)` | Trace-local allocation ID, not address or event position |
| `store.raw()`, `store.semantic_input().checkpoint_context(index)` | Borrowed canonical source and checkpoint context |
| `store.evidence_ref(location)`, `store.resolve_evidence(&reference)` | Trace-scoped `EvidenceRef`; foreign store → `ForeignTrace` |

### Validation and qualified evidence

| Public item | Contract |
|---|---|
| `store.validate_deep() -> ValidationReport` | Opt-in structural, partial, unavailable and optional findings |
| `store.validated_semantic() -> ValidatedSemanticView<'_>` | Once-deep-validated borrower; `view.report()` must be inspected |
| `view.variable(&EvidenceRef)`, `view.variable_target(&VariableEvidence)` | Variable/byte/pointer and same-stop variable-identity relationship |
| `view.heap(id)`, `view.heap_reference(&EvidenceRef)` | Identity and *separate* realloc predecessor confidence |
| `view.source_expression(id)` | Static metadata and call candidates; **not execution** |
| `view.checkpoint(index)`, `view.execution()`, `view.process_coverage()` | Stop and independent execution/process dimensions |
| `view.diagnostics()`, `view.valgrind_error(index)` | Independent reports; never a primary-run Valgrind allocation ID |

### TP-7 pages (all `range: Range<usize>` are half-open)

| Public item | Returned evidence |
|---|---|
| `view.checkpoint_summary(index)` | Borrowed `CheckpointSummary<'_, '_>` and snapshot reference |
| `stop.variables_page(VariableScope::{Local, Global, Caller(position)}, range)` | TP-6 variable confidence; caller *position* is not GDB level |
| `stop.instruction_events_page(range)`, `stop.heap_interval_page(range)` | Explicit tag / candidate interval; not exact event cause |
| `stop.gaps_in_attempted_window(range)`, `stop.source_expressions_page(range)` | Position-window gaps / optional static syntax |
| `stop.caller_frame(position)`, `stop.frame_activation(id)` | Same-snapshot activation; missing association is not guessed |
| `view.heap_events_page(&EvidenceRef, range)`, `view.valgrind_errors_page(range)` | Trace-scoped allocation event rows / independent diagnostics |
| `stop.coverage()`, `stop.session_diagnostics()`, `stop.first_location_findings()` | Distinct scope, session reports and finding's first location |

All TP-7 page widths are `≤ MAX_CONSUMER_PAGE` (128); an oversize request returns `QueryError::LimitExceeded`, not a truncated iterator. Raw `TraceStore::instruction_range` has its separate slice API; **do not misstate the TP-7 bound as a universal input/trace-size cap**. See [pagination implementation](parser-consumer.html#level-3-real-signatures-and-bounded-traversal) and the [runnable example](usage.html#level-3-load-and-validate).

> NOTE: Type annotations above summarize the public methods; some pages return `impl Iterator<Item=QueryResult<_>>` rather than a copied vector. Consult the named Rust source for complete generic and lifetime parameters.

# TP-7 — Consumer-facing, bounded evidence composition

**Status:** checkpoint-centric read-only API over **the existing TP-6 `ValidatedSemanticView`**. It does not introduce another trace owner, confidence model, or evidence-ID space. Read with `trace-parser-v2.2-tp5.md`, `trace-parser-v2.2-tp6.md`, `trace-parser.md` and `trace-engine-v2.2-release-closure.md`. No Trace Engine production behavior, Semantic Engine, frontend or TP-8 was implemented.

## Public API and ownership

`src/trace_parser/consumer.rs` extends `ValidatedSemanticView` with `checkpoint_summary(snapshot_index)`. This borrows TP-6's **once-validated** store and `CheckpointEvidence`; it stores one TP-3 `EvidenceRef` to the retained snapshot. Building/querying a summary does **not** rerun `validate_deep`, clone a trace/registry, or cache a graph. The canonical `Arc<DebugSessionTrace>`, shared position-only indexes, existing borrowed TP-1/2/3/5/6 methods, schema v2 and V2.1 defaults remain intact. `TraceStore::from_trace` still gates only schema, not structural soundness; request `validated_semantic()` and inspect its report.

```rust
use c_trace_engine::trace_parser::{TraceStore, VariableScope};
# let store = TraceStore::open("examples/v21_dual.json", Default::default())?;
let validated = store.validated_semantic(); // once, not once per page
let stop = validated.checkpoint_summary(0)?;
let (execution, processes) = stop.coverage(); // independent dimensions
let _ = (stop.reference.location(), stop.stop_id(),
         stop.attempted_instruction_position(), stop.source_location(),
         execution.retained, processes.unobserved_descendants);
for item in stop.variables_page(VariableScope::Local, 0..stop.checkpoint.context.variables().len().min(128))? {
    let variable = item?;
    let _ = (variable.reference.location(), variable.value_confidence,
             variable.memory.valid_range, variable.pointer.as_ref());
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

| Query | Contract |
|---|---|
| `CheckpointSummary` | `reference`, `checkpoint`, `snapshot_index`, `stop_id`, attempted `instruction_position`, recorded `pid`, recorded `source_location`, and `observation_complete`. These are *different identities*: snapshot vector index ≠ GDB stop counter ≠ attempted instruction index ≠ retained event position. `None` is missing recorded stop/source evidence, not an invented value. |
| `variable(scope, index)`, `variables_page(scope, range)` | `VariableScope::{Local, Global, Caller(caller_frames_position)}`; each result is the **same** TP-6 `VariableEvidence` (value/type/declaration/scope/activation/read/bytes/pointer confidence), with a trace-scoped `EvidenceRef`. Names and addresses never merge shadowed activations. Caller position is not a GDB frame level. |
| `caller_frame(position)`, `frame_activation(id)` | Borrow the recorded caller frame with its activation ID, errors, `EvidenceRef` and qualified confidence; frame-activation view offers an O(1) slice-backed `variables_page` at this snapshot only. Missing legacy activation ID yields `None`, duplicate/mixed associations do not choose a first match. Globals have no frame activation. |
| `instruction_events_page(range)` | Only events **explicitly tagged** `checkpoint_id=Some(snapshot_index)`, with canonical event position/`EvidenceRef` and definite *tag* confidence. This does **not** establish that an event occurred at that exact stop, or that a same-line AST expression executed. Event outcome and attempted count remain independent. |
| `heap_interval_page(range)` | Borrow events bracketed by the preceding and current **retained snapshot's cumulative heap counters**, with event position/ref. The interval association is `Candidate` timing: skipped stops coarsen attribution; no exact stop or pointer origin is inferred. `item.allocation(&validated)` uses the TP-6 trace-scoped heap reference, retaining incomplete/unknown-history confidence. |
| `heap_events_page(&allocation_ref, range)` | Paginate TP-2 positions of recorded events for a heap event/lifetime/block `EvidenceRef`. Foreign references fail **before** numeric ID lookup. Each returned event has separate `observed_confidence` (observed record) and `history_confidence` (identity/completeness); ID 0 remains untracked. Not an address join. |
| `gaps_in_attempted_window(range)` | Borrow indexed-position citations for gaps **after the preceding retained checkpoint's attempted position through this checkpoint's position** (first window includes position zero). Verified ordering + binary search; reverse-order imports error. This is a position window, **not** proof a gap happened *at* the GDB stop. |
| `source_expressions_page(range)` | Optional Clang AST expressions whose recorded span contains the recorded checkpoint line and matches the *same-stop* GDB frame's `full_file` when available, otherwise the snapshot file **exactly**. No basename substitution or cross-run source identity. TP-6 `SourceExpressionEvidence` supplies existing indexed call candidates/ambiguities; source-location confidence is at most `Candidate` and `execution_confidence` remains unavailable. Cross-provider correlation marked unavailable stays unavailable. |
| `coverage()`, `session_diagnostics()`, `session_report_refs()`, `valgrind_errors_page(range)` | Separate execution/process scope, raw reports and trace-scoped optional session report refs; Valgrind errors have source-site candidates only and no primary-run ID. Session reports are **not checkpoint-causal joins**. An absent crash/Valgrind report is not a negative diagnosis. |
| `first_location_findings()` | Only report findings whose **first recorded location** names this snapshot or nested record. Validation findings aggregate occurrences by code, so absence here is *not* a local completeness certificate; consult the full TP-6 report and qualified evidence. |

## Bound, identity and confidence rules

All TP-7 page methods use half-open vector/ordinal `Range<usize>`, with **`MAX_CONSUMER_PAGE = 128` per request**. Empty pages are valid; reversed or beyond-end ranges return `QueryError::InvalidRange`; width >128 returns the additive `QueryError::LimitExceeded { collection, requested, max }` rather than truncating. Borrowed iterators return original records in canonical order with their original positions; they do not assemble an unbounded result or copy records. Callers can request consecutive bounded pages explicitly. AST source spans are capped at 4096 expressions by the engine and scanned on demand for exact file/line matches (O(AST count) per request); no eager line-expanded index is built. Stop-tagged instructions and gap windows use TP-2's verified ordering and binary search; unordered imported evidence errors rather than guessing. All page item refs use the existing weak-identity `EvidenceRef`: the same store's clones resolve them, an independently loaded trace returns `ForeignTrace`. Static AST expression IDs are trace-local strings, **not** transferable references or execution IDs.

TP-7 uses TP-6's `EvidenceConfidence::{Definite, Candidate, Ambiguous, Unavailable, Contradictory}` unchanged. Definite **recorded tag, value, or captured bytes** never implies execution of a source expression, pointer origin, valid initialization, or complete program/fork coverage. Numeric pointer containment only provides a candidate; same-address reuse stays ambiguous; freed/incomplete history is not silently repaired. A complete *observed-PID* heap history cannot certify silent children; child logs missing ACK remain partial. Valgrind source-site candidates belong to an **independent** run, never to a primary-run allocation ID. When deep validation finds structural corruption, TP-6 conservatively downgrades composed confidence to `Contradictory`. `observation_complete`, memory readability, scope liveness, execution status and process coverage are separate fields—not one success boolean.

## Tests and compatibility

Permanent `tests/trace_parser_consumer_tp7.rs` tests checkpoint identity/ordering and same-store/foreign-store citations; recursive caller activation, same-name shadowed locals, globals and captured bytes; explicit variable/event/Valgrind/heap paging (including 310 locals, 300 instruction events and 130 untracked allocation-failure events); over-limit/reversed/beyond-end errors, untracked ID 0 and partial history; missing stops and position-window gaps; malformed imported pointer provenance and first-location findings; optional AST on/off and ambiguous heap-call candidates; crash report versus independent Valgrind diagnostics; observed child versus silent child versus missing-ACK partial history; legacy V2.1 and absent optional source/process metadata. Live GCC/GDB cases use bounded existing C fixtures; Clang-positive source assertions run when available. The TP-6 real/synthetic evidence tests, TP-5 adversarial scope/provenance regressions and existing TP-1/2/3 V2.1 examples remain active. Four older pre-V2.1 `examples/v2_*.json` with removed limit fields still fail ingestion, by the previously documented contract; compatibility was not expanded.

The tests assert exact pointer identity to canonical records, `EvidenceRef` origin and vector position, stop/attempted/event domain separation, confidence, completeness and ordering—not just collection sizes. Focused TP-7 log: `/tmp/cte-tp7-focus-final.log` (**10/10**). No test-only processing was added to production Trace Engine code.

## Release-profile measurements

The existing TP-3/TP-6 release benchmark was extended with TP-7 queries and a **separate synthetic 1024-variable checkpoint** for large-page behavior. Three sequential runs after the last TP-7 API edit: `/tmp/cte-tp7-bench-last-{0,1,2}.log`. The real samples are TP-6's same 81,473-byte AST-on trace (7 snapshots/22 events), 424,971-byte reuse trace (44/125), and 22,059,776-byte source-large trace (1000 snapshots/**0 retained instruction events**). Times are medians of run medians; queries were warmed. `tp7_summary_live_delta=0 bytes allocs=0` on all three real samples (measured allocator delta at summary creation); every listed serial query reported **0 allocations**. `rss_after_index` includes process runtime and allocator effects, not isolated index memory; no assembled-trace RAM bound is claimed.

| Real input | TP-6 → TP-7 TP-2 index build µs | TP-7 RSS after index KiB | validation once µs | summary / local page≤16 / checkpoint event page / static source page median ns |
|---|---:|---:|---:|---|
| AST-on | 147.70 → 116.12 | 3,736 | 77.45 | 55.0 / 825.6 / 193.3 / 627.9 |
| heap reuse | 155.16 → 140.82 | 5,128 | 86.38 | 55.6 / 2,341.0 / 220.4 / n/a |
| source-large | 44.50 → 43.21 | 65,364 | 257.63 | 56.9 / 615.1 / 29.5 / n/a |

The composition layer does not alter index construction; the apparent index-speed differences are run-to-run noise. Page cost includes **all yielded items**, not a first-row lookup. On the synthetic 1024-variable snapshot, a full **128-variable page** took median **22.13 µs**, **eight bounded pages processing 1024 variables** took **182.76 µs**, and explicitly rejecting an over-limit page took **14.9 ns** (0 query allocations). Synthetic benchmark RSS printed in the logs includes memory retained by earlier real-sample loads and must not be attributed to the synthetic page alone. These timings are workload-specific, not Stage 2 throughput guarantees; the source-large fixture has no retained instructions and is not an instruction-index stress test.

## Verification and TP-8 handoff

After the final code edits, all required commands passed: `cargo check --all-targets`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and `cargo test --no-fail-fast -- --test-threads=1`: **378 passed, 0 failed, 0 ignored across 21 targets** (including one doctest). Logs: `/tmp/cte-tp7-{check,fmt,clippy,release}-final.log`, `/tmp/cte-tp7-tests-final.log`; `git diff --check` passed. The first strict-Clippy attempt found four audit-test style warnings (reversed literal range, redundant `matches!`, boolean comparison and length check); each was corrected without suppressing assertions. The final serial suite had no failures. Stage 2 may use `checkpoint_summary`, bounded pages, and the underlying TP-6 view/report without rejoining raw records or re-validating per query. TP-8 should independently stress bound semantics, repeated multireader/RSS behavior, large instruction/event datasets, malicious imports and release compatibility without expanding these evidence guarantees. There is still no untrusted-code sandbox, all-descendant-process census, exact AST-expression execution proof, cross-run Valgrind allocation identity, full-trace RAM guarantee from JSON export limits, or semantic C-language interpretation.

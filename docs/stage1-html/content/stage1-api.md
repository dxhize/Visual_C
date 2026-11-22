# 63 · Stage 1 public API reference

> **In one sentence:** This is a compact map of what Stage 1 gives to other code. It distinguishes loading the recording from checking its claims.

## Key terms

| Term | Plain meaning |
|---|---|
| **TraceStore** | The owner of one loaded trace and its lookup indexes. |
| **Validated view** | A read-only interface made after the deeper consistency check, with confidence attached to specific questions. |

> **Example.** A consumer can follow an evidence reference to a variable at stop 4, inspect its captured bytes, and still report the variable's pointer origin as unknown.
>
> **What not to assume:** Do not treat `TraceStore::from_trace` or successful JSON import as a deep semantic validation.

## The big picture

This is the downstream boundary for a recorded Stage 1 trace. It is not a Stage 2 semantic engine. The API returns borrowed evidence, explicit confidence and bounded pages; it does not infer C objects, pointer origin, initialization or process-tree completeness.

## Imports and ownership

```rust
use c_trace_engine::trace_parser::{
    EvidenceConfidence, EvidenceRef, QueryError, TraceLoadOptions, TraceStore,
};
```

`TraceStore::open(path, options)` returns an owned handle with `Arc<DebugSessionTrace>` and `Arc<TraceIndexes>` (`src/trace_parser/mod.rs:240-258`). Cloning the store is cheap and shares the same trace identity. Loading the same JSON again creates an independent owner; its references are foreign.

## Load and validate

```rust
let store = TraceStore::open("trace.json", TraceLoadOptions::default())?;
let validated = store.validated_semantic();
let report = store.validate_deep();

assert!(report.is_structurally_sound());
```

`validated_semantic()` runs `validate_deep()` once and retains the report (`src/trace_parser/semantic.rs:38`). Queries do not rerun validation. `is_structurally_sound()` is separate from `is_partial()`: a partial trace can be structurally sound, and an optional field can be absent without corruption.

## Checkpoint summary

```rust
let summary = validated.checkpoint_summary(index)?;
let stop = summary.stop_id();
let source = summary.source_location();
let locals = summary.variables_page(
    c_trace_engine::trace_parser::VariableScope::Local,
    0..16,
)?;
```

`checkpoint_summary(index)` resolves a snapshot and creates a trace-scoped snapshot `EvidenceRef` (`src/trace_parser/consumer.rs:58`). The summary borrows the validated view and the canonical store; it does not clone the snapshot. `stop_id()` converts the model's zero sentinel to `None`.

## Variables, frames and pages

`VariableScope` is `Local`, `Global` or `Caller(position)` (`src/trace_parser/consumer.rs:41-46`). `variables_page` returns an exact-size iterator of `QueryResult<VariableEvidence>`; `frame_activation(id)` resolves a unique activation at that checkpoint, not a cross-stop lifetime (`:251`). `instruction_events_page(range)` pages events explicitly tagged with the checkpoint ID (`:319`). `heap_interval_page(range)` pages heap events between retained snapshot counters and is a candidate temporal association (`:349`). `source_expressions_page(range)` pages AST source-span candidates and never proves execution (`:422`).

All pages use `MAX_CONSUMER_PAGE = 128` and a half-open range. A page wider than 128 returns `QueryError::LimitExceeded`; an invalid or reversed range returns `InvalidRange`. There is no silent truncation.

## Heap and diagnostics

`validated.heap(allocation_id)` joins lifetime, final block, history and realloc confidence (`src/trace_parser/semantic.rs:225`). `validated.heap_reference(&evidence_ref)` resolves a trace-scoped allocation reference first (`:205`). `validated.valgrind_error(index)` returns an independent Valgrind error; `primary_allocation_id` is always `None` (`:357`). `validated.diagnostics()` borrows compiler/crash/sanitizer/Valgrind reports (`:345`). `validated.process_coverage()` exposes the recorded observed-PID scope and leaves unknown descendants unavailable (`:378`).

## Confidence handling

`EvidenceConfidence` is `Definite`, `Candidate`, `Ambiguous`, `Unavailable` or `Contradictory` (`src/trace_parser/semantic.rs:20`). It belongs to a relationship. A variable value can be definite while its pointer origin is unavailable. A heap identity can be definite while its realloc link is ambiguous. Any structural corruption makes `qualify` return `Contradictory` for all relationships (`:61-67`).

Do not merge confidence with execution status, memory readability, initialization, scope activity, heap coverage or process-tree completeness.

## Evidence references and errors

An `EvidenceRef` stores a `Weak<DebugSessionTrace>` and an `EvidenceLocation` (`src/trace_parser/evidence.rs:68-75`). `resolve_evidence` checks owner identity before resolving the position (`:140-145`). A reference from an independent load is `QueryError::ForeignTrace`; an invalid position is `InvalidReference`; missing optional evidence is `Unavailable`. Raw records remain accessible through `store.raw()` even when a semantic relationship is unavailable.

## Deliberate limits

Stage 1 leaves C-language interpretation to downstream code. It does not execute source, evaluate DWARF location expressions, infer an active union member, turn a source candidate into an executed expression, turn an address match into allocation origin, or turn observed-PID completeness into a descendant census. Those are not missing convenience features; they are evidence-contract boundaries.

## Diagram

{{diagram:d3-pipeline}}

## Related reading

[Parser ingestion](parser-ingestion-deep.html), [ownership and evidence references](parser-ownership-deep.html), [TP-6](parser-semantic-deep.html), [TP-7](parser-consumer-deep.html), [limitations](limitations.html).

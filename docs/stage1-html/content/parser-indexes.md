# TP-2 · Compact, position-only indexes

## Level 1 · Intuition

A large trace repeats instruction addresses and source lines thousands of times. Scanning every event for each question would waste time; copying every record into lookup tables would waste memory. TP-2 builds **keys → integer positions** once and borrows original records when queried. It does not create an address→allocation provenance index.

{{diagram:ownership}}

## Level 2 · What is indexed

`TraceIndexes::build` eagerly constructs the index during `TraceStore::assemble`. `ExecutionsIndex` groups all retained event positions for each disassembly table ID using contiguous offset arrays. `SourceIndex` interns complete file paths and exact `(file,line)` keys, mapping to static instruction IDs and **all** dynamic event positions; the reverse mapping retrieves source positions for a disassembled instruction. `AllocationIndex` maps allocation ID to event positions, final state position and lifetime position. `MetadataIndex` maps trace-local DWARF/AST IDs and source/heap-call candidate IDs to registry positions. Only positions/keys are retained; no large memory bytes, snapshots or events are cloned into indexes.

Direct vector access needs no index for snapshot index or `event_id`. GDB stop IDs and checkpoint event runs use verified monotonic ordering and binary searches, not a stop hash map. Heap-event intervals use neighboring **retained** snapshots' cumulative `heap_event_count` boundaries; skipped stops make timing coarser, not an exact join. Imported broken order yields `OrderingViolation` rather than an unreliable binary-search result. No source-basename equivalence and no address-to-lifetime join are cached.

### Mini example

Disassembly instruction 12 may occur at `instruction_events` positions `[5, 21, 36]`. The index contains three `u32` positions, not three copies of the event or one merged “executed once” record. `instruction_execution_positions(12)` returns that slice; `instruction_executions(12)` borrows canonical event records at each position. `instruction_id=12` is **not** event 12.

## Level 3 · Source and query contracts

- `src/trace_parser/indexes.rs::{TraceIndexes, ExecutionsIndex, SourceIndex, AllocationIndex, MetadataIndex, OrderingFacts}`.
- `TraceStore::{instruction_event, instruction_range, events_at_checkpoint, snapshot_at_stop, instruction_executions, executed_events_for_source, heap_events_for_allocation, allocation_lifetime}` in `src/trace_parser/mod.rs`.

```rust
let positions: &[u32] = store.heap_event_positions_for_allocation(allocation_id);
for &position in positions {
    let heap_event = &store.heap_events()[position as usize];
    assert_eq!(heap_event.allocation_id, allocation_id);
}
```

This excerpt assumes `store` and `allocation_id` are defined. It visits recorded heap-event **positions** without confusing them with `event_id`, stop IDs or allocation IDs; the executable [usage sample](usage.html#checkpoints-and-paging) also handles empty traces and paginates. `TraceIndexes::estimated_bytes()` is a rough logical estimate of index structures, **not process RSS**. In the Stage 1 audit the 900-snapshot/3,143-instruction trace had ~28,369 estimated index bytes, ~1.13 ms median index build and ~17 MiB parser-process RSS after indexing (process includes trace, runtime and buffers; workload-specific).

> NOTE: A valid empty source/heap lookup means “no matching **recorded** item”; it is not a statement that nothing happened in a gap, another process or an uninstrumented library.

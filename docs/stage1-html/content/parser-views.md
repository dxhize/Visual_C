# TP-3 · Borrowed views and EvidenceRef

## Level 1 · Intuition

One large immutable trace should have many lightweight readers, not many copies. A view is a borrowed window into canonical records; an `EvidenceRef` is a **trace-scoped citation** that can later be resolved against that same recording. Equal numeric indices in two independent traces do not refer to the same execution.

{{diagram:ownership}}

## Level 2 · Ownership and lifecycle

`TraceStore` owns `Arc<DebugSessionTrace>` and `Arc<TraceIndexes>`; cloning the store cheaply shares both. `raw()` exposes recorded evidence, while `semantic_input()` builds `CheckpointContext` from a snapshot index or a `source_checkpoints` position and offers borrowed stack, locals, caller frames, globals, live heap and metadata. A context's `events_at_checkpoint()` finds explicit checkpoint tags; `heap_events_in_snapshot_interval()` uses adjacent retained cumulative counters. A missing stop/source association returns `Unavailable` rather than creating a fake stop.

`EvidenceLocation` names a collection and position, e.g. `Snapshot(3)`, `SnapshotVariable { snapshot:3, variable:1 }`, `HeapEvent(7)`, `AllocationLifetime(2)`, `CrashReport`, or `ValgrindError(0)`. `EvidenceRef` also carries a weak `Arc` trace identity. `resolve_evidence` checks identity first: a cloned store resolves the reference; an independent reload of equal JSON rejects it as `ForeignTrace`. The reference is not a portable serialized ID or a cached ownership graph.

### Worked citation

A variable on the top frame of snapshot 3 may be observed with `snapshot.context.variable_evidence(1)?`. Resolving that reference returns the **same** canonical `VariableInfo` as `store.trace().snapshots[3].variables[1]`. A later stop with a local of the same name is **not** substituted. See [TP-6 variable confidence](parser-semantic.html#pointer-and-memory-confidence).

## Level 3 · APIs and errors

```rust
use c_trace_engine::trace_parser::{EvidenceLocation, TraceStore};
let store = TraceStore::open("examples/v21_dual.json", Default::default())?;
let cite = store.evidence_ref(EvidenceLocation::Snapshot(0))?;
let same_run = store.resolve_evidence(&cite)?;   // borrows original record
let _ = (cite.location(), same_run);
```

The snippet is a **body excerpt** (wrap it in `fn main() -> Result<(), Box<dyn std::error::Error>>`); see [Practical usage](usage.html#level-3-load-and-validate) for a standalone example. Implementation: `src/trace_parser/{views,evidence}.rs`; `TraceStore::raw`, `semantic_input`, `evidence_ref`, `resolve_evidence`. A `QueryError` distinguishes `OutOfRange`, `InvalidRange`, `LimitExceeded`, `InvalidReference`, `ForeignTrace`, `Unavailable` and `OrderingViolation`. No caller should turn an error into “zero evidence.”

> NOTE: A borrowed `CheckpointContext` cannot outlive its store. Keeping a separate `Arc<DebugSessionTrace>` alive is possible with `shared()`, but a different `TraceStore::open` still has a different trace identity even with byte-identical contents.

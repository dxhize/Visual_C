# 43 · TP-3 · Borrowed views and EvidenceRef

> **In one sentence:** A view is a convenient way to look at the trace you already loaded. It does not own a second independent history.

## Key terms

| Term | Plain meaning |
|---|---|
| **Arc** | Rust's shared-ownership handle: several readers can refer to the same underlying saved trace without copying all its records. |
| **EvidenceRef** | A citation to one original record, tied to the trace it came from. |

> **Example.** Two readers cloned from one `TraceStore` can resolve the same citation. If you independently load the same JSON again, its references belong to a different trace owner and are rejected as foreign.
>
> **What not to assume:** A borrowed view cannot recreate a value the engine never captured.

## Level 1 · Intuition

One large immutable trace should have many lightweight readers, not many copies. A view is a borrowed window into canonical records; an `EvidenceRef` is a **trace-scoped citation** that can later be resolved against that same recording. Equal numeric indices in two independent traces do not refer to the same execution.

{{diagram:ownership}}

## Level 2 · Ownership and lifecycle

`TraceStore` holds two shared Rust `Arc` values: the original `DebugSessionTrace` and the small `TraceIndexes`. You can clone the store without copying the trace. `raw()` offers recorded items directly. `semantic_input()` opens a **borrowed** `CheckpointContext` for a saved snapshot, so callers can inspect its current frame, optional callers, globals and observed heap without taking ownership of their bytes. `events_at_checkpoint()` uses recorded checkpoint tags, while a heap-event interval uses counts from neighboring *saved* snapshots. If the trace has no supported stop or source association, the query says `Unavailable`; it does not add a stop to make an answer look complete.

`EvidenceLocation` is like a precise address *inside the notebook*: “snapshot 3, variable 1,” or “heap event at array position 7.” `EvidenceRef` adds a weak link to **which loaded notebook** it came from. When you resolve the reference, the parser checks that owner first. A cloned `TraceStore` points to the same trace and accepts it; opening the same JSON independently creates a new owner and returns `ForeignTrace`. This prevents accidentally linking events from two separate loads just because their numbers happen to match. An `EvidenceRef` is not a portable ID you can serialize and use against another trace.

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

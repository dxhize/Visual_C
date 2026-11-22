# 18 · Allocation events, lifetimes and realloc

> **In one sentence:** Think of allocations as *lifetimes*, not addresses. A freed address can be handed out again to a brand-new block.

## Key terms

| Term | Meaning |
|---|---|
| **Wrapper** | A small function inserted before a linked call to `malloc`, `calloc`, `realloc` or `free`. It logs the call and passes it on to the real allocator. |
| **Lifetime ID** | A number given to a tracked allocation's period of existence. Two blocks can share an address without sharing this ID. |

## A lifetime, step by step

```
malloc(…)      →  ID 1 created at address A
free(A)        →  ID 1 ends
malloc(…)      →  ID 2 created at address A again   ← same address, different lifetime
```

A successful in-place `realloc` also starts a **new** lifetime ID in this engine, even though it keeps address A.

> **Limit:** The logger cannot promise to see allocators that run entirely inside unwrapped shared libraries.

## Level 1 · Intuition

The same numeric address can host many unrelated allocations over time. Stage 1:

- gives each intercepted allocation a lifetime ID,
- records the events that affect it, and
- keeps snapshots **immutable** (later events never rewrite earlier captures).

A current pointer's address can fall inside a block's range without proving which lifetime produced the pointer. `realloc` can retire one lifetime and create a **linked** successor. A *failed* `realloc` must not retire the original.

{{diagram:heap}}

## Level 2 · From logged events to reconstructed lifetimes

### On the C side (`c_runtime/alloc_tracker.c`)

1. `--wrap=malloc` redirects linked `malloc` calls through a wrapper. The wrapper calls the real `malloc` **once** and returns its actual result. Other wrappers handle `calloc`, `realloc` and `free`.
2. A **fixed-size C table** remembers each tracked address, allocation ID and size.
3. Each event is written to a dedicated file with its **PID** and an increasing number *for that process*.
4. A **guard** stops the logger from recursively logging its own allocator work.

### On the Rust side

1. At each debugger pause, `EventTailer` reads the *complete* new lines.
2. `HeapTimeline::ingest` builds block lifetimes from them, **marking missing, duplicated or out-of-order lines** instead of assuming a perfect history.
3. The engine reads the event file **before** taking the stopped-state snapshot.
4. It may look up a recorded caller instruction address in the live process's maps, but an unresolved source location stays unresolved.

### How `realloc` is handled

| Case | What the recorder does |
|---|---|
| Successful, block **moves** | New, **linked** lifetime ID |
| Successful, block **stays** at the same address | New, **linked** lifetime ID (same treatment) |
| **Failed** `realloc` | The old block stays alive |
| `realloc(ptr, 0)` in the observed glibc environment | May free the old block and return NULL. The logged terminal event uses the **old ID**, with no new address or size, so it is **not** a link to a new block |

The parser checks recorded events and lifetime boundaries before it says two IDs are connected. Equal addresses alone can't prove that. See [realloc validation](parser-validation.html#realloc-link-verification).

### What each heap record means

| Evidence | Meaning | Caution |
|---|---|---|
| `HeapEvent` | Wrapper event in ingestion order: PID, sequence, IDs, kind, optional address | ID `0` is untracked; internal library allocators may be invisible |
| `HeapBlockInfo` | Final or historical state, with optional realloc predecessor | Freed block bytes are historical; check `observed_at_stop` |
| `AllocationLifetime` | PID, allocation sequence, optional free sequence | Complete *observed-PID* history is not a fork census |
| `HeapHistoryCoverage` | Gaps, transport, ACK, caps, process scope | `complete=true` has a specific observed-PID scope |

## Level 3 · C/Rust boundary and example

```c
// Source: fixtures/c/realloc_move.c (excerpt)
char *p = malloc(32);
char *guard = malloc(32);
char *next = realloc(p, 1024 * 1024);
if (!next) { free(p); free(guard); return 1; }
free(next);
```

For a successful move, the raw event carries `old_allocation_id`, a new `allocation_id` and the recorded PID/sequence. The old lifetime ends at that event's sequence.

In the final parser, `heap(new_id).realloc_confidence` is `Definite` **only when all four hold**:

1. a matching creation event exists,
2. the predecessor's creation is recorded,
3. same-PID lifetime boundaries are compatible, and
4. the observed history is complete and verified.

Partial or legacy evidence stays *candidate* or *unavailable*. `src/trace_parser/validation.rs::realloc_link` performs the shared relationship checks, and structural contradictions and cycles can never become definite.

| Layer | Where to look |
|---|---|
| **C** | `__wrap_malloc`, `__wrap_calloc`, `__wrap_realloc`, `__wrap_free`, `cte_emit_locked`, `cte_after_fork_child` in `c_runtime/alloc_tracker.c` |
| **Rust** | `src/allocation_tracker.rs::{parse_event_line, EventTailer, HeapTimeline::ingest, verify_event_status}`; `src/trace.rs` backfills snapshots and builds final state |
| **Parser** | `TraceStore::heap_events_for_allocation(id)` uses a position index; `ValidatedSemanticView::heap(id)` qualifies identity and realloc **separately**. See [schema fields](schema.html#heap-records) |

> **Warning:** The wrapper does **not** intercept allocator calls made entirely inside unwrapped shared libraries. The transport's terminal ACK verifies only observed PIDs plus the followed inferior. No absolute heap completeness or descendant coverage is promised.

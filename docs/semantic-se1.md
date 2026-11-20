# Stage 2 / SE-1 — Trace-scoped Runtime Object Registry

**Scope:** SE-1 only. `src/semantic/mod.rs` is a read-only, separately named Stage 2 module. No Stage 1 schema-v2, recorder, parser, TP-6 confidence, validation, or Stage 1 audit contract was changed. This is not an object-value simulator, pointer graph, alias oracle, byte-copying layer, or SE-2.

## Construction and identity

API excerpt (the complete executable example is `examples/semantic_se1.rs`):

```rust
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};
use c_trace_engine::semantic::ObjectRegistry;
let store = TraceStore::open("trace.json", TraceLoadOptions::default())?;
let validated = store.validated_semantic();
let objects = ObjectRegistry::build(&validated)?; // fails on structurally unsound imports
for object in objects.objects_at_checkpoint(0)? {
    println!("{:?} {:?} {:?}", object.kind, object.name, object.id.evidence().location());
    for observation in objects.observations(&object.id)? {
        // Borrow canonical evidence. No independent allocation, byte decode or initialization claim.
        let _ = observation.variable()?;
    }
}
```

`ObjectRegistry<'a, 'v>` borrows a `ValidatedSemanticTrace<'a>` and its canonical `TraceStore`; the store owns its `Arc<DebugSessionTrace>`. Keep the store and validated view alive for the registry's lifetime. `ObjectId` is its **first evidence reference**, scoped to the store: using an ID or frame/event reference from a separately loaded copy is an error (`ForeignTrace`), even when the JSON is byte-for-byte equal. An ID is not an address, source name, source declaration, GDB value, or JSON position exported to another trace. Two IDs at different positions may have the same address or symbol spelling.

The build checks `ValidationReport::is_structurally_sound()`; deserialization and `TraceStore::from_trace` are **not** validation. For each stack local, a stable key needs PID, native activation ID, exact qualified runtime identity, and corroborating observed frame activation (`call_stack`/`caller_frames`). Same-name recursion, caller-frame shadowing, repeated invocations, and stack address reuse remain separate. If any component is absent or is an address-derived fallback identity, the variable is a **single-checkpoint object with `Unavailable` identity confidence**, not an inferred cross-stop match. A repeated qualified key within one checkpoint is ambiguous, so neither observation merges with another checkpoint. GDB lifetime and variable read/capture status do not establish a new identity.

Globals and function statics use qualified native `symbol:`/`static:block:` identities plus the observed PID and storage kind. Statics do **not** follow automatic stack activation. Unqualified globals, same-named translation-unit statics, ambiguous symbols, and missing identities are not merged; same-name source declarations/AST candidates alone never identify a runtime object. This policy does not prove an imported native identity truthful beyond Stage 1's verified evidence contract. A diagnostic or independent run is not a primary-run object.

Heap identity is the Stage 1 allocation **ID**, not an address. SE-1 anchors an ID to a canonical lifetime/heap-state/event/retained-block reference, indexes all borrowed events of that ID, and separately indexes retained checkpoint blocks. Failed fresh allocations with no old ID do not create objects. Free events and the terminal-free `realloc(ptr, 0)` on an existing ID end the recorded lifetime without creating another object. Moved and same-address reallocs use **different IDs**; `realloc_predecessor` returns a relation *only if TP-6 has a non-contradictory, corroborated relation and an observed predecessor object*. Its returned confidence is **exactly TP-6's** `Definite`/`Candidate`; no address-equality promotion. TP8-F01 forged/cyclic/contradictory predecessors reject construction as structurally unsound. A missing/partial/legacy process scope downgrades heap identity and predecessor confidence; it does not erase events or assert missing descendants are complete.

## State and queries

- `objects()`, `object(&ObjectId)`, `objects_at_checkpoint(index)`, `observations(&ObjectId)`, `observation_at(&ObjectId, index)`, and `observations_at_checkpoint(index)` enumerate objects and **actual** recorded observations. Checkpoint indices are bounded; no implicit cap or silent truncation. Each observation stores a position and a visibility/presence fact, not a variable/heap-block copy.
- `stack_objects(&EvidenceRef)` accepts an actual current or caller frame; `heap_object(&EvidenceRef)` accepts an event/lifetime/heap block and checks trace ownership. `heap_by_id(u64)`, `heap_events(&ObjectId)` and `heap_lifetime(&ObjectId)` give indexed, borrowed heap evidence. For other location types the specialized queries return an explicit `Unavailable` error.
- `CheckpointPresence::Observed { count }` counts retained records; `observation_at` exposes each one's visibility and snapshot-completeness. `NotObserved { snapshot_complete }` says **only** that the checkpoint has no record for this object. It does not infer birth, death, a frame's execution, or a missed child. In-scope, out-of-scope, unknown-scope, recorded heap active, and recorded heap freed are deliberately distinct. Snapshot completeness is qualified by memory truncation.
- `HeapLifetime` retains the recorded predecessor ID and native allocation/free sequences and PID; `HeapLifetimeStatus::RecordedFreed` means a recorded free/terminal-free, not that all process history was traced. `RecordedActiveAtEnd` is restricted to a definite, complete, verified observed history. Otherwise lifetime status is `Unavailable`. Pointer `target` fields and Valgrind findings are **not** queried for heap identity.
- `ObservationView::variable()` returns TP-6's qualified variable view; `heap_block()` resolves the *original* block. Recorded raw byte prefixes remain valid prefixes only: neither bytes, default C calloc behavior, nor a successful read proves all bytes initialized. Historical stopped-state observations are immutable after a later free or address reuse.

Construction is O(retained variable records + retained heap blocks + heap events + lifetime records), with per-checkpoint temporary hash deduplication and compact `u32` position vectors (checked conversions). Object/checkpoint and object/observation accesses use indexes; an `observation_at` range uses binary search; heap-ID lookup is hashed. Enumerating all members of a checkpoint or object remains proportional to its result size. The registry does **not** create another trace, raw-byte cache, pointer graph, or unbounded per-query result allocation. `estimated_bytes()` counts its vectors and an approximate heap-ID payload, **excluding** allocator bucket metadata, Stage 1 indexes, and the shared trace; it is not a memory guarantee. Stage 1's JSON-export cap remains a per-document limit, not an assembled-RAM bound.

## Permanent checks

`tests/semantic_registry_se1.rs` and `tests/semantic_registry_se1_edges.rs` contain **14** tests. Synthetic tests exercise recursive activations, same-address locals, caller-frame shadowing, repeated native keys, distinct translation-unit statics and persistent function statics, source-declaration/address fallback, absent activation, unreadable/partial capture, no-record and trace-gap checkpoints, foreign trace refs, legacy V2.1 import, partial heap scope, event borrowing, duplicate DWARF type IDs, forged TP8-F01 realloc links, address-only pointer imports, and false descendant coverage. Corrupt evidence either fails the Stage 1 validation gate or remains non-definitive; tests never treat an independent execution as primary-run evidence.

The live integration test compiles and traces seven C fixtures through the **real Engine → TP-1 store → TP-6 validation → SE-1 registry**, with native GDB, observed fork PIDs, recursion/caller frames, malloc/calloc/free, same-address realloc, moved realloc, terminal-free realloc, failed allocations, and address reuse. Tests skip this portion explicitly if `gcc` or `gdb` is unavailable; on the recorded machine both were available and all seven ran. Fixture oracles compare actual allocation IDs, native event sequences and source snapshot positions rather than assuming address matches prove allocation identity. The fork oracle confirms that two *observed* event PIDs never imply unseen descendants were traced.

To run only the SE-1 checks:

```sh
cargo test --test semantic_registry_se1 --test semantic_registry_se1_edges -- --test-threads=1
```

## Reproducible benchmark / sample

```sh
cargo build --release --example semantic_se1
# One completed JSON trace per process, for meaningful RSS comparisons:
target/release/examples/semantic_se1 /path/to/trace.json
```

The example demonstrates public lookup and lifetime semantics and reports parse/index and validation separately from the SE-1 build. Linux `/proc/self/status` supplies RSS immediately before/after the build and whole-process peak (`VmHWM`); `System` allocator accounting reports the **net live requested allocation bytes added by SE-1**. `registry_est_bytes` is the module's approximate payload estimate. The query allocation counter surrounds 1,000 warmed repeats of checkpoint/object-range lookup and optional heap-ID lookup; timing medians use 500 warmed calls each. No query result is stored or cloned. The timed checkpoint count is O(objects at that checkpoint), not a constant-time access. Measurements are not memory caps or universal performance guarantees.

On this machine (Linux, release build, three independent example processes per input, medians of the three), the inputs were completed native schema-v2 traces generated earlier with Stage 1: `/tmp/cte-stage1/{defined,recursion-callers,reuse,zero,heavy,fork-observed}.json`. The large `heavy` input has **900 snapshots / 3,143 retained instruction events**; the SE-1 registry has 1,798 observations. All six benchmark input processes exited successfully.

| Trace | Objects / observations | SE-1 build µs | Added live bytes | RSS before → after KiB | Peak KiB | Checkpoint / observation / heap lookup ns | Warm query allocs |
|---|---:|---:|---:|---:|---:|---:|---:|
| Defined, same-address realloc | 19 / 235 | 182 | 26,388 | 6,340 → 6,340 | 6,764 | 47 / 139 / 81 | 0 |
| Recursive callers | 9 / 119 | 118 | 13,472 | 4,876 → 4,876 | 5,016 | 41 / 139 / n/a | 0 |
| Address reuse | 17 / 288 | 260 | 39,860 | 5,888 → 5,888 | 6,176 | 49 / 139 / 82 | 0 |
| Terminal-free/failed allocations | 26 / 599 | 492 | 69,016 | 8,164 → 8,164 | 8,940 | 57 / 154 / 83 | 0 |
| **900-snapshot heavy** | 2 / 1,798 | 1,716 | 179,520 | 27,220 → 27,220 | 30,344 | 42 / 233 / n/a | 0 |
| Observed fork | 10 / 59 | 72 | 9,652 | 4,628 → 4,628 | 4,696 | 42 / 127 / 82 | 0 |

RSS granularity/allocator reuse masks small deltas: identical before/after RSS does **not** mean zero incremental allocations. Peak includes parse/index, validation, building, and query setup, and is not an isolated registry peak. `live_delta_bytes` *is* the added net requested size measured around construction, not malloc's actual bucket overhead or OS resident memory. Build time excludes loading/validating Stage 1 and compiler/GDB tracing. The heavy trace's larger cost scales with its 1,798 observations, while no bytes from the underlying 3,143-event instruction stream are copied into the registry.

## Final verification

After the last code and test edits, the following commands all passed: `cargo check --all-targets`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and `cargo test -- --test-threads=1`. The full serial suite had **397 passed, 0 failed, 0 ignored across 24 targets** (including the documentation test). The focused SE-1 integration targets passed **14/14**; the live seven-fixture oracle ran with native `gcc` and `gdb`. Verification logs: `/tmp/cte-se1-verify/{check,fmt,clippy,release,tests}.log`; three-run benchmark samples: `/tmp/cte-se1-final-bench-<trace>-<run>.log`.

**Decision:** SE-1 is ready as a bounded, qualified, trace-scoped identity/presence/lifetime index. It does not expand the Stage 1 approval to complete process-tree capture, initialized-memory proof, speculative aliasing, or SE-2 semantics.

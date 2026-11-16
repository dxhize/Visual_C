# V2.1 Register Dependency Audit

This document records every CPU-register code path that existed in V2, the
evidence for why each was kept or removed, and how instruction addresses are
obtained after the change. It is the audit required before modifying code
(task section 3), written up after the fact against the implemented state.

## 1. What existed in V2

### 1.1 Engine-issued register queries (the expensive part)

All of these lived in `src/recorder.rs` (`Recorder::registers`, ~120 lines,
deleted in V2.1):

| Query | Frequency | Purpose in V2 |
|---|---|---|
| `-data-list-register-values x` | once per instruction event, plus once at startup | full register snapshot → delta + periodic checkpoints |
| `-data-list-register-names` | once per session (lazily) | map register numbers to names |
| `-interpreter-exec console "show architecture"` | once per session | label `RegisterObservation.architecture` |

Supporting machinery (all removed):

- Per-session caches `GdbSession.register_names` / `GdbSession.architecture`
  (`src/gdb_mi.rs`, fields removed).
- Delta base tracking: `Recorder.registers` map + `need_full` flag; gaps
  invalidated delta bases (removed).
- Periodic full checkpoints every `register_checkpoint_interval` (default 256)
  instruction events (removed).
- `max_register_snapshot_bytes` budget guard producing
  `error: "max_register_snapshot_bytes"` observations (removed from
  `ExecutionLimits`).
- `Recorder::initial_registers()` + the startup capture call site
  (`src/trace.rs`, pre-`main` stop; removed).
- Per-event capture call site in the stepping loop (`src/trace.rs`); the
  instruction-event literal now carries `registers: None`.
- Reconstruction: `processor::replay_registers` (BTreeMap walk over deltas and
  full checkpoints; removed). A repo-wide grep found **zero external
  consumers** — only its own unit tests used it.
- `config.tracing.registers` / `register_checkpoint_interval` /
  `config.profiling` knobs and the `--no-registers`, `--profile`,
  `--max-register-snapshot-bytes` CLI flags (removed or deprecated).

### 1.2 Where register values were used

Every consumer was checked before removal (task section 3):

| Consumer | Needs engine register queries? | Evidence |
|---|---|---|
| Current instruction address (`event.address`) | **No** — taken from the GDB stop frame (`stop.frame.address`) of the stop *before* the attempt | `src/trace.rs`, event construction |
| Next instruction address (`event.next_address`) | **No** — taken from the *next* stop's frame address | `src/trace.rs` (was line ~1535) |
| Faulting instruction identification | **No** — the faulted stop's frame address is the faulting PC | crash path unchanged |
| Instruction transitions / event ordering | **No** — derived from stop sequence | stepping loop unchanged |
| Source↔assembly correlation | **No** — disassembly cache keyed by address | `Recorder::disassemble` unchanged |
| Stack frames / variable reads | **No** — GDB resolves variables through frame context using its own register state | unchanged |
| Pointer target classification, heap/globals | **No** — expression + memory reads only | unchanged |
| `replay_registers` (GUI register replay) | It *was* the only consumer; the feature is dropped from the GUI plan | zero external callers found |
| Special stop conditions (signals, exits) | **No** — outcome/signal come from stop records | unchanged |

### 1.3 The three categories from the task

1. **GDB internally using CPU registers** — inherent to debugging; untouched.
   GDB still maintains register state to step, read frames, and evaluate
   variables. We never need to *query* it for that.
2. **Our engine querying register values** — this is what was removed.
3. **Storing register values in the trace** — removed from V2.1 output;
   the deserialization types remain for old traces (§3).
4. **GUI display** — no longer part of the product plan (decision in the
   V2.1 brief).

## 2. What remains (and why it is not register collection)

- `RegisterObservation` / `RegisterValue` structs in `src/models.rs`, now
  `#[serde(skip_serializing_if = "Option::is_none")]` behind their optional
  slots. They exist **solely** so a V2.1 engine (or future GUI) can
  deserialize a V2 trace without failing. V2.1 output never populates them.
- `Profiling.register_time_us` stays in the schema, documented as always `0`
  since V2.1, so historical V2 benchmark metrics stay comparable and the
  counter is never silently reused for unrelated debugger work.
- Per-event `memory` observations (`selected_memory`) are kept — they are
  cheap expression reads at the stop, not register polling, and they feed
  pointer/heap visualization.
- No PC queries were added: every address (current, next, faulting) is read
  from stop/frame data GDB already returns. No `-data-evaluate-expression
  $pc` or `-data-list-register-values` traffic exists in the V2.1 stepping
  loop.

## 3. How instruction addresses are obtained now

Unchanged from V2 — this is the key finding that made removal safe:

```
event.address      = stop.frame.address        (stop before the attempt)
event.next_address = next_stop.frame.address   (stop after the attempt)
event.outcome      = classifier(stop, next_stop)
```

`machine instructions do not execute sequentially` — the engine never
infers `next_address` from `address + len`; it always observes the real next
stop. That property is preserved and is now the *only* address source.

## 4. Negative evidence produced by the removal

- `tests/v2.rs::loop_events_are_dynamic_and_registers_are_not_collected`
  asserts `register_time_us == 0`, no `registers` key in serialized event
  JSON, and no `initial_registers` — proving the queries themselves are gone,
  not merely hidden (task section 11).
- The V2→V2.1 GDB command counts in `docs/v2.1/performance.md` drop by
  roughly one per instruction event (the register-values query), consistent
  with removing the query rather than discarding its result.

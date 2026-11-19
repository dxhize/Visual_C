# GDB/MI execution and checkpoints

## Level 1 · Intuition

A stopped program is observable; running code between stops is not automatically known. The GDB/MI2 driver sends commands, waits for the corresponding responses and asynchronous stops, and inspects bounded source state. At a stop the PC names the **next** instruction. The engine records an instruction attempt from that *before* address and a *next* address only when observed. Source checkpoints are selected stops, not a reconstruction of every intermediate state.

{{diagram:checkpoint}}

## Level 2 · Command/response lifecycle

A GDB session launches the debug binary, stops at entry and identifies a followed inferior. The engine classifies frames as user, runtime, library or unknown; collects stack/source context at checkpoints; drains allocator telemetry while the inferior is stopped; snapshots variables, bytes and live heap subject to budgets; and records that snapshot with a monotonic `stop_id`. In dual mode it attempts `-exec-step-instruction` and separately waits for `*stopped` (a tokened `^running` is only an acknowledgement). The `InstructionEvent` stores the before PC, attempted `event_id`, source context, thread/PID, `stop_id_before`/optional `stop_id_after`, outcome and optional next PC. An async missing-stop wait is classified differently from a whole-session deadline; a missing response poisons the command session rather than giving late output to a future command.

Outcomes include `completed`, `faulted`, `interrupted` and `unknown`. Breakpoints do not prove the attempted instruction completed. Library/source stepping introduces [TraceGap](model.html#events-gaps-and-checkpoints) records with attempted-position boundaries, not guessed counts. A user callback can be captured by bounded entry-breakpoint discovery; unobserved callbacks cannot be inferred from a static source call. GDB is in all-stop mode, but external processes are not paused and a multi-threaded selected-thread timeline is incomplete. A forked child is not promised GDB snapshots.

### Worked stop → parser query

At source line 15 of `fixtures/c/phase8b_defined.c`, the audited live capture recorded an integer `sum` with displayed value `"10"` and raw bytes `"0a000000"` (Linux little-endian), while native GDB in a *separate* run independently printed `sum=10`. A later `TraceStore` can retrieve the snapshot using its vector index, and `checkpoint_summary()` can page its variables. Neither the stop ID nor the value of `instruction_position` should be used as that vector index. See the [API walkthrough](usage.html#checkpoints-and-paging).

## Level 3 · Source and invariants

- `src/gdb_mi.rs` owns MI command tokens, async records, stop backlog, frame parsing, selected-thread tracking and memory reads. `src/mi_parser.rs` parses MI syntax.
- `src/trace.rs` orchestrates the stop loop and `push_snapshot`. `src/recorder.rs::{checkpoint, event, gap, event_outcome}` appends bounded canonical records and journal lines.
- `src/models.rs::{SnapshotObservation, InstructionEvent, TraceGap}` defines the serializable identities. `src/trace_parser/indexes.rs::OrderingFacts` enables binary search only when order is verified.

```json
{"step_index":0,"observation":{"stop_id":1,"instruction_position":0,"heap_event_count":0}}
```

The JSON above is **illustrative and deliberately partial**; a real `ExecutionSnapshot` has required file, function, call stack and variable fields. `stop_id=1` is a debugger stop counter, `step_index=0` a snapshot vector index, and `instruction_position=0` a count of attempts before this stop. See [schema identity domains](schema.html#identity-domains).

> WARNING: Recording a `checkpoint_id` on an instruction event associates it with the most recent checkpoint, **not** a claim that its instruction happened precisely at the checkpoint stop or that a source-line AST expression executed.

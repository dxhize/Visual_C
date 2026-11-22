# 21 · Timeouts, budgets, coverage and export

> **In one sentence:** The recording has several budgets, each with a different job. Before interpreting a shortened trace, ask *what* was limited.

## Key terms

| Term | Meaning |
|---|---|
| **Snapshot limit** | A bound on how many stopped-state snapshots can be kept. |
| **Journal vs export** | The JSONL **journal** grows during capture. The final **JSON** is a separate document made afterward, with its own byte limit. |

> **Example.** A capped journal can be 1,961 bytes while the final JSON is 22,883 bytes. That doesn't break the cap: the first cap applied only to the journal.
>
> Even a successful JSON export limit doesn't constrain the memory already used to assemble the trace.

## Level 1 · Intuition

A finite budget produces a useful *partial* recording rather than pretending missing data was observed. Different caps protect different things: journal bytes, JSON document size, debugger commands, snapshot reads, tool time and parser page size. They are **not one universal memory limit**.

A crash, timeout or cap is an **outcome**. It does not automatically corrupt the records already captured.

{{diagram:partial}}

## Level 2 · Budgets across stages

### Time

- The clock starts **before** the engine even checks which tools are available.
- Each compiler, debugger or analysis step must finish before the **whole session's deadline**.
- GDB commands also have their own command-count and time checks.
- "The program never stopped again" and "GDB never replied to my command" are different errors, reported differently.

### Cancellation and cleanup

- Ctrl-C or a normal termination signal asks the supervisor to cancel tools and save partial JSON when possible.
- An abrupt kill may leave an unfinished journal.
- The engine tries to manage child process groups and bound read pipes.
- OS limits such as `prlimit` apply to the program GDB stopped, and optional cgroups can help with descendants.

> These reduce resource risk. They do **not** make arbitrary C safe to run as a security sandbox.

### The limits

| Limit | Default | Governs (not equivalent to RAM) |
|---|---:|---|
| `timeout_seconds` | 10 s | Entire pipeline; per-stage limits clamp to it |
| `max_steps` | 1,000 | Source/dual progression; separate from instruction cap |
| `max_instruction_events` | 100,000 | Attempted instruction recording |
| `max_source_checkpoints` | 1,000 | Selected source-state records |
| `max_trace_bytes` | 128 MiB | JSONL journal byte accounting, not final JSON |
| `max_trace_memory_bytes` | 32 MiB | Resident serialized-record accounting, not total Rust RSS |
| `max_json_export_bytes` | 128 MiB | Each formatted final JSON document |
| `max_snapshot_memory_bytes` | 1 MiB | Budget of captured bytes at a checkpoint |
| `max_gdb_commands` | 1,000,000 | MI queries and control commands |
| `MAX_CONSUMER_PAGE` | 128 | Trace Parser TP-7 results in a single request |

### What happens when a limit is hit

- The recorder writes down **which scope** the limit covered and why it stopped or truncated data.
- It keeps explicit `trace_gaps` for stretches of instructions it can't show exactly.
- Heap events and optional source/type metadata can be partial for different reasons.
- `storage.finalized` only means the JSONL journal was **closed**. It does not mean the C program finished successfully.
- In `--journal-only` mode, the final trace may omit in-memory instruction arrays. The journal can be read in order, but it alone cannot recreate all the final reports needed for a full `TraceStore`.

### A measured example of why caps differ

The final audit set `--max-trace-bytes 2048`:

| Run | Result |
|---|---|
| Journal | **1,961 bytes** |
| Separately exported JSON | **22,883 bytes**, correctly marked partial |
| Second run with `--max-json-export-bytes 8192` | Exited with `max_json_export_bytes exceeded` and wrote **no** JSON |

Neither test establishes a peak process-RAM limit. See [parser ingestion limits](parser-ingestion.html#level-2-architecture-and-compatibility).

## Level 3 · Enforcement sites and results

| File | Role |
|---|---|
| `src/resource_limits.rs::{ExecutionLimits::validate, limit_stopped_inferior, CgroupGuard}` | Limit validation and enforcement |
| `src/supervisor.rs::{scope, stage_deadline, request_cancel}` | Deadlines and cancellation |
| `src/recorder.rs::{Recorder::truncate, Recorder::checkpoint, Recorder::event}` and `src/storage.rs::{Journal::append, Journal::finalize, recover}` | Distinguish retained records from journal contents and total attempts |
| `src/models.rs::RecordingData` | Holds the serialized result |
| `src/json_export.rs::{preflight, export_json_cancellable}` | Bounds each emitted JSON document **without** first assembling a giant output string. This does **not** bound the already-assembled `DebugSessionTrace` or parser RSS |

```sh
target/release/c-trace-engine fixtures/c/infinite_loop.c \
  --trace-mode dual --max-steps 60 --timeout 35 \
  --no-pretty --output /tmp/visual-c-limit.json
```

A capped trace can have `execution_status=step_limit_reached`, retained events, gaps and `coverage.complete_for_scope=false`. Consumers should read [validation findings](parser-validation.html) and [confidence](parser-semantic.html). They should not treat `compiled_successfully=true` as complete execution.

> **Warning:** The Trace Parser's input cap defaults to **512 MiB**. This prevents oversized input buffering but does not make deserialization or trace assembly constant-memory. The 128-record page bound constrains **query outputs**, not the whole trace.

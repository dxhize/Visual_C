# Timeouts, budgets, coverage and export

## Level 1 · Intuition

A finite tracing budget makes a useful *partial* recording rather than pretending missing data was observed. Different caps protect different things: journal bytes, JSON document size, debugger commands, snapshot reads, tool time and parser page size are not one universal memory limit. A crash, timeout or cap is an **outcome**; it does not automatically corrupt the records already captured.

{{diagram:partial}}

## Level 2 · Budgets across stages

`supervisor::scope` starts the global wall deadline **before** tool probes. Compiler, debugger and analysis deadlines are clamped to it. Every GDB command checks deadline and `max_gdb_commands`; an inferior stop-wait timeout and missing MI response have different diagnoses. SIGINT/SIGTERM request supervised cancellation and return useful partial JSON when possible; abrupt SIGKILL cannot finalize the journal. Subprocess groups and pipe drains are bounded; `prlimit` is installed on the stopped inferior (not GDB), optional delegated cgroups add best-effort process-tree limits, and neither provides a security sandbox.

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

The recorder preserves `coverage.scope`, `complete_for_scope`, `truncation_reasons`, `stop_model` and `trace_gaps` with attempted-position boundaries. Disassembly/AST/heap histories have their own partiality. `storage.finalized` means journal collection closed, **not** successful program completion. `--journal-only` can omit resident instruction arrays; journal records can be read sequentially, but cannot be converted into a complete `TraceStore` without final JSON.

### Measured counterexample to conflated caps

The final audit set `--max-trace-bytes 2048`: the JSONL journal was **1,961 bytes**, but separately exported JSON was **22,883 bytes** and correctly marked partial. A second run with `--max-json-export-bytes 8192` exited with `max_json_export_bytes exceeded` and wrote no JSON. Neither test establishes a peak process-RAM limit. See [parser ingestion limits](parser-ingestion.html#level-2-architecture-and-compatibility).

## Level 3 · Enforcement sites and results

- `src/resource_limits.rs::{ExecutionLimits::validate, limit_stopped_inferior, CgroupGuard}`; `src/supervisor.rs::{scope, stage_deadline, request_cancel}`.
- `src/recorder.rs::{Recorder::truncate, Recorder::checkpoint, Recorder::event}` and `src/storage.rs::{Journal::append, Journal::finalize, recover}` distinguish retained records from journal and total attempts; `src/models.rs::RecordingData` holds the serialized result.
- `src/json_export.rs::{preflight, export_json_cancellable}` bounds each emitted JSON document without first assembling a giant output string. This does **not** bound the already assembled `DebugSessionTrace` or parser RSS.

```sh
target/release/c-trace-engine fixtures/c/infinite_loop.c \
  --trace-mode dual --max-steps 60 --timeout 35 \
  --no-pretty --output /tmp/visual-c-limit.json
```

A capped trace can have `execution_status=step_limit_reached`, retained events, gaps and `coverage.complete_for_scope=false`. Consumers should read [validation findings](parser-validation.html) and [confidence](parser-semantic.html), not interpret `compiled_successfully=true` as complete execution.

> WARNING: Trace Parser input cap defaults to 512 MiB; this prevents oversized input buffering but does not make deserialization/trace assembly constant-memory. The 128-record page bound constrains **query outputs**, not the whole trace.

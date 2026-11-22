# Visual C · Stage 1 Documentation — Part 4

**Covers chapters 18–21:** Trace Engine (allocation lifetimes, Valgrind and sanitizers, processes and event transport, budgets/gaps/export).

Together with Part 3, this finishes the Trace Engine overview section. These four chapters are about **memory over time and the limits of what was seen**: how allocations are tracked, what other tools add, why child processes are hard, and what the size and time limits mean. Layout, code excerpts, field names and numbers follow the earlier parts.

---

<!-- BEGIN CHAPTER: content/engine-heap.md -->
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
<!-- END CHAPTER: content/engine-heap.md -->

<!-- BEGIN CHAPTER: content/engine-diagnostics.md -->
# 19 · Independent diagnostics · Valgrind and sanitizers

> **In one sentence:** Memory-error tools add valuable clues, but they run *separately* from the main GDB observation, so their findings belong to their own runs.

## Key terms

| Term | Meaning |
|---|---|
| **Valgrind** | Runs the program under memory checking and reports leaks or invalid accesses. Generally slower. |
| **Sanitizer** | Compiler-inserted checks in a separately built program, such as AddressSanitizer for memory misuse. |

> **Example.** A Valgrind leak at line 52 may point you at useful source code. It cannot identify "allocation 17" from the debugger's run just because that run also visited line 52.
>
> Each analysis records its own status. A **skipped or failed** analysis is not the same as "checked, no errors found."

## Level 1 · Intuition

GDB shows what happened in the **primary** traced run. Valgrind and sanitizer runs can detect memory problems the primary trace doesn't prove, but they execute the program *again*.

- A similar address or source line across executions is **not** the same allocation.
- A diagnostic belongs to its own run.
- Missing or truncated output is **not** a clean bill of health.

{{diagram:metadata}}

## Level 2 · Stages and uncertainty

### Valgrind

| Point | Detail |
|---|---|
| How it runs | Starts **another execution** and writes an XML report |
| What the engine reads | Complete error entries, leak details and summary numbers |
| Truncated output | `recover_errors` keeps the `<error>` entries that were fully written, and marks the whole report **incomplete** |
| Missing leak total | Means "we don't know the total," **not** zero (even if an older default field contains `0`) |
| The allocation wrapper | Does **not** actively track blocks in the Valgrind run, because its own table could make leaks look reachable and hide the problem |

### Sanitizer

It is compiled into a **different executable**. The `sanitizer_trigger` setting chooses when it runs:

| Setting | Runs the sanitizer… |
|---|---|
| never | Never |
| after a crash | Only if the primary run crashed |
| after a crash or Valgrind error | **Default** |
| always | Always |

The trace records whether the stage was **skipped, timed out, failed or completed**.

| Situation | How it's treated |
|---|---|
| An ASan report and a primary GDB crash have a supported source-location match | May be shown together as **related clues** |
| No such support | The sanitizer finding stays **independent** |
| Nonzero exit, no usable diagnostic | **Inconclusive**, not proof of a memory bug |
| A deadline arrives | May stop optional checks |
| Valgrind fails | Does **not** change the status of the program's primary run |

### Worked crash: a useful prefix, separate completeness

```c
// Source: fixtures/c/phase8r3_crash.c
int observed_global = 123;
int main(void) { *(volatile int *)0 = observed_global; return 0; }
```

In a fresh audited run, this produced:

| Result | Value |
|---|---|
| `execution_status` | `"crashed"` |
| Crash report | `SIGSEGV` |
| Snapshots | **3** |
| Retained instruction attempts | **5** |
| `coverage.complete_for_scope` | `true` (for the observed user-code interval) |
| `heap_history.complete` | `false` (the allocation tracker got no normal terminal ACK) |

These are **different dimensions**. Neither the crash nor the heap partiality erases the captured stops, and a complete user-code scope does not make heap history complete. With a forced sanitizer pass, the separately run sanitizer also produced a diagnostic. Don't infer unrecorded writes or a complete process-tree history from the retained prefix.

### Source site is not identity

- `src/valgrind.rs::correlate_with_allocations` is a legacy-name helper that now **ignores** supplied primary allocation IDs.
- It may produce ID-free **same-absolute-file/line candidates**, but never an exact cross-run allocation link.
- Imported legacy `related_allocation_ids` can remain in raw reports, yet TP-6 `valgrind_error(index).primary_allocation_id` is **always `None`**.

See [confidence contracts](parser-semantic.html#independent-diagnostics).

## Level 3 · Models and real failure path

| File / item | Role |
|---|---|
| `src/valgrind.rs::{parse_valgrind_xml, recover_errors, correlate_with_allocations}` | Valgrind parsing and recovery |
| `src/sanitizer.rs::{parse_sanitizer_output, matches_crash}` | Sanitizer parsing and crash matching |
| `src/trace.rs` | Schedules passes and records `stage_results` |
| `src/models.rs::{ValgrindReport, ValgrindError, SanitizerReport, CrashReport, StageResult}` | Holds outputs |
| `ValidatedSemanticView::diagnostics()` | Borrows the session reports |
| `valgrind_error(index)` | A trace-local reference to an **independent** diagnostic |

```rust
let diagnostics = validated.diagnostics();
if let Some(report) = diagnostics.valgrind {
    for index in 0..report.detailed_errors.len() {
        let item = validated.valgrind_error(index)?;
        assert!(item.primary_allocation_id.is_none());
    }
}
```

The code assumes a previously defined `validated` view; see the runnable [usage example](usage.html#level-3-load-and-validate).

In the final Stage 1 audit, a fresh Valgrind fixture produced one native diagnostic and **zero** primary allocation links. The crash and forced-sanitizer samples each kept only the reports actually observed. A missing report is never a negative diagnosis.

> **Warning:** A primary-run allocation ID found in an imported Valgrind raw field is **not** an authorized Stage 2 identity join. Never use matching source, address, PID or name across executions as a substitute for same-execution event evidence.
<!-- END CHAPTER: content/engine-diagnostics.md -->

<!-- BEGIN CHAPTER: content/engine-process.md -->
# 20 · Process lifecycle, forks and event transport

> **In one sentence:** A process can create children. Seeing events from two PIDs does *not* prove there were only two processes.

## Key terms

| Term | Meaning |
|---|---|
| **PID** | The operating system's process ID. Wrapper event lines include it so events from a parent and its observed children stay separate. |
| **ACK** | A final status message from an observed process, reporting its last event number and whether any writes failed. |

> **Example.** If a child prints nothing and never writes to the wrapper file, the event reader can't discover it by looking at recorded PIDs. The parent and the observed PIDs might have complete logs while that child stays unknown.
>
> **Complete history for observed event PIDs is not a census of every descendant of a fork.**

## Level 1 · Intuition

The debugger follows one inferior. The allocation wrapper can *also* write PID-labelled events from forked children. These are **different scopes**.

A child that forks, logs no allocation and exits silently may be invisible. "Every observed PID's events were verified" can never mean "every descendant was discovered."

{{diagram:partial}}

## Level 2 · From wrapper file to scoped coverage

### How events are written

The C wrapper writes one completed line per event to `C_TRACE_EVENT_FILE`. Each line contains:

| Field | Meaning |
|---|---|
| PID | Which process |
| Sequence number | The next event for that PID |
| Operation | What was called |
| Allocation ID | The tracked ID |
| Address and result | What came back |

### What a fork does to IDs

- After a fork, the **child resets** its inherited logger lock and counters. So parent and child can both have a *local* allocation ID 1.
- In the final JSON, Rust can combine PID and local ID into a wider ID, `(pid << 32) | local_id`, so that observed processes' lifetimes stay distinct.
- Older journal rows may still show the local IDs.
- This is a change in representation **within one trace**. It is not a link between independent program runs.

### How the reader decides history is complete

- On normal exit, a process may write a final **ACK** with its last event number and any write failures. A crash or `_exit` may not.
- The reader detects incomplete lines, missing sequence numbers, missing ACKs and caps, and marks the history **partial** when appropriate.

### What "complete" is scoped to

`process_scope="verified_observed_pids_only"` means the engine checked:

1. the processes that actually supplied event logs, **plus**
2. the inferior GDB followed.

When `heap_history.complete=true`, the available event sequence and transport checks passed **for that limited scope only**. It does **not** turn `unobserved_process_coverage="unknown"` into "all children observed."

| Point | Consequence |
|---|---|
| GDB's saved stack and variable snapshots | Belong to the followed inferior, **not** automatically to every child that wrote allocation lines |
| Two different PIDs in an imported trace | Don't by themselves prove a parent-child relationship |
| An observed child sends no final ACK | The engine **lowers heap-history confidence**; it does not invent a GDB snapshot of that child |

### Worked fork cases

From a fresh audit of the `phase8dr_fork.c` fixture:

| Mode | Heap events | Event PIDs | `complete` | Note |
|---|---|---|---|---|
| `observed` | 6 | 2 | scoped complete | Both processes supplied events |
| `silent` | 4 | 1 | scoped complete | **Despite** a silent child: it never wrote to the log, so it was never observed |
| `incomplete` | 6 | 2 | `false` | The child's final ACK is **missing** |

All three have `unobserved_process_coverage="unknown"`.

## Level 3 · Implementation and typed view

| Layer | Where to look |
|---|---|
| **C** | `cte_after_fork_child`, `cte_write_all`, `cte_emit_locked`, and the normal-exit status writer in `c_runtime/alloc_tracker.c` |
| **Rust** | `src/allocation_tracker.rs::{EventTailer, verify_event_status, HeapTimeline::global_id}`; `src/trace.rs` assembles `HeapHistoryCoverage`; `src/supervisor.rs` controls subprocess groups, cancellation, subreaper ownership and best-effort cgroup handling |
| **Parser** | `TraceStore::heap_process_scope()` returns `HeapProcessScope::Unknown`, `VerifiedObservedPidsOnly { observed_event_pids, followed_inferior_pid, observed_history_complete }`, or `Unsupported`. `validate_deep()` verifies annotations against recorded events. `process_coverage().unobserved_descendants` stays unavailable on sound traces |

```json
{"complete":true,"process_scope":"verified_observed_pids_only","unobserved_process_coverage":"unknown"}
```

This is an **illustrative subset** of `heap_history`. It is **not** an all-process claim and not a complete JSON object for the model. The [identity tables](schema.html#identity-domains) explain why per-PID sequences and global allocation IDs are not stops or event positions.

> **Warning:** The engine is not a security sandbox. With no delegated cgroup, descendants that escape the owned group may evade tree-wide limits. Neither `RLIMIT_NPROC` nor a global fork census is used.
<!-- END CHAPTER: content/engine-process.md -->

<!-- BEGIN CHAPTER: content/engine-limits.md -->
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
<!-- END CHAPTER: content/engine-limits.md -->

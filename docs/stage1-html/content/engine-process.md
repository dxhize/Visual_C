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

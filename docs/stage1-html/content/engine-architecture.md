# 5 · Trace Engine architecture

> **In one sentence:** The engine is a coordinator. It asks several smaller parts to compile, observe, record and export, then assembles the result.

## Two terms first

| Term | Meaning |
|---|---|
| **Supervisor** | Sets deadlines and manages child tool processes, so a hung compiler or debugger can't run forever unnoticed. |
| **Subsystem** | A group of related source files. Example: `gdb_mi` talks to GDB; `recorder` saves what was observed. |

## What happens at one pause

When GDB pauses, the engine:

1. checks **why** it stopped,
2. collects the current **frame** (the executing function and its local variables),
3. reads new **allocator log lines**, and
4. saves a **snapshot**.

Later, it may run a sanitizer in a separate execution.

> **Don't assume:** A diagram of the normal route does not promise every optional tool ran, or that every instruction between two stops was seen. The engine can pause and inspect at selected moments only. It can't fill in unseen time or unknown child processes afterwards.

**Entry points:** `trace::trace_program` arranges timeouts and cleanup, `trace_program_inner` organizes the work, and `run_gdb_phase` handles the debugger.

## Subsystems, as a journey

{{diagram:engine-subsystems}}

| Order | Module | Job |
|---|---|---|
| 1 | `cli`, `config` | Turn your command into a request |
| 2 | `supervisor`, `resource_limits` | Watch time and other limits |
| 3 | `compiler` | Ask GCC to build the debug program (including the C allocation wrapper) and report compiler errors |
| 4 | `gdb_mi`, `mi_parser` | Send commands to GDB / translate GDB's text replies into fields |
| 5 | `recorder`, `storage` | Remember pauses and attempted instructions / write a bounded JSONL journal |
| 6 | `allocation_tracker` | Turn wrapper log lines into recorded heap events |
| 7 | `dwarf`, `clang_ast` *(optional)*, `evidence` | Link records to type or source descriptions, only when a match is justified |
| 8 | `valgrind`, `sanitizer` | Handle the *other runs* |
| 9 | `models`, `json_export` | Define the result / write the final JSON |

> **Reminder:** a pointer's address matching an allocation's address is not that pointer's allocation history.

## Inside the stop loop

At a pause, GDB reports the address of the instruction the program is *about to* execute. The loop then:

1. reads new output and wrapper events,
2. finds out why the program paused,
3. captures current state **within its budgets**, and
4. decides how to continue.

Two GDB messages look alike but differ: *"I accepted your continue command"* is **not** *"the program stopped again."* If the loop skips an interval inside a library or wrapper, it records a **gap** rather than inventing instruction records.

More: [debugger chapter](engine-debugger.html) (step-by-step example), [call-flow explorer](call-flow.html) (checked code paths).

## Runtime order and conditionals

{{diagram:runtime-lifecycle}}

The diagram is an overview; not every optional pass always runs at one fixed point.

| Situation | What happens |
|---|---|
| Sanitizer trigger already met | The sanitizer pass can run **before** Valgrind |
| Trigger not yet met | The Valgrind result may decide whether the sanitizer runs |
| After runtime and heap records are assembled | DWARF association and **evidence fusion** (linking records from different providers, only where justified) |
| Clang configured | Clang extraction runs (it's optional) |
| Compilation fails | A trace with diagnostics is returned, with **no** GDB observations |

See `src/trace.rs` and [process boundaries](resource-boundaries.html) for exact branches.

## Follow the implementation

| Location | What happens there |
|---|---|
| `src/main.rs:34` | Calls `trace::trace_program` |
| `src/trace.rs:132` | Supervises the session |
| `src/trace.rs:167` | Orchestrates it |
| `src/trace.rs:1113` | Begins the debugger phase |

The source index lists all files. Function-to-function call claims belong in the verified explorer, not in arrows inferred between module names.

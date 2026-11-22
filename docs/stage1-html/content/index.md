# 1 · Visual C · Stage 1

> **In one sentence:** Visual C runs a C program under a debugger, saves what it saw into a file (a *trace*), and lets other code read that file safely.

## The big picture

```
 C files + build options
          │
          ▼
 ┌───────────────────┐        ┌──────────────┐        ┌───────────────────┐
 │   Trace Engine    │ ─────► │  trace.json  │ ─────► │   Trace Parser    │ ─► your code
 │ compile + record  │        │ (saved run)  │        │ load + check + ask│
 └───────────────────┘        └──────────────┘        └───────────────────┘
        THE CAMERA                THE VIDEO                 THE VIEWER
```

{{diagram:architecture}}

Think of it as a scientific recording. The program runs once while a tool collects evidence. Afterwards, readers ask questions of the saved evidence. They do not re-run the experiment.

## Key terms

| Term | Plain meaning |
|---|---|
| **Trace** | The saved record of **one** run of a C program. It lists what was captured *and* what could not be captured. |
| **Stage 1** | The first half of Visual C: compile, run under a debugger, record, export, then load and check. The later "semantic engine" (Stage 2) is a separate layer not covered here. |
| **Trace Engine** | The recording side. It compiles the program, runs a bounded debugger session (limited in time, steps and memory) and writes a version-2 JSON trace. |
| **Trace Parser** | The reading side. It loads the JSON so other code can inspect the records without changing them. |
| **Checkpoint** | A selected pause in the run where variables and memory were read. |
| **Gap** | An interval of execution that was *not* recorded exactly. |
| **Validated** | The records passed extra consistency checks. It does **not** mean every moment of execution was seen. |

## The central idea: observation is not conclusion

A debugger can only say *"at this pause, these bytes were readable."* That is useful, but on its own it cannot tell you more.

| The debugger showed… | You can say | You **cannot** say (from that alone) |
|---|---|---|
| Four bytes for variable `sum` at a pause | Those four bytes were readable then | `sum` was initialized earlier (it could be leftover garbage) |
| A pointer holding `0x1000` | The pointer held that address | The pointer came from allocation 7 |
| A complete log from the followed process | That process's allocator calls were seen | Every child process was observed |

Visual C records the observation **and** the uncertainty around it. As a reader, hold on to two habits:

1. Keep [addresses separate from identities](schema.html#identity-domains). An address is just a place in memory, and a later allocation can reuse it.
2. Tell apart [a structurally sound trace from a complete execution](parser-validation.html#level-2-architecture). "The file is consistent" and "we saw everything" are different claims.

## What ships

| Tool | Role in Stage 1 |
|---|---|
| **GCC** | Compiles your C code into a runnable program. |
| **GDB** | Runs the program and pauses it so values can be read. |
| **DWARF** | Debug information stored inside the compiled program; describes types and source lines. |
| **Allocator wrapper** | Records memory calls (`malloc`, `free`, …) that it can intercept. |
| **Clang** *(optional)* | Reads the source text and describes it. |
| **Valgrind / sanitizers** | Memory-error checkers that run in **other executions**, so their findings are *not* allocation IDs from the GDB run. |

One saved [trace](model.html) holds: selected pauses (**checkpoints**), attempted instructions, missing intervals (**gaps**), observed heap lifetimes (when each allocation was created and freed), and diagnostic reports.

The [Trace Engine](engine-build.html) writes it. The [Trace Parser](parser-ingestion.html) reads the final JSON **once**, builds lookup tables of positions, and returns references to the saved items, a validation report, and pages of results. Asking a new question never rewrites earlier observations.

## Choose your route

| If you want to… | Start with |
|---|---|
| **Follow the whole run** | [Complete pipeline](pipeline.html) → [Engine architecture](engine-architecture.html) → [Parser architecture](parser-architecture.html) → [Debugger lifecycle](engine-debugger.html) |
| **Navigate unfamiliar code** | [Source files](files.html), [functions and types](functions.html), [module relationships](dependencies.html), [call-flow explorer](call-flow.html) |
| **Add an engine feature** | [Compilation](engine-build.html), [variable capture](engine-memory.html), [heap telemetry](engine-heap.html) (allocation recording), [budgets](engine-limits.html) |
| **Consume evidence** | [Parser ingestion](parser-ingestion.html), [validation](parser-validation.html), [semantic confidence](parser-semantic.html), [bounded pages](parser-consumer.html), [Stage 2 handoff](handoff.html) |
| **Look up a type** | [Public API](api.html), [schema and identity tables](schema.html), the page outline, or offline search (press `/`) |

## Where to begin in code

| Purpose | Location |
|---|---|
| Recording entry point | `src/trace.rs::trace_program(TraceConfig)` |
| Saved data shape | `src/models.rs::DebugSessionTrace` |
| Parser entry point | `src/trace_parser/mod.rs::TraceStore::open` |
| Deeper validation | `TraceStore::validated_semantic()` in `semantic.rs` |
| Stable consumer example | `examples/stage1_public_consumer_audit.rs` (runnable intro in [Practical usage](usage.html)) |

## Scope and inspected version

> **Inspected version:** base revision `558037c0da180e152a3e6cc3f351edf0b0583215` **plus existing uncommitted Stage 1 changes**. The commit hash alone does not identify the inspected parser sources. See the [repository map](repository.html) and [DOC-1 report](DOC1-REPORT.md).

> **Important:** The Stage 1 freeze is approved **within a bounded evidence scope**. It is *not* a sandbox, a whole-process-tree tracer, a memory-initialization oracle, or a guarantee about assembled-trace RAM. See the [precise limits](handoff.html#known-limitations).

This site explains Stage 1, including the final `realloc` repair. It is not a manual for Stage 2 internals.

## Using this site offline

Open `index.html` in a browser. Search, diagrams, themes, navigation and content are all local, so no network is needed. The [README](README.md) explains regeneration; every generated page is checked in next to its Markdown source.

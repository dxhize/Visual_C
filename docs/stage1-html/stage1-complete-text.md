# Complete text of the Visual C Stage 1 documentation

This is an editable copy of the Markdown text in all 67 offline-site chapters, in the same order as the site navigation. The text **inside each BEGIN/END CHAPTER pair is copied verbatim** from the named `content/*.md` file. Edit that text and keep the chapter markers so your changes can later be applied to the corresponding pages. Changes to this collection do not automatically change the site. Do not regenerate this file after editing it unless you intend to discard those edits.

Diagram, inventory and interactive-explorer placeholders are preserved exactly as they appear in the chapter sources; their generated graphics, source inventory and interface are not prose from the chapters. Links ending in `.html` are the original site links.

## Chapter list

1. **Getting Started · Documentation home** — `content/index.md`
2. **Getting Started · How to read this reference** — `content/reading.md`
3. **Getting Started · What Stage 1 does** — `content/architecture.md`
4. **Architecture · Complete pipeline** — `content/pipeline.md`
5. **Architecture · Trace Engine architecture** — `content/engine-architecture.md`
6. **Architecture · Trace Parser architecture** — `content/parser-architecture.md`
7. **Architecture · Evidence and identity contracts** — `content/identity-contracts.md`
8. **Architecture · Process and resource boundaries** — `content/resource-boundaries.md`
9. **Developer Internals · Repository map** — `content/repository.md`
10. **Developer Internals · Source file index** — `content/files.md`
11. **Developer Internals · Function index** — `content/functions.md`
12. **Developer Internals · Module dependency explorer** — `content/dependencies.md`
13. **Developer Internals · Interactive call-flow explorer** — `content/call-flow.md`
14. **Trace Engine · Compilation & diagnostics** — `content/engine-build.md`
15. **Trace Engine · GDB/MI and checkpoints** — `content/engine-debugger.md`
16. **Trace Engine · Frames, variables & memory** — `content/engine-memory.md`
17. **Trace Engine · DWARF, Clang & fusion** — `content/engine-metadata.md`
18. **Trace Engine · Allocation lifetimes** — `content/engine-heap.md`
19. **Trace Engine · Valgrind & sanitizers** — `content/engine-diagnostics.md`
20. **Trace Engine · Processes & event transport** — `content/engine-process.md`
21. **Trace Engine · Budgets, gaps & export** — `content/engine-limits.md`
22. **Trace Engine Internals · Entry, configuration & errors** — `content/engine-entry-deep.md`
23. **Trace Engine Internals · Compiler & diagnostics** — `content/engine-compiler-deep.md`
24. **Trace Engine Internals · GDB controller & stop loop** — `content/engine-gdb-deep.md`
25. **Trace Engine Internals · MI2 parser & transport** — `content/engine-mi-deep.md`
26. **Trace Engine Internals · Recorder & journals** — `content/engine-recorder-deep.md`
27. **Trace Engine Internals · Frames, variables & memory** — `content/engine-stack-deep.md`
28. **Trace Engine Internals · DWARF extraction** — `content/engine-dwarf-deep.md`
29. **Trace Engine Internals · Clang AST provider** — `content/engine-ast-deep.md`
30. **Trace Engine Internals · C allocator runtime** — `content/engine-allocator-deep.md`
31. **Trace Engine Internals · Heap reconstruction** — `content/engine-heap-deep.md`
32. **Trace Engine Internals · Valgrind & sanitizer internals** — `content/engine-diagnostics-deep.md`
33. **Trace Engine Internals · Evidence fusion internals** — `content/engine-fusion-deep.md`
34. **Trace Engine Internals · Supervision & resource limits** — `content/engine-supervision-deep.md`
35. **Trace Engine Internals · Trace model, journal & export** — `content/engine-export-deep.md`
36. **Trace Engine Internals · Subsystem walkthroughs** — `content/engine-walkthroughs.md`
37. **Trace Engine Internals · Source file handbook** — `content/engine-handbook.md`
38. **Trace Engine Internals · Algorithms & invariants** — `content/engine-algorithms.md`
39. **Trace Engine Internals · Trace Engine function reference** — `content/engine-functions.md`
40. **Data model · Trace data model** — `content/model.md`
41. **Trace Parser · TP-1 · Load & compatibility** — `content/parser-ingestion.md`
42. **Trace Parser · TP-2 · Position indexes** — `content/parser-indexes.md`
43. **Trace Parser · TP-3 · Borrowed views** — `content/parser-views.md`
44. **Trace Parser · TP-5 · Deep validation** — `content/parser-validation.md`
45. **Trace Parser · TP-6 · Evidence confidence** — `content/parser-semantic.md`
46. **Trace Parser · TP-7 · Bounded queries** — `content/parser-consumer.md`
47. **Trace Parser Internals · JSON and JSONL ingestion** — `content/parser-ingestion-deep.md`
48. **Trace Parser Internals · Ownership, views & evidence references** — `content/parser-ownership-deep.md`
49. **Trace Parser Internals · Compact position indexes** — `content/parser-indexes-deep.md`
50. **Trace Parser Internals · Deep validation architecture** — `content/parser-validation-deep.md`
51. **Trace Parser Internals · Realloc verification and TP8-F01** — `content/parser-realloc-deep.md`
52. **Trace Parser Internals · Pointer provenance & process coverage** — `content/parser-coverage-deep.md`
53. **Trace Parser Internals · TP-6 validated semantic evidence** — `content/parser-semantic-deep.md`
54. **Trace Parser Internals · TP-7 consumer composition** — `content/parser-consumer-deep.md`
55. **Trace Parser Internals · Parser data structures** — `content/parser-data-structures.md`
56. **Trace Parser Internals · Parser algorithms & invariants** — `content/parser-algorithms.md`
57. **Trace Parser Internals · Parser walkthroughs** — `content/parser-walkthroughs.md`
58. **Trace Parser Internals · Parser source file handbook** — `content/parser-handbook.md`
59. **Trace Parser Internals · Parser function reference** — `content/parser-functions.md`
60. **Execution Walkthroughs · Execution walkthroughs** — `content/walkthroughs.md`
61. **Practical usage · Worked API walkthrough** — `content/usage.md`
62. **Reference · Public API reference** — `content/api.md`
63. **Reference · Stage 1 public API** — `content/stage1-api.md`
64. **Reference · Schema & identity reference** — `content/schema.md`
65. **Reference · Stage 2 handoff & limits** — `content/handoff.md`
66. **Reference · Terminology and glossary** — `content/glossary.md`
67. **Reference · Known limitations** — `content/limitations.md`

---

<!-- BEGIN CHAPTER: content/index.md -->
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
<!-- END CHAPTER: content/index.md -->

<!-- BEGIN CHAPTER: content/reading.md -->
# 2 · How to read this reference

> **In one sentence:** Don't read in order. Start with a question, find the page that answers it, and only then open the code.

## Pick a starting point

| Your question | Go to |
|---|---|
| "How does a program turn into a trace?" | [Complete pipeline](pipeline.html) |
| "What is this type or function?" | [Function index](functions.html), then the [call-flow explorer](call-flow.html) |
| "Where does this file/directory fit?" | [Repository map](repository.html), [Source file index](files.html) |
| "What does this recorded value mean?" | [Identity contracts](identity-contracts.html) |
| "Why is an allocation missing?" | Heap overview → event-reader links → coverage links → compare with the actual trace |

> **Example.** To find why an allocation is missing, follow that last route and then compare with the real trace file. Finding a record with a *matching address* is not an answer, because addresses are reused.

## What an index entry does and doesn't tell you

| An index/graph entry… | Means | Does **not** mean |
|---|---|---|
| Lists a file or definition | The code exists at that path | It ran in any particular recording |
| Was found by a script | A real Rust/C definition at a real location | A verified call, or a written explanation of its algorithm |
| Appears as a graph edge | Someone read the production caller and callee | — (edges are published only after this check) |
| Is labeled **"conceptual"** | It explains the architecture | A runtime function call |

For deeper reading, the **DOC-2 engine chapters** and **DOC-3 parser chapters** include source-guided walkthroughs, branches and error paths. Use the navigation to open them.

## Version this reference describes

This is a map of the **inspected working tree** (the folder of code as it was on disk), not of a commit alone.

- Base revision: `558037c0da180e152a3e6cc3f351edf0b0583215`
- The parser and its tests had **uncommitted changes** at inspection time.
- Line numbers refer to that state and can shift as the repository changes.

## Navigating offline

- **Sidebar, breadcrumbs, outline, previous/next** all work from checked-in HTML.
- **Search** uses a generated local JavaScript index (no `fetch`), covering source paths and symbol names.
- **Source links** in a `file://` browser open a local repository file. The adjacent `path:line` text gives the exact location even if the browser can't jump to the line.
- The [maintenance guide](README.md) explains regeneration.

## Read evidence before inference

Keep these five layers separate in your mind:

| # | Layer | Example of confusing it with another |
|---|---|---|
| 1 | The recorded execution | — |
| 2 | Static source metadata | A syntax match is **not** an executed expression |
| 3 | Independent diagnostics (other runs) | An address match is **not** a lifetime identity |
| 4 | Validation findings | A successful import is **not** semantic validation |
| 5 | Downstream interpretation | — |

The [limitations page](limitations.html) is part of every walkthrough, not an appendix to skip.
<!-- END CHAPTER: content/reading.md -->

<!-- BEGIN CHAPTER: content/architecture.md -->
# 3 · Architecture · one recording, many qualified readers

> **In one sentence:** The engine records, the parser reads, and every kind of evidence is kept labeled with what it can and cannot prove.

## Level 1 · Intuition

Imagine recording a short video of a C program and giving it to someone who wasn't there.

- The **Trace Engine** is the *camera*. It compiles the program, runs it under a debugger, and saves the moments it actually observed.
- The **Trace Parser** is the *viewer*. It opens the finished recording and helps other code find moments in it.

The viewer can't ask the finished program what happened *between* two recorded moments. That's why the trace also records **gaps**: the places where the camera wasn't looking. The recording is a JSON file, and parser queries point back to its original observations rather than changing it.

{{diagram:architecture}}

## Level 2 · Pipeline and boundaries

### The tools, in plain words

| Tool / term | What it is | Important limit |
|---|---|---|
| **GCC** | The C compiler | — |
| **Debug executable** | A compiled program with extra info for debuggers; the engine also links an allocation logger into it | — |
| **DWARF** | The debug-info format inside that executable | Describes the *compiled result*; gives no value for every variable at every moment |
| **Clang AST** *(optional)* | A tree Clang builds from the *source text* (it can spot the written `malloc(40)`) | Text presence does **not** show the call ran |
| **GDB** | The debugger that runs and pauses the executable | Sees pauses, not the time between |
| **MI2** | GDB's machine-readable command format, so the engine can ask for a frame or variable automatically | — |
| **Inferior** | The program GDB is debugging | A *followed* inferior is observed; a forked child it doesn't follow gets no automatic snapshots |

### What happens, in order

| # | Step | What happens |
|---|---|---|
| 1 | **Configure and build** | A `TraceConfig` names the C files, program arguments, and time/memory/recording limits. GCC builds the debug program and may *try* a second build with a **sanitizer** (compiler-inserted checks for buffer overflows, use-after-free, etc.). If compilation fails, the result holds compiler errors, not imaginary snapshots. |
| 2 | **Read types, then observe** | The engine reads DWARF. If requested, Clang examines the source. GDB runs the program and pauses it at selected **stops**. A stop is a moment we can ask "what does this variable look like *right now*?" |
| 3 | **Log allocations** | A **wrapper** around `malloc`, `free`, etc. acts as a detour: the call reaches the wrapper, which logs it and then calls the real allocator. Each line carries a **PID** (which process) and a **sequence number** (order within that process). The engine reads new lines at each stop. The log goes to its own file, never mixed into the program's output. |
| 4 | **Package and export** | Observations become one `DebugSessionTrace`, exported as JSON. A **JSONL journal** (one small JSON record per line) may be written along the way; it helps keep a partial run but can't rebuild the final document. |
| 5 | **Load and read** | `TraceStore::open` loads the JSON and builds position lookups. Loading only says the file has a *supported shape*. Call `validated_semantic()` to check consistency. Queries return references to original records, in pages of **at most 128** records. |

{{diagram:compilation}}

### Three runs that must not be mixed up

| Run | What executes | Reports | Can supply Run 1 allocation IDs? |
|---|---|---|---|
| **Run 1 · primary** | Debug executable under GDB | Stopped variables and heap events with IDs like "allocation 17" | **Yes** (via its own wrapper) |
| **Run 2 · if enabled** | Separately built sanitizer executable | Memory problems caught by compiler-added checks | No |
| **Run 3 · if enabled** | **Valgrind** on another execution (no sanitizer build needed) | Invalid memory operations, leaks | No |

Each run has its own status: succeeded, failed or skipped.

> **Example.** Valgrind reports "40 leaked bytes at line 52." That's useful, but it does **not** prove the leaked block is allocation 17 from Run 1, since addresses and allocation order can differ between runs.

### What each evidence provider can and cannot prove

| Provider | Can tell you | Cannot prove by itself |
|---|---|---|
| GCC and DWARF | How the compiled program describes types, declarations, source lines | Which expressions ran, or what a variable held later |
| GDB/MI | State at a recorded pause, including bounded reads | Anything between pauses, or an unfollowed child's state |
| C allocation wrapper | Calls it intercepted in the primary run, with PID, order, ID | Allocations made wholly inside unwrapped shared libraries |
| Optional Clang AST | Code that appears in the source, and where | That an expression executed |
| Valgrind / sanitizers | Errors in their own executions | The exact primary-run allocation ID of an error |

## Level 3 · Trace through the implementation

Start at `src/trace.rs::trace_program`. In order, it:

1. checks `ExecutionLimits`,
2. starts a supervised session,
3. calls `compiler::compile_project` to build the program,
4. extracts DWARF with `dwarf::extract_excluding`, and
5. runs `clang_ast::analyze` only if configured.

The stop loop uses `src/gdb_mi.rs`, `src/recorder.rs` and `src/allocation_tracker.rs`. `src/models.rs` defines the saved data; `src/json_export.rs` writes it.

On the reading side, `src/trace_parser/mod.rs::TraceStore::assemble` keeps one shared `Arc<DebugSessionTrace>` plus position indexes. **`Arc`** is Rust's shared-ownership handle: many readers share one trace without copying its records.

```rust
// Core architecture signatures; source: src/trace.rs and src/trace_parser.
pub async fn trace_program(config: TraceConfig) -> Result<DebugSessionTrace>;
// TraceStore's public entry point (method declaration):
pub fn open(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<Self>;
// Constructing this view runs one opt-in validation pass:
pub fn validated_semantic(&self) -> ValidatedSemanticView<'_>;
```

> **Note:** These are **signature excerpts**, not one runnable file; each lives in its own module. See [practical usage](usage.html) for a working program.

### Four questions to ask at every boundary

Before treating two facts as one story, ask:

- [ ] Did GDB observe this?
- [ ] Is it only a description of the source?
- [ ] Did it come from another run?
- [ ] Is it a parser warning or a later interpretation?

For instance, a pointer holding an address and an allocation recorded at that address are **two separate observations**. [Evidence confidence](parser-semantic.html) shows how the parser keeps them apart.
<!-- END CHAPTER: content/architecture.md -->

<!-- BEGIN CHAPTER: content/pipeline.md -->
# 4 · Complete Stage 1 pipeline

> **In one sentence:** One C program goes from source files to a saved recording, then a reader opens that recording. The parser never runs C code and can't fill in missing observations.

The key distinction throughout: **what the program did in the main debugger run** versus **what another tool found in a separate run**.

## The system boundary

{{diagram:stage1-system}}

## Part A · Recording (Trace Engine)

### Step 1 — Build a program we can inspect

| Piece | What it does |
|---|---|
| **GCC** | Builds a **debug executable**: a program with extra info so GDB can link machine instructions to source lines, types and variables. |
| **DWARF** | That extra info. It describes the *compiled program*, not a history of what happened when it ran. |
| **Allocation logger** | Built in with `--wrap`, a linker option that redirects calls (see below). |
| **Sanitizer build** | GCC *attempts* a second executable with sanitizer checks. It may fail or be skipped without erasing the debug run's observations. |
| **Clang AST** *(optional)* | A structured outline of written expressions. An entry for `malloc(40)` is a clue about the source, not evidence the call happened. |

**How `--wrap` works.** The linker option `--wrap=malloc` redirects calls so they pass through the project's own function first:

```
your code calls:   malloc(40)
                      │
                      ▼
              __wrap_malloc(40)      ← the project's wrapper: writes a log entry
                      │                 ("this run allocated 40 bytes")
                      ▼
              __real_malloc(40)      ← the real allocator, reached through the linker
```

`calloc`, `realloc` and `free` are wrapped the same way, and your C source doesn't change. The limit: only calls it *intercepts* are recorded. Allocations made entirely inside unwrapped libraries aren't promised.

### Step 2 — Observe the primary run

| Piece | What it does |
|---|---|
| **GDB** | Runs the debug executable. |
| **MI2** | GDB's machine-readable language: "continue," "stop," "show me this variable." |
| **Inferior** | The program being debugged. The engine observes its *followed* inferior, not every process created by `fork`. |
| **Stop** | A pause where variables and memory can be read. New wrapper log lines are read at every stop. |
| **PID + sequence number** | Each log line says which process wrote it and its order within that process. |

The wrapper log is a separate file so allocator messages can't be confused with the program's normal output. And a child process or instruction that was never observed doesn't become a fact just because a later stop was captured.

### Step 3 — Keep separate runs separate

| Run | Executable | Status |
|---|---|---|
| **Run 1 · primary** | Debug executable under GDB | Own status |
| **Run 2 · sanitizer** (if enabled) | Separate sanitizer executable | Own status |
| **Run 3 · Valgrind** (if enabled) | Another execution | Own status |

Statuses are *completed, failed or skipped*. Runs 2 and 3 can report useful errors such as "40 bytes leaked near line 52." They can't reliably say "that was allocation 17 in Run 1," since a new run may use different addresses or allocate in a different order. **Only Run 1's wrapper events supply Run 1's allocation IDs.**

### Reading the diagram's arrows

| Arrow | Meaning |
|---|---|
| **Solid** | A tool produces an artifact or passes actual runtime data. |
| **Dashed** | Optional, independent or only a possible match. It does **not** mean Clang or Valgrind observed the GDB execution. |

See [Trace Engine architecture](engine-architecture.html) for control flow and [evidence contracts](identity-contracts.html) for what each tool can prove.

## Part B · Handoff and reading

{{diagram:engine-parser-flow}}

| Stage | What happens | What it does **not** do |
|---|---|---|
| **Package** | Observations gather into a `DebugSessionTrace` (defined in `src/models.rs`). | — |
| **Export** | `src/json_export.rs` writes it as JSON. | — |
| **Journal** *(during recording)* | **JSONL** writes records one per line and can preserve a partial run. | It is **not** a full replacement for the final JSON; it lacks some final reports and reconstructed state. |
| **Load** | `TraceStore::open` keeps one canonical shared copy and builds small lookup tables. | A successful load says the schema is supported. It does **not** prove records agree with each other. |
| **Validate** | Call `validated_semantic()` before relying on relationships such as "this realloc came from that earlier allocation." | The parser never restarts GDB or changes history. |

## Where to continue

- **Inside the recording:** [engine architecture](engine-architecture.html)
- **How loading avoids copying per query:** [parser ownership](parser-architecture.html)
- **Individual implementations:** [source file index](files.html)
- **New to `PID`, allocation ID, checkpoint or `EvidenceRef`?** Keep the [glossary](glossary.html) open alongside.
<!-- END CHAPTER: content/pipeline.md -->

<!-- BEGIN CHAPTER: content/engine-architecture.md -->
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
<!-- END CHAPTER: content/engine-architecture.md -->

<!-- BEGIN CHAPTER: content/parser-architecture.md -->
# 6 · Trace Parser architecture

> **In one sentence:** The parser works *after* recording. It makes a saved trace safe and convenient to inspect without pretending it can watch the program again.

## Two terms first

| Term | Meaning |
|---|---|
| **Deserialize** | Turn JSON text into Rust records. Proves the file is readable in the expected format, **not** that its claims agree. |
| **Validate** | Check relationships between records (e.g. does a claimed realloc predecessor have supporting events?). |

> **Example.** A JSON file says `old_id=7`, but event 7 was a different allocation. Loading succeeds because the field is well-formed. Deep validation rejects the unsupported relationship.

The parser answers questions about a recorded trace **without rebuilding or silently correcting it**. It does not guess which record the author meant. Its store is immutable (nothing changes after loading), and queries **borrow** the original records: you get a reference, not a copy. Positions in compact indexes point back into one canonical `Arc<DebugSessionTrace>`.

## Loading is not validating

{{diagram:parser-subsystems}}

| File | Job |
|---|---|
| `json.rs` | Selects and decodes supported schema-v2 JSON |
| `journal.rs` | Handles ordered JSONL records separately |
| `mod.rs` | Constructs `TraceStore` |
| `indexes.rs` | Builds position-based lookups instead of cloning records |
| `views.rs` | Exposes raw and borrowed views |
| `evidence.rs` | Checks trace-scoped references |
| `validation.rs` | **Opt-in** checks; can report a trace as structurally unsound, partial, or having an unavailable relationship |
| `semantic.rs` | Confidence-qualified evidence via `validated_semantic()` |
| `consumer.rs` | Bounded checkpoint pages (up to **128** entries per request) |

> **Name clash:** `trace_parser/semantic.rs` is a Stage 1 evidence interface. It is **not** the excluded Stage 2 `src/semantic/` module.

## Imported trace ownership

{{diagram:ownership}}

- Cloning a `TraceStore` shares its trace and indexes through `Arc`, so a clone is cheap.
- A second, independently loaded copy of an identical JSON document is a **different owner**.
- An `EvidenceRef` (a citation pointing to one record) is valid only against the store that issued it. Another store's numeric positions don't become valid references.

**Memory notes:**

- Index memory estimates are **not** the parser process's RSS (memory the process actually holds), and not a cap on whole-trace RAM.
- The exporter's cap applies to each written JSON document, not to an assembled trace in memory.

## The realloc repair (TP8-F01)

| | |
|---|---|
| **Found by** | The historical **TP-8 audit** (a review of the parser) |
| **Problem** | A forged `realloc` predecessor could be reported with `Definite` confidence (the strongest level) even though nothing supported it |
| **Fix** | **TP8-F01** changed deep validation and semantic confidence |
| **Now required** | Compatible recorded realloc events **and** lifetime boundaries from the same PID |
| **Unchanged** | Raw imported fields are still preserved; schema-only loading is still allowed |

Reading order: [deep validation](parser-validation.html) → [confidence](parser-semantic.html) → [the approved Stage 1 audit](../stage1-final-audit.md). The older conditional TP-8 report predates the repair.

## Next implementation path

Start at `src/trace_parser/mod.rs`, then `json.rs`, `indexes.rs`, `views.rs`, `validation.rs`. The [function index](functions.html) also lists private parsers and helpers. The [DOC-3 parser handbook](parser-handbook.html) already explains their control flow in more detail.
<!-- END CHAPTER: content/parser-architecture.md -->

<!-- BEGIN CHAPTER: content/identity-contracts.md -->
# 7 · Evidence and identity contracts

> **In one sentence:** A trace is full of numbers, and they are *not* interchangeable just because they print the same.

## Two terms first

| Term | Meaning |
|---|---|
| **Identity domain** | What a number identifies: a debugger stop, an array position, a process, a heap lifetime. Each has its own rules. |
| **Provenance** | Evidence of where a pointer actually came from, which is more than "which recorded block contains this address." |

> **Example.** A block at `0x1000` is freed, and a different block is later allocated at `0x1000`. The address matches, but the wrapper gives the two lifetimes different IDs. A pointer holding `0x1000` doesn't tell us which lifetime produced it.
>
> Likewise, a matching source line or value from a Valgrind run is a possible clue, not proof of a link to the main debugger run.

Evidence can't be made more certain by reusing a number from the wrong domain. Stage 1 keeps four things distinct, each with its own identities: the stopped execution, the retained recording, static metadata, and separate diagnostic runs.

## Identity domains

{{diagram:identity-domains}}

| This… | …is not the same as | Because |
|---|---|---|
| Snapshot vector position | GDB stop ID | Different counters |
| Attempted instruction | Retained instruction-event position | Retention and gaps are accounted separately |
| Frame activation (one call of a function) | A variable's address | Different things |
| PID-local allocation event / trace-local lifetime ID | A numeric address | They're linked by recorded wrapper events and reconstructable boundaries |
| DWARF declaration/type IDs | AST declaration/expression IDs | Two separate static providers |
| A `TraceStore` position | A valid reference elsewhere | References also require the same canonical trace owner |
| Matching source site (file + line) | Exact same-execution allocation identity | At best a **candidate** |

## Facts, candidates and unknowns

| Evidence | Supported interpretation | Unsupported promotion |
|---|---|---|
| GDB stop and bounded read | Observed stopped-state bytes/status | Initialized C object, or every intermediate instruction |
| DWARF DIE and optional AST node | Compiled metadata or source syntax | Proof that a syntax node executed |
| Wrapper event and same-PID lifetime | Observed interceptor event and qualified predecessor | Origin inferred from a reused address |
| Valgrind/ASan report | Independent diagnostic | Primary-run allocation ID from matching line/address |
| Complete observed-PID heap history | Verified scope of seen PIDs and the followed inferior | Census of silent fork descendants |

*A **DIE** is one entry in the DWARF data. **ASan** is AddressSanitizer, the common sanitizer for memory errors. **Silent fork descendants** are child processes that produced no wrapper events.*

## Provenance at the public boundary

{{diagram:public-consumer}}

Check `ValidationReport::is_structurally_sound()` **separately** from whether the trace is partial.

`validated_semantic()` borrows the store and labels each relationship with one of five confidence levels:

| Level | Meaning |
|---|---|
| `Definite` | Verified for a **specific** observation, within its documented scope |
| `Candidate` | Compatible evidence, but unproved |
| `Ambiguous` | Several relationships fit |
| `Unavailable` | Not enough evidence |
| `Contradictory` | Structurally rejected |

A contradiction doesn't erase the raw record. Missing evidence is never converted into a negative fact. Bounded consumer pages expose the original trace-scoped references, and Stage 1 intentionally ends there.

## Read next

When an identity looks obvious from printed addresses alone, check [schema and identity reference](schema.html), [parser validation](parser-validation.html) and [limitations](limitations.html) first.
<!-- END CHAPTER: content/identity-contracts.md -->

<!-- BEGIN CHAPTER: content/resource-boundaries.md -->
# 8 · Process and resource boundaries

> **In one sentence:** Different limits protect different stages. A cap on what's written to a file is not a cap on what the process holds in memory.

## The caps at a glance

| Limit | Controls | Does **not** control |
|---|---|---|
| **Journal cap** | Max size of the JSONL record stream written while tracing | The final JSON document |
| **Export cap** | Max size of one final JSON document written out | The already-built trace sitting in RAM |
| **Parser input / page limits** | What the parser accepts and returns (TP-7 rejects pages wider than **128** rather than truncating) | The engine's export setting; process RSS |
| **Resource controls overall** | Accidental runaway work | A hostile program (not a sandbox) |

> **Example.** A 2 KB journal can stop growing while the final JSON still takes 22 KB. That's expected: they are different products with different caps, and the missing journal tail must stay visible.

The engine limits a collection session, the journal and the export **independently**. The parser has its own input/page limits and validation findings. None of this implies a hostile-code sandbox, and none bounds all assembled Rust objects.

## Who enforces what

{{diagram:resource-failure}}

| Layer | Enforces |
|---|---|
| `supervisor` | A whole-session **monotonic deadline** (clock that never jumps backwards) and child-process cleanup |
| `resource_limits` | Supported process restrictions; process-count limits use **cgroups** (a Linux feature for limiting process resources) only where available |
| GDB stop loop | Budgets for steps, instructions, commands, snapshots, variables and memory reads |
| Recorder / storage | Journal limits on JSONL records |
| `json_export` | A separate per-document byte cap on the final JSON |

**What happens on failure:** a timeout or limit may retain a **partial trace**. An export failure is an operational error and is **not** a successful exported trace.

## Two different completeness questions

`execution_status`, `stage_results`, gaps and process/heap coverage each answer a different question:

- A stage can fail *after* some records were captured.
- A debugger can stop at a limit without proving the C program would never terminate.
- A heap event transport or ACK (acknowledgement message) failure shrinks the verified observed-PID history.
- Even a complete observed-PID history says nothing about a child that produced no event.

The parser reports these distinctions and does not fill them in.

## Parser and consumer limits

- The parser rejects oversized input **independently** of the engine's export setting.
- It does not reconstruct final trace state from JSONL.
- Validation can call a trace structurally sound but **partial**, or mark one relationship unavailable.

Concrete settings: [engine limits](engine-limits.html) and [the parser API](api.html).

## Security scope

> **Warning:** These controls protect against **accidental** runaway work. They do not protect the host against an adversarial executable. For hostile C programs, use external OS or container isolation. Read the actual resource and process modules before changing any cleanup assumptions.
<!-- END CHAPTER: content/resource-boundaries.md -->

<!-- BEGIN CHAPTER: content/repository.md -->
# 9 · Repository map

> **In one sentence:** A map from "what do I want to know?" to "which directory answers it?" You don't need to memorize the repository.

## Key terms

| Term | Meaning |
|---|---|
| **Production source** | The Rust and C code that actually performs recording or parsing, as opposed to tests or generated HTML pages. |
| **Fixture** | A small example C program used by tests to check one behavior. |

## How the crate is organized

The project is **one Rust package** with one CLI binary and one library target. Stage 1 is *not* split into separate engine and parser packages. The two halves share the trace model (`models`) as their common **wire contract**: the engine writes it, the parser reads it.

> **Out of scope:** Stage 2's `src/semantic/` lives in the same crate but is **outside this documentation inventory**.

## Production directories

| Directory | Responsibility | Start here |
|---|---|---|
| `src/` | CLI, session orchestration, models, debugger, metadata, diagnostics, limits and export | [Engine architecture](engine-architecture.html) |
| `src/evidence/` | Post-processing links between *existing* provider records, without turning syntax into execution | [Metadata chapter](engine-metadata.html) |
| `src/trace_parser/` | Schema ingestion, immutable indexes, borrowed views, opt-in validation, qualified evidence and pages | [Parser architecture](parser-architecture.html) |
| `c_runtime/` | The C allocator wrapper that is compiled and linked into the debug executable | [Heap chapter](engine-heap.html) |
| `tests/` | Rust integration tests and adversarial regression tests | [Source file index](files.html) |
| `fixtures/c/` | C programs used as observable test subjects | [Practical usage](usage.html) |
| `examples/` | CLI outputs, audit probes and reference consumers | [API reference](api.html) |

## Where the project touches the outside world

Visual C talks to other programs. These are **subprocess boundaries** (separate programs communicating through files or pipes), not Rust modules calling each other in-process.

| External piece | How it's used |
|---|---|
| `Cargo.toml` | Describes the Rust package and its libraries |
| `src/config.rs` | Models the build and runtime requests |
| **GCC** | Builds the target binaries |
| **GDB/MI2** | Communicates through a tokened process protocol (messages carry a token so replies can be matched to requests) |
| **`addr2line`** | May resolve which source location a caller address belongs to |
| **Clang** *(optional)* | Produces syntax metadata |
| **Valgrind** | Emits its own separate XML diagnostics |
| **Sanitizer binaries** | Emit their own separate reports |

## Example route: from C allocation to trace record

> To understand how a C allocation becomes a trace record, read in this order:
>
> 1. `c_runtime/alloc_tracker.c` (the C wrapper that logs the call)
> 2. `src/allocation_tracker.rs` (Rust side that turns log lines into events)
> 3. the heap parser chapters (how the saved events are read back)

A file's presence doesn't mean its optional feature ran in every trace. Check the recording's **stage statuses** too.

## Navigate by relationship

{{diagram:module-dependencies}}

Module-level arrows show **data or compile-time dependencies**. They do *not* claim that every named function calls another module's function.

| For… | Use |
|---|---|
| Direct calls | [Call-flow explorer](call-flow.html) |
| All definitions and their locations | [Function index](functions.html) |
| Machine-readable data | The [inventory](assets/data/stage1-inventory.json), which can later be extended with carefully verified edges |

## Which version is documented

> The source state is base commit `558037c0da180e152a3e6cc3f351edf0b0583215` **plus pre-existing uncommitted changes**. The commit alone does not reproduce the parser's TP-6/7 and TP8-F01 implementation as inspected. See the [DOC-1 report](DOC1-REPORT.md) for exact scope and caveats.
<!-- END CHAPTER: content/repository.md -->

<!-- BEGIN CHAPTER: content/files.md -->
# 10 · Source file index

> **In one sentence:** A table of contents for the actual source tree. A path tells you *where* an implementation lives, not that it ran.

## Key terms

| Term | Meaning |
|---|---|
| **Generated catalog** | A listing built by scanning the current source tree, so you can navigate even when a file has no long explanation yet. |
| **Source location** | A file and line number where a definition can be read. The number may shift after code edits. |

> **Example.** Searching `src/gdb_mi.rs` helps you find the debugger commands. It does **not** show a particular command ran in your trace. That needs recorded evidence.

## What's listed

- Every Stage 1 Rust file under `src/`
- The allocator wrapper under `c_runtime/`
- **Not** `src/semantic/` (deliberately excluded; it's Stage 2)

File names, paths and symbols come from the checked-in, generated, machine-readable inventory. Each file's short responsibility note describes a *subsystem*. It is not a replacement for reading the implementation.

## Browsing the catalog

Select a file to open its source, or follow its **symbol count** to the [function index](functions.html). Line locations refer to the inspected working tree.

> **Tip:** Local browsers may open the source file but ignore the line number. The adjacent `path:line` text is the reliable locator.

### Stage 1 file catalog

{{inventory:files}}

## How far to trust this list

| Question | Answer |
|---|---|
| How is the list checked? | Against repository **file discovery**, not an old design document. |
| Are definitions and module references verified calls? | No. They're labeled as *automatically discovered* in the JSON. |
| Where are verified **execution edges**? | Maintained separately in the [call-flow dataset](call-flow.html). |
| Do adjacent file names form a call chain? | No. For how files cooperate, use the walkthroughs. |
| Are there fuller explanations? | Yes. Follow the DOC-2 engine handbook and DOC-3 parser handbook for the existing implementation walkthroughs in this same website. |
<!-- END CHAPTER: content/files.md -->

<!-- BEGIN CHAPTER: content/functions.md -->
# 11 · Function and type index

> **In one sentence:** An address book for code names. It is *not* a list of functions your program executed.

## Key terms

| Term | Meaning |
|---|---|
| **Definition** | The place in a Rust or C file where a function or type is declared. |
| **Call-flow link** | A separate, *reviewed* arrow showing that one piece of code calls another, transfers data, or marks a conceptual boundary. |

> **Example.** Look up `validate_deep` here to open its source. Then read the validation chapter to learn why it distinguishes a corrupt trace from a partial one.

## What this index is (and isn't)

It covers production Stage 1 definitions, including important **private helpers**. For each, it gives the signature and source location as a *starting point*.

| It is | It is not |
|---|---|
| A lookup of names, signatures and locations | A story of how a function gets invoked |
| Searchable by qualified name, type or path | An explanation of each algorithm |
| An address book alongside the DOC-2/DOC-3 chapters | Proof of a runtime call |

Why it can't prove calls: source parsing alone can't see dispatch through **traits** (Rust's interfaces), **callbacks**, or another process.

## Browsing

Use the local search with a qualified function name, type or source path. Expand a symbol to see its signature and source reference.

- For selected **verified** callers and callees, open the [call-flow explorer](call-flow.html).
- Functions not connected in the curated graph are still listed here. See the existing [engine handbook](engine-handbook.html) and [parser handbook](parser-handbook.html) for longer explanations.

### Source locations

{{inventory:functions}}

## Template for future function walkthroughs

A later, full walkthrough of one function must cover these ten points. Line-by-line paraphrase should be avoided.

| # | Cover… |
|---|---|
| 1 | Purpose |
| 2 | When and under what conditions it's invoked |
| 3 | Inputs, preconditions and invariants |
| 4 | The algorithm, in meaningful groups |
| 5 | Important branches |
| 6 | Borrowed versus owned state |
| 7 | Result or emission, plus errors and partiality |
| 8 | Downstream consumers |
| 9 | Rationale (why it's built this way) |
| 10 | The exact source reference |

The metadata index is the **scaffold**, not an assertion that this prose is already complete.
<!-- END CHAPTER: content/functions.md -->

<!-- BEGIN CHAPTER: content/dependencies.md -->
# 12 · Module dependency explorer

> **In one sentence:** A dependency says one part of the code *refers to* another. It says nothing about what happened in a specific C execution.

## Key terms

| Term | Meaning |
|---|---|
| **Module** | A named group of Rust definitions, often backed by one source file or directory. |
| **Dependency** | A source-level reference, such as using another module's type. Some dependencies are about **data**, not function calls. |

> **Example.** The parser depends on the shared `models` definitions because it reads the trace format the engine wrote. That doesn't give the parser any control over a running debugger.

## How to read the maps

Treat these as **subsystem-level dependency and data-flow relationships**. A module reference is not necessarily an executed call, because several boundaries aren't function calls at all:

- external processes (GCC, GDB, Valgrind),
- serialization (writing and reading JSON), and
- optional metadata (Clang).

When you need a specific call path, select a real function in the [call-flow explorer](call-flow.html) and check its direct evidence.

## Engine module groups

{{diagram:engine-subsystems}}

| Point | Explanation |
|---|---|
| **Entry/orchestration layer** | Owns the session result. |
| **Shared `models`** | Used by the recorder, diagnostics and parser. That doesn't make them peer processes. |
| **C wrapper ↔ Rust** | The C wrapper writes a **file** that Rust reads *later*. There is **no Rust-to-C function call** in the tracing loop. |

## Parser module groups

{{diagram:parser-subsystems}}

| Layer | Role |
|---|---|
| **Parsing** | Owns the canonical trace |
| **Indexes** | Own positions (not copies of records) |
| **Raw and validated views** | *Borrow* the trace |

> **Name clash:** `src/trace_parser/semantic.rs` is Stage 1 qualification. Don't confuse it with `src/semantic/`, the excluded Stage 2 implementation.

## Sources and limits

{{diagram:module-dependencies}}

The [file inventory](files.html) shows each file's outbound module references, labeled as **static references**. They are not certified function calls.

| This alone… | …does **not** prove |
|---|---|
| A shared data type | A particular function invocation |
| An imported module | A particular function invocation |

Graph edges carry distinct classifications and source evidence instead.
<!-- END CHAPTER: content/dependencies.md -->

<!-- BEGIN CHAPTER: content/call-flow.md -->
# 13 · Interactive call-flow explorer

> **In one sentence:** An interactive map of *reviewed* paths through the Stage 1 code. The arrow labels say what kind of relationship each arrow shows.

## Arrow types

| Label | Meaning |
|---|---|
| **Direct** | A checked call expression in source: one function explicitly calls another. |
| **Callback dispatch** | A call routed through a callback rather than named directly. |
| **Subprocess communication** | Talking to an external process (GCC, GDB). |
| **Data handoff** | Data passed from one part to another, such as a file or struct. |
| **Conceptual** | Two parts belong to the same explanation; **no** runtime function call is claimed. |
| **Dashed / candidate** | A possible link. It is **not** an invocation. |

These types stay visually and textually distinct on the graph.

> **Example.** Select the path from `trace_program` to GDB capture. Open each source link to verify exactly where a function is called, and use the other arrow types for tool or data boundaries.
>
> An arrow between source functions does **not** mean that route ran in every recorded trace.

## What the graph covers

It follows selected **verified Stage 1 production relationships**:

```
CLI  →  trace  →  JSON loading  →  validation  →  consumer queries
```

Two nodes are explicitly labeled **external tools** (GCC and GDB). They stand for process boundaries and are **not** made-up Rust functions.

The graph is **deliberately selective**. If a function is missing, check the complete [function index](functions.html). Selecting a node shows its source, purpose, incoming and outgoing edges, each with its classification and supporting references.

## Exploring

- **Filter** by subsystem, or **search** by function or path.
- **Select** a node to expand its neighborhood.
- **Walk** a multi-module path by choosing adjacent nodes.

### Interactive graph

{{explorer}}

## Guided execution paths

The **Guided path** selector offers six source-verified tours:

| # | Tour |
|---|---|
| 1 | CLI → export |
| 2 | GDB stop → checkpoint |
| 3 | Allocator wrapper → heap lifetime |
| 4 | DWARF extraction → type registry |
| 5 | JSON input → checkpoint summary |
| 6 | Deep validation → semantic confidence |

`Previous` and `Next` move one real graph node at a time, and each step names the relationship and its source location. Choose **Free exploration** to return to the full neighborhood view.

## Where the data comes from

- Graph entries live in `assets/call-flow.js` with stable IDs and source references.
- A `path:line` is copyable. Local source files open without assuming your browser supports line anchors.
- **No Stage 2 symbol** is included.
- This graph is a **guided subset**, not a complete dynamic call graph. The DOC-2/DOC-3 chapters give deeper explanations; future reviewed edges can be added without changing the frontend.
<!-- END CHAPTER: content/call-flow.md -->

<!-- BEGIN CHAPTER: content/engine-build.md -->
# 14 · Compilation and compiler diagnostics

> **In one sentence:** Before a debugger can inspect C, GCC has to build a real program. If that fails, the trace records the failure and does **not** pretend a run happened.

## Key terms

| Term | Meaning |
|---|---|
| **Debug build** | An executable with source-to-machine debug information and the project's allocation wrapper linked in. |
| **Sanitizer build** | A separate executable with compiler-added memory checks. It can fail or be skipped independently of the debug build. |

> **Example.** For `return missing_symbol;`, GCC reports a compiler error. The trace keeps that error and has **zero** GDB snapshots, rather than inventing a run that never started.
>
> A successful sanitizer *build* also doesn't mean its separate *execution* ran or found no errors.

## Level 1 · Intuition

Visual C needs a real executable with debug information, not a guess from reading C text. It runs GCC on your sources and keeps warnings and errors as **structured diagnostics**. If compilation fails, the trace says so and does not pretend the debugger ran. The same project can also produce a separate sanitizer build for a later diagnostic run.

{{diagram:compilation}}

## Level 2 · Build lifecycle

### What describes the request

| Config | Holds |
|---|---|
| `BuildConfig` | The C files and compiler options (include directories, extra flags) |
| `TraceConfig` | Everything in `BuildConfig`, plus the program's own arguments, output path, time and byte limits, and optional tools |

### The two builds

| | **Debug variant** | **Sanitizer variant** |
|---|---|---|
| Built by | `compiler::compile_project` | The same function (a *different* build) |
| Includes the allocator wrapper (`c_runtime/alloc_tracker.c`)? | **Yes** | **No** |
| How allocations are logged | Linker switches `-Wl,--wrap=malloc` (and similar for `calloc`, `realloc`, `free`) send calls through a small logging function before the real allocator | Not wrapped |
| Compiler checks | Debug info | **ASan/UBSan** (AddressSanitizer for memory errors, UndefinedBehaviorSanitizer for undefined C behavior) |
| Used for | The primary GDB run | A later, separate diagnostic run |

Other build details:

- The engine detects which **compiler-diagnostic format** GCC supports, and saves each error with a file, line, kind and message.
- Time and output bounds apply while GCC runs.
- **User-supplied flags can change what the compiler emits**, so keep them in mind when comparing runs.

### What happens if the build fails

| Situation | Result |
|---|---|
| GCC rejects the C source | `compiled_successfully=false`, `execution_status=compile_failed`; the compiler's explanation is kept; later debugger stages are marked **skipped**; `snapshots=[]` (there was no program to pause) |
| GCC fails to *start*, or a deadline expires | A **different kind** of failure from GCC rejecting your source |
| Debug build succeeds | The engine can read DWARF type information and start GDB |
| Sanitizer executable builds | Says nothing about whether its separate run found, or ruled out, a crash |

## Level 3 · Files, signatures and example

| File | Provides |
|---|---|
| `src/config.rs::{BuildConfig, TraceConfig, SanitizerTrigger}` | The request |
| `src/compiler.rs::{CompileOptions, compile_project, detect_diagnostics_format}` | Both build variants and error formats |
| `src/trace.rs::trace_program_inner` | Assigns `stage_results.compiler`; short-circuits on failed compilation |
| `src/models.rs::{CompilerDiagnostic, StageResults, StageResult}` | Structured outcomes |
| `src/supervisor.rs` | Subprocess groups and session deadlines |

```c
// Source fixture: fixtures/c/phase8b_defined.c (abridged)
int oracle_global = 41;
static int checkpoint(struct Packet *packet, int *alias, int *heap, int sum) {
    volatile int value = packet->items[1] + *alias + heap[2] + sum + oracle_global;
    return value;
}
```

```sh
cargo build --release
target/release/c-trace-engine fixtures/c/phase8b_defined.c \
  --trace-mode dual --max-steps 350 --timeout 50 --no-pretty \
  --sanitizer-trigger never --output /tmp/visual-c-defined.json
```

**A real failure case.** From the [final integrated audit](../stage1-final-audit.md): a deliberately invalid `return missing_symbol;` produced an engine JSON trace with `execution_status="compile_failed"`, one compiler diagnostic, and **zero** snapshots and events.

For safe output inspection see [practical usage](usage.html#level-2-run-a-recording); for independent analysis stages see [diagnostics](engine-diagnostics.html).

> **Note:** Tool availability is checked **before** compilation. GCC and GDB are required. A missing Valgrind or `addr2line` is reported and qualified, not treated as a successful zero-error analysis. No part of this pipeline is a sandbox for hostile C.
<!-- END CHAPTER: content/engine-build.md -->

<!-- BEGIN CHAPTER: content/engine-debugger.md -->
# 15 · GDB/MI execution and checkpoints

> **In one sentence:** A debugger pause is a *photograph*, not a video of everything between two pauses. The engine must also tell a command reply apart from a real new stop.

## Key terms

| Term | Meaning |
|---|---|
| **Program counter (PC)** | The machine address of the instruction GDB is **about to** execute at a stop. It is not automatically an instruction that already ran. |
| **MI acknowledgement** | GDB saying it *accepted* a command (`^running`). A separate `*stopped` message says the program actually stopped later. |
| **Inferior** | The program GDB is controlling. |

> **Example.** At one stop, GDB reads `sum=10`. That value belongs to *that stop*. If an instruction step never reports another stop, the engine can't claim the attempted instruction completed.

> **Three different numbers:** a **stop counter**, a **snapshot's vector index** and an **attempted-instruction position** are separate. Never swap them.

## Level 1 · Intuition

A stopped program is observable; code running between stops is not automatically known. The GDB/MI2 driver sends commands, waits for each response and for the asynchronous stops, and inspects bounded source state.

- At a stop, the PC names the **next** instruction.
- An instruction attempt is recorded from that *before* address, plus a *next* address **only when observed**.
- Source checkpoints are **selected** stops, not a reconstruction of every intermediate state.

{{diagram:checkpoint}}

## Level 2 · Command/response lifecycle

### What the engine does at a chosen pause

1. GDB starts the debug program and pauses it.
2. The engine notes the **function and source line**.
3. It reads available variables and heap bytes, **within the configured budgets**.
4. It collects new allocator log lines.
5. It assigns the stop a `stop_id` that increases as stops are observed.

### Stepping one instruction

In *dual* source-and-instruction mode, the engine can send `-exec-step-instruction`.

```
engine ──► -exec-step-instruction
GDB    ──► ^running        ← only means "I started the step"
GDB    ──► *stopped        ← NOW we know where execution stopped
```

An `InstructionEvent` stores the address *before* the attempted step, its event number and context, and a next address only when observed.

| Situation | How it's handled |
|---|---|
| The stop never arrives | Reported differently from the **whole session** running out of time |
| A command reply is missing | Late text is **not** handed to a different command |
| Breakpoint hit | Gives the engine a stop, but that alone doesn't prove the attempted instruction finished |
| Stepping skips a library interval | A [TraceGap](model.html#events-gaps-and-checkpoints) marks what was **not** recorded, instead of inventing a number of missing instructions |

### How a step is classified

A step can be recorded as **completed**, **faulted**, **interrupted** or **unknown**.

### What GDB does not cover

- The engine may set bounded breakpoints to catch some callbacks, but a function *appearing in source* is not evidence it was called.
- GDB pauses its selected debugging session. Outside processes and unfollowed fork children are not guaranteed snapshots.
- Observing one thread does not give a complete timeline of all threads.

### Worked example: a stop, then a parser query

At source line 15 of `fixtures/c/phase8b_defined.c`, the audited live capture recorded:

| Item | Value |
|---|---|
| Variable | integer `sum` |
| Displayed value | `"10"` |
| Raw bytes | `"0a000000"` (Linux little-endian) |
| Independent check | Native GDB, in a *separate* run, printed `sum=10` |

A later `TraceStore` can fetch that snapshot by its **vector index**, and `checkpoint_summary()` can page its variables. Neither the stop ID nor the value of `instruction_position` should be used as that vector index. See the [API walkthrough](usage.html#checkpoints-and-paging).

## Level 3 · Source and invariants

| File | Owns |
|---|---|
| `src/gdb_mi.rs` | MI command tokens, async records, stop backlog, frame parsing, selected-thread tracking, memory reads |
| `src/mi_parser.rs` | MI syntax parsing |
| `src/trace.rs` | The stop loop and `push_snapshot` |
| `src/recorder.rs::{checkpoint, event, gap, event_outcome}` | Appends bounded canonical records and journal lines |
| `src/models.rs::{SnapshotObservation, InstructionEvent, TraceGap}` | The serializable identities |
| `src/trace_parser/indexes.rs::OrderingFacts` | Enables binary search **only when order is verified** |

```json
{"step_index":0,"observation":{"stop_id":1,"instruction_position":0,"heap_event_count":0}}
```

This JSON is **illustrative and deliberately partial**. A real `ExecutionSnapshot` has required file, function, call stack and variable fields. In it:

| Field | What it is |
|---|---|
| `stop_id=1` | A debugger **stop counter** |
| `step_index=0` | A **snapshot vector index** |
| `instruction_position=0` | A **count of attempts** before this stop |

See [schema identity domains](schema.html#identity-domains).

> **Warning:** Recording a `checkpoint_id` on an instruction event associates it with the *most recent* checkpoint. It is **not** a claim that the instruction happened precisely at that checkpoint stop, or that a source-line AST expression executed.
<!-- END CHAPTER: content/engine-debugger.md -->

<!-- BEGIN CHAPTER: content/engine-memory.md -->
# 16 · Stack frames, variables and raw memory

> **In one sentence:** A variable can have a displayed value, an address and captured raw bytes. They are related observations that answer *different* questions.

## Key terms

| Term | Meaning |
|---|---|
| **Frame activation** | One particular *call* of a function. Two recursive calls have two activations even if both have a local named `n`. |
| **Captured prefix** | The bytes actually read from the *start* of an object. A missing tail has **not** been filled with zeros. |

> **Example.** For `int *p`, `address` is where the variable `p` itself lives; `points_to` is the address *stored inside* it. Reading `p` successfully doesn't prove the memory at its target is alive or initialized.
>
> The debugger can also read bytes from an **uninitialized** object. Being readable is not an initialization test.

## Level 1 · Intuition

A displayed debugger value is an observation made at one stopped moment, not proof of C-language initialization. Visual C records:

- where a variable lived,
- what GDB could display,
- whether bytes were actually read, and
- which limits prevented a fuller read.

Each function call creates a new frame activation, so two locals named `n` in different recursive calls are **not** the same variable.

{{diagram:stack}}

## Level 2 · At a stopped checkpoint

### Where things live in a snapshot

An `ExecutionSnapshot` is one saved pause.

| Part of the snapshot | Holds |
|---|---|
| `variables` | Variables of the currently selected function call, called **frame 0** |
| `caller_frames` | *(If configured)* bounded information about the functions that called it |
| `snapshot.observation.globals` | Globals and file-static variables |
| `call_stack` | Frames with their **GDB levels** |

> The position of an item inside `caller_frames` is a **different index** from a GDB level. Don't swap them.

Each observed call has an **activation ID**, so `recurse(3).value` and `recurse(2).value` are never merged just because both locals are named `value`.

- GDB's Python support can help find exact lexical blocks and read a shadowed variable.
- If those details are unavailable, the engine labels the **weaker fallback** rather than pretending it has the same identity confidence.
- A variable being *in scope* does not prove its initializer has run.

### The two addresses of a pointer

For `int *p`:

| Field | Meaning |
|---|---|
| `VariableInfo.address` | Where **the pointer variable `p` itself** is stored |
| `points_to` | The address value *inside* `p` |
| `address_match_allocation_id=7` | A *possible* numeric match, if the target lies in heap block 7. It does **not** say `p` originally came from block 7. Addresses may be reused. |

### Separate memory questions, separate fields

| Field | Answers |
|---|---|
| `read_status` | What GDB's **printed value** looked like |
| `bytes_status` | Whether the **raw byte read** succeeded |
| `memory_evidence` | How many bytes were **requested** and how many **captured** |
| `initialization_status` | Whether it was initialized. Separate, and **often unknown** |

If only the first four of eight bytes were captured, `raw_bytes_hex` contains that four-byte prefix. The other four have **not** been read or zero-filled.

A requested per-instruction memory region belongs only to its **actual instruction stop**. Bytes saved at an older checkpoint are historical elsewhere.

### Worked call

```c
// Illustrative C; see fixtures/c/recursive.c for the real tested fixture.
int recurse(int value) {
    if (value == 0) return 1;
    return value * recurse(value - 1);
}
```

When `recurse(3)` calls `recurse(2)`, the second frame is a distinct activation.

- The audited engine trace with `limits.max_caller_frames=3` captured **45 caller-frame entries across 26 snapshots**. That is not an unlimited call history.
- Each checkpoint is a **historical observation**: later returns or frees must not change earlier captures.
- The [TP-7 frame activation](parser-consumer.html#level-3-real-signatures-and-bounded-traversal) query resolves only **within one retained snapshot**.

## Level 3 · Where to look

| File | Role |
|---|---|
| `src/gdb_mi.rs` | Extracts `StackFrameInfo`, `RawVariable` and bounded GDB reads |
| `src/trace.rs::push_snapshot` | Collects checkpoint state, heap blocks and caller frames |
| `src/memory.rs` | Enriches byte and target observations |
| `src/recorder.rs::set_validity` | Keeps display status and byte status distinct |
| `src/dwarf.rs::associate` | Correlates **unique** declarations without replacing GDB values |
| `src/models.rs::{ExecutionSnapshot, SnapshotObservation, VariableInfo, MemoryEvidence, FrameVariables, ScopeStatus, DataValidity}` | The schema |
| `src/trace_parser/semantic.rs::MemoryEvidenceView` | Qualifies captured byte ranges without decoding C semantics |

```json
{"name":"sum","value":"10","raw_bytes_hex":"0a000000","read_status":"valid","bytes_status":"valid"}
```

This is an **abridged excerpt** of the audited `phase8b_defined.c:15` variable, not a stand-alone `VariableInfo` document. Four readable bytes show a captured representation of 10, **not** proof of prior initialization. See the [data model](model.html#variables-bytes-and-heap) and [confidence](parser-semantic.html).

> **Note:** Caller capture defaults to `max_caller_frames=0`. A missing caller vector is **not** evidence the program had no caller. Per-snapshot and per-variable read budgets may truncate capture without invalidating the other observations.
<!-- END CHAPTER: content/engine-memory.md -->

<!-- BEGIN CHAPTER: content/engine-metadata.md -->
# 17 · DWARF, Clang AST and evidence fusion

> **In one sentence:** Type information comes from the built program and optional syntax information from the source. Neither replaces reading runtime memory.

## Key terms

| Term | Meaning |
|---|---|
| **DWARF** | Compiler-written debug info describing compiled types, declarations and source locations. |
| **AST** | Clang's tree of *written* C syntax. It can show a call appears in the source without showing the program took that branch. |
| **Fusion** | One cautious pass that links existing records from different providers. It never supplies new values or invents writes. |

> **Example.** Three tools answer three different questions:
>
> | Tool | Question it answers |
> |---|---|
> | DWARF | Where is `struct Pair.y` located inside the compiled struct? |
> | GDB | What bytes were there at this stop? |
> | Clang | Is there a written `malloc` call in the source? |
>
> A **same-line** source match is only a *candidate*, especially if macros or several calls share that line.

## Level 1 · Intuition

Static metadata helps explain what a stopped value *might be*: DWARF identifies the compiler's types and declarations, and optional Clang AST identifies syntax and source spans. GDB supplies the dynamic runtime values. **None** of the static sources proves a C expression executed.

{{diagram:metadata}}

## Level 2 · The three evidence streams

### Stream 1: DWARF

| Step | What happens |
|---|---|
| `dwarf::extract_excluding` | Reads debug info from the **program GCC actually built**, leaves out the injected allocator logger, and saves bounded lists of compiled types and declarations in `TypeRegistry` |
| `dwarf::associate` *(later)* | Links a stopped GDB variable to a type or declaration **only when it finds a unique match**; keeps the observed size and address exactly as GDB reported them |

### Stream 2: Clang AST (optional, `--clang-ast`)

- Clang separately reads the C source into `SourceMetadataRegistry`.
- A missing Clang tool or partly parsed source is marked **missing or partial**, not mistaken for an empty program.
- GCC and Clang can use different compilation settings, so a link between their descriptions isn't always justified.

### Stream 3: Fusion

After the engine finishes capturing runtime and reconstructing the heap, `evidence::fuse` makes **one** cautious linking pass:

| File | Handles |
|---|---|
| `declarations.rs` | Compiled declarations and members |
| `source_links.rs` | Links to observed variables |
| `heap.rs` | Marks *possible* source locations for allocator calls |
| `validation.rs` | Removes links that can't be supported |

The summary counts links kept and rejected.

| Fusion **does** | Fusion **does not** |
|---|---|
| Link existing records, cautiously | Ask GDB for new values |
| Count kept and rejected links | Copy the type registry for every pause |
| | Turn a possible source call into an executed call |

The parser later stores **positions** for these metadata IDs instead of cloning their records.

### Worked ambiguity

Two heap-call expressions on one source line, or a macro expanding to one, can share a line with a recorded allocation event. The relationship can be a **candidate** or **ambiguous**. A same-line match is never a dynamic expression-to-event identity.

So TP-6 `source_expression(id).execution_confidence` stays `Unavailable` even when `heap_call_candidates` exist. This matters when writing [future Stage 2 explanations](handoff.html#level-2-the-freeze-contract).

## Level 3 · Files and API

| File / function | Role |
|---|---|
| `src/dwarf.rs::{TypeRegistry, extract_excluding, associate}` | Reads ELF/DWARF, matches unique declarations, reports missing or ambiguous data |
| `src/clang_ast.rs::{SourceMetadataRegistry, analyze, correlate_heap_calls}` | Produces optional static syntax |
| `src/evidence/mod.rs::fuse` and `src/evidence/{declarations,source_links,heap,validation}.rs` | Bounded reconciliation |
| `src/models.rs::EvidenceFusionSummary` | Serializes the counts |
| `TraceStore::type_entry(id)`, `dwarf_declaration(id)`, `source_declaration(id)`, `source_expression(id)` | Return borrowed, indexed metadata |
| `ValidatedSemanticView::source_expression(id)` | Adds confidence plus indexed candidate/ambiguity event positions |

```rust
// Real TP-6 API; expression IDs belong to this trace, not to other runs.
let expression = validated.source_expression(expression_id);
let syntax = expression.expression;                  // Option<&SourceExpression>
let execution = expression.execution_confidence;     // Unavailable on sound traces
for candidate in expression.call_sites() {
    let _ = (candidate.event_position, candidate.event);
}
```

The snippet assumes `validated` and `expression_id` are already defined. Run the complete [parser walkthrough](usage.html#level-3-load-and-validate) first. Signatures are in the [public API reference](api.html#level-3-signatures-and-contracts).

> **Warning:** "source line observed" is **not** "specific AST expression executed." Never use an AST call candidate to grant pointer provenance or to allocate a new runtime lifetime.
<!-- END CHAPTER: content/engine-metadata.md -->

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

<!-- BEGIN CHAPTER: content/engine-entry-deep.md -->
# 22 · Engine entry, configuration, and errors

> **In one sentence:** The front door. Command-line arguments become a tracing request (`TraceConfig`), and errors become meaningful exit results.

## Key terms

| Term | Meaning |
|---|---|
| **CLI** | Command-line interface: the flags and file names you pass when starting `c-trace-engine`. |
| **Configuration precedence** | The rule that decides who wins when a CLI flag and a config-file value set the same thing. |

> **Example.** You pass a C file and `--max-steps 50`. `main` converts them into a `TraceConfig`, and the tracing code receives those *values* instead of re-reading command-line text.
>
> **A bad flag** is an input error. **A compiler diagnostic** is a recorded C build result. They are not the same failure.

## The big picture

The executable is a **thin adapter** over the library. It:

1. parses flags,
2. builds a `TraceConfig`,
3. awaits `trace::trace_program`, then
4. exports the returned `DebugSessionTrace`.

| Outcome | What you get |
|---|---|
| A compilation **rejection** | A *trace result* (with diagnostics) |
| A configuration, tool or export **failure** | An error, possibly with **no** usable trace |

Next boundaries: [compilation](engine-compiler-deep.html), [process ownership](engine-supervision-deep.html), [export](engine-export-deep.html).

## File 1 · `src/main.rs`: from CLI to exit status

**Principal function:** `async fn main() -> ExitCode` (`src/main.rs:22-80`)

- `Cli::parse()` belongs to Clap (the argument-parsing library). `to_config()` is the first engine-specific step that can fail.
- The CLI owns the effective config, clones it into the tracing call, then *borrows* the resulting trace for a synchronous JSON export.
- After tracing, it installs an independent **SIGINT watcher** and passes its `AtomicBool` to `export_json_cancellable`. It aborts that watcher once export returns (`src/main.rs:34-59`).
- This export watcher is **distinct** from the supervised collection-time SIGINT/SIGTERM token.

```rust
let config = cli.to_config()?; // schematic: main prints the error and returns exit 1
let trace = trace::trace_program(config.clone()).await?;
let output = json_export::export_json_cancellable(
    &trace, config.pretty, config.limits.max_json_export_bytes,
    config.output_file.as_deref(), &mut std::io::stdout().lock(), Some(&cancelled),
);
```

This shortens the actual `match` arms at `src/main.rs:26-79`. `main` does not itself return `Result`.

### Exit codes

| Situation | Exit code |
|---|:---:|
| Bad config, operational tracing error, failed export, or inability to install the export signal watcher (message on stderr) | **1** |
| Export succeeded **and** `execution_status == CompileFailed` (a diagnostic JSON trace is still emitted) | **2** |
| Any other successfully exported trace, **including crashes and timeouts** | **0** |

(`src/main.rs:59-78`.) So a shell exit of 0 does **not** certify complete execution.

There's no `main.rs` unit test. The [final audit](../stage1-final-audit.md) independently exercised compile-failure exit 2 and export-cap exit 1.

## File 2 · `src/lib.rs`: the public crate boundary

| Item | Detail |
|---|---|
| **Principal exports** | `pub use config::{BuildConfig, TraceConfig};` `pub use error::{EngineError, Result};` `pub use models::DebugSessionTrace;` `pub use resource_limits::ExecutionLimits;` (`src/lib.rs:49-52`) |
| **Public modules** | `cli`, `compiler`, `supervisor`, `processor`, `storage`, `json_export`, `trace` (`src/lib.rs:25-47`) |
| **Ownership** | The library owns **no global trace instance**: callers supply a `TraceConfig` and own the returned trace |
| **Embedding** | You can call `trace_program` without Clap or `main`'s stdout/exit-code policy |
| **Tests** | None in `lib.rs`; behavior is tested in the modules and integration suite |

The module map at `src/lib.rs:8-23` is orientation, not an execution guarantee.

## File 3 · `src/cli.rs`: flag precedence and source validation

**Principal items:**

- `#[derive(Parser, Debug)] pub struct Cli` (`src/cli.rs:21-192`)
- `pub fn to_config(&self) -> Result<TraceConfig>` (`src/cli.rs:194-383`)
- `fn file_build(file: &ConfigFile, source: &Path) -> BuildConfig` (`src/cli.rs:385-399`)

### How `to_config` decides things

| Step | Rule |
|---|---|
| 1. Config file | Reads and deserializes the optional `--config` first. Unreadable or invalid JSON is `EngineError::Config`. |
| 2. Primary source | Chosen in this order: `--source`, then the positional source, then the JSON file's `source`. (Clap forbids giving both CLI forms together.) |
| 3. Lists | Copies the file's build lists, then **appends** repeated CLI `--add-source`, `--define`, `--library`, etc. |
| 4. Scalar settings | Replaced **only when the matching CLI option is present** (`src/cli.rs:199-240`). |
| 5. Source check | The primary source is checked to be a file, relative to `build.working_directory` when set. This does **not** validate every auxiliary input or canonicalize the stored path (`src/cli.rs:242-256`). |

```rust
let source = self.source.clone()
    .or_else(|| self.source_positional.clone())
    .or_else(|| file.source.clone())
    .ok_or_else(|| EngineError::Config("no source file given ...".into()))?;
let mut build = file_build(&file, &source);
build.defines.extend(self.define.iter().cloned());
let mut program_args = file.program_args.clone();
program_args.extend(self.arg.iter().cloned());
program_args.extend(self.program_args.iter().cloned());
```

This abbreviated walkthrough follows `src/cli.rs:211-261`. The real missing-source message also names the supported CLI forms.

### Details worth knowing

- Arguments after `--` and repeated `--arg` go to the **inferior** (your C program), *not* to GCC.
- Limits start from `file.limits.unwrap_or_default()`, and each CLI scalar that is present overrides one field (`src/cli.rs:263-293,330-359`).
- `--no-pretty`, `--keep-temp` and `--clang-ast` are **one-way switches**. When unset, the file or default value stays in force (`src/cli.rs:295-301,378-380`).
- `--journal-only` turns off retained instruction events, and Clap requires `--journal` with it. `trace_program_inner` also refuses journal-only configuration without a path for non-CLI callers (`src/cli.rs:154-157,306-312`; `src/trace.rs:172-176`).
- An invalid `--memory-region` syntax or byte count gives a `Config` error **before tracing** (`src/cli.rs:319-329`).
- **Tests:** `positional_source`, `missing_source_is_config_error`, `flag_accumulation_and_program_args`, `limit_flags_override_defaults`, `config_file_merge_precedence` (`src/cli.rs:415-550`).

## File 4 · `src/config.rs`: request types and defaults

| Type / function | Location | What it is |
|---|---|---|
| `BuildConfig` | `src/config.rs:20-45` | Project inputs and linking options |
| `user_sources(&self) -> Vec<PathBuf>` | `src/config.rs:47-56` | Collects primary plus additional sources, dropping empty paths |
| `TraceConfig` + `impl Default` | `src/config.rs:58-112` | Adds inferior args/cwd, limits, output destination, tool overrides, optional AST, tracing policy |
| `ConfigFile` | `src/config.rs:114-144` | A **different**, flat JSON-file schema with optional scalars and `deny_unknown_fields` |
| `SanitizerTrigger` | `src/config.rs:154-174` | Defaults to `OnCrashOrValgrindError` |
| `TracingConfig` + `Default` | `src/config.rs:176-221` | Defaults: `Dual` mode, static disassembly on, global discovery on, resident instruction events retained |

Things to watch for:

> **Don't paste a serialized `TraceConfig` into `--config`.** `ConfigFile` has different keys.

- `BuildConfig` deliberately places **user compiler flags after the engine defaults**, and that ordering can change observation fidelity.
- These are owned, cloned configuration values, not global mutable knobs.
- There are no tests in `config.rs`; CLI merge tests and `ExecutionLimits` validation cover adjacent behavior.
- A comment on `TraceConfig::output_file` mentions `stdout_json`, but **no such field exists**. The current CLI writes stdout on a successful export (`src/config.rs:70-75`; `src/main.rs:50-57`).

## File 5 · `src/error.rs`: the failure vocabulary

**Principal type and alias:** `pub enum EngineError` and `pub type Result<T> = std::result::Result<T, EngineError>` (`src/error.rs:10-59`).

| Variant | Meaning |
|---|---|
| `Config` | Bad configuration |
| `MissingTool` | A required tool isn't available |
| `Deadline { operation }` | A deadline expired |
| `Cancelled` | The session was cancelled |
| `LimitReached` | A configured limit was hit |
| `Io`, `Json` | I/O and JSON failures (converted via `#[from]`) |
| *others* | GDB/MI, allocation-event, Valgrind and resource-limit stage errors |

`#[from]` converts I/O, JSON and MI-parse errors rather than flattening everything into strings. The **caller** decides whether an error is returned directly or recorded in a partial trace:

- A GCC source error produces `CompilationOutput { success: false, diagnostics }`, then `CompileFailed`.
- `probe_tool` can return `MissingTool`, and the CLI exits 1 (`src/compiler.rs:99-128,518-527`; `src/trace.rs:336-350`).

`error.rs` has no local tests; CLI, compiler and supervisor tests exercise the error branches. A bare `EngineError` never proves that a serialized trace was produced.

For the wire vocabulary, continue with [model and exporter internals](engine-export-deep.html). For limits governing the request, see [supervision](engine-supervision-deep.html).

## Diagram notes

| Diagram | What it shows |
|---|---|
| **The CLI boundary**<br>{{diagram:d2-cli}} | `main` owns process exit policy; `trace_program` owns the bounded session; export is a separate fallible operation. |
| **Configuration precedence**<br>{{diagram:d2-config}} | Defaults, JSON-file settings and explicit CLI values meet in `Cli::to_config`; later stages consume the owned effective configuration. |
| **Session control flow**<br>{{diagram:d2-session}} | Compilation, static metadata, the GDB stop loop, optional diagnostics and trace assembly are distinct phases under one deadline. |
| **Failure propagation**<br>{{diagram:d2-error}} | A compile failure can produce a trace; an operational error or export-cap failure need not produce a usable JSON document. |
<!-- END CHAPTER: content/engine-entry-deep.md -->

<!-- BEGIN CHAPTER: content/engine-compiler-deep.md -->
# 23 · Compiler invocation and diagnostic evidence

> **In one sentence:** The code-level story of how GCC commands are assembled, started and interpreted.

## Key terms

| Term | Meaning |
|---|---|
| **Argv** | The exact list of arguments given to an OS process. Reading it is safer than imagining what shell command ran. |
| **Diagnostic fallback** | If GCC can't produce one structured warning format, the engine tries another supported one instead of silently losing errors. |

> **Example.** The debug GCC command includes the wrapper source and the `--wrap` linker switches. The separate sanitizer command uses different flags. A warning from either build has a file, line and message when GCC supplies them.
>
> A compiler subprocess **timing out** is not the same event as GCC **finishing and rejecting** invalid C.

## The big picture

The engine builds an **instrumented debug executable** before any debugger run, then *attempts* a separate sanitized executable.

- Compilation **success** is a prerequisite for runtime evidence.
- Sanitizer **build** success is **not** proof that a sanitizer later found or reproduced a defect.

The request is assembled in [entry/configuration](engine-entry-deep.html), and subprocesses are owned by [supervision](engine-supervision-deep.html).

## Step 1 · Options and tool probes (`src/compiler.rs`)

**`CompileOptions`** holds `{ sanitized, sanitizer_variant, include_alloc_tracker, tracker_source, output_path }`, with three constructors (`src/compiler.rs:28-73`):

| Constructor | Use |
|---|---|
| `debug(output_path, tracker_source)` | The debug build |
| `sanitized(output_path)` | A sanitizer build |
| `sanitized_with_variant(output_path, variant)` | A specific sanitizer variant |

**`CompilationOutput`** retains: success, native exit code, binary path, parsed diagnostics, raw stderr, the selected format, and an optional sanitizer variant (`src/compiler.rs:75-97`).

The orchestrator creates the tracker source and passes it to `compile_project`. The debug build wraps `malloc`/`calloc`/`realloc`/`free`; sanitized builds **omit** the tracker (`src/trace.rs:226-237`; `src/compiler.rs:543-566`).

### Probing tools

| Function | Location | Behavior |
|---|---|---|
| `probe_tool(tool, version_arg) -> Result<String>` | `src/compiler.rs:99-128` | Requires a successful status; returns the first stdout line; turns most spawn/probe failures into `MissingTool`, preserving cancellation |
| `detect_diagnostics_format(cc) -> DiagnosticsFormat` | `src/compiler.rs:130-158` | Tries `-fdiagnostics-format=json`, then `sarif-stderr`, then falls back to `Text` |

Both launch processes through `supervisor::output` with a **three-second** stage deadline and **1 MiB per-pipe** capture.

Format detection tests candidate commands and recognizes an unsupported-argument message, rather than comparing a hard-coded GCC version. A probe that returns output *without* that message can still select the attempted format, because the later parse has a text fallback.

`compile_project` itself probes `gcc --version`. `trace_program_inner` also probes required GCC and GDB **before** creating the session directory (`src/compiler.rs:535-537`; `src/trace.rs:203-229`).

## Step 2 · Assembling the exact argv

**Signature:** `fn build_argv(build: &BuildConfig, opts: &CompileOptions, diag_flag: Option<&str>) -> Vec<String>` (`src/compiler.rs:392-452`).

Every argument is passed **individually** to `tokio::process::Command`, never interpolated into a shell. The order matters:

```text
gcc -g -O0 -fno-omit-frame-pointer [-fsanitize=address,undefined]
    [-fdiagnostics-format=json | -fdiagnostics-format=sarif-stderr]
    [-Wl,--wrap=malloc -Wl,--wrap=calloc -Wl,--wrap=realloc -Wl,--wrap=free]
    [-D...] [-I...] [compiler_flags...]
    source_file [additional_sources...] [tracker_source] [object_files...]
    [-L...] [linker_flags...] [-l...] -o output_path
```

Brackets mark conditional or vector segments, not literal shell syntax.

| Fact | Detail |
|---|---|
| Sanitizer vs wrapper | Mutually exclusive in the standard constructors |
| User compiler flags | Come **after** `-g -O0 -fno-omit-frame-pointer`, so they **can override defaults** |
| Linking options | Come after the inputs (`src/compiler.rs:396-450`) |
| `program_args` | **Absent**, since they're for the inferior |
| Working directory | `compile` sets `current_dir` only when `build.working_directory` is present (`src/compiler.rs:471-475`) |
| Validation | Only checks the primary source path is nonempty; GCC handles unresolved extra inputs and linkage |
| Output paths | Debug: `<session_dir>/<output_name or program>_debug`; sanitized: `_asan` (`src/compiler.rs:539-565`) |

## Step 3 · Running GCC and handling failures

**Signature:** `pub async fn compile(build, cc, opts, format) -> Result<CompilationOutput>` (`src/compiler.rs:454-527`).

It runs the command via `supervisor::output(&mut cmd, supervisor::deadline(), 8 * 1024 * 1024)`.

| Situation | Result |
|---|---|
| Timeout or cancellation | An **engine error**, not a fabricated GCC diagnostic |
| Nonzero GCC status | Normally `Ok(CompilationOutput { success: false, ... })`; exit code `-1` if no numeric code exists |
| Success | Requires **both** a successful process status **and** no parsed `error`, `linker-error` or `fatal error` diagnostics (`src/compiler.rs:478-526`) |
| Output beyond the capture cap | The output carries `output_truncated`, but `compile` does **not** propagate that flag into `CompilationOutput`. So diagnostics or stderr beyond the cap can't be claimed complete |

`trace_program_inner` separately clamps the **whole build future** to `compiler_timeout_seconds` within the global deadline (`src/trace.rs:230-238`). Process cleanup can briefly outlast a deadline.

## Step 4 · The sanitizer fallback builds

```rust
let debug_result = compile(build, "gcc", &debug_opts, format).await?;
if !debug_result.success {
    return Ok((debug_result, not_built()));
}
for variant in ["address,undefined", "address", "undefined"] {
    let opts = CompileOptions::sanitized_with_variant(asan_out.clone(), variant);
    let result = compile(build, "gcc", &opts, format).await;
    // Schematic: actual loop breaks on a successful variant or an engine error.
}
```

This follows `pub async fn compile_project(build, session_dir, tracker_source) -> Result<(CompilationOutput, CompilationOutput)>` (`src/compiler.rs:529-581`).

| Behavior | Detail |
|---|---|
| Variant order | Combined ASan/UBSan, then ASan-only, then UBSan-only |
| A failed variant's output | Does **not** become the returned sanitizer result |
| If none succeeds | The returned sentinel is `not_built()` (with an error string in `raw_stderr` only when a compile call returned `Err`) |
| Debug build vs sanitizer failure | The successful debug build is **retained** even if optional sanitizer compilation fails |
| If the debug build failed | `trace_program_inner` records its diagnostics and stops without snapshots (`src/trace.rs:291-350`); optional sanitizer unavailability is marked separately |

Sanitizer runs, when triggered, are **separate executions**, not observations of the GDB process. See [diagnostics](engine-diagnostics.html).

## Step 5 · Normalizing diagnostic formats

| Parser | Location | Handles |
|---|---|---|
| `parse_legacy_gcc_json(stderr)` | `src/compiler.rs:232-259` | Legacy GCC JSON: first location/caret, flattens child notes |
| `parse_sarif(stderr)` | `src/compiler.rs:317-366` | SARIF: first physical location, maps camel-case region fields |
| `text_diagnostics(stderr)` | `src/compiler.rs:368-390` | Plain-text fallback |

Rules shared by all three:

- Legacy JSON and SARIF each support **consecutive documents** from multiple translation units.
- A **missing location** becomes `<unknown>`/zero, never a guessed source position.
- If structured parsing fails, it falls back to a **single text diagnostic** (up to 4,000 characters). Known linker phrases map to `linker-error`; any other nonempty stderr maps to `error`.
- The normalized payload is `CompilerDiagnostic { file, line, column, kind, message }` (`src/models.rs:50-59`).

**Tests:** `gcc_legacy_caret_locations_and_streams_are_preserved` and `gcc_sarif_stream_and_camel_case_region` (`src/compiler.rs:595-611`). The [final audit](../stage1-final-audit.md) independently confirms a live compilation failure produced diagnostics without debugger events.

> **Reminder:** No compiler output or tool version is evidence that a source expression executed. Runtime and static metadata have separate provenance (see [schema](schema.html)).

## Diagram notes

| Diagram | What it shows |
|---|---|
| **Compilation lifecycle**<br>{{diagram:d2-compile}} | GCC invocation, bounded output capture, diagnostic normalization and optional sanitizer variants. |
| **Build artifacts**<br>{{diagram:d2-build-artifacts}} | The wrapped debug executable is the **only** binary GDB executes; sanitizer artifacts and Clang syntax are separate. |
<!-- END CHAPTER: content/engine-compiler-deep.md -->

<!-- BEGIN CHAPTER: content/engine-gdb-deep.md -->
# 24 · GDB/MI2 controller and the recorded stop loop

> **In one sentence:** The machinery that sends GDB commands and waits for replies while the program runs and pauses.

## Key terms

| Term | Meaning |
|---|---|
| **Token** | A number attached to a debugger command so its reply can be routed back to the command that asked. |
| **Asynchronous stop** | A separate GDB message announcing the program paused, possibly after a command was already acknowledged. |

> **Example.** GDB may answer `4^running` to command 4. That only means it *started* the requested continuation. A later `*stopped` tells us where the program actually paused.
>
> A **missing reply** must never let late output answer the next command. A **missing stop** is a different timeout condition.

## The big picture

The debugger subsystem does two separate jobs:

1. **Own one GDB process** and turn its MI2 output into typed debugger facts.
2. **Run the stop loop** that decides what to retain.

A command acknowledgement is *never* treated as a `*stopped` event.

| Piece | File |
|---|---|
| Orchestration | `src/trace.rs` |
| Process, token and PTY machinery | `src/gdb_mi.rs` |
| Syntax decoding | `src/mi_parser.rs` |

## Part 1 · Orchestration around GDB (`src/trace.rs`)

**Principal signatures:**

- `pub(crate) async fn trace_program_inner(config: TraceConfig) -> Result<DebugSessionTrace>` (`src/trace.rs:167`)
- `pub async fn run_gdb_phase(config, limits, classifier, binary, event_file, event_status_file, reader, timeline, notes, type_metadata) -> Result<GdbPhaseResult>` (`src/trace.rs:1113-1124`)

### Who does what

| Function | Does |
|---|---|
| `trace_program_inner` | Probes tools, writes the C tracker source, compiles, optionally extracts static metadata, then calls `run_gdb_phase` with the event/status paths (`src/trace.rs:203-439`) |
| `run_gdb_phase` | Creates the `Recorder`, clones the `UserCodeClassifier`, records the injected allocator source as `instrumentation_file`, optionally creates a cgroup, builds `GdbLaunchOptions` (`src/trace.rs:1125-1189`) |

### Startup failures

`GdbSession::launch` is called at `src/trace.rs:1191-1212`.

| Failure | Becomes |
|---|---|
| Launch failure after the wall deadline expired | `Timeout` |
| Launch failure otherwise | `DebuggerFailure` |
| An output limit | `OutputLimitReached` |
| Missing `main` breakpoint | A **recorded limitation**, not a fabricated source checkpoint (`src/trace.rs:1226-1249`) |

### The stop loop

The `loop` starts at `src/trace.rs:1371`. Each iteration follows a **strict evidence order**:

| # | Action | Reference |
|---|---|---|
| 1 | Increment `stop_id`; check command and terminal conditions | |
| 2 | Drain `EventFileReader::read_new` | `src/trace.rs:1387-1405` |
| 3 | Mark transport damage | |
| 4 | Ingest rows into `HeapTimeline` | |
| 5 | Journal newly observed events with `Recorder::allocations(..., stop_id)` | |
| 6 | **Only then** classify an exit, signal, user-code stop or non-user frame | |

Why heap events come first: allocation events logged before this stop must be known **before** the snapshot is taken.

| Case | Handling |
|---|---|
| Fatal signals | Capture state and a crash report (`src/trace.rs:1441-1518`) |
| Resource vs benign signals | Distinguished by `classify_signal` (`src/trace.rs:2031-2063`) |
| Nonterminal limits | Break to cleanup and a gap |
| Normal exit / crash | Separate terminal branches (`src/trace.rs:1954-2024`) |

After termination, a **final `read_new`** preserves destructor and exit-handler events (`src/trace.rs:1977-1991`). Then `finish_phase`, debugger counters, profiling categories and `recorder.finish()` produce `GdbPhaseResult` (`src/trace.rs:1991-2024`). If multiple threads were observed, the recorder is truncated to **selected-thread-only** coverage.

## Part 2 · Session types and launch (`src/gdb_mi.rs`)

### Principal types

| Type | Location | Owns |
|---|---|---|
| `GdbSession` | `:270-318` | The child process, stdin, shared routing state, async receiver, activation map, caches |
| `Shared` | `:117-138` | Pending oneshot waiters, stream accumulation, program buffer, EOF flag, module epoch |
| `StoppedInfo` | `:139-164` | The typed asynchronous stop |
| `GdbLaunchOptions` | `:165-178` | Binary, cwd, args, env, limits |
| `MiCommandResponse` | `:179-221` | The tokened record plus stream text |
| `CheckpointReads` | `:222-269` | The per-stop byte-read reuse map |

### Launching GDB

**Signature:** `pub async fn launch(opts: &GdbLaunchOptions) -> Result<GdbSession>` (`src/gdb_mi.rs:389-512`).

- Refuses cancelled or expired sessions.
- Creates a **PTY** (a pseudo-terminal, so the inferior's output can be captured) and enables subreaper ownership.
- Starts `gdb --nx --quiet --interpreter=mi2 <binary>` with stdin/stdout piped and stderr null (`:396-423`).
- GDB gets its **own process group** and `kill_on_drop(true)`. No inferior rlimits are installed on GDB itself (`:411-416`).
- The reader task and PTY task are spawned **before** session configuration (`:446-450`).

### Configuration commands sent at launch (`:483-505`)

| Command | Why |
|---|---|
| `-gdb-set pagination off`, `confirm off` | No interactive prompts |
| `debuginfod enabled off` | No network symbol fetching |
| `mi-async on`, `non-stop off` | Asynchronous stops, all threads stop together |
| `style enabled off` | Plain output |
| `may-call-functions off` | GDB won't call functions in the inferior |
| `auto-load off` | No auto-loaded scripts |
| Set the inferior TTY; optionally pass program arguments | |

A configuration failure **kills** the session rather than returning a half-configured debugger. The inferior inherits `C_TRACE_EVENT_FILE` and `C_TRACE_EVENT_STATUS_FILE` from `run_gdb_phase` (`src/trace.rs:1176-1187`).

## Part 3 · Command tokens and asynchronous stops

### `execute`

`pub async fn execute(&mut self, command: &str) -> Result<MiCommandResponse>` (`src/gdb_mi.rs:533-621`)

1. Checks cancellation, poison, deadline and command cap.
2. Allocates `next_token`.
3. Registers a oneshot in `Shared.pending`.
4. Clears stale stream text.
5. Writes `"{token}{command}\n"`.
6. Waits **only** for that token's result.

> A response **timeout sets `poisoned = true`**, because a late stream can't safely be matched to a later command (`:600-608`).

### `wait_stopped`

`pub async fn wait_stopped(&mut self, timeout: Duration) -> Result<StoppedInfo>` (`:624-671`)

- Loops over the asynchronous channel and returns only `MiRecord::Exec { class: "stopped" }`.
- Records `selected_thread`, handles notifications such as `gdb-eof`, and buffers unrelated async records up to **4,096** (`:624-670`).
- `StoppedInfo::is_exit`, `is_signal`, `value` and `error_message` classify the record (`:139-191`).

### Reader and routing

| Component | Behavior | Location |
|---|---|---|
| `reader_task` | 16 MiB line bound; EOF fails every pending command and emits `gdb-eof` | `src/gdb_mi.rs:2302-2338` |
| `handle_line` | Ignores blank lines and GDB prompts; sends malformed lines to the bounded console log | `:2340-2372` |
| `route_record` | Sends tokened `^` results to the matching waiter; routes async records to the async channel; tracks module/thread/inferior notifications; treats console/log stream text as diagnostics | `:2374-2445` |
| PTY reader | Owns the master fd, appends inferior output to `ProgramBuffer`, notifies waiters when the output cap is first exceeded | `:2447-2484` |

> An asynchronous token is **not** a command acknowledgement, even if the numbers happen to match.

## Part 4 · Frame and variable capture

### Types

| Type | Location | Role |
|---|---|---|
| `UserCodeClassifier` | `src/gdb_mi.rs:1550-1559` | Distinguishes instrumentation runtime, user code, system library and unknown frames |
| `CaptureOptions` | `:1698-1717` | Variable/read/aggregate budgets, optional DWARF `ObservationTypes`, caller-frame limits, the transient-stop optimization |
| `CapturedState` | `:1740-1749` | Owns frames, variables, caller frames, memory usage and errors |

### `capture_state` (`src/gdb_mi.rs:1840-2227`)

`pub async fn capture_state(session, opts, reads) -> Result<CapturedState>`

| Step | What it does | Reference |
|---|---|---|
| 1 | Handles the derived-chain transient stop | `:1847-1862` |
| 2 | Otherwise runs `-stack-list-frames 0 200`, parses bounded frames, computes an **activation path** oldest-to-youngest | `:1864-1910` |
| 3 | A truncated chain **clears** activation continuity instead of reusing sliding prefixes | `:1889-1895` |
| 4 | Non-user frames return after frame/activation bookkeeping | `:1911-1913` |

**For user frames**, it then:

- executes `-stack-list-variables --all-values`,
- computes **duplicate-name counts** over the full list, and probes lexical symbols for the selected cap (`:1914-1947`),
- keeps duplicate lexical declarations as **separate variables** with activation/scoped identities. An unresolved collision becomes `unresolved:...`, not a guessed value (`:1981-2051`),
- otherwise uses fresh probe address/size/target data, falling back to `&name`, `sizeof(name)` or console `whatis` (`:2054-2127`).

**Variable identity** is `static:<scope>:<name>` or `activation:<id>:<scope>:<name>` (`:2129-2171`).

`prepare_variable_memory` preserves GDB's runtime extent and picks aggregate vs scalar read limits (`:1719-1736`). The actual bytes are read by `recorder::capture_variable_memory` (`src/recorder.rs:45-185`).

### Caller frames

`capture_caller` (`src/gdb_mi.rs:1753-1834`) never changes GDB's selected frame or thread, records per-caller errors, and assigns separate activation IDs. `capture_or_empty` in `trace.rs` keeps a stop even if capture fails (`src/trace.rs:2164-2175`).

## Part 5 · Memory, globals, shutdown and failure

| Function | Behavior | Location |
|---|---|---|
| `evaluate` | Runs `-data-evaluate-expression` | `src/gdb_mi.rs:1104-1118` |
| `read_memory_bytes_partial` | Validates address/range, returns empty for size zero, runs `-data-read-memory-bytes`, parses a **contiguous prefix** with an error | `:1136-1179` |
| `parse_memory_read` | Combines contiguous segments in address order but **never bridges holes** (tests at `:3330-3390`) | `:1415-1497` |
| `capture_global` (`recorder.rs`) | Proves static storage for qualified names when possible, evaluates the symbol, and caches only file-qualified globals within a module epoch | `src/recorder.rs:512-647` |
| `shutdown` | Tries `-gdb-exit`, waits up to **3 seconds**, kills the group on failure, aborts reader tasks, reaps | `src/gdb_mi.rs:1199-1216` |
| `kill` | The forced path | |

The **module epoch** advances when library-loaded/unloaded notifications arrive (`src/gdb_mi.rs:2418-2421`), which invalidates cached globals.

### Tests and limits

`src/gdb_mi.rs` tests cover: token/async separation, late results, malformed records, bounded frames, recursive caller shadowing, partial memory reads, budget exhaustion, output cap and module epoch (`:2494-3732`). The MI parser has independent stream/result tests (`src/mi_parser.rs:699-1043`).

These prove **implementation branches**, not that every C execution reaches every branch. **No DWARF location expression is evaluated here**; static metadata only qualifies observations.

See [MI2 parser](engine-mi-deep.html), [recorder](engine-recorder-deep.html), [stack/memory](engine-stack-deep.html) and [failure walkthrough](engine-walkthroughs.html).

## Diagram notes

| Diagram | What it shows |
|---|---|
| **Stop-loop choices**<br>{{diagram:d2-stop}} | Heap ingestion precedes capture; exit, signal, user-code and non-user frames follow different branches. |
| **Checkpoint construction**<br>{{diagram:d2-checkpoint}} | A retained frame plus bounded variable/byte capture become one snapshot and source-checkpoint index. |
| **MI command lifecycle**<br>{{diagram:d2-mi-cycle}} | Tokened results and asynchronous stops are separate channels. |
<!-- END CHAPTER: content/engine-gdb-deep.md -->

<!-- BEGIN CHAPTER: content/engine-mi-deep.md -->
# 25 · GDB/MI2 lexer and record parser

> **In one sentence:** MI2 looks like text, but it has a grammar. This parser turns one complete GDB line into typed fields *before* the engine interprets it.

## Key terms

| Term | Meaning |
|---|---|
| **Lexer** | The first pass that splits characters into pieces: numbers, commas, quoted strings, braces. |
| **Record** | One parsed GDB message. `^done` is a command **result**; `*stopped` reports the program **stopped**. |

> **Example.** For `4^done,frame={func="main"}`, the lexer sees token 4, the result marker and a `frame` field. The parser builds a structured result **for command 4**, *not* a new program stop.
>
> A partial or malformed line never becomes a valid frame or a fabricated checkpoint.

## The big picture

`src/mi_parser.rs` decodes **one complete line** of GDB's machine interface into a typed `MiRecord`.

- It is **not** the Trace Parser's JSON reader. It parses debugger *protocol* syntax before any engine evidence exists.
- The caller in `src/gdb_mi.rs` supplies newline-terminated lines from GDB's stdout.
- Malformed input is **logged**, never converted into a stop.

## Part 1 · Tokens and typed values

| Type | Location | Represents |
|---|---|---|
| `Token` | `src/mi_parser.rs:55-73` | An optional numeric command token, MI punctuation (`^ * = ~ @ & , { } [ ]`), a decoded quoted string, or a bare identifier |
| `MiValue` | `:80-85` | A constant, a tuple of results, or a list |
| `MiResult` | `:88-92` | A `variable=value` binding |
| `MiListItem` | `:96-99` | Distinguishes a plain list value from an inline result |
| `MiRecord` | `:117-137` | `Result`, `Exec`, `Notify` and `Stream { Console \| Target \| Log }` variants |

The parser **deliberately preserves unknown record classes and fields** instead of discarding them. Protocol extensions stay visible, without the engine inventing an interpretation.

## Part 2 · Lexing

| Function | Location | Job |
|---|---|---|
| `lex_line(input: &[u8]) -> Result<Vec<Token>, MiParseError>` | `src/mi_parser.rs:361-390` | Skips ASCII whitespace, tracks byte offsets for errors, repeatedly applies `next_token` |
| `next_token(input: &[u8]) -> IResult<&[u8], Token>` | `:340-358` | Tries number, quoted string, identifier, then each punctuation token |
| `lex_quoted_string` | `:259-329` | Decodes C-style escapes |
| `lex_ident` / `lex_number` | `:216-258` | MI identifier characters / the decimal command prefix |

**Escapes decoded:** `\n`, `\r`, `\t`, `\\`, `\"`, `\'`, `\e`, `\b`, `\f`, `\v`, `\a`, `\0`, and two-digit `\xHH`.

- An **unknown escape** degrades to the escaped character.
- An **unterminated string** is an error.

The GDB reader adds a separate **16 MiB line bound** and UTF-8-lossy conversion before calling `parse_line` (`src/gdb_mi.rs:2302-2323`). The parser itself expects a *complete* line; a partial line must never be parsed as a record.

## Part 3 · Recursive-descent parsing

**Signature:** `pub fn parse_line(line: &str) -> Result<Option<MiRecord>, MiParseError>` (`src/mi_parser.rs:559-637`)

Blank lines and `(gdb)` prompts return `Ok(None)`. Otherwise it lexes, optionally consumes a leading number as the token, then dispatches on the first punctuation:

| Prefix | Produced record |
|---|---|
| `^class,...` | `MiRecord::Result` |
| `*class,...` | `MiRecord::Exec` |
| `=class,...` | `MiRecord::Notify` |
| `~"text"` | console stream |
| `@"text"` | target stream |
| `&"text"` | log stream |

### The helper functions

| Function | Location | Requires / accepts |
|---|---|---|
| `expect_class` | `:639-647` | An identifier |
| `parse_to_end` | `:665` | Zero or more comma-separated result bindings |
| `parse_result` | `:474-493` | `name=value` |
| `parse_value` | `:428-471` | A quoted constant, tuple, list, or bare identifier |
| `parse_result_list` | `:496-521` | The MI list-of-results comma forms |
| `parse_list_items` | `:524-557` | Disambiguates `name=value` from a plain value by looking **one token ahead** |

## Worked example

This is an *illustrative* MI fragment, not captured output:

```text
4^done,frame={level="0",func="main",file="main.c",line="7",address="0x401136"}
```

| Stage | What happens |
|---|---|
| Lexer | Produces `Number(4)`, `Caret`, `Ident("done")`, `Comma`, `Ident("frame")`, `Equal`, `LBrace`, then the quoted fields and separators |
| `parse_line` | Consumes the token, emits a `Result` with token `Some(4)` and class `done`, stores a tuple under `frame` |
| `gdb_mi::route_record` | Sends it **only** to the pending command 4 |

A following `*stopped,reason="end-stepping-range",frame={...}` is an `Exec` record. It is delivered through the **asynchronous stop channel** and can never acknowledge command 4.

## Failure behavior and tests

`MiParseError` distinguishes (`src/mi_parser.rs:34-53`):

- unterminated strings,
- unexpected tokens,
- trailing characters,
- unknown records, and
- token overflow.

The GDB reader logs the line and error in a bounded console buffer and continues. A malformed line **never** fabricates a frame or a completion. EOF fails all pending commands (`src/gdb_mi.rs:2328-2337`).

**Tests** cover simple, result, exec, notify and stream records; empty containers; deep nesting; escapes; unknown classes; real GDB transcript fixtures; whitespace; token overflow; and seeded mutations (`src/mi_parser.rs:699-1043`). They validate **syntax preservation**. They do not claim GDB produced a particular record in a live session.

See the [GDB controller](engine-gdb-deep.html) for token routing and stop consumption.

## Diagram notes

| Diagram | What it shows |
|---|---|
| **MI parsing pipeline**<br>{{diagram:d2-mi-parse}} | Lexing, recursive value parsing, typed record classification and caller routing. |
<!-- END CHAPTER: content/engine-mi-deep.md -->

<!-- BEGIN CHAPTER: content/engine-recorder-deep.md -->
# 26 · Recorder, resident accounting, and journal append

> **In one sentence:** The recorder is the notebook used during debugging. It appends observations and records where the notebook could not keep up.

## Key terms

| Term | Plain meaning |
|---|---|
| **Checkpoint** | A saved view of the program at a particular observed stop, including bounded variables and heap state. |
| **Gap** | An explicitly marked stretch whose exact instruction events were not retained. It is not a guessed list of instructions. |

> **Example.** If stepping skips a library call, the recorder saves the known stop and a gap marker. It does not invent one event per unknown library instruction.
>
> **What not to assume:** Resident memory, streamed journal bytes and exported JSON have separate accounting and limits.

## The big picture

`src/recorder.rs` is the engine's write boundary for retained observations. It owns the resident `RecordingData`, a create-new JSONL `Journal`, the resident-byte counter, the disassembly cache and per-journal counters. It also performs the bounded stopped-inferior byte reads for variables and configured memory regions. It does not decide when a source checkpoint is semantically warranted; `run_gdb_phase` does that.

## Construction and resident accounting

**Principal type:** `Recorder { data, config, limits, cache, journal, resident, module_epoch, journaled_allocations }` (`src/recorder.rs:194-203`). **Signature:** `pub fn new(config: &TracingConfig, limits: &ExecutionLimits) -> Result<Self>` (`:206-236`). It initializes coverage with user-code scope, `stop_model="gdb_all_stop"`, and limitations that explicitly say startup/excluded code is a gap, memory is exact only at its recorded stop, and readable bytes do not establish initialization (`:207-220`). It opens the optional journal and appends the configuration record before any observation (`:221-225`).

`fn reserve(&mut self, size: usize) -> Result<()>` (`:251-257`) rejects a resident addition above `max_trace_memory_bytes`. This is **resident record accounting**, not process RSS and not the JSONL cap. `truncate(reason)` clears `complete_for_scope` and appends a de-duplicated reason (`:238-249`). `finish` records resident bytes, journal bytes, calls `journal.finalize()`, and marks incomplete if finalization fails (`:461-469`).

## Checkpoints, allocations, gaps and instructions

**Signatures:**
- `pub fn checkpoint(&mut self, snapshot: &ExecutionSnapshot) -> Result<()>` (`src/recorder.rs:259-265`) serializes to measure, reserves, journals `"checkpoint"`, then pushes `snapshot.step_index` into `source_checkpoints`.
- `pub fn allocations(&mut self, events: &[HeapEvent], stop_id: u64) -> Result<()>` (`:267-285`) journals only newly seen events, wrapping each as `observed_at_stop` plus the event. The stop is when the engine **noticed** the row, not a proven allocation call time.
- `pub fn gap(&mut self, start: Option<String>, end: Option<String>, origin: &str, reason: &str) -> Result<()>` (`:287-305`) creates a `TraceGap` at the current `instruction_count` and journals it.
- `pub fn event(&mut self, event: InstructionEvent) -> Result<()>` (`:446-459`) reserves/journalizes the attempt, increments `instruction_count`, and retains the event only if configured. An empty resident array therefore does not imply zero attempts.

A checkpoint is an index into the snapshot vector, not a duplicated snapshot object in the journal or final JSON. A gap records an interval of unsupported/unrecorded execution; its unknown instruction count is not guessed.

## Disassembly cache and module epoch

**Signature:** `pub async fn disassemble(&mut self, session: &mut GdbSession, stop: &StoppedInfo) -> Result<Option<u64>>` (`src/recorder.rs:307-373`). It returns `None` when static disassembly is disabled. It clears the address-to-`instruction_id` cache when the session's module epoch changes (`:315-318`), which prevents a reused address from inheriting instructions from a previous module mapping. A cached address increments `disassembly_cache_hits`; otherwise it requests a bounded 128-byte window with `-data-disassemble -s <addr> -e <end> -- 2` (`:322-335`).

Each returned `asm_insns` item becomes a `DisassembledInstruction` with a new table ID, module identity containing PID/module epoch, raw address, opcodes, text and function (`:340-369`). Every inserted instruction is reserved, journaled and appended to `disassembly_table`. The result is the ID for the originally requested address, not an execution claim. The module epoch is advanced by GDB library notifications (`src/gdb_mi.rs:2418-2421`).

## Bounded variable and configured-memory reads

**Signature:** `pub(crate) async fn capture_variable_memory(session: &mut GdbSession, var: &mut VariableInfo, reads: &mut CheckpointReads, budget: &mut MemoryBudget)` (`src/recorder.rs:45-185`). It first sets `initialization_status="unknown"`, preserves GDB's runtime size when DWARF did not supply it, and creates `MemoryEvidence` from the requested extent (`:51-59`).

The branches are deliberately distinct:
- no address: `unavailable`;
- address but no extent: `unknown`;
- invalid/overflowing range: `unknown`;
- zero extent: `full` with an empty byte string;
- exhausted object/checkpoint/read budget: `unattempted` with the limiting field;
- otherwise chunk reads up to the allowed bytes, reusing `CheckpointReads` when possible (`:60-138`).

Each chunk is requested through `read_memory_bytes_partial`; a cached chunk increments `saved_memory_reads`, while a real read decrements `remaining_reads` when set (`:104-121`). A short response or error stops the loop and preserves the prefix. Status becomes `full`, `partial`, `unreadable`, `truncated` or `unattempted` according to bytes and failure (`:141-176`). The raw prefix is stored, the reason/limit recorded, and `set_validity` derives the variable's validity. **Captured bytes do not prove initialization, and a partial read is not a zero-filled object.**

`pub async fn selected_memory(&self, session: &mut GdbSession) -> Vec<MemoryObservation>` (`:375-445`) handles configured `memory_regions`. It evaluates each address expression, caps size by per-read and snapshot budgets, and records `Valid`, `Truncated`, `Unreadable` or `Unavailable` with an error when appropriate. It never silently extends a read past the requested region.

`capture_global` (`:512-647`) evaluates a named global, proves static storage for qualified names when possible, and caches only file-qualified global metadata within a module epoch. Function-local statics stay on the query path. Globals use `identity="symbol:<name>"`; a file-qualified name is not a runtime activation ID.

## Outcome classification and tests

`pub fn event_outcome(before: &StoppedInfo, after: &StoppedInfo) -> &'static str` (`src/recorder.rs:492-510`) returns `completed` only for an observed `end-stepping-range`; `faulted` only for a synchronous signal at the same PC; `interrupted` for another signal; otherwise `unknown`. A missing after-stop is not completion.

Tests cover cap/no-overwrite behavior, torn last records, cache/quota behavior, partial reads and invalid references (`src/recorder.rs:656-687`; `src/gdb_mi.rs:2590-3228`). The recorder's journals are ordered evidence, not a reconstructable final trace; see [journal/export](engine-export-deep.html).

## Diagram notes

### Memory read evidence

{{diagram:d2-memory}}

A bounded chunked read preserves raw prefix bytes and explicit partial/unreadable status.
<!-- END CHAPTER: content/engine-recorder-deep.md -->

<!-- BEGIN CHAPTER: content/engine-stack-deep.md -->
# 27 · Stack frames, variables, and observed memory

> **In one sentence:** This chapter explains why the same local name can refer to different storage in different function calls.

## Key terms

| Term | Plain meaning |
|---|---|
| **Call frame** | The debugger's record of one active function call: function name, position, and where it is stopped. |
| **Lexical scope** | The part of a function's source where a declaration is in scope. Two nested blocks may both declare `value`. |

> **Example.** In `recurse(3)` calling `recurse(2)`, both frames can display a local named `value`. Their activation IDs and observed frame positions keep them apart.
>
> **What not to assume:** A visible name or matching address does not by itself prove two observations refer to one lifetime.

## The big picture

Stopped-state capture answers a narrow question: at this observed GDB stop, what did the selected frame, selected caller frames, globals and bounded byte reads show? It does not reconstruct an object graph or prove that a source declaration was instantiated. `src/gdb_mi.rs` owns frame/variable extraction; `src/recorder.rs` performs byte reads; `src/memory.rs` qualifies addresses and pointer candidates; `src/trace.rs` assembles snapshots.

## Frames and activation identity

`StackFrameInfo` is the serialized frame record produced by `gdb_mi::frame_from_tuple` and `parse_frames` (`src/gdb_mi.rs:1302-1345`). `capture_state` requests at most 200 frames with `-stack-list-frames 0 200`, marks whether the chain was truncated, and builds an activation path from oldest to youngest (`src/gdb_mi.rs:1864-1910`). The selected frame is frame zero. Recursive calls with the same function name receive different activation IDs because the path is positional and compared with the prior path; a truncated chain clears continuity rather than reusing a sliding prefix (`:1889-1907`).

A variable's `identity` is `activation:<activation>:<scope>:<name>` for locals or `static:<scope>:<name>` for static storage (`src/gdb_mi.rs:2129-2171`). When lexical probing fails, a storage-based identity may include an address, but an unresolved collision becomes `unresolved:...`; equal names or equal addresses do not establish identity. `caller_frames` repeat this with separate activation IDs and never change GDB's selected frame (`:1753-1834`).

### Recursive C illustration

This is a small illustrative input, not a captured trace:

```c
int recurse(int value) {
    int local = value;
    return value ? recurse(value - 1) : local;
}
int main(void) { return recurse(3); }
```

If the engine retained three recursive activations, their frames may all print `func="recurse"` and a variable named `local`. The activation IDs distinguish the three recorded frames; a same-name static would use static identity, not an activation. This page does not claim that this example was compiled or run for DOC-2.

## Variable observation and raw bytes

`capture_state` first obtains `-stack-list-variables --all-values`, counts duplicate names over the **full** list, and probes lexical symbols for the configured cap (`src/gdb_mi.rs:1914-1947`). If a duplicate has multiple lexical declarations, each declaration is retained with its own scope/identity; if association fails, the variable remains an explicit unresolved collision (`:1981-2051`). For ordinary variables, fresh probe address/size/target data is preferred, with `&name`, `sizeof(name)` and console `whatis` as fallbacks (`:2054-2127`).

`prepare_variable_memory` preserves the runtime size and selects aggregate vs scalar limits (`src/gdb_mi.rs:1719-1736`). `recorder::capture_variable_memory` then reads chunks within the per-object, per-read and checkpoint budgets, retaining a raw-byte prefix plus `MemoryEvidence` (`src/recorder.rs:45-185`). A short read is a prefix, not a zero-filled or initialized object. `set_validity` marks the variable's bytes status, and `initialization_status` remains unknown.

## Process maps and pointer candidates

**Signatures:** `pub fn address(text: &str) -> Option<u64>`, `pub fn storage_region(pid: u32, addr: u64) -> Option<&'static str>` and `pub fn executable_bias(binary: &Path, pid: u32) -> Option<u64>` (`src/memory.rs:5-100`). `storage_region` reads `/proc/<pid>/maps` and returns stack, heap, code or another mapping class. `executable_bias` compares the ELF's load addresses with the observed mapping to subtract relocation bias. These are process observations at capture time, not C type facts.

**Signature:** `pub fn enrich_snapshot(snapshot: &mut ExecutionSnapshot, timeline: &HeapTimeline, pid: Option<u32>)` (`src/memory.rs:145-205`). It builds the set of observed objects from frame-zero variables, globals, and caller variables only when the caller's retained frame is on this snapshot's call stack and its activation matches (`:162-187`). It then computes pointer targets without cloning variable/value/byte buffers. A target can be null, a heap address candidate, a stack/global containing object, or unresolved.

`pointer_target` (`:260-340`) treats `points_to == 0` as null. For a heap address, it asks the timeline for **all** address candidates and uses the newest only as an address match; multiple historical occupants yield `ambiguous_reuse`, a freed block yields `known_freed_address`, and otherwise `address_only`. It records base, offset and within-extent, but `allocation_id` is not pointer provenance. For non-heap addresses, it finds a containing observed object; recursive statics are deduplicated only with identical identity and extent, never by name or address alone (`:299-315`).

## Reconciliation after the run

`refresh_heap_targets` (`src/memory.rs:212-249`) runs after terminal transport verification. It widens local heap IDs when multiple PIDs were observed, but does **not** resolve a historical pointer through later frees/reallocs/reuse. If history is incomplete it sets `active=None`, `allocation_id=None` and `provenance_status="history_incomplete"`. This preserves the distinction between what was observed while stopped and what the final timeline later knew.

## Snapshot assembly and failure

`push_snapshot` (`src/trace.rs:2179-2304`) checks `max_snapshot_count`, computes remaining memory budget after variable reads, reads alive observed heap blocks up to `max_heap_blocks_per_snapshot`, captures a bounded stdout head, and appends an `ExecutionSnapshot` with `step_index`, stop/frame metadata, call stack, variables, caller frames, heap blocks and program output. `capture_or_empty` preserves a stop even if `capture_state` fails (`src/trace.rs:2164-2175`). Missing frame, metadata, read or budget produces explicit partial state; no fabricated values are inserted.

### Tests and boundaries

Tests cover truncated recursive chains, caller shadowing, partial/unreadable memory, quota accounting, module changes and pointer candidate ambiguity (`src/gdb_mi.rs:2590-3228`; `src/memory.rs:363-943`). Static DWARF can supply a type or fallback extent, but GDB remains authoritative for the stopped value; captured bytes do not establish initialization. See [GDB capture](engine-gdb-deep.html), [recorder](engine-recorder-deep.html) and [identity contracts](identity-contracts.html).

## Diagram notes

### Frame and activation identity

{{diagram:d2-frame}}

Recursive activations remain distinct even when function and variable names repeat.

### Capture scope and invalidation

{{diagram:d2-cache}}

Module epochs and stop-local reuse prevent stale live observations; static metadata is separate.
<!-- END CHAPTER: content/engine-stack-deep.md -->

<!-- BEGIN CHAPTER: content/engine-dwarf-deep.md -->
# 28 · GCC DWARF extraction and observation association

> **In one sentence:** DWARF is the compiler's description of the finished executable. This page explains how that description becomes trace-local type and declaration entries.

## Key terms

| Term | Plain meaning |
|---|---|
| **ELF** | The executable file format of the debug program; DWARF sections live inside it. |
| **DIE** | A debug-information entry: one DWARF record for something such as a type, variable or member. |

> **Example.** A GCC-produced struct entry can say that member `count` starts four bytes into the struct. It cannot say that a particular stopped instance's bytes were initialized.
>
> **What not to assume:** Missing or compiler-dependent DWARF facts stay missing; the engine should not replace them with a guessed textbook layout.

## The big picture

`src/dwarf.rs` reads compiled metadata from the **actual GCC debug executable**, not the original C text or a GDB location expression. The registry belongs to one trace; its DIE identifiers are not runtime object identities. `trace_program` calls `extract_excluding(&debug_build.binary_path, &tracker_source)` after successful compilation, rejects a registry with no project CU, constructs an `Arc<ObservationTypes>` for stop capture, and finally calls `associate(&mut trace)` after capture ([trace.rs:351-366](../../src/trace.rs#L351-L366), [trace.rs:423-439](../../src/trace.rs#L423-L439), [trace.rs:944-955](../../src/trace.rs#L944-L955), [trace.rs:2728-2760](../../src/trace.rs#L2728-L2760)). See [AST extraction](engine-ast-deep.html) and [fusion](engine-fusion-deep.html) for the other two static-evidence steps.

## The registry and its identities

`TypeRegistry` holds provider/artifact, retained `UnitInfo` CUs, `TypeEntry` types, `Declaration` records, counters and limitations; `compilation_units_scanned` also counts excluded CUs. `UnitInfo` records CU name, compilation directory, DWARF version and address size. A type has a CU ID, DIE offset, kind, size/encoding, optional referenced type, members, dimensions and enumerators; a declaration has a kind, source file/line/column, containing function, lexical `scope_id`, optional type and origin explanation ([dwarf.rs:26-124](../../src/dwarf.rs#L26-L124)). `debug:u<absolute-CU-header-offset>` and `debug:u...:o<unit-relative-DIE-offset>` are generated during the two scans; these distinguish identically named types and statics in different translation units ([dwarf.rs:535-574](../../src/dwarf.rs#L535-L574), [dwarf.rs:625-647](../../src/dwarf.rs#L625-L647), [tests/dwarf_registry.rs:169-214](../../tests/dwarf_registry.rs#L169-L214)). A declaration's `scope_id` comes from the parent DIE stack, not from its name; shadowed `value` declarations retain distinct lexical parents ([dwarf.rs:632-647](../../src/dwarf.rs#L632-L647), [dwarf.rs:775-818](../../src/dwarf.rs#L775-L818), [tests/dwarf_registry.rs:151-167](../../tests/dwarf_registry.rs#L151-L167)).

## ELF sections and the two DIE passes

`extract(path: &Path)` and `extract_excluding(path: &Path, instrumentation_source: &Path)` return `Result<(TypeRegistry, ExtractionMetrics), String>` through `extract_impl`; the latter excludes only a CU whose recorded name is the exact injected source path, not a user file sharing its basename ([dwarf.rs:577-631](../../src/dwarf.rs#L577-L631)). `load_dwarf` checks the object is at most 64 MiB and has `.debug_info` or `.zdebug_info`, parses it with `object::File`, takes the object's endianness, locates normal or legacy compressed sections, caps aggregate *decoded* section bytes before decompressing, and gives gimli `EndianRcSlice`s backed by owned `Rc` bytes. No borrowed object-file buffer escapes this reader boundary ([dwarf.rs:478-533](../../src/dwarf.rs#L478-L533)).

`index_dies(&Dwarf<R>, Instant)` first traverses CU headers and all DIEs with `next_dfs`, recording every absolute `.debug_info` DIE offset as a CU-qualified key. This supports forward and cross-CU `DebugInfoRef`s; at most 512 units, 150,000 DIEs and five seconds are accepted ([dwarf.rs:15-24](../../src/dwarf.rs#L15-L24), [dwarf.rs:535-575](../../src/dwarf.rs#L535-L575)). `extract_impl` reopens the unit iterator without reloading sections; `OriginResolver` caches parsed units by absolute header offset, while a depth-indexed parent stack supplies the current aggregate, function and lexical scope. It retains recognized type tags and variable, parameter and subprogram declarations, counting otherwise unsupported DIE tags rather than promoting them to known facts ([dwarf.rs:603-681](../../src/dwarf.rs#L603-L681), [dwarf.rs:775-819](../../src/dwarf.rs#L775-L819)). Subprogram DIEs serve source-level function correlation but are excluded from runtime variable matching ([dwarf.rs:782-810](../../src/dwarf.rs#L782-L810), [dwarf.rs:1052-1068](../../src/dwarf.rs#L1052-L1068)).

### Specification and abstract-origin chains

`OriginResolver::resolve` starts with attributes on the concrete DIE, follows `DW_AT_specification` preferentially or `DW_AT_abstract_origin`, and fills only *missing* name, source file, line, column and type from each referenced DIE. `UnitRef` stays in the current CU; absolute `DebugInfoRef` uses the DIE index to find the owning CU header, converts to a unit offset and loads that entry. The visited key is `(absolute CU-header offset, unit DIE offset)`; four hops maximum, a cycle, an unsupported form or a missing target marks the chain failed. Cross-CU and same-CU hop descriptions are retained when supplemental facts were actually used; unsuccessful chains increment `unresolved_origin_references` and remain limitations, not file/line guesses ([dwarf.rs:181-211](../../src/dwarf.rs#L181-L211), [dwarf.rs:213-423](../../src/dwarf.rs#L213-L423), [dwarf.rs:648-657](../../src/dwarf.rs#L648-L657), [dwarf.rs:859-863](../../src/dwarf.rs#L859-L863)). `source_file` resolves the DIE's line-table file through directory and CU compilation directory, canonicalizing if possible; `absolute_source_path` instead requires an absolute path for qualified-symbol identity, with lexical normalization if the file moved ([dwarf.rs:424-476](../../src/dwarf.rs#L424-L476)). A real GCC `-O2 -gdwarf-5 -flto` test exercises cross-CU abstract origins and checks filled source attributes ([tests/dwarf_registry.rs:271-310](../../tests/dwarf_registry.rs#L271-L310)).

### Layout is metadata, not a memory read

`type_kind` recognizes base, pointer, array, struct, union, enum, typedef, qualifiers, function and unspecified DIEs. `TypeEntry.byte_size` comes from `DW_AT_byte_size`, except pointer size can use the CU address size; `DW_AT_type` resolves only `UnitRef` or indexed `DebugInfoRef` ([dwarf.rs:163-191](../../src/dwarf.rs#L163-L191), [dwarf.rs:678-701](../../src/dwarf.rs#L678-L701)). For struct/union children, `Member` preserves numeric byte offset, bit size, `DW_AT_data_bit_offset` and legacy storage-relative `DW_AT_bit_offset` separately. Missing union location yields an explicitly marked *implicit* zero; an unhandled member-location expression makes the parent `runtime_dependent` instead of calculating an offset ([dwarf.rs:73-88](../../src/dwarf.rs#L73-L88), [dwarf.rs:702-728](../../src/dwarf.rs#L702-L728)). Array subranges store signed bounds and numeric count (or derive it from ordered bounds); C's default lower bound is zero. A nonnumeric bound/count sets `runtime_dependent`, while missing static information is `unavailable`. Enumerators retain signed or unsigned constants, including negative values ([dwarf.rs:729-774](../../src/dwarf.rs#L729-L774)). The second resolution pass clears any type/member/declaration reference absent from the retained type set and records the count; excluded-CU references therefore do not masquerade as valid types ([dwarf.rs:820-874](../../src/dwarf.rs#L820-L874)). The real-GCC layout test covers bit offsets, union zero offsets, enums, multidimensional arrays and VLAs ([tests/dwarf_registry.rs:36-149](../../tests/dwarf_registry.rs#L36-L149)); none of these fields identifies an active union member or proves initialization.

## Linking stopped variables without guessing

The immutable, trace-local `ObservationTypes::new(&TypeRegistry)` builds location and qualified-symbol lookups before GDB runs. It skips declaration-only and subprogram DIEs, follows at most 64 typedef/qualifier links to find size/aggregate/pointer shape, and withholds size on missing, cyclic or runtime-dependent paths ([dwarf.rs:876-971](../../src/dwarf.rs#L876-L971)). At each stop `gdb_mi::prepare_variable_memory` preserves GDB's original size in `runtime_size_bytes`, calls `enrich(&mut VariableInfo, func)`, and selects the aggregate memory-read budget; requested globals also pass this lookup to `recorder::capture_global` ([gdb_mi.rs:1719-1736](../../src/gdb_mi.rs#L1719-L1736), [recorder.rs:629-646](../../src/recorder.rs#L629-L646)).

`enrich` accepts one compatible `(declaration_file, declaration_line, variable name)` candidate with matching function. The alternate symbol route is **only** for GDB-qualified names without a declaration line: a quoted file qualifier must normalize to the same full absolute path; bare names and ambiguous location hits cannot fall back to a guessed symbol. A unique match sets `type_ref`/`declaration_ref` and missing location fields; DWARF size is a fallback only if an address exists and GDB supplied no size. It returns aggregate shape for the capture budget; `is_pointer` reports known pointer shape, not pointee extent ([dwarf.rs:973-1040](../../src/dwarf.rs#L973-L1040), [tests/dwarf_registry.rs:379-529](../../tests/dwarf_registry.rs#L379-L529)). After capture, `associate(&mut DebugSessionTrace) -> u64` makes a separate conservative pass over top-frame variables, caller-frame variables and globals, requiring exact file/line/name, compatible function, valid type and exactly one declaration; it attaches only the two DWARF IDs ([dwarf.rs:1042-1105](../../src/dwarf.rs#L1042-L1105)). The synthetic ambiguity test leaves both IDs absent and preserves observed values ([tests/dwarf_registry.rs:312-377](../../tests/dwarf_registry.rs#L312-L377)); the stop-time test preserves a runtime size over the static fallback ([dwarf.rs:1157-1187](../../src/dwarf.rs#L1157-L1187)).

## Failure and scope

Missing debug sections, invalid/oversized objects, traversal/retention limits, deadline expiration and zero eligible units return an error rather than a plausible empty registry; `trace_program` retains the runtime trace and adds an analysis note instead ([dwarf.rs:480-490](../../src/dwarf.rs#L480-L490), [dwarf.rs:545-567](../../src/dwarf.rs#L545-L567), [dwarf.rs:870-874](../../src/dwarf.rs#L870-L874), [trace.rs:353-367](../../src/trace.rs#L353-L367), [tests/dwarf_registry.rs:573-597](../../tests/dwarf_registry.rs#L573-L597)). Unsupported tags, unresolved references and dynamic layout expressions remain explicit partial metadata. DWARF is compiled layout/declaration evidence; GDB observation remains the authority for stopped-state bytes, values, extents and execution. See [metadata overview](engine-metadata.html) and [evidence boundaries](identity-contracts.html).

## Diagram notes

### DWARF extraction flow

{{diagram:d2-dwarf}}

Object loading, CU/DIE scans, bounded reference resolution and conservative observation association.

### Origin/reference resolution

{{diagram:d2-origin}}

Cross-CU references are followed through indexed DIE offsets with a four-hop and cycle bound.
<!-- END CHAPTER: content/engine-dwarf-deep.md -->

<!-- BEGIN CHAPTER: content/engine-ast-deep.md -->
# 29 · Optional Clang source-syntax extraction

> **In one sentence:** Clang's AST describes what the source code says. It is useful for navigation, but its expressions are not an execution log.

## Key terms

| Term | Plain meaning |
|---|---|
| **AST** | Abstract syntax tree: a structured version of source constructs such as function calls, declarations and fields. |
| **Source span** | The file and character range where Clang located a written construct. |

> **Example.** If the source contains `if (0) malloc(40);`, Clang can still find the `malloc` expression. The debugger may never execute it.
>
> **What not to assume:** Macros, flags and same-line calls can make a source-to-event match ambiguous. Keep candidates visibly different from confirmed events.

## The big picture

`src/clang_ast.rs` is an opt-in, bounded **second compiler's** syntax view. `trace_program` invokes `analyze(&BuildConfig, clang_path, Option<&TypeRegistry>)` only when `config.clang_ast` is true and time remains; it stores one session-level `SourceMetadataRegistry`, including per-translation-unit failure reasons, without withholding GDB execution when Clang fails ([trace.rs:368-391](../../src/trace.rs#L368-L391)). GCC DWARF and GDB retain layout and runtime authority; see [DWARF extraction](engine-dwarf-deep.html) and [fusion](engine-fusion-deep.html).

## Registry, spans and trace-local IDs

`SourceMetadataRegistry` owns provider/version, cross-provider incompatibility reason, `AstUnit` statuses, source declarations/expressions, heap-call *candidates* and ambiguities, timing and limitations. `SourceDeclaration` records kind/name/type spelling, name position, function and optional initializer-expression ID; `SourceExpression` records kind/span/type/operator, access syntax, callee/classification, operand IDs, declaration references and `sizeof` operand. Optional DWARF IDs, type evidence and `DwarfMemberRef` are **links**, not C execution evidence ([clang_ast.rs:22-163](../../src/clang_ast.rs#L22-L163)). `SourceSpan` uses source byte offsets, line/column and `direct` versus `macro_expansion` provenance, with optional spelling location. Neither source ID (`tu0:d...`, `tu0:e...`) nor Clang's raw node ID is a runtime activation ID: `extract` assigns serial IDs in retained traversal order, then resolves temporary raw IDs into these serialized references ([clang_ast.rs:86-163](../../src/clang_ast.rs#L86-L163), [clang_ast.rs:578-586](../../src/clang_ast.rs#L578-L586), [clang_ast.rs:646-706](../../src/clang_ast.rs#L646-L706), [clang_ast.rs:800-813](../../src/clang_ast.rs#L800-L813)). Repeated analysis on the test fixture checks stable `(ID, kind, start offset)` triples ([tests/clang_ast.rs:251-289](../../tests/clang_ast.rs#L251-L289)).

## Invocation and option fidelity

`clang_flags(&BuildConfig) -> Result<Vec<String>, String>` forwards build definitions/includes and classified compiler options in order: language mode, preprocessing/include flags, `-f`/`-m`/`-pthread`, and supported optimization levels; it drops `-g` and warnings, but rejects unclassified flags and missing option arguments rather than silently analyzing a different preprocessed program ([clang_ast.rs:178-225](../../src/clang_ast.rs#L178-L225)). Clang receives `-Xclang -ast-dump=json -fsyntax-only -x c -O0`, then the forwarded flags and user source. The `-O0` baseline plus user optimization order matters for macros such as `__OPTIMIZE__`; the GCC/Clang fixture independently checks that `-O2` chooses `optimized_branch` ([clang_ast.rs:290-301](../../src/clang_ast.rs#L290-L301), [tests/clang_ast.rs:15-79](../../tests/clang_ast.rs#L15-L79)). Accepted compiler-specific `-f*`, `-m*` or `-pthread` can still mean different things to GCC and Clang: `analyze` retains syntax but sets `cross_provider_correlation_unavailable`, withholding DWARF correlation; fusion also honors this gate ([clang_ast.rs:248-256](../../src/clang_ast.rs#L248-L256), [clang_ast.rs:386-390](../../src/clang_ast.rs#L386-L390), [tests/clang_ast.rs:556-597](../../tests/clang_ast.rs#L556-L597)). Acceptance is not proof of semantic equivalence.

`analyze` uses `build.user_sources()` rather than the generated wrapper and analyzes at most 16 TUs in canonical path order, with an eight-second shared budget, per-Clang three-second deadline, tool-only 512 MiB best-effort memory limit, 1 MiB input source and 8 MiB output cap. Supervisor cancellation, timeout, truncated output, nonzero Clang status or warnings that a configured argument was unused/unsupported make that TU unavailable; malformed JSON does too ([clang_ast.rs:228-390](../../src/clang_ast.rs#L228-L390)). An extraction failure *after* some records are retained marks the unit `partial`, while unsupported expression nodes also mark `partial` with children retained; the unit reports byte, node, parse and registry timings. A failed Clang binary still leaves a compiled runtime trace ([tests/clang_ast.rs:438-506](../../tests/clang_ast.rs#L438-L506)).

## JSON traversal and conservative source ranges

`span(node, path, &LineIndex)` computes byte-based positions from Clang expansion offsets and token lengths, rejects zero/out-of-range spans and included-header or foreign-file locations, and records spelling location separately for macros. `LineIndex::line_column` derives line/column from the source bytes instead of trusting a foreign file's position ([clang_ast.rs:397-488](../../src/clang_ast.rs#L397-L488)). `extract(&Value, &Path, &[u8], unit_id, &mut AstUnit, &mut SourceMetadataRegistry, deadline) -> Result<(), String>` walks `inner` nodes with an explicit depth/function/record stack, bounded to 100,000 nodes and depth 96; skipped header spans are not traversed into user records ([clang_ast.rs:14-20](../../src/clang_ast.rs#L14-L20), [clang_ast.rs:560-636](../../src/clang_ast.rs#L560-L636)).

It retains `VarDecl`, `ParmVarDecl`, `FunctionDecl`, `FieldDecl` and selected calls, operators, subscripts, members, casts, conditionals, declaration references and literals, with limits of 2,048 declarations and 4,096 expressions. An unrecognized expression/operator/literal increments `unsupported_nodes`; unsupported wrapper nodes may still expose supported children. `visible_child_ids` traverses at most 64 descendant nodes through nonselected wrappers, and a deferred pass resolves operand IDs, referenced declarations and initializer IDs *only if retained* ([clang_ast.rs:490-558](../../src/clang_ast.rs#L490-L558), [clang_ast.rs:637-813](../../src/clang_ast.rs#L637-L813)). `direct_callee` follows at most eight first-child wrappers to a `DeclRefExpr`; `malloc`/`calloc`/`realloc`/`free` is classified as a standard allocator only if that declaration ID was observed among header `FunctionDecl`s. Otherwise calls are `direct_other` or `indirect_or_unknown`, not inferred from spelling alone. `MemberExpr` preserves dot/arrow syntax, base type spelling and member name; `sizeof` preserves whether its argument is a type or expression ([clang_ast.rs:504-537](../../src/clang_ast.rs#L504-L537), [clang_ast.rs:569-575](../../src/clang_ast.rs#L569-L575), [clang_ast.rs:700-794](../../src/clang_ast.rs#L700-L794)). The GNU-expression fixture verifies partial extraction rather than false completeness ([tests/clang_ast.rs:508-524](../../tests/clang_ast.rs#L508-L524)).

## Declaration matching against GCC

When DWARF exists **and** the provider gate permits it, `correlate_declarations` indexes DWARF declarations by canonical `(file, line, column, name, function)` and a separate line-only key. `FunctionDecl` matches subprogram, `VarDecl`/`ParmVarDecl` match variable/parameter; `FieldDecl` is deliberately left for the parent-type/member resolver in [fusion](engine-fusion-deep.html). It selects one compatible exact-position DIE, preferring a declaration-only subprogram DIE over a definition. A fallback to one line match is allowed only when *at least one side lacks a column*, never when two present columns disagree. Zero or multiple results remain `unresolved` or `ambiguous`; a type link is kept only if the matched type ID exists in the registry ([clang_ast.rs:816-945](../../src/clang_ast.rs#L816-L945), [clang_ast.rs:386-390](../../src/clang_ast.rs#L386-L390)). Focused tests cover duplicate exact matches, missing-column fallback, fields and function declaration preference ([clang_ast.rs:1087-1153](../../src/clang_ast.rs#L1087-L1153), [clang_ast.rs:1210-1275](../../src/clang_ast.rs#L1210-L1275)). This is a static location correlation, not proof that a declaration was instantiated at a stop.

## Heap calls are source-line candidates

`correlate_heap_calls(&mut SourceMetadataRegistry, &[HeapEvent], &Path)` clears prior candidates/ambiguities, indexes calls by canonical file and every covered line up to start+64, with separate direct/macro buckets. Only events whose attribution says `resolved_return_address_minus_one` and have source file/line qualify; operation must match (`malloc`/`calloc` with allocation, `realloc` with reallocation, `free` with free). One direct compatible call with **no** compatible macro call yields `unique_direct_call_on_resolved_line_not_exact_execution`; multiple calls or macro-only calls yield explicit ambiguity (at most eight displayed expression IDs). Empty match stays empty; cancellation or 250 ms stops the event loop with a limitation ([clang_ast.rs:947-1054](../../src/clang_ast.rs#L947-L1054)). The real mixed direct/macro `free` test demonstrates why one return-address line cannot choose a particular call ([tests/clang_ast.rs:190-249](../../tests/clang_ast.rs#L190-L249)); synthetic tests distinguish one candidate, multiple calls, macro-only and the eight-ID cap ([tests/evidence_fusion.rs:355-440](../../tests/evidence_fusion.rs#L355-L440)). No call candidate supplies runtime argument values, allocation identity or expression execution. See [heap instrumentation](engine-heap.html) and [evidence constraints](identity-contracts.html).

## Diagram notes

### Clang AST pipeline

{{diagram:d2-ast}}

Optional bounded Clang syntax extraction is retained independently of GCC/GDB runtime evidence.

### Syntax versus runtime

{{diagram:d2-ast-runtime}}

A source expression or declaration link is not proof that a particular dynamic call executed.
<!-- END CHAPTER: content/engine-ast-deep.md -->

<!-- BEGIN CHAPTER: content/engine-allocator-deep.md -->
# 30 · C allocator runtime: interception and transport

> **In one sentence:** This is the implementation of the C allocation detour. Its job is to record calls it intercepts without changing what the allocator returns.

## Key terms

| Term | Plain meaning |
|---|---|
| **`--wrap`** | A linker feature that routes a linked `malloc` call to `__wrap_malloc`; the wrapper calls `__real_malloc` after recording it. |
| **Event ACK** | A separate end-of-process status line giving the last event number and failed-write count, when normal shutdown permits it. |

> **Example.** The program calls `malloc(24)`. The wrapper calls the real allocator once, assigns an ID if it can track the result, and writes a PID/sequence-tagged line to its own file. Rust later reads that file.
>
> **What not to assume:** A missing final ACK, a torn write or a silent fork child lowers certainty. The wrapper does not intercept every internal library allocator.

## The big picture

The debug executable links `c_runtime/alloc_tracker.c` through linker `--wrap` references to `malloc`, `calloc`, `realloc`, and `free`. It records operations actually intercepted by the linked executable, not every allocation in the process. `src/allocation_tracker.rs::ALLOC_RUNTIME_C` embeds this translation unit and `write_runtime_source(session_dir: &Path) -> Result<PathBuf>` writes it into the session directory (`src/allocation_tracker.rs:30-38`). `trace_program_inner(config: TraceConfig) -> Result<DebugSessionTrace>` hands that file to `compiler::compile_project` (`src/trace.rs:167,226-237`); `run_gdb_phase(...)` supplies the event and status paths in the inferior environment (`src/trace.rs:1113-1124,1171-1189`). For reconstruction and the meaning of verified history, see [Heap timeline](engine-heap-deep.html); for other executions, see [Independent diagnostics](engine-diagnostics-deep.html).

## Source file: `c_runtime/alloc_tracker.c`

### Lifecycle, lock and table

`cte_init(void)` is constructor priority 101 (`c_runtime/alloc_tracker.c:293-317`). An absent/empty `C_TRACE_EVENT_FILE` or failed event-file `open` leaves `g_fd=-1`: wrappers pass through. Otherwise it opens the event file append-only with mode `0600`, optionally opens the independent `C_TRACE_EVENT_STATUS_FILE` the same way, and registers `pthread_atfork(NULL, NULL, cte_after_fork_child)`. Registration failure closes the descriptors and disables instrumentation. The status-file open can fail independently: events can still be written, but there can be no valid terminal ACK. `cte_fini(void)` (`:319-337`) attempts the status line only if both descriptors are valid, on normal destructor execution, then closes them. `_exit`, a crash, or a killed process need not run it.

State is process-local static storage: `g_table` has `CTE_TABLE_SLOTS=1<<17` entries, each `{addr,id,size}` with address zero marking an empty slot; `g_seq` and `g_ids` count emitted attempts and issued allocation IDs respectively; `g_write_failures` counts failed event emissions (`:64-82`). `static unsigned long cte_hash(unsigned long x)` (`:161-169`) mixes address bits. `static cte_slot *cte_find(unsigned long addr)` probes linearly until an empty cell or match (`:171-181`). `static int cte_insert(unsigned long addr, unsigned long id, unsigned long size)` finds an empty cell or returns zero when full (`:183-196`); the wrapper **still emits** the event on overflow. `static void cte_remove(unsigned long addr)` clears the matching cell and reinserts the following cluster so searches do not stop too early; there are no tombstones (`:198-218`). The Rust timeline can match an unindexed predecessor by `(pid,address)`, but neither table overflow nor address equality establishes pointer provenance.

`cte_lock(void)` and `cte_unlock(void)` (`:108-126`) use a test-and-set spinlock around real allocator calls, table changes, ID/sequence updates and writes. `g_busy` is a thread-local reentry guard: allocation inside tracking calls the real allocator without another event. `cte_after_fork_child(void)` (`:136-143`) atomically releases the inherited lock, resets counters, and sets `g_child_reset`; the child's first lock clears its inherited address table (`:117-120`). Clearing is deferred out of the child callback. No prepare/parent lock is taken across foreign atfork handlers. Forking inside an allocator or a foreign handler that later acquires the lock is unsupported; the wrapper cannot make libc allocation after a multithreaded fork generally safe (`:128-135`). `CTE_TESTING` enables fault/lock hooks only in test builds (`:84-97,145-154`), not in production.

### Wrappers and state transitions

Each wrapper passes through if `g_busy` or `g_fd<0`, otherwise sets the guard, captures `__builtin_return_address(0)`, locks, calls the corresponding `__real_*` **once**, saves `errno`, emits an event, adjusts the table, unlocks, restores `errno`, and returns the real result. The captured PC is a return address, not intrinsically a verified source expression; tail calls/inlining can make it unreliable (`:41-47,343-467`).

| Qualified C signature | Branch and ownership result |
|---|---|
| `void *__wrap_malloc(size_t size)` (`:343-361`) | Issues a local ID even on NULL; emits `ok` or `failed`, inserting `{address,id,size}` only for non-NULL. |
| `void *__wrap_calloc(size_t nmemb, size_t size)` (`:363-385`) | Calls real calloc with original operands. Computes `nmemb*size` only if representable; on overflow records byte size `0`, status `overflow`, and both original operands. Otherwise records `ok` or `failed`, inserts only non-NULL. A zero-size **successful** allocation can have size zero and a real nonzero address. |
| `void *__wrap_realloc(void *ptr, size_t size)` (`:387-443`) | Looks up the old address before calling real realloc under the same lock. `ptr==NULL`: fresh ID, `new` or `failed`. Same returned address: fresh ID, `ok_same` if table-held or `new` if predecessor untracked; replace/insert table entry. Non-NULL moved result: fresh ID, `ok_moved`, remove old and insert new. NULL with non-NULL old pointer and `size==0`: `zero`, old ID, remove old (glibc contract used here). NULL with nonzero size: `failed`, old ID, leave original entry intact. IDs are **new for successful in-place as well as moving realloc**, not merely for address changes. |
| `void __wrap_free(void *ptr)` (`:445-467`) | Looks up address and size, calls real free, emits `ok` for table-held, `unknown` for non-NULL untracked, or `null` for NULL; removes only a tracked entry. ID zero marks missing table identity. Calling free on an invalid pointer remains the C program's undefined behavior; logging is not a safety mechanism. |

The table maps **addresses**, not ownership of pointers. A later successful allocation can reuse a freed numeric address with a different lifetime ID. If the C table filled, a `free unknown` or a `realloc` with `old_id=0` can still be paired with a live observed address by Rust, provided its history is reliable ([reconstruction](engine-heap-deep.html#source-file-src-allocation-tracker-rs)). Internal allocations inside unwrapped shared libraries, custom allocators, and `strdup` internals are not intercepted (`:41-46`).

### Event rows, partial writes and terminal ACK

`static void cte_emit_locked(const char *kind, unsigned long id, unsigned long addr, unsigned long size, unsigned long old_id, unsigned long old_addr, unsigned long caller, const char *status, unsigned long nmemb, unsigned long element_size)` increments `g_seq`, formats a line into a 320-byte stack buffer, and calls `cte_write_all` (`:265-287`). Decimal fields are `pid seq id size old_id` and, for calloc only, `nmemb element_size`; hexadecimal **without `0x`** fields are `addr old_addr caller`. The 10-field format is `pid seq kind id addr size old_id old_addr caller status`; calloc appends two operands for 12 fields (`:9-25,273-281`). Status values actually emitted include `overflow` as well as `ok`, `failed`, `ok_same`, `ok_moved`, `new`, `zero`, `null`, `unknown` (`:343-467`); the introductory C comment's status list omits `overflow`, but the code and Rust reader include it.

The following is **illustrative, not captured output**. The first row could represent an intercepted 24-byte malloc in PID 100; the second a free of that tracked address. The separate ACK reports the last issued sequence and zero failed writes:

```text
# alloc_events.txt (illustrative; comments are not part of the actual file)
100 1 malloc 1 1000 24 0 0 4011b6 ok
100 2 free 1 1000 24 0 0 4011d0 ok
# alloc_event_status.txt (illustrative)
100 2 0
```

`static int cte_write_all(int fd, const char *buf, size_t len, int event)` retries short writes, retries `EINTR` at most eight times, and fails on zero write or any other error (`:224-263`). No rollback of bytes already appended is possible: a partially written row may be completed by a later append and must **not** be trusted merely because it has a newline. `cte_emit_locked` increments `g_write_failures` (saturating at `ULONG_MAX`) for a failed/oversized emission (`:282-286`); the destructor writes `pid seq write_failures\n` through the **separate** status descriptor (`:322-330`). The reader verifies ACKs against observed sequences and the followed inferior; a clean-looking prefix cannot prove a failed *last* event did not exist. A missing ACK, nonzero failure count or wrong sequence downgrades completeness, rather than erasing observed rows ([verification](engine-heap-deep.html#transport-and-coverage)). Neither transport nor ACK authenticates files against hostile modification.

## Tests and boundaries

`tests/heap_instrumentation.rs:152-334` compiles real wrapped C fixtures: normal ACK, PID-local child restart and lock-holder fork, multithread serialization, partial/EINTR retry, terminal EIO/zero/repeated-EINTR/partial-then-error, missing/failed ACK and failed status-file open. The terminal-write test feeds actual C files to `EventFileReader` and `verify_event_status`, demonstrating why a contiguous prefix alone is insufficient (`:261-305`). The later [release closure](../trace-engine-v2.2-release-closure.md#fork-child-heap-history-contract) documents observed, silent and unacknowledged children; a silent child with no event/ACK is not discoverable. This is Linux/GCC/glibc-oriented instrumentation on a 64-bit `unsigned long` target (`c_runtime/alloc_tracker.c:55-58`), not an all-allocator, all-libc or all-process census.

## Diagram notes

### Allocator wrapper sequence

{{diagram:d2-wrapper}}

A linked wrapper calls the real allocator once, updates a fixed table and appends a PID-local event.

### Event and ACK transport

{{diagram:d2-transport}}

The C runtime writes a dedicated event file and separate terminal status; Rust consumes complete rows and checks ACKs.

### Fork and PID scope

{{diagram:d2-fork}}

A child resets inherited allocator state; an unlogged silent descendant remains unknown to the Rust reader.
<!-- END CHAPTER: content/engine-allocator-deep.md -->

<!-- BEGIN CHAPTER: content/engine-heap-deep.md -->
# 31 · Heap event reader and lifetime reconstruction

> **In one sentence:** This Rust code turns individual wrapper log lines into an ordered account of allocation lifetimes.

## Key terms

| Term | Plain meaning |
|---|---|
| **Event tailer** | A reader that takes new, complete lines from the event file rather than rereading all of it as a new history. |
| **Heap timeline** | The accumulated view of which tracked block was created, resized, freed or still observed. |

> **Example.** An allocated event starts ID 1. A later free closes ID 1. A new allocation can reuse the same address but must become ID 2; the timeline does not merge them.
>
> **What not to assume:** Missing lines or process acknowledgements mean the timeline has an observed prefix, not necessarily complete history.

## The big picture

The C wrapper produces PID-local allocation telemetry; Rust owns incremental input, checked sequence history, block lifetimes and final serialization. Read [the C runtime](engine-allocator-deep.html) for when a row is emitted and [independent diagnostics](engine-diagnostics-deep.html) for why a Valgrind error is not a primary-run allocation event. The GDB-followed inferior's snapshots and events from other processes have different observation scopes.

## Source file: `src/allocation_tracker.rs`

### Parsing and incremental input

`parse_event_line(line: &str) -> Result<RawEvent>` (`src/allocation_tracker.rs:82-160`) accepts exactly 10 whitespace-separated fields, or 12 only for calloc. It parses PID as `u32`, sequence/IDs/size/calloc operands as decimal `u64`, and addresses/caller as hexadecimal `u64`; unknown kind/status, malformed numbers, out-of-range PID and wrong field count yield `EngineError::AllocationEventParse`. `RawEvent` retains PID, sequence, kind/status, old and new IDs/addresses, caller PC and optional calloc operands (`:44-80`). A syntactically valid row is **not** by itself proof of a complete event stream.

`EventFileReader::new(path: PathBuf) -> Self` and `EventFileReader::read_new(&mut self) -> Vec<RawEvent>` (`:262-275,293-400`) track file offset, partial bytes, parse-error count and first 50 messages. Each call opens the event file, checks loss after prior observations (disappearance, smaller size, Unix device/inode replacement, seek/read failure), then reads at most 1 MiB plus a sentinel byte. Exceeding that per-stop cap keeps the bounded prefix but permanently quarantines subsequent input. Only newline-terminated lines parse; an unfinished tail up to 4096 bytes is buffered without rereading it from disk, and a larger line is rejected. `incomplete_line(&self) -> bool` reports a pending/oversized tail; `transport_damaged(&self) -> bool` reports definitive malformed/oversized/truncated input (`:277-284`). A short line can still be in flight at one stop; a trailing partial line at the **final** read means incomplete history. This reader is bounded, not a general-purpose recovery protocol for torn writes.

### Ordered timeline and ownership

`HeapTimeline::new(limits_max_blocks: usize) -> Self` sets a block cap and an event-retention cap of four times that value (`:431-486`). `HeapTimeline::ingest(&mut self, ev: &RawEvent)` (`:521-756`) uses `(pid,id)` and `(pid,address)` for live lookup. Per-PID sequences must begin at 1 and increase by one. Duplicates or out-of-order rows are counted but not ingested as state transitions; a gap is counted/recorded (up to 50 gap details) and marks that PID incomplete (`:523-566`). Malformed transport quarantines **all** PIDs, including future first-seen PIDs; after a PID gap it still retains subsequent raw-derived event records but no longer infers block changes for that PID. `history_complete(&self) -> bool` and `is_pid_complete(&self, pid: u32) -> bool` also reflect transport/retention limits (`:501-519`).

On a reliable prefix, successful malloc/calloc calls `add_block(&mut self, ev: &RawEvent, alloc_type: &'static str, realloc_from: Option<u64>)`, which rejects zero IDs/addresses, repeated IDs, overlapping live base addresses or exhausted block retention (`:583-604,768-808`). Overflow calloc is an `AllocationFailed` event with **unknown byte size**, preserving both operands; failed allocation creates no block. `retire_block(&mut self, idx: usize, seq: u64)` marks the block freed, stores its free sequence and subtracts live bytes (`:758-766`). `push_event(&mut self, event_type: HeapEventType, ev: &RawEvent, detail: Option<String>)` records the per-PID usage, raw caller and completeness qualification until the event cap (`:810-855`). Per-PID usage is an **observed** ledger, not total OS heap consumption.

`realloc` behavior is branch-specific (`:606-699`). `new` from NULL starts a block. An untracked predecessor that returns the **same** address is also `new`: if Rust has a live block for that PID/address (for example, C table overflow), it retires/links the observed predecessor; otherwise it starts at the first observed block. `ok_same` retires the old block and creates a linked new lifetime under the current wrapper's fresh ID; historical input using the old ID simply resizes its existing lifetime. `ok_moved` retires the address/ID-matched old block and links the new block with `realloc_from_id`. `zero` records the glibc `realloc(ptr,0)` free as a terminal `Reallocated` event with the old ID when found, **not** a new lifetime; `failed` records failure and preserves the old live block. The same numeric address can later have multiple historical block candidates: `address_candidates(&self, pid: u32, addr: u64) -> Vec<&BlockState>` deliberately returns all observed containing extents, not pointer-origin proof (`:882-895`).

For free (`:700-746`), `ok` retires the matching live block or marks the PID incomplete if none exists; `null` is an observed no-op. `unknown` may resolve by live `(pid,address)` despite a full C table. If it matches only a previously freed address, the detail says **possible** double free because unobserved reuse remains possible; otherwise it is an untracked address. `alive_blocks(&self, pid: u32) -> Vec<&BlockState>` returns no asserted live blocks when that PID is incomplete (`:861-873`). Caps and gaps preserve observed prefixes, not guaranteed final ownership.

### Transport and coverage

`verify_event_status(path: &Path, timeline: &HeapTimeline, inferior_pid: Option<u32>) -> Vec<String>` (`:178-260`) independently reads at most 1 MiB of status data. It requires a final newline and strict decimal `pid seq failures` lines, no PID zero or duplicate ACK; each observed event PID and the followed inferior (including one with zero events) must have one ACK with its last observed sequence and **zero** failures. It reports unexpected ACKs, mismatches and missing ACKs (up to 50 messages). It cannot discover a descendant with no events or ACK. An ACK cannot mend a malformed event stream; failure after the last logged row is detectable only because sequence/failure ACK checking is separate. At `src/trace.rs:478-483`, status errors quarantine the timeline, while retained events survive.

`pids_seen(&self)`, `needs_pid_widening(&self)` and `global_id(&self, pid: u32, local_id: u64) -> u64` (`:897-921`) leave IDs local for one observed PID; for multiple observed PIDs, nonzero IDs serialize as `(pid << 32) | local_id`, while zero stays zero. `ingest` flags any local ID above `u32::MAX` in a multi-PID history as `id_widening_overflow`, so completeness is false rather than claiming unique serialized identity (`:548-552`). Event sequence numbers remain local to the process; interleaved file order is not a universal heap clock.

### Caller location and final state

`resolve_caller_locations(binary: &Path, callers: &[u64]) -> Result<HashMap<u64, CallerInfo>>` batches distinct PCs (512 per `addr2line -f -C -e` invocation) with supervised output/deadline; unknown `??` or zero line leaves the caller unresolved (`:936-985`). `apply_caller_locations(timeline: &mut HeapTimeline, resolved: &HashMap<u64, CallerInfo>)` annotates blocks/events without replacing their raw return PCs (`:1004-1031`). Its caller `trace_program_inner` subtracts executable load bias and **one byte** from the recorded return address before resolution, then maps the resolved location back to the raw PC (`src/trace.rs:590-636`). A resolved file/line identifies an attributed call site, not which expression on a multi-call source line executed.

## Orchestration in `src/trace.rs`

`run_gdb_phase(...) -> Result<GdbPhaseResult>` receives the reader/timeline and paths (`src/trace.rs:1113-1124`). At each observed stop it calls `read_new`, quarantines damaged transport, ingests rows and records their count **before** capturing source state (`:1371-1405`). It performs one final read after termination for destructor/exit-handler events (`:1977-1991`). `trace_program_inner` checks terminal ACKs, distinguishes tracker timeout/cancellation/failure from completed collection (`:478-483,517-588`), and builds JSON events and heap state (`:851-943`). `final_events(timeline: &HeapTimeline) -> Vec<HeapEvent>` widens IDs as needed (`:2600-2614`); `backfill_snapshots(...)` attaches resolved locations and adjusted IDs without rewriting historical pointer observations (`:2564-2597`); `build_heap_state(...)` carries forward only bytes actually read at earlier snapshots, including historical bytes of freed blocks (`:2658-2694`). Event-to-checkpoint correlation reports **bracketing intervals**, not an exact stop or source expression (`:2616-2656`).

`heap_history.complete` requires timeline completeness, no parser errors/partial line, and a terminal debugger outcome (`:891-933`); status errors already quarantine the timeline. Its `process_scope="verified_observed_pids_only"`, `observed_event_pids`, `followed_inferior_pid` and `unobserved_process_coverage="unknown"` qualify that claim. An observed child missing an ACK makes it incomplete; a silent child cannot be inferred. Partial events remain available, with heap usage and lifetime status downgraded (`:860-870,979-1006`). Neither heap telemetry nor checkpoint bytes prove pointer origin or active union member.

## Tests and interpretation

The module's tests exercise parsing, calloc overflow and zero sizes, success/failure/zero realloc, address reuse, table-overflow fallback, PID widening, sequence gaps, transport corruption and caps (`src/allocation_tracker.rs:1053-1788`); `seeded_reuse_sequences_match_independent_live_byte_accounting` checks deterministic live-byte transitions (`:1133-1218`). Real wrapped C transport tests live in `tests/heap_instrumentation.rs:152-334`; `tests/heap_provenance.rs:48-90,131-185` checks ACK and observed/silent forks. The [release closure](../trace-engine-v2.2-release-closure.md#fork-child-heap-history-contract) confirms scoped completeness, not a census of all processes. The older [V2 contract](../v2.md#memory-and-value-validity) describes the recording background; where its in-place realloc wording differs, the current wrapper and timeline branches above govern.

## Diagram notes

### Event to lifetime

{{diagram:d2-lifetimes}}

Sequence checks precede block transitions; final lifetimes are reconstructed from retained events, not address equality.

### Realloc branches

{{diagram:d2-realloc}}

Failure, same-address success, moved success and terminal zero-size free produce different lifetime transitions.
<!-- END CHAPTER: content/engine-heap-deep.md -->

<!-- BEGIN CHAPTER: content/engine-diagnostics-deep.md -->
# 32 · Independent Valgrind and sanitizer diagnostics

> **In one sentence:** Diagnostics are additional, separate experiments on the C program. This page follows their launches and saved reports.

## Key terms

| Term | Plain meaning |
|---|---|
| **Independent execution** | A new run, distinct from the GDB run. Address and allocation order may change. |
| **Stage status** | A separate result for each tool: succeeded, failed, skipped or timed out, with partial evidence if available. |

> **Example.** A sanitizer reports a use-after-free at line 30. Preserve that report and its own status. Do not translate its pointer into a GDB-run allocation ID from a different execution.
>
> **What not to assume:** Failure to launch Valgrind is not evidence that Valgrind ran and found no bug.

## The big picture

Diagnostic passes rerun the program: they do not observe the same allocation lifetimes or memory addresses as GDB's primary run. The C instrumentation and verified PID-local event stream are described in [Allocator runtime](engine-allocator-deep.html) and [Heap timeline](engine-heap-deep.html). `trace_program_inner(config: TraceConfig) -> Result<DebugSessionTrace>` first builds a debug executable with the wrapper and a distinct sanitized executable (`src/trace.rs:226-237,313-320`), then schedules independent analyses after the GDB phase (`:640-849`). The program's execution outcome is not replaced by a Valgrind failure.

## Source file: `src/valgrind.rs`

### Launch and report ownership

`run_valgrind(binary: &Path, program_args: &[String], working_directory: Option<&Path>, limits: &ExecutionLimits, xml_path: &Path, _event_file: &Path, valgrind_path: Option<&str>) -> Result<ValgrindRun>` (`src/valgrind.rs:39-133`) executes `valgrind --tool=memcheck --leak-check=full --show-leak-kinds=all --track-origins=yes --xml=yes --xml-file=<path> <debug-binary> <args>`. An explicit tool path overrides PATH. It removes `C_TRACE_EVENT_FILE` so wrapper table roots cannot change leak classification; the `_event_file` argument is unused. Separate bounded stdout/stderr pipes and a separate XML file prevent client stderr from corrupting XML (`:48-79`); diagnostic resource limits, a process group and supervised deadline apply. `ValgrindRun` owns optional native report, failure reason, exit/timeout/cancellation, captured output and XML path (`:28-37`). Reading XML is capped by `min(max_trace_memory_bytes,16 MiB)` (`:85-106`); missing root or read/parse failure is a failure, not an empty clean report.

`parse_valgrind_xml(xml: &str) -> Result<ValgrindReport>` (`:168-262`) uses `roxmltree`, requires a `valgrindoutput` root, and marks native output complete only if a `FINISHED` status exists. It reads `heap_summary` (bytes/blocks at exit, allocations, frees, bytes allocated), `all_heap_blocks_freed`, and each `<error>` including leak-kind byte sums. Native missing numbers remain nullable in `observed_summary`; legacy numeric totals can default to zero and **must not** be read as observed zero (`:179-258`). `parse_native_u64(text: &str) -> Option<u64>` strictly validates comma-grouped Valgrind counters such as `4,152` (`:271-286`). `parse_error(node: roxmltree::Node) -> ValgrindError` captures kind, description, unique ID, nullable leaked bytes/blocks, primary and auxiliary stacks; `parse_frame(node: roxmltree::Node) -> ValgrindFrame` retains IP/module/function/path/line when available (`:288-374`). Valgrind supplies native errors and summary, **not** a time-indexed GDB heap history.

`recover_errors(xml: &str) -> Option<ValgrindReport>` (`:139-166`) scans for complete `<error>...</error>` fragments if the full XML parse fails, parses each fragment as XML, and returns only complete native errors. It never fabricates a heap summary; `parse_error`/`output_complete` qualify recovery. If nothing complete survives, there is no report. A timed-out run may have a partial report, never an automatically clean one (`run_valgrind:85-133`; `src/trace.rs:705-757`).

### Source-site candidates, not cross-run IDs

`pub fn correlate_with_allocations(report: &mut ValgrindReport, allocation_sites: &[(String, u32, u64)])` (`src/valgrind.rs:376-429`) is a retained public entry point, **not called by `trace_program_inner`**. It ignores tuple allocation IDs, clears existing `related_allocation_ids`, and, only when an absolute `<dir>/<file>` and an absolute primary source path have equal path and line (canonicalized where possible), emits an ID-free `source_site_candidates` entry with `evidence="same_source_site_independent_run_not_allocation_identity"`. Different directories with the same basename, relative paths and matching addresses cannot prove the allocation. Old imported reports may still contain legacy IDs until explicitly re-correlated; the production pipeline does not generate these links (`src/trace.rs:711-716`). The [later release closure](../trace-engine-v2.2-release-closure.md#f14-correction-of-the-premise-failure-repair) supersedes the earlier [conditional audit](../trace-engine-v2.2-final-audit.md#1-executive-assessment-and-verdict) on this helper. No exact cross-execution allocation-event ID is available.

## Source file: `src/sanitizer.rs`

### Triggered separate execution

`run_sanitized(binary: &Path, program_args: &[String], working_directory: Option<&Path>, limits: &ExecutionLimits) -> SanitizerReport` (`src/sanitizer.rs:20-80`) starts the separately built sanitized binary with the same args/directory, null stdin, bounded piped stdout/stderr, process group, diagnostic limits and supervised deadline. ASan replaces the allocator, so its addresses/events cannot be merged with wrapped GDB telemetry (`:1-13`). Spawn failure produces `reproduction="skipped"` and an explanation; otherwise the report includes native exit/timeout/cancellation, UTF-8-lossy output excerpts (stdout 4096, stderr 8192 bytes) and an `output_complete` flag based on truncation/timeout/cancellation (`:38-69`). `truncate(s: &str, cap: usize) -> String` respects UTF-8 boundaries (`:82-92`).

`parse_sanitizer_output(stderr: &str, stdout: &str, report: &mut SanitizerReport)` (`:94-160`) prefers `SUMMARY: AddressSanitizer:`; otherwise looks for ASan `ERROR:`/marker, then UBSan `runtime error:`. It records category, message, source file/line/column when parsable, and up to 32 textual stack frames; `extract_asan_category(line: &str) -> String` and `extract_location(line: &str) -> Option<(String,u32,Option<u32>)>` perform bounded text extraction (`:188-211`). Matching a report marker makes the native pass `reproduced`; absent a marker, timeout/cancellation/nonzero exit is `inconclusive`, while a successful clean exit is `completed_normally` (`:69-79`). These are outcomes of the **sanitized run**, not proofs that GDB's crash reproduced. Text parsing may have missing location evidence.

`matches_crash(report: &SanitizerReport, crash: &CrashReport) -> bool` (`:162-186`) compares recorded report/stack file+line against primary-crash backtrace file+line, accepting exact text or successfully canonicalized equal paths. It does not compare addresses/PIDs or establish exact operation identity. In `run_sanitizer_pass(...) -> Option<SanitizerReport>` (`src/trace.rs:49-106`), a native `reproduced` diagnostic becomes `diagnostic_observed` unless it matches an existing primary crash; only a matched crash receives `asan_summary`. Stage health distinguishes completed, failed, timed-out, cancelled and skipped passes. A matching site supports a crash correlation, not equivalence of run histories.

## Scheduling in `src/trace.rs`

`SanitizerTrigger` offers `never`, `on_crash`, default `on_crash_or_valgrind_error`, and `always` (`src/config.rs:155-168`). If crash/`always` already meets the trigger and the sanitized build succeeded, `trace_program_inner` runs sanitizer **before** Valgrind so a slow memcheck cannot consume its session window (`src/trace.rs:644-680`). Otherwise it runs Valgrind when installed and within deadline, preserving native errors without allocation-ID correlation (`:682-785`); for a clean primary run in the default trigger, Valgrind errors can then request sanitizer (`:787-849`). The two passes never share an inferior process. Missing Valgrind, failed sanitizer build, exhausted deadline or cancellation produces an explicit skip/failure reason rather than a negative diagnostic (`:392-414,777-847`). `trace.stage_results`, `sanitizer_report` and `valgrind_report` are assembled independently (`:1007-1009`). The V2.1 [recording contract](../v2.md#deadlines-processes-and-stage-health) describes stage-level budgets.

## Tests and caveats

`src/valgrind.rs:439-622` tests grouped/missing counters, complete-error recovery from truncated XML, UAF/leak fixture stacks and leak sizes, invalid XML, and site-only correlation including same-basename collisions and legacy IDs. `src/sanitizer.rs:241-297` tests ASan overflow, UBSan location, clean output and Unicode-safe truncation. `tests/release_closure_8dr.rs` supplies a real GCC/Valgrind numeric-address collision oracle; equal numeric mappings across processes are **not malloc identity**, and current CLI Valgrind errors retain zero primary allocation links ([release findings](../trace-engine-v2.2-release-closure.md#f14-correction-of-the-premise-failure-repair)). Neither a missing XML file, a truncated report nor a clean sanitized rerun proves absence of a bug in the primary run. There is no supported exact diagnostic-to-GDB heap-event join.

## Diagram notes

### Independent diagnostic processes

{{diagram:d2-diagnostics}}

Valgrind and sanitizer runs produce separate reports and never become primary-run allocation identities.
<!-- END CHAPTER: content/engine-diagnostics-deep.md -->

<!-- BEGIN CHAPTER: content/engine-fusion-deep.md -->
# 33 · Evidence fusion: bounded links between existing records

> **In one sentence:** Fusion adds *references between existing records*. It does not rewrite the values GDB saw or invent an executed source expression.

## Key terms

| Term | Plain meaning |
|---|---|
| **Correlation** | Finding a carefully checked way to point from one provider's record to another provider's record. |
| **Candidate** | A plausible possible match that does not meet the requirements for an exact same-execution link. |

> **Example.** A GDB variable can carry a unique DWARF declaration ID and then point to a matching Clang declaration. If two declarations compete, the link is left unset instead of choosing the first one.
>
> **What not to assume:** A Clang `malloc` on the same line as a heap event is still only a source-call candidate.

## The big picture

`src/evidence/` reconciles optional GCC DWARF, optional Clang syntax and recorded GDB/heap evidence. It does **not** run GDB or clone registries per checkpoint; it adds optional references, never overwrites observed values, addresses, extents, activation IDs or allocation lifetimes ([evidence/mod.rs:1-11](../../src/evidence/mod.rs#L1-L11)). `trace_program` calls `dwarf::associate` after heap history/observations are assembled, then `evidence::fuse(&mut trace)` exactly once in the normal pipeline ([trace.rs:944-969](../../src/trace.rs#L944-L969)). The extraction contracts are detailed in [DWARF](engine-dwarf-deep.html) and [Clang AST](engine-ast-deep.html).

## `mod.rs`: orchestrator, state and accounting

`fuse(trace: &mut DebugSessionTrace)` returns immediately if both static registries are absent; otherwise it derives a working directory from session metadata or process cwd, initializes `EvidenceFusionSummary`, and treats AST as available only if a TU is `complete` or `partial` ([evidence/mod.rs:18-50](../../src/evidence/mod.rs#L18-L50)). If Clang marked cross-provider semantics unavailable, it skips DWARF/AST member and expression links, runtime-source links and heap-call correlation, even if syntax remains retained. Otherwise it borrows the trace's DWARF registry and mutably borrows source metadata to build one transient `DwarfIndex`, link fields and expressions, then borrows metadata for the runtime bridge and mutably borrows it for heap-call correlation. It always audits existing optional refs, counts *remaining* links, and writes elapsed fusion/validation microseconds to both profiling and summary ([evidence/mod.rs:51-91](../../src/evidence/mod.rs#L51-L91)). `count_links` counts declaration, type, expression, member and candidate/ambiguity refs plus visited runtime variables across top frame, caller frames and globals; it does not claim each counted link proves a syntax node executed ([evidence/mod.rs:93-152](../../src/evidence/mod.rs#L93-L152)). The missing-provider test distinguishes absent summary, empty syntax registry, and one usable partial TU ([tests/evidence_fusion.rs:137-178](../../tests/evidence_fusion.rs#L137-L178)).

## `declarations.rs`: CU-scoped type and member selection

`DwarfIndex::new(&TypeRegistry, &Path)` borrows `TypeEntry`s and builds ID, canonical CU-file-to-unit, and `(unit ID, kind/name)` indexes. `unit_file` resolves a CU name against its compilation directory or cwd; artificial/empty names cannot be matched. `unique_unit`, `unique_record` and `unique_member` require one CU for the AST TU file, one complete named struct/union in that CU, then one member with the requested name ([evidence/declarations.rs:13-86](../../src/evidence/declarations.rs#L13-L86), [evidence/declarations.rs:155-187](../../src/evidence/declarations.rs#L155-L187)). `member_link(&self, file, parent_spelling, member_name) -> Option<(DwarfMemberRef, Option<&str>)>` stores the parent type ID, index in the parent's serialized member vector, name and recorded byte offset; it returns the member's own type only if that type ID exists. `unique_type` is a separate fallback for one non-declaration-only named type in that same CU. `spelling_key` strips simple qualifiers and understands `struct`/`union`/`enum` tags but rejects pointers, arrays, function forms and multiword base spellings instead of parsing C types by guesswork ([evidence/declarations.rs:88-130](../../src/evidence/declarations.rs#L88-L130), [evidence/declarations.rs:189-221](../../src/evidence/declarations.rs#L189-L221)). `link_source_members(&mut SourceMetadataRegistry, &DwarfIndex)` applies that resolution only to `FieldDecl`s with a parent spelling, recording `unique_parent_type_and_member_name` and any valid member type ([evidence/declarations.rs:132-153](../../src/evidence/declarations.rs#L132-L153)). Tests demonstrate `struct Pair.x` -> member index 0/offset 0, rejection of pointer spellings, and refusal to choose between two complete `Pair` records ([evidence/declarations.rs:269-332](../../src/evidence/declarations.rs#L269-L332)). A member's byte offset is DWARF layout, not evidence of the active member of a union.

## `source_links.rs`: expression and stopped-variable bridges

`link_expressions(&mut SourceMetadataRegistry, &DwarfIndex)` first indexes AST unit files and declaration IDs. For each expression it clears previous cross-provider links, then follows an existing `referenced_declaration` to its correlated DWARF declaration/type and optional member. A `MemberExpr` without a member link may use its AST base type spelling and member name through `member_link`. If type remains unknown, `sizeof(type)` operand spelling, then plain expression type spelling, can resolve through `unique_type` within the TU's GCC CU; `dwarf_type_evidence` records whether the path was declaration, member, `sizeof` or unique name. This selection is static typing evidence, not runtime execution ([evidence/source_links.rs:15-93](../../src/evidence/source_links.rs#L15-L93)). The real GCC/Clang integration test asserts valid type/member references and a JSON schema-2 roundtrip ([tests/evidence_fusion.rs:270-353](../../tests/evidence_fusion.rs#L270-L353)).

`link_runtime(Option<&SourceMetadataRegistry>, &mut [ExecutionSnapshot]) -> usize` builds a borrowed DWARF-declaration-ID -> optional AST-declaration-ID bridge. Two AST declarations claiming the same DIE replace the bridge value with `None`, never a first-match winner. It clears and attaches only `VariableInfo.source_declaration_ref` when the variable already has that **exact serialized** `declaration_ref`, visiting top-frame variables, caller frames and globals. It never uses a variable's address or a coincident source line as the runtime bridge ([evidence/source_links.rs:95-144](../../src/evidence/source_links.rs#L95-L144)). The duplicate-claim test with activation IDs 7 and 8 shows the collision leaves both source refs unset; removing the duplicate links the first observation without changing either activation ([tests/evidence_fusion.rs:469-535](../../tests/evidence_fusion.rs#L469-L535)).

## `heap.rs`: explicit delegation, not a second matcher

`correlate(registry: &mut SourceMetadataRegistry, events: &[HeapEvent], cwd: &Path)` is the entire heap fusion module: it delegates directly to `clang_ast::correlate_heap_calls` ([evidence/heap.rs:1-10](../../src/evidence/heap.rs#L1-L10)). That callee indexes canonical source lines of direct and expanded allocator calls, requires resolved return-address-minus-one attribution and compatible event operation, and returns either a unique-direct **candidate** or explicit multiple/macro-only ambiguity. It never attaches an allocation ID to a syntax expression or identifies which same-line call executed ([clang_ast.rs:947-1054](../../src/clang_ast.rs#L947-L1054), [tests/evidence_fusion.rs:355-467](../../tests/evidence_fusion.rs#L355-L467)). E.g., two `free` calls at one resolved line yield `multiple_compatible_calls_on_resolved_line`, not an exact event-expression link; macro-only `free` yields `only_macro_expanded_calls_on_resolved_line` ([tests/evidence_fusion.rs:390-440](../../tests/evidence_fusion.rs#L390-L440)).

## `validation.rs`: construction-time reference audit

`audit(&mut Option<SourceMetadataRegistry>, Option<&TypeRegistry>, &mut [ExecutionSnapshot], &[HeapEvent]) -> usize` builds borrowed sets/maps of existing DWARF declaration and type IDs, then clears dangling AST declaration/type refs and invalid `DwarfMemberRef`s. `valid_member` verifies that parent is a struct/union and that the indexed serialized member has the same name **and byte offset**; an invalid FieldDecl member also clears its field type, while a member-derived expression type/evidence is cleared with an invalid member. Expression type evidence is absent when its type is absent ([evidence/validation.rs:15-107](../../src/evidence/validation.rs#L15-L107), [evidence/validation.rs:183-197](../../src/evidence/validation.rs#L183-L197)). The audit removes heap candidates with missing event/expression IDs, prunes ambiguity expression IDs and drops empty/out-of-range ambiguities. Finally it clears a runtime `source_declaration_ref` unless the source declaration still exists **and** its DWARF declaration equals the observed variable's `declaration_ref`, in top frame, caller frames and globals. The returned rejection count is recorded by `fuse`; this is an in-memory construction audit, not the import-time Trace Parser's deeper structural/relation validation ([evidence/validation.rs:108-181](../../src/evidence/validation.rs#L108-L181), [evidence/mod.rs:77-90](../../src/evidence/mod.rs#L77-L90), [tests/evidence_fusion.rs:555-627](../../tests/evidence_fusion.rs#L555-L627)). For example, a hand-edited trace whose *existing* expression ID points to the wrong declaration or whose valid call ID has an incompatible source line is caught by parser deep validation, not necessarily by this existence-focused construction audit ([tests/evidence_fusion.rs:629-767](../../tests/evidence_fusion.rs#L629-L767), [parser validation](parser-validation.html)).

## Partiality and invariants

No DWARF or AST means no summary; DWARF-only can still produce a summary with `ast_available=false`. Missing/failed Clang or unverified GCC/Clang flags preserve source syntax or absence as appropriate, while cross-provider links are withheld ([evidence/mod.rs:33-75](../../src/evidence/mod.rs#L33-L75), [tests/clang_ast.rs:556-597](../../tests/clang_ast.rs#L556-L597)). AST-on/off tests compare execution outcome, event shape and normalized runtime observations while allowing only additive metadata differences; that comparison checks preservation, not an independent proof of runtime truth ([tests/evidence_fusion.rs:769-891](../../tests/evidence_fusion.rs#L769-L891)). A unique compiler declaration is not a unique dynamic activation; a source initializer does not prove initialization; a source-line heap candidate is not the event's executed expression. See [metadata overview](engine-metadata.html) and [identity contracts](identity-contracts.html).

## Diagram notes

### Fusion inputs and audit

{{diagram:d2-fusion}}

Existing DWARF, AST and runtime records are matched conservatively; invalid references are removed before publication.
<!-- END CHAPTER: content/engine-fusion-deep.md -->

<!-- BEGIN CHAPTER: content/engine-supervision-deep.md -->
# 34 · Deadlines, child ownership, and resource limits

> **In one sentence:** The supervisor watches tools and deadlines so a failed child process does not masquerade as a completed trace.

## Key terms

| Term | Plain meaning |
|---|---|
| **Deadline** | The maximum allowed time for a supervised task or session. |
| **Process group** | A way to manage a tool and its child processes together when stopping or cancelling work. |

> **Example.** If GDB stops responding, the supervisor records a timeout and tries to clean up its process group. Any already captured snapshots remain partial observations.
>
> **What not to assume:** Time, journal, JSON export, parser input and assembled process memory are different resource limits.

## The big picture

Stage 1 uses a monotonic wall deadline, explicitly owned subprocesses, bounded captured output, and Linux inferior limits. These mechanisms limit accidental runaway work; they do not sandbox C code or certify that all forked processes were observed. [Entry](engine-entry-deep.html) starts the pipeline; [compiler](engine-compiler-deep.html) and diagnostic stages use the supervised subprocess path.

## `src/supervisor.rs`: deadline and cancellation context

**Principal signatures:** `pub async fn scope<T>(seconds: u64, future: impl Future<Output = T>) -> T` (`src/supervisor.rs:106-132`), `pub async fn scope_until<T>(end: Instant, future: impl Future<Output = T>) -> T` (`src/supervisor.rs:134-136`), `pub fn deadline() -> Instant`, `pub fn remaining() -> Duration`, and `pub fn stage_deadline(seconds: u64) -> Instant` (`src/supervisor.rs:138-153`). `scope` installs task-local `DEADLINE` and cancellation token, plus a signal task listening for SIGINT/SIGTERM; the signal task is aborted on drop. It does not itself wrap `future` in `tokio::time::timeout`: callers use deadlines in waits and commands. `scope_until` clamps a stage to the earlier of its own end or the enclosing deadline. If called outside a scope, `deadline()` supplies a 300-second fallback; do not treat that as the configured CLI timeout. `trace_program` validates limits *before* entering `scope(config.limits.timeout_seconds.max(1), ...)` (`src/trace.rs:131-165`), and stages clamp their own budgets afterward. `request_cancel()` wakes `cancellation()` via `Notify`; outside the scope `cancelled()` is false and `cancellation()` is pending (`src/supervisor.rs:76-104`). `timeout_error(operation: &str) -> EngineError` names the exhausted operation (`src/supervisor.rs:155-159`).

## `src/supervisor.rs`: subprocess lifecycle

**Principal types/functions:** `OwnedChild::spawn(cmd: &mut Command) -> io::Result<Self>` and `OwnedChild::terminate(&mut self)` (`src/supervisor.rs:161-198`); `ProcessOutput { cancelled, status, stdout, stderr, timed_out, output_truncated }` (`src/supervisor.rs:200-207`); `pub async fn output(cmd: &mut Command, end: Instant, cap: usize) -> Result<ProcessOutput>` (`src/supervisor.rs:226-304`). `spawn` requests a new process group, `kill_on_drop(true)`, and Linux subreaper ownership. `output` rejects already-cancelled/expired launches, nulls stdin, pipes both output streams, and concurrently drains each to **its own** `cap` while continuing to read/discard bytes beyond the cap. It waits for the direct child's status *and* both pipes; a successful parent is not allowed to leave a grandchild holding a pipe open (`src/supervisor.rs:209-280`).

```rust
tokio::select! {
    _ = cancellation() => { was_cancelled = true; owned.terminate().await; break; }
    _ = &mut stdout, if !out_done => out_done = true,
    _ = &mut stderr, if !err_done => err_done = true,
    result = owned.child.wait(), if status.is_none() => { /* reap and kill group */ }
    _ = tokio::time::sleep_until(end.into()) => {
        timed_out = true; owned.terminate().await; break;
    }
}
```

This is the decision point at `src/supervisor.rs:253-280`, abbreviated only in the parent-exit arm. On cancel/timeout `terminate` kills descendant process groups and the leader, waits/reaps for up to two seconds, and offers each unfinished pipe a further bounded 100 ms to retain already produced bytes (`src/supervisor.rs:175-187,281-303`). `Drop` also starts killing a group on early unwinding, but cannot promise the same asynchronous reaping as explicit termination. `enable_subreaper`, `descendants`, and `reap_descendants` (`src/supervisor.rs:12-59`) inspect Linux `/proc` with a defensive 4,096-descendant scan bound; they target known PIDs, not `waitpid(-1)` which might steal other Tokio children's status. `output_truncated` reports capture-cap exhaustion; `status` can remain `None` on cancellation/timeout. `stage(started: Instant, begin: Instant, status: StageStatus, exit: Option<i32>, reason: Option<String>, complete: bool, partial: bool) -> StageResult` records timing/status/completeness, not proof of total process-tree coverage (`src/supervisor.rs:306-325`; `src/models.rs:12-44`). Tests cover cancellation waking a stalled wait while retaining partial stdout, timeout with inherited pipes, timeout output, and a parent that exits before its grandchild (`src/supervisor.rs:327-409`).

## `src/resource_limits.rs`: configured budgets versus enforcement

**Principal type/function:** `ExecutionLimits` and `pub fn validate(&self) -> Result<()>` (`src/resource_limits.rs:48-198`). Defaults include 1,000 steps/snapshots, ten seconds whole-session wall time, 256 MiB inferior address space, 1 MiB captured program output, 10,000 allocations, 16 child processes, 128 MiB journal budget, 32 MiB resident-record accounting, and **separate** 128 MiB final JSON budget (`src/resource_limits.rs:120-149`). `validate` rejects a zero step/snapshot/read budget, zero or over-one-year timeout, inferior memory below 16 MiB or overflow, and zero command/journal/resident/export budgets; other counters need not all be positive (`src/resource_limits.rs:152-198`). These limits are not interchangeable: `max_trace_bytes` is journal accounting; `max_json_export_bytes` counts serialized UTF-8 including newline; `max_trace_memory_bytes` is not total Rust RSS. The [final audit](../stage1-final-audit.md) measured a 1,961-byte capped journal with a 22,883-byte final JSON and a separate export-cap failure with no emitted JSON.

**Enforcement signatures:** `pub fn apply_pre_exec_limits(cmd: &mut Command, limits: &ExecutionLimits)` (`src/resource_limits.rs:200-225`) installs RLIMIT_AS, CPU, FSIZE, NOFILE and CORE via a pre-exec hook for a directly launched inferior. `pub fn apply_diagnostic_limits(cmd: &mut Command, limits: &ExecutionLimits)` omits RLIMIT_AS for tool/ASan compatibility (`src/resource_limits.rs:227-249`). `pub fn limit_stopped_inferior(pid: u32, limits: &ExecutionLimits) -> io::Result<()>` applies `prlimit` to the GDB-stopped inferior, *not GDB*, after its initial main breakpoint; startup before that stop is not covered by this action (`src/resource_limits.rs:251-276`). These functions offer different paths; do not imply all three are applied to every child. `set_rlimit` intentionally ignores individual kernel failures and other limits remain in play (`src/resource_limits.rs:278-291`). `RLIMIT_NPROC` is deliberately absent because it is per real user, not per inferior tree (`src/resource_limits.rs:36-41,220-222`). Fixed file/open-file caps are 64 MiB and 256 descriptors (`src/resource_limits.rs:29-34`). `set_process_group` and `kill_process_group` arrange a group and send SIGKILL to group/leader best-effort (`src/resource_limits.rs:293-307`).

**Optional cgroup:** `CgroupGuard::create(limits: &ExecutionLimits) -> Option<CgroupGuard>`, `attach(&self, pid: u32) -> bool`, and `kill_all(&self)` (`src/resource_limits.rs:309-380`). Creation needs a writable delegated cgroup v2 subtree; it writes `memory.max` and `pids.max`, returning `None` if unavailable. Attachment can fail, `cgroup.kill` requires kernel support, and drop only attempts directory removal. A configured process count is therefore **best effort**, not an unconditional per-tree enforcement claim. No unit tests live in `resource_limits.rs`; the existing suite's subprocess/resource regressions and the [V2 recording contract](../v2.md) exercise and qualify the integrated behavior. The [release closure](../trace-engine-v2.2-release-closure.md) further narrows heap completeness to verified observed PIDs plus the followed inferior; a silent fork remains unknown.

## Diagram notes

### Resource boundaries

{{diagram:d2-limits}}

Deadline, output, GDB, journal, transport and export controls operate at different points and do not form one global memory bound.
<!-- END CHAPTER: content/engine-supervision-deep.md -->

<!-- BEGIN CHAPTER: content/engine-export-deep.md -->
# 35 · Trace model, validation, journal, and JSON export

> **In one sentence:** Recording in memory, writing a stream during the run and exporting a final JSON file are three separate steps.

## Key terms

| Term | Plain meaning |
|---|---|
| **JSONL journal** | One JSON object per line written while the program is observed; it can preserve a prefix of events. |
| **Preflight** | A size check before committing the final JSON file, so an oversized export is rejected before replacing an existing file. |

> **Example.** If the final JSON would exceed its byte limit, export stops without presenting a partly valid final document. If the journal stopped earlier, it still cannot rebuild final reports it never contained.
>
> **What not to assume:** A final-document byte cap does not constrain the trace already held in RAM, and a successful file commit cannot be undone by a later stdout error.

## The big picture

The engine constructs an owned schema-v2 `DebugSessionTrace` during collection. Structural validation, append-only JSONL recovery, and final JSON export are separate operations with separate budgets and failure semantics. The journal cannot reconstruct every final report; the final JSON is what the [schema reference](schema.html) and downstream consumer load.

## `src/models.rs`: owned trace and evidence domains

**Principal types:** `DebugSessionTrace` (`src/models.rs:499-540`), `RecordingData` (`src/models.rs:734-755`), `ExecutionSnapshot` (`src/models.rs:65-94`), `InstructionEvent` (`src/models.rs:698-723`), `HeapEvent` (`src/models.rs:248-285`), `HeapHistoryCoverage` (`src/models.rs:757-782`), `ExecutionOutcome` and `ProgramOutcome` (`src/models.rs:925-950`). `DebugSessionTrace` owns vectors of snapshots, heap events and final heap state, status, reports, notes, effective configuration/limits, and independent `StageResults`; `recording` is `#[serde(flatten)]`, so its instruction/disassembly/checkpoint/coverage/storage fields serialize at top level rather than as a nested JSON object (`src/models.rs:501-539`). Its `Default` constructs `schema_version: 2`, empty evidence and `ExecutionFailure`, **not** a successful trace (`src/models.rs:897-923`). `trace_program_inner` initializes this value with effective tracing configuration and limits, later fills compilation status and early-returns a `CompileFailed` trace with no synthetic snapshots when GCC rejects input (`src/trace.rs:196-200,291-350`). A status enum is a recorded outcome, not validation of absent observations.

```rust
pub struct DebugSessionTrace {
    #[serde(default, flatten)]
    pub recording: RecordingData,
    pub schema_version: u32,
    pub snapshots: Vec<ExecutionSnapshot>,
    pub heap_events: Vec<HeapEvent>,
    pub execution_status: ExecutionStatus,
    pub stage_results: StageResults,
    // Additional reports, configuration and evidence fields in src/models.rs:501-540.
}
```

This is a **field projection**, not a compilable redefinition. `source_checkpoints` are indices into the snapshots vector, `instruction_count` counts attempted events even when instruction events are not retained, and `event_id` differs from a disassembly `instruction_id` (`src/models.rs:699-755`). `VariableInfo.address` is pointer storage whereas `points_to` is its target; `PointerTarget.address_match_allocation_id` is a numeric address candidate, **not** allocation origin (`src/models.rs:118-186,605-633`). `HeapHistoryCoverage.complete` applies to verified observed event PIDs plus the followed inferior, not all forked descendants (`src/models.rs:757-781`). `ScopeStatus::Active` and readable bytes do not prove initialization (`src/models.rs:579-603`). Optional static registry/source links never prove an expression executed. `schema_tests::legacy_trace_deserializes_as_schema_v2_with_unstarted_stages` exercises defaults for old optional fields (`src/models.rs:952-985`); the [final audit](../stage1-final-audit.md) qualifies the supported historical corpus, not all older files.

## `src/processor.rs`: structural check only

**Signature:** `pub fn validate(trace: &DebugSessionTrace) -> Result<(), String>` (`src/processor.rs:6-43`). It borrows the trace without rewriting events or simulating registers. For each *resident* instruction event it requires `event_id == vector position`; when `instruction_id` exists it indexes `disassembly_table` and checks the stored address matches; it bounds any `checkpoint_id` against `snapshots`. If events are nonempty it compares their length with `instruction_count`, then checks every `source_checkpoints` index exists and its snapshot's `instruction_position` does not exceed the count (`src/processor.rs:7-41`).

```rust
if !data.instruction_events.is_empty()
    && data.instruction_count != data.instruction_events.len() as u64
{
    return Err("instruction count mismatch".into());
}
for id in &data.source_checkpoints {
    let checkpoint = trace.snapshots.get(*id).ok_or("invalid source checkpoint")?;
    if checkpoint.observation.instruction_position > data.instruction_count {
        return Err("checkpoint beyond timeline".into());
    }
}
```

This excerpt is `src/processor.rs:28-41`. In journal-only mode an empty resident event array is not itself a mismatch. Passing this limited check does **not** prove heap provenance, execution completeness, or every semantic reference's integrity; use the [parser validation guide](parser-validation.html) for imported semantic evidence. The unit `broken_reference_and_duplicate_event_id_are_rejected` tests noncontiguous IDs and invalid disassembly references (`src/processor.rs:45-76`).

## `src/storage.rs`: ordered JSONL, not final JSON

**Principal type/signatures:** `Journal { file: Option<File>, bytes: u64, cap: usize }`; `Journal::new(path: Option<&Path>, cap: usize) -> Result<Self>`; `append(&mut self, kind: &str, value: &impl Serialize) -> Result<usize>`; `finalize(&mut self) -> Result<()>`; and `recover(path: &Path) -> Result<impl Iterator<Item = Result<serde_json::Value>>>` (`src/storage.rs:8-70`). A path is opened with `create_new`, so it never overwrites an existing journal; `None` still counts serialized bytes against the cap. `append` wraps a value as `{"kind": ..., "data": ...}`, serializes a complete newline-terminated record, checks `max_trace_bytes` before writing, and increments `bytes` after `write_all` (`src/storage.rs:25-43`). `finalize` appends a `finalized` footer **outside the data cap** and calls `sync_data`; the footer denotes journal closure, not completed execution. A write failure can still leave a partial last line, and no `sync_data` occurs on each append (`src/storage.rs:44-51`).

`recover` incrementally reads with a 16 MiB per-read bound, ignores a final line without newline, and yields per-record JSON or I/O errors; it does not reconstruct a full `DebugSessionTrace` from a partial journal (`src/storage.rs:54-70`). Tests `bounded_and_no_overwrite` and `torn_last_record_is_recoverable_and_existing_file_is_not_overwritten` cover the cap, create-new behavior, and incomplete-last-line handling (`src/storage.rs:72-97`). The [V2 contract](../v2.md) describes journal-only capture and retained-record accounting; `storage.finalized` is not a coverage flag.

## `src/json_export.rs`: count first, commit file, then stdout

**Principal signatures:** `preflight<T: Serialize>(trace: &T, pretty: bool, limit: usize) -> io::Result<usize>` (`src/json_export.rs:72-76`); `export_json<T: Serialize, W: Write>(trace: &T, pretty: bool, limit: usize, file: Option<&Path>, stdout: &mut W) -> io::Result<usize>` and its cancellation-aware form `export_json_cancellable<T: Serialize, W: Write>(trace: &T, pretty: bool, limit: usize, file: Option<&Path>, stdout: &mut W, cancel: Option<&AtomicBool>) -> io::Result<usize>` (`src/json_export.rs:139-168`). `serialize_limited` streams serde JSON to a `BoundedWriter`, counts actual UTF-8 output including pretty whitespace and final newline, checks the byte budget on writes, and optionally checks cancellation (`src/json_export.rs:9-70`). `preflight` uses `io::sink()` so it counts without allocating a full JSON string or touching either destination. It does not cap the already assembled trace in memory.

```rust
let bytes = serialize_limited(trace, pretty, limit, io::sink(), cancel)?;
if let Some(path) = file {
    write_file_atomic(trace, pretty, limit, path, cancel)?;
}
serialize_limited(trace, pretty, limit, stdout, cancel)?;
Ok(bytes)
```

This is the actual ordering at `src/json_export.rs:162-168`. For a file, `create_sibling` makes a unique same-directory `create_new` temp file (up to 16 names), serialization writes through a `BufWriter`, flushes and `sync_all`s the file, checks cancellation, then renames over the target (`src/json_export.rs:78-137`). `TempPath::drop` removes an uncommitted temp on failure; an existing target is preserved on preflight/temporary-write failure. The rename commits the file before stdout starts; stdout I/O or cancellation *after* that point cannot roll back the committed file, and stdout itself can be partially written on I/O error. Repeated serialization assumes stable trace contents: a custom stateful `Serialize` implementation could change between preflight and destinations, so preflight is not a transaction over arbitrary mutable serializers. The CLI maps export errors to exit 1 (`src/main.rs:50-77`). Tests cover exact pretty/compact limits and UTF-8, legacy round-trip, preserving the existing file on one-byte excess or serialization error, broken stdout pipe with valid committed file, disk-full stdout, cancellation before rename, and unwritable file preventing stdout (`src/json_export.rs:170-413`).

The [final audit](../stage1-final-audit.md) confirms an 8,192-byte JSON export limit rejected a larger trace before emitting JSON; that result says nothing about trace-construction RAM or general stdout atomicity.

## Diagram notes

### Producer-side ownership

{{diagram:d2-model}}

Provider-owned records converge in one schema-2 `DebugSessionTrace` before export.

### JSON export ordering

{{diagram:d2-export}}

Counting preflight rejects oversized output before file commit; committed file output cannot be rolled back by a later stdout error.
<!-- END CHAPTER: content/engine-export-deep.md -->

<!-- BEGIN CHAPTER: content/engine-walkthroughs.md -->
# 36 · Engine subsystem walkthroughs

> **In one sentence:** Each section follows one actual route through several engine files. Read it as a story of what data changes hands.

## Key terms

| Term | Plain meaning |
|---|---|
| **Call path** | The sequence of checked functions and tool boundaries used in a particular explanation. |
| **Intermediate data** | A record made between input and output, such as a GDB stop or a wrapper event line. |

> **Example.** For an allocation story, follow a C `malloc` call to the wrapper's file, then the Rust reader, then the saved heap event. At each step ask what might have been missed.
>
> **What not to assume:** An example route does not assert that optional stages ran in every execution or that a source candidate proves a runtime call.

## The big picture

These are six **illustrative source-guided sequences**, not a transcript of one executed trace. A C statement in an example is not proof that it ran: only a recorded stop, allocation event, or diagnostic from its own execution supports the corresponding claim. The [Stage 1 final integrated audit](../stage1-final-audit.md) is **historical evidence, not a rerun for this page**; its fixture counts and outcomes must not be read as measurements of a new invocation. Each walkthrough stops at an engine subsystem boundary rather than following one program end to end.

## A. Compiling a C project into two different artifacts

### Example and transition

`fixtures/c/phase8b_defined.c:19-27` contains `int *heap = malloc(3 * sizeof(int));` followed by a checked `realloc`. This source illustrates a build input, **not** a claim that either call executed. `src/main.rs:24-34` converts CLI options to `TraceConfig` and calls `trace::trace_program`; `src/trace.rs:132-165` validates limits and establishes the overall `supervisor::scope` deadline. `trace_program_inner` checks inspection expressions, probes required GCC/GDB and optional Valgrind/addr2line, creates a session directory, then writes the embedded `c_runtime/alloc_tracker.c` through `allocation_tracker::write_runtime_source` (`src/trace.rs:167-235`, `src/allocation_tracker.rs:30-37`). The resulting `tracker_source` is a real additional translation unit, not a source rewrite of the user's C.

`compiler::compile_project` (`src/compiler.rs:529-581`) detects the accepted diagnostics format and calls `compile` for the debug variant first. `build_argv` puts `-g -O0 -fno-omit-frame-pointer` and the four linker `--wrap` flags before user flags, then sources, objects and link options (`src/compiler.rs:392-451`); user flags can override defaults. Only after debug success does `compile_project` attempt the separate **unwrapped** sanitized binary, trying `address,undefined`, then `address`, then `undefined`. Its result is a pair of `CompilationOutput` records: `success`, `exit_code`, binary path, diagnostics, raw stderr, and selected diagnostics format (`src/compiler.rs:75-97,454-527`). The debug artifact is the one GDB later executes; building an ASan artifact does not run it.

`detect_diagnostics_format` probes JSON, SARIF-to-stderr, then text (`src/compiler.rs:130-158`); `compile` normalizes compiler diagnostics, falling back to `text_diagnostics` for unparseable output/linker errors (`src/compiler.rs:232-258,317-390,493-526`). `supervisor::output` enforces a deadline and bounded subprocess output (`src/supervisor.rs:226-260`). If invocation/deadline fails, `trace_program_inner` returns a partial trace with compiler stage failed/timed out and subsequent stages skipped (`src/trace.rs:238-289`). If GCC instead returns a failed debug build, `compiled_successfully=false`, `compile_errors` is populated, `ExecutionStatus::CompileFailed` is recorded and GDB is not started (`src/trace.rs:291-349`). A failed *optional* sanitized build keeps a successful debug build and notes degraded diagnostics (`src/compiler.rs:552-580`, `src/trace.rs:392-414`). The CLI exports a returned compile-failure trace but exits 2; an operational `Err` exits 1 (`src/main.rs:34-78`).

```text
CLI -> trace_program_inner: TraceConfig + session deadline
trace_program_inner -> write_runtime_source: embedded tracker -> session/alloc_tracker.c
trace_program_inner -> compile_project -> compile: debug GCC argv + wrapper TU
compile_project -> compile: optional unwrapped sanitizer variant(s)
compile_project -> trace_program_inner: (debug CompilationOutput, asan CompilationOutput)
trace_program_inner -> trace: compiler StageResult; failed debug build -> skip GDB
```

Source details: [entry](engine-entry-deep.html), [compiler](engine-compiler-deep.html), [supervision](engine-supervision-deep.html), [export](engine-export-deep.html).

## B. Turning a GDB stop into a source checkpoint

### Example and transition

`fixtures/c/phase8b_defined.c:13-16` has a non-inlined `checkpoint` function reading a packet, alias, heap element and global. Those expressions describe **possible** observations; a line number alone cannot establish values or execution. After compilation, `trace_program_inner` creates `UserCodeClassifier`, `EventFileReader` and `HeapTimeline`, and passes optional `dwarf::ObservationTypes` to `run_gdb_phase` (`src/trace.rs:416-439`). `GdbSession::launch` starts `gdb --nx --quiet --interpreter=mi2`, configures all-stop mode, PTY output, arguments and the allocator event-file environment (`src/trace.rs:1171-1189`, `src/gdb_mi.rs:385-511`). `insert_breakpoint("main")`, `install_user_entries` and `run` start observed stops; a missing `main` breakpoint allows execution to completion without intermediate stepping snapshots (`src/trace.rs:1226-1249`).

MI text is parsed by `mi_parser::parse_line`; `gdb_mi::route_record` delivers tokened `^` results to the pending command and asynchronous `*stopped` records separately, while `GdbSession::wait_stopped` returns a `StoppedInfo` with frame/reason/thread information (`src/mi_parser.rs:559-636`, `src/gdb_mi.rs:623-671,2302-2444`). At each stop, `run_gdb_phase` first ingests complete allocator lines, then classifies exit/signal, then checks whether the frame belongs to user code (`src/trace.rs:1371-1568`). In dual mode, a source checkpoint is created on a changed `(file,line,func)` or breakpoint hit; repeated same-source instruction stops can still produce instruction events without another snapshot. Source-only mode uses `exec_step`, whereas dual mode uses `exec_instruction`; non-user frames are normally finished or continued rather than traced as user instructions (`src/trace.rs:1562-1568,1720-1751,1799-1929`).

For an eligible checkpoint, `gdb_mi::capture_state` produces `CapturedState` with frames, bounded frame-zero variables, optionally caller-frame variables, errors and memory used (`src/gdb_mi.rs:1738-1749,1840-1964,2177-2227`). `recorder::capture_variable_memory` attempts actual stopped-inferior byte reads under object/checkpoint budgets, records partial/unreadable status, and leaves `initialization_status="unknown"` (`src/recorder.rs:45-185`). `push_snapshot` spends only the remaining snapshot budget on currently alive *observed* heap blocks and creates `ExecutionSnapshot`; `capture_requested_globals`, `memory::enrich_snapshot`, `finalize_memory_coverage`, and `Recorder::checkpoint` add qualified globals/pointer evidence, memory errors and the checkpoint journal record (`src/trace.rs:1609-1677,2179-2304,2356-2476`, `src/recorder.rs:259-265`). `SnapshotObservation` carries `stop_id`, instruction position, PID, thread, origin and heap-event count, not an exact allocation-call timestamp. If frame mapping, lexical metadata, reads, budgets, or GDB commands fail, partial captures and errors are retained; `capture_or_empty` preserves an otherwise lost stop (`src/trace.rs:2162-2176`).

```text
GDB --MI2 -> route_record -> wait_stopped: tokened result + async StoppedInfo
run_gdb_phase -> EventFileReader/HeapTimeline: ingest available complete lines
run_gdb_phase -> capture_state: bounded frames/variables/bytes at this stop
run_gdb_phase -> push_snapshot: remaining-budget live heap -> ExecutionSnapshot
run_gdb_phase -> globals/enrich_snapshot/finalize_memory_coverage
run_gdb_phase -> Recorder::checkpoint: stop-indexed checkpoint (or explicit cap)
```

Source details: [GDB control](engine-gdb-deep.html), [MI protocol](engine-mi-deep.html), [recorder](engine-recorder-deep.html), [stack capture](engine-stack-deep.html), [heap](engine-heap-deep.html).

## C. From a wrapped `malloc` to a qualified heap observation

### Example and transition

`fixtures/c/pointer_demo.c:7-19` includes `int *heap = malloc(4 * sizeof(int));`, writes elements and later frees the block. **Illustrative conditional sequence:** if that call executes successfully under the wrapped debug binary, `__wrap_malloc` calls the real allocator exactly once, assigns a process-local ID, emits `pid seq malloc id addr size old_id old_addr caller status` to the dedicated append-only event file, and inserts the returned address in its fixed table (`c_runtime/alloc_tracker.c:343-361,265-287`). It does not print allocator events to program stdout. `--wrap` covers references from linked objects, **not** allocations internal to shared libraries (`c_runtime/alloc_tracker.c:41-47`). A failed allocation emits `failed` and no live block; a zero-sized request may return either NULL or a non-NULL block, so its outcome must come from the event, not the syntax.

`EventFileReader::read_new` tails at most a 1 MiB segment per stop, buffers an unfinished line, and `parse_event_line` turns a complete line into `RawEvent` (`src/allocation_tracker.rs:65-159,262-399`). `HeapTimeline::ingest` checks `(pid, seq)`, adds a successful malloc as `BlockState` indexed by `(pid,id)` and `(pid,address)`, updates live-byte usage, then appends `HeapEvent::Allocated` (`src/allocation_tracker.rs:406-485,521-605,768-855`). A sequence gap, duplicate, malformed transport or retention cap downgrades history; it must not manufacture a live block from later events (`src/allocation_tracker.rs:501-567`). `Recorder::allocations` journals newly ingested events with the **stop at which they were noticed**, not the precise execution stop of the call (`src/recorder.rs:267-285`). At a later observed checkpoint, `push_snapshot` calls `alive_blocks(pid)`, reads live memory via `capture_variable_memory`, and creates `HeapBlockInfo` with ID/address/extent and read evidence; a block not observed at a stop has no captured bytes for that stop (`src/trace.rs:2179-2277`, `src/allocation_tracker.rs:861-873`). `memory::enrich_snapshot` can annotate pointer *address candidates* but address equality alone cannot prove pointer origin (`src/trace.rs:1667-1672`).

After the GDB phase, optional `resolve_caller_locations` and `apply_caller_locations` map the wrapper's raw return PC **minus the executable bias and one instruction byte** back to a file/line; an unresolved address remains a raw PC (`src/trace.rs:590-637`, `src/allocation_tracker.rs:936-1029`). `final_events` and `backfill_snapshots` widen local IDs only when multiple event PIDs were observed, while `correlate_heap_intervals` brackets an event between sampled checkpoints; neither backfill nor bracketing fabricates an exact source expression (`src/trace.rs:2564-2655`). `build_heap_state` keeps the last bytes *previously captured while alive*, including on a subsequently freed block, with its original observation stop (`src/trace.rs:2658-2715`). At a normal terminal boundary `verify_event_status` checks destructor ACKs and reports absent/damaged transport; the ACK cannot repair missing event lines, and an unobserved forked process is not counted as covered (`src/allocation_tracker.rs:178-260`, `src/trace.rs:477-483,906-943`).

```text
linked C call -> __wrap_malloc -> real malloc: result + raw return PC
__wrap_malloc -> event file -> read_new -> parse_event_line: RawEvent(pid,seq,id,...)
HeapTimeline::ingest -> BlockState + HeapEvent (or failed/incomplete evidence)
GDB source stop -> push_snapshot: live block bytes -> HeapBlockInfo
addr2line -> apply_caller_locations; final_events -> checkpoint interval
```

Source details: [allocator runtime](engine-allocator-deep.html), [heap reconstruction](engine-heap-deep.html), [recorder](engine-recorder-deep.html), [GDB reads](engine-gdb-deep.html).

## D. `realloc` is a new lifetime only when the event says so

### Example and transition

`fixtures/c/heap_provenance_realloc.c:14-45` deliberately contains a 256-to-128 resize, a huge failed resize, a larger resize, `realloc(zeroed, 0)`, and `realloc(NULL, 24)`. These are **fixture branches, not guaranteed outcomes on another host**: even the variable named `same` cannot prove same-address reallocation. `__wrap_realloc` reads its fixed-table predecessor before invoking `__real_realloc` and emits `old_id`, `old_addr`, returned address, new ID and one of `new`, `ok_same`, `ok_moved`, `zero` or `failed` (`c_runtime/alloc_tracker.c:387-443`). For a successful same-address or moved result it allocates a **fresh ID** and updates the table; failed nonzero resize preserves the original entry. The `zero` classification describes this runtime's **glibc** `realloc(ptr,0)` returning NULL and freeing the old allocation, not a portable C rule. `realloc(NULL,n)` is classified as `new` on success or `failed` on failure (`c_runtime/alloc_tracker.c:403-418`).

`parse_event_line` materializes the status as `RawEvent` (`src/allocation_tracker.rs:82-159`); `HeapTimeline::ingest` handles each branch differently (`src/allocation_tracker.rs:606-699`). `OkSame` retires the old `BlockState` and adds a linked successor when IDs differ; only historical event lines that reuse the *same* ID update the existing size in place. `OkMoved` retires the old block by address/ID and links the new block via `realloc_from_id`; `New` with no observed predecessor begins observation without inventing an old lifetime. `Failed` appends `AllocationFailed` **without retiring the old block**; `Zero` retires an observed old block and appends a terminal reallocation event without a new live block. `retire_block` updates live-byte usage; `add_block` rejects invalid or duplicate IDs/addresses and marks the PID incomplete rather than forcing a chain (`src/allocation_tracker.rs:758-807`). The successor link in `HeapBlockInfo` comes from reconstructed event IDs, not an assignment in the C file (`src/trace.rs:2566-2597,2662-2715`).

At a subsequent source stop, `push_snapshot` reads only `alive_blocks(pid)`; thus a failed realloc can leave the original eligible for another stopped read, whereas a terminal-free zero event cannot (`src/allocation_tracker.rs:861-873`, `src/trace.rs:2217-2268`). `correlate_heap_intervals` brackets retained realloc events between checkpoint counts; it does not assign an event to the line of `observe(same)` or a particular execution of a source call (`src/trace.rs:2616-2655`). An address reused after freeing, an event gap, or a missing ACK must not upgrade an address candidate to predecessor/origin identity. The historical [Stage 1 final audit](../stage1-final-audit.md) reports a 15-event `heap_provenance_realloc.c` run, but this page does **not** rerun or generalize its outcomes.

```text
fixture branch -> __wrap_realloc -> __real_realloc: observed return/status
wrapper -> event file -> RawEvent(old_id,old_addr,id,addr,status)
HeapTimeline::ingest: ok_same/ok_moved -> retire + linked successor
                     failed -> old remains; zero -> old retired, no successor
GDB checkpoint -> alive_blocks -> optional stopped-byte HeapBlockInfo
final_events/backfill_snapshots -> qualified IDs and checkpoint intervals
```

Source details: [allocator runtime](engine-allocator-deep.html), [heap lifetimes](engine-heap-deep.html), [GDB checkpoint](engine-gdb-deep.html), [recorder](engine-recorder-deep.html).

## E. Static metadata meets runtime evidence without becoming execution proof

### Example and transition

`fixtures/c/ast_source.c:2-17` contains a `TAKE` macro, `struct Point`, several allocator calls on one line, a member assignment, and a shadowed `value`. These are useful *static* syntax cases; no AST node, initializer or declaration establishes that a call executed or that bytes were initialized. `trace_program_inner` calls `dwarf::extract_excluding` on the **GCC debug binary** and rejects a registry with no eligible project compilation unit (`src/trace.rs:351-367,2731-2759`, `src/dwarf.rs:578-630`). This produces a `TypeRegistry` of units, type entries and declarations while excluding the injected tracker CU. With `clang_ast` enabled, `clang_ast::analyze` separately invokes Clang for bounded user translation units and populates `SourceMetadataRegistry` with translation-unit statuses, declarations and expressions; rejected flags, unavailable Clang, timeout and partial extraction remain explicit (`src/trace.rs:368-391`, `src/clang_ast.rs:180-225,228-389`). Clang parsing is opt-in, and its provider may be incompatible with GCC for cross-provider correlation.

Before GDB capture, `dwarf::ObservationTypes::new` builds declaration/type lookup tables (`src/trace.rs:434-437`, `src/dwarf.rs:879-971`). `ObservationTypes::enrich` attaches `type_ref`/`declaration_ref` to a uniquely plausible stopped `VariableInfo`, possibly supplying a static size **only when a runtime extent is absent and storage is known**; `gdb_mi::capture_state` and `recorder::capture_global` still obtain values, storage and bytes from the stopped inferior (`src/dwarf.rs:973-1039`, `src/gdb_mi.rs:1971-2174`, `src/recorder.rs:512-647`). After heap reconstruction, `dwarf::associate` matches runtime declarations by recorded full file, declaration line, name and (where available) function; ambiguous or missing matches remain unlinked (`src/trace.rs:944-955`, `src/dwarf.rs:1042-1102`). `clang_ast::correlate_declarations` separately matches AST to DWARF by name/location/kind with a restricted missing-column fallback (`src/clang_ast.rs:832-945`).

Finally `evidence::fuse` runs once, **without new GDB queries**, after runtime observations and heap history exist (`src/trace.rs:966-968`, `src/evidence/mod.rs:33-90`). `declarations::link_source_members` and `source_links::link_expressions` add eligible member/type references; `source_links::link_runtime` bridges an observed variable to an AST declaration only through the **same unique DWARF declaration ID** (`src/evidence/mod.rs:56-74`, `src/evidence/source_links.rs:15-143`). `heap::correlate` delegates to `clang_ast::correlate_heap_calls`: a unique direct compatible call on a resolved file/line is a *candidate*, while multiple or macro-expanded calls are *ambiguities*, never exact event-to-expression execution (`src/evidence/heap.rs:8-10`, `src/clang_ast.rs:947-1053`). `validation::audit` removes dangling newly constructed references, and `EvidenceFusionSummary` counts retained links/rejections (`src/evidence/validation.rs:15-180`, `src/evidence/mod.rs:77-90`). With no static provider fusion stays absent; with AST off, DWARF-only enrichment remains possible but no AST source-expression bridge exists.

```text
GCC debug ELF -> extract_excluding -> TypeRegistry -> ObservationTypes
optional Clang source pass -> analyze -> SourceMetadataRegistry
GDB stop -> capture_state/ObservationTypes::enrich -> VariableInfo + bytes
finished heap + snapshots -> dwarf::associate -> declaration_ref
AST/DWARF location match -> evidence::fuse -> qualified links/candidates
validation::audit -> cleared invalid links + EvidenceFusionSummary
```

Source details: [DWARF](engine-dwarf-deep.html), [Clang AST](engine-ast-deep.html), [evidence fusion](engine-fusion-deep.html), [stack observations](engine-stack-deep.html).

## F. A fatal stop or timeout retains evidence without claiming a normal exit

### Example and transition

`fixtures/c/phase8r3_crash.c:1-3` writes through a volatile null pointer while referring to `observed_global`; it illustrates the fatal-stop path and requested-global capture. `fixtures/c/infinite_loop.c:5-12` illustrates an **alternative** step/deadline path. These are distinct possible executions, **not** a combined trace; the historical audit reports a fatal-stop fixture and a step-limit fixture, but neither was rerun here ([historical audit](../stage1-final-audit.md)). `run_gdb_phase` classifies a reported fatal signal after ingesting event lines; `classify_signal` treats SIGSEGV and other configured fatal signals differently from resource and benign signals (`src/trace.rs:1371-1551,2031-2063`). It calls `capture_or_empty`, `push_snapshot` and `capture_requested_globals` even at the fatal stop, enriches the snapshot, records a checkpoint if space remains, then constructs `CrashReport` from observed signal/frame/backtrace via `crash_from` and sets `ExecutionStatus::Crashed` (`src/trace.rs:1441-1518,2489-2508`). A snapshot-cap failure is recorded as truncation; it does not erase the observed crash. `exited-signalled`/SIGKILL instead records interruption without inventing a cause (`src/trace.rs:1423-1440`).

If an execution command times out waiting for `*stopped`, `stop_wait_timeout_status` maps that to `Timeout`; a command-reply timeout before the stage deadline can instead be `DebuggerFailure` and poison the MI session (`src/trace.rs:1754-1797,2069-2089`, `src/gdb_mi.rs:583-609`). The loop also checks source checkpoint, step, instruction, command, output and wall budgets (`src/trace.rs:1569-1575,1682-1729,1932-1953`). For a nonterminal limit/deadline it kills the inferior tree and marks recording gaps/truncation; normal/crash paths terminate through the other branch (`src/trace.rs:1954-2024`). A final event read can preserve post-stop events, but `verify_event_status` and `HeapHistoryCoverage` explicitly flag missing terminal ACKs or incomplete event transport: a crash generally has no destructor ACK, so observed prefix events do **not** imply complete heap history (`c_runtime/alloc_tracker.c:319-336`, `src/trace.rs:477-483,851-943`). `trace_program_inner` maps the phase status to debugger/tracker `StageResult`, `ExecutionOutcome`, resource counters and `CrashReport` (`src/trace.rs:484-588,1007-1065`).

On a crash with a successful sanitized build and matching trigger, `run_sanitizer_pass` executes the **separate** ASan/UBSan binary before Valgrind; otherwise `OnCrashOrValgrindError` can wait for Valgrind findings (`src/trace.rs:49-106,644-680,787-848`, `src/sanitizer.rs:21-79`). `matches_crash` checks actual native source sites before labeling sanitizer reproduction; different-site findings become `diagnostic_observed` (`src/trace.rs:68-86`, `src/sanitizer.rs:162-185`). `valgrind::run_valgrind` is another independent process with passive wrapper telemetry and XML diagnostics, never the primary GDB execution (`src/trace.rs:682-785`, `src/valgrind.rs:39-132`). Missing tools, expired deadlines or malformed XML are stage failures/skips, not clean analyses. The CLI's `json_export::export_json_cancellable` separately preflights serialized size and may fail export even when a partial trace exists in memory (`src/main.rs:34-78`, `src/json_export.rs:72-75,139-167`).

```text
GDB stop -> run_gdb_phase: fatal signal / limit / stop-wait timeout
fatal -> capture_or_empty -> push_snapshot + globals -> crash_from: Crashed
nonterminal timeout/limit -> kill + recorder gap: partial recording
final event read -> verify_event_status -> qualified HeapHistoryCoverage
trace_program_inner -> optional separate sanitizer / Valgrind executions
trace_program_inner -> DebugSessionTrace -> export_json_cancellable (or export error)
```

Source details: [GDB failure paths](engine-gdb-deep.html), [diagnostics](engine-diagnostics-deep.html), [supervision](engine-supervision-deep.html), [recorder](engine-recorder-deep.html), [export](engine-export-deep.html).

## Diagram notes

### Walkthrough A diagram

{{diagram:d2-story-compile}}

Configuration to GCC artifacts.

### Walkthrough B diagram

{{diagram:d2-story-checkpoint}}

An observed MI stop to a retained checkpoint.

### Walkthrough C diagram

{{diagram:d2-story-malloc}}

A wrapped allocation to reconstructed lifetime evidence.

### Walkthrough D diagram

{{diagram:d2-story-realloc}}

Recorded realloc statuses to branch-specific transitions.

### Walkthrough E diagram

{{diagram:d2-story-static}}

Static declaration metadata to qualified additive references.

### Walkthrough F diagram

{{diagram:d2-story-failure}}

A crash or limit through partial evidence and optional separate diagnostics.
<!-- END CHAPTER: content/engine-walkthroughs.md -->

<!-- BEGIN CHAPTER: content/engine-handbook.md -->
# 37 · Trace Engine source file handbook

> **In one sentence:** This is a reading guide for engine source files, not a claim that you need to understand every helper before using the trace.

## Key terms

| Term | Plain meaning |
|---|---|
| **Orchestrator** | Code such as `trace.rs` that decides which stage runs next and handles its result. |
| **File chapter** | A focused explanation of the responsibilities and limitations of one source file. |

> **Example.** Start with `trace.rs` to see the big sequence, then visit `gdb_mi.rs` if your question is about debugger replies, or `allocation_tracker.rs` if it is about heap events.
>
> **What not to assume:** Source file order is not the same as runtime execution order, especially for optional or failed stages.

## The big picture

This is the coverage index for the DOC-2 engine handbook. Each row names a production Stage 1 file, its ownership boundary, and the detailed chapter that explains its types, functions, state and failure behavior. The [function reference](engine-functions.html) is generated from reviewed graph nodes; the complete lexical symbol index remains in [functions](functions.html).

## How to use the handbook

Start with [entry and orchestration](engine-entry-deep.html), then follow the execution path through compilation, GDB/MI, stopped-state capture, heap reconstruction, static metadata, diagnostics and export. A file's presence here means it has a detailed chapter; it does **not** mean every private helper has an independently reviewed walkthrough. Source locations are from the inspected worktree, not a claim that historical line numbers remain stable.

### Engine files and detailed treatment

| Production file | Responsibility | Detailed treatment |
|---|---|---|
| `src/main.rs` | CLI process entry, exit codes and final export call | [Entry and errors](engine-entry-deep.html) |
| `src/lib.rs` | Crate module declarations and public re-exports | [Entry and errors](engine-entry-deep.html) |
| `src/cli.rs` | Clap arguments, config merge and source validation | [Entry and errors](engine-entry-deep.html) |
| `src/config.rs` | Build, trace and sanitizer request types/defaults | [Entry and errors](engine-entry-deep.html) |
| `src/error.rs` | Engine error taxonomy and result alias | [Entry and errors](engine-entry-deep.html) |
| `src/compiler.rs` | GCC argv, probes, builds and diagnostic normalization | [Compiler internals](engine-compiler-deep.html) |
| `src/trace.rs` | Session orchestration, stop loop, assembly and caller resolution | [GDB controller](engine-gdb-deep.html) |
| `src/gdb_mi.rs` | GDB/MI2 process, command tokens, reader tasks and state capture | [GDB controller](engine-gdb-deep.html) |
| `src/mi_parser.rs` | GDB/MI lexer and typed record parser | [MI2 parser](engine-mi-deep.html) |
| `src/recorder.rs` | Resident observations, checkpoints, instructions and journal append | [Recorder](engine-recorder-deep.html) |
| `src/memory.rs` | Process maps, pointer extents and address-candidate qualification | [Stack and memory](engine-stack-deep.html) |
| `src/dwarf.rs` | GCC ELF/DWARF registry, origin resolution and observation types | [DWARF extraction](engine-dwarf-deep.html) |
| `src/clang_ast.rs` | Optional Clang JSON syntax extraction and source correlation | [Clang AST](engine-ast-deep.html) |
| `src/allocation_tracker.rs` | C runtime writer, event reader, heap timeline and caller resolution | [Heap reconstruction](engine-heap-deep.html) |
| `c_runtime/alloc_tracker.c` | Linked malloc/calloc/realloc/free wrappers and event/ACK files | [C allocator runtime](engine-allocator-deep.html) |
| `src/valgrind.rs` | Independent Memcheck process, XML parsing and ID-free site candidates | [Valgrind and sanitizers](engine-diagnostics-deep.html) |
| `src/sanitizer.rs` | Separate ASan/UBSan execution and bounded output parsing | [Valgrind and sanitizers](engine-diagnostics-deep.html) |
| `src/evidence/mod.rs` | One bounded post-assembly fusion pass and summary | [Evidence fusion](engine-fusion-deep.html) |
| `src/evidence/declarations.rs` | CU-scoped DWARF type/member lookup for AST fields | [Evidence fusion](engine-fusion-deep.html) |
| `src/evidence/source_links.rs` | Expression and runtime-variable source bridges | [Evidence fusion](engine-fusion-deep.html) |
| `src/evidence/heap.rs` | Heap-call candidate adapter | [Evidence fusion](engine-fusion-deep.html) |
| `src/evidence/validation.rs` | Construction-time reference audit | [Evidence fusion](engine-fusion-deep.html) |
| `src/supervisor.rs` | Deadline, cancellation, owned child groups and output drains | [Supervision and limits](engine-supervision-deep.html) |
| `src/resource_limits.rs` | Effective limits, rlimits and optional cgroup controls | [Supervision and limits](engine-supervision-deep.html) |
| `src/models.rs` | Schema-2 trace, snapshots, events, reports and coverage | [Model and export](engine-export-deep.html) |
| `src/processor.rs` | Optional structural instruction/checkpoint check | [Model and export](engine-export-deep.html) |
| `src/storage.rs` | Bounded create-new JSONL journal and recovery iterator | [Model and export](engine-export-deep.html) |
| `src/json_export.rs` | Exact serialized-byte preflight, atomic file and stdout | [Model and export](engine-export-deep.html) |

### Excluded from this handbook

`src/trace_parser/` is Stage 1 Trace Parser material reserved for DOC-3. `src/semantic/` is Stage 2 implementation and is outside this documentation project. The shared [schema and identity reference](schema.html) describes the wire contract consumed by both engine and parser, but it is not an engine-internals chapter.

### Coverage status

The machine-readable [inventory](assets/data/stage1-inventory.json) marks these files as having a detailed chapter. Graph-linked functions carry reviewed purpose, inputs/outputs, limitations and classified relationships. Other indexed functions are deliberately marked as source-located but not yet deeply walked through; DOC-2 does not turn a lexical definition match into a call or a behavioral proof.
<!-- END CHAPTER: content/engine-handbook.md -->

<!-- BEGIN CHAPTER: content/engine-algorithms.md -->
# 38 · Algorithms and invariants

> **In one sentence:** An algorithm is the rule the code follows. An invariant is a condition the code tries to keep true while following that rule.

## Key terms

| Term | Plain meaning |
|---|---|
| **Invariant** | A checkable rule, such as 'a later recorded stop must not be relabeled as an earlier one.' |
| **Bounded** | A request is limited by time, record count or bytes, and the limitation is reported rather than silently ignored. |

> **Example.** When a GDB step times out, the algorithm records that failure instead of turning the attempted instruction into a completed instruction. This preserves the invariant that attempts and completions are different.
>
> **What not to assume:** A proof about an algorithm under recorded inputs is not a promise that external tools never fail or that all processes were observed.

## The big picture

This page collects the mechanisms that are easy to misread from a single function name: supervised process control, MI dispatch, stop classification, bounded capture, DWARF reference chasing, source correlation, heap prefix reconstruction, fusion auditing and export preflight. Each section names the state that makes the algorithm correct and the evidence it refuses to invent.

## Supervised bounded subprocesses

`supervisor::output` starts an owned process group, drains stdout and stderr concurrently into separate bounded buffers, and waits for both the direct child and pipe EOF (`src/supervisor.rs:226-304`). It does not let a successful parent hide a descendant holding a pipe open. On cancellation or deadline it terminates the group, reaps what it can, and gives unfinished pipes a short bounded drain. `ProcessOutput.output_truncated` is a capture fact, not permission to claim complete diagnostics (`src/supervisor.rs:200-304`).

The invariant is **deadline plus ownership**, not a sandbox. A compiler or diagnostic stage can time out after producing useful bytes; those bytes remain partial evidence. A later stage must not infer success from the earlier stage's retained prefix.

## MI tokens and asynchronous stops

A command receives a monotonically increasing token. `GdbSession::execute` registers a pending waiter, writes a tokened MI command to stdin, and waits for the matching `^` result. The stdout reader task parses complete records and routes tokened results separately from asynchronous `*stopped` records (`src/gdb_mi.rs:532-671,2302-2484`). This separation is the central correctness rule: `-exec-step-instruction` returning `^done` means the command was accepted, not that the next stop was observed.

The reader preserves unknown classes and nested tuple/list structure through `mi_parser`; a malformed or partial line is an error/diagnostic, never a fabricated stopped frame (`src/mi_parser.rs:53-205,396-636`). Cancellation and EOF can leave a pending command unresolved; the caller must treat that as no sufficient completion evidence.

## Stop-loop classification and checkpoints

At each observed stop, `run_gdb_phase` first ingests available complete heap rows, then classifies the stop. A fatal signal, an exit, a normal source change and a dual-mode instruction attempt are different branches (`src/trace.rs:1371-1568,1720-1929`). The frame PC at a stop is the **next** address; an instruction event stores the pre-attempt PC plus the next observed address and stop reason. An absent after-stop is not proof that the attempted instruction completed.

Source checkpoints are retained when the implementation's source/origin and change rules select them. Repeated loop lines can remain distinct events. A checkpoint's `instruction_position` counts attempts before that snapshot; it is not an allocation timestamp or an exact instruction index into an unrecorded interval.

## Bounded stopped-state and memory capture

`gdb_mi::capture_state` builds a `CapturedState` from the selected frame, bounded frame-zero locals/arguments, optional caller frames, globals and memory requests (`src/gdb_mi.rs:1840-2227`). `recorder::capture_variable_memory` performs bounded inferior reads and retains raw bytes plus readability/truncation status (`src/recorder.rs:45-185`). A variable's static type from DWARF can supply a fallback extent only when the runtime extent is absent and the type is known; GDB remains the source of live values (`src/dwarf.rs:973-1040`).

The capture invariant is **preserve the observation, including its failure**. A missing read or an over-budget object remains visible as unavailable/partial; captured bytes do not prove initialization or an active union member. Pointer enrichment compares addresses with observed maps and heap extents, producing candidates, not ownership proofs (`src/memory.rs:141-205`).

## DWARF origin and specification resolution

`OriginResolver` follows `DW_AT_specification` or `DW_AT_abstract_origin`, filling only missing name/location/type attributes. A `UnitRef` stays in the current CU; an absolute `DebugInfoRef` is mapped through an index of absolute DIE offsets to its owning CU header (`src/dwarf.rs:213-423,535-575`). The visited set is `(CU header offset, unit DIE offset)`, with a four-hop limit and cycle detection.

The invariant is **bounded reference identity**. A missing target, unsupported form or cycle becomes an unresolved limitation; it does not fall back to a same-name declaration in another translation unit. CU-qualified keys preserve distinct declarations that happen to share a basename or symbol spelling.

## Clang spans and heap-call candidates

`span` computes source byte offsets and rejects zero/out-of-range or included-header locations; macro expansions retain a separate spelling location (`src/clang_ast.rs:397-488`). `correlate_heap_calls` indexes retained direct and macro allocator calls by canonical file and covered line, then accepts only a unique direct compatible call on a resolved return-address-minus-one line. Multiple calls or macro-only calls become explicit ambiguities (`src/clang_ast.rs:947-1054`).

The invariant is **site-only candidacy**. A resolved line chooses neither a particular dynamic call nor an allocation ID. An AST declaration/expression ID is static syntax identity, never a runtime activation identity.

## Heap verified prefixes and realloc state

`EventFileReader` accepts only complete newline-terminated rows; `parse_event_line` requires the exact 10-field or calloc 12-field shape (`src/allocation_tracker.rs:82-160,262-400`). `HeapTimeline::ingest` requires per-PID sequences to start at 1 and increase by one. A gap or malformed transport marks that PID/global history incomplete; later rows remain available but cannot retroactively certify missing transitions (`src/allocation_tracker.rs:521-756`).

Realloc is a branch table, not pointer arithmetic: failure preserves the old block; same-address success retires and recreates a linked lifetime; moved success links a successor; terminal zero-size free retires without a successor (`src/allocation_tracker.rs:606-699`). JSON IDs are widened to `(pid << 32) | local_id` only when multiple event PIDs were observed, with overflow reported rather than silently colliding (`src/allocation_tracker.rs:897-921`).

## Fusion and construction-time audit

`evidence::fuse` runs once after runtime and heap records exist. It indexes DWARF declarations/types, links AST fields and expressions only through unique CU/DIE-compatible matches, bridges runtime variables only through an already exact DWARF declaration ID, and delegates heap-call candidacy to the AST matcher (`src/evidence/mod.rs:33-91`). The final `audit` removes dangling references and verifies member name plus byte offset (`src/evidence/validation.rs:15-197`).

The invariant is **additive, qualified references**. Fusion does not rewrite observed values, addresses, activation IDs or allocation lifetimes, and it does not promote a source candidate to execution proof.

## Exact JSON preflight

`serialize_limited` writes through a byte-counting writer and rejects output above the configured cap before the destination is committed. For a file, `export_json` serializes to a sibling temporary file, flushes and renames it, then serializes to stdout (`src/json_export.rs:9-168`). The budget is per serialized JSON document, not a bound on the already assembled trace or on arbitrary serializer memory.

The invariant is **no partial final JSON from a known size violation**. A late stdout error after a successful rename is an operational failure with a committed file, not a rollback; a capped capture journal is a different, earlier boundary.

## Where to read next

Use the [source handbook](engine-handbook.html) for file-by-file ownership, the [function reference](engine-functions.html) for reviewed call relationships, and the [walkthroughs](engine-walkthroughs.html) for six subsystem-level sequences. The [limitations](limitations.html) page is part of every algorithm claim.
<!-- END CHAPTER: content/engine-algorithms.md -->

<!-- BEGIN CHAPTER: content/engine-functions.md -->
# 39 · Trace Engine function reference

> **In one sentence:** Use this reference when you already have a function name and want to find its purpose, source and reviewed connections.

## Key terms

| Term | Plain meaning |
|---|---|
| **Signature** | The function name, inputs and output written in source code. |
| **Reviewed graph entry** | A function whose purpose and linked relationships were checked against source instead of guessed from its name. |

> **Example.** Look up `run_gdb_phase`, then follow its links to the stop loop. The entry tells you where to read, while the debugger chapter explains what the stop actually means.
>
> **What not to assume:** A direct source call may be conditional. The catalog does not prove that it executed in a particular trace.

## The big picture

This page lists the engine functions that have a reviewed entry in the DOC-2 call-flow graph. Each entry shows the actual source location, signature, purpose, input/output boundary, qualification, and classified incoming/outgoing relationships. Use the subsystem links to read the surrounding algorithm and state transitions.

## Scope and reading order

A reviewed entry is **not** a claim that the function ran in a particular trace. Direct edges were checked at call expressions; subprocess, data and conceptual edges retain their own labels. For functions absent here, search the complete [function index](functions.html), then consult the file chapter in the [source handbook](engine-handbook.html). DOC-2 prioritizes orchestration, debugger, compiler, heap, metadata, diagnostics and export paths; not every private helper is expanded yet.

### Review status

The generated catalog marks these functions as `explained_and_graph_linked`. Functions in the same files remain `file_chapter_indexed_only` until a later pass verifies their own behavior. This distinction is intentional: an accurate inventory is more useful than a large but unsupported function encyclopedia.

### Engine graph entries

{{inventory:engine-functions}}

## Related algorithm notes

For cross-file mechanisms, see [algorithms and invariants](engine-algorithms.html). For the six source-guided subsystem sequences, see [walkthroughs](engine-walkthroughs.html). The interactive [call-flow explorer](call-flow.html) filters the same verified graph by subsystem.
<!-- END CHAPTER: content/engine-functions.md -->

<!-- BEGIN CHAPTER: content/model.md -->
# 40 · DebugSessionTrace · the recorded data model

> **In one sentence:** The data model is the shape of the saved recording. Think of it as labeled folders, not a reconstruction of missing history.

## Key terms

| Term | Plain meaning |
|---|---|
| **Snapshot** | One saved view at a debugger pause: where the program stopped, what variables were read, and what heap blocks were observed. |
| **Optional field** | A value that can be missing because the tool did not record or could not establish it; missing is not automatically false. |

> **Example.** A trace may contain a variable's displayed value but no raw bytes, or a heap event with no source location. Both can be valid partial evidence if the missing part is kept visible.
>
> **What not to assume:** A shortened JSON field example on this page illustrates the schema; it is not a complete file you can import.

## Level 1 · Intuition

A trace is not a video of a program. It is a set of timestamped/stopped observations, bounded attempts and gaps, heap telemetry, optional static metadata and diagnostics. Links between them use **different identity domains**. Preserving absent evidence is just as important as preserving a successful read.

{{diagram:records}}

## Level 2 · Record relationships

Picture the JSON as a notebook with several sections written at different times. In Rust, `DebugSessionTrace` holds the notebook; `RecordingData` is folded into the top-level JSON, so Rust's `recording.instruction_events` appears as JSON's `instruction_events`. Each `source_checkpoints` entry points to a saved snapshot **by array position**, not by including a second snapshot copy. `disassembly_table` is a dictionary of instructions; a saved instruction event refers to a dictionary entry by `instruction_id`. Running the same instruction three times still gives **three separate events**. `trace_gaps` mark intervals without exact captured events. Heap events list observed operations in time order; `allocation_lifetimes` describe a block's start and end; `heap_state` describes saved block state. They answer different questions. Instruction `coverage`, heap `heap_history`, journal `storage` and the outcomes in `stage_results` can be complete or partial **independently**.

### Events, gaps and checkpoints

- `snapshots[i].step_index=i`: captured checkpoint position. `snapshots[i].observation.stop_id`: processed debugger stop counter; may have holes. `instruction_position`: number of attempts preceding it.
- `instruction_events[j].event_id`: ordered attempted instruction ID; optional `checkpoint_id` references a **recent snapshot**, not an exact temporal join. `stop_id_before/after` refer to debugger observations.
- `trace_gaps[k].instruction_position` is a boundary where instructions were not captured; its unknown instruction count is **never guessed**.

### Variables, bytes and heap

`VariableInfo.read_status`, `bytes_status` and `memory_evidence` describe distinct observations. Pointer `address` is storage and `points_to` is the pointee. `HeapEvent.sequence` is **per-PID**, not event-vector position. `HeapBlockInfo.observed_at_stop` timestamps heap byte reads; a freed final block may retain historical bytes. `AllocationLifetime` has an allocation sequence and optional free sequence; a successful realloc can link old and new lifetime IDs. The schema does not create pointer origin from address equality.

## Level 3 · Actual schema excerpt and paths

```rust
// Reduced field excerpt from src/models.rs; other fields omitted.
pub struct DebugSessionTrace {
    pub schema_version: u32,
    pub snapshots: Vec<ExecutionSnapshot>,
    pub heap_events: Vec<HeapEvent>,
    pub heap_state: Vec<HeapBlockInfo>,
    pub execution_status: ExecutionStatus,
    pub stage_results: StageResults,
    // #[serde(flatten)] pub recording: RecordingData,
}
```

**Illustrative reduced excerpt**, not a copy/paste Rust definition or complete JSON: `DebugSessionTrace` also contains typed `execution_limits`, `execution`, compilation results, optional metadata and reports, usage and notes. For the actual declaration, see `src/models.rs::DebugSessionTrace` and [schema reference](schema.html).

```json
{"schema_version":2,"execution_status":"step_limit_reached","instruction_count":203,"coverage":{"scope":"user_code","complete_for_scope":false,"truncation_reasons":["max_steps"]}}
```

This is an **illustrative partial JSON projection**: no unspecified keys are implied, and a real trace's truncation-reason list may include additional causes. The importer requires the full model, not this excerpt. `schema_version` is 2 for supported V2.1/V2.2 documents; a missing marker defaults to 2, but unknown old `execution_limits` fields can still make pre-V2.1 files fail to deserialize. See [TP-1 compatibility](parser-ingestion.html#level-2-architecture-and-compatibility).

> NOTE: The wire model records uncertainty as `Option`/nullable values and explicit status fields. Never rewrite a missing `stop_id_after`, unloaded AST registry or unreadable bytes to a successful observation. Find identity semantics in the [reference dictionary](schema.html#identity-domains).
<!-- END CHAPTER: content/model.md -->

<!-- BEGIN CHAPTER: content/parser-ingestion.md -->
# 41 · TP-1 · Loading traces without inventing data

> **In one sentence:** Opening a trace is like opening a notebook: check that it is the right kind of notebook before relying on what its pages claim.

## Key terms

| Term | Plain meaning |
|---|---|
| **Schema** | The agreed JSON field format. Stage 1 uses schema version 2 for supported V2.1 and V2.2 recordings. |
| **JSONL** | One JSON object per line. The runtime journal is not the final trace document and cannot reconstruct missing reports. |

> **Example.** `TraceStore::open` can read a well-formed `old_id=7`. That still does not prove allocation 7 is the right predecessor; only deeper validation can examine the supporting events.
>
> **What not to assume:** An unsupported older example or malformed input fails import. A supported but incomplete trace may load and remain incomplete.

## Level 1 · Intuition

The Trace Parser reads the engine's finished JSON exactly once and makes a single immutable `TraceStore` available to multiple readers. Successful parsing means **schema-compatible**, not “all records are internally consistent” and not “the program ran to completion.” Deep validation is an explicit separate step.

{{diagram:ownership}}

## Level 2 · Architecture and compatibility

`TraceStore::open(path, options)` first checks how many bytes it is allowed to read. It reads one byte past the configured maximum only to tell the difference between an exactly fitting file and an oversized one. Next it distinguishes a complete JSON trace from a JSONL journal. `from_reader` and `from_json_bytes` accept input supplied by Rust code instead of a file. `from_trace` accepts a trace that Rust has already decoded, but checks **only its schema version**, not the truth of its relationships. Default import limits are 512 MiB of input and 2,000,000 JSONL records; oversized input is rejected, not quietly cut off. After loading, `TraceStore` keeps a shared `Arc` for the one original trace and another for its position indexes. Cloning the store shares these rather than duplicating every snapshot.

The supported format uses `schema_version=2`. The six supported V2.1 examples still load, even when newer optional fields were never recorded. In this model, a missing schema marker is interpreted as version 2. That does **not** mean every older file works: four pre-V2.1 `examples/v2_*.json` files use a removed limit field and cannot be decoded. Wrong schema, bad text encoding, broken JSON and oversized input each produce a specific error. By contrast, a C program crashing, timing out or reaching its step limit can produce a **valid but partial trace**; those outcomes are not reasons to pretend the JSON is malformed.

### JSONL is not a TraceStore

The `parse_journal` family can read each JSONL line as a configuration, checkpoint, allocation, gap, instruction or other recorded item. It also says whether the last line was torn off and whether the journal was finalized. A partially written last line can be identified as partial; a completely written but malformed line is an error. **Why not just open the journal as a trace?** The final heap state, crash reports and other end-of-session results may never have been written to it. `TraceStore::open` therefore refuses to guess a complete trace from journal lines. See [storage and limits](engine-limits.html).

## Level 3 · Implementation and API

```rust
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = TraceStore::open("examples/v21_dual.json", TraceLoadOptions::default())?;
    assert_eq!(store.trace().schema_version, 2);
    let validated = store.validated_semantic(); // opt-in; not part of open()
    if !validated.report().is_structurally_sound() {
        eprintln!("import has contradictory record relationships");
    }
    Ok(())
}
```

The snippet is executable as a Rust example in this crate (adjust the path if run elsewhere); for a larger end-to-end query see [Practical usage](usage.html#level-3-load-and-validate). Actual `TraceStore` methods: `open`, `from_reader`, `from_json_bytes`, `from_trace`, `trace`, `shared`, `source`, `validate_deep`. `src/trace_parser/{mod,json,journal,error}.rs` implement ingestion; `TraceParseError` covers malformed input, unsupported schema, journal-only input and limits. `src/trace_parser/indexes.rs` eagerly builds positions after successful decoding. `open` preserves file path/byte length in a small `TraceSourceMetadata` descriptor.

> WARNING: Neither schema compatibility nor `heap_process_scope()` on raw input certifies a claimed relationship. For imported/untrusted data, inspect `validated.report()` **and** the confidence/coverage of each relationship before presenting it as fact.
<!-- END CHAPTER: content/parser-ingestion.md -->

<!-- BEGIN CHAPTER: content/parser-indexes.md -->
# 42 · TP-2 · Compact, position-only indexes

> **In one sentence:** The parser keeps an index the way a book keeps page numbers. It does not photocopy every page of the book.

## Key terms

| Term | Plain meaning |
|---|---|
| **Position index** | A small table that says which position in the original trace array contains a record you asked for. |
| **Borrowed result** | A view of the existing saved record, not a newly invented or copied instruction event. |

> **Example.** If allocation 7 has events at positions 2 and 10, the index stores those two positions. A query uses them to return the original events, in their recorded order.
>
> **What not to assume:** An index lets you find a record quickly; it cannot prove that a recorded relationship is correct.

## Level 1 · Intuition

A large trace repeats instruction addresses and source lines thousands of times. Scanning every event for each question would waste time; copying every record into lookup tables would waste memory. TP-2 builds **keys → integer positions** once and borrows original records when queried. It does not create an address→allocation provenance index.

{{diagram:ownership}}

## Level 2 · What is indexed

When `TraceStore` is created, `TraceIndexes::build` makes lookup tables **once**. `ExecutionsIndex` answers “at which saved event positions did instruction 12 appear?” It keeps a compact range of position numbers for each compiled instruction. `SourceIndex` answers “which recorded events had this **exact file path and line**?” It also lets you go back from an instruction to recorded source positions. `AllocationIndex` answers “where are the events, final block and lifetime for allocation ID 7?” `MetadataIndex` locates DWARF and Clang source/type records by their IDs. These tables store numbers and keys, not second copies of instruction events, snapshots or raw bytes.

If you already have a snapshot's **vector position**, the parser can look it up directly. Other questions, such as “which snapshot has GDB stop ID 17?”, use a fast binary search only after checking that the imported stop IDs are in order. A broken order produces `OrderingViolation`, not a guessed answer. To show heap events between two saved snapshots, the parser uses their recorded running event counts. If an intermediate stop was never saved, that interval is only an approximate time window: it does not prove the event happened at the later stop. The index never treats equal filename endings or matching addresses as proven identities.

### Mini example

Disassembly instruction 12 may occur at `instruction_events` positions `[5, 21, 36]`. The index contains three `u32` positions, not three copies of the event or one merged “executed once” record. `instruction_execution_positions(12)` returns that slice; `instruction_executions(12)` borrows canonical event records at each position. `instruction_id=12` is **not** event 12.

## Level 3 · Source and query contracts

- `src/trace_parser/indexes.rs::{TraceIndexes, ExecutionsIndex, SourceIndex, AllocationIndex, MetadataIndex, OrderingFacts}`.
- `TraceStore::{instruction_event, instruction_range, events_at_checkpoint, snapshot_at_stop, instruction_executions, executed_events_for_source, heap_events_for_allocation, allocation_lifetime}` in `src/trace_parser/mod.rs`.

```rust
let positions: &[u32] = store.heap_event_positions_for_allocation(allocation_id);
for &position in positions {
    let heap_event = &store.heap_events()[position as usize];
    assert_eq!(heap_event.allocation_id, allocation_id);
}
```

This excerpt assumes `store` and `allocation_id` are defined. It visits recorded heap-event **positions** without confusing them with `event_id`, stop IDs or allocation IDs; the executable [usage sample](usage.html#checkpoints-and-paging) also handles empty traces and paginates. `TraceIndexes::estimated_bytes()` is a rough logical estimate of index structures, **not process RSS**. In the Stage 1 audit the 900-snapshot/3,143-instruction trace had ~28,369 estimated index bytes, ~1.13 ms median index build and ~17 MiB parser-process RSS after indexing (process includes trace, runtime and buffers; workload-specific).

> NOTE: A valid empty source/heap lookup means “no matching **recorded** item”; it is not a statement that nothing happened in a gap, another process or an uninstrumented library.
<!-- END CHAPTER: content/parser-indexes.md -->

<!-- BEGIN CHAPTER: content/parser-views.md -->
# 43 · TP-3 · Borrowed views and EvidenceRef

> **In one sentence:** A view is a convenient way to look at the trace you already loaded. It does not own a second independent history.

## Key terms

| Term | Plain meaning |
|---|---|
| **Arc** | Rust's shared-ownership handle: several readers can refer to the same underlying saved trace without copying all its records. |
| **EvidenceRef** | A citation to one original record, tied to the trace it came from. |

> **Example.** Two readers cloned from one `TraceStore` can resolve the same citation. If you independently load the same JSON again, its references belong to a different trace owner and are rejected as foreign.
>
> **What not to assume:** A borrowed view cannot recreate a value the engine never captured.

## Level 1 · Intuition

One large immutable trace should have many lightweight readers, not many copies. A view is a borrowed window into canonical records; an `EvidenceRef` is a **trace-scoped citation** that can later be resolved against that same recording. Equal numeric indices in two independent traces do not refer to the same execution.

{{diagram:ownership}}

## Level 2 · Ownership and lifecycle

`TraceStore` holds two shared Rust `Arc` values: the original `DebugSessionTrace` and the small `TraceIndexes`. You can clone the store without copying the trace. `raw()` offers recorded items directly. `semantic_input()` opens a **borrowed** `CheckpointContext` for a saved snapshot, so callers can inspect its current frame, optional callers, globals and observed heap without taking ownership of their bytes. `events_at_checkpoint()` uses recorded checkpoint tags, while a heap-event interval uses counts from neighboring *saved* snapshots. If the trace has no supported stop or source association, the query says `Unavailable`; it does not add a stop to make an answer look complete.

`EvidenceLocation` is like a precise address *inside the notebook*: “snapshot 3, variable 1,” or “heap event at array position 7.” `EvidenceRef` adds a weak link to **which loaded notebook** it came from. When you resolve the reference, the parser checks that owner first. A cloned `TraceStore` points to the same trace and accepts it; opening the same JSON independently creates a new owner and returns `ForeignTrace`. This prevents accidentally linking events from two separate loads just because their numbers happen to match. An `EvidenceRef` is not a portable ID you can serialize and use against another trace.

### Worked citation

A variable on the top frame of snapshot 3 may be observed with `snapshot.context.variable_evidence(1)?`. Resolving that reference returns the **same** canonical `VariableInfo` as `store.trace().snapshots[3].variables[1]`. A later stop with a local of the same name is **not** substituted. See [TP-6 variable confidence](parser-semantic.html#pointer-and-memory-confidence).

## Level 3 · APIs and errors

```rust
use c_trace_engine::trace_parser::{EvidenceLocation, TraceStore};
let store = TraceStore::open("examples/v21_dual.json", Default::default())?;
let cite = store.evidence_ref(EvidenceLocation::Snapshot(0))?;
let same_run = store.resolve_evidence(&cite)?;   // borrows original record
let _ = (cite.location(), same_run);
```

The snippet is a **body excerpt** (wrap it in `fn main() -> Result<(), Box<dyn std::error::Error>>`); see [Practical usage](usage.html#level-3-load-and-validate) for a standalone example. Implementation: `src/trace_parser/{views,evidence}.rs`; `TraceStore::raw`, `semantic_input`, `evidence_ref`, `resolve_evidence`. A `QueryError` distinguishes `OutOfRange`, `InvalidRange`, `LimitExceeded`, `InvalidReference`, `ForeignTrace`, `Unavailable` and `OrderingViolation`. No caller should turn an error into “zero evidence.”

> NOTE: A borrowed `CheckpointContext` cannot outlive its store. Keeping a separate `Arc<DebugSessionTrace>` alive is possible with `shared()`, but a different `TraceStore::open` still has a different trace identity even with byte-identical contents.
<!-- END CHAPTER: content/parser-views.md -->

<!-- BEGIN CHAPTER: content/parser-validation.md -->
# 44 · TP-5 · Deep structural validation

> **In one sentence:** A file can be readable JSON and still tell an impossible story. Validation asks whether its records agree, without demanding that every possible event was observed.

## Key terms

| Term | Plain meaning |
|---|---|
| **Structural corruption** | A contradiction: for example, a claimed realloc predecessor conflicts with the recorded creation event. |
| **Partial coverage** | A legitimate missing stretch, such as a crash or exhausted recording limit. Partial does not automatically mean corrupt. |

> **Example.** If a trace reports a block came from `realloc` of ID 7, validation checks the old and new events, PID and lifetimes. If those contradict, do not display the link as certain, even though the raw field is preserved.
>
> **What not to assume:** `TraceStore::open` checks the format. Call `validated_semantic()` or deep validation explicitly for relationship checks.

## Level 1 · Intuition

A JSON file can have the right *shape* and still contradict itself. TP-5's opt-in deep validation checks cross-record relationships. It distinguishes a corrupt link from a legitimate missing observation: a crashed or truncated program can yield a structurally sound but incomplete trace.

{{diagram:confidence}}

## Level 2 · Architecture

`validate_deep()` runs a list of consistency checks and collects the results in a `ValidationReport`. Each finding says **where** a problem was found, **what kind** it was, and how often it occurred. There are four important kinds: **StructuralCorruption** means records contradict one another; **PartialCoverage** means some intended observations are missing; **UnavailableEvidence** means a particular link cannot be supported; **OptionalAbsence** means an optional source of information is not there. `is_structurally_sound()` means there was no contradiction. It does **not** promise that all instructions, processes or bytes were captured. `is_partial()` asks a separate question about missing coverage. TP-6 runs this check once when you ask for its validated view. If structural contradictions exist, it refuses to give definite semantic confidence, while leaving raw records visible for investigation.

The checks ask practical questions: Do referenced snapshots and instructions exist? Are recorded stops in the required order? Do the counts agree? Are two different heap blocks claiming the same lifetime ID? Does a pointer marked `address_only` incorrectly claim a definite allocation origin? Does a trace claim all child processes were observed without evidence? The validator also checks static metadata links and whether reports agree with their stated coverage. For example, a pointer address inside block 7 is **not enough** to justify a claim that block 7 created that pointer. Older traces may simply lack a provenance or process-scope field: that means **unavailable**, not “the parser has filled it in.”

### Realloc link verification

A block's `realloc_from_id` is not enough. `realloc_link` compares indexed creation events of **both** new and predecessor identities with lifetimes: `Reallocated` must name the exact old ID; PID, order, allocation/free sequence and direction must agree. Self-links, cycles and explicitly conflicting events are structural contradictions even if history is partial. Missing corroboration receives `UnavailableEvidence`; an observed link in incomplete scope cannot become definite. glibc's `realloc(ptr,0)` terminal-free same-ID/null-address/zero-size event ends a lifetime but is **not** new lifetime creation. No address equality decides a predecessor. [Heap lifetimes](engine-heap.html) explains the source events.

## Level 3 · Report and import example

```rust
let store = TraceStore::open(path, TraceLoadOptions::default())?;
let report = store.validate_deep(); // explicit; not called by TraceStore::open
for finding in report.findings() {
    eprintln!("{} at {}: {}", finding.code, finding.location, finding.detail);
}
if report.is_partial() { /* retain the usable prefix, display missing scope */ }
```

This is a **body excerpt**; use [the executable sample](usage.html#level-3-load-and-validate) for imports. `src/trace_parser/validation.rs::{validate_deep, validate_heap, validate_heap_process_coverage, validate_pointer_reference, realloc_link}` implements checks. The resulting `ValidationReport` borrows no input JSON and aggregates occurrences by code; `first_location_findings()` on a TP-7 checkpoint is **not** a completeness proof for that checkpoint. `TraceStore::from_trace` intentionally remains schema-only.

### Example findings

| Code | Class | Correct response |
|---|---|---|
| `REALLOC_LINK_CONTRADICTORY` | Structural | Preserve raw import for investigation; never claim definite predecessor |
| `POINTER_HEAP_PROVENANCE_CONTRADICTORY` | Structural | Do not promote an address-only candidate |
| `HEAP_PROCESS_COVERAGE_UNSUPPORTED` | Structural | Reject claimed all-descendant completeness |
| `HEAP_HISTORY_INCOMPLETE` | Partial | Preserve observed events; downgrade history-dependent confidence |
| `REALLOC_LINK_UNVERIFIED` | Unavailable | Do not infer missing event/lifetime data |
| `SANITIZER_REPORT_UNRECORDED` | Optional | Absent report is not proof of no bug |

> WARNING: Trusting successful deserialization—or a raw `heap_history.complete` field without checking its scope and report—is a false-certainty bug. The [F01 regression](../trace-parser-v2.2-repair-f01.md) permanently checks the repaired realloc relationship.
<!-- END CHAPTER: content/parser-validation.md -->

<!-- BEGIN CHAPTER: content/parser-semantic.md -->
# 45 · TP-6 · Validated semantic evidence

> **In one sentence:** A confidence label always describes a *particular claim*. 'I read these bytes' and 'these bytes were initialized' are not the same claim.

## Key terms

| Term | Plain meaning |
|---|---|
| **Definite** | The named observation or relationship is supported under its stated limits; it does not mean the whole execution was complete. |
| **Candidate** | Evidence fits one explanation, but does not prove it. An address inside a heap block is a candidate for a match, not pointer origin. |

> **Example.** GDB might definitely have captured four bytes of a pointer. The pointer's allocation origin can still be unavailable, especially if its address was reused.
>
> **What not to assume:** Never promote an AST source call to an executed call, or Valgrind's separate-run address to a primary-run allocation ID.

## Level 1 · Intuition

“Definite” must answer **which relationship, in what scope**. A readable byte can be a definite *capture* without a definite C initialization. A pointer can definitely contain an address without a definite allocation origin. TP-6 packages borrowed evidence with per-relationship confidence; it is **not** the future C Semantic Engine.

{{diagram:confidence}}

## Level 2 · The confidence model

Call `TraceStore::validated_semantic()` once before asking for evidence-level answers. It borrows the unchanged trace and holds the validation report. Each answer has a confidence label. **Definite** means *this specific claim* has the required support in its stated scope; **Candidate** means plausible but unproved; **Ambiguous** means more than one reading fits; **Unavailable** means the needed evidence is missing; and **Contradictory** means recorded facts conflict. A structurally unsound import prevents the semantic layer from calling its relationships definite, but you can still inspect the raw fields to investigate. These labels do not say whether the C program finished, a variable was in scope, its bytes were initialized, or every child was observed: those are different questions.

### Pointer and memory confidence

Ask `variable(&EvidenceRef)` for the **exact** local, global or caller variable you want. It points to the saved `VariableInfo` and any available type information. A pointer's `address_candidate` means “the number in this pointer could fit in this block's address range.” It does **not** say the pointer came from that allocation; under the current V2.2 contract, `definitive_allocation` stays `None`. If an address was reused or its block already freed, the question can be ambiguous or unavailable. `variable_target` can find another **recorded variable identity at this same stop**; it will not search a later stop or a foreign trace, and duplicate identities stay ambiguous. `valid_range` is merely the number of bytes read at this historical stop. It does not prove those bytes were initialized or still have the same contents later.

### Identity and source relationships

`heap(id)` brings together the block, its recorded lifetime and events, plus any *claimed* older realloc ID. The ID's confidence can be definite only for a verified, complete history of the **processes actually observed**. A realloc relationship needs more: the old and new creation events, PID, order and lifetime boundaries must agree. If an older trace lacks scope details, even a plausible link stays a candidate. `source_expression(id)` points to an optional Clang description of code that was written, and may show possible heap-call matches. It keeps `execution_confidence` unavailable: the same source line appearing in an event does not establish which expression ran.

### Independent diagnostics

`diagnostics()` borrows compile, stage, crash, sanitizer and Valgrind records. `valgrind_error(index)` exposes source-site candidate confidence, but `primary_allocation_id` is **always `None`**, including for legacy imported raw IDs. `process_coverage().unobserved_descendants` stays unavailable on sound traces; `heap_history.complete` is not a descendant census. `execution()` reports attempted vs retained counts, gaps and engine-declared scope independently.

## Level 3 · The relevant implementation

- `src/trace_parser/semantic.rs::{ValidatedSemanticView, EvidenceConfidence, VariableEvidence, MemoryEvidenceView, PointerEvidence, HeapEvidence, SourceExpressionEvidence, ExecutionEvidence, ProcessCoverageEvidence}`.
- `TraceStore::validated_semantic` calls `self.validate_deep()` **once**. `SemanticInputView::validated()` offers the same path. `self.qualify` downgrades on structural unsoundness without mutating source records.
- `heap(id)` uses TP-2's per-allocation event positions and the shared `validation::realloc_link` classifier. `source_expression(id)` uses TP-2 metadata candidate/ambiguity positions; no global pointer graph or synthetic execution interpreter is stored.

```rust
let view = store.validated_semantic();
if !view.report().is_structurally_sound() {
    eprintln!("Structural contradiction; do not present definite links");
}
let evidence = view.heap(allocation_id);
let _ = (evidence.identity_confidence, evidence.realloc_confidence,
         evidence.old_id, evidence.events());
```

This excerpt assumes `store` and `allocation_id`; the full runnable [walkthrough](usage.html#level-3-load-and-validate) covers imports. `old_id` is **raw recorded data** and not itself a certified edge; inspect `realloc_confidence`. A heap-event interval's TP-7 `allocation(&view)` inherits this same confidence rather than silently upgrading a candidate.

> WARNING: “Definite” is scoped to the specifically named observation or relationship. It does not imply a complete primary execution, a complete process tree, initialized bytes or that independent diagnostics belong to the primary run.
<!-- END CHAPTER: content/parser-semantic.md -->

<!-- BEGIN CHAPTER: content/parser-consumer.md -->
# 46 · TP-7 · Checkpoint composition and bounded pages

> **In one sentence:** A checkpoint summary groups related questions about one recorded stop so callers do not have to hunt through unrelated arrays.

## Key terms

| Term | Plain meaning |
|---|---|
| **Consumer** | Code such as a UI or analysis example that asks the parser questions about a trace. |
| **Page** | An explicitly requested slice of results. This API returns at most 128 entries in one request; ask for the next slice if there are more. |

> **Example.** For a stop with 300 variable records, request positions 0–127, then 128–255, then 256–299. Returning just the first page without saying so would hide evidence.
>
> **What not to assume:** Grouping nearby events is a navigation convenience, not proof that a particular source expression caused one.

## Level 1 · Intuition

A user-facing query often wants several related things at one stop: variables, instruction events, heap events, gaps and optional diagnostics. TP-7 composes existing TP-6 evidence around a checkpoint without copying a trace or inventing a second evidence-ID system. Every page is **explicitly** limited to 128 records: request the next range to continue.

{{diagram:consumer}}

## Level 2 · What a summary means

Start with `checkpoint_summary(index)` when you know a saved snapshot's position. It keeps a citation to that snapshot and uses the same validated trace. The snapshot position, GDB stop number, instruction-attempt count, PID and source line are **different labels**; do not substitute one for another. `variables_page` reads a chosen page of current-frame, global or recorded caller variables with TP-6 confidence. `caller_frame` and `frame_activation` describe one actual saved stop, not a lifetime guessed from a function name. `instruction_events_page` finds events the recording tagged with this checkpoint; that tag helps navigation, but does not prove an expression on the line caused the event. `heap_interval_page` shows events between two **retained** snapshots, so the timing is a candidate interval, not an exact stop. Gaps describe attempted-position windows. Optional AST source expressions are only descriptions of written code.

Some information belongs to the **whole session**, not to one stop: overall coverage, compiler and diagnostic reports, Valgrind findings and validation notes. A report first displayed near checkpoint 4 did not necessarily happen *because of* checkpoint 4. `heap_events_page` first checks that your allocation citation belongs to this trace, then looks up its numeric ID. When you follow an interval's allocation link, it keeps TP-6's existing realloc confidence rather than quietly changing a candidate into a certainty.

## Level 3 · Real signatures and bounded traversal

```rust
// Excerpt for a previously constructed `view: ValidatedSemanticView`.
let stop = view.checkpoint_summary(0)?;
let len = stop.checkpoint.context.variables().len();
for start in (0..len).step_by(128) {
    let end = (start + 128).min(len);
    for item in stop.variables_page(VariableScope::Local, start..end)? {
        let variable = item?; // borrowed VariableInfo + confidence + EvidenceRef
        let _ = (variable.info.name.as_str(), variable.value_confidence);
    }
}
```

This excerpt assumes `VariableScope` is imported and a nonempty trace (use the [complete runnable example](usage.html#checkpoints-and-paging) for zero-snapshot handling). `MAX_CONSUMER_PAGE=128` in `src/trace_parser/consumer.rs` governs **each** half-open `Range<usize>`; empty pages are valid. Reversed or past-end ranges return `InvalidRange`; width 129 returns `LimitExceeded` **without dropping rows**. `src/trace_parser/indexes.rs` supplies event/allocation positions, `views.rs` supplies borrowed checkpoint context, and `semantic.rs` supplies unchanged TP-6 confidence.

### Cost and ordering

Stop/event/gap windows use verified order and binary search. Heap interval pagination uses canonical event slices and counters; AST source spans are scanned on demand within bounded registry size. Per-page iteration borrows existing records; it does not build an unbounded output array. The audit exercised **3,143 real retained instructions across 900 checkpoints**; separate permanent tests exercise 300 events and 1,024 variables across multiple ≤128 pages. The large real trace does **not** prove 128 events at a single real checkpoint. See [performance](handoff.html#performance-not-a-guarantee).

> NOTE: Keep the page's borrowed lifetime bounded by its `ValidatedSemanticView`; do not collect an unbounded `Vec` just because the API makes small pages cheap. When evidence is `Unavailable`, handle it explicitly rather than displaying an empty success state.
<!-- END CHAPTER: content/parser-consumer.md -->

<!-- BEGIN CHAPTER: content/parser-ingestion-deep.md -->
# 47 · Trace Parser ingestion and schema gating

> **In one sentence:** This is the code-level route from bytes on disk to a canonical, indexed `TraceStore`.

## Key terms

| Term | Plain meaning |
|---|---|
| **Decode** | Parse JSON text into Rust's saved-trace types after input and schema checks. |
| **Gate** | A check that rejects something too large, malformed, journal-shaped or from an unsupported schema before it is trusted as a full trace. |

> **Example.** A valid final JSON passes the input-size and schema gates and gets an index. A JSONL event journal fails reconstruction because it lacks some final reports and state.
>
> **What not to assume:** A successful decode is only a format result. Structural and semantic validation remain separate actions.

## The big picture

The parser starts with bytes, not a live engine session. TP-1 decides whether those bytes are a schema-2 `DebugSessionTrace` document, applies resource gates, and constructs one immutable canonical store. It also exposes a separate journal reader for the engine's JSONL record stream. The two paths deliberately do not merge: a journal cannot supply final reports and computed state that only exist in a complete exported trace.

## `src/trace_parser/mod.rs`: loading and assembly

**Principal signatures:**

- `pub fn open(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<TraceStore>` (`src/trace_parser/mod.rs:240-258`)
- `pub fn from_reader(reader: impl BufRead, options: TraceLoadOptions) -> Result<TraceStore>` (`:264-295`)
- `pub fn from_json_bytes(bytes: impl AsRef<[u8]>, options: TraceLoadOptions) -> Result<TraceStore>` (`:301-325`)
- `pub fn from_trace(trace: DebugSessionTrace) -> Result<TraceStore>` (`:331-352`)
- `fn assemble(trace: DebugSessionTrace, source: TraceSourceMetadata) -> Self` (`:216-230`)

`open` and `from_reader` call `read_trace_bytes`, which rejects an input larger than `options.max_bytes` (`:232-238`). `from_json_bytes` does not read `options.format`: it applies the byte limit, runs content gates, decodes JSON, and always constructs a store. `open`/`from_reader` dispatch through `load_trace_document`, which accepts JSON, rejects journal-shaped input with `JournalReconstructionUnsupported`, and never silently reconstructs a journal (`:973-995`). `gates` rejects empty/whitespace-only bytes and non-UTF-8 input before dispatch (`:990-995`).

`assemble` is the ownership boundary:

```rust
let indexes = TraceIndexes::build(&trace);
TraceStore {
    trace: Arc::new(trace),
    indexes: Arc::new(indexes),
    source,
}
```

(`src/trace_parser/mod.rs:222-229`). Index construction borrows the trace, then the trace and indexes become two separately shared Arcs. No record vector is cloned and no per-query index is rebuilt. `from_trace` takes an already constructed model, validates only its schema version, and then assembles. It does **not** call `validate_deep`.

### Loading order

1. Check size.
2. Reject empty or non-UTF-8.
3. Decode JSON and validate schema.
4. Build compact indexes.
5. Store the canonical trace and indexes behind Arcs.
6. Return a cheap-to-clone handle.

Deep validation is a separate explicit operation (`TraceStore::validate_deep`, `:406-408`), and semantic validation is requested separately by `validated_semantic()`.

## `src/trace_parser/json.rs`: document decode

**Signatures:** `pub(crate) fn decode_document(text: &str) -> Result<DebugSessionTrace>` (`src/trace_parser/json.rs:19-29`), `pub(crate) fn validate_schema_version(trace: &DebugSessionTrace) -> Result<()>` (`:32-46`), `pub(crate) fn detect_format(text: &str) -> DetectedFormat` (`:48-67`).

`decode_document` first uses serde to deserialize the existing `DebugSessionTrace` model, preserving the model's optional/additive V2.1/V2.2 fields, then calls the schema gate. A serde error becomes `MalformedJson`; a non-2 schema version becomes `UnsupportedSchemaVersion`. JSON object key order and whitespace are irrelevant. The parser never accepts a legacy removed required field merely because it is numerically similar.

`detect_format` classifies complete JSON documents and journal-shaped text. It is content-based, not filename-based. A malformed final journal-shaped line can be classified as a journal candidate even if newline-terminated; the later typed journal parser still rejects that record. The detector is therefore a dispatch aid, not proof that the journal is valid.

## `src/trace_parser/journal.rs`: typed JSONL records

**Principal signatures:**

- `pub(crate) fn parse_bytes(bytes: &[u8], options: &TraceLoadOptions) -> Result<JournalTrace>` (`src/trace_parser/journal.rs:142-149`)
- `pub(crate) fn parse_reader(mut reader: impl BufRead, options: &TraceLoadOptions) -> Result<JournalTrace>` (`:156-230`)
- `fn push_record(...) -> Result<()>` (`:234-267`)
- `fn decode_line(...) -> Result<Option<JournalRecord>>` (`:270-361`)

`JournalRecord` preserves configuration, checkpoints, allocations, gaps, disassembly, instruction events, finalized markers, and optional unknown records in file order (`:46-68`). `JournalTrace` adds per-kind counts, a `finalized` flag, a `torn_tail` flag, and the physical line count (`:108-122`). It is evidence in recorded order, not a `DebugSessionTrace`.

`parse_reader` buffers one line at a time and bounds a single record by `options.max_bytes` (`:167-174`). If a final line lacks a newline, it is accepted only when it fully decodes; otherwise it becomes `torn_tail` and is never interpreted (`:180-207`). `push_record` enforces `max_journal_records` and requires the first record to be configuration (`:242-254`). `decode_line` validates UTF-8, JSON object shape, string `kind`, and typed payload (`:275-361`).

### A concrete journal row

This is an illustrative record shape, not captured output:

```json
{"kind":"allocation","data":{"observed_at_stop":7,"event":{"event_type":"Allocated","allocation_id":1,"pid":100,"sequence":4,"address":"0x55d0a0002a10","size_bytes":32}}}
```

The parser returns `JournalRecord::Allocation { observed_at_stop: 7, event }`. It does not infer that this allocation happened exactly at stop 7; the stop is the recorder's observation marker. `parse_journal`, `parse_journal_bytes`, and `parse_journal_reader` are public wrappers around this path (`src/trace_parser/mod.rs:926-962`).

## `src/trace_parser/error.rs`: failure categories

`TraceParseError` separates input failure from query failure (`src/trace_parser/error.rs:17-119`): source wrapping, I/O, empty input, size limit, invalid encoding, malformed JSON, unrecognized format, unsupported schema, invalid trace, malformed/unsupported journal records, journal record limit, unsupported reconstruction, and missing journal configuration. File entry points wrap inner failures in `Source { path, source }` so the originating input remains visible.

`QueryError` is a different type (`:139-218`): page limit, foreign trace, out-of-range position, invalid half-open range, unresolvable reference, unavailable evidence, and ordering violation. Ingestion success therefore does not imply query success. A partial execution is recorded data, not a `TraceParseError`.

## Tests and boundaries

`tests/trace_parser.rs` covers file/reader/byte loading, schema rejection, format selection, journal fixtures and malformed inputs; `tests/fixtures/journal/{calloc_demo_dual,finite_loop_dual}.jsonl` exercise typed record streams. `TraceLoadOptions::default()` caps JSON input at 512 MiB and journals at two million records (`src/trace_parser/mod.rs:141-184`). Those limits do not bound an already assembled parser process. The engine's separate JSON export cap does not become a parser input guarantee.

## Diagram notes

### JSON ingestion sequence

{{diagram:d3-ingest-json}}

### Schema gate

{{diagram:d3-schema-gate}}

### Journal record lifecycle

{{diagram:d3-journal}}

### Ingestion error classes

{{diagram:d3-errors}}
<!-- END CHAPTER: content/parser-ingestion-deep.md -->

<!-- BEGIN CHAPTER: content/parser-ownership-deep.md -->
# 48 · Canonical ownership, borrowed views, and evidence references

> **In one sentence:** This page explains who owns the saved trace and why queries can return original records without making copies.

## Key terms

| Term | Plain meaning |
|---|---|
| **Canonical owner** | The one shared `Arc<DebugSessionTrace>` holding the records for this loaded trace. |
| **Foreign trace** | A second independent load, even from identical JSON, with a different owner. Its citations cannot be used against the first load. |

> **Example.** The parser returns a reference to snapshot 3's variable rather than copying its bytes. An `EvidenceRef` carries enough ownership context to reject a mistaken cross-trace lookup.
>
> **What not to assume:** A weak reference does not keep a dropped trace alive or make numeric IDs transferable between loads.

## The big picture

The parser's central design is simple: own the trace once, build positions once, and let every higher layer borrow the original records. A view is organization, not a second trace. An evidence reference is a checked position plus a weak owner identity, not a copy of an observation.

## Canonical owner and cheap clones

`TraceStore` has exactly three fields (`src/trace_parser/mod.rs:191-214`):

```rust
pub struct TraceStore {
    trace: Arc<DebugSessionTrace>,
    indexes: Arc<TraceIndexes>,
    source: TraceSourceMetadata,
}
```

`assemble` builds indexes from `&trace`, then moves the trace and index sets into separate Arcs (`:216-230`). `TraceStore` derives `Clone`: cloning shares both Arcs and copies only the small source metadata. `shared()` returns the canonical trace Arc (`:354-357`), not another store or an index clone.

This creates three ownership cases:

| Case | Owner | Query behavior |
|---|---|---|
| `let alias = store.clone()` | Same `Arc<DebugSessionTrace>` | Same trace identity; all evidence references resolve |
| Load the same JSON twice | Two independent Arcs | Equal content, different identity; references are foreign |
| Drop every `TraceStore` clone | Trace and indexes freed when last Arc drops | Old `EvidenceRef` cannot become accidentally valid through address reuse |

The weak identity in `EvidenceRef` is what prevents a numeric position from becoming portable. Two stores can both have `snapshots[0]`; that does not make their records the same trace.

## Borrowed views

**Signatures:** `pub fn raw(&self) -> RawTraceView<'_>` (`src/trace_parser/views.rs:433-435`) and `pub fn semantic_input(&self) -> SemanticInputView<'_>` (`:438-440`). Both views hold one `&'a TraceStore`; both are `Clone, Copy` and allocate no records.

`RawTraceView` dereferences to `TraceStore`, exposing existing TP-1/TP-2 methods without duplicate APIs (`:44-86`). `SemanticInputView` adds checkpoint traversal and evidence resolution (`:91-166`). It does not infer C objects, pointer origins or state changes.

### Checkpoint contexts

`SemanticInputView::checkpoint_context(snapshot_index)` resolves a snapshot and returns `CheckpointContext` with `source_checkpoint_position = None` (`:114-122`). `source_checkpoint_context(position)` first maps the explicit source-checkpoint position to a snapshot index (`:128-133`). `snapshots()` iterates all retained snapshots, including snapshots not in `source_checkpoints` (`:137-144`). `source_checkpoints()` iterates the explicit list and yields `QueryResult` items so a bad recorded reference is reported at its actual position rather than skipped (`:149-154`).

`CheckpointContext` (`:175-181`) borrows the store, a snapshot, and the snapshot index. It keeps source-checkpoint position separate. Its `stop_id()` converts the model's zero sentinel to `None` (`:196-200`); `source_location()` rejects empty file/line 0 (`:204-207`). `events_at_checkpoint` and `heap_events_in_snapshot_interval` delegate to verified TP-2 searches (`:273-290`). `snapshot_evidence`, `variable_evidence`, `global_evidence`, caller evidence and heap-block evidence create trace-scoped references (`:292-341`).

The distinction matters: a context reached through `source_checkpoints` has an explicit checkpoint position, while a context reached directly from `snapshots()` does not. Neither invents a checkpoint entry.

## `EvidenceRef` and `EvidenceLocation`

`EvidenceLocation` enumerates canonical collection positions and nested positions (`src/trace_parser/evidence.rs:21-66`): snapshot, instruction event, disassembly, heap event/state, allocation lifetime, gap, frame, variable, caller frame/variable, global, heap block, event memory, compiler/Valgrind/internal diagnostic, and report markers. A caller-frame `frame` is a vector position, not a GDB level or activation ID.

`EvidenceRef` stores a `Weak<DebugSessionTrace>` and an `EvidenceLocation` (`:68-75`). It never owns the trace or a record. `evidence_ref` first calls `resolve_location` to reject invalid positions, then binds the location to this store's weak trace identity (`src/trace_parser/evidence.rs:129-135`; `src/trace_parser/mod.rs:1026-1039`).

`resolve_evidence` checks `Weak::ptr_eq` before resolving the location (`src/trace_parser/evidence.rs:140-145`). This ordering is important: a reference from an independently loaded identical JSON document returns `ForeignTrace`, not a plausible-looking snapshot. Clones of the same store share the same Arc and resolve normally.

`resolve_location` performs direct vector lookups for every supported category and returns `Evidence<'_>` borrowing the original record (`:147-265`). Optional reports return `Unavailable`; invalid indices return `InvalidReference`. `Evidence` has no owned payload, so raw bytes, addresses and statuses remain exactly as recorded.

### Valid and invalid example

```rust
let store = TraceStore::open("trace.json", TraceLoadOptions::default())?;
let context = store.semantic_input().checkpoint_context(0)?;
let reference = context.variable_evidence(0)?;
let evidence = store.resolve_evidence(&reference)?;

let other = TraceStore::open("trace.json", TraceLoadOptions::default())?;
assert!(matches!(
    other.resolve_evidence(&reference),
    Err(QueryError::ForeignTrace)
));
```

The example uses the real API names. It is illustrative, not a checked-in test transcript. A `reference` is not serialized as a cross-session identity; its location alone is insufficient without the matching owner.

## Raw versus semantic input

`RawTraceView` is for exact recorded access: ranges, instruction events, source/disassembly, heap events and diagnostics. `SemanticInputView` composes those records into checkpoint contexts without making semantic claims. `ValidatedSemanticView` is the next layer: it owns a `ValidationReport` and borrows the store (`src/trace_parser/semantic.rs:28-34`). Creating it runs one deep-validation pass; queries do not rerun it.

## Performance and tests

Creating either view copies one reference; creating a checkpoint context is O(1) and allocates no record buffer. Evidence creation validates a position and allocates an `EvidenceRef` with a weak pointer. Resolution is direct vector indexing. `tests/trace_parser_views.rs` exercises raw/semantic context separation, foreign-reference rejection, caller evidence, source declarations and process-scope annotations; `tests/trace_parser.rs` covers positional/range errors.

## Diagram notes

### Canonical ownership

{{diagram:d3-ownership}}

### Borrowed lifetime relationships

{{diagram:d3-lifetimes}}

### Evidence reference creation and resolution

{{diagram:d3-evidence-ref}}
<!-- END CHAPTER: content/parser-ownership-deep.md -->

<!-- BEGIN CHAPTER: content/parser-indexes-deep.md -->
# 49 · Compact position indexes

> **In one sentence:** These are the actual lookup-table designs behind the fast queries. Start with the book-index idea before reading the array layouts.

## Key terms

| Term | Plain meaning |
|---|---|
| **CSR** | Compressed sparse row: two compact arrays tell you where the positions for one key begin and end. Think of a table of contents pointing to a short run of page numbers. |
| **Canonical array** | The original trace's ordered list of events or snapshots; the index stores positions into it rather than duplicating records. |

> **Example.** If static instruction 4 was observed at event positions 2, 9 and 20, CSR stores those positions together. Looking up instruction 4 retrieves the three original events in their saved order.
>
> **What not to assume:** Fast lookup is not validation. If an imported event names an invalid instruction, deep validation reports it instead of fixing it by guessing.

## The big picture

TP-2 exists so repeated consumer questions do not rescan canonical arrays. The indexes store **positions**, interned path keys and small ordering facts; they never clone an `InstructionEvent`, snapshot, heap event or metadata record. `TraceIndexes::build` runs once during `TraceStore::assemble`, and every clone of the store shares the same index Arc.

## Index inventory

| Index | Source collection | Key | Stored value | Main consumers |
|---|---|---|---|---|
| `ExecutionsIndex` | `instruction_events` | disassembly `instruction_id` | CSR event positions | `instruction_execution_positions`, `instruction_executions` |
| `SourceIndex` | `instruction_events` | exact `(file, line)` and instruction ID | CSR instruction IDs/event positions/source-key positions | source lookup and reverse instruction→source |
| `AllocationIndex` | `heap_events`, `heap_state`, `allocation_lifetimes` | allocation ID | event positions, state position, lifetime position | heap queries and TP-6/TP-7 |
| `MetadataIndex` | optional DWARF/AST registries | trace-local type/declaration/expression IDs | registry positions and candidate/ambiguity positions | TP-3/TP-6 static lookup |
| `OrderingFacts` | snapshots, events, gaps | recorded sequence | booleans | zero-memory binary searches |

`TraceIndexes::build` calls each builder in one construction pass (`src/trace_parser/indexes.rs:94-116`). It does not sort canonical arrays. `estimated_bytes` is an approximate payload estimate, not process RSS (`:123-160`).

## Disassembly executions: CSR

`ExecutionsIndex` stores `offsets: Vec<u32>` and `positions: Vec<u32>` (`:270-274`). The build makes two passes over events:

1. Count events per valid `instruction_id`.
2. Build `offsets[id]..offsets[id+1]`, then fill positions using a cursor.

The result is a compressed sparse row layout: repeated executions of one static instruction stay distinct and appear in ascending event order (`:277-309`). `positions_of` rejects an ID outside the disassembly table with `InvalidReference` and returns the borrowed slice (`:313-326`). Events with missing or out-of-range IDs are absent from the index; deep validation reports them, and direct queries surface invalid references rather than inventing a match.

### Lookup path

```text
instruction_execution_positions(id)
  -> ExecutionsIndex::positions_of(id)
  -> offsets[id]..offsets[id+1]
  -> &positions[start..end]
  -> canonical instruction_events[position]
```

There is no per-query scan and no cloned event vector. Complexity is O(1) slice lookup after O(events + disassembly entries) construction.

## Source locations: static and dynamic

`SourceIndex` records exact file strings, not basenames (`src/trace_parser/indexes.rs:343-397`). Each distinct path is interned once; a `(file_id, line)` key maps to a slot. For each event with both `source.file` and `source.line`, the builder accumulates:

- `event_positions`: every recorded event at that location, in event order;
- `instruction_ids`: deduplicated disassembly IDs associated by those events;
- reverse instruction→source-key positions.

The transient slots are flattened into CSR arrays (`:461-499`). `key_position` scans only distinct recorded files, then uses a hash lookup for the line (`:507-510`). `instruction_ids_of` is the **static** side; `event_positions_of` is the **dynamic** side. They answer different questions and are never merged.

`source_key_positions_of` returns source keys for a disassembly ID (`:584-597`). The disassembly table itself has no source fields in this schema; the association is resolved through recorded event evidence. A source line can therefore have several instructions, and one instruction can have several recorded source contexts.

## Allocation identity, never address

`AllocationIndex` keys on `allocation_id`, not address and not vector position (`:610-688`). It stores:

- `events_by_id`: chronological event positions per ID, including ID 0's untracked events;
- `state_by_id`: first `heap_state` position per ID;
- `lifetime_by_id`: first lifetime position per ID.

`event_positions_of` returns an empty slice for an unknown ID, a valid empty result (`:661-666`). `state_position_of` and `lifetime_position_of` return `Option`, allowing callers to distinguish "not recorded" from an event history. Duplicate state/lifetime IDs are not silently merged; deep validation reports them.

## Metadata identities

`MetadataIndex` holds hash maps from trace-local type/declaration/source declaration/source expression IDs to positions (`src/trace_parser/indexes.rs:167-257`). Candidate and ambiguity maps hold **positions** into the optional source registry. Duplicate IDs retain the first position; deep validation reports the structural contradiction before trusted semantic queries (`:163-166`).

This is a static index, not an execution index. A source expression ID never becomes an executed-expression ID, and a heap-call candidate never becomes an allocation identity.

## Ordering facts and binary search

`OrderingFacts` verifies five recorded orderings in one pass (`:694-747`):

- snapshot `stop_id` strictly increasing;
- event `stop_id_before` non-decreasing;
- event `checkpoint_id` non-decreasing, with `None` first;
- snapshot cumulative `heap_event_count` non-decreasing;
- trace-gap `instruction_position` non-decreasing.

`TraceStore::snapshot_at_stop` refuses to binary-search unless `snapshots_by_stop_sorted` is true (`src/trace_parser/mod.rs:554-567`). `events_at_stop` checks event stop order and returns a borrowed contiguous run (`:578-589`). `events_at_checkpoint` checks checkpoint order (`:596-615`). `heap_events_at_stop` checks heap counts and returns the slice between the previous retained snapshot and this one (`:634-664`). A broken ordering is `QueryError::OrderingViolation`, not a nearest-match guess.

## Position conversion and memory

`pos(index)` converts `usize` to `u32` with an assertion that a record count above `u32::MAX` is impossible under the ingestion byte cap (`src/trace_parser/indexes.rs:755-759`). Positions are `u32` in compact arrays; canonical lookups convert back to `usize`. This is a checked implementation invariant, not an arbitrary truncation.

`estimated_bytes` counts CSR arrays at four bytes per position, approximates hash overhead, and includes interned path bytes and metadata maps (`:123-160`). It excludes allocator bucket metadata, the canonical trace, and the validation report. The historical Stage 1 audit's 900-checkpoint/3,143-instruction workload is evidence about one release benchmark, not a universal RSS bound.

## Tests

`tests/trace_parser_indexes.rs` covers execution positions, source keys, duplicate/invalid references, heap identity, ordering and iterators. `tests/trace_parser.rs` exercises public stop/checkpoint/source/heap queries. The tests validate implementation branches; they do not prove that a particular imported trace is semantically valid.

## Diagram notes

### Index construction pipeline

{{diagram:d3-index-build}}

### Position-to-record resolution

{{diagram:d3-index-lookup}}

### Identity domains

{{diagram:d3-identity-domains}}
<!-- END CHAPTER: content/parser-indexes-deep.md -->

<!-- BEGIN CHAPTER: content/parser-validation-deep.md -->
# 50 · Deep validation and finding classification

> **In one sentence:** This chapter shows how the parser checks imported records against one another and reports contradictions separately from missing evidence.

## Key terms

| Term | Plain meaning |
|---|---|
| **Validation finding** | A report entry with a code, severity and location saying what check failed or what evidence is missing. |
| **Aggregation** | Repeated findings can be counted together without erasing where they first appeared. |

> **Example.** A trace that stops because of a time limit can be structurally sound and partial. A trace that claims one realloc has two incompatible predecessors is structurally corrupt.
>
> **What not to assume:** The parser retains the original raw fields for investigation; it does not turn a contradiction into a supported semantic link.

## The big picture

Deep validation is an explicit audit of the imported trace. It is not part of deserialization, and it never rejects a trace. `TraceStore::validate_deep()` calls `validation::validate_deep(&self.trace)` and returns a report (`src/trace_parser/mod.rs:401-408`). TP-6 calls the same pass once when a `ValidatedSemanticView` is constructed.

## Report model

`ValidationSeverity` has four categories (`src/trace_parser/validation.rs:48-70`):

- `StructuralCorruption`: a guaranteed invariant is violated; dependent queries refuse to answer.
- `PartialCoverage`: the trace legitimately lacks observations because of caps, retention or interruption.
- `UnavailableEvidence`: a relationship cannot be established from recorded evidence.
- `OptionalAbsence`: the schema permits the field to be absent.

`ValidationLocation` identifies the first canonical position for a finding (`:75-156`). `ValidationFinding` carries severity, stable code, human detail, location and an occurrence count (`:158-171`). `ValidationReport::findings()` returns all findings; `of_severity` filters; `is_structurally_sound()` is true only when there are no corruption findings (`:193-232`). `is_partial()` is a separate question.

### Aggregation

`Builder::push` aggregates by `(code, severity)`, preserving the first location and incrementing `occurrences` (`:814-841`). `finish` sorts corruption first, then partial, unavailable and optional (`:843-855`). This keeps a 94k-event report compact instead of emitting one finding per record.

## Orchestration

`validate_deep` runs the engine's structural validator first, then a fixed sequence of passes (`src/trace_parser/validation.rs:243-267`):

1. `crate::processor::validate` → `ENGINE_STRUCTURAL_VALIDATION`
2. instruction events
3. snapshots
4. source checkpoints
5. heap
6. DWARF registry
7. AST registry
8. source links
9. gaps
10. coverage
11. diagnostics

The function is O(E + S + H + B + L + G) in retained records. It does not require `TraceIndexes`; it can validate a store built before the current index layer existed.

## Instruction events

`validate_instruction_events` (`:860-1120`) checks the orderings used by zero-memory queries: `stop_id_before` non-decreasing, `checkpoint_id` non-decreasing, and event IDs/addresses/disassembly references consistent with the canonical arrays. It distinguishes a missing `instruction_id` (optional/unavailable) from an out-of-range ID (corruption). It also checks memory evidence shape and variable/event references without turning an unreadable read into corruption.

A recorded gap is not an event. An attempted instruction count above the retained event count is a retention fact, not a reason to invent missing events.

## Snapshots and source checkpoints

`validate_snapshots` (`:1123-1250`) checks snapshot positions, stop identity, frame/variable/caller references, heap counts, memory status and pointer evidence. It verifies that a snapshot's `step_index` is its vector position, that source locations are internally consistent, and that referenced records exist. Missing optional capture is unavailable/optional; a reference that cannot resolve is a structural finding.

`validate_source_checkpoints` (`:1419-1459`) verifies that every entry in `recording.source_checkpoints` is a valid snapshot position. An out-of-range entry is corruption; an empty list is normal.

Pointer checks are deep enough to preserve the corrected contract:

- a heap target with an allocation ID but no recorded provenance status is `POINTER_HEAP_PROVENANCE_UNRECORDED` (unavailable);
- a numeric-address, uncertain, freed or non-heap target claiming a definite ID is `POINTER_HEAP_PROVENANCE_CONTRADICTORY` (corruption);
- a target identity must be definite, not `unresolved:`, and cannot also identify a heap allocation;
- an inactive-only variable target, base+offset mismatch, or address outside every recorded extent is corruption;
- a missing target variable at one checkpoint is unavailable, not proof that the activation died.

See [pointer and process coverage](parser-coverage-deep.html) for the decision tables.

## DWARF and AST registries

`validate_dwarf_registry` checks duplicate trace-local type, declaration and CU IDs (`src/trace_parser/validation.rs:270-315`). `validate_ast_registry` checks duplicate AST declaration/expression IDs and verifies translation-unit, declaration, expression, DWARF and type references (`:317-812`). A missing optional registry returns without findings. A malformed reference is structural corruption; an absent optional provider is optional absence, not a failed execution.

`validate_source_links` checks fusion references, member offsets and runtime-variable source bridges. A candidate source relationship is not promoted to execution proof. Invalid links are removed by the engine's construction-time audit and reported here when imported.

## Heap, gaps and diagnostics

`validate_heap` checks unique state/lifetime IDs, per-PID sequence order, checkpoint intervals, realloc origins, terminal event shape, address presence and history completeness (`:1568-1835`). Realloc predecessor verification is substantial enough to have its own [chapter](parser-realloc-deep.html).

`validate_gaps` verifies gap positions and ordering; `validate_coverage` distinguishes configured user-code coverage from observed-PID heap coverage; `validate_diagnostics` checks report shapes and optional absence. A crash, step cap, missing Valgrind report or partial memory read is a recorded outcome, not corruption by itself.

## What this layer does not do

It does not execute C, evaluate DWARF location expressions, infer an active union member, prove initialization from bytes, or turn an AST line match into a dynamic call. It does not certify all fork descendants. It does not make an independent Valgrind report a primary-run allocation ID. It reports corruption separately from incompleteness so consumers can refuse wrong answers without confusing them with missing evidence.

## Tests

`tests/trace_parser.rs` and `tests/trace_parser_realloc_f01.rs` exercise malformed imports, forged pointer/process claims, realloc links and valid partial traces. `examples/trace_parser_tp8_mutate.rs` and `examples/trace_parser_tp8_cycles.rs` are preserved audit probes; they are not DOC-3 reruns. The historical TP-8 audit's conditional verdict predates F01; the current source includes the corroboration checks described here.

## Diagram notes

### Validation control flow

{{diagram:d3-validation-flow}}

### Finding classification

{{diagram:d3-findings}}
<!-- END CHAPTER: content/parser-validation-deep.md -->

<!-- BEGIN CHAPTER: content/parser-realloc-deep.md -->
# 51 · Realloc provenance and TP8-F01

> **In one sentence:** `realloc` can resize or move a block, fail, or in the observed glibc case free it when asked for size zero. Those cases must not be confused.

## Key terms

| Term | Plain meaning |
|---|---|
| **Predecessor** | The older allocation lifetime that a successful `realloc` actually continued from, if supported by recorded events. |
| **Corroborate** | Check other independent records—creation events, PID, sequence and lifetime boundaries—instead of trusting just one `old_id` number. |

> **Example.** An imported block says `realloc_from_id=2`, but its creation event is an ordinary `malloc`. The predecessor number exists, yet the event contradicts the claimed relationship; the parser rejects the link.
>
> **What not to assume:** A reused or unchanged address is never enough to establish a realloc edge. A failed realloc leaves the old block alive.

## The big picture

The TP-8 audit found a correctness defect: an imported heap object could name an existing but wrong predecessor ID, and validation treated ID existence as sufficient. TP8-F01 changed both deep validation and TP-6 confidence to require recorded creation/event/lifetime corroboration. The repair is in `realloc_link` and `validate_heap`; the raw `realloc_from_id` field is preserved for investigation.

## The defect in one sentence

An allocation ID is a number in a sparse identity space. It is not proof that a realloc event connected that predecessor to this successor. The old check could answer "the ID exists" and accidentally answer "the relationship is true."

## `realloc_link`

**Signature:** `pub(crate) fn realloc_link<'a>(block: &HeapBlockInfo, current: Option<&AllocationLifetime>, previous: Option<&AllocationLifetime>, events: impl Iterator<Item = &'a HeapEvent>, previous_events: impl Iterator<Item = &'a HeapEvent>) -> ReallocLink` (`src/trace_parser/validation.rs:1485-1566`).

`ReallocLink` is `Verified`, `Unavailable`, `Ambiguous` or `Contradictory` (`:1467-1473`). The function receives only events indexed for the successor and predecessor, not the whole log.

The algorithm in execution order:

1. If `block.realloc_from_id` is `None`, return `Unavailable`: there is no recorded predecessor claim.
2. If the predecessor ID equals the successor ID, return `Contradictory`: a non-terminal realloc cannot be its own predecessor.
3. If the predecessor ID is 0, return `Unavailable`: 0 is the untracked domain, not a lifetime identity.
4. Scan successor creation events:
   - terminal `realloc(ptr, 0)` rows are skipped;
   - an `Allocated` event for the successor is contradictory;
   - only `Reallocated` events can establish the link;
   - a different `old_allocation_id`, a non-`realloc` block type, a PID mismatch, an allocation sequence that does not match, or a free boundary before the event is contradictory;
   - a missing `old_allocation_id` sets `missing = true`.
5. Count predecessor creation events (`Allocated` or `Reallocated`, excluding terminal zero realloc). A PID or sequence mismatch is contradictory.
6. More than one successor match or predecessor creation is `Ambiguous`.
7. Missing corroboration, missing current/previous lifetime, a zero allocation sequence, or a predecessor without a free boundary is `Unavailable`.
8. Exactly one compatible successor creation and one predecessor creation, with compatible lifetimes, is `Verified`.

### Terminal zero-size realloc

`terminal_zero_realloc` recognizes the engine's recorded form: `Reallocated`, same ID as `old_allocation_id`, no address, and size 0 (`:1475-1483`). This is treated as a terminal free-shaped event, not a new lifetime or a self-link. The implementation does not claim this is universal C allocator behavior; it is the verified environment's recorded convention.

## Validation integration

`validate_heap` builds `known` IDs from heap state and lifetimes, `linked` successor IDs, `related` predecessor/successor IDs, and `creations` event lists (`src/trace_parser/validation.rs:1568-1638`). For each block with `realloc_from_id`:

- an unresolved origin is `REALLOC_ORIGIN_UNRESOLVED` (unavailable);
- `Contradictory` becomes `REALLOC_LINK_CONTRADICTORY` (structural corruption);
- `Ambiguous` or `Unavailable` becomes `REALLOC_LINK_UNVERIFIED` (unavailable);
- `Verified` produces no finding.

It also validates event-level origins (`REALLOC_EVENT_ORIGIN_CONTRADICTORY`) and walks predecessor chains with a `visited`/`in_path` set to report `REALLOC_LINK_CYCLE` (`:1692-1834`). A cycle is corruption even when events/lifetimes are missing. A terminal zero realloc is excluded from creation evidence at `:1475-1483`.

## Confidence propagation

TP-6 uses the same helper rather than duplicating the rule. `ValidatedSemanticView::heap` computes `identity_confidence` and `realloc_confidence` (`src/trace_parser/semantic.rs:225-286`):

- contradictory PID or malformed scope → contradictory;
- ID 0 or no lifetime/block → unavailable;
- complete observed-PID history with a verified process scope → definite identity;
- otherwise → candidate.

`realloc_link` maps to confidence:

| Helper result | Semantic confidence |
|---|---|
| `Verified` + definite identity | `Definite` |
| `Verified` + incomplete/legacy identity | `Candidate` |
| `Ambiguous` | `Ambiguous` |
| `Unavailable` / no block | `Unavailable` |
| `Contradictory` | `Contradictory` |

`qualify` upgrades every result to `Contradictory` if the whole report has any structural corruption (`src/trace_parser/semantic.rs:61-67`). This is deliberately conservative: one bad imported relationship prevents any query from claiming `Definite`, rather than letting a local lookup ignore a global contradiction.

## Special cases

- **In-place realloc:** same address, new size; a new lifetime may be linked to the predecessor only through recorded events.
- **Moved realloc:** predecessor closes at the recorded sequence; successor has a new ID and an explicit old ID.
- **Failed realloc:** old block remains alive; no successor link.
- **Address reuse:** two lifetimes may share an address; address equality never joins them.
- **Terminal `realloc(ptr,0)`:** the observed environment may free the old ID and return null; the event is terminal, not a predecessor edge.

## Tests and examples

`tests/trace_parser_realloc_f01.rs` covers fresh in-place/moved realloc, forged predecessors, missing/ambiguous creation, cycles, terminal zero realloc and confidence. `examples/trace_parser_tp8_audit.rs` and `examples/trace_parser_tp8_cycles.rs` are preserved probes. `tests/fixtures/stage1/realloc_move.json` is a minimal recorded move fixture. These tests validate the implementation; they are not a rerun of the full Stage 1 corpus for DOC-3.

## Diagram notes

### Valid predecessor chain

{{diagram:d3-realloc-valid}}

### Forged or cyclic predecessor

{{diagram:d3-realloc-forged}}

### Confidence decision flow

{{diagram:d3-realloc-confidence}}
<!-- END CHAPTER: content/parser-realloc-deep.md -->

<!-- BEGIN CHAPTER: content/parser-coverage-deep.md -->
# 52 · Pointer provenance and process coverage

> **In one sentence:** Two attractive shortcuts are unsafe: guessing where a pointer originated from its address, and claiming every child process was seen because the observed event logs are complete.

## Key terms

| Term | Plain meaning |
|---|---|
| **Address containment** | The number inside a pointer falls inside the current range of a recorded heap block. It does not tell us how the pointer was obtained. |
| **Observed-PID scope** | The particular processes that wrote events and whose available event history was checked; it is not a list of every fork descendant. |

> **Example.** A silent child can run without an event line. The known parent PID's log might be complete while the child's activity is still unknown.
>
> **What not to assume:** Treat imported claims of a definite allocation ID from `address_only`, or of all-descendant completeness, as contradictions rather than convenient facts.

## The big picture

Two imported claims are especially easy to forge because they look numeric: a pointer target with an allocation ID, and a heap history marked complete. TP-5 and TP8-F01 tightened both. The parser does not infer pointer origin from address containment, and it does not infer a process-tree census from event/ACK records.

## Pointer provenance decision table

`validate_pointer_reference` receives the variable, an identity map and a finding builder (`src/trace_parser/validation.rs:1306-1415`). If there is no recorded `target`, it returns without manufacturing one.

| Recorded target | Finding | Meaning |
|---|---|---|
| Heap ID with no `provenance_status` | `POINTER_HEAP_PROVENANCE_UNRECORDED` (unavailable) | Legacy metadata; do not infer origin |
| Any allocation ID with a status, or a non-heap definite ID | `POINTER_HEAP_PROVENANCE_CONTRADICTORY` (corruption) | Numeric/uncertain/freed/unknown evidence cannot claim definite identity |
| ID and `address_match_allocation_id` disagree | `POINTER_HEAP_ADDRESS_MISMATCH` (corruption) | Two recorded identity claims conflict |
| Resolved variable identity empty, `unresolved:`, or also a heap ID | `POINTER_VARIABLE_REFERENCE_INVALID` (corruption) | A variable target needs a definite non-heap identity |
| Identity not observed at this checkpoint | `POINTER_VARIABLE_REFERENCE_UNRESOLVED` (unavailable) | Capture caps/omission, not death |
| All same-identity candidates inactive | `POINTER_VARIABLE_REFERENCE_INACTIVE` (corruption) | A resolved pointer points only at inactive objects |
| Base + offset does not equal pointer | `POINTER_VARIABLE_OFFSET_MISMATCH` (corruption) | Arithmetic inconsistency |
| Pointer outside every recorded extent | `POINTER_VARIABLE_EXTENT_MISMATCH` (corruption) | Address cannot fit any observed object |

A numeric address match is always a candidate. `address_only`, `ambiguous_reuse`, `known_freed_address` and `history_incomplete` are not promoted to definite origin. A missing status is not blindly treated as corruption either: it becomes unavailable and tells the caller not to infer provenance.

### Forged `address_only` example

Suppose an imported variable has `points_to = "0x1000"`, `target.kind = "heap"`, `target.allocation_id = 7`, `target.address_match_allocation_id = 7`, and no `provenance_status`. Validation emits `POINTER_HEAP_PROVENANCE_UNRECORDED` (unavailable). TP-6's `PointerEvidence` keeps `definitive_allocation: None`, reports `address_candidate: Some(7)`, and assigns `Candidate` only for an address-only/known-freed status. It never returns allocation 7 as a definite origin.

If the imported status is `address_only` but `allocation_id` is also present, the second row makes it contradictory. The raw pointer record remains available for inspection; the relationship is not accepted.

## Heap process scope

`HeapProcessScope` is a typed view over the optional `HeapHistoryCoverage` (`src/trace_parser/views.rs:14-42`):

- `Unknown`: legacy metadata has no process-scope annotation;
- `VerifiedObservedPidsOnly { observed_event_pids, followed_inferior_pid, observed_history_complete }`: the explicit V2.2 scope;
- `Unsupported(other)`: an imported scope value outside the supported contract.

`TraceStore::heap_process_scope()` simply parses the recorded annotation (`:351-355`). It is not itself validation. `validate_heap_process_coverage` then checks it (`src/trace_parser/validation.rs:1839-1930`):

- `Unsupported` → `HEAP_PROCESS_SCOPE_UNSUPPORTED` corruption;
- `Unknown` plus an unobserved-process annotation → `HEAP_PROCESS_SCOPE_UNRECORDED` unavailable;
- `VerifiedObservedPidsOnly` without an unobserved annotation → `HEAP_UNOBSERVED_PROCESS_COVERAGE_UNRECORDED` unavailable;
- any unobserved annotation other than `unknown` → `HEAP_PROCESS_COVERAGE_UNSUPPORTED` corruption;
- PID 0, duplicate observed PIDs, or an event PID absent from the observed list → corruption;
- `complete = true` together with sequence gaps, missing events, malformed lines, transport errors, truncation or ID overflow → `HEAP_OBSERVED_COVERAGE_CONTRADICTORY` corruption.

### Silent-child mutation

The adversarial example changes `unobserved_process_coverage` from `unknown` to `all_children_observed` while retaining only observed event PIDs and the followed inferior. Validation has no event/ACK evidence for a silent child, so it emits `HEAP_PROCESS_COVERAGE_UNSUPPORTED` (structural corruption). The history can still be complete for observed PIDs, but that is not a descendant census.

## What completeness means

`heap_history.complete` is a flag about the **verified observed-PID event history**. It does not mean:

- every fork child was traced;
- every shared-library allocation was wrapped;
- a missing ACK was invented;
- a dropped snapshot was reconstructed.

`execution_status` and `coverage.complete_for_scope` answer a different question about configured user-code recording. A complete heap history and a partial user-code recording can coexist.

## Tests

`tests/trace_parser_semantic_tp6.rs` covers candidate pointers, forged imports, aliases, crash/gap and unsupported fork claims. `tests/trace_parser_realloc_f01.rs` covers process scope and sequence gates. `examples/trace_parser_tp8_mutate.rs` is the preserved mutation probe. These are source-verified tests, not a DOC-3 full-suite rerun.

## Diagram notes

### Pointer provenance decision flow

{{diagram:d3-pointer}}

### Process coverage decision flow

{{diagram:d3-process}}
<!-- END CHAPTER: content/parser-coverage-deep.md -->

<!-- BEGIN CHAPTER: content/parser-semantic-deep.md -->
# 53 · TP-6 validated semantic evidence

> **In one sentence:** This code turns validated raw records into answers with confidence labels. Each label belongs to the exact claim being made.

## Key terms

| Term | Plain meaning |
|---|---|
| **Evidence view** | A borrowed bundle of an original record, optional metadata and carefully scoped confidence about relationships. |
| **Valid byte range** | The captured prefix of an object's memory at one stop. It does not say the rest is zero or initialized. |

> **Example.** A variable view can say the displayed value was observed, while the pointer origin is unavailable and the byte capture is partial. Those labels do not have to agree.
>
> **What not to assume:** An AST call is a source candidate; an independent Valgrind report has no exact primary-run allocation ID.

## The big picture

TP-6 is not a C interpreter and not Stage 2's `src/semantic/`. It is a thin, validated evidence layer over the canonical trace. It borrows records, runs deep validation once, and attaches a confidence to each stated relationship. It never invents an object graph, an execution proof or a pointer origin.

## Construction and confidence

**Signature:** `pub fn validated_semantic(&self) -> ValidatedSemanticView<'_>` (`src/trace_parser/semantic.rs:36-44`). It stores `store: &'a TraceStore` and an owned `ValidationReport` (`:28-34`). Construction calls `validate_deep()` once. Queries borrow the store and do not rerun validation. `SemanticInputView::validated()` delegates to the same method (`:46-50`).

`EvidenceConfidence` is `Definite`, `Candidate`, `Ambiguous`, `Unavailable` or `Contradictory` (`:17-26`). It belongs to a **specific relationship**, not to the execution as a whole. A variable value can be definite while its pointer origin is unavailable. A heap identity can be definite while its realloc link is ambiguous.

`qualify` is intentionally global for corruption: if the report is not structurally sound, every confidence becomes `Contradictory` (`:61-67`). Partial coverage and unavailable evidence do not trigger this; they remain local qualifications. This prevents a consumer from reading one clean-looking field while another imported relationship is contradictory.

## Variable evidence

**Signature:** `pub fn variable(&self, reference: &EvidenceRef) -> QueryResult<VariableEvidence<'a>>` (`:72-129`). It resolves the reference, accepts only snapshot-variable, snapshot-global or snapshot-caller-variable evidence, and rejects other categories with `Unavailable`.

For the borrowed `VariableInfo`, it performs three independent metadata lookups:

- `type_ref` → `store.type_entry(id)`;
- `declaration_ref` → `store.dwarf_declaration(id)`;
- `source_declaration_ref` → `store.source_declaration(id)`.

These are optional static links. A missing link is `Unavailable`, not a negative claim. Value confidence is `Definite` only when `read_status == Valid` and the recorded value is non-empty; this is a debugger observation, not proof of C initialization (`:95-100`). Memory and pointer views are built separately.

`variable_target` resolves a recorded variable identity **at the same checkpoint** (`:134-201`). It scans frame-zero variables, globals and caller variables, counts duplicate identities, and returns `Ambiguous` if more than one match exists. A target that is inactive is `Contradictory`; an active target with unavailable source scope is `Candidate`; an active target with a valid pointer and recorded scope is `Definite`. It never searches another stop or equates names and addresses.

### Memory and pointer views

`MemoryEvidenceView::new` treats `valid_range` as a prefix of captured bytes, not the object's full extent or initialized bytes (`:444-484`). Full captured bytes are `Definite`; a partial/truncated prefix is `Candidate`; missing or inconsistent bytes are `Unavailable`. `unobserved_range` records the requested-but-uncaptured tail.

`PointerEvidence::new` (`:499-540`) maps the recorded provenance status:

| Status / evidence | Confidence |
|---|---|
| `ambiguous_reuse` | `Ambiguous` |
| `address_only`, `known_freed_address` | `Candidate` |
| `history_incomplete`, unknown status | `Unavailable` |
| null target with no status | `Definite` (null address, never heap identity) |
| variable identity without status | `Candidate` |
| no usable target/read | `Unavailable` |

`definitive_allocation` is always `None` under the present V2.2 status contract. An address candidate is exposed as `address_candidate`, not promoted.

## Heap evidence

`heap_reference` resolves a trace-scoped allocation event/lifetime/block reference to its allocation ID and delegates to `heap` (`:205-219`). A foreign reference fails before any numeric ID join.

`heap(allocation_id)` (`:225-287`) looks up the lifetime, final block, predecessor ID/lifetime and history through TP-2 indexes. `pid_conflict` is checked across events for the ID. Identity confidence is:

- contradictory on a PID conflict;
- unavailable for ID 0 or no lifetime/block;
- definite when history is complete, the lifetime PID is in the observed event PIDs, and the process scope is `VerifiedObservedPidsOnly`;
- candidate otherwise (recorded ID with incomplete/legacy scope).

`HeapEvidence` exposes the lifetime, block, old ID/lifetime, history, historical memory at the block's observed stop, identity confidence and realloc confidence (`:543-560`). Its `events()` iterator returns the original chronological events. A final state never asserts that intermediate events were retained.

## Source expressions and diagnostics

`source_expression(id)` (`:291-317`) borrows the AST expression, registry, candidate positions, ambiguities, referenced declaration and fusion summary. `heap_call_confidence` is `Ambiguous` if ambiguity records exist, `Candidate` if candidates exist, otherwise `Unavailable`; `execution_confidence` is always `Unavailable`. A source expression is static syntax, never a dynamic call proof.

`checkpoint(index)` returns the canonical checkpoint and its stop-local source/attempted position (`:321-328`). `execution()` exposes status, coverage, gaps, attempted and retained counts; confidence is definite only for complete configured coverage (`:330-343`). `diagnostics()` borrows compiler diagnostics, stage results, crash, sanitizer and Valgrind reports (`:345-353`). These are session-wide reports, not causal joins to a stop.

`valgrind_error(index)` returns an independent error plus trace-scoped reference. `source_confidence` is `Candidate` when source-site candidates exist, otherwise unavailable; `primary_allocation_id` is always `None` (`:357-374`). A Valgrind source line never becomes a primary-run allocation ID.

`process_coverage()` returns the typed scope, optional history and an unavailable confidence for unobserved descendants (`:378-384`). Unknown child activity remains unknown even if observed-PID history is complete.

## Tests and boundaries

`tests/trace_parser_semantic_tp6.rs` covers variable/pointer candidates, heap identity, source expressions, Valgrind, crash/gap and process scope. `tests/trace_parser_realloc_f01.rs` covers realloc confidence. The tests use real APIs and recorded traces; they do not prove every possible import. TP-6 deliberately stops at evidence: it does not infer C objects, initialization, active union members or complete process trees.

## Diagram notes

### Validated view construction

{{diagram:d3-semantic-build}}

### Evidence confidence classification

{{diagram:d3-confidence}}

### Variable evidence resolution

{{diagram:d3-variable-evidence}}

### Heap evidence resolution

{{diagram:d3-heap-evidence}}

## Diagram notes for static and diagnostic evidence

### Static source versus runtime evidence

{{diagram:d3-source-evidence}}

### Diagnostic evidence boundaries

{{diagram:d3-diagnostics}}
<!-- END CHAPTER: content/parser-semantic-deep.md -->

<!-- BEGIN CHAPTER: content/parser-consumer-deep.md -->
# 54 · TP-7 consumer evidence composition

> **In one sentence:** This chapter explains the bounded API a UI or analysis tool can call to inspect a checkpoint without creating a parallel evidence model.

## Key terms

| Term | Plain meaning |
|---|---|
| **Checkpoint summary** | A convenient starting point for one saved stop and its related, qualified questions. |
| **Explicit pagination** | The caller chooses a start and end for each slice, at most 128 records. More results require another request. |

> **Example.** Request the first 128 variables, process them, and then request the next 128. The summary never quietly turns '300 available' into '128 total.'
>
> **What not to assume:** A nearby allocation event or source expression is not automatically the cause of this particular stop.

## The big picture

TP-7 is the public composition layer. It takes one validated view and one checkpoint and returns borrowed evidence with explicit positions, bounded pages and confidence. It does not build a second trace, a global object graph or a guessed temporal join.

## `checkpoint_summary()`

**Signature:** `pub fn checkpoint_summary(&self, index: usize) -> QueryResult<CheckpointSummary<'_, 'a>>` (`src/trace_parser/consumer.rs:57-68`). It calls `checkpoint(index)`, creates a snapshot `EvidenceRef`, and stores both plus the validated view in `CheckpointSummary` (`:50-55`).

`CheckpointSummary` fields are intentionally small:

- `checkpoint: CheckpointEvidence<'a>` — canonical observation and context;
- `reference: EvidenceRef` — trace-scoped snapshot citation;
- `view: &'v ValidatedSemanticView<'a>` — one validation pass, reused.

`CheckpointEvidence` itself contains the borrowed `Observation`, `CheckpointContext` and confidence. The summary does not clone the snapshot or variables.

## Summary methods

`CheckpointSummary` exposes recorded facts directly: `snapshot_index`, `stop_id`, `attempted_instruction_position`, `pid`, `observation_complete`, `source_location` and `coverage` (`src/trace_parser/consumer.rs:116-144`). `session_diagnostics()` returns session-wide reports and explicitly does not join them to this stop (`:146-150`). This is important: a Valgrind error at the same source line is not evidence about this checkpoint's allocation.

### Variables and frames

`VariableScope` is `Local`, `Global` or `Caller(position)` (`:41-46`). `scope_len` resolves the selected vector (`:205-211`). `variable(scope, index)` checks bounds, creates the correct `EvidenceLocation`, and delegates to `view.variable()` (`:213-235`). `variables_page(scope, range)` applies the shared page gate and returns an iterator (`:239-246`).

`frame_activation(id)` (`:251-314`) resolves a **unique activation at this checkpoint**, not a cross-stop lifetime. It rejects ID 0 as `None`, detects duplicate caller activations and local/caller collisions, and returns `Unavailable` for ambiguous cases. Local activation confidence is candidate if any local lacks an activation ID; caller confidence is definite only when the caller has no capture errors. Globals are available separately through `VariableScope::Global`.

### Events, heap and gaps

`instruction_events_page(range)` (`:319-344`) uses only events explicitly tagged `checkpoint_id = Some(snapshot_index)`. The tag is definite; it is not proof of exact stop causality or source-expression execution. It computes the canonical start position with `partition_point` and returns borrowed events with references.

`heap_interval_page(range)` (`:349-373`) returns events between the previous retained snapshot's cumulative heap count and this one. The interval is a **retained-snapshot boundary**, not an exact event timestamp. Events inside it may belong to omitted stops; confidence is `Candidate`.

`gaps_in_attempted_window(range)` (`:379-416`) checks gap ordering, computes the previous retained snapshot's attempted position, and binary-searches gaps in `(previous, current]`. This is an attempted-position window, not a causal assignment to one stop.

### Source and reports

`source_expressions_page(range)` (`:422-470`) requires an optional AST registry and a recorded source location. It prefers the frame's `full_file` over the checkpoint's possibly relative file, but never joins by basename. It counts and pages matching expressions with a bounded on-demand scan; missing AST is `Unavailable`. Match confidence is `Unavailable` if cross-provider correlation is marked unavailable, otherwise `Candidate`. It never proves execution.

`valgrind_errors_page(range)` (`:71-83`) pages independent Valgrind errors. `heap_events_page(allocation, range)` (`:88-113`) resolves a trace-scoped allocation reference, pages its chronological event positions, and returns each event with its own reference. A foreign reference fails before numeric lookup.

## Bounded pagination

`MAX_CONSUMER_PAGE` is 128 (`src/trace_parser/consumer.rs:16-18`). `page(range, len, collection)`:

1. rejects `start > end` or `end > len` with `InvalidRange`;
2. rejects `end - start > 128` with `LimitExceeded`;
3. returns the half-open range unchanged.

There is no silent truncation and no clamping. `0..0` is a valid empty page. A page is a view into canonical positions; the iterator yields `QueryResult` items so a bad recorded reference is reported at its position.

### More-than-128 example

For a collection of 300 events, valid calls are `0..128`, `128..256`, and `256..300`. `0..129` returns `LimitExceeded { requested: 129, max: 128 }`; `250..400` returns `InvalidRange` if the collection has only 300 entries. The caller must advance pages explicitly. This is an illustrative API sequence, not a captured trace.

## Temporal relationships

TP-7 keeps four different associations separate:

| Evidence | Meaning |
|---|---|
| `instruction_events_page` | Events explicitly tagged with the checkpoint ID |
| `heap_interval_page` | Heap events between retained snapshot counters |
| `gaps_in_attempted_window` | Gaps in an attempted-position interval |
| `source_expressions_page` | AST expressions whose source span contains the checkpoint line |

Only the first is a definite recorded tag. Heap and gap associations are candidate boundary windows. Source matches are static candidates. None is an exact instruction timestamp or a pointer provenance proof.

## Tests

`tests/trace_parser_consumer_tp7.rs` covers summary construction, bounded pages, source-expression paging, heap intervals, allocations, crash/fork scopes and the 128-entry limit. `examples/stage1_public_consumer_audit.rs` is the stable public consumer example. The tests validate behavior; DOC-3 did not rerun the full Stage 1 suite.

## Diagram notes

### Checkpoint summary construction

{{diagram:d3-consumer-summary}}

### Bounded pagination

{{diagram:d3-pagination}}

### Temporal associations

{{diagram:d3-temporal}}
<!-- END CHAPTER: content/parser-consumer-deep.md -->

<!-- BEGIN CHAPTER: content/parser-data-structures.md -->
# 55 · Parser data structures and ownership reference

> **In one sentence:** This page names the Rust containers that hold saved evidence and the small indexes that point into it.

## Key terms

| Term | Plain meaning |
|---|---|
| **Vector** | An ordered list; a position such as `snapshots[3]` identifies the fourth saved snapshot, not necessarily debugger stop ID 3. |
| **Reference** | A checked pointer to an existing record. An `EvidenceRef` is also tied to its trace owner. |

> **Example.** The trace owns the actual heap events. An allocation index keeps the positions of events with ID 7, so a query can borrow the original event records.
>
> **What not to assume:** Do not compare unrelated numbers—event position, allocation ID, PID and memory address—as if they were one identity.

## The big picture

This page is a reference for the types that carry parser state. It is not a second schema manual: the authoritative wire definitions are in `src/models.rs`, and the parser-specific wrappers are in `src/trace_parser/`. The distinction between owned, shared, borrowed and indexed data is the key to reading the rest of the parser.

## `DebugSessionTrace` and `RecordingData`

`DebugSessionTrace` is the canonical imported/exported model (`src/models.rs:501-548`). Important top-level fields include execution limits/outcome, flattened `RecordingData`, tracing configuration, schema version, compile success/errors, optional DWARF/AST/fusion registries, snapshots, heap events/state, optional crash/Valgrind/sanitizer reports, and stage results. `RecordingData` (`:736-755`) owns disassembly, instruction events, source checkpoint positions, gaps, instruction count, coverage, storage/profiling, allocation lifetimes and optional heap history.

The parser never wraps this model in a second semantic object graph. `TraceStore` owns one `Arc<DebugSessionTrace>`; indexes store positions into these vectors. A snapshot's large byte buffers are not cloned by TP-2.

### `ExecutionSnapshot` and `VariableInfo`

`ExecutionSnapshot` (`src/models.rs:68-122`) owns the call stack, frame-zero variables, globals, caller frames, heap blocks, registers when present, and observation metadata. `VariableInfo` (`:124-180`) carries identity, scope/activation, type/declaration references, points-to target, raw bytes, memory evidence, read/bytes validity and source declaration reference. `MemoryEvidence` (`:190-205`) records requested/captured bytes, address, status and error.

These records are evidence, not C objects. A variable's `identity` distinguishes activations; a `MemoryEvidence` prefix does not establish initialization.

## `TraceStore`

`TraceStore` (`src/trace_parser/mod.rs:191-214`) is the public owner:

| Field | Type | Ownership |
|---|---|---|
| `trace` | `Arc<DebugSessionTrace>` | Shared canonical trace |
| `indexes` | `Arc<TraceIndexes>` | Shared position indexes |
| `source` | `TraceSourceMetadata` | Small path/provenance metadata |

`TraceStore` is cheap to clone; a clone shares both Arcs. `shared()` returns the trace Arc (`:354-357`). `raw()` and `semantic_input()` borrow the store; neither copies records. A `ValidatedSemanticView` borrows the store and owns one `ValidationReport`.

## `TraceIndexes` and friends

`TraceIndexes` (`src/trace_parser/indexes.rs:61-77`) contains `ExecutionsIndex`, `SourceIndex`, `AllocationIndex`, `MetadataIndex` and `OrderingFacts`.

- `ExecutionsIndex` owns two `Vec<u32>` CSR arrays (`:270-274`).
- `SourceIndex` owns interned `Arc<str>` paths, key lookup, CSR arrays and invalid-reference markers (`:376-397`).
- `AllocationIndex` owns `HashMap<u64, Vec<u32>>` and `HashMap<u64, u32>` (`:613-624`).
- `MetadataIndex` owns hash maps of `Arc<str>` keys to `u32` positions (`:167-175`).
- `OrderingFacts` is a `Copy` record of booleans (`:702-716`).

All positions refer back to the canonical trace. None owns a snapshot, instruction event, heap event or metadata payload. `estimated_bytes` is an approximation, not RSS.

## `EvidenceLocation`, `EvidenceRef`, `Evidence`

`EvidenceLocation` is a `Copy` enum of canonical collection positions and nested positions (`src/trace_parser/evidence.rs:21-66`). `EvidenceRef` owns a `Weak<DebugSessionTrace>` plus a location (`:68-75`). `Evidence<'a>` is a borrowed enum of references to original records (`:85-109`).

The invariant is: **a reference is meaningful only with its owner**. `resolve_evidence` checks `Weak::ptr_eq` before resolving (`:140-145`). A reference cannot outlive the store because the result borrows `'a`; a clone of the same store shares identity, while an independent load is foreign.

## `ValidationReport` and `ValidationFinding`

`ValidationReport` owns a vector of findings (`src/trace_parser/validation.rs:187-232`). `ValidationFinding` contains severity, code, detail, `ValidationLocation` and occurrence count (`:158-171`). `ValidationLocation` is a position in a canonical collection (`:75-156`). The report is owned by the validated view, not stored in the trace. `is_structurally_sound` and `is_partial` are separate queries.

## `ValidatedSemanticView` and `EvidenceConfidence`

`ValidatedSemanticView<'a>` (`src/trace_parser/semantic.rs:28-34`) owns a `ValidationReport` and borrows `&'a TraceStore`. `EvidenceConfidence` (`:17-26`) is a relationship qualifier: `Definite`, `Candidate`, `Ambiguous`, `Unavailable`, `Contradictory`.

The lifetime relationship is:

```text
TraceStore (owns Arc trace)
   ^ borrowed by
ValidatedSemanticView<'a> (owns report)
   ^ borrowed by
VariableEvidence<'a> / HeapEvidence<'a> / CheckpointSummary<'v, 'a>
```

A validated view cannot outlive its store. Evidence values borrow canonical records and cannot turn into owned semantic objects.

## `HeapEvidence` and `HeapProcessScope`

`HeapEvidence<'a>` (`src/trace_parser/semantic.rs:543-560`) borrows the store and exposes allocation ID, optional lifetime/block/old lifetime, history, historical memory, identity confidence and realloc confidence. Its `events()` method returns the TP-2 borrowed event iterator.

`HeapProcessScope<'a>` (`src/trace_parser/views.rs:14-42`) borrows the recorded observed PID list and followed inferior PID. It has `Unknown`, `VerifiedObservedPidsOnly` and `Unsupported` variants. It is a view, not a validator and not a descendant census.

## `CheckpointSummary` and evidence items

`CheckpointSummary<'v, 'a>` (`src/trace_parser/consumer.rs:50-55`) borrows a validated view and owns a `CheckpointEvidence<'a>` plus an `EvidenceRef`. `CheckpointEvidence` contains the canonical observation, context and confidence. The summary's methods return borrowed records or page iterators; they do not clone snapshots.

The item types (`VariableEvidence`, `InstructionEvidence`, `HeapIntervalEvidence`, `GapEvidence`, `SourceLocationEvidence`) carry a reference and a confidence. Their exact fields are in `semantic.rs` and `consumer.rs`; the common invariant is that a position plus an owner, not a copied payload, is the evidence identity.

## Invariants to check when changing code

- Every index position remains in bounds of its canonical collection.
- `EvidenceRef` is created only after position validation and retains the owner.
- `TraceStore` clones share Arc identity; independent loads do not.
- Deep validation is explicit and its report is not silently cached on the trace.
- A page range is half-open, bounded and never clamped.
- A confidence describes the stated relationship, not the whole execution.
- Source/AST/Valgrind metadata never becomes a primary runtime identity without the documented evidence.

## Diagram notes

### Ownership and references

{{diagram:d3-ownership}}

### Data-structure relationships

{{diagram:d3-lifetimes}}
<!-- END CHAPTER: content/parser-data-structures.md -->

<!-- BEGIN CHAPTER: content/parser-algorithms.md -->
# 56 · Parser algorithms and invariants

> **In one sentence:** The parser's algorithms explain how loading, indexing and checking work; none of them is a replay of the C program.

## Key terms

| Term | Plain meaning |
|---|---|
| **Binary search** | A fast search on a list whose order was checked first. If imported ordering is broken, the parser refuses this shortcut. |
| **Invariant** | A rule a query depends on, such as stop IDs increasing in the recorded order. |

> **Example.** To find a stop by ID, the parser first needs evidence that the snapshot list is ordered. An unsorted import produces an ordering error instead of a guessed nearest stop.
>
> **What not to assume:** Performance of an index does not make malformed imported relationships trustworthy.

## The big picture

This page collects the algorithms that are easy to misread from a method name: content detection, index construction, ordering-gated lookup, evidence resolution, deep-validation aggregation, realloc corroboration, confidence qualification and bounded paging. Each section names the invariant that makes the implementation correct.

## Content detection and ingestion gates

`from_reader`/`open` first reject oversized input, then `gates` rejects empty/whitespace and non-UTF-8 bytes (`src/trace_parser/mod.rs:232-238,990-995`). `load_trace_document` uses `json::detect_format` to distinguish a JSON document from journal-shaped text; journal-shaped input is rejected with `JournalReconstructionUnsupported` (`:973-985`). The detector is a dispatch aid, not a validation result. A malformed final journal line can classify as journal-shaped, but the typed journal parser still rejects it (`src/trace_parser/json.rs:69-105`; `src/trace_parser/journal.rs:180-220`).

Invariant: **format classification never upgrades malformed evidence into a valid trace.**

## Eager position indexes

`TraceIndexes::build` constructs five structures in one pass each (`src/trace_parser/indexes.rs:94-116`). The CSR builders use counting/prefix sums and a cursor; the source builder interns paths and deduplicates instruction IDs per source key (`:400-500`). The allocation builder maps IDs to positions without sorting canonical arrays (`:626-656`).

Invariant: **an index stores a position, not a record.** A canonical vector is never reordered or cloned to satisfy a query. `pos()` checks the `u32` bound against the ingestion cap (`:755-759`).

## Ordering-gated binary search

`OrderingFacts` verifies snapshot stop order, event stop/checkpoint order, heap counters and gap positions (`:694-747`). A query that needs binary search first checks the relevant boolean:

- `snapshot_at_stop` checks `snapshots_by_stop_sorted` (`src/trace_parser/mod.rs:554-567`);
- `events_at_stop` checks `events_by_stop_sorted` (`:578-589`);
- `events_at_checkpoint` checks `events_by_checkpoint_sorted` (`:596-615`);
- `heap_events_at_stop` checks `heap_counts_sorted` (`:634-664`).

Invariant: **a broken recorded ordering is corruption, not permission to guess a nearest match.** The dependent query returns `OrderingViolation`; raw canonical arrays remain available for investigation.

## Evidence reference resolution

`evidence_ref` validates a location with `resolve_location`, then binds the location to `Arc::downgrade(&self.trace)` (`src/trace_parser/evidence.rs:129-135`). `resolve_evidence` checks `Weak::ptr_eq` before resolving (`:140-145`). `resolve_location` performs direct `get` lookups and maps optional reports to `Unavailable`.

Invariant: **identity is owner + location.** Equal numeric positions from two independent loads are foreign. A reference cannot be serialized as a portable identity.

## Deep-validation aggregation

`validate_deep` runs the engine validator and a fixed sequence of passes (`src/trace_parser/validation.rs:243-267`). `Builder::push` groups by `(code, severity)`, keeps the first location and increments a count (`:814-841`). `finish` orders corruption first (`:843-855`).

Invariant: **missing evidence and corruption are different severities.** A partial trace can be structurally sound; an optional field can be absent; a malformed reference can be corruption. `is_structurally_sound()` only checks corruption.

## Realloc corroboration

`realloc_link` requires a recorded predecessor ID, one compatible successor creation, one predecessor creation, matching PID/sequence/lifetime boundaries, and no cycle (`src/trace_parser/validation.rs:1485-1566`). Terminal `realloc(ptr,0)` rows are excluded from creation evidence (`:1475-1483`). `validate_heap` reports contradictions, ambiguity/unverified links and cycles (`:1778-1834`).

Invariant: **ID existence is not relationship proof.** The semantic layer maps `Verified` to definite only when identity is also definite; otherwise it remains candidate (`src/trace_parser/semantic.rs:264-285`).

## Confidence qualification

`ValidatedSemanticView::qualify` changes every confidence to `Contradictory` if the report has any structural corruption (`src/trace_parser/semantic.rs:61-67`). Otherwise, each method supplies a relationship-specific confidence: value validity for a variable, observed PID scope for a heap identity, source candidate status for an AST call, and so on.

Invariant: **confidence is local in normal cases, global only for corruption.** A complete observed-PID heap history does not make a source expression executed or a silent child observed.

## Bounded consumer pages

`page` validates half-open bounds and rejects ranges wider than 128 (`src/trace_parser/consumer.rs:20-37`). Callers pass a canonical collection length; the iterator yields `QueryResult` items with positions and references. `checkpoint_summary` composes one checkpoint and one snapshot reference (`:57-68`); variable, event, heap, gap and source pages all use the same gate.

Invariant: **no silent truncation.** A 300-item collection is traversed as `0..128`, `128..256`, `256..300`; `0..129` is an explicit limit error.

## Performance boundaries

Loading is monolithic JSON deserialization plus one index build. Queries borrow canonical records; most are O(1) or O(log n) after ordering checks. A source-expression page is a bounded scan over the optional registry (capped at 4,096 expressions), not a line-expanded index. The parser's input byte cap and estimated index bytes are not a process RSS guarantee, and the engine's JSON export cap is not a parser memory cap. The historical 3,143-instruction workload is a benchmark input, not a universal bound.

## Diagram notes

### Parser pipeline

{{diagram:d3-pipeline}}

### Performance boundaries

{{diagram:d3-performance}}
<!-- END CHAPTER: content/parser-algorithms.md -->

<!-- BEGIN CHAPTER: content/parser-walkthroughs.md -->
# 57 · Parser cross-file walkthroughs

> **In one sentence:** These are short guided routes through parser code: start with one question and follow the records it touches.

## Key terms

| Term | Plain meaning |
|---|---|
| **Entry point** | The public function a caller uses first, such as `TraceStore::open` or `validated_semantic()`. |
| **Intermediate record** | An existing trace value or index position examined along the way; not a reconstructed missing observation. |

> **Example.** To inspect one variable, open the trace, validate it, locate its exact checkpoint reference, then ask TP-6 for a borrowed variable view and qualified memory evidence.
>
> **What not to assume:** An illustrated path shows how code can work, not that its optional branch ran in every trace.

## The big picture

These are source-guided subsystem sequences, not DOC-4 end-to-end execution stories. They follow real functions and name the data at each boundary. Unless a section explicitly cites a checked-in fixture or a fresh run, the sequence is illustrative. No walkthrough changes the trace or invents a missing relationship.

## A. Loading a JSON trace

**Entry:** `TraceStore::open(path, options)`.

**Call path:** `open` → `read_trace_bytes` → `load_trace_document` → `gates` → `json::decode_document` → `validate_schema_version` → `assemble` → `TraceIndexes::build` (`src/trace_parser/mod.rs:240-258,973-995`; `src/trace_parser/json.rs:19-46`).

**Input:** a schema-2 JSON file, or bytes/reader. **Intermediate:** UTF-8 `&str`, deserialized `DebugSessionTrace`, index builders over borrowed vectors. **Output:** `TraceStore { trace: Arc, indexes: Arc, source }`.

**Branches:** empty/non-UTF-8 fails before decode; oversized input fails at the byte gate; journal-shaped input fails reconstruction; schema 1/3 fails the schema gate. **Errors:** `EmptyInput`, `InvalidEncoding`, `LimitExceeded`, `MalformedJson`, `UnsupportedSchemaVersion`, `JournalReconstructionUnsupported`.

**Sequence:**

```text
open -> read_trace_bytes -> load_trace_document -> gates
     -> json::decode_document -> schema gate
     -> assemble -> TraceIndexes::build -> TraceStore
```

**Why:** loading is intentionally permissive about recorded partiality but strict about input identity. It does not run deep validation.

### A diagram

{{diagram:d3-story-load}}

## B. Resolving an instruction event

**Entry:** `TraceStore::instruction_event(index)` or `instruction_execution_positions(id)`.

**Call path:** direct vector `get` for an event (`src/trace_parser/mod.rs:448-458`); for a static instruction, `ExecutionsIndex::positions_of` → CSR slice → `instruction_events[position]` (`:744-756`; `src/trace_parser/indexes.rs:313-326`).

**Input:** a retained event index or a disassembly ID. **Intermediate:** CSR offsets/positions. **Output:** `&InstructionEvent` or a borrowed event iterator.

**Branches:** an out-of-range event index is `OutOfRange`; an invalid disassembly ID is `InvalidReference`; a missing source context is simply not indexed. **Why:** a static instruction can execute many times; the CSR preserves every event position in ascending order.

### B diagram

{{diagram:d3-story-instruction}}

## C. Resolving a variable

**Entry:** `ValidatedSemanticView::variable(&EvidenceRef)`.

**Call path:** `variable` → `resolve_evidence` → owner check → `resolve_location` → `type_entry`/`dwarf_declaration`/`source_declaration` → `VariableEvidence` (`src/trace_parser/semantic.rs:72-129`).

**Input:** a snapshot-variable, global or caller-variable reference. **Intermediate:** borrowed `VariableInfo`, optional static metadata. **Output:** value/type/declaration/source confidences, memory and pointer views.

**Branches:** wrong evidence category is `Unavailable`; missing static metadata is `Unavailable`; a valid debugger value is `Definite` as an observation; pointer status controls candidate/ambiguous/unavailable. **Why:** static links are optional, while the recorded value is primary evidence at that stop.

### C diagram

{{diagram:d3-story-variable}}

## D. Validating imported corruption

**Entry:** `TraceStore::validate_deep()`.

**Call path:** `validation::validate_deep` → `validate_snapshots` → `validate_pointer_reference`; `validate_heap` → `validate_heap_process_coverage` (`src/trace_parser/validation.rs:243-267,1306-1415,1839-1930`).

**Input:** a forged heap target with an allocation ID but no provenance status, or a process annotation claiming all children observed. **Intermediate:** identity map, recorded history, finding builder. **Output:** `ValidationReport` with unavailable/corruption findings.

**Branches:** missing status → `POINTER_HEAP_PROVENANCE_UNRECORDED`; definite claim from address-only → contradictory; unsupported child claim → `HEAP_PROCESS_COVERAGE_UNSUPPORTED`. **Why:** an address or a complete observed-PID flag cannot certify a process tree.

### D diagram

{{diagram:d3-story-corruption}}

## E. Realloc provenance: valid versus forged

**Entry:** `validate_heap` and TP-6 `heap`.

**Call path:** `validate_heap` → `realloc_link` → `Verified`/`Ambiguous`/`Unavailable`/`Contradictory`; TP-6 `heap` uses the same helper and maps it to confidence (`src/trace_parser/validation.rs:1485-1566,1778-1795`; `src/trace_parser/semantic.rs:264-285`).

**Input:** successor block, current/previous lifetimes, creation event iterators. **Intermediate:** match counts, PID/sequence checks, terminal-zero filter. **Output:** validation finding and/or heap confidence.

**Branches:** missing predecessor/creation → unavailable; multiple matches → ambiguous; contradictory type/PID/sequence/cycle → corruption; one compatible chain → verified. **Why:** the audit defect was treating an existing ID as proof; the repair requires event and lifetime corroboration.

### E diagram

{{diagram:d3-story-realloc}}

## F. Building a consumer checkpoint summary

**Entry:** `TraceStore::open` → `validated_semantic()` → `checkpoint_summary(index)`.

**Call path:** `checkpoint_summary` → `checkpoint` → `evidence_ref(Snapshot)`; then `variables_page`, `heap_events_page`, `valgrind_errors_page` (`src/trace_parser/consumer.rs:57-113,239-246`).

**Input:** one retained snapshot and a validated view. **Intermediate:** checkpoint evidence, snapshot reference, page ranges. **Output:** borrowed variables, heap events and independent diagnostics with confidences.

**Branches:** invalid snapshot → `OutOfRange`; foreign allocation reference → `ForeignTrace`; page >128 → `LimitExceeded`; missing optional report → `Unavailable`. **Why:** composition reuses one validation pass and never clones the checkpoint.

### F diagram

{{diagram:d3-story-summary}}

## G. Independent Valgrind evidence

**Entry:** `ValidatedSemanticView::valgrind_error(index)`.

**Call path:** `evidence_ref(ValgrindError(index))` → `resolve_evidence` → `ValgrindEvidence` (`src/trace_parser/semantic.rs:357-374`).

**Input:** an independent Valgrind report recorded by the engine. **Intermediate:** a trace-scoped error reference. **Output:** source-site candidates and `primary_allocation_id: None`.

**Branches:** missing report → `Unavailable`; candidates → `Candidate`; no candidates → `Unavailable`. **Why:** Valgrind ran in a separate process. A matching source line is not a primary-run allocation identity.

### G diagram

{{diagram:d3-story-valgrind}}

## H. Partial execution

**Entry:** `TraceStore::open` on a crash, timeout or step-limited trace.

**Call path:** load/index → `validate_deep` → `validated_semantic` → raw views and page queries (`src/trace_parser/mod.rs:240-408`; `src/trace_parser/semantic.rs:36-44`).

**Input:** a trace with `execution_status` such as `Crashed`, `Timeout` or `StepLimitReached`. **Intermediate:** report findings for partial coverage/unavailable evidence. **Output:** original records remain available; confidence is qualified; gaps and coverage stay explicit.

**Branches:** structural corruption only for broken invariants; partial coverage for caps/interruption; unavailable for absent optional evidence. **Why:** a partial trace is valid data, not a failed ingestion.

### H diagram

{{diagram:d3-story-partial}}

## Fixture and verification note

`tests/fixtures/stage1/realloc_move.json` and `tests/fixtures/journal/*.jsonl` are checked-in inputs used by parser tests. The Stage 1 audit's 383-test/16-trace corpus is historical evidence, not a DOC-3 rerun. The walkthroughs above are source-guided; DOC-4 will add fresh end-to-end execution stories.
<!-- END CHAPTER: content/parser-walkthroughs.md -->

<!-- BEGIN CHAPTER: content/parser-handbook.md -->
# 58 · Trace Parser source file handbook

> **In one sentence:** This is a guide to where parser behavior lives in the repository, organized by the question you are trying to answer.

## Key terms

| Term | Plain meaning |
|---|---|
| **TP** | Trace Parser phase; the numbered phases introduced loading, indexes, validation, evidence views and bounded consumer queries. |
| **Source chapter** | A focused explanation of the implementation in one parser file. |

> **Example.** If `TraceStore::open` rejects a file, read the ingestion chapter. If it loads but a realloc link is rejected, read validation and the realloc repair chapter.
>
> **What not to assume:** Finding a type or helper in the handbook does not mean that a particular imported trace passed its checks.

## The big picture

This is the DOC-3 coverage index. Every Stage 1 parser production file has a detailed chapter or a dedicated section; the table records the ownership boundary and where to read the implementation. `src/semantic/` is Stage 2 and is deliberately excluded. `src/trace_parser/semantic.rs` is the Stage 1 TP-6 evidence layer, not the Stage 2 semantic engine.

## How to read the parser

Start with [ingestion](parser-ingestion-deep.html), then [ownership and evidence references](parser-ownership-deep.html), then [indexes](parser-indexes-deep.html). Read [deep validation](parser-validation-deep.html) before relying on [TP-6 confidence](parser-semantic-deep.html), and finish with [TP-7 consumer composition](parser-consumer-deep.html). The [algorithm reference](parser-algorithms.html) collects cross-file invariants; the [function reference](parser-functions.html) is generated from reviewed graph nodes.

### Production files and detailed treatment

| File | Responsibility | Detailed treatment |
|---|---|---|
| `src/trace_parser/mod.rs` | Loading, gates, schema dispatch, canonical store, raw queries, journal wrappers | [Ingestion](parser-ingestion-deep.html), [ownership](parser-ownership-deep.html), [indexes](parser-indexes-deep.html) |
| `src/trace_parser/json.rs` | JSON document decode, schema gate, content-based format detection | [Ingestion](parser-ingestion-deep.html) |
| `src/trace_parser/journal.rs` | Typed JSONL records, ordering, torn tail, record limits | [Ingestion](parser-ingestion-deep.html) |
| `src/trace_parser/error.rs` | `TraceParseError`, `QueryError`, ingestion/query boundary | [Ingestion](parser-ingestion-deep.html) |
| `src/trace_parser/indexes.rs` | Position-only execution/source/allocation/metadata indexes and ordering facts | [Indexes](parser-indexes-deep.html) |
| `src/trace_parser/evidence.rs` | `EvidenceLocation`, `EvidenceRef`, borrowed `Evidence` resolution | [Ownership and references](parser-ownership-deep.html) |
| `src/trace_parser/views.rs` | Raw/semantic input views, checkpoint contexts, process scope | [Ownership and references](parser-ownership-deep.html) |
| `src/trace_parser/validation.rs` | Deep validation passes, report model, pointer/process/realloc checks | [Validation](parser-validation-deep.html), [realloc](parser-realloc-deep.html), [coverage](parser-coverage-deep.html) |
| `src/trace_parser/semantic.rs` | TP-6 validated evidence, confidence, variable/heap/source/diagnostic views | [TP-6](parser-semantic-deep.html) |
| `src/trace_parser/consumer.rs` | TP-7 checkpoint summaries, bounded pages and evidence items | [TP-7](parser-consumer-deep.html) |

### Shared model boundary

The parser borrows the engine's `DebugSessionTrace` model from `src/models.rs`. DOC-2 covers how the engine constructs that model; DOC-3 only documents how the parser reads and validates it. The engine's JSON export limit is not a parser memory bound, and a schema-valid import is not a semantic validation result.

### Coverage status

The machine-readable [inventory](assets/data/stage1-inventory.json) marks these files as having a detailed chapter. Graph-linked functions carry reviewed purpose and classified relationships. Other indexed functions remain source-located but not independently walked through; DOC-3 does not turn a lexical definition into a behavioral proof.

### Excluded

`src/semantic/` (SE-1/SE-2), Stage 2 tests/reports, frontend implementation and Trace Engine internals are outside DOC-3. Cross-links to DOC-2 are intentional and do not duplicate engine documentation.
<!-- END CHAPTER: content/parser-handbook.md -->

<!-- BEGIN CHAPTER: content/parser-functions.md -->
# 59 · Trace Parser function reference

> **In one sentence:** This catalog helps you find reviewed parser functions and the source lines that implement them.

## Key terms

| Term | Plain meaning |
|---|---|
| **Public query** | A function other code may call to read existing trace evidence. |
| **Reviewed relationship** | A call or data link checked against source and labeled by what it actually means. |

> **Example.** Find `checkpoint_summary`, then follow its source link. Its page queries keep the original evidence references and enforce the 128-record page limit.
>
> **What not to assume:** A function list is not a run log. Look at the trace to see which observations actually exist.

## The big picture

This page lists parser functions with a reviewed entry in the DOC-3 call-flow graph. Each entry shows the source location, actual signature, purpose, inputs/outputs, qualification and classified incoming/outgoing relationships. It is a navigation aid, not a claim that every parser helper has an independent walkthrough.

## Scope

A reviewed entry is not a runtime trace claim. Direct edges were checked at call expressions; borrowed-data, index/reference and validation dependencies retain their own labels. Functions absent here can be found in the complete [function index](functions.html) or the [source handbook](parser-handbook.html). DOC-3 prioritizes ingestion, ownership, indexing, validation, TP-6 and TP-7; less central helpers remain source-located for later expansion.

### Review status

The inventory marks graph-matched symbols as `explained_and_graph_linked`. Other parser symbols remain `file_chapter_indexed_only`. This distinction prevents a large but unsupported function encyclopedia.

### Parser graph entries

{{inventory:parser-functions}}

## Related reading

See [algorithms and invariants](parser-algorithms.html), the eight [walkthroughs](parser-walkthroughs.html), and the interactive [call-flow explorer](call-flow.html). Engine counterparts remain in the [DOC-2 function reference](engine-functions.html).
<!-- END CHAPTER: content/parser-functions.md -->

<!-- BEGIN CHAPTER: content/walkthroughs.md -->
# 60 · End-to-end execution walkthroughs

> **In one sentence:** These are complete stories from a C example through recording and later reading. Use one story at a time rather than memorizing every subsystem first.

## Key terms

| Term | Plain meaning |
|---|---|
| **Route** | The ordered steps from a C action, through GCC/GDB/wrapper, into saved JSON and finally a parser query. |
| **Oracle** | An independently checked expected fact used by a test, such as C's `sizeof` or a native GDB value. |

> **Example.** In the heap story, follow `malloc` to a wrapper event, then a lifetime record, then an indexed query. Ask at each arrow whether the next tool saw this *same* execution.
>
> **What not to assume:** Illustrative event rows and historical audit measurements are labeled; do not treat them as the current program's addresses or complete importable JSON.

## The big picture

These twelve stories connect the DOC-2 engine chapters and DOC-3 parser chapters into continuous source-guided paths. They are not a replacement for the subsystem pages. The normal-execution and parser stories reuse fresh smoke runs recorded in DOC-2/DOC-3; the crash, recursion, fork, heap and adversarial cases use the final Stage 1 audit corpus and are explicitly labeled as historical audit evidence where applicable. Illustrative data is marked as such.

## 1. Complete normal C execution

**Scenario.** `fixtures/c/phase8b_defined.c` defines a struct, global, alias, heap array, recursion and realloc. The DOC-2 smoke run and the final audit both report 45 snapshots, 157 retained instruction events and 3 heap events for this fixture. The trace is a normal dual-mode recording with optional AST metadata.

**Route.** CLI/config → GCC → DWARF → GDB stop loop → trace assembly → JSON export → parser load/index → validation → TP-6/TP-7.

**Call path.**

1. `main` parses CLI and calls `Cli::to_config` (`src/main.rs:26-34`; `src/cli.rs:197`).
2. `trace::trace_program` creates the supervisor scope and calls `trace_program_inner` (`src/trace.rs:132-167`).
3. `compiler::compile_project` builds the wrapped debug binary and optional sanitizer artifacts (`src/trace.rs:235`; `src/compiler.rs:529-581`).
4. `dwarf::extract_excluding` records static metadata (`src/trace.rs:355`; `src/dwarf.rs:584`).
5. `run_gdb_phase` launches GDB and enters the stop loop (`src/trace.rs:1113-1191`). The MI reader parses asynchronous stops; a tokened `^done` is not a stop (`src/gdb_mi.rs:533-671`).
6. At a stop, the loop drains event rows, captures state and appends a snapshot/checkpoint (`src/trace.rs:1387-1405,1577-1677`; `src/gdb_mi.rs:1840-2227`).
7. `trace_program_inner` reconstructs heap history, associates DWARF and runs `evidence::fuse` (`src/trace.rs:851-968`).
8. `main` exports the returned `DebugSessionTrace` through `json_export` (`src/main.rs:50-57`; `src/json_export.rs:152-167`).
9. `TraceStore::open` gates, decodes and assembles the canonical store (`src/trace_parser/mod.rs:240-258`).
10. `validated_semantic` runs one deep pass; `checkpoint_summary` composes a bounded consumer view (`src/trace_parser/semantic.rs:38`; `src/trace_parser/consumer.rs:58`).

**Intermediate data.** A stopped frame has `stop_id`, source location, frame PC and activation path. A snapshot owns variables, caller frames, heap blocks and raw bytes. The JSON has instruction events, source-checkpoint positions, heap events/lifetimes, stage results and coverage. The parser adds only indexes and reports; it does not change the trace.

**Failure and uncertainty.** A missing optional AST registry, a partial memory read or a nonterminal limit remains partial/unavailable. A crash or timeout can still produce a valid trace. The parser's schema load does not certify structural soundness.

**Why this design.** The engine is a recorder, not an interpreter. The parser is an immutable reader. Separating export from load prevents a live debugger from becoming a query dependency.

**Navigate.** [Engine architecture](engine-architecture.html), [GDB controller](engine-gdb-deep.html), [parser ingestion](parser-ingestion-deep.html), [parser walkthroughs](parser-walkthroughs.html).

### Diagram

{{diagram:d4-1}}

## 2. Function calls, stack frames and recursion

**Scenario.** `fixtures/c/recursive.c` calls `recurse(3)`; the final audit records 26 snapshots, 69 retained instructions and 45 caller-frame records with `max_caller_frames=3`.

**Route.** GCC → GDB frame query → activation path → snapshot caller frames → parser frame-scoped variables.

**Call path.** `run_gdb_phase` observes a user-code stop (`src/trace.rs:1518-1568`); `GdbSession::capture_state` requests `-stack-list-frames 0 200` and computes an oldest-to-youngest activation path (`src/gdb_mi.rs:1840-1910`); `capture_caller` records caller frames without changing GDB's selected frame (`:1753-1834`); `Recorder::checkpoint` retains the snapshot (`src/recorder.rs:259-265`); `TraceStore::checkpoint_summary` and `CheckpointSummary::frame_activation` expose one activation (`src/trace_parser/consumer.rs:58,251`).

**Intermediate data.** Each frame has a level, function, file/line, PC and activation ID. A local variable identity is `activation:<id>:<scope>:<name>`; two recursive activations with the same name remain distinct. A caller variable is stored in `caller_frames`, not merged into frame zero.

**Failure and uncertainty.** A truncated frame chain clears activation continuity rather than reusing a sliding prefix. A duplicate caller activation is ambiguous; an ID 0 is no activation. A caller capture error remains visible.

**Why this design.** GDB can print the same function name at several levels. Activation IDs preserve the observed distinction without inventing a cross-stop variable lifetime.

**Navigate.** [Stack and memory](engine-stack-deep.html), [GDB capture](engine-gdb-deep.html), [TP-7 frames](parser-consumer-deep.html).

### Diagram

{{diagram:d4-2}}

## 3. Heap allocation

**Scenario.** `phase8b_defined.c` performs `malloc(3 * sizeof(int))`; the audit records 3 heap events for the run.

**Route.** C wrapper → event file → Rust reader → HeapTimeline → trace lifetime → parser index → consumer allocation page.

**Call path.** `__wrap_malloc` calls the real allocator and `cte_emit_locked` (`c_runtime/alloc_tracker.c:343-353,266`); the event is appended to the PID-local event file. `EventFileReader::read_new` accepts complete rows (`src/allocation_tracker.rs:293`); `HeapTimeline::ingest` verifies the per-PID sequence and updates block state (`:521`); `push_snapshot` records alive blocks (`src/trace.rs:2179`). The parser builds `AllocationIndex` by allocation ID (`src/trace_parser/indexes.rs:627`), `ValidatedSemanticView::heap` joins lifetime/state/events (`src/trace_parser/semantic.rs:225`), and `heap_events_page` returns borrowed events (`src/trace_parser/consumer.rs:88`).

**Intermediate data.** An illustrative event row is:

```json
{"kind":"malloc","id":1,"addr":"0x55d0a0002a10","size":32,"pid":100,"seq":4,"caller":"0x401136"}
```

The parser indexes the numeric ID, never the address. A final trace may widen IDs when multiple PIDs are observed; raw event IDs remain PID-local.

**Failure and uncertainty.** A partial write, malformed row, sequence gap or missing ACK lowers completeness. A later event cannot retroactively certify missing rows. An allocation ID is not an exact instruction timestamp.

**Why this design.** Address reuse is legal in C, so identity must come from wrapper events and lifetimes. A file boundary keeps the C runtime independent of Rust.

**Navigate.** [C allocator runtime](engine-allocator-deep.html), [heap reconstruction](engine-heap-deep.html), [parser indexes](parser-indexes-deep.html).

### Diagram

{{diagram:d4-3}}

## 4. Free, address reuse and realloc

**Scenario.** `fixtures/c/heap_provenance.c` frees and reallocates at a reused address; `realloc_move.c` moves a block; `heap_provenance_realloc.c` covers success, failure and glibc's terminal `realloc(ptr,0)`. The final audit reports 44/125/7 for reuse, 12/37/5 for moved and 51/153/15 for the realloc fixture.

**Route.** Wrapper status rows → HeapTimeline → lifetime records → `realloc_link` → TP-6 confidence.

**Call path.** `__wrap_realloc` emits `failed`, `ok_same`, `ok_moved` or the observed terminal zero form (`c_runtime/alloc_tracker.c:387-419`). `HeapTimeline::ingest` applies status-specific transitions (`src/allocation_tracker.rs:606-699`). `validate_heap` calls `realloc_link` (`src/trace_parser/validation.rs:1568-1795`), and `ValidatedSemanticView::heap` maps the helper result to confidence (`src/trace_parser/semantic.rs:264-285`).

**Valid and rejected relationships.** A moved realloc has one compatible successor creation, one predecessor creation, matching PID/sequence and compatible lifetimes; `realloc_link` returns `Verified`. A forged `realloc_from_id` with no compatible creation is `Unavailable`; conflicting type/PID/sequence or a cycle is `Contradictory`. A terminal `realloc(ptr,0)` is a free-shaped terminal event, not a predecessor edge. A failed realloc leaves the old block alive.

**Intermediate data.** A lifetime records allocation ID, PID, native sequence, address, size, free boundary and optional `realloc_from_id`. Equal addresses across two lifetimes do not merge them.

**Why this design.** The TP8-F01 audit showed that ID existence is not relationship proof. The repair requires event and lifetime corroboration and preserves raw fields for investigation.

**Navigate.** [Realloc and TP8-F01](parser-realloc-deep.html), [heap evidence](parser-semantic-deep.html), [heap reconstruction](engine-heap-deep.html).

### Diagram

{{diagram:d4-4}}

## 5. Pointer observation and provenance

**Scenario.** `heap_provenance.c` keeps a stale pointer after `free`, then observes a new allocation at the same address. The audit records ambiguous pointer evidence, not a definite origin.

**Route.** GDB variable read → pointer target → observed maps/heap candidates → validation → `PointerEvidence`.

**Call path.** `GdbSession::capture_state` records the variable's value/address/extent (`src/gdb_mi.rs:2054-2171`); `memory::pointer_target` compares the numeric address with heap/stack/global extents (`src/memory.rs:260-340`); `validate_pointer_reference` checks status, identity, offset and extent (`src/trace_parser/validation.rs:1306-1415`); `PointerEvidence::new` maps status to confidence (`src/trace_parser/semantic.rs:499-540`).

**Intermediate data.** A pointer target may contain `kind`, `address`, `allocation_id`, `address_match_allocation_id`, `provenance_status` and `points_to`. A numeric match is `address_only`/`ambiguous_reuse`; a definite ID requires compatible recorded provenance. A freed address is not a live object.

**Failure and uncertainty.** A missing status is unavailable, not permission to infer. A definite ID with address-only evidence is contradictory. A pointer variable is not its target object; a target variable identity is resolved only at the same checkpoint.

**Why this design.** Address containment is cheap and useful, but C permits reuse and pointer arithmetic. The parser preserves the observation and refuses to upgrade a candidate.

**Navigate.** [Pointer and process coverage](parser-coverage-deep.html), [stack and memory](engine-stack-deep.html), [TP-6 heap/pointer evidence](parser-semantic-deep.html).

### Diagram

{{diagram:d4-5}}

## 6. DWARF type information

**Scenario.** `fixtures/c/dwarf_types.c` defines `struct Node`, `struct Container`, arrays, bitfields, typedefs and a union. `phase8b_defined.c` defines `struct Packet`.

**Route.** GCC debug ELF → DWARF CU/DIE traversal → type/declaration registry → runtime variable association → parser metadata lookup.

**Call path.** `compiler::compile_project` produces the debug binary; `dwarf::extract_excluding` enumerates CUs and DIEs (`src/dwarf.rs:584`); `OriginResolver` follows specification/abstract-origin references with a four-hop/cycle bound (`:213-423`); `dwarf::associate` links unique runtime declarations (`:1045`); the parser indexes type/declaration IDs (`src/trace_parser/indexes.rs:178`) and `type_entry` borrows a record (`src/trace_parser/views.rs:369`); `ValidatedSemanticView::variable` exposes optional type/declaration confidence (`src/trace_parser/semantic.rs:72`).

**Intermediate data.** A type entry records kind, size/alignment where known, members, array bounds, bitfields, declaration refs and CU identity. A variable's `type_ref`/`declaration_ref` are trace-local IDs, not pointers into the engine process.

**Failure and uncertainty.** Missing optional DWARF is unavailable. An unresolved cross-CU chain is not replaced by a same-name declaration in another CU. A type does not prove initialization or an active union member.

**Why this design.** Static metadata must remain CU-qualified and bounded. The parser borrows it rather than decoding a Stage 2 typed-memory graph.

**Navigate.** [DWARF extraction](engine-dwarf-deep.html), [Clang AST](engine-ast-deep.html), [parser data structures](parser-data-structures.html).

### Diagram

{{diagram:d4-6}}

## 7. Optional Clang AST and evidence fusion

**Scenario.** `fixtures/c/ast_source.c` contains declarations, a macro allocation, direct calls and source expressions.

**Route.** GCC flags → optional Clang JSON AST → AST registry → evidence fusion → trace cross-references → parser source expression.

**Call path.** `trace_program_inner` invokes the optional Clang pass (`src/trace.rs:371`; `src/clang_ast.rs:228`); `evidence::fuse` runs once after runtime/heap records (`src/evidence/mod.rs:33-91`); declaration linking uses CU/DIE-compatible unique matches (`src/evidence/declarations.rs:132-153`); heap-call correlation is site-only and records candidates/ambiguities (`src/clang_ast.rs:947-1054`); the parser exposes `source_expression` (`src/trace_parser/semantic.rs:291`) and TP-7 source pages (`src/trace_parser/consumer.rs:422`).

**Intermediate data.** An AST expression has a source span, declaration/expression IDs and optional macro spelling location. A heap-call candidate is a source line plus ambiguity records, not an allocation ID.

**Failure and uncertainty.** Unsupported GNU extensions or preprocessing differences make correlation unavailable. Multiple calls on one line are ambiguous. A source expression never proves dynamic execution.

**Why this design.** AST syntax is valuable for navigation, but C execution is observed by GDB and allocation telemetry. Fusion is additive and audited.

**Navigate.** [Clang AST](engine-ast-deep.html), [evidence fusion](engine-fusion-deep.html), [parser source expressions](parser-consumer-deep.html).

### Diagram

{{diagram:d4-7}}

## 8. Crash or segmentation fault

**Scenario.** `fixtures/c/phase8r3_crash.c` writes through a null pointer. The final audit records 3 snapshots, 5 retained instructions and 0 heap events; the run is a crash with partial evidence.

**Route.** GDB fatal-signal stop → partial capture → crash report → optional diagnostics → partial JSON → parser validation → diagnostic query.

**Call path.** `run_gdb_phase` classifies a fatal signal, captures state and builds a crash report (`src/trace.rs:1441-1518`); optional sanitizer/Valgrind passes run separately when configured (`src/trace.rs:644-849`); final assembly preserves coverage and stage results (`src/trace.rs:851-1070`); the parser's `validate_deep` reports partial/unavailable evidence (`src/trace_parser/validation.rs:243-267`); `diagnostics()` and `valgrind_error()` expose reports (`src/trace_parser/semantic.rs:345-374`).

**Intermediate data.** A crash report has a signal, source/PC site and optional notes. A snapshot at the fatal stop may be partial. Sanitizer/Valgrind reports are independent diagnostics, not primary-run heap identities.

**Failure and uncertainty.** A missing report is unavailable. A crash does not prove every earlier instruction was retained. A source line in Valgrind is not a primary allocation ID.

**Why this design.** A crash is a valid recorded outcome, not an ingestion error. Partiality is preserved so consumers can distinguish missing evidence from corruption.

**Navigate.** [GDB controller](engine-gdb-deep.html), [diagnostics](engine-diagnostics-deep.html), [parser coverage](parser-coverage-deep.html).

### Diagram

{{diagram:d4-8}}

## 9. Infinite loop and resource limits

**Scenario.** `fixtures/c/infinite_loop.c` loops on a volatile counter. The final audit records 60 snapshots and 203 retained instructions for `infinite_loop`, with `step_limit_reached` and incomplete execution scope.

**Route.** Supervisor deadline/step caps → stop loop → gaps/partial events → partial JSON → parser partial view.

**Call path.** `supervisor::scope` owns the deadline (`src/trace.rs:132-164`); `run_gdb_phase` checks command/step/terminal limits and records a gap when it stops early (`src/trace.rs:1371-1568,1954-2024`); `Recorder::gap` stores an attempted-position interval (`src/recorder.rs:287-305`); export preserves `Timeout`/`StepLimitReached` and coverage; the parser reports partial coverage and retains raw events (`src/trace_parser/validation.rs:1934-2014`).

**Intermediate data.** Attempted instruction count can exceed retained event count. A gap has an origin/reason and instruction position, not invented events. The trace may have snapshots but no proof of termination.

**Failure and uncertainty.** A step limit is not a proof of nontermination. A missing after-stop is not completion. A partial trace is valid data; the parser does not synthesize the unrecorded tail.

**Why this design.** Resource controls are separate from evidence completeness. The engine records what it observed and why it stopped.

**Navigate.** [Supervision and limits](engine-supervision-deep.html), [GDB stop loop](engine-gdb-deep.html), [parser algorithms](parser-algorithms.html).

### Diagram

{{diagram:d4-9}}

## 10. Forked process and heap coverage

**Scenario.** `fixtures/c/phase8dr_fork.c` supports parent, observed-child, silent-child and incomplete-ACK modes. The final audit reports two event PIDs for observed, one event PID for silent, and two with incomplete ACK; none proves a descendant census.

**Route.** fork/atfork child reset → PID-local event files → reader/ACK verification → heap history → parser process scope.

**Call path.** The C runtime resets child lock/counters/IDs and defers table clearing (`c_runtime/alloc_tracker.c:128-143`); parent and logged child append PID-local rows; `EventFileReader` reads rows and `verify_event_status` checks per-observed-PID ACKs (`src/allocation_tracker.rs:178-400`); `HeapTimeline` keeps per-PID sequences; `HeapProcessScope::from_history` exposes the recorded scope (`src/trace_parser/views.rs:28`); `validate_heap_process_coverage` rejects unsupported child claims (`src/trace_parser/validation.rs:1839-1930`).

**Intermediate data.** The history has observed event PIDs, followed inferior PID, unobserved-process annotation and completeness. Final JSON may widen allocation IDs when multiple PIDs are observed; raw event IDs remain PID-local.

**Failure and uncertainty.** A silent child may produce no events/ACK. A missing ACK lowers completeness. Complete observed-PID history is not all-descendant coverage.

**Why this design.** Fork can duplicate locks and allocator state. PID-local events prevent cross-process identity collisions; absence of evidence remains unknown.

**Navigate.** [C allocator runtime](engine-allocator-deep.html), [heap reconstruction](engine-heap-deep.html), [parser coverage](parser-coverage-deep.html).

### Diagram

{{diagram:d4-10}}

## 11. Trace Parser from the inside

**Scenario.** A completed schema-2 JSON trace, such as `examples/v21_heap.json` used by the DOC-3 smoke run.

**Route.** Load → schema gate → Arc store → indexes → views → deep validation → TP-6 → TP-7 pages.

**Call path.** `TraceStore::open` reads/gates/decodes and calls `assemble` (`src/trace_parser/mod.rs:240-258,973-995`); `TraceIndexes::build` constructs position indexes (`src/trace_parser/indexes.rs:94`); `raw()`/`semantic_input()` borrow the store (`src/trace_parser/views.rs:433-440`); `validate_deep` runs explicit passes (`src/trace_parser/validation.rs:243`); `validated_semantic` retains one report (`src/trace_parser/semantic.rs:38`); `checkpoint_summary` composes a snapshot reference (`src/trace_parser/consumer.rs:58`); `variables_page` and `heap_events_page` return bounded borrowed items (`:239,88`).

**Intermediate data.** A `TraceStore` owns `Arc<DebugSessionTrace>` and `Arc<TraceIndexes>`; a `CheckpointContext` borrows one snapshot; an `EvidenceRef` holds a weak owner identity plus a location. The parser never clones the trace to answer a query.

**Failure and uncertainty.** A foreign reference fails before numeric lookup; an invalid range/page fails explicitly; a partial trace can be structurally sound but partial. Loading does not run deep validation.

**Why this design.** Canonical ownership makes references safe and cheap. Indexes store positions, not payloads. Validation is explicit and confidence is relationship-specific.

**Navigate.** [Parser ingestion](parser-ingestion-deep.html), [ownership](parser-ownership-deep.html), [indexes](parser-indexes-deep.html), [TP-6](parser-semantic-deep.html), [TP-7](parser-consumer-deep.html).

### Diagram

{{diagram:d4-11}}

## 12. Corrupted imported evidence

**Scenario.** Schema-compatible mutations of fresh traces: a pointer given a matching address candidate's allocation ID, a silent-fork trace claiming all children observed, and a forged realloc predecessor. The final audit reports structural findings and no definite semantic promotion.

**Route.** Mutated JSON → schema-only load → indexes → deep validation → finding → confidence downgrade → consumer result.

**Call path.** `TraceStore::open` accepts schema-compatible bytes; `validate_deep` runs `validate_snapshots` → `validate_pointer_reference`, `validate_heap` → `validate_heap_process_coverage`, and `validate_heap` → `realloc_link` (`src/trace_parser/validation.rs:243-267,1306-1415,1568-1930`); `validated_semantic` retains the report; `qualify` forces `Contradictory` when structural corruption exists (`src/trace_parser/semantic.rs:61-67`); TP-7 pages still expose raw records but do not claim definite relationships.

**Observed outcomes.**

| Mutation | Finding / disposition |
|---|---|
| Pointer `address_only` given matching address candidate's ID | `POINTER_HEAP_PROVENANCE_CONTRADICTORY`; structurally unsound |
| Silent fork `unobserved_process_coverage` forged as all children observed | `HEAP_PROCESS_COVERAGE_UNSUPPORTED`; observed-PID scope remains distinct |
| `realloc_from_id` forged to successor ID | `REALLOC_LINK_CONTRADICTORY` and/or `REALLOC_LINK_CYCLE`; not definite |

**Intermediate data.** The raw mutated field remains in the trace. Validation reports the first location and occurrence count; semantic confidence describes the relationship, not the whole trace.

**Failure and uncertainty.** Deserialization success is not evidence trustworthiness. A consumer must inspect `is_structurally_sound()` and `is_partial()` separately. A missing optional field is not corruption; a contradictory relationship is.

**Why this design.** Raw preservation supports debugging; confidence prevents false certainty. TP8-F01's shared helper ensures parser validation and TP-6 agree.

**Navigate.** [Validation](parser-validation-deep.html), [realloc](parser-realloc-deep.html), [coverage](parser-coverage-deep.html), [TP-6](parser-semantic-deep.html).

### Diagram

{{diagram:d4-12}}

## Source navigation and verification note

The normal-execution counts and parser smoke are fresh DOC-2/DOC-3 runs. The recursion, heap, crash, loop, fork and adversarial counts are from `docs/stage1-final-audit.md` and are historical audit evidence, not DOC-4 reruns. Source links use the inspected dirty worktree at `558037c0da180e152a3e6cc3f351edf0b0583215`. Illustrative JSON/MI rows are labeled and are not claimed as captured output.
<!-- END CHAPTER: content/walkthroughs.md -->

<!-- BEGIN CHAPTER: content/usage.md -->
# 61 · Practical usage · from C file to borrowed evidence

> **In one sentence:** This page is the hands-on route: record a small C program, open the resulting JSON, and ask one careful question at a time.

## Key terms

| Term | Plain meaning |
|---|---|
| **CLI** | The commands and flags you type to run the trace engine. |
| **Consumer** | Your Rust code reading the finished trace through the parser's public API. |

> **Example.** After recording, check the program's outcome and the validation report. Then request a checkpoint's first page; if there are more than 128 results, request the next page explicitly.
>
> **What not to assume:** The example's chosen stop and address are observations from one run, not identifiers you can reuse in another run.

## Level 1 · Intuition

Run the engine to obtain a JSON document, then parse **that document** and check its deep-validation report before asking what happened at a checkpoint. One parser store can serve many readers. Observations stay historical; a missing diagnosis or event is not a proven absence.

{{diagram:consumer}}

## Level 2 · Run a recording

From the repository root on the supported Linux toolchain:

```sh
cargo build --release
target/release/c-trace-engine fixtures/c/phase8b_defined.c \
  --trace-mode dual --max-steps 350 --timeout 50 \
  --clang-ast --sanitizer-trigger never \
  --journal /tmp/visual-c-session.jsonl \
  --no-pretty --output /tmp/visual-c-session.json
python3 -m json.tool /tmp/visual-c-session.json > /tmp/visual-c-readable.json
```

GCC/GDB must be installed; Clang is optional for this command's `--clang-ast` pass (without it, remove the flag). Valgrind may run as an independent stage if available; `--sanitizer-trigger never` disables the separate sanitizer run. Do not pass `--` before engine options; only **program arguments** follow `--`. The engine writes one full JSON document to `--output`; JSONL can be parsed for records but **not** loaded as a complete `TraceStore`. Journals are never overwritten: choose a fresh `--journal` path for each repeat run (or remove your previous audit-owned temporary file first). See [compilation](engine-build.html), [deadline policy](engine-limits.html) and the [input schema](parser-ingestion.html).

### A real captured pointer, carefully read

The audited `phase8b_defined.c` trace's frame-0 `heap` variable at snapshot 4 recorded `points_to="0x705010"` and the following *abridged, exact-field* pointer target:

```json
{"kind":"heap","allocation_id":null,"base_address":"0x705010","offset_bytes":0,"active":true,"within_extent":true,"variable_identity":null,"address_match_allocation_id":1,"provenance_status":"address_only"}
```

The raw event at sequence 1 created allocation ID 1 at `0x705010`. **Matching the numeric address did not certify the pointer's provenance**: `allocation_id` stayed null and the evidence layer's `definitive_allocation` remains `None`. Addresses/PIDs can change in new runs; do not hardcode these values. At the same run's `checkpoint` line 15, snapshot 36 / GDB stop 119 / attempted instruction position 116 are **different identifiers**; `sum` displayed `"10"` with bytes `"0a000000"`.

## Level 3 · Load and validate

The following is a **complete** Rust example body for `examples/stage1_docs_consumer.rs`; it uses only public APIs. Save it there and run `cargo run --example stage1_docs_consumer -- /tmp/visual-c-session.json` (or use the checked-in larger audit consumer `cargo run --release --example stage1_public_consumer_audit -- /tmp/visual-c-session.json`).

```rust
use c_trace_engine::trace_parser::{
    EvidenceLocation, TraceLoadOptions, TraceStore, VariableScope, MAX_CONSUMER_PAGE,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("provide trace JSON path")?;
    let store = TraceStore::open(&path, TraceLoadOptions::default())?;
    let view = store.validated_semantic(); // one deep validation pass
    for finding in view.report().findings() {
        eprintln!("{}: {} ({})", finding.severity.label(), finding.code, finding.location);
    }
    if !view.report().is_structurally_sound() {
        return Err("contradictory imported relationships; inspect raw evidence".into());
    }
    println!("status={:?} partial={}", view.execution().status, view.report().is_partial());
    for index in 0..store.trace().snapshots.len() {
        let stop = view.checkpoint_summary(index)?;
        assert_eq!(stop.reference.location(), EvidenceLocation::Snapshot(index));
        let locals = stop.checkpoint.context.variables().len();
        for start in (0..locals).step_by(MAX_CONSUMER_PAGE) {
            let end = (start + MAX_CONSUMER_PAGE).min(locals);
            for entry in stop.variables_page(VariableScope::Local, start..end)? {
                let var = entry?;
                println!("{}: {:?}", var.info.name, var.value_confidence);
                if let Some(ptr) = &var.pointer {
                    println!("address candidate={:?}; provenance={:?}",
                        ptr.address_candidate, ptr.definitive_allocation);
                }
            }
        }
    }
    let process = view.process_coverage();
    println!("observed scope={:?}; descendants={:?}",
             process.scope, process.unobserved_descendants);
    Ok(())
}
```

### Checkpoints and paging

This example iterates all retained checkpoints and requests half-open pages of ≤128 locals. It never suppresses a page error or promotes address containment to provenance. Other page lengths come from the actual context (`globals()`, `caller_frames()`), event slice (`events_at_checkpoint()?`) or allocation-position index (`heap_event_positions_for_allocation(id)`); always request subsequent ranges explicitly. Use `view.heap_reference(&ref)` for trace-scoped heap evidence, `view.diagnostics()` for independent reports, and [`CheckpointSummary`](parser-consumer.html) for per-stop composition. The [API reference](api.html) details the signatures.

### Troubleshooting

| Symptom | What to check |
|---|---|
| `CompileFailed`, no snapshots | `compile_errors`, `stage_results.compiler` and tool versions; a syntax error is not an empty successful run |
| `ForeignTrace` | Do not pass `EvidenceRef` from a separately loaded JSON, even if bytes are identical |
| `Unavailable` AST/bytes | Was `--clang-ast` enabled? Was memory readable and within its budget? Unknown is not zero |
| `StepLimitReached` / missing heap ACK | Consult coverage, gaps, heap history, stage results and partial findings |
| `InvalidRange` / `LimitExceeded` | Use `start..end` with `end≤len` and `end-start≤128`; do not truncate silently |
| `REALLOC_LINK_CONTRADICTORY` | Imported predecessor disagrees with event/lifetime; inspect [verification rules](parser-validation.html#realloc-link-verification) |

> NOTE: A structurally sound report can still be partial or missing optional evidence. Treat the report and each per-relationship confidence as **different questions**.
<!-- END CHAPTER: content/usage.md -->

<!-- BEGIN CHAPTER: content/api.md -->
# 62 · Public API reference · Trace Engine and Parser

> **In one sentence:** An API is the set of functions another Rust program is allowed to call. This reference states their inputs, outputs and limits.

## Key terms

| Term | Plain meaning |
|---|---|
| **Borrowed** | The returned view points to records owned by the `TraceStore`; it cannot outlive that store. |
| **Out of range** | A request for a position that does not exist in a recorded collection; it is an error, not an empty fabricated record. |

> **Example.** Load a trace, call `validated_semantic()`, then ask for an actual snapshot index and an explicitly bounded page of its variables. The result points into the same saved trace.
>
> **What not to assume:** An API signature is not a runnable full example; use the practical usage page for imports and setup.

## Level 1 · Intuition

The engine creates a `DebugSessionTrace` from C source; the parser makes that finished recording navigable. **Loading, deep validation and semantic confidence are three separate operations.** Use trace-scoped references for identity, explicit bounded pages for display, and the report for import integrity.

{{diagram:handoff}}

## Level 2 · Ownership and query path

`TraceConfig`/`BuildConfig` → async `trace_program` → JSON export; then `TraceStore::open` → `Arc<DebugSessionTrace>` + `Arc<TraceIndexes>` → `validated_semantic()` → borrowed `CheckpointSummary`/`EvidenceRef`. Cloning a store shares owners; constructing an independent store changes reference identity. All queries borrow canonical records; `source_expression(id)` and bare `heap(id)` are **trace-local string/numeric lookups**, not portable identity tokens. A whole-trace `ValidationReport` is created once per validated view; pages do not revalidate.

## Level 3 · Signatures and contracts

### Record and load

| Public item (path) | Result / contract |
|---|---|
| `trace_program(config: TraceConfig) -> Result<DebugSessionTrace>` (`src/trace.rs`, async) | Runs supervised engine; failure trace is still a structured result when possible |
| `TraceStore::open(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<Self>` | Schema-2 JSON file; 512 MiB default input cap |
| `TraceStore::from_json_bytes(bytes: &[u8], options: TraceLoadOptions) -> Result<Self>` | Explicit one JSON document |
| `TraceStore::from_reader(reader: impl Read, options: TraceLoadOptions) -> Result<Self>` | Bounded stream read |
| `TraceStore::from_trace(trace: DebugSessionTrace) -> Result<Self>` | **Schema-only** gate; does not run `validate_deep` |
| `parse_journal(path, options) -> Result<JournalTrace>` | Ordered JSONL **records only**, no reconstructed TraceStore |
| `store.trace() -> &DebugSessionTrace`, `store.shared() -> Arc<DebugSessionTrace>` | Canonical immutable owner; clone shares indexes |

### Raw and referenced navigation

| Public item | Use |
|---|---|
| `store.snapshot(index)`, `instruction_event(index)`, `instruction_range(range)` | Original position/half-open borrowed records; raw TP-2 ranges are **not** TP-7 pages |
| `store.events_at_checkpoint(snapshot_index)`, `snapshot_at_stop(stop_id)` | Explicit tag run and verified-order stop lookup; stop ID is not snapshot index |
| `store.instruction_executions(instruction_id)`, `executed_events_for_source(file,line)` | Preserve repeated dynamic visits; exact recorded source strings |
| `store.heap_events_for_allocation(id)`, `heap_block(id)`, `allocation_lifetime(id)` | Trace-local allocation ID, not address or event position |
| `store.raw()`, `store.semantic_input().checkpoint_context(index)` | Borrowed canonical source and checkpoint context |
| `store.evidence_ref(location)`, `store.resolve_evidence(&reference)` | Trace-scoped `EvidenceRef`; foreign store → `ForeignTrace` |

### Validation and qualified evidence

| Public item | Contract |
|---|---|
| `store.validate_deep() -> ValidationReport` | Opt-in structural, partial, unavailable and optional findings |
| `store.validated_semantic() -> ValidatedSemanticView<'_>` | Once-deep-validated borrower; `view.report()` must be inspected |
| `view.variable(&EvidenceRef)`, `view.variable_target(&VariableEvidence)` | Variable/byte/pointer and same-stop variable-identity relationship |
| `view.heap(id)`, `view.heap_reference(&EvidenceRef)` | Identity and *separate* realloc predecessor confidence |
| `view.source_expression(id)` | Static metadata and call candidates; **not execution** |
| `view.checkpoint(index)`, `view.execution()`, `view.process_coverage()` | Stop and independent execution/process dimensions |
| `view.diagnostics()`, `view.valgrind_error(index)` | Independent reports; never a primary-run Valgrind allocation ID |

### TP-7 pages (all `range: Range<usize>` are half-open)

| Public item | Returned evidence |
|---|---|
| `view.checkpoint_summary(index)` | Borrowed `CheckpointSummary<'_, '_>` and snapshot reference |
| `stop.variables_page(VariableScope::{Local, Global, Caller(position)}, range)` | TP-6 variable confidence; caller *position* is not GDB level |
| `stop.instruction_events_page(range)`, `stop.heap_interval_page(range)` | Explicit tag / candidate interval; not exact event cause |
| `stop.gaps_in_attempted_window(range)`, `stop.source_expressions_page(range)` | Position-window gaps / optional static syntax |
| `stop.caller_frame(position)`, `stop.frame_activation(id)` | Same-snapshot activation; missing association is not guessed |
| `view.heap_events_page(&EvidenceRef, range)`, `view.valgrind_errors_page(range)` | Trace-scoped allocation event rows / independent diagnostics |
| `stop.coverage()`, `stop.session_diagnostics()`, `stop.first_location_findings()` | Distinct scope, session reports and finding's first location |

All TP-7 page widths are `≤ MAX_CONSUMER_PAGE` (128); an oversize request returns `QueryError::LimitExceeded`, not a truncated iterator. Raw `TraceStore::instruction_range` has its separate slice API; **do not misstate the TP-7 bound as a universal input/trace-size cap**. See [pagination implementation](parser-consumer.html#level-3-real-signatures-and-bounded-traversal) and the [runnable example](usage.html#level-3-load-and-validate).

> NOTE: Type annotations above summarize the public methods; some pages return `impl Iterator<Item=QueryResult<_>>` rather than a copied vector. Consult the named Rust source for complete generic and lifetime parameters.
<!-- END CHAPTER: content/api.md -->

<!-- BEGIN CHAPTER: content/stage1-api.md -->
# 63 · Stage 1 public API reference

> **In one sentence:** This is a compact map of what Stage 1 gives to other code. It distinguishes loading the recording from checking its claims.

## Key terms

| Term | Plain meaning |
|---|---|
| **TraceStore** | The owner of one loaded trace and its lookup indexes. |
| **Validated view** | A read-only interface made after the deeper consistency check, with confidence attached to specific questions. |

> **Example.** A consumer can follow an evidence reference to a variable at stop 4, inspect its captured bytes, and still report the variable's pointer origin as unknown.
>
> **What not to assume:** Do not treat `TraceStore::from_trace` or successful JSON import as a deep semantic validation.

## The big picture

This is the downstream boundary for a recorded Stage 1 trace. It is not a Stage 2 semantic engine. The API returns borrowed evidence, explicit confidence and bounded pages; it does not infer C objects, pointer origin, initialization or process-tree completeness.

## Imports and ownership

```rust
use c_trace_engine::trace_parser::{
    EvidenceConfidence, EvidenceRef, QueryError, TraceLoadOptions, TraceStore,
};
```

`TraceStore::open(path, options)` returns an owned handle with `Arc<DebugSessionTrace>` and `Arc<TraceIndexes>` (`src/trace_parser/mod.rs:240-258`). Cloning the store is cheap and shares the same trace identity. Loading the same JSON again creates an independent owner; its references are foreign.

## Load and validate

```rust
let store = TraceStore::open("trace.json", TraceLoadOptions::default())?;
let validated = store.validated_semantic();
let report = store.validate_deep();

assert!(report.is_structurally_sound());
```

`validated_semantic()` runs `validate_deep()` once and retains the report (`src/trace_parser/semantic.rs:38`). Queries do not rerun validation. `is_structurally_sound()` is separate from `is_partial()`: a partial trace can be structurally sound, and an optional field can be absent without corruption.

## Checkpoint summary

```rust
let summary = validated.checkpoint_summary(index)?;
let stop = summary.stop_id();
let source = summary.source_location();
let locals = summary.variables_page(
    c_trace_engine::trace_parser::VariableScope::Local,
    0..16,
)?;
```

`checkpoint_summary(index)` resolves a snapshot and creates a trace-scoped snapshot `EvidenceRef` (`src/trace_parser/consumer.rs:58`). The summary borrows the validated view and the canonical store; it does not clone the snapshot. `stop_id()` converts the model's zero sentinel to `None`.

## Variables, frames and pages

`VariableScope` is `Local`, `Global` or `Caller(position)` (`src/trace_parser/consumer.rs:41-46`). `variables_page` returns an exact-size iterator of `QueryResult<VariableEvidence>`; `frame_activation(id)` resolves a unique activation at that checkpoint, not a cross-stop lifetime (`:251`). `instruction_events_page(range)` pages events explicitly tagged with the checkpoint ID (`:319`). `heap_interval_page(range)` pages heap events between retained snapshot counters and is a candidate temporal association (`:349`). `source_expressions_page(range)` pages AST source-span candidates and never proves execution (`:422`).

All pages use `MAX_CONSUMER_PAGE = 128` and a half-open range. A page wider than 128 returns `QueryError::LimitExceeded`; an invalid or reversed range returns `InvalidRange`. There is no silent truncation.

## Heap and diagnostics

`validated.heap(allocation_id)` joins lifetime, final block, history and realloc confidence (`src/trace_parser/semantic.rs:225`). `validated.heap_reference(&evidence_ref)` resolves a trace-scoped allocation reference first (`:205`). `validated.valgrind_error(index)` returns an independent Valgrind error; `primary_allocation_id` is always `None` (`:357`). `validated.diagnostics()` borrows compiler/crash/sanitizer/Valgrind reports (`:345`). `validated.process_coverage()` exposes the recorded observed-PID scope and leaves unknown descendants unavailable (`:378`).

## Confidence handling

`EvidenceConfidence` is `Definite`, `Candidate`, `Ambiguous`, `Unavailable` or `Contradictory` (`src/trace_parser/semantic.rs:20`). It belongs to a relationship. A variable value can be definite while its pointer origin is unavailable. A heap identity can be definite while its realloc link is ambiguous. Any structural corruption makes `qualify` return `Contradictory` for all relationships (`:61-67`).

Do not merge confidence with execution status, memory readability, initialization, scope activity, heap coverage or process-tree completeness.

## Evidence references and errors

An `EvidenceRef` stores a `Weak<DebugSessionTrace>` and an `EvidenceLocation` (`src/trace_parser/evidence.rs:68-75`). `resolve_evidence` checks owner identity before resolving the position (`:140-145`). A reference from an independent load is `QueryError::ForeignTrace`; an invalid position is `InvalidReference`; missing optional evidence is `Unavailable`. Raw records remain accessible through `store.raw()` even when a semantic relationship is unavailable.

## Deliberate limits

Stage 1 leaves C-language interpretation to downstream code. It does not execute source, evaluate DWARF location expressions, infer an active union member, turn a source candidate into an executed expression, turn an address match into allocation origin, or turn observed-PID completeness into a descendant census. Those are not missing convenience features; they are evidence-contract boundaries.

## Diagram

{{diagram:d3-pipeline}}

## Related reading

[Parser ingestion](parser-ingestion-deep.html), [ownership and evidence references](parser-ownership-deep.html), [TP-6](parser-semantic-deep.html), [TP-7](parser-consumer-deep.html), [limitations](limitations.html).
<!-- END CHAPTER: content/stage1-api.md -->

<!-- BEGIN CHAPTER: content/schema.md -->
# 64 · Schema v2 and identity reference

> **In one sentence:** The schema lists the JSON fields that save a recording. The most important reading habit is to ask what each number identifies.

## Key terms

| Term | Plain meaning |
|---|---|
| **Schema version** | The format version. The supported V2.1 and V2.2 traces here use version 2; not every older example file can be imported. |
| **Nullable** | A field can say 'no value recorded.' That is not the same as 'the value was zero' or 'nothing happened.' |

> **Example.** An event can have `allocation_id=2`, while its `address` is `0x705010`. A later block can also live at `0x705010` with a different ID. Keep the ID and address separate.
>
> **What not to assume:** Short JSON projections show selected fields for study, not full documents to paste into the parser.

## Level 1 · Intuition

A source line, stopped PC, snapshot position, instruction attempt, allocation lifetime and process address are different kinds of information. Mixing their integers creates convincing but false explanations. The model deliberately records explicit references, nullable fields, scopes and gaps so readers can say **what was observed and what remains unknown**.

{{diagram:records}}

## Level 2 · Top-level JSON

The complete JSON trace is represented in Rust by `DebugSessionTrace` in `src/models.rs`. Its `RecordingData` fields are **flattened**: instead of appearing under a `recording` object, items such as `snapshots` appear at the top level. Think of the fields in groups. `compiled_successfully` and `compile_errors` describe the build; `snapshots`, `instruction_events`, `source_checkpoints`, `trace_gaps` and `coverage` describe the debugger recording; `heap_events`, `heap_state`, `allocation_lifetimes` and `heap_history` describe observed allocator activity. `stage_results`, `execution_status`, `resource_usage`, `execution_limits` and `storage` describe outcomes and limits. `type_registry`, `source_metadata`, `evidence_fusion` and crash or memory-error reports may be optional. **Null or missing optional data does not mean the check succeeded.** `schema_version=2` works for supported V2.1/V2.2 files, not every older schema-2 example: four pre-V2.1 files contain a removed limit key and do not import. Older register fields may be parsed for compatibility but are no longer exported for each instruction.

### Identity domains

| Domain | Where recorded | What it is **not** |
|---|---|---|
| Snapshot vector index / `step_index` | `snapshots[i]` | GDB stop counter or instruction attempt |
| GDB `stop_id` | Snapshot observation, event before/after, journal wrapper | Snapshot index; not necessarily consecutive |
| `instruction_position`, `instruction_count` | Stop/gap/trace | Retained event position; journal-only can retain zero in JSON |
| `event_id` / event vector position | `instruction_events[j]` | `instruction_id` (disassembly table) or unique source line |
| `checkpoint_id` | InstructionEvent | Exact execution at that stop; only most recent checkpoint tag |
| `source_checkpoints[n]` | RecordingData | Duplicated snapshot; it stores snapshot index |
| `allocation_id`, `old_allocation_id` | Heap events, state, lifetime | Process address, event index or pointer provenance; `0` untracked |
| `sequence`, `pid` | Heap runtime | Stop/event positions; sequences start per PID |
| `activation_id`, `lexical_scope_id` | Frame/variable | Source variable name or address |
| `address`, `points_to`, `next_address` | Memory/event/variable | Identity; numeric addresses can recur |

### Heap records

`HeapEvent` stores `event_type=allocated|reallocated|freed|allocation_failed`, per-PID `sequence`, `pid`, `allocation_id`, optional `old_allocation_id`, address, size, caller and detail. `AllocationLifetime` stores allocation ID, PID, allocation sequence and optional free sequence. `HeapBlockInfo` stores final/historical state and optionally `realloc_from_id`; each snapshot can store alive `heap_blocks` as observed at a stop. These are complementary, **not one-to-one byte replicas**. Successful realloc requires [relationship verification](parser-validation.html#realloc-link-verification). `HeapHistoryCoverage.complete` is scoped to verified observed event PIDs plus followed inferior, with unknown descendants.

## Level 3 · Accurate field-level examples

The following is a **projection of fields from a captured `phase8b_defined.c` run**, not a complete importable trace. OS-specific PID/address values differ between runs. Its first two events describe an in-place realloc that starts a new lifetime ID; the third frees the successor:

```json
{"sequence":1,"allocation_id":1,"old_allocation_id":null,"event_type":"allocated","address":"0x705010","size_bytes":12}
{"sequence":2,"allocation_id":2,"old_allocation_id":1,"event_type":"reallocated","address":"0x705010","size_bytes":16}
{"sequence":3,"allocation_id":2,"old_allocation_id":null,"event_type":"freed","address":"0x705010","size_bytes":16}
```

`raw_bytes_hex="0a000000"` at an audited stop is a byte capture, not proof the C initializer ran. A separately recorded `PointerTarget` can report `address_match_allocation_id=1` and `allocation_id=null`, because **address containment is not origin**. To load a full trace, run the [CLI walkthrough](usage.html#level-2-run-a-recording); do not paste these projections into `TraceStore::open`.

> NOTE: JSON export's `max_json_export_bytes` bounds the serialized document; neither that nor the parser's input cap bounds already assembled engine trace RAM. [Resource policy](engine-limits.html#level-2-budgets-across-stages) lists the separate limits.
<!-- END CHAPTER: content/schema.md -->

<!-- BEGIN CHAPTER: content/handoff.md -->
# 65 · Stage 2 handoff · guarantees, limits and glossary

> **In one sentence:** Stage 1 hands later code evidence, not unquestionable conclusions. This page says exactly where that handoff ends.

## Key terms

| Term | Plain meaning |
|---|---|
| **Freeze contract** | The agreed, tested behavior and limits downstream code must preserve rather than silently broaden. |
| **Qualified evidence** | A recorded fact together with its scope, confidence, source and missing-data conditions. |

> **Example.** Stage 2 may draw a variable's observed bytes and label their capture status. It must not infer that a pointer owns the heap block whose current address matches it.
>
> **What not to assume:** Stage 1 is not a sandbox, a whole-process-tree recorder, an initialized-memory oracle or a guarantee of assembled-trace RAM.

## Level 1 · Intuition

Stage 1 hands future consumers a **validated, immutable, bounded-query evidence source**, not a complete reconstruction of C program meaning. A sound trace can be partial; an observed object can have uncertain origin. Stage 2 may derive explanations while displaying their evidence and qualifications, but must never turn a missing observation into a fact.

{{diagram:handoff}}

## Level 2 · The freeze contract

- Load a supported schema-v2 document through `TraceStore`; remember ingestion is schema-only. Call `validated_semantic()` and inspect `report().is_structurally_sound()`, `is_partial()` and individual findings.
- Share the canonical `Arc<DebugSessionTrace>`/`Arc<TraceIndexes>` via cloned `TraceStore`; use borrowed records and same-trace `EvidenceRef`, not numeric IDs from another execution.
- Page checkpoint variables, events and diagnostics using explicit half-open ranges no wider than **128**; request all subsequent pages. Distinguish `Unavailable`, empty evidence and an actual error.
- Maintain **separate** execution status, attempted/retained events, gaps, byte readability, lexical scope, initialization (often unknown), heap-history/observed PID scope and semantic relationship confidence.
- Keep heap realloc links dependent on recorded event **and** same-PID lifetime boundaries; no predecessor from numeric address equality. Never equate pointer containment to origin, AST syntax to execution, Valgrind source site to primary-run allocation ID or scoped PID completion to complete descendants.

## Level 3 · Concrete API handoff

```rust
let view = store.validated_semantic();
if !view.report().is_structurally_sound() {
    // Display contradictory imported relationships, not a definite explanation.
}
let scope = view.process_coverage();   // descendants remain unavailable
let stop = view.checkpoint_summary(snapshot_index)?;
let _ = (scope.unobserved_descendants, stop.reference.location());
```

This is a **body excerpt** using public APIs from `src/trace_parser/{semantic,consumer,evidence}.rs`; follow the [complete runnable example](usage.html#level-3-load-and-validate). The [API tables](api.html#level-3-signatures-and-contracts) list query signatures; [validation](parser-validation.html#realloc-link-verification) covers adversarial realloc imports. Stage 2 implementation is **outside the scope of this site**; this public boundary does not provide an execution interpreter, provenance graph or GUI.

### Performance (not a guarantee)

In the independent Stage 1 release-profile audit, a **3.337 MB**, 900-snapshot / **3,143-real-instruction-event** trace took approximately **48 ms** median typed parse, **1.13 ms** index build, **0.555 ms** once-only semantic validation and ~**17 MiB** parser-process RSS after indexing across three runs. A 461 KB small trace took ~8.27 ms parse and ~0.264 ms index build. The index estimate for the heavy sample was 28,369 bytes, **not** process RSS; per-query TP-7 measurements reported zero allocations but once-only validation allocated. Host, allocator and workload variation are material; these are not Stage 2 latency commitments. Input/JSON byte limits do not bound total RAM. See [limits](engine-limits.html).

### Known limitations

1. **Coverage:** user-code, selected-thread and followed-inferior stop scope; gaps, no complete all-thread or fork-descendant instruction timeline; silent children unknown.
2. **Heap:** only linked-object allocator wrappers and accepted telemetry; incomplete transport/ACK or legacy metadata lowers confidence. Pointer address containment is not allocation provenance. Historical bytes never get rewritten by later frees/reuse.
3. **Static/diagnostic:** AST candidates do not prove expression execution. DWARF metadata may be missing. Valgrind and sanitizers are independent processes; equal source/address does not establish primary allocation identity.
4. **Memory:** readable GDB bytes and lexical scope do not prove C initialization; unattempted or capped tails are not silently decoded.
5. **Resources/portability:** no sandbox, hard total-engine-RAM guarantee or full parser streaming/seek index. Linux x86-64/GCC/GDB-supported evidence is not a universal cross-platform promise. Four pre-V2.1 files with removed fields remain unsupported on import.

### Glossary

| Term | Meaning |
|---|---|
| Checkpoint | Retained stopped snapshot; not every program instruction or stop |
| Stop ID | Processed GDB stop counter, distinct from vector/attempt indices |
| Event position | Index into canonical retained event array; never invent missing entries |
| Gap | Interval where instruction capture lacks exact events; unknown count |
| Allocation lifetime | Trace-local observed identity with PID/sequence boundaries, not an address |
| Provenance | Evidence of where a pointer originated; containment alone is insufficient |
| `EvidenceRef` | Weak-trace-identity citation to a canonical record; not portable |
| Definite | Verified **specific** observation/relation under its documented scope |
| Candidate / Ambiguous | Plausible unproved relation / multiple compatible relations |
| Unavailable / Contradictory | Insufficient evidence / structurally rejected relationship |
| Partial coverage | Some observations missing; does **not** mean record corruption |
| Independent diagnostic | Valgrind or sanitizer output from another execution |

> WARNING: The approved Stage 1 freeze authorizes a **qualified input** for future semantic work, not permission to collapse these terms into a single “trace complete” flag. See the [final integrated audit](../stage1-final-audit.md) for verification scope.
<!-- END CHAPTER: content/handoff.md -->

<!-- BEGIN CHAPTER: content/glossary.md -->
# 66 · Terminology and glossary

> **In one sentence:** Use this as a quick translation when you meet a technical word elsewhere. Return to its linked chapter for the full example.

## Key terms

| Term | Plain meaning |
|---|---|
| **Evidence** | Something actually recorded or statically described, with a source and limits. |
| **Inference** | A conclusion drawn *from* evidence. It may be useful, but it should never be relabeled as a directly recorded fact. |

> **Example.** GDB recorded `raw_bytes_hex=0a000000`. 'Four bytes were read' describes evidence. 'The C initializer ran' would be an inference that those bytes alone do not support.
>
> **What not to assume:** A word such as 'complete' always needs a scope: complete for which processes, time interval and kind of record?

## The big picture

The same numeric value can identify different things at different boundaries. Start with a type's owner and scope before joining two records.

## Recording terms

| Term | Meaning |
|---|---|
| Checkpoint | Retained source-level snapshot at one observed stop, not every executed instruction. |
| Stop ID | Debugger stop counter, not a snapshot vector position. |
| Attempted instruction | An attempt to record/advance at a PC; retention and gaps have separate accounting. |
| Retained event | Actual indexed record in the instruction-event array. |
| Gap | Explicit interval of execution not recorded as exact instructions. |
| Allocation lifetime | Wrapper-derived, scoped identity; an address may be reused by another lifetime. |

### Evidence terms

| Term | Meaning |
|---|---|
| Structural finding | Imported trace contradicts a supported invariant; not simply an incomplete run. |
| Partial | Some observation scope is absent, capped or interrupted. |
| Unavailable | Evidence does not establish the requested relationship. |
| Candidate | Compatible evidence without an exact proof under the documented scope. |
| `EvidenceRef` | Citation bound to the same canonical trace owner and original record position. |
| Source-site candidate | Shared source file/line from an independent process; no primary allocation ID. |

## Continue reading

See [identity domains](identity-contracts.html), [schema](schema.html) and [public API](api.html) for concrete fields and confidence qualifiers. DOC-2/3 will expand individual terms next to the code paths where they are produced.
<!-- END CHAPTER: content/glossary.md -->

<!-- BEGIN CHAPTER: content/limitations.md -->
# 67 · Known limitations

> **In one sentence:** This page is part of the explanation, not fine print. A good UI shows what a trace cannot say alongside what it can.

## Key terms

| Term | Plain meaning |
|---|---|
| **Partial trace** | A recording in which some intended observations were not retained, perhaps after a crash, timeout or budget limit. |
| **Unknown** | The system lacks sufficient evidence. It is not equivalent to false, zero, freed or no bug. |

> **Example.** A silent child might create allocations without producing wrapper log lines. A complete log for the parent does not make that child's history appear.
>
> **What not to assume:** Missing observations, independent diagnostic runs and RAM limits remain real even when the JSON file loaded successfully.

## The big picture

Stage 1 is an evidence recorder and immutable reader, not a complete semantics oracle or a security sandbox. These limits apply even when a trace loads and deep validation reports no structural corruption.

## Observation limits

GDB follows one inferior and its chosen execution scope; unstepped libraries, threads and fork descendants may be missing. Heap wrappers intercept the linked program's allocator calls, not all allocations inside shared libraries. `heap_history.complete` covers verified observed event PIDs and the followed inferior, not silent descendants. Memory reads can be partial or unavailable; complete bytes do **not** prove initialization, union activity or a pointer's origin.

### Independent providers

Clang AST syntax does not show that an expression ran. DWARF declarations and source lines can be ambiguous. Valgrind and sanitizer passes execute separately and cannot supply exact primary-run allocation IDs by matching an address, source line or function. Reallocation confidence requires recorded event/lifetime corroboration, not mere agreement between addresses.

## Resource and compatibility limits

Resource budgets are not hostile-program isolation. The 128 MiB default JSON export budget limits a serialized document, not already assembled trace RAM. Parser JSON input is not streaming; its input cap and estimated index bytes do not bound process RSS. Journals contain ordered records but not enough final-state fields for full trace reconstruction. The four historical pre-V2.1 `examples/v2_*.json` traces with removed limit fields remain unsupported imports; the six named V2.1 traces are the compatible corpus.

### Inspection caveats

The historical TP-8 conditional audit predates the TP8-F01 repair and the final integrated Stage 1 audit. A source review found that `json::detect_format` treats a malformed final journal-shaped line as a journal candidate even when newline-terminated, while typed `journal::parse_reader` still rejects that complete malformed record. Separately, `journal::parse_reader` reapplies `Take(max_bytes)` per line, so its arbitrary-reader route does not enforce an aggregate stream-byte cap. These are source-inspection observations, **not** DOC-1 production fixes or claims of an executed exploit; see `src/trace_parser/json.rs:69` and `src/trace_parser/journal.rs:167`. DOC-2/3 should reproduce and characterize them before changing contractual claims.
<!-- END CHAPTER: content/limitations.md -->


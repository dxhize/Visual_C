# Visual C · Stage 1 Documentation — Part 1

**Covers chapters 1–8:** Getting Started (1–3) and Architecture (4–8).

Every chapter below keeps its original scope, links, file paths, line numbers and code excerpts. The inner layout is reorganized so each chapter reads in the same order: **what it is → key terms → how it works → what not to assume → where to go next**.

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

Detailed statement-by-statement walkthroughs, branch analyses and error paths are planned: **DOC-2** for the engine, **DOC-3** for the parser.

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

Start at `src/trace_parser/mod.rs`, then `json.rs`, `indexes.rs`, `views.rs`, `validation.rs`. The [function index](functions.html) also lists private parsers and helpers. DOC-3 will expand their control-flow explanations.
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

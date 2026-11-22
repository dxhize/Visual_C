# Visual C · Stage 1 Documentation — Part 2

**Covers chapters 9–13:** Developer Internals (repository map, source file index, function index, module dependency explorer, call-flow explorer).

These five chapters are *navigation* tools. They help you find code and see how it relates, but none of them describes what happened in a particular recorded run. Each chapter keeps the same layout as Part 1: **summary → key terms → how it works → what not to assume → next steps**.

> **A rule that applies to all five chapters:** a listing, a module arrow or a call-graph edge tells you about the *source code*. Only a saved trace tells you about an *execution*.

---

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
| Will short summaries be expanded? | Yes: DOC-2 (engine) and DOC-3 (parser) will replace them with implementation walkthroughs, without creating a second website. |
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
| A scaffold for DOC-2/DOC-3 | Proof of a runtime call |

Why it can't prove calls: source parsing alone can't see dispatch through **traits** (Rust's interfaces), **callbacks**, or another process.

## Browsing

Use the local search with a qualified function name, type or source path. Expand a symbol to see its signature and source reference.

- For selected **verified** callers and callees, open the [call-flow explorer](call-flow.html).
- Functions not yet connected in the curated graph are still listed here, ready for DOC-2/DOC-3 to expand.

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
- This first graph is a **guided subset**, not a complete dynamic call graph. DOC-2/DOC-3 may add verified edges without changing the frontend.
<!-- END CHAPTER: content/call-flow.md -->

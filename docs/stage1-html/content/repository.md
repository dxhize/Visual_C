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

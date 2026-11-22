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

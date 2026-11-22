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

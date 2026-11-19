# Architecture · one recording, many qualified readers

## Level 1 · Intuition

The system has two non-interchangeable jobs. The **Trace Engine** gathers bounded observations while a compiled C program runs under GDB; the **Trace Parser** lets other code navigate that finished recording safely. A JSON trace is the contract between them. No parser query can revisit an inferior or recover an event that the engine did not capture.

{{diagram:architecture}}

## Level 2 · Pipeline and boundaries

1. A `TraceConfig` describes sources, program arguments, tools, limits and optional tracing modes. GCC builds a debug executable with an allocator wrapper and prepares a separately sanitized executable. Compilation failure has diagnostics but no fabricated stops.
2. The engine extracts static [DWARF](engine-metadata.html) from the GCC artifact and optionally runs a Clang AST pass. GDB/MI observes one followed inferior, source checkpoints and instruction attempts. The wrapper appends PID-local [heap events](engine-heap.html). Analyses have separately recorded outcomes.
3. One `DebugSessionTrace` preserves original observations, metadata, reports and [scope/gaps](engine-limits.html). The recorder can append JSONL records during execution, but a JSONL journal is **not** a complete JSON trace.
4. `TraceStore::open` parses the JSON, checks schema version and builds immutable [position indexes](parser-indexes.html). `validated_semantic()` performs **one opt-in deep validation pass**; TP-7 composes qualified borrowed views in explicit pages of at most 128.
5. A future Stage 2 consumer may compute explanations from these citations. Its interpretations must not be confused with primary observations.

{{diagram:compilation}}

### Distinct evidence providers

| Provider | What it knows | What it does not establish |
|---|---|---|
| GCC + DWARF | Compiled types, declarations, debug layout, diagnostics | Dynamic value or initializer execution |
| GDB/MI | Stopped frame, source line, values and bounded memory reads | All execution between stops or other processes |
| C allocation wrapper | PID/sequence-tagged intercepted allocator calls | Internal allocations in unwrapped libraries |
| Clang AST (optional) | Candidate syntax and source spans | That an expression executed |
| Valgrind / ASan / UBSan | Diagnostics from independent reproductions | Identity of a primary-run allocation |

## Level 3 · Trace through the implementation

`src/trace.rs::trace_program` calls `ExecutionLimits::validate`, enters a `supervisor::scope`, probes tools, invokes `compiler::compile_project`, extracts DWARF with `dwarf::extract_excluding`, and conditionally runs `clang_ast::analyze`. Its stopped loop cooperates with `src/gdb_mi.rs`, `src/recorder.rs` and `src/allocation_tracker.rs`. After reconstructing lifetimes and optional [evidence fusion](engine-metadata.html), `src/models.rs` defines the serializable result. `src/json_export.rs` writes an individually capped JSON document. `src/trace_parser/mod.rs::TraceStore::assemble` stores `Arc<DebugSessionTrace>` plus `Arc<TraceIndexes>` without copying source records.

```rust
// Core architecture signatures; source: src/trace.rs and src/trace_parser.
pub async fn trace_program(config: TraceConfig) -> Result<DebugSessionTrace>;
// TraceStore's public entry point (method declaration):
pub fn open(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<Self>;
// Constructing this view runs one opt-in validation pass:
pub fn validated_semantic(&self) -> ValidatedSemanticView<'_>;
```

> WARNING: The above is a **signature excerpt**, not a single compilable Rust module; `Result` and `Self` come from their respective modules. The runnable importable version is on [Practical usage](usage.html).

### A question to ask at every boundary

Is this datum an *observation*, a *static candidate*, a *recorded association*, a *validation finding* or a *future interpretation*? See [confidence](parser-semantic.html) for the rules that keep those categories separate.

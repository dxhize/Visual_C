# Visual C · Stage 1

**A field guide to C execution evidence.** Stage 1 accepts C source and a build configuration, compiles and observes a bounded primary execution, exports a schema-v2 trace, and provides immutable, validated, trace-scoped evidence to future consumers. This site documents the **implemented** Trace Engine and Trace Parser. Stage 2's C Semantic Engine is not implemented here.

{{diagram:architecture}}

## Choose a route

- **New to the system?** Start with [the architecture](architecture.html), then the [debugger lifecycle](engine-debugger.html), [data model](model.html), and the [API walkthrough](usage.html).
- **Adding an engine feature?** Read [compilation](engine-build.html), [variable capture](engine-memory.html), [heap telemetry](engine-heap.html), and [budgets](engine-limits.html) before editing Stage 1.
- **Consuming evidence?** Read [parser ingestion](parser-ingestion.html), [validation](parser-validation.html), [semantic confidence](parser-semantic.html), [bounded pages](parser-consumer.html), and the [Stage 2 handoff](handoff.html).
- **Looking up a type?** Use the [public API](api.html), [schema/identity tables](schema.html), right-hand page contents, or offline search (`/`).

## Level 1 · The central idea

A debugger can say “at this stop, these bytes were readable.” It cannot, by that observation alone, say “this pointer definitely came from allocation 7,” “these bytes were initialized,” or “every child process was observed.” Visual C records both the useful observation and its uncertainty. A consumer should keep [addresses separate from identities](schema.html#identity-domains), and distinguish [a structurally sound trace from a complete execution](parser-validation.html#level-2-architecture).

## Level 2 · What ships

The [Trace Engine](engine-build.html) uses GCC, GDB/MI2, DWARF and optional Clang syntax; a linked C allocator wrapper records its own events. Valgrind and sanitizers are **separate executions**. One [DebugSessionTrace](model.html) contains checkpoints, instruction attempts, gaps, heap lifetimes, stage outcomes and reports. The [Trace Parser](parser-ingestion.html) loads the JSON once, builds position-only indexes, and exposes borrowed, validated, explicitly paginated queries. This boundary is deliberate: Stage 1 captures evidence; Stage 2 may later interpret C-language meaning without changing recorded history.

## Level 3 · Begin in code

- Recording entry point: `src/trace.rs::trace_program(TraceConfig)`; schema: `src/models.rs::DebugSessionTrace`.
- Parser entry point: `src/trace_parser/mod.rs::TraceStore::open`; validation: `TraceStore::validated_semantic()` in `semantic.rs`.
- Stable consumer example: `examples/stage1_public_consumer_audit.rs`; runnable introduction in [Practical usage](usage.html).

> NOTE: The Stage 1 freeze is approved **within a bounded evidence scope**, not as a sandbox, a whole-process-tree tracer, a memory-initialization oracle, or a guarantee of assembled-trace RAM. Read the [precise limits](handoff.html#known-limitations).

## How to use this site offline

Open `index.html` directly in a browser. No network is required. Search, diagrams, themes, code styles, navigation and content are local. The [README](README.md) explains regeneration; every generated page is checked in alongside its Markdown source.

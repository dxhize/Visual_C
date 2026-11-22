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

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

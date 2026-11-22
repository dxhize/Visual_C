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

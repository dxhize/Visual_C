# 59 · Trace Parser function reference

> **In one sentence:** This catalog helps you find reviewed parser functions and the source lines that implement them.

## Key terms

| Term | Plain meaning |
|---|---|
| **Public query** | A function other code may call to read existing trace evidence. |
| **Reviewed relationship** | A call or data link checked against source and labeled by what it actually means. |

> **Example.** Find `checkpoint_summary`, then follow its source link. Its page queries keep the original evidence references and enforce the 128-record page limit.
>
> **What not to assume:** A function list is not a run log. Look at the trace to see which observations actually exist.

## The big picture

This page lists parser functions with a reviewed entry in the DOC-3 call-flow graph. Each entry shows the source location, actual signature, purpose, inputs/outputs, qualification and classified incoming/outgoing relationships. It is a navigation aid, not a claim that every parser helper has an independent walkthrough.

## Scope

A reviewed entry is not a runtime trace claim. Direct edges were checked at call expressions; borrowed-data, index/reference and validation dependencies retain their own labels. Functions absent here can be found in the complete [function index](functions.html) or the [source handbook](parser-handbook.html). DOC-3 prioritizes ingestion, ownership, indexing, validation, TP-6 and TP-7; less central helpers remain source-located for later expansion.

### Review status

The inventory marks graph-matched symbols as `explained_and_graph_linked`. Other parser symbols remain `file_chapter_indexed_only`. This distinction prevents a large but unsupported function encyclopedia.

### Parser graph entries

{{inventory:parser-functions}}

## Related reading

See [algorithms and invariants](parser-algorithms.html), the eight [walkthroughs](parser-walkthroughs.html), and the interactive [call-flow explorer](call-flow.html). Engine counterparts remain in the [DOC-2 function reference](engine-functions.html).

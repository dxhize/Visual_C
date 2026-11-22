# 45 · TP-6 · Validated semantic evidence

> **In one sentence:** A confidence label always describes a *particular claim*. 'I read these bytes' and 'these bytes were initialized' are not the same claim.

## Key terms

| Term | Plain meaning |
|---|---|
| **Definite** | The named observation or relationship is supported under its stated limits; it does not mean the whole execution was complete. |
| **Candidate** | Evidence fits one explanation, but does not prove it. An address inside a heap block is a candidate for a match, not pointer origin. |

> **Example.** GDB might definitely have captured four bytes of a pointer. The pointer's allocation origin can still be unavailable, especially if its address was reused.
>
> **What not to assume:** Never promote an AST source call to an executed call, or Valgrind's separate-run address to a primary-run allocation ID.

## Level 1 · Intuition

“Definite” must answer **which relationship, in what scope**. A readable byte can be a definite *capture* without a definite C initialization. A pointer can definitely contain an address without a definite allocation origin. TP-6 packages borrowed evidence with per-relationship confidence; it is **not** the future C Semantic Engine.

{{diagram:confidence}}

## Level 2 · The confidence model

Call `TraceStore::validated_semantic()` once before asking for evidence-level answers. It borrows the unchanged trace and holds the validation report. Each answer has a confidence label. **Definite** means *this specific claim* has the required support in its stated scope; **Candidate** means plausible but unproved; **Ambiguous** means more than one reading fits; **Unavailable** means the needed evidence is missing; and **Contradictory** means recorded facts conflict. A structurally unsound import prevents the semantic layer from calling its relationships definite, but you can still inspect the raw fields to investigate. These labels do not say whether the C program finished, a variable was in scope, its bytes were initialized, or every child was observed: those are different questions.

### Pointer and memory confidence

Ask `variable(&EvidenceRef)` for the **exact** local, global or caller variable you want. It points to the saved `VariableInfo` and any available type information. A pointer's `address_candidate` means “the number in this pointer could fit in this block's address range.” It does **not** say the pointer came from that allocation; under the current V2.2 contract, `definitive_allocation` stays `None`. If an address was reused or its block already freed, the question can be ambiguous or unavailable. `variable_target` can find another **recorded variable identity at this same stop**; it will not search a later stop or a foreign trace, and duplicate identities stay ambiguous. `valid_range` is merely the number of bytes read at this historical stop. It does not prove those bytes were initialized or still have the same contents later.

### Identity and source relationships

`heap(id)` brings together the block, its recorded lifetime and events, plus any *claimed* older realloc ID. The ID's confidence can be definite only for a verified, complete history of the **processes actually observed**. A realloc relationship needs more: the old and new creation events, PID, order and lifetime boundaries must agree. If an older trace lacks scope details, even a plausible link stays a candidate. `source_expression(id)` points to an optional Clang description of code that was written, and may show possible heap-call matches. It keeps `execution_confidence` unavailable: the same source line appearing in an event does not establish which expression ran.

### Independent diagnostics

`diagnostics()` borrows compile, stage, crash, sanitizer and Valgrind records. `valgrind_error(index)` exposes source-site candidate confidence, but `primary_allocation_id` is **always `None`**, including for legacy imported raw IDs. `process_coverage().unobserved_descendants` stays unavailable on sound traces; `heap_history.complete` is not a descendant census. `execution()` reports attempted vs retained counts, gaps and engine-declared scope independently.

## Level 3 · The relevant implementation

- `src/trace_parser/semantic.rs::{ValidatedSemanticView, EvidenceConfidence, VariableEvidence, MemoryEvidenceView, PointerEvidence, HeapEvidence, SourceExpressionEvidence, ExecutionEvidence, ProcessCoverageEvidence}`.
- `TraceStore::validated_semantic` calls `self.validate_deep()` **once**. `SemanticInputView::validated()` offers the same path. `self.qualify` downgrades on structural unsoundness without mutating source records.
- `heap(id)` uses TP-2's per-allocation event positions and the shared `validation::realloc_link` classifier. `source_expression(id)` uses TP-2 metadata candidate/ambiguity positions; no global pointer graph or synthetic execution interpreter is stored.

```rust
let view = store.validated_semantic();
if !view.report().is_structurally_sound() {
    eprintln!("Structural contradiction; do not present definite links");
}
let evidence = view.heap(allocation_id);
let _ = (evidence.identity_confidence, evidence.realloc_confidence,
         evidence.old_id, evidence.events());
```

This excerpt assumes `store` and `allocation_id`; the full runnable [walkthrough](usage.html#level-3-load-and-validate) covers imports. `old_id` is **raw recorded data** and not itself a certified edge; inspect `realloc_confidence`. A heap-event interval's TP-7 `allocation(&view)` inherits this same confidence rather than silently upgrading a candidate.

> WARNING: “Definite” is scoped to the specifically named observation or relationship. It does not imply a complete primary execution, a complete process tree, initialized bytes or that independent diagnostics belong to the primary run.

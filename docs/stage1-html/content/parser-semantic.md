# TP-6 · Validated semantic evidence

## Level 1 · Intuition

“Definite” must answer **which relationship, in what scope**. A readable byte can be a definite *capture* without a definite C initialization. A pointer can definitely contain an address without a definite allocation origin. TP-6 packages borrowed evidence with per-relationship confidence; it is **not** the future C Semantic Engine.

{{diagram:confidence}}

## Level 2 · The confidence model

Call `TraceStore::validated_semantic()` once per analysis session; it holds one `ValidationReport` and borrows the immutable store. `EvidenceConfidence` has five states: `Definite` (verified observation/relationship in stated scope), `Candidate` (plausible but unproved), `Ambiguous` (multiple compatible readings), `Unavailable` (no sufficient evidence), and `Contradictory` (a rejected structural relationship). An unsound report conservatively makes semantic query confidence contradictory globally; raw evidence is still inspectable. This model is **separate** from `ExecutionStatus`, scope liveness, byte readability, initialization and process coverage.

### Pointer and memory confidence

`variable(&EvidenceRef)` borrows local/global/caller `VariableInfo`, optional static metadata, value confidence and `MemoryEvidenceView`. A `PointerEvidence.address_candidate` is a numeric containment candidate, **not** provenance; `definitive_allocation` is **always `None` under present V2.2 statuses**. `address_only`, known freed, reused/ambiguous and incomplete history remain candidate, ambiguous or unavailable as appropriate. `variable_target(&VariableEvidence)` resolves a *recorded variable identity* only within the same stop (and rejects foreign refs); duplicate identities remain ambiguous. Readable `valid_range` certifies captured bytes in one historical observation, never C initialization or a future heap block's contents.

### Identity and source relationships

`heap(id)` borrows block, lifetime, old ID, old lifetime and event iterator. Identity confidence can be definite only in a verified complete **observed-PID** history; corroborated realloc confidence additionally requires exact event/lifetime predecessor agreement. Incomplete/legacy scope downgrades a supported link to candidate. `source_expression(id)` borrows optional AST and recorded heap-call candidates; `execution_confidence` remains unavailable. A compatible source line is not a dynamic execution link.

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

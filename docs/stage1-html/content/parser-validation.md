# TP-5 · Deep structural validation

## Level 1 · Intuition

A JSON file can have the right *shape* and still contradict itself. TP-5's opt-in deep validation checks cross-record relationships. It distinguishes a corrupt link from a legitimate missing observation: a crashed or truncated program can yield a structurally sound but incomplete trace.

{{diagram:confidence}}

## Level 2 · Architecture

`validate_deep()` aggregates `ValidationFinding { severity, code, detail, location, occurrences }` in a `ValidationReport`. Its four severities are **StructuralCorruption**, **PartialCoverage**, **UnavailableEvidence**, **OptionalAbsence**. `is_structurally_sound()` means no structural finding—not complete instruction, process, byte or diagnostic coverage. `is_partial()` concerns only partial findings. TP-6 creates this report once; it downgrades all semantic confidence to `Contradictory` when the report is unsound, while retaining raw records for investigation.

The pass reuses `processor::validate()` and checks event/disassembly/checkpoint references, event/stop ordering, snapshot indices, source-checkpoint bounds, gaps and attempted counts, duplicate heap/lifetime/metadata identities, pointer status vs claimed heap IDs, heap PID/history annotations, static links and diagnostic/coverage coherence. `address_only` with a definite allocation ID is contradictory even if a block contains that address. A false `complete_all_processes` annotation is unsupported. Legacy *missing* provenance or process scope is not synthesized; it is unavailable/unknown.

### Realloc link verification

A block's `realloc_from_id` is not enough. `realloc_link` compares indexed creation events of **both** new and predecessor identities with lifetimes: `Reallocated` must name the exact old ID; PID, order, allocation/free sequence and direction must agree. Self-links, cycles and explicitly conflicting events are structural contradictions even if history is partial. Missing corroboration receives `UnavailableEvidence`; an observed link in incomplete scope cannot become definite. glibc's `realloc(ptr,0)` terminal-free same-ID/null-address/zero-size event ends a lifetime but is **not** new lifetime creation. No address equality decides a predecessor. [Heap lifetimes](engine-heap.html) explains the source events.

## Level 3 · Report and import example

```rust
let store = TraceStore::open(path, TraceLoadOptions::default())?;
let report = store.validate_deep(); // explicit; not called by TraceStore::open
for finding in report.findings() {
    eprintln!("{} at {}: {}", finding.code, finding.location, finding.detail);
}
if report.is_partial() { /* retain the usable prefix, display missing scope */ }
```

This is a **body excerpt**; use [the executable sample](usage.html#level-3-load-and-validate) for imports. `src/trace_parser/validation.rs::{validate_deep, validate_heap, validate_heap_process_coverage, validate_pointer_reference, realloc_link}` implements checks. The resulting `ValidationReport` borrows no input JSON and aggregates occurrences by code; `first_location_findings()` on a TP-7 checkpoint is **not** a completeness proof for that checkpoint. `TraceStore::from_trace` intentionally remains schema-only.

### Example findings

| Code | Class | Correct response |
|---|---|---|
| `REALLOC_LINK_CONTRADICTORY` | Structural | Preserve raw import for investigation; never claim definite predecessor |
| `POINTER_HEAP_PROVENANCE_CONTRADICTORY` | Structural | Do not promote an address-only candidate |
| `HEAP_PROCESS_COVERAGE_UNSUPPORTED` | Structural | Reject claimed all-descendant completeness |
| `HEAP_HISTORY_INCOMPLETE` | Partial | Preserve observed events; downgrade history-dependent confidence |
| `REALLOC_LINK_UNVERIFIED` | Unavailable | Do not infer missing event/lifetime data |
| `SANITIZER_REPORT_UNRECORDED` | Optional | Absent report is not proof of no bug |

> WARNING: Trusting successful deserialization—or a raw `heap_history.complete` field without checking its scope and report—is a false-certainty bug. The [F01 regression](../trace-parser-v2.2-repair-f01.md) permanently checks the repaired realloc relationship.

# TP-7 · Checkpoint composition and bounded pages

## Level 1 · Intuition

A user-facing query often wants several related things at one stop: variables, instruction events, heap events, gaps and optional diagnostics. TP-7 composes existing TP-6 evidence around a checkpoint without copying a trace or inventing a second evidence-ID system. Every page is **explicitly** limited to 128 records: request the next range to continue.

{{diagram:consumer}}

## Level 2 · What a summary means

`checkpoint_summary(index)` borrows one `CheckpointEvidence`, its trace-scoped snapshot `EvidenceRef`, and the validated view. `snapshot_index`, `stop_id`, `attempted_instruction_position`, recorded PID and source line are **distinct**, not convertible integers. `variables_page(Local | Global | Caller(position), range)` returns qualified TP-6 variable views. `caller_frame(position)` and `frame_activation(id)` use the recorded activation within that same stop. `instruction_events_page(range)` returns events explicitly tagged with this checkpoint ID; the tag is definite, **not exact same-stop causality**. `heap_interval_page(range)` groups events by adjacent retained snapshots' cumulative heap counts, with only `Candidate` interval timing. `gaps_in_attempted_window(range)` cites attempted-position windows, not causal attribution to one stop. `source_expressions_page(range)` scans optional AST by **exact recorded file/span** and returns static candidates, not execution proof.

Session-wide `coverage()`, `session_diagnostics()`, `session_report_refs()`, `valgrind_errors_page()` and `first_location_findings()` stay separate from event cause. A report recorded in a session need not have happened at this checkpoint; “first validation finding here” is not a certificate of local completeness. `heap_events_page(&allocation_ref, range)` rejects foreign trace references **before** using numeric IDs. `HeapIntervalEvidence::allocation(&validated)` inherits TP-6 realloc confidence.

## Level 3 · Real signatures and bounded traversal

```rust
// Excerpt for a previously constructed `view: ValidatedSemanticView`.
let stop = view.checkpoint_summary(0)?;
let len = stop.checkpoint.context.variables().len();
for start in (0..len).step_by(128) {
    let end = (start + 128).min(len);
    for item in stop.variables_page(VariableScope::Local, start..end)? {
        let variable = item?; // borrowed VariableInfo + confidence + EvidenceRef
        let _ = (variable.info.name.as_str(), variable.value_confidence);
    }
}
```

This excerpt assumes `VariableScope` is imported and a nonempty trace (use the [complete runnable example](usage.html#checkpoints-and-paging) for zero-snapshot handling). `MAX_CONSUMER_PAGE=128` in `src/trace_parser/consumer.rs` governs **each** half-open `Range<usize>`; empty pages are valid. Reversed or past-end ranges return `InvalidRange`; width 129 returns `LimitExceeded` **without dropping rows**. `src/trace_parser/indexes.rs` supplies event/allocation positions, `views.rs` supplies borrowed checkpoint context, and `semantic.rs` supplies unchanged TP-6 confidence.

### Cost and ordering

Stop/event/gap windows use verified order and binary search. Heap interval pagination uses canonical event slices and counters; AST source spans are scanned on demand within bounded registry size. Per-page iteration borrows existing records; it does not build an unbounded output array. The audit exercised **3,143 real retained instructions across 900 checkpoints**; separate permanent tests exercise 300 events and 1,024 variables across multiple ≤128 pages. The large real trace does **not** prove 128 events at a single real checkpoint. See [performance](handoff.html#performance-not-a-guarantee).

> NOTE: Keep the page's borrowed lifetime bounded by its `ValidatedSemanticView`; do not collect an unbounded `Vec` just because the API makes small pages cheap. When evidence is `Unavailable`, handle it explicitly rather than displaying an empty success state.

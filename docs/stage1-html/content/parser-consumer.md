# 46 · TP-7 · Checkpoint composition and bounded pages

> **In one sentence:** A checkpoint summary groups related questions about one recorded stop so callers do not have to hunt through unrelated arrays.

## Key terms

| Term | Plain meaning |
|---|---|
| **Consumer** | Code such as a UI or analysis example that asks the parser questions about a trace. |
| **Page** | An explicitly requested slice of results. This API returns at most 128 entries in one request; ask for the next slice if there are more. |

> **Example.** For a stop with 300 variable records, request positions 0–127, then 128–255, then 256–299. Returning just the first page without saying so would hide evidence.
>
> **What not to assume:** Grouping nearby events is a navigation convenience, not proof that a particular source expression caused one.

## Level 1 · Intuition

A user-facing query often wants several related things at one stop: variables, instruction events, heap events, gaps and optional diagnostics. TP-7 composes existing TP-6 evidence around a checkpoint without copying a trace or inventing a second evidence-ID system. Every page is **explicitly** limited to 128 records: request the next range to continue.

{{diagram:consumer}}

## Level 2 · What a summary means

Start with `checkpoint_summary(index)` when you know a saved snapshot's position. It keeps a citation to that snapshot and uses the same validated trace. The snapshot position, GDB stop number, instruction-attempt count, PID and source line are **different labels**; do not substitute one for another. `variables_page` reads a chosen page of current-frame, global or recorded caller variables with TP-6 confidence. `caller_frame` and `frame_activation` describe one actual saved stop, not a lifetime guessed from a function name. `instruction_events_page` finds events the recording tagged with this checkpoint; that tag helps navigation, but does not prove an expression on the line caused the event. `heap_interval_page` shows events between two **retained** snapshots, so the timing is a candidate interval, not an exact stop. Gaps describe attempted-position windows. Optional AST source expressions are only descriptions of written code.

Some information belongs to the **whole session**, not to one stop: overall coverage, compiler and diagnostic reports, Valgrind findings and validation notes. A report first displayed near checkpoint 4 did not necessarily happen *because of* checkpoint 4. `heap_events_page` first checks that your allocation citation belongs to this trace, then looks up its numeric ID. When you follow an interval's allocation link, it keeps TP-6's existing realloc confidence rather than quietly changing a candidate into a certainty.

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

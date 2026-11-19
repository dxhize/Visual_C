# Practical usage · from C file to borrowed evidence

## Level 1 · Intuition

Run the engine to obtain a JSON document, then parse **that document** and check its deep-validation report before asking what happened at a checkpoint. One parser store can serve many readers. Observations stay historical; a missing diagnosis or event is not a proven absence.

{{diagram:consumer}}

## Level 2 · Run a recording

From the repository root on the supported Linux toolchain:

```sh
cargo build --release
target/release/c-trace-engine fixtures/c/phase8b_defined.c \
  --trace-mode dual --max-steps 350 --timeout 50 \
  --clang-ast --sanitizer-trigger never \
  --journal /tmp/visual-c-session.jsonl \
  --no-pretty --output /tmp/visual-c-session.json
python3 -m json.tool /tmp/visual-c-session.json > /tmp/visual-c-readable.json
```

GCC/GDB must be installed; Clang is optional for this command's `--clang-ast` pass (without it, remove the flag). Valgrind may run as an independent stage if available; `--sanitizer-trigger never` disables the separate sanitizer run. Do not pass `--` before engine options; only **program arguments** follow `--`. The engine writes one full JSON document to `--output`; JSONL can be parsed for records but **not** loaded as a complete `TraceStore`. Journals are never overwritten: choose a fresh `--journal` path for each repeat run (or remove your previous audit-owned temporary file first). See [compilation](engine-build.html), [deadline policy](engine-limits.html) and the [input schema](parser-ingestion.html).

### A real captured pointer, carefully read

The audited `phase8b_defined.c` trace's frame-0 `heap` variable at snapshot 4 recorded `points_to="0x705010"` and the following *abridged, exact-field* pointer target:

```json
{"kind":"heap","allocation_id":null,"base_address":"0x705010","offset_bytes":0,"active":true,"within_extent":true,"variable_identity":null,"address_match_allocation_id":1,"provenance_status":"address_only"}
```

The raw event at sequence 1 created allocation ID 1 at `0x705010`. **Matching the numeric address did not certify the pointer's provenance**: `allocation_id` stayed null and the evidence layer's `definitive_allocation` remains `None`. Addresses/PIDs can change in new runs; do not hardcode these values. At the same run's `checkpoint` line 15, snapshot 36 / GDB stop 119 / attempted instruction position 116 are **different identifiers**; `sum` displayed `"10"` with bytes `"0a000000"`.

## Level 3 · Load and validate

The following is a **complete** Rust example body for `examples/stage1_docs_consumer.rs`; it uses only public APIs. Save it there and run `cargo run --example stage1_docs_consumer -- /tmp/visual-c-session.json` (or use the checked-in larger audit consumer `cargo run --release --example stage1_public_consumer_audit -- /tmp/visual-c-session.json`).

```rust
use c_trace_engine::trace_parser::{
    EvidenceLocation, TraceLoadOptions, TraceStore, VariableScope, MAX_CONSUMER_PAGE,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("provide trace JSON path")?;
    let store = TraceStore::open(&path, TraceLoadOptions::default())?;
    let view = store.validated_semantic(); // one deep validation pass
    for finding in view.report().findings() {
        eprintln!("{}: {} ({})", finding.severity.label(), finding.code, finding.location);
    }
    if !view.report().is_structurally_sound() {
        return Err("contradictory imported relationships; inspect raw evidence".into());
    }
    println!("status={:?} partial={}", view.execution().status, view.report().is_partial());
    for index in 0..store.trace().snapshots.len() {
        let stop = view.checkpoint_summary(index)?;
        assert_eq!(stop.reference.location(), EvidenceLocation::Snapshot(index));
        let locals = stop.checkpoint.context.variables().len();
        for start in (0..locals).step_by(MAX_CONSUMER_PAGE) {
            let end = (start + MAX_CONSUMER_PAGE).min(locals);
            for entry in stop.variables_page(VariableScope::Local, start..end)? {
                let var = entry?;
                println!("{}: {:?}", var.info.name, var.value_confidence);
                if let Some(ptr) = &var.pointer {
                    println!("address candidate={:?}; provenance={:?}",
                        ptr.address_candidate, ptr.definitive_allocation);
                }
            }
        }
    }
    let process = view.process_coverage();
    println!("observed scope={:?}; descendants={:?}",
             process.scope, process.unobserved_descendants);
    Ok(())
}
```

### Checkpoints and paging

This example iterates all retained checkpoints and requests half-open pages of ≤128 locals. It never suppresses a page error or promotes address containment to provenance. Other page lengths come from the actual context (`globals()`, `caller_frames()`), event slice (`events_at_checkpoint()?`) or allocation-position index (`heap_event_positions_for_allocation(id)`); always request subsequent ranges explicitly. Use `view.heap_reference(&ref)` for trace-scoped heap evidence, `view.diagnostics()` for independent reports, and [`CheckpointSummary`](parser-consumer.html) for per-stop composition. The [API reference](api.html) details the signatures.

### Troubleshooting

| Symptom | What to check |
|---|---|
| `CompileFailed`, no snapshots | `compile_errors`, `stage_results.compiler` and tool versions; a syntax error is not an empty successful run |
| `ForeignTrace` | Do not pass `EvidenceRef` from a separately loaded JSON, even if bytes are identical |
| `Unavailable` AST/bytes | Was `--clang-ast` enabled? Was memory readable and within its budget? Unknown is not zero |
| `StepLimitReached` / missing heap ACK | Consult coverage, gaps, heap history, stage results and partial findings |
| `InvalidRange` / `LimitExceeded` | Use `start..end` with `end≤len` and `end-start≤128`; do not truncate silently |
| `REALLOC_LINK_CONTRADICTORY` | Imported predecessor disagrees with event/lifetime; inspect [verification rules](parser-validation.html#realloc-link-verification) |

> NOTE: A structurally sound report can still be partial or missing optional evidence. Treat the report and each per-relationship confidence as **different questions**.

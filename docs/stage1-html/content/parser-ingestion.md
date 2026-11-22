# 41 · TP-1 · Loading traces without inventing data

> **In one sentence:** Opening a trace is like opening a notebook: check that it is the right kind of notebook before relying on what its pages claim.

## Key terms

| Term | Plain meaning |
|---|---|
| **Schema** | The agreed JSON field format. Stage 1 uses schema version 2 for supported V2.1 and V2.2 recordings. |
| **JSONL** | One JSON object per line. The runtime journal is not the final trace document and cannot reconstruct missing reports. |

> **Example.** `TraceStore::open` can read a well-formed `old_id=7`. That still does not prove allocation 7 is the right predecessor; only deeper validation can examine the supporting events.
>
> **What not to assume:** An unsupported older example or malformed input fails import. A supported but incomplete trace may load and remain incomplete.

## Level 1 · Intuition

The Trace Parser reads the engine's finished JSON exactly once and makes a single immutable `TraceStore` available to multiple readers. Successful parsing means **schema-compatible**, not “all records are internally consistent” and not “the program ran to completion.” Deep validation is an explicit separate step.

{{diagram:ownership}}

## Level 2 · Architecture and compatibility

`TraceStore::open(path, options)` first checks how many bytes it is allowed to read. It reads one byte past the configured maximum only to tell the difference between an exactly fitting file and an oversized one. Next it distinguishes a complete JSON trace from a JSONL journal. `from_reader` and `from_json_bytes` accept input supplied by Rust code instead of a file. `from_trace` accepts a trace that Rust has already decoded, but checks **only its schema version**, not the truth of its relationships. Default import limits are 512 MiB of input and 2,000,000 JSONL records; oversized input is rejected, not quietly cut off. After loading, `TraceStore` keeps a shared `Arc` for the one original trace and another for its position indexes. Cloning the store shares these rather than duplicating every snapshot.

The supported format uses `schema_version=2`. The six supported V2.1 examples still load, even when newer optional fields were never recorded. In this model, a missing schema marker is interpreted as version 2. That does **not** mean every older file works: four pre-V2.1 `examples/v2_*.json` files use a removed limit field and cannot be decoded. Wrong schema, bad text encoding, broken JSON and oversized input each produce a specific error. By contrast, a C program crashing, timing out or reaching its step limit can produce a **valid but partial trace**; those outcomes are not reasons to pretend the JSON is malformed.

### JSONL is not a TraceStore

The `parse_journal` family can read each JSONL line as a configuration, checkpoint, allocation, gap, instruction or other recorded item. It also says whether the last line was torn off and whether the journal was finalized. A partially written last line can be identified as partial; a completely written but malformed line is an error. **Why not just open the journal as a trace?** The final heap state, crash reports and other end-of-session results may never have been written to it. `TraceStore::open` therefore refuses to guess a complete trace from journal lines. See [storage and limits](engine-limits.html).

## Level 3 · Implementation and API

```rust
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = TraceStore::open("examples/v21_dual.json", TraceLoadOptions::default())?;
    assert_eq!(store.trace().schema_version, 2);
    let validated = store.validated_semantic(); // opt-in; not part of open()
    if !validated.report().is_structurally_sound() {
        eprintln!("import has contradictory record relationships");
    }
    Ok(())
}
```

The snippet is executable as a Rust example in this crate (adjust the path if run elsewhere); for a larger end-to-end query see [Practical usage](usage.html#level-3-load-and-validate). Actual `TraceStore` methods: `open`, `from_reader`, `from_json_bytes`, `from_trace`, `trace`, `shared`, `source`, `validate_deep`. `src/trace_parser/{mod,json,journal,error}.rs` implement ingestion; `TraceParseError` covers malformed input, unsupported schema, journal-only input and limits. `src/trace_parser/indexes.rs` eagerly builds positions after successful decoding. `open` preserves file path/byte length in a small `TraceSourceMetadata` descriptor.

> WARNING: Neither schema compatibility nor `heap_process_scope()` on raw input certifies a claimed relationship. For imported/untrusted data, inspect `validated.report()` **and** the confidence/coverage of each relationship before presenting it as fact.

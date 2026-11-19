# TP-1 · Loading traces without inventing data

## Level 1 · Intuition

The Trace Parser reads the engine's finished JSON exactly once and makes a single immutable `TraceStore` available to multiple readers. Successful parsing means **schema-compatible**, not “all records are internally consistent” and not “the program ran to completion.” Deep validation is an explicit separate step.

{{diagram:ownership}}

## Level 2 · Architecture and compatibility

`TraceStore::open(path, options)` checks file size and reads at most `max_bytes + 1`, then content-detects a JSON document or journal. `from_reader` and `from_json_bytes` support byte inputs; `from_trace` adopts an already decoded model and applies **only the schema-version gate**. `TraceLoadOptions` defaults to a 512 MiB input cap and 2,000,000 JSONL records, rejecting larger inputs rather than silently truncating them. The loader does not retain original JSON text. `TraceStore::assemble` builds one `Arc<DebugSessionTrace>` and one `Arc<TraceIndexes>`; cheap clones share both.

The wire schema version is `2`; the supported V2.1 examples (normal, loop, heap, crash, dual, bounded infinite) load, including optional missing metadata. A missing schema marker defaults to 2 per Serde model. The four older pre-V2.1 `examples/v2_*.json` files with removed `max_register_snapshot_bytes` **do not** deserialize; do not advertise support for all historical schema-2 payloads. Wrong schema, invalid UTF-8, malformed JSON and over-limit input produce typed errors. Crash/timeout/step-limit are *valid trace outcomes*, not parser failures.

### JSONL is not a TraceStore

`parse_journal`, `parse_journal_bytes` and `parse_journal_reader` decode ordered typed records (`configuration`, `checkpoint`, `allocation`, `gap`, `disassembly`, `instruction`, `finalized`) with per-kind counts and torn-tail/finalization status. An incomplete final line can be recognized, but a complete malformed line fails. Final heap state, reports and other session-end fields are **not** in JSONL; `TraceStore::open` refuses to reconstruct a full trace from it. See [storage and caps](engine-limits.html).

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

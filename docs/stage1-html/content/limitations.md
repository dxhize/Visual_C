# 67 · Known limitations

> **In one sentence:** This page is part of the explanation, not fine print. A good UI shows what a trace cannot say alongside what it can.

## Key terms

| Term | Plain meaning |
|---|---|
| **Partial trace** | A recording in which some intended observations were not retained, perhaps after a crash, timeout or budget limit. |
| **Unknown** | The system lacks sufficient evidence. It is not equivalent to false, zero, freed or no bug. |

> **Example.** A silent child might create allocations without producing wrapper log lines. A complete log for the parent does not make that child's history appear.
>
> **What not to assume:** Missing observations, independent diagnostic runs and RAM limits remain real even when the JSON file loaded successfully.

## The big picture

Stage 1 is an evidence recorder and immutable reader, not a complete semantics oracle or a security sandbox. These limits apply even when a trace loads and deep validation reports no structural corruption.

## Observation limits

GDB follows one inferior and its chosen execution scope; unstepped libraries, threads and fork descendants may be missing. Heap wrappers intercept the linked program's allocator calls, not all allocations inside shared libraries. `heap_history.complete` covers verified observed event PIDs and the followed inferior, not silent descendants. Memory reads can be partial or unavailable; complete bytes do **not** prove initialization, union activity or a pointer's origin.

### Independent providers

Clang AST syntax does not show that an expression ran. DWARF declarations and source lines can be ambiguous. Valgrind and sanitizer passes execute separately and cannot supply exact primary-run allocation IDs by matching an address, source line or function. Reallocation confidence requires recorded event/lifetime corroboration, not mere agreement between addresses.

## Resource and compatibility limits

Resource budgets are not hostile-program isolation. The 128 MiB default JSON export budget limits a serialized document, not already assembled trace RAM. Parser JSON input is not streaming; its input cap and estimated index bytes do not bound process RSS. Journals contain ordered records but not enough final-state fields for full trace reconstruction. The four historical pre-V2.1 `examples/v2_*.json` traces with removed limit fields remain unsupported imports; the six named V2.1 traces are the compatible corpus.

### Inspection caveats

The historical TP-8 conditional audit predates the TP8-F01 repair and the final integrated Stage 1 audit. A source review found that `json::detect_format` treats a malformed final journal-shaped line as a journal candidate even when newline-terminated, while typed `journal::parse_reader` still rejects that complete malformed record. Separately, `journal::parse_reader` reapplies `Take(max_bytes)` per line, so its arbitrary-reader route does not enforce an aggregate stream-byte cap. These are source-inspection observations, **not** DOC-1 production fixes or claims of an executed exploit; see `src/trace_parser/json.rs:69` and `src/trace_parser/journal.rs:167`. DOC-2/3 should reproduce and characterize them before changing contractual claims.

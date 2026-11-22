# 58 · Trace Parser source file handbook

> **In one sentence:** This is a guide to where parser behavior lives in the repository, organized by the question you are trying to answer.

## Key terms

| Term | Plain meaning |
|---|---|
| **TP** | Trace Parser phase; the numbered phases introduced loading, indexes, validation, evidence views and bounded consumer queries. |
| **Source chapter** | A focused explanation of the implementation in one parser file. |

> **Example.** If `TraceStore::open` rejects a file, read the ingestion chapter. If it loads but a realloc link is rejected, read validation and the realloc repair chapter.
>
> **What not to assume:** Finding a type or helper in the handbook does not mean that a particular imported trace passed its checks.

## The big picture

This is the DOC-3 coverage index. Every Stage 1 parser production file has a detailed chapter or a dedicated section; the table records the ownership boundary and where to read the implementation. `src/semantic/` is Stage 2 and is deliberately excluded. `src/trace_parser/semantic.rs` is the Stage 1 TP-6 evidence layer, not the Stage 2 semantic engine.

## How to read the parser

Start with [ingestion](parser-ingestion-deep.html), then [ownership and evidence references](parser-ownership-deep.html), then [indexes](parser-indexes-deep.html). Read [deep validation](parser-validation-deep.html) before relying on [TP-6 confidence](parser-semantic-deep.html), and finish with [TP-7 consumer composition](parser-consumer-deep.html). The [algorithm reference](parser-algorithms.html) collects cross-file invariants; the [function reference](parser-functions.html) is generated from reviewed graph nodes.

### Production files and detailed treatment

| File | Responsibility | Detailed treatment |
|---|---|---|
| `src/trace_parser/mod.rs` | Loading, gates, schema dispatch, canonical store, raw queries, journal wrappers | [Ingestion](parser-ingestion-deep.html), [ownership](parser-ownership-deep.html), [indexes](parser-indexes-deep.html) |
| `src/trace_parser/json.rs` | JSON document decode, schema gate, content-based format detection | [Ingestion](parser-ingestion-deep.html) |
| `src/trace_parser/journal.rs` | Typed JSONL records, ordering, torn tail, record limits | [Ingestion](parser-ingestion-deep.html) |
| `src/trace_parser/error.rs` | `TraceParseError`, `QueryError`, ingestion/query boundary | [Ingestion](parser-ingestion-deep.html) |
| `src/trace_parser/indexes.rs` | Position-only execution/source/allocation/metadata indexes and ordering facts | [Indexes](parser-indexes-deep.html) |
| `src/trace_parser/evidence.rs` | `EvidenceLocation`, `EvidenceRef`, borrowed `Evidence` resolution | [Ownership and references](parser-ownership-deep.html) |
| `src/trace_parser/views.rs` | Raw/semantic input views, checkpoint contexts, process scope | [Ownership and references](parser-ownership-deep.html) |
| `src/trace_parser/validation.rs` | Deep validation passes, report model, pointer/process/realloc checks | [Validation](parser-validation-deep.html), [realloc](parser-realloc-deep.html), [coverage](parser-coverage-deep.html) |
| `src/trace_parser/semantic.rs` | TP-6 validated evidence, confidence, variable/heap/source/diagnostic views | [TP-6](parser-semantic-deep.html) |
| `src/trace_parser/consumer.rs` | TP-7 checkpoint summaries, bounded pages and evidence items | [TP-7](parser-consumer-deep.html) |

### Shared model boundary

The parser borrows the engine's `DebugSessionTrace` model from `src/models.rs`. DOC-2 covers how the engine constructs that model; DOC-3 only documents how the parser reads and validates it. The engine's JSON export limit is not a parser memory bound, and a schema-valid import is not a semantic validation result.

### Coverage status

The machine-readable [inventory](assets/data/stage1-inventory.json) marks these files as having a detailed chapter. Graph-linked functions carry reviewed purpose and classified relationships. Other indexed functions remain source-located but not independently walked through; DOC-3 does not turn a lexical definition into a behavioral proof.

### Excluded

`src/semantic/` (SE-1/SE-2), Stage 2 tests/reports, frontend implementation and Trace Engine internals are outside DOC-3. Cross-links to DOC-2 are intentional and do not duplicate engine documentation.

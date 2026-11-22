# DOC-3 completion report

## Inspected repository state

Base commit: `558037c0da180e152a3e6cc3f351edf0b0583215` (`main`). The worktree was already dirty before DOC-3 with the Stage 1 parser TP-6/TP-7/F01 implementation and unrelated Stage 2/example files. DOC-3 documents the **inspected working-tree parser**, not a commit hash alone. No production Rust or C file, dependency, schema-v2 definition or public parser API was changed. The DOC-1/DOC-2 site, generator, styles, search and graph IDs were extended in place.

## Deliverable counts

| Deliverable | Result | Qualification |
|---|---:|---|
| Stage 1 Trace Parser files covered | 10 | `mod.rs`, `json.rs`, `journal.rs`, `error.rs`, `indexes.rs`, `evidence.rs`, `views.rs`, `validation.rs`, `semantic.rs`, `consumer.rs`. |
| Parser symbols indexed | 388 | 301 functions plus 87 types/constants/other symbols in the parser page mapping. |
| Parser symbols with reviewed graph entries | 73 | Unique source symbols marked `explained_and_graph_linked`; the graph has 83 parser nodes because a few query rows intentionally share a canonical symbol. |
| Parser symbols source-located but not deeply walked | 315 | Marked `file_chapter_indexed_only`; DOC-3 does not claim a behavioral walkthrough for every private helper. |
| Parser call-flow nodes | 83 | Real parser functions and reference/index boundaries; no Stage 2 node. |
| Parser call-flow edges | 76 | 57 direct, 1 callback, 10 data/reference and 8 conceptual. These are not 76 runtime calls. |
| Whole-site call-flow nodes | 171 | DOC-1/DOC-2 engine nodes remain in the same explorer. |
| Whole-site call-flow edges | 114 | Parser edges are included in the total. |
| New HTML pages | 13 | 65 total pages, up from DOC-2's 52. |
| New SVG diagrams | 36 | 90 total diagrams, up from DOC-2's 54. Editable source is `doc3-diagrams.json`. |
| Parser walkthroughs | 8 | A load, B instruction, C variable, D corruption, E realloc, F summary, G Valgrind, H partial execution. |

## What was added

- A **Trace Parser Internals** navigation group: ingestion, ownership/views/evidence references, indexes, deep validation, realloc/TP8-F01, pointer/process coverage, TP-6, TP-7, data structures, algorithms, walkthroughs, source handbook and function reference.
- Detailed source chapters for every parser production file, with exact signatures, line references, call paths, ownership/lifetimes, errors, confidence and tests.
- Thirty-six parser diagrams covering the ingestion pipeline, schema gate, journal lifecycle, ownership, evidence references, index construction/lookup, identity domains, validation flow, finding classes, pointer/process decisions, realloc chains and confidence, TP-6/TP-7 composition, pagination, temporal boundaries and all eight walkthroughs.
- Inventory fields for parser `docPage` and `documentationStatus`; parser graph nodes link to the new chapters. The machine-readable [inventory](assets/data/stage1-inventory.json) marks all ten parser files as having detailed chapters.
- A parser graph expansion from DOC-2's parser subset to 83 nodes and 76 classified parser relationships, with source evidence for each edge. Direct, callback, data/reference and conceptual edges are distinguished; no Rust-to-C direct call was invented.

## Fresh parser execution smoke

DOC-3 ran one fresh parser benchmark against checked-in V2.1 data:

```sh
cargo run --quiet --release --example trace_parser_tp3_bench -- examples/v21_heap.json
```

Observed output: **202,719 JSON bytes**, **93 instruction events**, **20 snapshots**, **2 heap events**, parse **2.012 ms**, TP-2 index build **129.819 µs**, and TP-7 summary construction with zero requested live bytes. The run also reported `TP7_LARGE_PAGE vars=1024 max_page=128` and `tp7 oversized page rejected`, exercising the bounded-page contract. It is one host-specific parser smoke, not a rerun of the full Stage 1 suite or a universal performance claim.

## Source and relationship verification

- `python3 docs/stage1-html/inventory.py --check` reproduces the catalog from the current source tree.
- `node docs/stage1-html/export-call-flow.mjs` exports the reviewed graph.
- `python3 docs/stage1-html/build.py` generates **65 pages and 90 SVGs**.
- `python3 docs/stage1-html/check.py` validates **8,175 local references**, page/anchor integrity, SVG metadata, search coverage, inventory path coverage and every call-flow node/edge source path and line. Final result: **all documentation checks passed**.
- Headless Google Chrome opened **all 65 pages** and their diagrams over both `file://` and a temporary local HTTP server. The smoke exercised sidebar, breadcrumbs, outline, theme, offline search, graph filtering/selection/path, local source links and mobile navigation. This is an interaction smoke, not a manual visual or assistive-technology audit.
- Graph edges were checked against real call expressions, index/reference lookups and process boundaries. Parser source locations were checked after correcting several initial line/signatures; the final checker found no missing endpoints or out-of-range evidence lines.

## Accuracy boundaries and gaps

- DOC-3 explains the parser's major files, ownership model, indexes, validation passes, confidence rules and consumer composition. It does **not** deeply walk every one of the 388 indexed parser symbols; 315 remain source-located for later expansion. A file chapter is not proof that every helper was reviewed.
- Static source claims were checked against the current source, but no exhaustive call graph or formal proof was produced. Uncertain dispatch remains unresolved rather than being promoted to a direct edge.
- The fresh parser smoke used one checked-in example and one host. It does not validate every trace shape, compiler/GDB environment or benchmark corpus.
- Browser checks are headless interaction smoke tests. Manual visual, keyboard-only and screen-reader review remains DOC-4 work.
- Stage 2 `src/semantic/` and Stage 2 tests/reports remain excluded. `src/trace_parser/semantic.rs` is only the Stage 1 TP-6 boundary documented here.
- The parser's input byte cap, estimated index bytes and validation report are not a process RSS guarantee. The engine's JSON export cap is not a parser memory cap. A schema-valid import is not deep validation.

## Exact DOC-4 handoff

1. Continue on **this same site**. Add an `Execution Walkthroughs` group after the parser chapters; do not create a second site or rename existing graph IDs.
2. Use fresh, real C fixtures and captured traces for end-to-end stories: compilation, debugger stop/checkpoint, allocation/realloc, DWARF/AST metadata, Valgrind/sanitizer diagnostics, crash/timeout and a parser consumer query. Label each result as a fresh run or historical audit; do not present illustrative sequences as captured output.
3. Cross-link each story to the existing DOC-2 engine chapters and DOC-3 parser chapters. Keep one shared glossary, source-reference policy, theme, search and call-flow explorer.
4. Add any newly verified execution edges to `assets/call-flow.js` with stable IDs and source evidence. Add diagrams to `doc3-diagrams.json` or a new `doc4-diagrams.json` and register them in `build.py`.
5. Update the inventory generator's page/status mapping for any new files, regenerate with `export-call-flow.mjs`, `inventory.py`, `build.py`, `check.py` and `browser-check.mjs`, then write `DOC4-REPORT.md` with measured counts and source/browser verification.

Regeneration and checks:

```sh
node docs/stage1-html/export-call-flow.mjs
python3 docs/stage1-html/inventory.py
python3 docs/stage1-html/inventory.py --check
python3 docs/stage1-html/build.py
python3 docs/stage1-html/check.py
node docs/stage1-html/browser-check.mjs
node docs/stage1-html/browser-check.mjs http://127.0.0.1:8000/index.html
```

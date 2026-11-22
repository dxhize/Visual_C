# DOC-4 completion report and release decision

## Inspected repository state

Base commit: `558037c0da180e152a3e6cc3f351edf0b0583215` (`main`). The worktree was already dirty before DOC-4 with Stage 1 parser TP-6/TP-7/F01 work, example/test changes and unrelated Stage 2 files. DOC-4 documents the **inspected working-tree Stage 1 implementation**, not a commit hash alone. No production Rust or C file, dependency, schema-v2 definition or public API was changed. All DOC-4 output is documentation, diagrams, graph data, verification tooling or reports.

## Deliverable counts

| Deliverable | Result | Qualification |
|---|---:|---|
| Stage 1 production source files | 38 | 37 Rust files plus `c_runtime/alloc_tracker.c`; `Cargo.toml` is a separate inventory entry. |
| Files with substantive documentation | 38 | Every Stage 1 production file has a detailed chapter or handbook section; `src/semantic/` is excluded. |
| Indexed symbols | 1,179 | 854 functions and 325 types/constants/other symbols across Stage 1. |
| Functions with deep implementation explanations | 156 | Functions with reviewed purpose, inputs/outputs, limitations and classified graph edges. The remaining functions are source-located but not essay-length walkthroughs. |
| Type symbols indexed | 224 | Major parser/engine types have dedicated data-structure treatment; the rest are source-located inventory entries. |
| Algorithm sections | 18 | 10 engine and 8 parser cross-file algorithm/invariant sections, plus twelve end-to-end stories. |
| HTML pages | 67 | Two new pages: [walkthroughs](walkthroughs.html) and [Stage 1 public API](stage1-api.html). |
| Technical diagrams | 102 | Twelve new DOC-4 diagrams in `doc4-diagrams.json`; all have captions and alt text. |
| Call-flow nodes / edges | 171 / 114 | 86 direct, 2 callback, 2 external, 15 data/reference and 9 conceptual edges. These are not 114 runtime calls. |
| Guided explorer paths | 6 | CLI to export, GDB stop to checkpoint, allocator to heap lifetime, DWARF to type registry, JSON to summary, validation to confidence. |
| End-to-end execution stories | 12 | Normal execution, recursion, malloc, free/reuse/realloc, pointer provenance, DWARF, Clang/fusion, crash, loop/limits, fork, parser internals, corrupted evidence. |
| New diagrams | 12 | One per walkthrough, plus the existing architecture diagrams remain available. |
| Fresh parser smoke | 1 | DOC-3's `v21_heap` run is reused and identified; DOC-4 did not rerun the full Stage 1 corpus. |

## What DOC-4 added

- A consolidated **Execution Walkthroughs** chapter with twelve source-guided stories. Each has a scenario, route, actual function path, intermediate data, failure/uncertainty analysis, design rationale and source navigation. Historical audit counts are labeled separately from fresh DOC-2/DOC-3 smoke runs.
- A final **Stage 1 public API** reference with the real `TraceStore::open` → `validated_semantic` → `checkpoint_summary` sequence, ownership rules, page limits, evidence references, confidence handling and deliberate non-goals.
- Six **guided explorer paths** in `assets/guided-paths.js`, with previous/next controls, step notes, source locations and relationship labels. The explorer keeps free exploration, subsystem filtering, search, neighborhood expansion and directed-path highlighting.
- Twelve local SVG diagrams registered in `doc4-diagrams.json`; no CDN or remote graph service is used.
- A final coverage audit and release verdict based on the actual site, inventory and browser checks.

## Source-reference and coverage verification

- `python3 docs/stage1-html/inventory.py --check` reproduces the catalog from the current source tree.
- `python3 docs/stage1-html/build.py` generates **67 pages and 102 SVGs**.
- `python3 docs/stage1-html/check.py` validates **8,618 local references**, page/anchor integrity, SVG metadata, search coverage, inventory path coverage and every call-flow node/edge source path and line. Final result: **all documentation checks passed**.
- The graph contains no `src/semantic/` node. External GCC/GDB nodes remain labeled as process boundaries, not Rust functions.
- The twelve walkthroughs cite real fixtures and source locations. Counts from `docs/stage1-final-audit.md` are explicitly marked historical; DOC-2/DOC-3 fresh smoke counts are identified as such. Illustrative JSON/MI rows are marked illustrative.
- Production source status was checked before and after DOC-4: only the pre-existing dirty files remain. No DOC-4 production edit was made.

## Browser and offline testing

Headless Google Chrome opened **all 67 pages** and their diagrams over both `file://` and a temporary local HTTP server. The smoke exercised sidebar, breadcrumbs, outline, theme, offline search, graph selection/filter/path, guided-path selection and next-step navigation, local source links, mobile navigation and missing-asset detection. The guided-path assertions selected `gdb-checkpoint`, checked all eight steps, advanced to step 2 and verified the status text.

This is an interaction smoke, not a manual visual, keyboard-only or screen-reader audit. No external network request is required for core navigation, search, diagrams or the explorer.

## Accuracy decision

The final site preserves the approved Stage 1 evidence contract: pointer addresses are not provenance; AST candidates are not execution; Valgrind is independent; observed-PID completeness is not a descendant census; bytes are not initialization; realloc IDs require corroboration; schema load is not deep validation; export/input/index limits are distinct; checkpoint intervals are not exact causality. No diagram or simplified walkthrough promotes a candidate to a definite relationship.

One previously documented implementation caveat remains deliberately unfixed: the arbitrary-reader journal path applies `Take(max_bytes)` per line rather than as an aggregate stream cap (`src/trace_parser/journal.rs:167-175`). DOC-3 records it as a source-inspection finding, not a DOC-4 production change. No new accuracy defect was found in the DOC-4 documentation pass.

## Remaining documentation gaps

- **Deep function coverage:** 156 of 854 indexed functions have reviewed graph-backed explanations. The remaining 698 are source-located, with file chapters and search entries, but not independent statement-group walkthroughs. This is a deliberate scope boundary, not a claim that every private helper is deeply documented.
- **Exhaustive call graph:** the explorer is a curated, source-verified graph, not a proof of every possible indirect call or dynamic dispatch.
- **Manual accessibility/visual review:** browser smoke tests behavior and asset loading; manual screen-reader, keyboard-only and visual contrast review remains an enhancement.
- **Benchmark breadth:** DOC-4 relies on DOC-2/DOC-3 fresh smoke runs and the final audit corpus; it does not rerun the full Stage 1 suite.
- **Stage 2:** intentionally excluded. `src/semantic/` is not in the inventory or graph.

## Final release verdict

**A — Documentation Approved.**

The Stage 1 site now presents a coherent architecture, source-grounded engine and parser chapters, twelve end-to-end stories, six guided explorer paths, a public API reference, local diagrams and offline search. Important production files and mechanisms are explained; source references and graph edges are checked; navigation and browser smoke tests pass; production code remains unchanged. The remaining gaps are depth enhancements for non-central helpers and manual accessibility review, not blockers to using this as the authoritative Stage 1 developer reference.

### Release artifact

[`docs/stage1-html/index.html`](index.html)

### Reproduction

```sh
node docs/stage1-html/export-call-flow.mjs
python3 docs/stage1-html/inventory.py
python3 docs/stage1-html/build.py
python3 docs/stage1-html/check.py
node docs/stage1-html/browser-check.mjs
```

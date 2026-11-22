# DOC-2 completion report

## Inspected repository state

Base commit: `558037c0da180e152a3e6cc3f351edf0b0583215` (`main`). The worktree was already dirty before DOC-2 with the Stage 1 parser TP-6/TP-7/F01 work and unrelated Stage 2/example files. DOC-2 documents the **inspected working-tree implementation**, not a commit hash alone. No production Rust or C file, dependency or schema-v2 definition was changed. The existing DOC-1 site, generator, styles, search and call-flow IDs were extended in place.

## Deliverable counts

| Deliverable | Result | Qualification |
|---|---:|---|
| Stage 1 Trace Engine files covered | 28 | 27 Rust files plus `c_runtime/alloc_tracker.c`; every file has a detailed chapter section or a row in the source handbook. |
| Engine symbols indexed | 791 | 553 functions and 238 types/constants/other symbols in the DOC-2 page mapping. |
| Engine symbols with reviewed graph entries | 83 | Marked `explained_and_graph_linked` with purpose, input/output, limitations and classified edges. |
| Engine symbols source-located but not yet deeply walked | 708 | Marked `file_chapter_indexed_only`; DOC-2 does not claim a behavioral walkthrough for every private helper. |
| Engine call-flow nodes | 88 | Real engine functions plus the GCC/GDB external process nodes. |
| Call-flow edges | 53 | 36 direct, 2 callback, 2 external subprocess, 5 data handoff and 8 conceptual. These are not 53 runtime calls. |
| Whole-site call-flow nodes | 102 | The DOC-1 parser/consumer nodes remain in the same explorer; 14 are non-engine DOC-1 entries. |
| New HTML pages | 18 | 52 total pages, up from DOC-1's 34. |
| New SVG diagrams | 33 | 54 total diagrams, up from DOC-1's 21. Editable sources are `diagrams.json` and `doc2-diagrams.json`. |
| Algorithms page sections | 10 | Supervised subprocesses, MI dispatch, stop/checkpoint, capture, DWARF origins, AST candidates, heap prefixes/realloc, fusion audit and JSON preflight. |
| Subsystem walkthroughs | 6 | A compile, B checkpoint, C malloc, D realloc, E static metadata, F crash/limit; explicitly illustrative except the separately identified smoke run. |

## What was added

- **Trace Engine Internals** navigation group with entry/configuration, compiler, GDB controller, MI2 parser, recorder, stack/memory, DWARF, Clang AST, C allocator, heap reconstruction, Valgrind/sanitizer, fusion, supervision/limits and model/export chapters.
- A [source file handbook](engine-handbook.html) mapping every engine production file to its detailed chapter; an [algorithm/invariant page](engine-algorithms.html); a generated [engine function reference](engine-functions.html); and six [subsystem walkthroughs](engine-walkthroughs.html).
- DOC-2 architecture/control/data diagrams covering CLI, config, session, failure, compilation artifacts, MI token/async flow, stop loop, checkpoints, frames, memory, DWARF/CU/origin, Clang AST, wrapper/transport/fork, lifetimes/realloc, independent diagnostics, fusion, model/export, resource boundaries and all six walkthroughs.
- Inventory fields for `docPage` and `documentationStatus`. Graph-matched symbols now carry reviewed purpose and classified incoming/outgoing edges; the remaining symbols are explicitly marked as chapter-indexed, not deeply documented.
- Graph expansion from DOC-1's 47 nodes to 102 nodes, with engine nodes linked to the new deep chapters. Existing parser node IDs were preserved where possible.

## Real execution verification

DOC-2 ran one fresh smoke trace with the existing release binary, not a synthetic JSON fixture:

```sh
target/release/c-trace-engine fixtures/c/phase8b_defined.c   --trace-mode dual --max-steps 350 --no-pretty --timeout 50   --clang-ast --sanitizer-trigger never   --output /tmp/kilo/doc2-defined.json --journal /tmp/kilo/doc2-defined.jsonl
```

Observed result from `/tmp/kilo/doc2-defined.json`: `execution_status="completed"`, **45 snapshots**, **45 source checkpoints**, **157 retained instruction events**, **3 heap events**, heap history complete for the observed PID scope, a completed independent Valgrind stage and a skipped sanitizer pass (`sanitizer-trigger never`). The tool reported a sanitizer build degraded to AddressSanitizer and no delegated cgroup; those are retained stage notes, not silent successes. This is one host-specific smoke run, not a rerun of the full Stage 1 corpus or a guarantee about other fixtures.

The six walkthroughs are labeled **illustrative source-guided sequences** unless they explicitly cite this smoke run or the historical Stage 1 audit. The audit's earlier 383-test / 16-trace corpus is historical evidence, not a DOC-2 measurement.

## Source and relationship verification

- `python3 docs/stage1-html/inventory.py --check` reproduces the catalog from the current source tree.
- `node docs/stage1-html/export-call-flow.mjs` exports the reviewed graph.
- `python3 docs/stage1-html/build.py` generates **52 pages and 54 SVGs**.
- `python3 docs/stage1-html/check.py` validates **5,920 local references**, page/anchor integrity, SVG metadata, search coverage, inventory path coverage and every call-flow node/edge source path and line. Final result: **all documentation checks passed**.
- Headless Google Chrome opened **all 52 pages** and their diagrams over both `file://` and a temporary local HTTP server. The smoke exercised sidebar, breadcrumbs, outline, theme, offline search, graph filtering/selection/path, local source links and mobile navigation. This is an interaction smoke, not a manual visual or assistive-technology audit.
- The graph contains direct, callback, external, data and conceptual classifications with source evidence. Process boundaries are not drawn as Rust-to-C direct calls. DWARF/AST and Valgrind relationships remain candidates or independent runs.

## Accuracy boundaries and gaps

- DOC-2 explains the engine's major files, types, algorithms, state transitions and important functions. It does **not** deeply walk every one of the 791 indexed engine symbols; 708 remain intentionally source-located for DOC-3/later expansion. A file chapter is not proof that every helper was reviewed.
- Static source claims were checked against the current source, but no exhaustive call graph or formal proof was produced. Uncertain dispatch remains unresolved rather than being promoted to a direct edge.
- The real smoke run used one fixture and one host. It does not validate every compiler, GDB, Clang, Valgrind or cgroup environment.
- Browser checks are headless interaction smoke tests. Manual visual, keyboard-only and screen-reader review remains DOC-4 work.
- Stage 2 `src/semantic/` and Stage 2 tests/reports remain excluded. `src/trace_parser/semantic.rs` is only the Stage 1 TP-6 boundary already covered by DOC-1.

## Exact DOC-3 handoff

1. Continue on **this same site**. Add a `Trace Parser Internals` group after the engine chapters; do not create a second site or rename existing graph IDs.
2. Expand, in source order: `src/trace_parser/json.rs` and `journal.rs`; `mod.rs` load/gates/assembly; `indexes.rs`; `evidence.rs`; `views.rs`; `validation.rs` including TP8-F01; Stage 1 `trace_parser/semantic.rs`; and `consumer.rs` bounded pages. Keep Stage 2 `src/semantic/` excluded.
3. For each parser file, add a detailed section to a new `parser-*-deep.md` page or extend the existing parser overview pages, preserving stable links. Document `Arc<DebugSessionTrace>` ownership, position-only indexes, foreign-trace rejection, borrowed views, confidence propagation, 128-entry page bounds, and the distinction between structural soundness and partiality.
4. Add verified parser edges to `assets/call-flow.js` using existing node IDs where present; update `docPage` only when the target chapter is published. Add parser diagrams to `doc2-diagrams.json` or a new `doc3-diagrams.json` and register them in `build.py`.
5. Update the inventory generator's page/status mapping for parser files, regenerate with `export-call-flow.mjs`, `inventory.py`, `build.py`, `check.py` and `browser-check.mjs`, then write `DOC3-REPORT.md` with measured counts and source/browser verification.

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

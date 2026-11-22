# DOC-1 completion report

## Inspected revision and scope

Base commit: `558037c0da180e152a3e6cc3f351edf0b0583215` (`main`). The repository was **dirty before DOC-1**: Stage 1 parser TP-6/TP-7/F01 files and tests, among other Stage 2 and example changes, were present in the working tree. DOC-1 documents the **inspected working-tree state**, not a reproducible commit identified by that hash alone. No production Rust/C source or schema version was changed for DOC-1. The inventory excludes `src/semantic/` and Stage 2 implementation/tests/reports; `src/trace_parser/semantic.rs` is an included **Stage 1** TP-6 interface.

## Deliverable counts

| Deliverable | Result | Qualification |
|---|---:|---|
| Stage 1 production files | 38 | 37 Rust (`src/`) + 1 C (`c_runtime/alloc_tracker.c`); `Cargo.toml` is a separate 39th catalog entry. |
| Indexed functions | 854 | Production source definitions, including private methods and in-file unit-test definitions; lexical locations/signatures are **not** all source-audited walkthroughs. |
| Indexed type definitions | 224 | Struct, enum, union, trait, type alias and C typedef categories; remaining indexed symbols are modules/constants/statics. |
| Total indexed symbols | 1,179 | `assets/data/stage1-inventory.json`, with stable source-location IDs and per-file roles, selected test matches and static module references. |
| Static module references | 121 | Lexical references, **not verified call edges** or executed dependencies. |
| Curated graph nodes | 47 | 45 real production functions and two explicitly labeled GCC/GDB tool boundaries. |
| Source-referenced graph relationships | 53 | Classified direct, callback, external subprocess, data handoff or conceptual; the classification is as important as the arrow. |
| Website pages | 34 | One offline site; no Stage 2 internals page. |
| Generated SVG diagrams | 21 | Includes all ten requested system, engine, parser, dependency, runtime, handoff, ownership, identity, resource and consumer maps. |

The graph's 53 relationships and the inventory's 121 lexical module references are different datasets and must **not** be added together as a count of verified calls. Graph relationships contain source locations, while conceptual edges explicitly disclaim function invocation. The first DOC-1 graph is deliberately selective; its 45 functions do not imply that every one of the 854 indexed functions has been behaviorally audited.

## Accuracy and verification

- Compared inventory paths to actual `src/**/*.rs` excluding `src/semantic/**`, `c_runtime/*.c`, and `Cargo.toml`; 39 catalog paths match. Every graph node/edge reference was checked for an existing source line; graph-matched functions enrich the symbol catalog with purpose, input/output, limitations, incoming/outgoing classified edges. The remaining definitions retain `automatically_indexed_unverified` status.
- Read final Stage 1 integrated audit, V2.2 engine release closure, TP-8 audit and TP8-F01 repair, TP-5/6/7, base parser and V2 contracts; inspected engine, parser and C runtime source, particularly orchestration and ownership boundaries. Sampled direct edges at `src/main.rs:34`, `src/trace.rs:235`, `src/trace.rs:968`, `src/trace_parser/mod.rs:224`, `src/trace_parser/semantic.rs:41` and `src/trace_parser/consumer.rs:245`. Tool boundaries were checked at `src/compiler.rs:471` and `src/gdb_mi.rs:398`.
- Generated pages and SVGs with the existing stdlib-only builder. `check.py` checks page count, asset/link targets and anchors, SVG metadata and search-index coverage including each file/symbol; the final check result is recorded by running the commands below. `inventory.py --check` verifies the catalog is reproducible from the current source tree.
- Headless Google Chrome opened **all 34 pages** and their embedded SVG diagrams over both direct `file://` and a temporary local HTTP server. The browser smoke also exercised sidebar, breadcrumbs, outline, theme, offline symbol search, graph selection, subsystem filter, highlighted multi-hop path, local source link, mobile menu and missing-asset detection. This is a browser interaction smoke, **not** a manual visual, keyboard-only, screen-reader or exhaustive accessibility audit.
- No Stage 1 trace suite was rerun: DOC-1 changes only documentation. Prior Stage 1 audit pass counts are historical claims, not measurements repeated here.

## Evidence-contract checkpoints

Valgrind and sanitizer analyses are separate executions from the GDB-recorded run. `src/main.rs` exports the JSON *after* `trace_program` returns. Compiled DWARF and optional Clang syntax do not establish executed expressions; wrapper events and same-PID lifetimes, not address reuse, establish allocation references. Stop IDs, snapshot positions, attempted positions and retained event indexes are distinct. A clean schema load is not deep validation, and a complete observed-PID heap history is not a census of silent fork descendants. JSON export bytes, journal record bytes, parser input bytes and process RSS are different controls.

## Historical discrepancies and source-inspection caveats

- `docs/trace-parser-v2.2-final-audit.md` recorded a **conditional B** before F01; the later `docs/trace-parser-v2.2-repair-f01.md` records the targeted repair, followed by `docs/stage1-final-audit.md`'s final integrated approval. Do not quote the earlier verdict as current or imply it already covered the later audit.
- `docs/trace-parser.md` stops at TP-3, TP-5 says TP-6 was future and TP-7 says TP-8 was future. Those were phase-local statements, not a current file inventory. `src/trace.rs:3-14` uses legacy language about correlating Valgrind findings with allocation IDs; production `trace_program` does **not** call the legacy `valgrind::correlate_with_allocations` helper, and the release contract prohibits exact cross-run IDs.
- Potential implementation/documentation mismatch, **not repaired here**: `src/trace_parser/json.rs:69-105` may dispatch a malformed final newline-terminated journal-shaped line to journal parsing even though older prose describes only an incomplete torn line; `src/trace_parser/journal.rs:180-220` still rejects that malformed complete record. `src/trace_parser/journal.rs:167-175` recreates `Take(max_bytes)` per line in the arbitrary-reader route, which does not enforce an aggregate stream-byte cap like the path/byte-buffer routes. These observations need a focused test and contract decision before implementation changes.

## Known DOC-1 gaps

The inventory is complete **by source file** but the machine-extracted 854 function signatures are not all curated caller/callee/algorithm analyses. For 45 selected functions the graph supplies reviewed purpose, typed relationships and evidence; DOC-2/3 must extend this rather than treating an import or text match as a verified call. External crates and per-function side effects are not exhaustively inventoried; explicit test-file matches are discovery aids rather than coverage claims. In-file unit-test functions appear in the lexical count even though they are not production entry points. The existing Stage 1 chapters still contain some earlier level-labeled prose; new DOC-1 pages use subject-centered organization. Generated graph SVGs use readable explicit coordinates, not an automatic layout solver. Manual responsive/visual and assistive-technology review remains for DOC-4.

## Exact DOC-2 handoff

1. Continue in **this** `docs/stage1-html/` site: edit `content/*.md`, update `build.py` page manifest, use `diagrams.json` for diagrams; regenerate checked-in HTML/SVG/search artifacts. Read `README.md` first. Preserve link IDs and `assets/call-flow.js` node IDs when possible.
2. Start engine walkthroughs at `src/main.rs:23`, `src/cli.rs:197`, `src/trace.rs:132,167,1113`, then follow compilation (`compiler.rs`), supervision (`supervisor.rs`, `resource_limits.rs`), MI (`gdb_mi.rs`, `mi_parser.rs`), stopped capture (`recorder.rs`, `memory.rs`) and wrapper/heap (`c_runtime/alloc_tracker.c`, `allocation_tracker.rs`). Cover `dwarf.rs`, optional `clang_ast.rs`, `evidence/*`, independent `valgrind.rs`/`sanitizer.rs`, models and `json_export.rs`; distinguish compile-time metadata from runtime evidence and separate process runs.
3. For each significant function, replace lexical-only inventory status with checked invocation, inputs, preconditions, branches, state/ownership, outputs, failure behavior, downstream effects, rationale and source locations. Verify all graph edge classifications at call expressions, callback registrations or process interfaces. Record unresolved dispatch edges as indirect/unknown, not direct.
4. DOC-3 then expands `src/trace_parser/` on the **same site**: JSON/journal import, eager indexes, immutable `Arc` ownership, borrowed views and foreign-reference checks, deep validation including TP8-F01, Stage 1 `trace_parser/semantic.rs` confidence and TP-7 bounded consumer pages. Do not cross into Stage 2 `src/semantic/`. DOC-4 adds actual execution stories and exhaustive browser/a11y review.

Regeneration and checks:

```sh
node docs/stage1-html/export-call-flow.mjs
python3 docs/stage1-html/inventory.py
python3 docs/stage1-html/inventory.py --check
python3 docs/stage1-html/build.py
python3 docs/stage1-html/check.py
node docs/stage1-html/browser-check.mjs
```

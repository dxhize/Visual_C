# Visual C Stage 1 developer reference

Open [`index.html`](index.html) from a file browser or run `python3 -m http.server 8000 -d docs/stage1-html` at the repository root and visit `http://localhost:8000/`. HTML, SVGs, search index and call-flow data are checked in. There is no CDN, runtime network call or frontend build dependency. Search uses a generated `assets/search-data.js` global instead of fetching JSON, so it works when `file://` forbids `fetch`. A local browser may open a linked Rust file without jumping to its numbered line: use the adjacent copyable `path:line` instead.

## Edit and regenerate

The existing single-site, Python-standard-library generator remains the source for HTML. Edit `content/*.md` for chapters and the page manifest/templates in `build.py`. Chapters 1–25 were adapted from the user-provided `new_docs_md/stage1-docs-part1.md` through `stage1-docs-part5.md` (kept unchanged as review inputs); later chapters use the same one-sentence summary, key-term table, example and boundary before their source-level detail. `stage1-complete-text.md` collects the current chapter Markdown for review, but the individual `content/*.md` files are the site sources. The earlier `study-notes.json`/`apply-study-notes.py` are archival scaffolding; **do not rerun that script** against the restructured chapters. Edit `diagrams.json`, `doc2-diagrams.json`, `doc3-diagrams.json` and `doc4-diagrams.json` for SVGs. The new `layout: graph` format accepts stable node IDs, explicit `(x,y)` coordinates and typed arrows. Linear diagrams retain the older simple format. `assets/site.css`/`site.js` supply the shared layout, theme, menu and search. `assets/call-flow.js` contains curated real nodes and classified relationships; `assets/guided-paths.js` defines six guided execution paths; `assets/explorer.js`/`explorer.css` implement the interactive browser and tour controls. Do not edit generated `.html`, `diagrams/*.svg`, `assets/search-data.js` or `assets/search.json` directly.

From the repository root:

```sh
node docs/stage1-html/export-call-flow.mjs
python3 docs/stage1-html/inventory.py
python3 docs/stage1-html/build.py
python3 docs/stage1-html/check.py
node --check docs/stage1-html/assets/site.js
node --check docs/stage1-html/assets/explorer.js
node docs/stage1-html/browser-check.mjs
```

`export-call-flow.mjs` projects the curated browser graph into `assets/data/verified-relationships.json` (Node is needed only to regenerate this optional machine-readable companion). `inventory.py` scans the current production Rust/C source tree, excluding `src/semantic/` Stage 2, and writes `assets/data/stage1-inventory.json`. It indexes definitions and static module references with source locations, and enriches graph-matched functions with reviewed purposes and classified incoming/outgoing edges. A lexical reference is **not** a verified call or a full implementation explanation. Review output after source edits rather than assuming a regex proves dispatch. `check.py` validates local links, anchors, generated SVGs and the search index. Update the browser-ready files together with their sources.

`browser-check.mjs` is optional real-browser verification. It needs Node 22 and `google-chrome`, creates a disposable system-temp Chrome profile, and checks file-open navigation, search, theme, call-flow filter/selection/path and mobile menu. Run it with `http://127.0.0.1:8000/index.html` as its argument to test a local static server as well. Static `check.py` does not claim to replace a visual/a11y browser review.

## Add verified call-flow edges

Choose stable node IDs in `assets/call-flow.js`; attach the actual source path, line, signature, purpose and relevant documentation page. An edge needs real source evidence for its classification: `direct` for a checked call expression, `callback` for an evidenced dispatch, `external` for process/tool communication, `data` for a transferred artifact, or `conceptual` for architecture only. Include a source reference for both dispatch and receiving implementation where practical. Do not infer runtime invocation from an import, shared type, identical name, address or source line. Keep candidate relationships visibly distinct and leave uncertain edges out. Subsystem filtering keeps the explorer usable when DOC-2/DOC-3 add more nodes.

## Continue this website

- **DOC-2 (complete):** Engine chapters live under `engine-*-deep.md`, with the [source handbook](engine-handbook.html), [algorithms](engine-algorithms.html), [function reference](engine-functions.html) and six subsystem [walkthroughs](engine-walkthroughs.html). DOC-2 diagrams are in `doc2-diagrams.json`. Preserve shared navigation and graph IDs when extending.
- **DOC-3 (complete):** Parser chapters live under `parser-*-deep.md`, with the [source handbook](parser-handbook.html), [data structures](parser-data-structures.html), [algorithms](parser-algorithms.html), [function reference](parser-functions.html) and eight [walkthroughs](parser-walkthroughs.html). DOC-3 diagrams are in `doc3-diagrams.json`; parser graph nodes point to these chapters.
- **DOC-4:** Add actual end-to-end execution walkthroughs with real C fixtures and captured trace records, then perform browser/device and final source-reference checks. Do not fork a second documentation site.

The site intentionally excludes `src/semantic/` (SE-1/SE-2), frontend implementation, and Stage 2 tests and reports. A brief public downstream boundary describes **only** what Stage 1 provides. See [`DOC1-REPORT.md`](DOC1-REPORT.md), [`DOC2-REPORT.md`](DOC2-REPORT.md), [`DOC3-REPORT.md`](DOC3-REPORT.md), and [`DOC4-REPORT.md`](DOC4-REPORT.md) for the final release verdict and verification evidence.

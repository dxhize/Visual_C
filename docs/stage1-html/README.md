# Visual C Stage 1 · offline HTML reference

Open **[`index.html`](index.html)** directly in a browser (`file://` works). All pages, SVGs, styles, scripts and search data are local; no server, CDN or network access is needed. On the page, press `/` or `Ctrl/⌘+K` to search, `Esc` to close; use the theme button to toggle light/dark. On narrow screens use the menu button. The content, navigation and diagrams still render with JavaScript disabled (search/theme toggling need JavaScript). Print styles remove navigation.

## Edit and regenerate

Source of truth for this *website*:

- `content/*.md` — page bodies, a deliberately small Markdown subset (headings, paragraphs, code fences, tables, lists, callouts, `[text](relative-link)`, inline code and `{{diagram:key}}`). The **implementation**, not these docs, remains the source of truth for Stage 1 behavior.
- `diagrams.json` — editable node, arrow, alt text and caption data for the twelve locally generated SVG diagrams. Edit this file rather than generated `diagrams/*.svg`. Node `kind` chooses the legend color: `source`, `engine`, `parser`, `evidence`, `risk`; `vertical:true` stacks the nodes. Every diagram has descriptive alt text and an HTML caption.
- `build.py` — standard-library-only static renderer, shared navigation/layout, syntax coloring for Rust/C/JSON/shell, diagram and search-index generator.
- `assets/site.css`, `assets/site.js` — shared CSS and optional offline interactions. `assets/search-data.js` is generated instead of fetched so `file://` works even in browsers that deny local `fetch()`; `assets/search.json` is also produced for inspection.

From the repository root:

```sh
python3 docs/stage1-html/build.py
python3 docs/stage1-html/check.py
```

Run `python3 docs/stage1-html/build.py` after editing `content/`, `diagrams.json` or the page list in `build.py`. Commit the generated `.html`, `.svg` and search data alongside sources so browsing needs **no build**. No Rust/C source or architecture behavior is changed by this process.

## Verify examples and document structure

`check.py` (Python stdlib) verifies all 21 pages, local assets, internal links/anchors, the 12 parseable SVGs, alt text/captions, three-level headings, relative navigation, local scripts, and the absence of remote resources. `node --check assets/site.js` can additionally check JavaScript syntax if Node is installed; a browser with an HTML5 engine is recommended to inspect responsive styles and print layout. Tested Rust snippets use the existing public crate APIs; snippets labelled *body excerpt*, *signature excerpt*, *illustrative projection* or *abridged excerpt* are **not** claimed to be standalone programs/importable traces. The complete executable consumer is on [`usage.html`](usage.html); the larger checked-in oracle is `examples/stage1_public_consumer_audit.rs` at the repository root.

## Scope and limitations

This site documents the frozen Stage 1 C Trace Engine and Trace Parser as implemented. It is not Stage 2, a developer GUI, a sandbox, a complete descendant census, a proof of pointer provenance or AST execution, or a guarantee that final JSON limits bound assembled-trace RAM. [Stage 1 handoff](handoff.html) describes the exact qualified evidence contract and known limitations. The integrated audit and historical reports in `docs/` are supporting evidence, not a substitute for inspecting current source.

# 2 · How to read this reference

> **In one sentence:** Don't read in order. Start with a question, find the page that answers it, and only then open the code.

## Pick a starting point

| Your question | Go to |
|---|---|
| "How does a program turn into a trace?" | [Complete pipeline](pipeline.html) |
| "What is this type or function?" | [Function index](functions.html), then the [call-flow explorer](call-flow.html) |
| "Where does this file/directory fit?" | [Repository map](repository.html), [Source file index](files.html) |
| "What does this recorded value mean?" | [Identity contracts](identity-contracts.html) |
| "Why is an allocation missing?" | Heap overview → event-reader links → coverage links → compare with the actual trace |

> **Example.** To find why an allocation is missing, follow that last route and then compare with the real trace file. Finding a record with a *matching address* is not an answer, because addresses are reused.

## What an index entry does and doesn't tell you

| An index/graph entry… | Means | Does **not** mean |
|---|---|---|
| Lists a file or definition | The code exists at that path | It ran in any particular recording |
| Was found by a script | A real Rust/C definition at a real location | A verified call, or a written explanation of its algorithm |
| Appears as a graph edge | Someone read the production caller and callee | — (edges are published only after this check) |
| Is labeled **"conceptual"** | It explains the architecture | A runtime function call |

For deeper reading, the **DOC-2 engine chapters** and **DOC-3 parser chapters** include source-guided walkthroughs, branches and error paths. Use the navigation to open them.

## Version this reference describes

This is a map of the **inspected working tree** (the folder of code as it was on disk), not of a commit alone.

- Base revision: `558037c0da180e152a3e6cc3f351edf0b0583215`
- The parser and its tests had **uncommitted changes** at inspection time.
- Line numbers refer to that state and can shift as the repository changes.

## Navigating offline

- **Sidebar, breadcrumbs, outline, previous/next** all work from checked-in HTML.
- **Search** uses a generated local JavaScript index (no `fetch`), covering source paths and symbol names.
- **Source links** in a `file://` browser open a local repository file. The adjacent `path:line` text gives the exact location even if the browser can't jump to the line.
- The [maintenance guide](README.md) explains regeneration.

## Read evidence before inference

Keep these five layers separate in your mind:

| # | Layer | Example of confusing it with another |
|---|---|---|
| 1 | The recorded execution | — |
| 2 | Static source metadata | A syntax match is **not** an executed expression |
| 3 | Independent diagnostics (other runs) | An address match is **not** a lifetime identity |
| 4 | Validation findings | A successful import is **not** semantic validation |
| 5 | Downstream interpretation | — |

The [limitations page](limitations.html) is part of every walkthrough, not an appendix to skip.

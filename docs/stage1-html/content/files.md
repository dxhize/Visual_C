# 10 · Source file index

> **In one sentence:** A table of contents for the actual source tree. A path tells you *where* an implementation lives, not that it ran.

## Key terms

| Term | Meaning |
|---|---|
| **Generated catalog** | A listing built by scanning the current source tree, so you can navigate even when a file has no long explanation yet. |
| **Source location** | A file and line number where a definition can be read. The number may shift after code edits. |

> **Example.** Searching `src/gdb_mi.rs` helps you find the debugger commands. It does **not** show a particular command ran in your trace. That needs recorded evidence.

## What's listed

- Every Stage 1 Rust file under `src/`
- The allocator wrapper under `c_runtime/`
- **Not** `src/semantic/` (deliberately excluded; it's Stage 2)

File names, paths and symbols come from the checked-in, generated, machine-readable inventory. Each file's short responsibility note describes a *subsystem*. It is not a replacement for reading the implementation.

## Browsing the catalog

Select a file to open its source, or follow its **symbol count** to the [function index](functions.html). Line locations refer to the inspected working tree.

> **Tip:** Local browsers may open the source file but ignore the line number. The adjacent `path:line` text is the reliable locator.

### Stage 1 file catalog

{{inventory:files}}

## How far to trust this list

| Question | Answer |
|---|---|
| How is the list checked? | Against repository **file discovery**, not an old design document. |
| Are definitions and module references verified calls? | No. They're labeled as *automatically discovered* in the JSON. |
| Where are verified **execution edges**? | Maintained separately in the [call-flow dataset](call-flow.html). |
| Do adjacent file names form a call chain? | No. For how files cooperate, use the walkthroughs. |
| Are there fuller explanations? | Yes. Follow the DOC-2 engine handbook and DOC-3 parser handbook for the existing implementation walkthroughs in this same website. |

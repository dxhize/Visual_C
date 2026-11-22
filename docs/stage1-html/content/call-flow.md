# 13 · Interactive call-flow explorer

> **In one sentence:** An interactive map of *reviewed* paths through the Stage 1 code. The arrow labels say what kind of relationship each arrow shows.

## Arrow types

| Label | Meaning |
|---|---|
| **Direct** | A checked call expression in source: one function explicitly calls another. |
| **Callback dispatch** | A call routed through a callback rather than named directly. |
| **Subprocess communication** | Talking to an external process (GCC, GDB). |
| **Data handoff** | Data passed from one part to another, such as a file or struct. |
| **Conceptual** | Two parts belong to the same explanation; **no** runtime function call is claimed. |
| **Dashed / candidate** | A possible link. It is **not** an invocation. |

These types stay visually and textually distinct on the graph.

> **Example.** Select the path from `trace_program` to GDB capture. Open each source link to verify exactly where a function is called, and use the other arrow types for tool or data boundaries.
>
> An arrow between source functions does **not** mean that route ran in every recorded trace.

## What the graph covers

It follows selected **verified Stage 1 production relationships**:

```
CLI  →  trace  →  JSON loading  →  validation  →  consumer queries
```

Two nodes are explicitly labeled **external tools** (GCC and GDB). They stand for process boundaries and are **not** made-up Rust functions.

The graph is **deliberately selective**. If a function is missing, check the complete [function index](functions.html). Selecting a node shows its source, purpose, incoming and outgoing edges, each with its classification and supporting references.

## Exploring

- **Filter** by subsystem, or **search** by function or path.
- **Select** a node to expand its neighborhood.
- **Walk** a multi-module path by choosing adjacent nodes.

### Interactive graph

{{explorer}}

## Guided execution paths

The **Guided path** selector offers six source-verified tours:

| # | Tour |
|---|---|
| 1 | CLI → export |
| 2 | GDB stop → checkpoint |
| 3 | Allocator wrapper → heap lifetime |
| 4 | DWARF extraction → type registry |
| 5 | JSON input → checkpoint summary |
| 6 | Deep validation → semantic confidence |

`Previous` and `Next` move one real graph node at a time, and each step names the relationship and its source location. Choose **Free exploration** to return to the full neighborhood view.

## Where the data comes from

- Graph entries live in `assets/call-flow.js` with stable IDs and source references.
- A `path:line` is copyable. Local source files open without assuming your browser supports line anchors.
- **No Stage 2 symbol** is included.
- This graph is a **guided subset**, not a complete dynamic call graph. The DOC-2/DOC-3 chapters give deeper explanations; future reviewed edges can be added without changing the frontend.

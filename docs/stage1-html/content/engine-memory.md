# 16 · Stack frames, variables and raw memory

> **In one sentence:** A variable can have a displayed value, an address and captured raw bytes. They are related observations that answer *different* questions.

## Key terms

| Term | Meaning |
|---|---|
| **Frame activation** | One particular *call* of a function. Two recursive calls have two activations even if both have a local named `n`. |
| **Captured prefix** | The bytes actually read from the *start* of an object. A missing tail has **not** been filled with zeros. |

> **Example.** For `int *p`, `address` is where the variable `p` itself lives; `points_to` is the address *stored inside* it. Reading `p` successfully doesn't prove the memory at its target is alive or initialized.
>
> The debugger can also read bytes from an **uninitialized** object. Being readable is not an initialization test.

## Level 1 · Intuition

A displayed debugger value is an observation made at one stopped moment, not proof of C-language initialization. Visual C records:

- where a variable lived,
- what GDB could display,
- whether bytes were actually read, and
- which limits prevented a fuller read.

Each function call creates a new frame activation, so two locals named `n` in different recursive calls are **not** the same variable.

{{diagram:stack}}

## Level 2 · At a stopped checkpoint

### Where things live in a snapshot

An `ExecutionSnapshot` is one saved pause.

| Part of the snapshot | Holds |
|---|---|
| `variables` | Variables of the currently selected function call, called **frame 0** |
| `caller_frames` | *(If configured)* bounded information about the functions that called it |
| `snapshot.observation.globals` | Globals and file-static variables |
| `call_stack` | Frames with their **GDB levels** |

> The position of an item inside `caller_frames` is a **different index** from a GDB level. Don't swap them.

Each observed call has an **activation ID**, so `recurse(3).value` and `recurse(2).value` are never merged just because both locals are named `value`.

- GDB's Python support can help find exact lexical blocks and read a shadowed variable.
- If those details are unavailable, the engine labels the **weaker fallback** rather than pretending it has the same identity confidence.
- A variable being *in scope* does not prove its initializer has run.

### The two addresses of a pointer

For `int *p`:

| Field | Meaning |
|---|---|
| `VariableInfo.address` | Where **the pointer variable `p` itself** is stored |
| `points_to` | The address value *inside* `p` |
| `address_match_allocation_id=7` | A *possible* numeric match, if the target lies in heap block 7. It does **not** say `p` originally came from block 7. Addresses may be reused. |

### Separate memory questions, separate fields

| Field | Answers |
|---|---|
| `read_status` | What GDB's **printed value** looked like |
| `bytes_status` | Whether the **raw byte read** succeeded |
| `memory_evidence` | How many bytes were **requested** and how many **captured** |
| `initialization_status` | Whether it was initialized. Separate, and **often unknown** |

If only the first four of eight bytes were captured, `raw_bytes_hex` contains that four-byte prefix. The other four have **not** been read or zero-filled.

A requested per-instruction memory region belongs only to its **actual instruction stop**. Bytes saved at an older checkpoint are historical elsewhere.

### Worked call

```c
// Illustrative C; see fixtures/c/recursive.c for the real tested fixture.
int recurse(int value) {
    if (value == 0) return 1;
    return value * recurse(value - 1);
}
```

When `recurse(3)` calls `recurse(2)`, the second frame is a distinct activation.

- The audited engine trace with `limits.max_caller_frames=3` captured **45 caller-frame entries across 26 snapshots**. That is not an unlimited call history.
- Each checkpoint is a **historical observation**: later returns or frees must not change earlier captures.
- The [TP-7 frame activation](parser-consumer.html#level-3-real-signatures-and-bounded-traversal) query resolves only **within one retained snapshot**.

## Level 3 · Where to look

| File | Role |
|---|---|
| `src/gdb_mi.rs` | Extracts `StackFrameInfo`, `RawVariable` and bounded GDB reads |
| `src/trace.rs::push_snapshot` | Collects checkpoint state, heap blocks and caller frames |
| `src/memory.rs` | Enriches byte and target observations |
| `src/recorder.rs::set_validity` | Keeps display status and byte status distinct |
| `src/dwarf.rs::associate` | Correlates **unique** declarations without replacing GDB values |
| `src/models.rs::{ExecutionSnapshot, SnapshotObservation, VariableInfo, MemoryEvidence, FrameVariables, ScopeStatus, DataValidity}` | The schema |
| `src/trace_parser/semantic.rs::MemoryEvidenceView` | Qualifies captured byte ranges without decoding C semantics |

```json
{"name":"sum","value":"10","raw_bytes_hex":"0a000000","read_status":"valid","bytes_status":"valid"}
```

This is an **abridged excerpt** of the audited `phase8b_defined.c:15` variable, not a stand-alone `VariableInfo` document. Four readable bytes show a captured representation of 10, **not** proof of prior initialization. See the [data model](model.html#variables-bytes-and-heap) and [confidence](parser-semantic.html).

> **Note:** Caller capture defaults to `max_caller_frames=0`. A missing caller vector is **not** evidence the program had no caller. Per-snapshot and per-variable read budgets may truncate capture without invalidating the other observations.

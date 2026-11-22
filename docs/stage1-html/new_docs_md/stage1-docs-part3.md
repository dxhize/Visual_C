# Visual C · Stage 1 Documentation — Part 3

**Covers chapters 14–17:** Trace Engine (compilation and diagnostics, GDB/MI and checkpoints, frames/variables/memory, DWARF/Clang/fusion).

These four chapters follow the engine in the order it works: **build the program → pause it and snapshot it → read what's in the snapshot → add type and source descriptions**. The layout matches the earlier parts, and all code excerpts, field names, file paths and numbers are unchanged.

---

<!-- BEGIN CHAPTER: content/engine-build.md -->
# 14 · Compilation and compiler diagnostics

> **In one sentence:** Before a debugger can inspect C, GCC has to build a real program. If that fails, the trace records the failure and does **not** pretend a run happened.

## Key terms

| Term | Meaning |
|---|---|
| **Debug build** | An executable with source-to-machine debug information and the project's allocation wrapper linked in. |
| **Sanitizer build** | A separate executable with compiler-added memory checks. It can fail or be skipped independently of the debug build. |

> **Example.** For `return missing_symbol;`, GCC reports a compiler error. The trace keeps that error and has **zero** GDB snapshots, rather than inventing a run that never started.
>
> A successful sanitizer *build* also doesn't mean its separate *execution* ran or found no errors.

## Level 1 · Intuition

Visual C needs a real executable with debug information, not a guess from reading C text. It runs GCC on your sources and keeps warnings and errors as **structured diagnostics**. If compilation fails, the trace says so and does not pretend the debugger ran. The same project can also produce a separate sanitizer build for a later diagnostic run.

{{diagram:compilation}}

## Level 2 · Build lifecycle

### What describes the request

| Config | Holds |
|---|---|
| `BuildConfig` | The C files and compiler options (include directories, extra flags) |
| `TraceConfig` | Everything in `BuildConfig`, plus the program's own arguments, output path, time and byte limits, and optional tools |

### The two builds

| | **Debug variant** | **Sanitizer variant** |
|---|---|---|
| Built by | `compiler::compile_project` | The same function (a *different* build) |
| Includes the allocator wrapper (`c_runtime/alloc_tracker.c`)? | **Yes** | **No** |
| How allocations are logged | Linker switches `-Wl,--wrap=malloc` (and similar for `calloc`, `realloc`, `free`) send calls through a small logging function before the real allocator | Not wrapped |
| Compiler checks | Debug info | **ASan/UBSan** (AddressSanitizer for memory errors, UndefinedBehaviorSanitizer for undefined C behavior) |
| Used for | The primary GDB run | A later, separate diagnostic run |

Other build details:

- The engine detects which **compiler-diagnostic format** GCC supports, and saves each error with a file, line, kind and message.
- Time and output bounds apply while GCC runs.
- **User-supplied flags can change what the compiler emits**, so keep them in mind when comparing runs.

### What happens if the build fails

| Situation | Result |
|---|---|
| GCC rejects the C source | `compiled_successfully=false`, `execution_status=compile_failed`; the compiler's explanation is kept; later debugger stages are marked **skipped**; `snapshots=[]` (there was no program to pause) |
| GCC fails to *start*, or a deadline expires | A **different kind** of failure from GCC rejecting your source |
| Debug build succeeds | The engine can read DWARF type information and start GDB |
| Sanitizer executable builds | Says nothing about whether its separate run found, or ruled out, a crash |

## Level 3 · Files, signatures and example

| File | Provides |
|---|---|
| `src/config.rs::{BuildConfig, TraceConfig, SanitizerTrigger}` | The request |
| `src/compiler.rs::{CompileOptions, compile_project, detect_diagnostics_format}` | Both build variants and error formats |
| `src/trace.rs::trace_program_inner` | Assigns `stage_results.compiler`; short-circuits on failed compilation |
| `src/models.rs::{CompilerDiagnostic, StageResults, StageResult}` | Structured outcomes |
| `src/supervisor.rs` | Subprocess groups and session deadlines |

```c
// Source fixture: fixtures/c/phase8b_defined.c (abridged)
int oracle_global = 41;
static int checkpoint(struct Packet *packet, int *alias, int *heap, int sum) {
    volatile int value = packet->items[1] + *alias + heap[2] + sum + oracle_global;
    return value;
}
```

```sh
cargo build --release
target/release/c-trace-engine fixtures/c/phase8b_defined.c \
  --trace-mode dual --max-steps 350 --timeout 50 --no-pretty \
  --sanitizer-trigger never --output /tmp/visual-c-defined.json
```

**A real failure case.** From the [final integrated audit](../stage1-final-audit.md): a deliberately invalid `return missing_symbol;` produced an engine JSON trace with `execution_status="compile_failed"`, one compiler diagnostic, and **zero** snapshots and events.

For safe output inspection see [practical usage](usage.html#level-2-run-a-recording); for independent analysis stages see [diagnostics](engine-diagnostics.html).

> **Note:** Tool availability is checked **before** compilation. GCC and GDB are required. A missing Valgrind or `addr2line` is reported and qualified, not treated as a successful zero-error analysis. No part of this pipeline is a sandbox for hostile C.
<!-- END CHAPTER: content/engine-build.md -->

<!-- BEGIN CHAPTER: content/engine-debugger.md -->
# 15 · GDB/MI execution and checkpoints

> **In one sentence:** A debugger pause is a *photograph*, not a video of everything between two pauses. The engine must also tell a command reply apart from a real new stop.

## Key terms

| Term | Meaning |
|---|---|
| **Program counter (PC)** | The machine address of the instruction GDB is **about to** execute at a stop. It is not automatically an instruction that already ran. |
| **MI acknowledgement** | GDB saying it *accepted* a command (`^running`). A separate `*stopped` message says the program actually stopped later. |
| **Inferior** | The program GDB is controlling. |

> **Example.** At one stop, GDB reads `sum=10`. That value belongs to *that stop*. If an instruction step never reports another stop, the engine can't claim the attempted instruction completed.

> **Three different numbers:** a **stop counter**, a **snapshot's vector index** and an **attempted-instruction position** are separate. Never swap them.

## Level 1 · Intuition

A stopped program is observable; code running between stops is not automatically known. The GDB/MI2 driver sends commands, waits for each response and for the asynchronous stops, and inspects bounded source state.

- At a stop, the PC names the **next** instruction.
- An instruction attempt is recorded from that *before* address, plus a *next* address **only when observed**.
- Source checkpoints are **selected** stops, not a reconstruction of every intermediate state.

{{diagram:checkpoint}}

## Level 2 · Command/response lifecycle

### What the engine does at a chosen pause

1. GDB starts the debug program and pauses it.
2. The engine notes the **function and source line**.
3. It reads available variables and heap bytes, **within the configured budgets**.
4. It collects new allocator log lines.
5. It assigns the stop a `stop_id` that increases as stops are observed.

### Stepping one instruction

In *dual* source-and-instruction mode, the engine can send `-exec-step-instruction`.

```
engine ──► -exec-step-instruction
GDB    ──► ^running        ← only means "I started the step"
GDB    ──► *stopped        ← NOW we know where execution stopped
```

An `InstructionEvent` stores the address *before* the attempted step, its event number and context, and a next address only when observed.

| Situation | How it's handled |
|---|---|
| The stop never arrives | Reported differently from the **whole session** running out of time |
| A command reply is missing | Late text is **not** handed to a different command |
| Breakpoint hit | Gives the engine a stop, but that alone doesn't prove the attempted instruction finished |
| Stepping skips a library interval | A [TraceGap](model.html#events-gaps-and-checkpoints) marks what was **not** recorded, instead of inventing a number of missing instructions |

### How a step is classified

A step can be recorded as **completed**, **faulted**, **interrupted** or **unknown**.

### What GDB does not cover

- The engine may set bounded breakpoints to catch some callbacks, but a function *appearing in source* is not evidence it was called.
- GDB pauses its selected debugging session. Outside processes and unfollowed fork children are not guaranteed snapshots.
- Observing one thread does not give a complete timeline of all threads.

### Worked example: a stop, then a parser query

At source line 15 of `fixtures/c/phase8b_defined.c`, the audited live capture recorded:

| Item | Value |
|---|---|
| Variable | integer `sum` |
| Displayed value | `"10"` |
| Raw bytes | `"0a000000"` (Linux little-endian) |
| Independent check | Native GDB, in a *separate* run, printed `sum=10` |

A later `TraceStore` can fetch that snapshot by its **vector index**, and `checkpoint_summary()` can page its variables. Neither the stop ID nor the value of `instruction_position` should be used as that vector index. See the [API walkthrough](usage.html#checkpoints-and-paging).

## Level 3 · Source and invariants

| File | Owns |
|---|---|
| `src/gdb_mi.rs` | MI command tokens, async records, stop backlog, frame parsing, selected-thread tracking, memory reads |
| `src/mi_parser.rs` | MI syntax parsing |
| `src/trace.rs` | The stop loop and `push_snapshot` |
| `src/recorder.rs::{checkpoint, event, gap, event_outcome}` | Appends bounded canonical records and journal lines |
| `src/models.rs::{SnapshotObservation, InstructionEvent, TraceGap}` | The serializable identities |
| `src/trace_parser/indexes.rs::OrderingFacts` | Enables binary search **only when order is verified** |

```json
{"step_index":0,"observation":{"stop_id":1,"instruction_position":0,"heap_event_count":0}}
```

This JSON is **illustrative and deliberately partial**. A real `ExecutionSnapshot` has required file, function, call stack and variable fields. In it:

| Field | What it is |
|---|---|
| `stop_id=1` | A debugger **stop counter** |
| `step_index=0` | A **snapshot vector index** |
| `instruction_position=0` | A **count of attempts** before this stop |

See [schema identity domains](schema.html#identity-domains).

> **Warning:** Recording a `checkpoint_id` on an instruction event associates it with the *most recent* checkpoint. It is **not** a claim that the instruction happened precisely at that checkpoint stop, or that a source-line AST expression executed.
<!-- END CHAPTER: content/engine-debugger.md -->

<!-- BEGIN CHAPTER: content/engine-memory.md -->
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
<!-- END CHAPTER: content/engine-memory.md -->

<!-- BEGIN CHAPTER: content/engine-metadata.md -->
# 17 · DWARF, Clang AST and evidence fusion

> **In one sentence:** Type information comes from the built program and optional syntax information from the source. Neither replaces reading runtime memory.

## Key terms

| Term | Meaning |
|---|---|
| **DWARF** | Compiler-written debug info describing compiled types, declarations and source locations. |
| **AST** | Clang's tree of *written* C syntax. It can show a call appears in the source without showing the program took that branch. |
| **Fusion** | One cautious pass that links existing records from different providers. It never supplies new values or invents writes. |

> **Example.** Three tools answer three different questions:
>
> | Tool | Question it answers |
> |---|---|
> | DWARF | Where is `struct Pair.y` located inside the compiled struct? |
> | GDB | What bytes were there at this stop? |
> | Clang | Is there a written `malloc` call in the source? |
>
> A **same-line** source match is only a *candidate*, especially if macros or several calls share that line.

## Level 1 · Intuition

Static metadata helps explain what a stopped value *might be*: DWARF identifies the compiler's types and declarations, and optional Clang AST identifies syntax and source spans. GDB supplies the dynamic runtime values. **None** of the static sources proves a C expression executed.

{{diagram:metadata}}

## Level 2 · The three evidence streams

### Stream 1: DWARF

| Step | What happens |
|---|---|
| `dwarf::extract_excluding` | Reads debug info from the **program GCC actually built**, leaves out the injected allocator logger, and saves bounded lists of compiled types and declarations in `TypeRegistry` |
| `dwarf::associate` *(later)* | Links a stopped GDB variable to a type or declaration **only when it finds a unique match**; keeps the observed size and address exactly as GDB reported them |

### Stream 2: Clang AST (optional, `--clang-ast`)

- Clang separately reads the C source into `SourceMetadataRegistry`.
- A missing Clang tool or partly parsed source is marked **missing or partial**, not mistaken for an empty program.
- GCC and Clang can use different compilation settings, so a link between their descriptions isn't always justified.

### Stream 3: Fusion

After the engine finishes capturing runtime and reconstructing the heap, `evidence::fuse` makes **one** cautious linking pass:

| File | Handles |
|---|---|
| `declarations.rs` | Compiled declarations and members |
| `source_links.rs` | Links to observed variables |
| `heap.rs` | Marks *possible* source locations for allocator calls |
| `validation.rs` | Removes links that can't be supported |

The summary counts links kept and rejected.

| Fusion **does** | Fusion **does not** |
|---|---|
| Link existing records, cautiously | Ask GDB for new values |
| Count kept and rejected links | Copy the type registry for every pause |
| | Turn a possible source call into an executed call |

The parser later stores **positions** for these metadata IDs instead of cloning their records.

### Worked ambiguity

Two heap-call expressions on one source line, or a macro expanding to one, can share a line with a recorded allocation event. The relationship can be a **candidate** or **ambiguous**. A same-line match is never a dynamic expression-to-event identity.

So TP-6 `source_expression(id).execution_confidence` stays `Unavailable` even when `heap_call_candidates` exist. This matters when writing [future Stage 2 explanations](handoff.html#level-2-the-freeze-contract).

## Level 3 · Files and API

| File / function | Role |
|---|---|
| `src/dwarf.rs::{TypeRegistry, extract_excluding, associate}` | Reads ELF/DWARF, matches unique declarations, reports missing or ambiguous data |
| `src/clang_ast.rs::{SourceMetadataRegistry, analyze, correlate_heap_calls}` | Produces optional static syntax |
| `src/evidence/mod.rs::fuse` and `src/evidence/{declarations,source_links,heap,validation}.rs` | Bounded reconciliation |
| `src/models.rs::EvidenceFusionSummary` | Serializes the counts |
| `TraceStore::type_entry(id)`, `dwarf_declaration(id)`, `source_declaration(id)`, `source_expression(id)` | Return borrowed, indexed metadata |
| `ValidatedSemanticView::source_expression(id)` | Adds confidence plus indexed candidate/ambiguity event positions |

```rust
// Real TP-6 API; expression IDs belong to this trace, not to other runs.
let expression = validated.source_expression(expression_id);
let syntax = expression.expression;                  // Option<&SourceExpression>
let execution = expression.execution_confidence;     // Unavailable on sound traces
for candidate in expression.call_sites() {
    let _ = (candidate.event_position, candidate.event);
}
```

The snippet assumes `validated` and `expression_id` are already defined. Run the complete [parser walkthrough](usage.html#level-3-load-and-validate) first. Signatures are in the [public API reference](api.html#level-3-signatures-and-contracts).

> **Warning:** "source line observed" is **not** "specific AST expression executed." Never use an AST call candidate to grant pointer provenance or to allocate a new runtime lifetime.
<!-- END CHAPTER: content/engine-metadata.md -->

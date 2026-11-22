# 3 · Architecture · one recording, many qualified readers

> **In one sentence:** The engine records, the parser reads, and every kind of evidence is kept labeled with what it can and cannot prove.

## Level 1 · Intuition

Imagine recording a short video of a C program and giving it to someone who wasn't there.

- The **Trace Engine** is the *camera*. It compiles the program, runs it under a debugger, and saves the moments it actually observed.
- The **Trace Parser** is the *viewer*. It opens the finished recording and helps other code find moments in it.

The viewer can't ask the finished program what happened *between* two recorded moments. That's why the trace also records **gaps**: the places where the camera wasn't looking. The recording is a JSON file, and parser queries point back to its original observations rather than changing it.

{{diagram:architecture}}

## Level 2 · Pipeline and boundaries

### The tools, in plain words

| Tool / term | What it is | Important limit |
|---|---|---|
| **GCC** | The C compiler | — |
| **Debug executable** | A compiled program with extra info for debuggers; the engine also links an allocation logger into it | — |
| **DWARF** | The debug-info format inside that executable | Describes the *compiled result*; gives no value for every variable at every moment |
| **Clang AST** *(optional)* | A tree Clang builds from the *source text* (it can spot the written `malloc(40)`) | Text presence does **not** show the call ran |
| **GDB** | The debugger that runs and pauses the executable | Sees pauses, not the time between |
| **MI2** | GDB's machine-readable command format, so the engine can ask for a frame or variable automatically | — |
| **Inferior** | The program GDB is debugging | A *followed* inferior is observed; a forked child it doesn't follow gets no automatic snapshots |

### What happens, in order

| # | Step | What happens |
|---|---|---|
| 1 | **Configure and build** | A `TraceConfig` names the C files, program arguments, and time/memory/recording limits. GCC builds the debug program and may *try* a second build with a **sanitizer** (compiler-inserted checks for buffer overflows, use-after-free, etc.). If compilation fails, the result holds compiler errors, not imaginary snapshots. |
| 2 | **Read types, then observe** | The engine reads DWARF. If requested, Clang examines the source. GDB runs the program and pauses it at selected **stops**. A stop is a moment we can ask "what does this variable look like *right now*?" |
| 3 | **Log allocations** | A **wrapper** around `malloc`, `free`, etc. acts as a detour: the call reaches the wrapper, which logs it and then calls the real allocator. Each line carries a **PID** (which process) and a **sequence number** (order within that process). The engine reads new lines at each stop. The log goes to its own file, never mixed into the program's output. |
| 4 | **Package and export** | Observations become one `DebugSessionTrace`, exported as JSON. A **JSONL journal** (one small JSON record per line) may be written along the way; it helps keep a partial run but can't rebuild the final document. |
| 5 | **Load and read** | `TraceStore::open` loads the JSON and builds position lookups. Loading only says the file has a *supported shape*. Call `validated_semantic()` to check consistency. Queries return references to original records, in pages of **at most 128** records. |

{{diagram:compilation}}

### Three runs that must not be mixed up

| Run | What executes | Reports | Can supply Run 1 allocation IDs? |
|---|---|---|---|
| **Run 1 · primary** | Debug executable under GDB | Stopped variables and heap events with IDs like "allocation 17" | **Yes** (via its own wrapper) |
| **Run 2 · if enabled** | Separately built sanitizer executable | Memory problems caught by compiler-added checks | No |
| **Run 3 · if enabled** | **Valgrind** on another execution (no sanitizer build needed) | Invalid memory operations, leaks | No |

Each run has its own status: succeeded, failed or skipped.

> **Example.** Valgrind reports "40 leaked bytes at line 52." That's useful, but it does **not** prove the leaked block is allocation 17 from Run 1, since addresses and allocation order can differ between runs.

### What each evidence provider can and cannot prove

| Provider | Can tell you | Cannot prove by itself |
|---|---|---|
| GCC and DWARF | How the compiled program describes types, declarations, source lines | Which expressions ran, or what a variable held later |
| GDB/MI | State at a recorded pause, including bounded reads | Anything between pauses, or an unfollowed child's state |
| C allocation wrapper | Calls it intercepted in the primary run, with PID, order, ID | Allocations made wholly inside unwrapped shared libraries |
| Optional Clang AST | Code that appears in the source, and where | That an expression executed |
| Valgrind / sanitizers | Errors in their own executions | The exact primary-run allocation ID of an error |

## Level 3 · Trace through the implementation

Start at `src/trace.rs::trace_program`. In order, it:

1. checks `ExecutionLimits`,
2. starts a supervised session,
3. calls `compiler::compile_project` to build the program,
4. extracts DWARF with `dwarf::extract_excluding`, and
5. runs `clang_ast::analyze` only if configured.

The stop loop uses `src/gdb_mi.rs`, `src/recorder.rs` and `src/allocation_tracker.rs`. `src/models.rs` defines the saved data; `src/json_export.rs` writes it.

On the reading side, `src/trace_parser/mod.rs::TraceStore::assemble` keeps one shared `Arc<DebugSessionTrace>` plus position indexes. **`Arc`** is Rust's shared-ownership handle: many readers share one trace without copying its records.

```rust
// Core architecture signatures; source: src/trace.rs and src/trace_parser.
pub async fn trace_program(config: TraceConfig) -> Result<DebugSessionTrace>;
// TraceStore's public entry point (method declaration):
pub fn open(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<Self>;
// Constructing this view runs one opt-in validation pass:
pub fn validated_semantic(&self) -> ValidatedSemanticView<'_>;
```

> **Note:** These are **signature excerpts**, not one runnable file; each lives in its own module. See [practical usage](usage.html) for a working program.

### Four questions to ask at every boundary

Before treating two facts as one story, ask:

- [ ] Did GDB observe this?
- [ ] Is it only a description of the source?
- [ ] Did it come from another run?
- [ ] Is it a parser warning or a later interpretation?

For instance, a pointer holding an address and an allocation recorded at that address are **two separate observations**. [Evidence confidence](parser-semantic.html) shows how the parser keeps them apart.

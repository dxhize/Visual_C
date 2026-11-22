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

# c-trace-engine

## V2: synchronized source and instruction recording

Dual capture is now the default. Source checkpoints and executed-instruction
attempts come from **one GDB run**, not two executions. Legacy JSON fields are
retained with `schema_version: 2`; see [V2 schema and semantics](docs/v2.md).
New to the engine or want the plain-language version? Start with
**[The Trace Engine](docs/trace-engine.html)** — a visual, self-contained
introduction (pipeline, trace contents, guarantees, limits). For a conceptual
architectural overview (channels, budgets, the stop loop, the trust model),
see [Engine architecture](docs/architecture.md).

```sh
cargo run -- sample.c --timeout 20 --output trace.json
cargo run -- sample.c --trace-mode dual --sanitizer-trigger always
cargo run -- sample.c --trace-mode source             # lower-cost capture
cargo run -- sample.c --max-instruction-events 5000 --max-gdb-commands 50000
cargo run -- sample.c --journal run.jsonl --journal-only --output index.json
cargo run -- fixtures/c/globals.c --memory-region '&global_value:4'
```

`--timeout` is a **whole-pipeline monotonic deadline**, including builds and
analyses. Expired sessions do not start new diagnostic passes. Process cleanup
has a two-second grace allowance. Filesystem/export latency is not a real-time
guarantee. Run `cargo run -- --help` for the full option list.

New modules: `supervisor.rs` (deadlines/child ownership), `recorder.rs`
(disassembly/register/event recording), `processor.rs` (validation/replay),
`memory.rs` (evidence-based pointer/symbol enrichment), and `storage.rs`
(incremental, recoverable JSONL). Instruction gaps and missing values are
explicit, including skipped allocator wrappers and libraries.

Dependencies: Rust/Cargo, Linux, GCC, GDB; Valgrind and addr2line are optional.
GCC sanitizer runtime availability is detected at link time. Everything runs
locally; no cloud service is required.

A standalone Rust CLI that takes a configurable C project, compiles it with
GCC, executes it under GDB/MI2 while recording execution snapshots and heap
allocation events, runs an independent Valgrind analysis plus a separate
sanitizer pass, and emits a single structured JSON
[`DebugSessionTrace`](examples/example_trace.json).

It is the backend for a future offline-first visual C IDE: every artifact in
the trace — snapshots, variables, pointer links, heap blocks, allocation
history, compiler diagnostics, crashes and memory-analysis findings — is
derived from real observations of the program. The engine never fabricates
values it could not observe; unobservable data is `null`.

## Quick start

```sh
cargo run -- sample.c
```

writes the trace to stdout and exits `0` on success. Useful variants:

```sh
cargo run -- sample.c --output trace.json          # also save to a file
cargo run -- sample.c -- --verbose 42              # args for the program
cargo run -- fixtures/c/multi_file/main.c --add-source fixtures/c/multi_file/helper.c
cargo run -- sample.c --timeout 3 --max-steps 200 --max-snapshots 100
cargo run -- sample.c --config build.json          # JSON BuildConfig file
cargo run -- broken.c                              # exit code 2 + diagnostics
```

## Exit codes

| Code | Meaning |
|------|---------|
| `0`  | A trace was produced (execution completed, crashed, timed out, hit a limit). |
| `2`  | Compilation failed; a partial trace with the compiler diagnostics is still emitted. |
| `1`  | Operational error (bad arguments, missing source file, missing required tool). No trace. |

## Trace contents (abridged)

```jsonc
{
  "execution_status": "completed | compile_failed | crashed | timeout | step_limit_reached | output_limit_reached | ...",
  "compiled_successfully": true,
  "compile_errors": [ { "file": "...", "line": 1, "column": 15, "kind": "error", "message": "..." } ],
  "snapshots": [                     // one per GDB stop in user code (bounded)
    {
      "step_index": 0, "file": "sample.c", "line": 30, "func": "main",
      "call_stack": [...], "variables": [ { "name": "arr", "c_type": "int *",
      "value": "0x0", "address": "0x7fff...", "points_to": "0x0", "raw_bytes_hex": "..." } ],
      "heap_blocks": [ { "allocation_id": 1, "address": "0x705010", "size_bytes": 12,
                         "raw_bytes_hex": "00000000...", "allocation_file": "sample.c",
                         "allocation_line": 30 } ],
      "stdout_so_far": "..."
    }
  ],
  "heap_events":  [ { "event_type": "allocated", "allocation_id": 1, "size_bytes": 12 }, ... ],
  "heap_state":   [ { "allocation_id": 1, "allocation_type": "malloc", "is_freed": false, ... } ],
  "crash_report": { "signal_name": "SIGSEGV", "file": "...", "line": 7, "backtrace": [...],
                    "asan_summary": "SUMMARY: AddressSanitizer: SEGV ..." },
  "sanitizer_report": { "reproduction": "reproduced", "message": "..." },
  "valgrind_report": { "errors": [...], "detailed_errors": [...], "definitely_lost_bytes": 0, ... },
  "analysis_notes": [ "..." ],       // degradations, fallbacks, skipped analyses
  "resource_usage": { "wall_time_ms": 3085, "gdb_commands_issued": 455,
                      "snapshots_captured": 26, "limits_triggered": [] },
  "session_meta": { "gcc_version": "...", "gdb_version": "...", "valgrind_version": "..." }
}
```

See [`examples/example_trace.json`](examples/example_trace.json) for a full
real document produced by `cargo run -- sample.c`.

## How it works

```
                     ┌──────────────┐
  .c sources ───────▶│   compiler   │──▶ program_debug  (+ alloc_tracker.c)
  BuildConfig ──────▶│  (GCC/SARIF) │──▶ program_asan   (sanitizers)
                     └──────────────┘
                            │
                     ┌──────▼───────┐     pty (program stdout, never mixed with MI)
                     │ gdb_mi.rs    │◀──▶ gdb --interpreter=mi2
                     │ trace.rs     │     step / finish / continue loop
                     └──────┬───────┘
                            │  per stop: variables, frames, live heap reads
                            ▼
                     alloc_events.txt ◀── __wrap_malloc/calloc/realloc/free
                     (dedicated file)      (link-time -Wl,--wrap instrumentation)
                            │
        ┌───────────────────┼────────────────────┐
        ▼                   ▼                    ▼
  Valgrind (XML)     sanitized run        addr2line batch
  memcheck report    (crash pass only)    (caller file:line resolution)
        └───────────────────┴────────────────────┘
                            ▼
                     DebugSessionTrace (JSON)
```

1. **Build** (`compiler.rs`): every translation unit is compiled with `-g -O0`
   plus GCC's machine-readable diagnostics (`sarif-stderr` first, with
   fallbacks for older GCC), linked with a small C runtime that intercepts
   `malloc/calloc/realloc/free` via `-Wl,--wrap=...` and appends one event
   line per operation to a dedicated trace file. A second, sanitizer-built
   binary (ASan, plus UBSan when installed) is produced for the crash
   diagnostic pass; if a sanitizer runtime is missing the engine degrades
   and records a note.
2. **Execute** (`gdb_mi.rs`, `trace.rs`): the debug binary runs under GDB in
   MI2 mode with a token-matched command/response layer and a dedicated pty
   for program output. The engine breaks at `main`, instruction-steps in user
   code (source-steps in reduced mode), and `finish`es out of non-user frames.
   At source checkpoints it captures call stack, variables (own address, pointed-to address,
   raw bytes), live heap block contents read through GDB within the
   per-snapshot memory budget, and the program output so far.
3. **Heap reconstruction** (`allocation_tracker.rs`): the event file is
   ingested into a timeline that reconstructs every allocation, its caller,
   realloc chains and its final status; `addr2line` resolves caller PCs to
   `file:line` in one batched call.
4. **Independent analyses**: Valgrind runs the program separately (its XML
   parsed with `roxmltree`, findings correlated with allocation ids by
   file/line). The sanitized binary runs as a diagnostic pass when the
   configured trigger fires — by default on a crash *or* when Valgrind
   reported memory errors (`--sanitizer-trigger`; V2.1). Native sanitizer
   findings are retained; a crash summary is attached only when source-site
   evidence supports correlation.

Partial results are always preserved: a timeout, step limit or crash yields
a trace with everything captured up to that point.

Traces emit `schema_version: 2` and `stage_results` to distinguish completed,
failed, timed-out, and skipped analyses. Older trace JSON without these fields
remains deserializable; absent stage metadata is unknown, not a clean result.

### V2.1: what changed and why

The V2 audit measured per-instruction CPU register collection at ~90 % of
debugger command time. Since register visualization is not part of the GUI
plan, V2.1 removes the collection itself (not just the JSON fields):
instruction addresses always came from GDB stop frames, so dual-level
source+assembly tracing, snapshots, heap tracking and crash handling are
unchanged, while dual-mode wall time improved 2.0×–3.2× on the audit
benchmarks (e.g. the finite-loop workload: 19.11 s → 6.10 s). Details and
evidence: [`docs/v2.1/`](docs/v2.1/summary.md).

## Resource limits

| Flag | Default | Enforced by |
|------|---------|-------------|
| `--max-steps` | 1000 | engine step loop |
| `--timeout` (s) | 10 | monotonic whole-session deadline, including compilation and analysis |
| `--max-memory-mb` | 256 | inferior `prlimit` after the initial stop; delegated cgroup when available |
| `--max-output-bytes` | 1048576 | program output head capture; terminate on exhaustion |
| `--max-heap-allocations` | 10000 | timeline |
| `--max-snapshots` | 1000 | snapshot list |
| `--max-snapshot-memory-bytes` | 1048576 | variables + heap reads per snapshot |
| `--max-memory-read-bytes` | 4096 | one `-data-read-memory-bytes` call |
| `--max-processes` | 16 | cgroup v2 `pids.max` (best effort) |
| `--max-variables` | 24 | per-snapshot introspection |

**These limits are not a security sandbox.** They protect the *engine and
the user session* from runaway programs; a hostile binary can still do harm
on the host machine. Real isolation needs a container or VM.

Platform notes: Linux `RLIMIT_NPROC` is per real user id, not per process
tree, so it is deliberately not used (it would break GDB's own spawning on
a normal desktop session). Process-tree caps work only when a delegated
cgroup v2 subtree is available; otherwise the engine keeps rlimits for CPU
and file size and says so in `analysis_notes`.

## CLI > config file > defaults

Scalar flags override the JSON config file; list flags (`--define`,
`--compiler-flag`, …) **append** to the file's lists. See
[`docs/build-config.md`](docs/build-config.md) for the exact field mapping
(the file is the same shape the future GUI will edit).

## Limitations & known deviations

* `--wrap` instrumentation covers the program's own allocator calls; memory
  allocated *inside* libc/other shared libraries is invisible to the
  timeline (Valgrind still sees it all — it is the independent check).
* A freed block's raw bytes are the **last bytes captured while it was
  alive**; freed memory contents are not read after the fact.
* The sanitized pass is ASan-only when libubsan is not installed (the
  engine notes this in `analysis_notes`). Since V2.1 the pass is triggered
  by `sanitizer_trigger` (default: crash or Valgrind memory errors); ASan
  need not reproduce every Valgrind finding.
* Per-instruction CPU register values are **not collected** since V2.1
  (audit: ~90 % of command time, no GUI consumer); legacy V2 traces
  containing register payloads still deserialize.
* Program output and GDB console chatter are kept separate (pty); the
  MI-facing `@` target stream is treated as program output for GDB versions
  without tty support.
* Dual mode captures rich checkpoints at source transitions; all recorded
  instruction attempts remain ordered, including repeated source lines.
* Multi-thread/fork instruction interleavings, JIT/self-modifying code and hard
  engine-RSS limits are not implemented. See [explicit limitations](docs/v2.md).

## Generated examples

[`examples/v21_manifest.json`](examples/v21_manifest.json) records the V2.1
examples' actual outcomes, counts, sizes and register-capture evidence
(`register_time_us == 0`, no `registers` key in any event):

- [`v21_normal.json`](examples/v21_normal.json): normal source-only capture.
- [`v21_loop.json`](examples/v21_loop.json): finite loop, dual level.
- [`v21_heap.json`](examples/v21_heap.json): heap allocation/free, pointers, dual level.
- [`v21_crash.json`](examples/v21_crash.json): null write and faulting instruction.
- [`v21_dual.json`](examples/v21_dual.json): synchronized C/assembly and heap.
- [`v21_bounded_infinite.json`](examples/v21_bounded_infinite.json): deadline and partial results.

The superseded V2 examples (`v2_success.json`, `v2_null_pointer.json`,
`v2_bounded_infinite.json`, `v2_dual.json` with their
[`v2_manifest.json`](examples/v2_manifest.json)) are kept for historical
before/after comparison; they contain register payloads that V2.1 no longer
collects.

## Development

```sh
cargo test --lib      # unit tests; one live GDB lifecycle regression skips if absent
cargo test --test integration   # full pipeline against real gcc/gdb/valgrind
cargo test --test v2 -- --test-threads=1  # dual-level and reliability regressions
python3 scripts/benchmark.py            # build first; three serial samples per case
```

The integration suite skips gracefully (with a printed reason) when a tool
is missing. Layout: `src/` modules mirror the pipeline stages; `c_runtime/`
holds the embedded C tracking runtime; `fixtures/c/` one deliberate bug per
file for manual and automated testing; `docs/v2.1/` holds the V2.1 update
reports (register dependency audit, schema, fixes F-01…F-06, correctness,
performance, tests, summary).

License: MIT.

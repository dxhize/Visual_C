# Visual C V2.2 — Phase 8C Reliability & Stress Testing

**Verdict: bounded stress pass executed, but the phase's completion criterion
is NOT met.** Audit F13 is reproducible: a 2,048-byte `max_trace_bytes`
setting bounds the JSONL journal, **not** the final exported JSON (22,959
bytes in one controlled run). No safe bounded-final-export design was made
here. Until such a policy is specified and implemented, do not describe this
setting, or `max_trace_memory_bytes`, as a hard bound on final JSON size or
engine RSS. This is a documented P2 operational resource gap, not evidence
that records were fabricated. No Phase 8D or Trace Parser upgrade began.

## Prerequisite and boundaries

Read the Phase 8A audit, 8R-1/2/3 reports, Phase 8B audit and supervisor/
resource-limit implementations. The final Phase 8B serial suite recorded
338/338 passing tests. Before editing, reran the real native/GDB/raw-log
adversarial test, the B01 historical-pointer regression (full module-qualified
test name), and supervisor cancellation test; all passed. Existing worktree
edits were preserved. This phase adds bounded probes and focused test changes,
not a new tracing architecture. It does **not** claim hostile-code isolation:
cgroup v2 was not delegated on this host, inferior `prlimit` begins after its
initial stop, `RLIMIT_NPROC` is deliberately not used, and independent
Valgrind/sanitizer processes are not constrained by the inferior address-space
limit.

## Stress matrix: actual enforcement, not configured names

Release CLI probes were run serially with an external subprocess watchdog,
source-mode capture, `--sanitizer-trigger never`, explicit time/step/capture
budgets and output under `/tmp/cte-8c-stress/`. Except where specified, these
are single runs; repeated cases below are distinguished explicitly. All
statuses are observations of this host, not assertions of real-time timing.

| Probe (fixture and bounds) | Observed outcome and coverage | Wall / user CPU / peak RSS | GDB commands; trace bytes |
|---|---|---|---|
| Finite loop, 8 steps, 25 s (`finite_loop.c`, twice) | `step_limit_reached` both; 9 snapshots; `max_steps`; incomplete recording | 3.18 / 2.53 s / 152664 KiB; 3.11 / 2.47 s / 152612 KiB | 65 each; 43815/43816 B |
| Infinite loop, 8 steps, 25 s vs 200,000 steps, 4 s (`infinite_loop.c`) | 8-step case `step_limit_reached` (58 commands); 4 s case `timeout` (3586 and 3545 commands on two runs), with partial snapshots and `overall_session_deadline`; all incomplete | 4 s timeout by configuration; no external watchdog triggered | 9 snapshots at step limit; 715/707 at timeout |
| Blocked syscall, 25 s pipeline (`phase8b_blocked.c`) | Phase 8B live GDB regression: accepted step, 10 s stop-wait expiry → `timeout`, not debugger failure. Native execution finishes in 14 s. | Focused test in full suite | Partial trace, stop-wait reason |
| GDB command cap 12 (`finite_loop.c`) | `step_limit_reached`, `max_gdb_commands`, no falsely complete trace | 2.29 / 1.78 s / 152644 KiB | **12**; 19228 B |
| Heap cap 8 blocks, 200 steps (`many_allocs.c`) | `step_limit_reached`, both `max_steps` and `max_heap_allocations`; 32 retained event slots (Phase 5 event retention is 4× block cap), `heap_history.complete=false` | 4.08 / 3.13 s / 152644 KiB | 1280; 650562 B |
| Shared checkpoint budget 8 B, single read 4 B (`runtime_memory_aggregates.c`) | Program completed; `max_snapshot_memory_bytes` and `max_memory_read_bytes` reasons, incomplete object evidence (not invented bytes) | 12.16 / 10.17 s / 152780 KiB | 539; 606987 B |
| Program output 256 B (`big_output.c`) | `output_limit_reached`, `max_output_bytes`; incomplete capture | 6.41 / 5.76 s / 152768 KiB | 67; 43090 B |
| 64 B exact-fit output (Phase 8B fixture) | Six release runs: completed, exactly 64 bytes, never marked truncated | Bounded individual runs | No false output cap |
| Fast 4,096 B terminal write (`phase8c_fast_output.c`) | Independent native `wc -c` = 4096; four serial release runs completed, reported exactly 4096 bytes, `program_output_truncated=false`, exit 0. No PTY-tail loss reproduced; not a proof it cannot occur. | 13.53, 10.92, 7.77, 6.94 s (host/tool variance) | 4096 observed bytes each |
| Missing Clang with AST enabled (`phase8b_macro.c`) | Program completed, source unit `unavailable`, `ast_available=false`; independent runtime heap history remained complete | 3.14 / 2.54 s / 152740 KiB | 88; 64991 B |
| Five rapid separate `hello.c` sessions | All completed, 4 checkpoints, 27 GDB commands, no surviving GDB/engine PID in post-run `/proc` scan | Wall 3.17–3.31 s, user CPU 2.58–2.62 s, peak RSS 152664–152772 KiB | JSON 24668–24670 B |
| Deep recursion >200 frames (`runtime_memory_deep.c`) | Existing real test completes with frame list ≤200 and explicitly loses/reestablishes activation identity; no assertion that truncated caller frames were seen | Serial integration in full suite | Existing bounded frame and byte tests |
| 2 KiB journal cap (`hello.c`) | `step_limit_reached`, `max_trace_bytes`, coverage incomplete; journal 1939 B, final JSON 22959 B. An independent new **dual-mode** regression confirms the same scope gap. | 3.29 / 2.62 s / 152776 KiB | 20; 22959 B |
| Large snapshot-heavy trace (`runtime_memory_deep.c`, prior 8B release measurement) | Source mode reached 1000-snapshot cap, partial history; 25,081,617 B JSON | 24.43 s / peak 152604 KiB | 6009; 1000 snapshots |

A 3-second deadline on a finite loop completed normally, so it is **not**
reported as a timeout. A `spin_same_line.c` case with 8 configured steps
instead hit the legitimate 10-second blocked-stop wait before eight stops:
`timeout`/`debugger_stop_wait_timeout`, *not* a step-limit race. These
counterexamples distinguish actual stopping order from configuration intent.

## Lifecycle, injected failures and cleanup

- Four sequential `trace_program(hello.c)` calls in one process completed
  and dropped their traces; measured test-process VmRSS after each call was
  **11,728 / 11,848 / 11,880 / 11,892 KiB** on one run and
  **11,896 / 11,932 / 11,952 / 11,960 KiB** on another: +164 / +64 KiB
  from the first warmed sample, not a sustained multi-MiB slope. A 16 MiB
  warm-sample bound is asserted in `tests/stress_8c.rs`. This does not prove
  leak freedom over hours or across thread counts.
- A fake GDB passes `--version` then immediately exits 7 on MI launch.
  CLI returned JSON with `execution_status=debugger_failure`, debugger
  stage `failed`, no fabricated snapshots and incomplete coverage. This is
  also a permanent bounded test. Missing GDB at probe time and missing GCC
  produced operational exit 1/no trace rather than false observations.
- A supervised fake GCC passes `--version`, then executes `sleep 10` with a
  PID marker. A 3-second **whole-session** timeout yielded JSON
  `execution_status=timeout`, `compiled_successfully=false`, compiler stage
  `timed_out`; external 14-second watchdog did not fire and marker PID no
  longer existed. A fake Clang similarly slept during AST extraction: at
  a five-second deadline the AST unit was `unavailable`, summary
  `ast_available=false`, the outcome was timeout and marker PID was gone.
  These are external fault injections, not engine-produced tool outputs.
- Missing Valgrind produced a skipped stage with no fabricated timestamps;
  missing Clang remained optional. An existing journal file was not
  overwritten (4 bytes remained); the engine returned a partial trace with
  `execution_failure`, debugger stage failed and incomplete coverage. The
  independent `--output` path is not claimed to be atomic or immune to disk
  failure.
- Existing serial regressions exercise fork under tracker lock, repeated
  fork/child PID separation, multithreaded allocations, partial/EINTR/EIO
  event writes, missing terminal ACK, truncated/replaced/oversized event
  files (1 MiB per-stop read cap), truncated MI2 records, malformed Clang
  output and the bounded DWARF extractor. No unbounded fork or disk-fill
  workload was launched in this stress pass.
- Existing SIGINT test verifies partial journal retention and process-tree
  reaping during GDB execution; supervisor tests verify timed-out children
  and inherited pipe holders. Compilation/Clang *deadlines* were injected
  above; forced SIGINT precisely during those phases and a timed-out GDB
  memory-read command were **not** independently reproduced here.

## Confirmed issues, repair evidence and limits

**R05 test-oracle race (P2 test reliability, not proven engine defect):**
Phase 8A's `integration::step_limit_bounds_execution` combined 50 debugger
steps with a five-second **whole-pipeline** deadline, so timeout could win on
a busy host. The new `stress_8c` test uses the same infinite-loop input with
well-separated 8-step/25-second and 200,000-step/5-second configurations,
repeated twice; observed statuses and limit reasons agree with the causal
budgets. The existing 50-step assertion remains intact, but its session
budget is now 20 seconds, so compiler/GDB startup cannot routinely preempt
it. Focused and full serial tests passed. This repairs a test's unsound
timing premise, not a GDB protocol issue.

**F13 final-export bound (P2, CONFIRMED, UNRESOLVED):** the 2 KiB journal
limit stops further checkpoints and records incomplete coverage; it does
**not** limit final JSON or the memory occupied by the full assembled
`DebugSessionTrace`. `src/recorder.rs::reserve` counts journaled retained
records; `src/main.rs` serializes to optional file and stdout without an
independent final byte limit. Test `journal_budget_stops_capture_but_does_not_bound_final_json`
reproduces the gap under *dual* mode, so incomplete coverage is a real
limit response, not merely the default source-mode coverage bit. A safe
final-export policy requires a decision about output refusal versus a small
self-describing partial document, treatment of both destinations, atomic
file writes, schema compatibility, and budget accounting for static
registries. No silent truncation or uncontrolled rewrite was attempted.
Until repaired, **Phase 8C's no-resource-limit-bypass completion criterion
is not satisfied**. `max_trace_bytes` is a hard *journal* cap, not a final
JSON cap; inferior memory/process limits remain best effort where cgroups or
`prlimit` are unavailable.

**No new confirmed P0/P1 lifecycle defect** emerged from these bounded
probes. Four fast-output samples cannot rule out the earlier suspected PTY
terminal-drain race. The subprocess watcher found no retained GDB/engine
processes after repeated sessions; it cannot establish global absence of
unrelated orphan processes. Tool restarts, process creation under cgroup
pressure, malicious binaries, and sustained host-level memory consumption
remain outside these measurements.

## Serialization and verification

`examples/phase8c_serialize_bench.rs` loads an existing 25,081,617-byte
trace and streams it to a counting writer three times, avoiding a second
in-memory JSON string. Timings: **30.720 / 43.508 / 30.959 ms**, process
VmRSS 46,424 KiB during those runs. A 22,959-byte limited trace took
0.059 / 0.046 / 0.040 ms, VmRSS 3,256 KiB. These figures measure Rust
serialization to a sink, **not** filesystem latency, stdout backpressure,
engine peak RSS, nor the cost of two exports in `main`.

Required commands passed after the final test edits:
`cargo check --all-targets`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and
`cargo test --no-fail-fast -- --test-threads=1`: **342 passed, 0 failed**
across 18 targets. Test log `/tmp/visual-c-8c-tests-final.log`; sum of target
wall times ~540 seconds (not simultaneous CPU time). This includes four
Phase 8C tests in `tests/stress_8c.rs`, prior Phase 8B correctness tests and
legacy/Trace Parser suites. No schema field, Trace Parser ownership/index,
allocator identity, optional AST setting or resource-limit default changed.

## Phase 8D recommendations (not implemented)

1. Specify an independent bounded final-output and full-trace memory policy
   for F13, with atomic destination handling and tests for tiny budgets,
   oversized registries and stdout/file failures. This is the explicit
   completion blocker before advertising final-export guarantees.
2. Add fault-injected MI memory-read hangs, same-PC debugger comparisons,
   rapid terminal PTY bursts under controlled reader scheduling, and SIGINT
   during compiler/Clang stages. Keep process-group/descendant watchdogs.
3. Repeat cold/warm sessions and large traces for many hours on an isolated
   host with system-wide RSS, file descriptors, disk and cgroup accounting.
   Recheck F14's unused cross-run Valgrind-ID helper before exposing it.

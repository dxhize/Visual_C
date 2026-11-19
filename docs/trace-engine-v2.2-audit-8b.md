# Visual C V2.2 — Phase 8B Adversarial Correctness Audit

Scope: the current **working tree**, including the uncommitted Phase 8R-1,
8R-2 and 8R-3 repairs. No worktree changes were reset. This is a bounded,
reproducible adversarial pass, **not** proof of all C programs or platforms.
No Phase 8C/8D or Trace Parser upgrade was begun.

## Prerequisite and method

Read the Phase 8A audit, the three repair reports, Phase 5's heap-evidence
contract and executable evidence tests. Before changes, targeted F01, F02,
F03, F04/F05, F06/F07 and F10 regressions passed: numeric containment,
optimization preprocessing, qualified DWARF file identity, nine C wrapper
transport/fork cases, terminal ACK, fatal-stop globals/cap and corrupted
cross-reference injection. The reports have no pending implementation
sections. The final serial suite also exercises them. No prerequisite blocker
was found.

For deterministic runtime values, compile and execute the C fixture outside
the engine, obtain a *separate* GDB breakpoint observation (also repeated
through an external MI2 `-break-insert` / `-exec-run` /
`-data-evaluate-expression` session), and compare JSON with these oracles. For heap operations, independently link the same fixture
against the C wrapper and inspect raw PID/sequence/size/status rows and its
terminal ACK. Addresses across independent processes are **not** compared:
ASLR, allocator reuse and debugger timing invalidate such comparisons. For
faults/undefined behavior, compare only observed signal/status and coverage,
not supposed deterministic contents. For corruption tests, inject known
impossible relationships rather than treating JSON shape/counts as proof.

## Scenario matrix

| Subsystem | Adversarial invariant and independent oracle | Executed evidence |
|---|---|---|
| Execution/stack | Defined `phase8b_defined.c`: native output fixes 41/7/23/10/88; separate GDB `break checkpoint`, `p *alias`, `p heap[2]`, `p sum`, `p oracle_global`, `up; p packet.items[1]` yields 7/23/10/41/7. Runtime checkpoint and caller-frame observations agree. Recursive `triangular(4)` contributes 10; packet contains nested aggregate. | New `tests/adversarial_8b.rs` (1 end-to-end test); existing `v2` shadowing/recursive activation tests. GDB initial prologue stops are not asserted initialized. |
| Heap/pointers | Independent wrapped execution yields malloc(12), realloc(16), free(16), sequences 1–3 and ACK `3 0`; engine records the same operation order/sizes, distinct realloc lifetimes and complete history. At the `checkpoint` before free, the heap pointer must still be **active**; alias is a stack address, not an inferred allocation origin. | New end-to-end test; 32 fixed-seed × 128-operation live-address toggle sequences compare timeline bytes, active IDs and reuse against an independent in-test state machine (4,096 operations). Synthetic terminal free/reuse test guards historical pointer state. |
| DWARF/layout | GCC `-g -O0` plus `readelf --debug-dump=info` of `dwarf_types.c` independently shows struct member offsets 0/4/8, bit sizes 3/5, and pointer DIEs. Cross-CU, duplicate basename and type registry tests retained. | `tests/dwarf_registry.rs` (12), `dwarf_feasibility.rs` (1), existing runtime memory tests; this pass inspected rather than exhaustively asserting all bitfield bytes. |
| Clang/fusion | GCC `-E -P` independently expands `free(first); DISPOSE(second)` to **two** calls on line 7. Clang AST has one direct and one macro-expanded `free`; one event return PC cannot identify either. | New `phase8b_macro.c` real GCC/Clang/GDB test plus extended synthetic ambiguity test; existing real multi-TU/optimization-sensitive and corrupted-reference tests. |
| Completeness/output | Native exact-four-writes fixture emits 64 bytes. Exact-fit capture is not truncation; 65th byte is. Fatal snapshots with absent global or memory cap remain incomplete. | New exact-fit unit and real GCC/GDB test; six serial release exact-fit probes returned completed/64/not-truncated; existing crash/global/limit/interrupt tests. |
| Stop wait/failure | Native blocked fixture exits 0 after 14 seconds, independent proof the inferior is not a failed MI peer. Engine's per-stop 10-second wait expires before its 25-second whole-session deadline. | New live GCC/GDB test and classification correction. Missing Clang, malformed AST, `_exit`/writer ACK and GDB errors covered by existing tests. |
| Parser/serialization | Deterministic seed `0x8b2026`, 256 single-byte MI2 mutations + every truncated prefix of a valid stopped record; no parser panic. Existing bounded journal/truncated log, legacy JSON, missing-ID and contradictory-reference tests. | New MI2 unit plus TP-1/2/3 and Phase 8R-3 suites in the serial run. This is bounded mutation testing, not a full grammar fuzzer. |
| Supervisor/cancellation | Independent shell marker is created *after* `echo observed` succeeds; a cancellation must preserve those already-written bytes and reap the process tree. | A formerly failing test was repaired; 12 successive focused passes and a clean full serial run. |

New fixtures: four (`phase8b_defined.c`, `phase8b_macro.c`,
`phase8b_exact_output.c`, `phase8b_blocked.c`). New test functions: eight
(one native/GDB/raw-log end-to-end, two live V2, one real AST, one heap state
machine, one MI mutation, one exact-fit buffer, one historical pointer
reconciliation); additional existing tests
were strengthened for mixed macro ambiguity, Clang availability and
synchronized cancellation. The sanitizer integration test was given a
15-second whole-pipeline deadline so its required independent pass can start
on a loaded host; its sanitizer assertion was not weakened. Counting one
fixture or randomized seed as one scenario would understate coverage; counting
each generated operation as an independent test would overstate it. The final
serial suite ran **338 Rust unit/integration/doc tests** across 17 targets.

## Confirmed defects and minimal repairs

| Finding | Before: reproducible failing evidence | Repair and after |
|---|---|---|
| F08, P2: accepted step then blocked `*stopped` wait mislabeled `DebuggerFailure` | `cargo test --test v2 blocked_inferior_stop_wait_is_timeout_not_debugger_failure -- --exact`: `DebuggerFailure` vs expected `Timeout`; native fixture completes in 14.00 s. | `src/trace.rs` distinguishes `GdbCommandTimeout.command == "waiting for *stopped"` (inferior wait → timeout) from a missing MI command reply (debugger failure), including `exec-finish`. Preserve partial snapshots and explicit stop-wait reason. Focused test passed. |
| F09, P2: direct call incorrectly declared unique alongside macro call | Extended `k_l_heap_candidates_and_ambiguities_are_explicit_and_bounded` failed: two candidates vs expected one. Independent GCC expansion and real Clang AST confirmed two same-line calls. | `src/clang_ast.rs` examines both compatible buckets before emitting a unique candidate. Mixed lines emit bounded ambiguity with both IDs; macro-only lines remain explicit. Synthetic and new real GCC/GDB test pass. |
| F11, P2: exact-fit output falsely truncated | `gdb_mi::tests::exact_fit_output_is_not_truncated_until_one_more_byte_arrives` failed at the exact three-byte boundary. | `ProgramBuffer::push` compares **incoming bytes to remaining capacity**, not final length to cap. Tracks total bytes independently, handles cap 0; real 64-byte fixture and six CLI probes completed without false output limit. |
| F12, P2: unavailable Clang reported as AST available | Existing missing-Clang test strengthened with availability assertion and failed against a one-unit `unavailable` registry. | `evidence::fuse` sets summary availability only if some TU has usable `complete`/`partial` status. Empty/all-failed false, mixed partial true, disabled false; per-unit errors untouched. The old assertion that an empty registry was available was corrected because it asserted the audited defect. |
| S01, P2: cancellation can lose already-written subprocess output | Full serial suite failed `supervisor::tests::cancellation_wakes_stalled_process_wait_and_retains_partial_output` twice across Phase 8R-3/8B. Inspection showed `output()` dropped its pending stdout/stderr drain futures immediately after killing the child. A fixed 100 ms sleep also did not prove shell output preceded cancellation. | `src/supervisor.rs` gives terminated pipes a bounded 100 ms drain each before drop; test now waits for a shell-created marker written *after* stdout and only then cancels. Twelve focused passes plus clean serial suite. This does not claim bytes a child had not written before cancellation. |
| B01, P1: terminal heap replay rewrites *past* pointer evidence | New independent raw-log fixture shows the free has sequence 3 while `checkpoint` recorded only 2 events. Before repair `tests/adversarial_8b.rs` failed: the checkpoint's `heap` pointer said `active=false` despite the separate native/GDB oracle proving it was live. `backfill_snapshots` reapplied `pointer_target` against the **final** timeline, including a later free/reuse. | `src/memory.rs::refresh_heap_targets` now preserves the original stopped-state target, only widens local IDs if later children require PID-qualified serialization, and downgrades certainty on terminal transport loss. It never re-matches an old address to a future lifetime. Real GCC/GDB/raw-log test passes with `active=true`; new synthetic test checks terminal free *and subsequent same-address reallocation* retain the historical candidate ID/status. Phase 8R-2 terminal-failure JSON downgrade and heap-provenance suites still pass. |

B01 is a **confirmed P1** newly exposed by independent event ordering, not
an unaddressed critical issue. The other findings are P2. All repaired paths
were rerun with related existing suites. Sequence-candidate matches are still *not exact execution*
and scope/readable bytes still do *not* prove initialization.

## Incorrect/unsupported claims and remaining risks

- A snapshot at a function's very early prologue can show stale argument
  storage before the ABI prologue has established its final DWARF location.
  The defined fixture's `checkpoint` snapshots at the later GDB breakpoint
  agree with ground truth; this pass does **not** treat a pre-prologue value as
  initialized C state. Further same-PC/PC-range GDB comparisons are needed.
- Audit F13 (journal budget does not cap final JSON/RSS) remains an
  **unresolved P2 operational limit**, not a demonstrated false execution
  value. A source-only deep-memory benchmark still produced 25,081,615 JSON
  bytes under ordinary limits; do not advertise journal caps as a hard final
  trace cap. A bounded export policy requires separate design/testing.
- Audit F14's `valgrind::correlate_with_allocations` can attach primary-run
  IDs by weak same-line/basename matching. Search of production call sites
  found **none** (only its unit test calls it); actual Valgrind diagnostics do
  not currently acquire these cross-run IDs from the orchestrator. Treat the
  public helper as unsafe for lifetime identity; do not claim it validates
  GDB-run provenance. A future caller needs site-only evidence fields or a
  strict gate; do not activate it unchanged.
- Audit R01 (shadowed tags/partially retained CU identities) is not disproved
  by valid-reference existence checks. R02 (PTY shutdown tail loss) was not
  reproduced in six exact-output CLI runs but is not ruled out. Forked children
  without event/ACK remain undiscoverable. Non-Linux targets and arbitrary
  user allocator implementations remain unverified. F14/F13 and these risks
  are **not** classified as fixed by this report.
- Raw event addresses or independent native GDB pointers cannot be compared
  numerically across executions; static type/member identities are not
  runtime assignment provenance. Undefined-behavior fixtures are tested for
  reported signal/diagnostic and partiality, never assumed values.

## Verification and measured cost

All required commands passed on the final worktree:
`cargo check --all-targets`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and
`cargo test --no-fail-fast -- --test-threads=1` (**338 passed, 0 failed**;
`/tmp/visual-c-8b-tests-final3.log`, sum of reported target durations ~468 s).
An earlier full run failed the supervisor unit described above; another
post-B01 run failed `integration::crash_is_reported_with_sanitizer_pass` because
its five-second whole-pipeline deadline elapsed before the sanitizer pass.
The test now uses 15 seconds, still requires the sanitizer report, and passed
both focused and final full runs. Neither failure was dismissed as flaky. The repaired suite and Phase 8R-1/2/3 targeted tests passed.

Single-run serial release observations (time via `/usr/bin/time`, peak RSS
for engine process tree as reported there; trace bytes on disk), stored in
`/tmp/cte-8b-bench-final/` (after B01):

| Workload | Status | Time | Peak RSS | GDB commands | Checkpoints / events | JSON bytes |
|---|---|---:|---:|---:|---:|---:|
| defined nested-call/realloc | completed | 4.43 s | 152704 KiB | 451 | 41 / 3 | 292434 |
| mixed direct/macro | completed | 6.67 s | 152700 KiB | 88 | 10 / 4 | 77315 |
| faulting global | crashed | 3.97 s | 152532 KiB | 41 | 3 / 0 | 27323 |
| deep memory/step cap | step_limit_reached | 24.43 s | 152604 KiB | 6009 | 1000 / 0 | 25081617 |

The deep case legitimately reaches the snapshot cap and has incomplete heap
history; a crash also lacks a normal terminal allocator ACK. These are
single samples, not speedup claims. The independent native defined fixture
printed `oracle global=41 alias=7 heap2=23 sum=10 result=88` and separate
GDB CLI printed 7/23/10/41/7; an independently started MI2 session stopped
at `checkpoint` with `sum="10"` and returned `value="41"` for
`oracle_global` and `value="7"` for `*alias`. Raw wrapper rows matched engine
event kinds and sizes but, correctly, not ASLR-dependent addresses.

## Phase 8C handoff (not implemented)

1. Stress independent same-PC GDB and engine observations at function entry,
   at recursion depths, and across lexical scope transitions. In particular,
   distinguish pre-prologue register/stack locations from initialized values.
2. Test PTY terminal drain with fast bulk writes, inherited FDs and
   controlled reader scheduling; track original stdout bytes independently.
3. Establish and test a final JSON/output-file budget distinct from journal
   memory/disk limits (F13); bound time/memory without silently losing trace
   records.
4. Replace or quarantine the unused cross-run Valgrind ID helper (F14)
   before it enters production. Independently challenge cross-CU shadowed
   records, bitfields/VLAs, C library ABI differences and deliberately broken
   GDB/Clang tool outputs.
5. Make time-vs-step limit tests deterministic by controlling the active
   deadline and stop count, not by assuming a fast host. No stress campaign,
   broad redesign or Trace Parser upgrade occurred in this phase.

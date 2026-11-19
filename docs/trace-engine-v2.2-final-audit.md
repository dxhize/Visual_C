# Visual C V2.2 — Phase 8D Final Release Audit

## 1. Executive assessment and verdict

**B — Conditional. Do not yet approve the dedicated Trace Parser upgrade.**
No *currently exercised engine path* was found fabricating a runtime value,
allocation origin, or completed snapshot in the bounded independent checks;
all previously confirmed P1 defects have fixes and passing permanent
regressions. The serial suite is clean: **350 passed, zero failed, zero
ignored**, 18 targets. However, **F14 remains an open confirmed P2**:
`valgrind::correlate_with_allocations` is public, accepts unrelated
same-basename files and assigns primary-run allocation IDs to separate-run
Valgrind errors. It is **not called by the engine's production pipeline**;
the hazard is to its public API/future activation, not a demonstrated wrong
ID in present CLI output. It lacks a wrong-basename/same-line permanent
regression. Phase 8A's confirmed documentation overclaim R04 also remains in
the older Phase 7 report. The release criterion requires *every previously
confirmed defect* to have verified closure; passing tests cannot close F14.
Quarantine/remove the helper or replace its identity claim with site-only
uncertainty, add a failing-before/passing-after collision test, formally
scope R03's undiscoverable fork-child coverage (or add a child census), and
correct Phase 7's misleading example/test-coverage wording before approval. These
are targeted changes, **not** a parser redesign. No audit-time production code
or dependencies were changed; this report is the only new audit artifact.

Scope is the dirty working tree (including uncommitted Phase 8R/8B/8C/F13
work); nothing was reset. Read the 8A/8B/8C audits, 8R-1/2/3 and F13 repair
reports, Phase 7's evidence contract and extraction-gap ledger, and checked
claimed fixes against current code, test names and release artifacts. Earlier
reports provide *historical before-failure evidence*, not a substitute for
the current independent checks below. Host: GCC 16.2.1, GDB 18.1, Clang
22.1.8, Valgrind 3.27.1; Linux/glibc only.

## 2. Complete confirmed-defect closure register

“Before” refers to a focused historical failure documented in the cited
repair; 8D ran the current focused paths and/or full serial suite, **not**
a pre-repair binary for every finding. “Closed” means bounded trigger and
related integration pass, not a proof for every compiler or arbitrary C.

| ID / priority | Current code, reproducible regression and integration evidence | Status |
|---|---|---|
| F01 P1: address candidate falsely promoted to pointer origin | `memory::pointer_target` assigns `address_match_allocation_id`, **not** `allocation_id`; `unique_live_numeric_containment_is_not_pointer_provenance`, current `heap_provenance` and independent C/GDB fixture pass. Historical before: promoted ID 7. | **Closed** |
| F02 P1: GCC/Clang optimization preprocessing mismatch | `clang_ast::clang_flags` preserves compatible `-O` order; unverifiable `-f/-m/-pthread` sets `cross_provider_correlation_unavailable` and suppresses cross-links. `gcc_optimization_preprocessing_matches_clang_ast_branch`, `accepted_compiler_specific_flags_retain_ast_without_cross_provider_identity` pass. Independently ran GCC and Clang `-O0 -O2 -dM`: **both** define `__OPTIMIZE__=1`. Unknown target/flag combinations remain unlinked, not claimed equal. | **Closed** for supported policy |
| F03 P1: same-basename DWARF static misassociation | `dwarf::ObservationTypes::enrich` requires equal absolute qualified paths and stops on ambiguous matches. `qualified_symbol_requires_full_file_identity`, `ambiguous_location_does_not_fall_back_to_a_symbol`, duplicate-`common.c` real GCC tests pass (12/12 registry tests in focused rerun). | **Closed** |
| F04 P1: fork under tracker lock deadlocks child | Child-only `pthread_atfork` scalar reset, deferred table clear in `alloc_tracker.c`; `fork_while_worker_holds_tracker_lock_cannot_deadlock_child` and test-only pre-repair switch confirm before/after; nine current C instrumentation tests pass. Does not cure foreign atfork/libc deadlocks. | **Closed** within stated fork contract |
| F05 P1: failed last event write looks complete | Independent terminal `(pid, seq, failures)` ACK, read/sequence verification and conservative downgrade in `allocation_tracker`/`trace`; C raw-log EIO/zero/partial/EINTR tests plus `heap_provenance` pass. `_exit`/missing ACK remains explicitly incomplete. | **Closed** for detectable transport loss |
| F06 P2: crash omits globals yet reports complete | Shared `capture_requested_globals` in normal/fatal `trace.rs`; live `crash_globals_and_snapshot_cap_do_not_claim_complete_coverage` and missing-global/budget tests pass; historical independent GDB observed global 123. | **Closed** |
| F07 P2: crash exactly at snapshot cap claims complete | Fatal omitted snapshot calls `recorder.truncate`; same live crash-cap test and `v2` suite pass. | **Closed** |
| F08 P2: inferior stop-wait mislabeled debugger failure | `trace.rs` distinguishes accepted-step `waiting for *stopped` timeout from missing MI reply. Live blocked-14-s fixture with 10-s stop wait and 25-s session budget passes as `Timeout` with partial evidence. | **Closed** |
| F09 P2: mixed direct/macro call falsely unique | `clang_ast` considers both compatible call buckets. `mixed_direct_and_macro_free_calls_are_not_claimed_unique`, `phase8b_macro.c`, and independent GCC expansion (two calls on one line) pass. No exact-call execution claim. | **Closed** |
| F10 P2: wrong but existing cross-source IDs pass deep validation | `trace_parser/validation.rs::validate_ast_registry` checks DWARF/declaration, unit, member parent and heap call/site consistency; `phase8r3_existing_targets_with_contradictory_relationships` and parser suites pass. Deep validation remains explicitly opt-in and cannot manufacture missing provenance. | **Closed** for recorded contradictions |
| F11 P2: exact-fit output falsely truncated | `ProgramBuffer::push` flags only discarded bytes; unit `exact_fit_output_is_not_truncated_until_one_more_byte_arrives` and native 64-byte fixture pass. | **Closed** |
| F12 P2: unavailable AST reported available | `evidence::fuse` requires at least one usable `complete`/`partial` TU; missing-Clang and AST-on/off regressions pass. | **Closed** |
| F13 P2: journal cap mistaken for final export cap | `ExecutionLimits.max_json_export_bytes` independent 128 MiB default; `json_export` counts UTF-8 formatting/newline with preflight, sibling temp + atomic rename, exact-fit/one-over/ENOSPC/cancel tests. 8D release CLI 2 KiB export rejected **exit 1, zero stdout, old 13-byte file intact**; 1 MB limit produced 22,735 complete file/stdout bytes with `max_trace_bytes` still stopping journal at 1,897 bytes. | **Closed for serialization/export**, not full trace RAM |
| F14 P2: cross-run Valgrind allocation-ID attribution | `src/valgrind.rs:380-417` still sets `related_allocation_ids` by line plus basename/`ends_with` matching, potentially across `/a/main.c` vs `/b/main.c` or distinct same-line lifetime events. Search finds **only its positive-path unit test calls it**, not `trace_program`; no negative collision regression. No correction is present. | **Open**, dormant public helper; conditional release blocker |
| B01 P1 (8B): terminal replay rewrites a *past* pointer target | `memory::refresh_heap_targets` changes only certainty/PID width; does not re-match using future free/reuse. Independent native/GDB/raw-log `adversarial_8b` passed 1/1; historical snapshot had two events and terminal free is event 3. `terminal_free_and_address_reuse_cannot_rewrite_checkpoint_pointer` passes. | **Closed** |
| S01 P2 (8B): cancelled subprocess loses already-written output | `supervisor::output` drains terminated pipes for bounded 100 ms; marker-synchronized cancellation regression and full suite pass. Cannot promise bytes never written or from uncooperative inherited FDs. | **Closed** within bounded drain |
| R03 P2 (8A): forked child inherits nonzero parent history | F04's PID-local restart gives observed children fresh seq/ID (raw tests); child with no event/ACK is **undiscoverable** and inherited block identity cannot be asserted. Explicitly documented in 8R-2; no global fork census. | **Partially resolved** (conservative coverage limitation) |
| R05 P2 (8A): step-limit test races five-second whole deadline | Existing 50-step test now has 20 s; `stress_8c` separately tests 8-step/25-s and 200,000-step/5-s competing limits twice. Current focused and serial runs pass; no false step count observed. On arbitrarily slow hosts a wall deadline can still win honestly. | **Closed** as test-oracle defect |
| R04 P3 (8A): Phase 7 documentation overstates independent proof | Original Phase 7 illustrative fragment and A–S wording remain as written; 8B/this audit supply independent checks but do not retroactively turn that example into literal fixture output. | **Open documentation correction** |

Additional **unconfirmed risks**, not reclassified as defects: R01 incomplete CU/
lexical-parent proof could cause a false static link if GCC omits relevant
DIEs; current shadowed/duplicate-file tests cover only their specific cases.
R02 possible final PTY-tail loss was **not reproduced** in the 8C four-run
4,096-byte comparison or 8B exact-output runs. No controlled PTY-read
scheduling test proves absence. These do not justify a fabricated P1 verdict.

## 3. Independent end-to-end ground truth

**8D direct external oracle** (not the engine's own GDB): compiled
`phase8b_defined.c` with independent `gcc -g -O0`; native stdout was
`oracle global=41 alias=7 heap2=23 sum=10 result=88`. Separate
`gdb --batch` breakpoint at `checkpoint` returned `$1..$5 =
7,23,10,41,7` for `*alias`, `heap[2]`, `sum`, `oracle_global`, caller
`packet.items[1]`. Independently wrapped C executable wrote raw PID-local
rows seq **1 malloc(12), 2 realloc(16), 3 free(16)**, with ACK `3 0`.
Engine release CLI `defined-0.json` has matching event kinds/sizes, distinct
malloc/realloc IDs (1/2), complete heap transport, and a checkpoint pointer
with `active=true` before the later free; `address_match_allocation_id=2`
but **`allocation_id=null`**. One earlier prologue checkpoint had an
`unmapped` target: it is not treated as initialized C pointer ground truth.
Raw addresses across separately launched processes differ (ASLR), so the
comparison tests relative offsets/identity within each execution and values,
not equality of numerical addresses across runs. The focused permanent
`adversarial_8b` test reran and passed, independently checking the same
native/GDB/raw-log invariants.

Independent `readelf --debug-dump=info` on current GCC `dwarf_types.c`
reported struct member offsets **0/4/8** and bit sizes **3/5**; the 12
registry tests assert member/array/union/bitfield, duplicate filename,
shadowed declaration and cross-CU origin behavior. `runtime_memory.rs`
asserts actual little/native-endian raw bytes for `pairs=11,12,13,14`,
`grid=1..6`, `bundle=21..26`, union overlays
`0x11223344/0x55667788`, VLA lengths **12 and 20**, different activation
IDs, and explicitly truncated 16,384/20,000-byte aggregate capture. These
are live GCC/GDB tests, not merely JSON parsing; 8D's separate three-run
`runtime_memory_aggregates` CLI benchmark uses default caller settings and
is **not** independently asserted to retain those caller values.

The serial integration paths additionally ran recursion (>200-frame
continuity loss and reestablishment), lexical shadowing, caller-frame
variables, pointer aliases/reuse/arithmetic, `malloc/calloc/realloc/free`,
segfault/ASan/Valgrind and nonzero exit, fatal globals and snapshot cap,
partial signal/timeout traces, multiple TUs and same-basename statics,
Clang-on/off fusion and wrong-existing-ID parser injections. Which assertions
were actually independent is distinguished: native C/GDB/readelf/raw rows
above are external; AST-on/off semantic-projection equality checks stability,
**not** truth of either output. The 3× paired `pointer/realloc/multiple_tu`
benchmark audited reference existence and counters and checked runtime/semantic
projection equality in all **nine** pairs; it does not independently prove
every declaration link or compare ASLR-dependent numeric addresses.

## 4. Architecture and evidence integrity

Compiler emits GCC debug artifacts separately from optional diagnostic builds;
GDB/MI2 owns stopped-state observations, token routing and bounded source
checkpoints. DWARF provides compiled type/layout/declaration facts, Clang
optional source syntax; fusion runs once after runtime capture, adding
optional links without rewriting GDB values, heap lifetimes or activation IDs.
The C wrapper appends raw PID-local events; Rust ingests a bounded verified
prefix and downgrades on missing ACK, corruption, truncation or replacement.
Snapshot bytes are stop-local (no cross-stop byte-cache reuse identified),
caller chain continuity is reported unknown on truncation, and scope does
**not** imply initialization. Cross-compiler incompatible flags suppress
cross-provider links. The static type/AST registries are not copied per
snapshot. Legacy schema-v2 fixtures load; final exporter leaves default-valued
new limit field absent for backward JSON compatibility. TP-1/2/3 borrowed
canonical store and deep validation tests passed; deep semantic validation
remains opt-in and cannot prove identities unsupported by evidence.

**Contradiction/risk:** the still-public F14 helper's comment calls its
site-match “deliberately conservative”; its `file_matches` accepts equal
basenames and its output field looks like actual allocation identity. This
contradicts Phase 7's cross-run identity contract if an API consumer invokes
it. Since no production caller exists, current Valgrind XML/CLI does not
inherit this false-ID edge. No other demonstrated cache invalidation or
persistently duplicated full-size JSON buffer was found in audited paths.
Phase 7's original claim that all preprocessing differences are documented
with “none” remaining is superseded by the later F02 conservative uncertainty
gate and this audit; do not use that old ledger as current proof.

## 5. Resource, lifecycle and hard-versus-best-effort guarantees

Current real release CLI: 2,048-byte **journal** cap triggered
`max_trace_bytes`, `step_limit_reached`, incomplete scope; 2,048-byte
**export** cap independently rejected file/stdout with nonzero exit and
unchanged existing file. At 1 MB export cap both complete JSON destinations
matched byte-for-byte (22,735 bytes), schema 2, no sibling temp. Unit tests
verify exact byte fit/one over for compact and pretty UTF-8 including newline,
file-preserving failed serialization/write/cancel and ENOSPC. Bounds checked
by current code: 1 MiB per-stop allocator-log read, 64 MiB DWARF input with
CU/DIE/deadline limits, Clang 8 MiB AST output/100,000 nodes/depth 96 and
per-TU limits, snapshot/read/event/journal limits. Serial suites include
oversized/replaced/truncated logs and missing ACK, 8C tested memory/read,
output, step and command caps and fake GDB/compiler/Clang timeouts, and
supervisor SIGINT cleanup. No leftover GDB/engine process was found in
post-run PID scans. 8C repeated-session RSS observations did not show a
multi-MiB per-session slope; this is not a sustained leak proof.

`max_json_export_bytes` bounds **serialization and bytes per document**, not
memory previously consumed assembling `DebugSessionTrace`; it also cannot
make stdout and file a single atomic transaction (file may be valid if stdout
later fails). SIGKILL/power loss may leave a temp; parent-directory fsync is
not guaranteed. Inferior `prlimit` is applied after the first stop, and
cgroup-v2 process/memory limits are best effort without delegation on this
host; Valgrind/GDB/sanitizer child processes are not given inferior address
space caps. No untrusted-code sandbox or arbitrary-platform/libc correctness
is claimed. Absent terminal ACK, unsupported Clang, runtime-dependent DWARF,
unknown initialization, and unobserved fork descendants remain explicitly
uncertain, not falsely complete. The 8C report's F13 “open” label describes
its **historical** state; the later F13 repair closes the scoped export gap.

## 6. Repeated release performance measurements

Three serial identical-config samples per *row*, real `--output` and discarded
stdout, `--no-pretty --timeout 35 --max-steps 350
--sanitizer-trigger never`; only `macro_ast` enables Clang. `wall` and RSS
are external `/usr/bin/time` (RSS includes traced child peaks); GDB duration,
checkpoint capture, DWARF/Clang and fusion are the engine's profiler/stage
counters, **not** independently timed subprocess traces. Table values are
medians, ms except wall seconds, RSS KiB and output bytes. All 15 ran without
watchdog/CLI failure. Raw runs: `/tmp/cte-8d-runs/`.

| Workload | Wall s | Peak RSS KiB | GDB commands / debugger ms | Capture ms | DWARF / Clang / fusion ms | JSON bytes |
|---|---:|---:|---:|---:|---:|---:|
| defined realloc/recursive | 4.10 | 152696 | 600 / 1658 | 179.45 | 1.185 / 0 / 0.019 | 381528 |
| arrays/structs/unions/VLA | 12.16 | 152704 | 1128 / 9887 | 8314.547 | 0.920 / 0 / 0.021 | 962239 |
| fatal global | 4.43 | 152628 | 35 / 1472 | 1.658 | 0.553 / 0 / 0.012 | 37004 |
| mixed macro/direct + AST | 5.54 | 152620 | 82 / 1689 | 34.22 | 1.020 / 314.944 / 0.240 | 81330 |
| multiple TUs | 4.05 | 152672 | 176 / 1217 | 67.619 | 0.884 / 0 / 0.020 | 134042 |

A separate three-repeat matched-config script (`scripts/phase7_benchmark.py`,
`/tmp/cte-8d-paired/`) toggled **only** AST. Across pointer/realloc/multiple
TU, medians of AST-off vs AST-on wall seconds: **3.462/3.798**, **3.184/3.505**,
**3.406/3.739**. Medians of *paired* wall differences:
**−0.075/0.391/0.333 s** respectively; pointer's negative noisy delta is
not a speedup. AST extraction medians 394/125/100 ms, fusion 105/153/110 µs,
JSON growth 7,516/18,289/7,001 B. All nine pairs had equal normalized
runtime projections/reference-count audits; AST added static syntax at a
measured ~0.1–0.4 s provider cost, while fusion was sub-ms. Internal
`profiling.engine_peak_rss_kib` from paired runs (~7–17 MiB) is not the same
measurement as external `/usr/bin/time` process-tree peak (~152 MiB), so
these numbers are **not** compared as if equivalent.

Current release `phase8f13_export_bench` (three new process samples per size,
input parsed before timing, streaming count-writer stdout) produced medians:
**81,325 B** AST trace: reject at −1 B 0.391 ms/0 output, exact success
0.820 ms; VmHWM 3,792 KiB. **25,081,617 B** legacy 8B deep trace: reject
at −1 B 86.686 ms/0 output, exact success 179.918 ms; VmHWM 71,064 KiB in
one of the three runs. The peak includes input parsing but no full-size JSON
output buffer. These are serializer costs, *not* file fsync or terminal I/O;
`/tmp/cte-8d-export-{small,large}-{1,2,3}.log` retain samples. 8B's one-run
4.43 s/451-command “defined” configuration and Phase 7/8R-1 one-run AST
baselines differ in flags, capture scope, host load or sample count; **no
trustworthy identical-config historical pre/post median exists**. Do not
infer repair speedups/slowdowns from them. Aggregate byte capture dominates
its ~9.9 s debugger interval (median capture ~8.3 s): this is a material
cost for the measured aggregate evidence, not a free feature.

## 7. Exact verification and release handoff

On the unchanged audited code all five requested commands succeeded:

```sh
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo test --no-fail-fast -- --test-threads=1
```

Serial log: `/tmp/visual-c-8d-tests-final.log`; **350 passed, zero failed,
zero ignored, 18 targets**, including nine C instrumentation tests,
adversarial GCC/GDB/raw-log fixture, eight JSON-export tests, 8C stress,
Clang/DWARF, runtime memory, and Trace Parser compatibility/deep-validation
suites. Focused 8D reruns: adversarial 1/1, C wrapper 9/9, DWARF registry
12/12, JSON exporter 8/8 (`/tmp/cte-8d-focus.log` for last suite). No failed
serial attempt was discarded in this audit. Tool-dependent tests sometimes
skip on unavailable tools, but GCC/GDB/Clang/Valgrind were present; the direct
external observations above prove the first three actually ran.

**Trace Parser upgrade readiness checklist (not started):**

- [x] P0/P1 repairs including historical pointer replay verified in code,
      focused regression, current integration and serial suite.
- [x] Existing schema 2/legacy parser ingestion, reference checking, optional
      AST absence, partial evidence and bounded final JSON verified.
- [x] Stable repeated release measurements and hard/best-effort resource
      distinctions recorded; unsupported configurations are not silently
      promoted to evidence.
- [ ] Close or explicitly remove **F14's public cross-run identity helper**;
      add wrong-basename and repeated same-line negative regressions; show
      current production Valgrind report still does not acquire false IDs.
- [ ] Formally restrict R03 heap completeness to observed/followed PIDs,
      including an explicit silent-child test, or add a verifiable child census;
      a child with no event/ACK must not be silently claimed observed. Correct
      R04 Phase 7 illustrative/coverage overclaims. Treat R01/R02 as future
      adversarial tests, not automatically proven failures.
- [ ] Re-run the five verification commands after any targeted repair and
      make a fresh approval decision; this audit does **not** waive its
      criterion because the helper currently lacks an internal caller.

**Final verdict: B — Conditional, targeted closure required.** No significant
architectural redesign is indicated by these measurements; no Trace Parser
upgrade was begun. The open public false-attribution path, incomplete fork-child coverage
contract and missing negative regressions prevent an unconditional “all
confirmed defects closed” claim.

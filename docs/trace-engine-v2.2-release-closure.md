# Visual C V2.2 — Phase 8D-R Release-Blocker Closure

## Decision

**A — Approved for the dedicated Trace Parser upgrade, under the documented
V2.2 evidence scope.** Both release findings are closed by a conservative
contract, real subprocess oracles and permanent regressions. This does **not**
authorize a claim of complete multiprocess tracing, exact cross-run
Valgrind-to-GDB lifetime attribution, a sandbox, or a bound on already
assembled trace RAM. The parser upgrade was **not** started. The earlier 8D
conditional verdict is historical; this report records the subsequent repair.
Existing dirty-worktree changes were preserved.

## F14: correction of the premise, failure, repair

**Important correction:** the public helper
`src/valgrind.rs::correlate_with_allocations(report, sites)` did **not**
compare numeric addresses. It matched `ValgrindFrame.file/line` to tuples
`(file, line, primary-run allocation ID)` and allowed basename/`ends_with`
collisions. Only its own positive-path unit test called it; `trace_program`
and the CLI did **not** call it. The resulting `related_allocation_ids` looked
like exact allocation identity even though Valgrind runs in another process.
A matching numeric address would not rescue this site-based association;
neither addresses nor equal source sites identify the lifetime. The earlier
Phase 7 table and README incorrectly described file/line IDs as normal
cross-run correlation; their affected entries were corrected.

**Failing before:** newly added
`valgrind::tests::independent_run_same_site_and_basename_collision_never_assert_identity`
failed against the old implementation (`/tmp/cte-8dr-f14-before.log`, 0/1):
it expected no exact ID for two distinct allocations at the same full
`leaky.c:3` source site and a foreign `/other/leaky.c:3`, but the old helper
filled `related_allocation_ids`. This is a genuine old-behavior assertion,
not just a test failing to compile against the new model.

**After:** the existing public entry point remains callable with the old
arguments, but explicitly **ignores the primary-run allocation ID** and
never creates an exact link from file/line, basename or numeric address.
It clears previously asserted IDs if asked to re-correlate an independent
report. It now requires equal *absolute* file/line (Valgrind `<dir>/<file>`
against the supplied absolute source path, canonicalized where possible).
An optional schema-v2 `ValgrindError.source_site_candidates` contains only
`file`, `line`, and
`evidence="same_source_site_independent_run_not_allocation_identity"`; it
has **no allocation ID or address**. Unresolved/relative/missing file,
different full path, different line, or unsupported execution identity
produce no candidate. Basename equality alone is insufficient. Ordinary
`ValgrindReport` error kinds, native XML stacks and leak totals are
untouched. Existing legacy `related_allocation_ids` deserializes unchanged;
calling the repaired helper on such a report clears those unverified IDs.
No production CLI call to the helper was added, so source candidates are an
explicit opt-in API view. There is **no supported source of exact
same-execution Valgrind allocation-event identity** in this engine: even a
same-run file/line match would not establish which dynamic allocation it
names. Exact links require future *verified same-execution event identity*,
not a caller-supplied matching string, PID, address or file/line; none are
fabricated here. The old function name is retained for API compatibility,
with its restricted semantics documented at the definition.

**Independent numeric-collision check:** compiled
`fixtures/c/phase8dr_address.c` with independent GCC. Two native processes
returned distinct PIDs but the same deliberately requested
`mmap(MAP_FIXED_NOREPLACE)` address **0x500000000000**; a third Valgrind
process returned the **same** numeric mapping while reporting an actual
`InvalidRead` from a separate `malloc/free` at the fixture's source line.
A fourth `alt` native process used **0x500000100000**. These OS mappings
are deliberately **not** claimed to be malloc allocation IDs: the oracle
shows why numeric equality across processes cannot establish malloc origin.
The permanent `tests/release_closure_8dr.rs` compiles/runs all cases, parses
actual Valgrind XML, supplies two site tuples with different proposed IDs
(including a wrong-directory same-basename site), and verifies a single
ID-free source candidate plus intact `InvalidRead` diagnostics. The live
engine CLI on the same fixture completed with two primary-run heap events,
`heap_history.complete=true` for that observed PID, Valgrind `InvalidRead`
and **zero** `related_allocation_ids` (`/tmp/cte-8dr/engine.json`).
Focused nine Valgrind units pass, including missing/other source paths,
legacy JSON IDs, positive exact-path source candidates, UAF and leak XML.
Identical vs different numeric addresses are *not* inputs to the helper,
so the outcome cannot depend on them.

## Fork-child heap-history contract

**Original ambiguity:** `HeapTimeline::history_complete` verified only the
PID-local events actually ingested. `verify_event_status` checked each
*observed* event PID plus the GDB-followed inferior's PID; it has **no fork
census**. A child that forks, emits no event and exits via `_exit` leaves
neither event row nor ACK. Before repair, a real `phase8r2_basic.c -- forks`
trace had `heap_history.complete=true` but no JSON field explaining the
scope: `observed_forks_never_claim_complete_multiprocess_coverage` failed
0/1 (`/tmp/cte-8dr-fork-before.log`, actual `process_scope=null`). No
inference from successful parent ACK can discover a silent child.

**Supported contract (now explicit in every new `heap_history`):**

- `complete` covers the **verified observed event PIDs plus the followed
  inferior PID** (including its valid zero-event ACK), conditional on
  sequence, transport, ACK and retention checks. It is **not** a statement
  about every process the C program created.
- `process_scope="verified_observed_pids_only"` and
  `unobserved_process_coverage="unknown"` are emitted even when the current
  observed PID set contains only the parent. This is intentional: a
  parent-only run and an invisible silent fork can have identical logs.
- `followed_inferior_pid` records the actual GDB PID when known;
  `observed_event_pids` lists distinct PIDs with verified-prefix rows.
  Other observed PIDs have separate sequence/ID spaces; multiple-PID JSON
  allocation IDs are PID-widened. PID inequality is **not** proof of a
  parent-child relationship in an arbitrary trace. GDB's existing default
  process-following behavior captures stopped snapshots only for its followed
  inferior, **not** each child; C wrapper descendants may still append
  PID-labelled events and terminal ACKs to the inherited transport.
- A logged child without an ACK, a failed/truncated ACK, corrupt event data,
  sequence gaps or caps makes the global observed-history `complete=false`
  and downgrades heap certainty. A silent child remains **unknown**, rather
  than changing the scoped parent claim to a fabricated all-process result.
  Abnormal `_exit` after child events demonstrably yields missing child ACK.
- `coverage.complete_for_scope` refers to GDB's configured user-code
  recording scope, not a multiprocess instruction census. No child snapshot,
  inherited heap lifetime, or unlogged allocation is invented. A foreign
  atfork handler or custom allocator may still bypass this model; the Phase
  8R-2 child-lock reset and writer failure detection are unchanged.

**Real raw and GDB evidence:** new bounded `phase8dr_fork.c` modes yield
independently checked raw event counts per PID:
`parent=[4]`, `observed=[2,4]`, `repeated=[2,2,2,4]`,
`silent=[4]`, `incomplete=[2,4]`. Successful ACK counts respectively
1/2/4/1/**1**; the last case's observed child has *no* ACK. Every observed
PID starts at sequence 1. Live GCC/GDB tests repeat all five modes: observed
PID counts 1/2/4/1/2, scoped `complete=true/true/true/true/false`, followed
PID in the observed set, all snapshots owned by that one followed PID, and
JSON round-trip preservation of the new metadata. The three-repeat release
probe independently confirms that `incomplete` contains
`missing allocation event status ACK for PID ...`, while `silent` reports
`unobserved_process_coverage="unknown"` despite its scoped `complete=true`.
Existing test-only pre-repair fork-deadlock reproduction, lock-holder fork,
threaded writes, terminal write failures and per-PID ID-restart tests still
pass. No multiprocess GDB architecture or child discovery was added.

## Compatibility, implementation and failures investigated

Production edits are limited to `src/valgrind.rs`, the optional source-site
and coverage fields in `src/models.rs`, and the coverage assembly in
`src/trace.rs`. Changes to `README.md` and the affected Phase 7 table/ledger
clarify the scope. New `fixtures/c/phase8dr_{address,fork}.c`, one live
Valgrind/GCC test and focused C/GDB/unit regressions preserve existing schema
**2**, legacy JSON field defaults, TP-1/2/3 APIs, heap lifetimes, pointer
provenance, F13 export and Phase 8R-2 instrumentation. No new dependency.
Legacy traces without the new optional fields still deserialize; old
Valgrind diagnostics remain intact unless a caller *explicitly* re-correlates
an independent report with the safe helper.

The **first full serial run was not clean** (`/tmp/visual-c-8dr-tests-final.log`):
AST-on/off projection compared newly added process PIDs literally across
independent executions. The projection now normalizes only session-specific
PID values while **retaining and comparing** observed-PID count, whether the
followed PID is observed, scoped completion, sequence/event shapes and every
other runtime field; its focused test then passed. The **second full run**
(`/tmp/visual-c-8dr-tests-final2.log`) had one independent timing-oracle
failure: the three-second *whole-pipeline* deadline in
`v2::deadline_includes_queries_and_does_not_start_valgrind_after_expiry`
fired before any instruction event on a busy host. The test retains its
`Timeout`, partial-instruction and skipped-Valgrind assertions, but separates
GCC/GDB startup from the runtime oracle with an eight-second deadline and
100,000 step budget; its cleanup bound remains deadline + three seconds.
Focused retest passed. Neither failure was hidden or attributed to an engine
regression without inspecting its assertion.

After the final edits **all five required commands passed**:

```
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo test --no-fail-fast -- --test-threads=1
```

Serial full log `/tmp/visual-c-8dr-tests-final3.log`: **356 passed, zero
failed, zero ignored, 19 targets**. The new real GCC/Valgrind numeric-collision
test ran (not skipped). Focused Valgrind, raw-fork, GDB-fork, AST-on/off and
deadline regressions passed. The preexisting adversarial pointer,
instrumentation, DWARF, export, legacy parser and source/instruction suites
also passed in the serial run.

## Repeated release measurements

Three serial release CLI runs per identical configuration, source vs
`phase8dr_fork.c` mode (or `phase8dr_address.c` for Valgrind),
`--no-pretty --output FILE --timeout 30 --max-steps 300
--sanitizer-trigger never`, stdout discarded. `/usr/bin/time` wall and
peak RSS include tool children; JSON bytes on disk, GDB commands and
instruction/snapshot/event counts from JSON. **Medians**, not pre/post
speedups; artifacts `/tmp/cte-8dr-bench/`:

| Mode | Wall s | GDB cmds | Peak RSS KiB | JSON bytes | Snapshots / instructions / events | Observed PID coverage |
|---|---:|---:|---:|---:|---:|---|
| parent | 3.75 | 152 | 152592 | 117132 | 17 / 48 / 4 | 1 PID, scoped complete; other processes unknown |
| observed child | 4.03 | 257 | 152644 | 177762 | 24 / 96 / 6 | 2 PIDs, scoped complete; other processes unknown |
| three children | 4.47 | 429 | 152600 | 274581 | 36 / 170 / 10 | 4 PIDs, scoped complete; other processes unknown |
| silent child | 4.02 | 257 | 152556 | 175786 | 24 / 96 / 4 | 1 PID, scoped complete; unseen child unknown |
| logged child without ACK | 3.89 | 257 | 152656 | 177818 | 24 / 96 / 6 | 2 PIDs, observed history **incomplete** |
| independent Valgrind error | 3.99 | 229 | 152544 | 210544 | 15 / 53 / 2 | primary PID, `InvalidRead`, zero cross-run IDs |

One parent cold run took 9.53 s vs 3.75/3.75 s warmed; reported median,
not silently discarded. Source/instruction counts are stable across the
three repetitions per mode. The dormant public F14 helper adds **no CLI
correlation pass**; new optional coverage fields account for a small part of
JSON, but no compatible pre-repair binary with the same fixtures/configuration
exists to derive a trustworthy causal time/RSS/size delta. Older Phase 8R-2
single-run figures have different fixtures/settings; do not use them as a
before/after speedup claim.

## Updated defect register and remaining limitations

- **F14 P2 — closed under an explicitly restricted contract:** false
  cross-run IDs no longer originate from the helper, wrong-basename paths
  cannot even form source-site candidates, and real Valgrind diagnostics
  remain intact. There is no supported *exact* Valgrind↔primary-allocation
  same-execution join. Legacy serialized IDs may still exist in imported
  records and require provenance review by a consumer.
- **R03 P2 — closed as an explicit scoped-coverage policy:** observed PID
  transport is checkable; silent/untracked descendants are always
  `unknown`, never claimed as verified multiprocess coverage. The confirmed
  child-lock and writer-loss P1 repairs (F04/F05) remain verified.
- **F01–F13, B01, S01, R05:** current regression suite passes; no new
  contradiction found in this targeted closure. Phase 8D's historical R04
  Phase 7 illustrative/test-count overclaim is a **nonblocking P3
  documentation issue**; the Valgrind-specific table/ledger was corrected
  here, but illustrative claims elsewhere in that historical report are
  not retroactively independent proof. R01/R02 remain unconfirmed risks,
  not classified as observed false evidence.

The process is not sandboxed; unobserved/foreign fork descendants cannot be
certified; event/ACK transport is not authenticated against malicious file
modification; Valgrind and ASan are separate runs; other compilers/libcs and
GDB fork-following modes have not been independently verified. Default final
JSON export remains separately bounded at 128 MiB and does **not** cap
memory used to assemble the trace. These limits remain explicit; none is
turned into a false success claim. On the tested Linux/GCC/GDB/Clang/Valgrind
configuration, both named release blockers have targeted closure and the
full suite is clean. **Final recommendation: A — Approved for starting the
separate Trace Parser upgrade, but do not treat that as already implemented.**

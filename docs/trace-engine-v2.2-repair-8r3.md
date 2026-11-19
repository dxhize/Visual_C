# Trace Engine V2.2 — Phase 8R-3: Evidence Completeness & Reference Validation

Scope: audit F06/F07 (fatal-stop completeness) and F10 (cross-source reference
validation). Phase 8R-2 instrumentation and Phase 8R-1 identity repairs remain
in the worktree. No Phase 8B work is included. The engine is still a recorder,
not a C interpreter: only observed values are reported as such.

## Root causes and repairs

### Snapshot completeness

`src/trace.rs` previously ran the configured/discovered global-observation
loop only for normal source checkpoints. The fatal-signal branch created a
snapshot, assigned `observation.complete` based only on frame/heap state, and
never visited globals. The same branch silently skipped `push_snapshot` at
`max_snapshot_count`, while `Crashed` bypassed the nonterminal coverage
truncation path. An existing requested global could therefore be absent from
an apparently complete crash snapshot; a missing fatal snapshot could coexist
with `coverage.complete_for_scope=true`.

Both source checkpoints and fatal stops now use one bounded
`capture_requested_globals` helper. It spends only the memory left after
frame and live-heap reads, retains the successfully captured prefix, and
records an explicit error/incomplete observation if the deadline/cancellation
interrupts remaining requests or if global discovery/inspection hit its cap.
GDB evaluation failures retain `VariableInfo.read_error` and the existing
memory-evidence status; `finalize_memory_coverage` marks the snapshot
incomplete if any requested object's evidence is incomplete. At a fatal stop,
the same static-alias normalization, pointer enrichment and memory-coverage
finalization run after the global loop. If the fatal snapshot itself cannot
fit, `Recorder::truncate` marks the trace-wide scope incomplete with an
explicit `max_snapshot_count: fatal stop snapshot omitted` reason. No new
required fields or budget increases were introduced. `read_status` and
`memory_evidence` describe readability, **not** variable initialization.

Timeout before a checkpoint means no snapshot at that stop, and the existing
nonterminal truncation path marks recording coverage incomplete. Timeout or
cancellation *during* requested-global capture records how many were observed
before stopping; failed individual GDB queries retain their errors. Normal
checkpoints still capture globals and retain their preexisting limits.

### Cross-source references

`src/trace_parser/validation.rs::validate_ast_registry` previously checked
mostly whether target IDs were present. A `DeclRefExpr` could point to one
source declaration while claiming the existing DWARF ID of another; a valid
member index could belong to an unrelated struct; a heap candidate could
cite a non-call expression or the wrong operation/site. All passed the old
existence checks. Parser ingestion and ownership remain unchanged: the
explicit `validate_deep()` report now emits `StructuralCorruption` findings
for demonstrated contradictions, without rejecting partial/legacy input.

One indexed pass builds maps of existing AST declarations/expressions, DWARF
units/declarations/types, and checks:

| Recorded edge | Definitive inconsistency checked |
|---|---|
| AST declaration → DWARF declaration | name, verified file/direct line/function, compilation-unit source path (using DWARF compilation directory when available), and explicit type ID if both are recorded |
| FieldDecl / MemberExpr → DWARF member | member's actual parent struct/union and index/name/offset, plus independent parent spelling and member-type ID when recorded |
| AST expression → declaration/DWARF | same AST unit, operand unit, exact DWARF declaration ID behind the source declaration, and type/member when its evidence says it was derived from that declaration |
| Heap event → CallExpr candidate or ambiguity | compatible allocator operation and resolved source file/line when both exist; target must actually be an allocator CallExpr |
| Runtime variable → source declaration | preexisting exact DWARF bridge assertion remains in force |

Missing optional locations, absent providers, unresolved DWARF CUs and
unavailable source sites do **not** become corruption. A source header or
macro span need not equal the TU's main file. File identity never falls back
to basename; a fast exact-path comparison avoids filesystem probes for the
common case, with canonical comparison for differing spellings. A heap call
on the resolved source line is **only a candidate**; validation does not
promote it to proof that that expression executed or that two runs shared an
allocation. Numeric heap-address matches retain Phase 8R-1's `address_only`
semantics, not pointer origin. Existing AST/DWARF correlation construction is
unchanged; this is a consumer-side contradiction check.

## Reproduction and independent evidence

- Before the snapshot repair, new live GCC/GDB test
  `crash_globals_and_snapshot_cap_do_not_claim_complete_coverage` failed:
  the fatal snapshot contained **zero** requested globals. After repair the
  crash snapshot contains `observed_global=123`; independent
  `gcc -g -O0` + `gdb --batch -ex run -ex 'p observed_global'` observed
  `$1 = 123` at the faulting `main` instruction. With dual capture and a cap
  of two preceding checkpoints, the crash is still reported as crashed, but
  its omitted fatal snapshot now truncates trace-wide coverage.
- The additional live crash regression requests a nonexistent global: the
  attempted row keeps GDB's error and the snapshot reports incomplete.
  A one-byte shared memory budget likewise leaves the requested global's
  memory evidence non-full and marks the crash snapshot incomplete. Existing
  normal checkpoint, timeout, interrupted-run, multi-frame, and memory-budget
  integration tests remain in the serial suite.
- Before deep-validation repair, the new synthetic test
  `phase8r3_existing_targets_with_contradictory_relationships` failed its
  first valid-ID/wrong-DWARF-expression assertion. It now confirms a valid
  baseline passes, then separately injects a wrong existing DWARF declaration,
  wrong AST unit, wrong GCC compilation unit, valid member under the wrong
  parent, non-allocator target, wrong allocator operation, and wrong source
  line; each reports the appropriate relation-corruption code. Absence of
  optional heap source location remains valid. The existing dangling-ID,
  real fused GCC/Clang fixture and legacy V2.1 parser tests pass.

## Compatibility, changed files and performance

Changed in this phase: `src/trace.rs`, `src/trace_parser/validation.rs`,
`tests/v2.rs`, `tests/evidence_fusion.rs`, new
`fixtures/c/phase8r3_crash.c`, new
`examples/phase8r3_validation_bench.rs`, and this report. Pre-existing
Phase 8R-2 worktree changes are preserved. Schema marker remains 2; no trace
fields, indexing, ownership model, stepping policy, allocator IDs or resource
limits changed. The parser's deep-validation cost is paid only when requested.

Paired *single-run* release CLI probes used the preserved pre-8R-3 executable
`/tmp/cte-8r2-engine-before-8r3` and the newly built release executable;
`/tmp/cte-8r3-bench/` contains the JSON and `/usr/bin/time` files:

| Workload | Before / after wall | Before / after peak RSS | Before / after JSON bytes | Checkpoints / heap events | GDB commands before / after |
|---|---:|---:|---:|---:|---:|
| Fatal global (`phase8r3_crash.c`) | 3.73 / 3.62 s | 152564 / 152640 KiB | 26000 / 27318 | 3 / 0 both | 36 / 41 |
| Fused AST (`ast_source.c`) | 4.35 / 4.12 s | 152668 / 152668 KiB | 432571 / 432569 | 24 / 9 both | 463 / 463 |

Crash's +5 debugger commands and +1318 JSON bytes buy two observed global
rows in the selected CLI configuration (explicit name plus discovered
qualified name). No AST-workload checkpoint/event/command changes. These are
single samples under ASLR and cannot establish runtime speed or RSS trends;
variable strings and process IDs also affect JSON byte counts. Release
`phase8r3_validation_bench` measures load separately from 100 repeated
opt-in validations over the *same* canonical store: 432569-byte AST trace
~8.2 ms/100 calls (~82 µs per call); 27318-byte crash trace ~0.145 ms/100
(~1.45 µs per call). The AST trace has three non-corruption findings; the
crash trace has four. Exact paths fast-path without repeated canonicalization.
This is validation overhead, not a claimed pre/post parser speedup; the
pre-8R-3 executable was a CLI, not a separately benchmarked parser library.

Validation run: `cargo check --all-targets`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and
`cargo test --no-fail-fast -- --test-threads=1` all passed on the final
worktree: **330 tests, zero failures**. Serial output is
`/tmp/visual-c-8r3-tests-final.log`. Phase 8R-2 targeted C/Rust regressions
passed before editing; the serial run also passes them afterward. The first
full serial run had one unrelated supervisor test fail because its 100 ms
cancellation fired before `echo observed` reached the captured pipe. We
inspected the exact assertion (`src/supervisor.rs:337`), reran that one test
five times successfully, then reran the entire serial suite successfully.
The first run's other targets passed; it is not represented as a clean run.

## Remaining limitations and Phase 8B handoff

Deep validation detects *contradictions with recorded evidence*, not missing
provenance: a lone compatible line candidate is not exact execution evidence;
headers, macro expansion, missing locations, and same-line calls can remain
ambiguous. A file path that cannot be resolved with supplied DWARF
compilation-directory data does not prove a CU mismatch. Initial global
symbol discovery still has its existing GDB query/cap limitations; absence
of an unrequested, undiscovered symbol is not asserted complete evidence.
Post-multithreaded-fork limits and terminal allocation ACK limitations remain
as in Phase 8R-2. Phase 8B should adversarially test cross-CU/header aliases,
macro-expanded calls, corrupt journals, global-query deadlines, and
interruption at individual checkpoint stages; none is implemented here.

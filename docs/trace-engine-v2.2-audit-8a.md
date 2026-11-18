# Trace Engine V2.2: Phase 8A Independent Audit

Audit scope: the working tree as inspected on 2026-11-18, not just committed
`HEAD`. This is a read-only assessment; the only intentional tracked-file
addition is this report. Earlier implementation reports are treated as claims,
not as validation. No Phase 8B tests or repairs were implemented.

## A. Executive assessment

The pipeline has a reasonably clear division between GCC/DWARF static facts,
GDB observations, the heap wrapper, optional Clang syntax, and trace parsing.
MI result routing, most partial memory reads, and historical allocation reuse
have substantive safeguards and tests. It is **not yet ready to be declared
correct**: several narrow, repairable paths can emit misleading provenance,
cross-compiler identities, or completeness, and a multithreaded fork can hang
the instrumented program. Verdict B in section T recommends targeted repairs
before broad adversarial testing. No P0 finding was established.

## B. Repository state

- Branch `main`, `HEAD` `486410b39646ee8c4ad20a527260d3a58929e2d1`.
- The initial `git status --porcelain=v1 --untracked-files=all` showed modified
  Cargo files, README, C wrapper, allocation/CLI/config/GDB/memory/models/
  recorder/resource/trace Rust modules and tests; deleted
  `session-00cff1b2-7b5c-4f49-b79c-58175358dfbb.md`; untracked phase
  documentation, fixtures, scripts, `src/dwarf.rs`, `src/clang_ast.rs`,
  `src/evidence/`, `src/trace_parser/`, and their tests. None was reset.
  The deleted session transcript is not an engine source deletion.
- Toolchain observed: rustc/cargo 1.99.1, GCC 16.2.1, Clang 22.1.8,
  GDB 18.1; `Cargo.toml` uses gimli 0.34 and object 0.39.
- Independent static checks passed: `cargo check --all-targets`,
  `cargo fmt --all -- --check`, and
  `cargo clippy --all-targets -- -D warnings`.
- Independent `cargo test --no-fail-fast -- --test-threads=1` did **not** pass:
  305 passed, one failed, zero ignored across targets. The only failure was
  `tests/integration.rs:255-273::step_limit_bounds_execution`: actual
  `ExecutionStatus::Timeout`, expected `StepLimitReached`. A single isolated
  rerun, `cargo test --test integration step_limit_bounds_execution -- --exact
  --test-threads=1`, passed (5.10s). Its configuration sets an overall
  five-second timeout at `tests/integration.rs:91-94`, which can expire before
  50 debugger steps on a slower run. That is **timing-sensitive evidence**,
  not proof that all timeout behavior is correct, nor a justified dismissal
  of the full-run failure. No source was modified to make it pass.
- Reviewed implementation paths in `main`, `cli`, `config`, `compiler`,
  `trace`, `gdb_mi`, `mi_parser`, `recorder`, `memory`, `allocation_tracker`,
  `dwarf`, `clang_ast`, `evidence`, `models`, `resource_limits`, `supervisor`,
  `storage`, `valgrind`, `sanitizer`, and `trace_parser`; examined integration
  tests, fixtures, scripts and Phase 3-7 reports. Prior phase history is mostly
  uncommitted; commit-to-commit attribution is therefore unavailable.

## C. Architecture map

`main::main` parses CLI/config and calls `trace::trace_program`, then writes
the resulting JSON to an optional file and stdout (`src/main.rs:21-69`).
`compiler::build_argv` compiles GCC debug and separately sanitized artifacts,
adding the allocation wrapper for the debug build (`src/compiler.rs:392-443`).
`trace_program` extracts optional GCC DWARF, optionally invokes Clang per user
translation unit, starts GDB/MI2, records source checkpoints/instruction events,
ingests allocator events, runs separate diagnostic passes, then correlates DWARF
and calls `evidence::fuse` before returning a `DebugSessionTrace`
(`src/trace.rs:350-387,913-937,995-1039`). TP-1 loads canonical traces into
`Arc<DebugSessionTrace>`; TP-2 indexes and TP-3 views borrow them. Independent
Valgrind/sanitizer runs are supplementary, not GDB-run observations.

## D. Component responsibility review

| Component | Entry, state/ownership and boundary | Failure/limit evidence | Tests reviewed |
| --- | --- | --- | --- |
| CLI/build | `main`, `Cli::to_config`, `build_argv`; one request-owned `TraceConfig` and temporary binaries | Config validation; tool-supervised compilation and session deadline | `cli` units; `tests/integration.rs` |
| DWARF | `dwarf::extract_excluding`; one session `TypeRegistry`, indexed DIEs, cached CUs | Object/decoded bytes, DIE/CU counts, 5s extraction; failure leaves registry absent | `tests/dwarf_registry.rs` |
| Clang | `clang_ast::analyze`; optional session `SourceMetadataRegistry`, per-TU statuses | Process/output/node/depth/retention/deadline bounds; failure does not stop GDB | `tests/clang_ast.rs` |
| GDB/MI | `GdbSession::launch`, tokened `execute`, `wait_stopped`; session-owned command, event and PTY state | Deadlines, command count, process-group cleanup | `gdb_mi` routing units; `tests/v2.rs` |
| Recorder/memory | `Recorder::checkpoint` and GDB capture; per-stop state and snapshot-owned bytes | Object/snapshot/read budgets, resident journal budget; partial status | `tests/runtime_memory*.rs` |
| Heap | C `__wrap_*` -> append-only file -> `EventFileReader` -> `HeapTimeline`; PID-local counters and trace-level lifetimes | Bounded ingestion/retention, sequence and transport quarantine; writer failure caveat F05 | `allocation_tracker` units, `tests/heap_provenance.rs` |
| Fusion | `evidence::fuse`; transient indexes, mutations restricted to optional links/summary | One pass and capped registries; validation is partly existence-only | `tests/evidence_fusion.rs` |
| Diagnostics/parser | Separate Valgrind/ASan processes; TP-1/2/3 retain shared canonical trace | XML/output caps; parser deep validation opt-in | `valgrind`/`sanitizer` units and `tests/trace_parser*.rs` |

This partition is maintainable overall. Coupling that needs tightening: AST
correlation runs in `clang_ast::analyze` while later fusion runs in `evidence`;
both must agree on source/CU identity. `recorder::reserve` measures journal
resident records, not the entire `DebugSessionTrace` or final JSON (F13).

## E. Confirmed correctness defects

Each finding lists a minimal correction direction only; no fix was made.

**F01 | P1 | Confirmed | Pointer provenance**

- Location: `src/memory.rs:226-255`, especially `pointer_target:233-252`;
  `src/models.rs:594-607` and parser `src/trace_parser/validation.rs:985-1006`.
- Problem/trigger: a pointer's *numeric address* matches exactly one currently
  live heap lifetime with complete observed history. The code correctly sets
  `provenance_status="address_only"` and
  `address_match_allocation_id`, but also sets `allocation_id` to that ID.
  Address containment cannot establish pointer origin (e.g. integer-to-pointer
  casts or unobserved pointer history). Parser validation explicitly permits
  this `address_only` + `allocation_id` combination.
- Impact: consumers can treat a candidate location as definite allocation
  identity, contrary to the Phase 5/7 evidence contract; plausible but wrong
  heap edges result even without previously observed address reuse.
- Existing test: `src/memory.rs:789` asserts the promoted ID, so the test
  codifies rather than challenges the assumption. Correction: reserve
  `allocation_id` for independently demonstrated provenance, leave the numeric
  candidate in `address_match_allocation_id`; verify casts and copy histories
  against independent runtime ground truth.

**F02 | P1 | Confirmed | Cross-compiler AST identity**

- Location: `src/compiler.rs:396-398,419-426` and
  `src/clang_ast.rs:174-220,273-283`.
- Problem/trigger: GCC receives user `-O2` after engine `-O0`; Clang drops
  `-O2` as supposedly preprocessing-neutral. GCC `-O2 -dM -E -x c /dev/null`
  defines `__OPTIMIZE__`, while the Clang syntax invocation without `-O2`
  does not (verified with the installed compilers). Source using
  `#if __OPTIMIZE__` can therefore produce a different AST than the compiled
  executable, yet `clang_ast::correlate_declarations` still emits location links.
- Impact: wrong source declarations/types/expressions or omitted compiled
  declarations can be fused into an otherwise valid runtime trace. Existing
  tests cover unsupported GCC flags, not optimization-dependent preprocessing.
  Correction: propagate compatible optimization flags or disable cross-provider
  links when preprocessing equivalence is not established. Verify conditional
  declarations at the *same source location* with independent GCC macro/DWARF
  and Clang AST evidence.

**F03 | P1 | Confirmed | Wrong DWARF object identity**

- Location: `src/dwarf.rs:950-995` (`ObservationTypes::enrich`).
- Problem/trigger: the fallback for a GDB-qualified `'/a/main.c'::value`
  accepts a DWARF declaration in `/b/main.c` if the basenames match and it is
  the only indexed candidate; it also runs after an ambiguous location match.
  The full-path qualifier is stronger evidence than a basename, not weaker.
- Impact: wrong declaration/type and possibly static extent propagate to a
  runtime observation and then through the Phase 7 source-ID bridge. Existing
  `tests/v2.rs` same-name-statics test does not cover the case where the
  correct file has no retained matching DIE but a wrong same-basename file does.
  Correction: compare canonical full paths when GDB gives one, and never
  resolve an ambiguous location match via this weaker fallback. Verify two
  directory-distinct `main.c` files with only the wrong DIE retained.

**F04 | P1 | Confirmed | C wrapper deadlock**

- Location: `c_runtime/alloc_tracker.c:73-78,89-103,236-250,280-292`.
- Problem/trigger: the process-global spinlock is held across real malloc or
  realloc, but there is no fork-child reset/at-fork protocol. A second thread
  can fork while the lock is held; the child inherits a locked `g_spin` with no
  releasing thread. Its next wrapped allocation spins indefinitely.
- Impact: instrumentation changes execution behavior and can hang a normal C
  program until the engine deadline. No multithreaded-fork wrapper test was
  found. Correction: establish safe fork coordination/child reset with explicit
  inherited-history status; verify a fork-under-lock test with an external
  watchdog, not timing alone.

**F05 | P1 | Confirmed | Heap history completeness**

- Location: `c_runtime/alloc_tracker.c:173-207` and
  `src/trace.rs:867-902`; `src/allocation_tracker.rs:420-465`.
- Problem/trigger: non-EINTR `write` failures or zero writes are discarded
  after `g_seq` is incremented. If the *last* operations fail, no following
  event exposes a sequence gap; the Rust reader has no writer-terminal counter.
- Impact: the final `heap_history.complete` can be true with missing allocation
  or free events. Sequence quarantine works only when a later event reveals
  loss. Correction: make writer failure/final sequence observable, or do not
  assert complete coverage without a terminal acknowledgement. Verify failure
  after the last emitted event with a controlled failing descriptor.

**F06 | P2 | Confirmed | Crash snapshot completeness**

- Location: fatal-signal branch `src/trace.rs:1403-1459` versus normal
  checkpoint globals loop `src/trace.rs:1582-1640`.
- Problem/trigger: a fatal-signal snapshot calls `push_snapshot` but skips the
  configured/discovered global capture loop. It can set
  `snapshot.observation.complete=true` with empty globals even when requested
  global evidence was omitted. The ordinary path captures them and records a
  `max_globals` omission. Impact: a crash view appears more complete than it
  is. Existing tests exercise ordinary global capture, not requested globals
  at a fatal stop. Correction: capture under the same budget on fatal stops or
  report the omission; compare a crashing C fixture's global with independent
  GDB at the crash PC.

**F07 | P2 | Confirmed | Crash-at-cap coverage**

- Location: `src/trace.rs:1403-1461,2155-2161,1977-1991` and
  `src/recorder.rs:205-220`.
- Problem/trigger: `push_snapshot` silently skips a fatal stop once
  `max_snapshot_count` is reached; `Crashed` bypasses the nonterminal coverage
  truncation path. Dual-mode `coverage.complete_for_scope` can remain true
  although the crash observation was omitted. A limit reason may exist
  elsewhere, so this is a conflicting completeness claim rather than a hidden
  crash. Correction: truncate recording coverage for the omitted terminal
  snapshot. Verify crash exactly after filling the cap.

**F08 | P2 | Confirmed | Step timeout classification**

- Location: `src/trace.rs:110-112,1715-1735,1814-1825,2051-2053`;
  `src/gdb_mi.rs:626-652,731-740`.
- Problem/trigger: when the inferior blocks for the fixed ten-second
  per-step wait but the session deadline has not expired, a
  `GdbCommandTimeout` is classified `DebuggerFailure`, hence
  `ProgramOutcome::DebuggerFailed` (`src/trace.rs:999-1015`), even though GDB
  accepted the step and the inferior simply did not stop. Existing stall tests
  use a shorter overall deadline. Correction: distinguish MI command failure
  from stop-wait timeout and classify the latter as timeout; test an inferior
  blocked >10s with a >10s session deadline.

**F09 | P2 | Confirmed | Heap-call candidate precision**

- Location: `src/clang_ast.rs:941-969,991-1032`.
- Problem/trigger: one compatible direct call causes immediate candidate
  emission; the compatible macro-expanded bucket is checked *only if there
  are no direct calls*. One `free(p)` plus a macro-expanded `free(q)` on the
  same line falsely satisfies `unique_direct_call_on_resolved_line...`.
  Impact: a location candidate receives stronger uniqueness than evidence
  supports (not an exact execution claim). Existing tests separate direct-only
  and macro-only lines. Correction: count both buckets before claiming unique;
  verify a mixed direct/macro line with an event from each call.

**F10 | P2 | Confirmed | Deep reference validation**

- Location: `src/evidence/validation.rs:72-80,108-139` and
  `src/trace_parser/validation.rs:315-398`.
- Problem/trigger: validation checks target *existence* but does not check
  `referenced_dwarf_declaration` equals the link of
  `referenced_declaration`, nor that a heap candidate names an allocator
  `CallExpr` compatible with its event/file/line. A trace with valid IDs but
  contradictory edges passes these checks. Impact: loaded or hand-edited
  trace can be structurally sound while cross-source identity is false.
  Existing invalid-reference tests inject *absent* IDs, not wrong existing
  IDs (`tests/evidence_fusion.rs:514-584`). Correction: validate consistency
  and candidate operation/position when evidence exists; inject wrong but
  existing IDs in parser tests.

**F11 | P2 | Confirmed | Inferior output cap**

- Location: `src/gdb_mi.rs:93-104`, `src/trace.rs:1693-1697`.
- Problem/trigger: `ProgramBuffer::push` marks output truncated when retained
  length *equals* `cap`, including an exact-fit final write that discards no
  byte. Impact: `OutputLimitReached` and incomplete-output metadata can be
  reported for a normal run. The unit test uses 100 bytes against cap 50,
  not exact fit (`src/gdb_mi.rs:3644-3651`). Correction: set truncated only
  if new data exceeds remaining capacity; verify exact fit and next-byte case.

**F12 | P2 | Confirmed | AST status summary**

- Location: `src/trace.rs:364-385`, `src/evidence/mod.rs:35-44`.
- Problem/trigger: an enabled but missing Clang creates `Some(registry)` with
  only unavailable units; `EvidenceFusionSummary.ast_available` is then true
  solely because the registry exists. Cancellation may create an empty registry
  with the same value. Impact: consumers using the convenient summary can
  mistake an unsuccessful optional provider for usable evidence. Per-unit
  status is retained, so evidence is not destroyed. Existing missing-Clang
  test checks per-unit status but not `ast_available`
  (`tests/clang_ast.rs:198-229`). Correction: make the summary's meaning
  explicit or derive it from actually retained usable units; test all-failed,
  partial and disabled modes.

**F13 | P2 | Confirmed | Final trace resource limit scope gap**

- Location: `src/storage.rs:25-42`, `src/recorder.rs:251-264`,
  `src/main.rs:31-49`.
- Problem/trigger: `max_trace_bytes` bounds *journal append*, while the final
  trace is serialized twice (output file and stdout) without a byte budget;
  `max_trace_memory_bytes` counts recorder records, not the total canonical
  trace, static registries, and serializer buffers. Impact: the advertised
  trace storage budgets do not bound final JSON growth or its duplicated I/O.
  This is a scope/operational defect, not evidence that normal-sized traces
  presently exceed memory. Correction: document separate journal/final limits
  and apply an explicit bounded final-output policy without silently dropping
  records; test a small configured limit with nontrivial trace.

**F14 | P2 | Confirmed | Cross-run diagnostic attribution**

- Location: `src/valgrind.rs:375-417`.
- Problem/trigger: `correlate_with_allocations` attaches GDB-run allocation IDs
  to separate-run Valgrind errors by same line and `file_matches`, which even
  accepts equal basenames from different directories. Multiple allocations on
  one line all acquire a `related_allocation_ids` edge. Impact: a different
  source file or execution lifetime can appear related to a diagnostic; the
  field does not encode this as a mere weak candidate. Existing unit tests
  cover a matching line, not basename collision. Correction: use exact
  canonical source identity when possible and expose cross-run/site-only
  certainty; verify two different `main.c` paths and repeated same-line calls.

## F. Suspected risks and gaps needing reproduction

**R01 | P2 | Strongly suspected | Partial static identity**

- Locations: `src/clang_ast.rs:819-925`, `src/dwarf.rs:950-995,1017-1054`,
  `src/evidence/declarations.rs:50-109`.
- DWARF/AST matching keys omit CU and lexical-scope identity, and member
  matching selects the unique retained tag spelling rather than proving the
  AST record's lexical parent. If GCC omits one shadowed/tag DIE, uniqueness
  among *retained* records need not imply entity identity. An incorrect but
  existing reference passes F10's validation. No wrong real-GCC link was
  reproduced in this audit, so this is **not** presented as a confirmed wrong
  trace. Minimal direction: require available CU/lexical/parent provenance;
  otherwise leave links unresolved. Ground-truth test: two TUs sharing a file
  path plus a shadowed local tag and intentionally omitted DWARF DIE.

**R02 | P2 | Needs reproduction | Terminal PTY drain**

- Locations: `src/trace.rs:1937-1940,2084-2089`,
  `src/gdb_mi.rs:1244-1252`.
- The debugger shutdown aborts the PTY task without waiting for a final drain.
  If MI exit arrives before the PTY reader processes queued bytes, terminal
  stdout can be short although the debugger stage reports complete output.
  No observed lost output was established. Test a fast-exiting program that
  writes a known terminal payload repeatedly against independent OS stdout;
  if reproduced, allow bounded drain before aborting reader on normal exit.

**R03 | P2 | Confirmed | Forked history; impact conditional**

- Locations: `c_runtime/alloc_tracker.c:73-78,195-207` and
  `src/allocation_tracker.rs:420-465`.
- After a fork, the child inherits nonzero sequence and live table. Its first
  event triggers a missing-prefix quarantine for its PID; this is conservative
  rather than fabricated history, but child allocations become unavailable
  despite being observed. The behavior matters if supported user C forks.
  Specify a fork/inherited-state protocol or explicitly document unsupported
  fork coverage; verify a fork after parent allocations.

**R04 | P3 | Confirmed | Documentation overclaim**

- Locations: `docs/trace-engine-phase7.md:116-118,192-224,293-307,362-397`
  versus `tests/evidence_fusion.rs:249-329,334-424,588-708`.
- The report calls a fabricated-ID illustrative JSON fragment an example
  "from" a real fixture, says same-function shadows differ by function key,
  and labels eight letter-grouped tests as A-S independent coverage. The
  actual test mostly checks nonzero link counts, existing targets and paired
  outputs. This overstates what is proved, not necessarily runtime behavior.
  Correct descriptions after independent tests, without inventing coverage.

**R05 | P2 | Confirmed | Test instability; engine cause unproven**

- Locations: `tests/integration.rs:91-94,255-273`.
- The step-limit test demands `StepLimitReached` but allows only five seconds
  for the entire trace; the independent full serial suite returned `Timeout`
  once, and a single isolated rerun passed in 5.10 seconds of test wall time.
  This does not establish an engine regression; it demonstrates that the
  fixture lacks a deterministic margin/ordering oracle. Phase 8B should
  distinguish the session deadline from the step limit using controlled
  scheduling and retained GDB command/step evidence, not simply rerun until
  green. No test was edited in this read-only audit.

## G. Resource and lifecycle review

The session deadline, supervised tool output, process-group cleanup and
inferior rlimits/cgroup attempt are real. `resource_limits.rs:15-20,191-260`
explicitly says this is **not a sandbox**; inferior memory/file limits are
best-effort when applied with `prlimit` after the first main stop, and the
process cap depends on a writable delegated cgroup. GDB/diagnostic children
are deliberately not given the inferior's address-space cap; pre-main code is
outside the later inferior `prlimit`. Source-checkpoint, read, heap-event,
DWARF/AST traversal/retention and journal budgets exist, but F13 limits their
whole-trace guarantee. The wrapper's file can grow before prlimit applies,
and its write errors are not reported (F05). Temporary session cleanup occurs
at normal finalization (`trace.rs:1036-1039`); examine early-return cleanup
in Phase 8B with fault-injected tools and process-tree observation.

## H. Rust/C instrumentation review

The primary unsafe boundary is Linux process-limit/pre-exec handling
(`resource_limits.rs:197-240`) and the C allocator wrapper. No demonstrated
Rust lifetime unsoundness or C use-after-free inside bookkeeping was found.
`Arc`-backed parser views borrow canonical records instead of cloning entire
registries. C maintains a fixed table and serializes event emission; this
prevents concurrent table races but holding its lock over real allocation
creates F04. C `write` errors create F05. `realloc(p,0)` handling explicitly
assumes glibc (`c_runtime/alloc_tracker.c:305-308`), not a portable C rule;
support on other libc targets is unverified, not asserted. No general-purpose
security boundary protects execution of untrusted C.

## I. GDB/MI2 review

MI command results are token matched, async stops use their own queue, late
results are preserved, and tests cover token/async separation and poisoned
timeouts (`src/gdb_mi.rs` routing regressions). I did not find a demonstrated
path for one command to acknowledge another. Observations happen at stopped
checkpoints; a per-checkpoint byte-read cache does not span stops. Activation
identities derive from frame chains and become unknown on truncated chains,
not simply stack depth. Caveats: F08 mislabels long stop waits, F11 exact-fit
output, R02 terminal output drain. A real MI transcript with non-stop or
multi-thread interleavings and independent GDB oracle remains necessary.

## J. Stack and memory review

Frame 0 remains distinct from optional caller observations; GDB lexical probes
carry activation and scope information, while static DWARF association is
post-processing. `ScopeStatus::Active` explicitly does not mean initialized
(`src/models.rs:566-590`). Memory parsers accept only contiguous verified
prefixes, with statuses for unreadable, truncated and not-captured; unit tests
cover holes/overlaps and per-stop cache containment. A shared snapshot byte
budget and caller-query quota reduce repeated reads. The fatal branch omits
global captures (F06), and terminal cap can make coverage inconsistent (F07).
No evidence was found that data from two stops is reused as fresh bytes.

## K. Heap provenance review

`HeapTimeline` uses per-PID sequence checks, allocation IDs and separate
historical lifetimes; realloc same-address and movement create linked
transitions. Duplicate/out-of-order/lost event history quarantines block-state
inference, and capped retention is explicitly reported. Reused addresses are
not rebound by the timeline; F01 is the separate pointer-target promotion
error. Transport accounting lacks terminal writer acknowledgement (F05);
fork hazards are F04/R03. Heap-to-AST links are source-line candidates, not
execution claims, but mixed macro/direct calls violate uniqueness (F09).

## L. DWARF/Clang review

DWARF sections have compressed-size, CU/DIE and deadline checks, and type
refs are checked against registry IDs. Member offsets use GCC attributes;
unsupported runtime-dependent expressions are marked unresolved. The
Phase 7 origin resolver indexes absolute DIE offsets, caches owning units,
limits chains to four hops and tracks cycles (`src/dwarf.rs:213-423,512-551`);
real LTO tests check a cross-CU path. Longer valid chains are counted as
unresolved but facts gathered on earlier hops can remain attached; a separate
fixture must test whether that partial result is ever consumed as a fully
resolved declaration. Clang has bounded process output, time and AST traversal,
and distinguishes direct/macro source locations. F02 shows that GCC/Clang
preprocessing equivalence is not actually guaranteed; R01 concerns missing
lexical/CU proof. Non-ELF, non-GCC, other endian/target, DWARF signature refs,
GNU AST variants and bitfield ground-truth coverage are unverified.

## M. Evidence fusion review

Fusion runs once after runtime capture (`trace.rs:913-937`), indexes DWARF
types/units and declaration bridges, and does not issue GDB commands or
rewrite observed values/addresses. Link validation removes dangling IDs and
the parser reports them on imported traces; valid-but-wrong identities are
not detected (F10). F02/F03 can inject wrong upstream identities into an
otherwise exact-ID bridge. F09 and R01 concern claimed uniqueness. Heap-call
work is time-checked every 64 events but an indexed line can still accumulate
and clone all calls before its eight-ID output cap (`clang_ast.rs:973-1033`);
that is a bounded-by-registry performance risk, not a demonstrated quadratic
runaway under current caps. The small AST/DWARF registries are not copied per
checkpoint.

## N. Trace schema review

Schema marker stays 2 and new optional fields default on deserialization
(`models.rs:498-511`); old fixtures load and absent optional providers do not
make structural validation fail. The parser owns canonical `Arc` data and
exposes borrowed views. `None` may mean missing/not recorded, while per-unit
AST status and per-observation byte statuses offer more precision; F12 is a
summary field that collapses "registry exists" with usable evidence. Deep
validation checks dangling references, not every semantic identity invariant
(F10). Allocation IDs remain distinct from address candidates in the model,
but F01 violates that distinction at construction.

### Cross-component invariant assessment

| Invariant | Assessment and evidence |
| --- | --- |
| Checkpoint belongs to observed execution context | Mostly protected by stopped MI frame/stop ID (`trace.rs:1582-1593`); crash omissions F06/F07 require qualification; independent MI oracle still missing. |
| Instruction order and interruption gaps | Recorder emits stop-linked ordered events, parser validates stop/order and missing boundaries (`trace_parser/validation.rs:542-595`); F08 can misclassify a blocked step, while nonterminal runs add a gap (`trace.rs:1977-1991`). |
| Recursive activation and caller frame ownership | Frame-chain IDs and caller activation checks exist in `gdb_mi` regression tests; truncated chain loses identity rather than reusing a depth. No demonstrated wrong activation. |
| Scope versus initialization | Explicitly separate in `models.rs:566-590`; `tests/v2.rs` lexical-scope regressions. Protected as modeled; initialized bytes remain unknown. |
| Byte location, length, failure and budgets | GDB parser/capture tests check contiguous prefixes, holes and read quotas; `recorder.rs:251-264` budgets journal records. Fatal globals F06 and whole-trace cap F13 qualify completeness. |
| Heap identity, reuse and realloc | PID-local sequence/lifetime reconstruction and `tests/heap_provenance.rs`; historical IDs remain distinct. F01 promotes address candidate, F05 may miss terminal log loss, F04/R03 affect fork. |
| DWARF compiled layout and dynamic unknowns | GCC registry, resolved type refs and real layout tests; dynamic expressions marked unresolved. Cross-CU LTO path tested; R01 flags missing parent/CU identity for fusion, not layout overwrite. |
| Clang syntax versus execution and TU separation | AST records are static and macro spans explicit; no source syntax is a runtime stop. F02 permits mismatched preprocessing; R01 tests TU/lexical evidence sufficiency. |
| Fusion link support and runtime immutability | One final pass only edits optional metadata and checks referential existence; AST-on/off test compares runtime views. F02/F03/F09/F10 undermine correctness of some links, not captured runtime bytes. |
| Serialization and optional absence | Serde defaults, V2.1 fixture load and parser borrowed views tested; F12 confuses registry presence with provider availability, F10 misses valid-ID contradictions. |

## O. Test-quality assessment

The suite has real GCC/GDB/Clang runs for ordinary execution, DWARF layouts,
cross-CU LTO, nested stack/memory, heap reuse/realloc and AST optionality;
it also has MI parser/unit and synthetic graph tests. This is meaningful
coverage, not mere serialization. However, `tests/evidence_fusion.rs:249-329`
chiefly asserts nonzero links and valid *targets*, not that each target is the
right declaration/member. Invalid-ref tests use missing IDs rather than
existing wrong IDs. Tool-dependent tests return early as passing tests if
prerequisites are unavailable (`tests/evidence_fusion.rs:26-36,249-257,588-592`).
The AST-on/off test compares two outputs of the same engine
(`tests/evidence_fusion.rs:631-708`), so it cannot prove either output true.
`scripts/phase7_benchmark.py:311-318` reuses
`scripts/phase5_benchmark.py:265-365`: that projection excludes caller-frame
variables (`:341-355`), replaces unresolved addresses with `None` (`:279-281,
:313-314`) and then excludes the unresolved counter from equality. The
test-local projection does include caller variables but still cannot establish
independent ground truth. Reports claiming A-S independent proof or overall
correctness exceed what tests establish (R04). The independent full suite had
one timeout-vs-step-limit failure, with one isolated pass (R05); it must not be
reported as a clean 306-test audit run. No Phase 8B tests were added.

## P. Code quality and handoff consistency

`evidence/heap.rs` is a ten-line forwarding shim to `clang_ast`, while Clang
analysis also performs declaration matching before the later evidence pass;
the boundary is comprehensible but spreads ownership of reconciliation.
`storage::recover` silently treats any >16MiB line lacking a newline in its
bounded read as a torn tail (`src/storage.rs:54-69`), whereas the newer TP-1
journal reader rejects oversized complete records; the former has no production
call site beyond tests and is a low-priority dormant inconsistency. The Phase
7 report's illustrative JSON and test-coverage wording need correction (R04).
No partial implementation was inferred merely from the dirty worktree.

## Q. Prioritized defect register

| ID | Severity | Confidence | Component/evidence | Impact | Action |
| --- | --- | --- | --- | --- | --- |
| F01 | P1 | Confirmed | `memory.rs:233-252` | Address candidate presented as allocation origin | Separate fields semantically |
| F02 | P1 | Confirmed | `compiler.rs:396-426`; `clang_ast.rs:210-214` | AST can describe a different compiled branch | Preserve equivalent preprocessor flags |
| F03 | P1 | Confirmed | `dwarf.rs:950-995` | Wrong full-file static DWARF link | Reject basename fallback for full paths |
| F04 | P1 | Confirmed | `alloc_tracker.c:89-103,242-244` | Post-fork allocator hang | At-fork lock protocol |
| F05 | P1 | Confirmed | `alloc_tracker.c:173-207` | Invisible terminal event loss | Verifiable writer completion |
| F06 | P2 | Confirmed | `trace.rs:1403-1455` | Crash globals omitted yet complete | Capture or disclose omission |
| F07 | P2 | Confirmed | `trace.rs:1977-1991,2155-2161` | Crash-at-cap coverage conflict | Truncate coverage |
| F08 | P2 | Confirmed | `trace.rs:1715-1735` | Inferior hang mislabeled debugger failure | Split wait/command deadlines |
| F09 | P2 | Confirmed | `clang_ast.rs:991-1024` | Mixed macro/direct line falsely unique | Count both buckets |
| F10 | P2 | Confirmed | `trace_parser/validation.rs:315-398` | Valid-ID wrong edges pass deep validation | Check cross-link consistency |
| F11 | P2 | Confirmed | `gdb_mi.rs:93-104` | Exact-fit output falsely truncated | Flag only discarded bytes |
| F12 | P2 | Confirmed | `evidence/mod.rs:41-44` | Failed AST reported available | Refine availability semantics |
| F13 | P2 | Confirmed | `storage.rs:25-42`; `main.rs:31-49` | Final JSON outside journal byte cap | Explicit final-output budget |
| F14 | P2 | Confirmed | `valgrind.rs:375-417` | Cross-run wrong-site ID candidates | Canonical path and certainty |
| R01 | P2 | Strongly suspected | `clang_ast.rs:819-925`; `evidence/declarations.rs:50-109` | Partial static context can false-match | Adversarial CU/scope oracle |
| R02 | P2 | Needs reproduction | `gdb_mi.rs:1244-1252` | Possible tail stdout loss | Independent terminal-output test |
| R03 | P2 | Confirmed | `alloc_tracker.c:73-78`; `allocation_tracker.rs:420-465` | Post-fork history quarantined | Explicit fork protocol/limit |
| R04 | P3 | Confirmed | `docs/trace-engine-phase7.md:116-118,192-224` | Documentation overstates evidence | Correct after independent tests |
| R05 | P2 | Confirmed | `tests/integration.rs:91-94,255-273` | Full serial suite failed once on deadline | Separate step-limit and wall-clock oracle |

## R. Phase 8B adversarial testing plan (not executed)

1. **Identity oracle:** instrument pointer creation/casts and `malloc`/`free`
   in a separate C test oracle; compare pointer origin and numeric containment
   separately, including reuse and integer casts. Targets F01.
2. **Preprocessor divergence:** compile `#if __OPTIMIZE__` and target-dependent
   declarations with GCC flags, dump GCC macros/DWARF and Clang AST independently,
   compare actual AST/DWARF links. Targets F02.
3. **Qualified statics and shadowed tags:** two directories each with `main.c`,
   one missing debug DIE, macro same-line shadows and repeated TUs from one
   source file. Use GDB `info symbol`/DWARF offsets plus AST location as ground
   truth. Targets F03/R01.
4. **Fork/concurrency:** force worker allocation while another thread forks;
   external watchdog and raw event log distinguish deadlock, child sequence
   discontinuity and reconstructed history. Targets F04/R03.
5. **Writer failure:** fail the last log write, compare an independent event
   counter/fixture output to final `heap_history.complete`. Targets F05.
6. **Crash boundary:** crash with a mutated requested global exactly at
   snapshot cap; independent GDB memory/value and stop trace check capture
   completeness. Targets F06/F07.
7. **Stalled step:** >10s blocked step with >20s session deadline; compare
   externally measured inferior wait, MI result and JSON outcome. Targets F08.
8. **Mixed call site:** direct and macro allocator calls on one source line;
   instrument which branch executed and inspect AST spans and candidate status.
   Targets F09.
9. **Wrong-existing-ID parser injection:** change references to valid but
   different declaration/member/expression/event IDs; deep validation should
   reject contradictions. Targets F10.
10. **Output boundary:** exact-cap payload and cap+1 payload, fast exit and
    PTY stress; compare independent stdout bytes and trace statuses. Targets
    F11/R02.
11. **Optional-provider matrix:** Clang missing, partial, disabled, cancelled,
    all-TUs failed; independently assert unit statuses and summary semantics.
    Targets F12.
12. **Resource enforcement:** small trace/journal/output caps; fork descendants,
    pre-main log growth and disk-full simulation; measure actual bytes/RSS and
    process cleanup externally. Targets F13/F05 and best-effort limits.
13. **Cross-run diagnostic sites:** same-basename files and same-line multiple
    allocations; compare original Valgrind XML to trace links. Targets F14.
14. **Independent general oracle:** across compiler versions/optimization
    levels, compare GDB commands and raw MI stops, `readelf` DIE offsets,
    wrapper log, C program assertions, final JSON and TP-3 views. Include
    arrays/VLAs/bitfields, recursion, pointer reuse, incomplete traces,
    malformed references and output-size measurements. Do not use only
    AST-on/off equality as the correctness criterion.
15. **Step-limit stability:** configure a bounded deterministic loop with a
    session deadline comfortably above measured debugger start/step time;
    record steps issued and elapsed time independently. Repeat both isolated
    and serial-suite runs to distinguish test deadline pressure from a wrong
    stop/status transition. Targets R05.

## S. Recommended repair order

First correct F01/F02/F03 and F04/F05: they can invalidate identity,
provenance or execution/lifetime completeness. Next F06/F07/F08/F09/F10/F11/F12,
each with a narrow regression demonstrating the original failure. Test R01/R02
before deciding whether further changes are necessary. F13/F14 and R03 require
clear policy/contract decisions with tests, not a broad architecture rewrite.
Correct R04 when the resulting guarantees have been independently established.
Do not start the dedicated parser upgrade to address Phase 8A findings.

## T. Final verdict

**B - Repairs recommended before Phase 8B.** The architecture is not
fundamentally unsound, and there are meaningful safeguards for MI routing,
partial memory, heap lifetimes and old trace loading. Nevertheless F01/F02/F03
allow false identities and F04/F05 challenge instrumentation/lifetime
completeness under realistic conditions. Those targeted defects justify
correction before treating a large Phase 8B pass as evidence of production
readiness. Passing tests alone cannot establish execution truth without an
independent oracle.

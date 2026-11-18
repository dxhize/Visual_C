# Trace Engine V2.2: Phase 8R-1 Evidence Identity Repairs

Scope: the three P1 evidence-identity findings F01-F03 in
`docs/trace-engine-v2.2-audit-8a.md`. Existing uncommitted phases and schema 2
are preserved. This is not Phase 8R-2, Phase 8B, or a Trace Parser redesign.

## Baseline and method

- Branch `main`, commit `486410b39646ee8c4ad20a527260d3a58929e2d1`.
  Phases 1-7 and the audit were already uncommitted; no pre-existing edit was
  reset or discarded.
- Baseline audit (8A) independently observed 305 passing and one failing test
  in a serial full suite. `step_limit_bounds_execution` received `Timeout`
  instead of `StepLimitReached` under its five-second deadline, then passed one
  isolated rerun. Any recurrence here must be reported, not automatically
  attributed to these three repairs.
- For each identity defect: assert the incorrect old behavior with a focused
  regression, record a failing-before result, make only the local correction,
  and verify a passing-after result. GCC/DWARF, GCC/Clang preprocessing, or
  deterministic heap histories provide ground truth independent of the
  repaired code.
- AST-on/off equality is useful for runtime preservation but is not independent
  evidence that either trace is otherwise semantically correct. Time/RSS/JSON
  comparisons below must state their sample count and environmental limits.

## Root causes and corrections

### F01: numeric containment is not allocation origin

`src/memory.rs::pointer_target` wrote the numeric candidate
`address_match_allocation_id` into `allocation_id` when just one unfreed
lifetime occupied the address. Uniqueness among observed lifetimes does not
show how the pointer obtained that value. The assignment is removed;
`kind="heap"`, candidate ID, region, offset, extent, active/freed/reuse status,
and the actual allocation timeline remain unchanged. No pointer-assignment
history is invented.

- Before: `cargo test --lib
  memory::tests::unique_live_numeric_containment_is_not_pointer_provenance
  -- --exact` failed, 0 passed/1 failed: actual `Some(7)`, expected `None`.
- After: `cargo test --lib memory::tests:: -- --nocapture` passed 15/15;
  `cargo test --test heap_provenance -- --test-threads=1` passed 3/3.
- Synthetic fixed allocation histories assert unique active, freed/reused,
  in-place/moved realloc, numeric pointer reassignment, and caller-frame
  candidates; they also check unchanged region, offset, active and extent.

### F02: optimization and compiler-specific options

GCC's build prepends `-O0` and appends user flags, while Clang previously
discarded `-O0/-O1/-O2/-O3/-Og/-Os`, and rejected `-Ofast`. Clang now starts
at `-O0` and forwards supported optimization levels in the original user
order. `-D`, `-U`, include paths, forced includes and `-std` retain their
ordering. Unclassified optimization/target options still leave AST extraction
unavailable rather than silently dropping them.

Accepted GCC/Clang `-f*`, `-m*` or `-pthread` options may have different
preprocessing semantics even if Clang accepts the spelling. The new optional
`SourceMetadataRegistry.cross_provider_correlation_unavailable` records this
uncertainty; Clang syntax remains available, but declaration, member, type,
runtime-source, and heap-call cross-provider links are withheld in both AST
analysis and the final fusion pass. This is a conservative policy, not a claim
that every supported compiler option produces identical preprocessing.

- Before: `cargo test --test clang_ast
  gcc_optimization_preprocessing_matches_clang_ast_branch -- --nocapture`
  failed, 0 passed/1 failed: GCC selected `optimized_branch`, Clang selected
  `unoptimized_branch` at the same source declaration position.
- After: `cargo test --test clang_ast -- --nocapture` passed 8/8 after the
  optimization repair (10/10 after additional flag-compatibility tests);
  `cargo test --lib clang_ast::tests -- --nocapture` passed 6/6.
- Before the uncertainty gate: `cargo test --test clang_ast
  accepted_compiler_specific_flags_retain_ast_without_cross_provider_identity
  -- --exact --nocapture` failed, 0 passed/1 failed. The independently parsed
  AST retained source declarations with `-fshort-enums` but incorrectly linked
  them to DWARF. After the gate the focused test and eight Phase 7 fusion tests
  passed.

### F03: full file qualifier is stronger than a basename

`dwarf::ObservationTypes::enrich` previously accepted any declaration with
the same basename as a quoted GDB file qualifier. It could also fall back to
symbol lookup after an ambiguous location match. Qualified symbols now
require equal normalized absolute paths; a relative/incomplete path leaves
the relationship unresolved, and ambiguous location evidence never triggers
the weaker fallback. `dwarf::associate` already requires exact file/line/name
and was not weakened. The runtime value, address, scope and activation are
untouched when a link is withheld.

- Before: `cargo test --test dwarf_registry
  qualified_symbol_requires_full_file_identity -- --nocapture` failed because
  `'/a/common.c'::value` acquired the declaration/type from `/b/common.c`;
  `ambiguous_location_does_not_fall_back_to_a_symbol` also failed.
- After: `cargo test --test dwarf_registry` passed 12/12, including synthetic
  exact/normalized/incomplete path cases and real GCC compilation of two
  directories each containing `common.c` with same-name file-scope statics.

## Independent verification

- GCC and Clang were run independently with `-O0 -O2 -E -P` against
  `fixtures/c/phase8r1_opt.c`. Both select `optimized_branch` after the fix;
  GCC `-O0 -O2 -dM -E` defines `__OPTIMIZE__`, while previously invoked Clang
  `-O0 -dM -E` does not. The seven supported optimization levels were also
  compared for `__OPTIMIZE__`, `__OPTIMIZE_SIZE__` and `__FAST_MATH__` where
  applicable. This comparison addresses these macros, not every builtin.
- A separate real GCC/Clang `-E -P` test checks `-D`, `-U`, `-I`, `-include`
  and `-std=gnu11` on the same fixture: both select `defined_branch = 7`,
  `unoptimized_branch`, and the forced header declaration, with no removed
  macro. AST-disabled and missing/unsupported-Clang paths remain covered.
- Real GCC compilation of `phase8r1_a/common.c` and
  `phase8r1_b/common.c` yields two independent DWARF declaration IDs;
  qualified observations bind to their own absolute file, while bare
  `common.c` stays unresolved. Synthetic fixtures isolate the wrong-file case
  even when the correct declaration is absent.
- Deterministic heap event sequences provide an allocator-independent oracle
  for lifetime identity and address containment. They deliberately do not
  claim independent proof of pointer assignment history, which is unrecorded.

## Changed files and schema

Schema marker remains 2. One public optional source-registry status field was
added; no required field, migration, or Trace Parser interface changes. Other
changes are confined to evidence assignment and conservative optional links.
The new field defaults to absent when deserializing legacy traces.

Changed: `src/memory.rs`, `src/clang_ast.rs`, `src/dwarf.rs`,
`src/evidence/mod.rs`, `tests/clang_ast.rs`, `tests/dwarf_registry.rs`,
`tests/v2.rs`,
`fixtures/c/phase8r1_opt.c`,
`fixtures/c/phase8r1_{a,b}/common.c`, `fixtures/c/phase8r1_main.c`, and this
report. All other pre-existing worktree changes were preserved.

## Full tests and performance

Verification commands and results:

- `cargo check --all-targets`: pass.
- `cargo fmt --all -- --check`: pass.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `cargo build --release`: pass.
- First `cargo test --no-fail-fast -- --test-threads=1`: 314 passed, one failed.
  `tests/v2.rs::pointer_identities_follow_reassignment_aliases_and_stay_evidence_based`
  asserted the now-invalid historical premise that a numeric heap match has
  `allocation_id.is_some()` (`tests/v2.rs:1003-1012`). This was not a runtime
  regression or dismissed as flaky. The assertion now checks non-null
  `address_match_allocation_id` and absent definitive `allocation_id`.
- Focused rerun of that real GCC/GDB test passed 1/1.
- Final `cargo test --no-fail-fast -- --test-threads=1`: **315 passed, zero
  failed, zero ignored**, including legacy JSON/Trace Parser, dual-mode
  instruction, caller-frame/raw-memory, heap history and full Phase 7 fusion
  tests. No timing-sensitive failure appeared in this final run. The separate
  Phase 8A run had one five-second step-limit timeout, which remains a known
  timing-sensitive test, not an 8R-1 change.

Release benchmark (one sample per mode and case; do not read these wall-clock
differences as a stable speedup):

`python3 scripts/phase7_benchmark.py --binary
target/release/c-trace-engine --cases
pointer,heap,realloc,multiple_tu,ambiguous_heap --repeats 1 --directory
/tmp/kilo/8r1-post`

| Case | AST-off wall s | AST-on wall s | AST-on minus off JSON bytes | RSS delta KiB | Clang µs | Fusion µs | Validation µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| pointer | 3.19 | 3.11 | 7,512 | 52 | 111,803 | 53 | 6 |
| heap | 3.36 | 3.28 | 8,622 | 8,960 | 116,477 | 101 | 7 |
| realloc | 3.18 | 3.46 | 18,288 | 9,316 | 143,702 | 132 | 11 |
| multiple_tu | 3.11 | 3.38 | 7,001 | 80 | 133,774 | 88 | 12 |
| ambiguous_heap | 3.12 | 3.28 | 20,244 | 8,996 | 110,726 | 164 | 15 |

All five AST-off/on pairs passed both the recorded-runtime invariant checks
and normalized semantic-projection equality. That comparison explicitly
checks GDB command counts, snapshot and instruction counts, stop/checkpoint
positions, execution outcome, heap-event shape and normalized source/variable
evidence. The benchmark does **not** compare ASLR-dependent raw addresses or
the raw byte buffers, nor does configuration parity alone prove independent
runtime correctness. Real GDB memory-read and caller-frame tests in the full
suite exercise raw bytes and budgets separately; Phase 5 realloc/reuse tests
exercise distinct heap lifetime IDs. `tests/trace_parser_views.rs` passed
14/14, and the full parser suites passed, with no interface change.

For historical context, Phase 7's `/tmp/opencode/p7-release-final` measured
one-sample AST-on/off JSON deltas of 7,692 / 8,833 / 18,720 / 7,163 / 20,709
bytes for these same cases. Those are a prior run with different timing and
session paths, **not** a controlled before/after throughput benchmark. This
repair removes false pointer-origin IDs and adds an optional compatibility
reason only on unverified flag configurations; the default-flag benchmark does
not exercise that compatibility gate.

## Remaining limitations and 8R-2 handoff

The separate 8A findings F04/F05 (fork-sensitive wrapper locking and silent
terminal event-log write failures), along with its P2 findings, remain outside
8R-1. An address-only match may legitimately identify a numeric region, but
does not reconstruct a pointer's historical origin. GCC and Clang can still
disagree in unsupported tool/target configurations; evidence must remain
independent or unlinked rather than silently equated. Phase 8R-2 is not
implemented here.

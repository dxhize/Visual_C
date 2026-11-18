# Trace Engine V2.2 Phase 7 — Evidence Fusion and Trace Finalization

This is the final implementation report. Sections A–Q document repository
findings, the integration architecture, evidence contracts, benchmarks, tests,
and the extraction-gap ledger.

## A. Repository findings

`DebugSessionTrace` stores four independent evidence families:

* GCC-produced DWARF in the optional session-level `type_registry`.
* Optional Clang syntax in `source_metadata`.
* GDB observations in `snapshots` / `recording`.
* Intercepted heap events, reconstructed state and lifetimes.

Phase 7 adds a bounded post-processing reconciliation pass after runtime events
exist and before final profiling/serialization. It does not parse DWARF or
Clang ASTs, step GDB, reconstruct heap lifetimes, clone registries per
checkpoint, or reinterpret runtime values. Missing optional providers stay
optional. Schema version remains `2`; every new field is optional with a
`Default`/`serde(default)` path so V2.1 and Phase 1–6 traces still load.

### Relationship matrix (final)

| Relationship | Existing support | Phase 7 disposition |
| --- | --- | --- |
| Runtime variable → DWARF declaration | `VariableInfo.declaration_ref` + `type_ref`; one lookup built by `dwarf::associate`, also frame 0, caller frames and globals; unique file + declaration line + name + function + declared type | Preserved; no runtime fields touched |
| Runtime variable → AST declaration | None | Added via exact serialized DWARF declaration identity bridge (`source_declaration_ref`); collision → no link |
| Runtime activation identity | `activation_id` on variables/frames; recursive activations retained | Preserved unchanged |
| AST declaration → DWARF declaration | `SourceDeclaration.dwarf_declaration` + `correlation`; unique canonical file + name line/column + name + containing function + limited kind check | Preserved; line fallback only when a column is missing on one side |
| AST declaration → DWARF type | None | Added `dwarf_type_ref` from the correlated DWARF declaration |
| AST expression → source declaration | `SourceExpression.referenced_declaration`, `operand_ids`, declaration `initializer` | Preserved |
| AST expression → DWARF declaration/type | None | Added `referenced_dwarf_declaration`, `dwarf_type_ref`, `dwarf_type_evidence` |
| AST member → DWARF member | `MemberExpr.referenced_declaration` points at `FieldDecl` | Added `member_ref` + `base_type_spelling`; unique parent + member name required |
| AST allocation call → heap event | `HeapCallCandidate` for one direct allocator call of compatible operation on the resolved line; macros excluded | Preserved; multiple/macro-only lines now emit explicit `HeapCallAmbiguity` |
| Heap event → allocation lifetime | Event/lifetime allocation IDs, realloc old/new links, checkpoint intervals | Preserved unchanged |
| Pointer → allocation lifetime | `PointerTarget.allocation_id` / `address_match_allocation_id` / `provenance_status`; Phase 5 reuse rules | Preserved unchanged |
| Caller-frame variable → activation | `FrameVariables.activation_id`, frame identity, caller variables | Preserved; source bridge added per observation only |
| Heap state → checkpoint/event history | `observed_at_stop`, event intervals, block/lifetime identities | Preserved unchanged |
| Valgrind/sanitizer → primary allocations | File/line `related_allocation_ids` only | Preserved; no cross-run identity merge |
| DWARF specification/abstract origin | One same-CU `UnitRef` hop; `DebugInfoRef` and chains counted unresolved | Fixed: absolute `DebugInfoRef` cross-CU resolution with unit cache, 4-hop bound, cycle detection |

## B. Integration architecture

`src/evidence/` is the narrow reconciliation layer:

* `mod.rs` — lifecycle, shared indexes, summary and bounded orchestration.
* `declarations.rs` — DWARF ↔ AST declaration matching, type/member resolution
  and source-declaration member links.
* `source_links.rs` — expression declaration/type links, member-expression
  links and runtime-variable → source-declaration links.
* `heap.rs` — indexed source-call ↔ heap-event candidate/ambiguity matching.
* `validation.rs` — construction-time audit of every new reference; the Trace
  Parser keeps its own deep validation contract.

`crate::evidence::fuse` runs once in `trace.rs` after `dwarf::associate` and
the runtime heap history are available. It receives the existing canonical
trace, builds lookup tables once, appends only small optional references, and
records measured counts/times in `EvidenceFusionSummary`. It never runs at a
checkpoint and never issues a GDB command.

## C. Evidence contracts

| Evidence | Authority |
| --- | --- |
| GCC / DWARF | Compiled type layouts, member offsets, object sizes, CU identity, compiler declaration metadata |
| GDB | Observed values, addresses, frames, activations, execution locations, memory bytes, read outcomes |
| Heap instrumentation | Intercepted allocation events and recorded lifetimes within coverage |
| Clang AST | Source syntax, expressions, declaration relationships, ranges, operators, initializers |
| Valgrind / sanitizers | Supplementary diagnostics from possibly different executions |

A Clang layout estimate never replaces GCC DWARF. A source expression never
proves execution. Address equality never proves pointer origin. Static
declaration correlation never changes a runtime value, address, extent, read
status, checkpoint, stop ID or activation ID.

Certainty is represented with the existing conventions and one consistent
vocabulary, not a new overlapping enum:

* `unique_location` — one compatible DWARF declaration at the exact canonical
  file + name line/column.
* `unique_line_with_missing_column` — one compatible declaration at the
  canonical file + line when at least one side lacks a column; only used when
  no other candidate remains.
* `ambiguous` / `unresolved` — retained as existing status strings; no link is
  invented.
* `unique_dwarf_declaration_bridge` — runtime observation to AST declaration
  through the exact serialized DWARF declaration identity.
* `unique_parent_type_and_member_name` — AST member to DWARF member only after
  the parent struct/union is unique in that TU's DWARF CU and the member name is
  unique in it.
* `unique_direct_call_on_resolved_line_not_exact_execution` — existing heap
  candidate certainty, unchanged.
* `multiple_compatible_calls_on_resolved_line` /
  `only_macro_expanded_calls_on_resolved_line` — explicit heap ambiguity, not
  an event-to-expression claim.

## D. Declaration fusion

`clang_ast::correlate_declarations` builds two indexed lookups once:

1. `by_location`: canonical file + line + column + name + function → DWARF
   declarations. Used for exact matches.
2. `by_line`: canonical file + line + name + function → DWARF declarations.
   Used only when at least one side lacks a column.

Kind compatibility: `FunctionDecl` ↔ `subprogram`, `VarDecl`/`ParmVarDecl` ↔
`variable`/`parameter`. `FieldDecl` is never correlated by location; it is
correlated by parent type + member name in the fusion pass.

For `FunctionDecl`, declaration-only subprogram DIEs are preferred over
definitions when both share a location. The result is recorded in
`SourceDeclaration.correlation` with one of `unique_location`,
`unique_line_with_missing_column`, `ambiguous`, or `unresolved`.

Two same-name declarations in different lexical contexts remain distinct
because the function key differs. Two AST declarations claiming one DWARF DIE
stay unlinked rather than silently choosing one.

## E. Runtime-variable associations

`evidence::source_links::link_runtime` builds a bridge map from DWARF
declaration ID → AST declaration ID, then applies it to:

* frame-0 variables (`snapshot.variables`)
* caller-frame variables (`snapshot.caller_frames[*].variables`)
* globals (`snapshot.observation.globals`)

A runtime observation is linked only when exactly one AST declaration claims
the same exact serialized DWARF declaration ID. Activation IDs, values,
addresses, extents, read statuses and scope statuses are never modified.
Unmatched observations remain valid.

## F. Source-expression relationships

`evidence::source_links::link_expressions` propagates `referenced_declaration`
to `referenced_dwarf_declaration` and `dwarf_type_ref` when the referenced
source declaration has a unique DWARF link. For `MemberExpr` without a
declaration link, it attempts a unique parent-type + member-name resolution
against the TU's own GCC CU. For `sizeof(type)` and plain type spellings, it
attempts a unique named-type resolution. Pointer/array structural spellings
stay unresolved rather than being guessed.

## G. Heap/source correlation

`clang_ast::correlate_heap_calls` indexes direct allocator calls by canonical
resolved source line once. For each heap event with a resolved return address
and compatible operation, it looks up the indexed calls:

* Exactly one compatible direct call → `HeapCallCandidate` with evidence
  `unique_direct_call_on_resolved_line_not_exact_execution`.
* Multiple compatible direct calls → `HeapCallAmbiguity` with evidence
  `multiple_compatible_calls_on_resolved_line` (bounded to 8 IDs).
* Only macro-expanded calls → `HeapCallAmbiguity` with evidence
  `only_macro_expanded_calls_on_resolved_line`.

The AST never supplies runtime allocation arguments. Heap allocation IDs remain
the runtime authority. Reallocation lifetime transitions and address reuse are
preserved unchanged.

## H. DWARF reference improvements

`dwarf::OriginResolver` resolves `DW_AT_specification` /
`DW_AT_abstract_origin` chains across compilation units using `gimli`
`DebugInfoRef` + unit headers. Units are cached by absolute `.debug_info`
offset; chains are bounded by `MAX_ORIGIN_HOPS = 4` and cycle-checked with a
visited set. Same-CU `UnitRef` behavior is preserved. `Declaration.resolved_origin`
records the chain composition (e.g. `specification_same_cu`,
`abstract_origin_cross_cu`, `unresolved`).

Real GCC `-O2 -gdwarf-5 -flto` fixtures emit cross-CU `DW_AT_abstract_origin`
references; the resolver now completes them. Normal `-O2 -gdwarf-5` origins
remain same-CU. Unsupported forms and malformed chains stay explicit
(`unresolved_origin_references`), never silently dropped.

## I. JSON and reference changes

Additive optional fields, all with `#[serde(default,
skip_serializing_if = "Option::is_none")]` or equivalent:

* `VariableInfo.source_declaration_ref: Option<String>`
* `SourceDeclaration.dwarf_type_ref`, `dwarf_member`, `parent_type_spelling`
* `SourceExpression.referenced_dwarf_declaration`, `dwarf_type_ref`,
  `dwarf_type_evidence`, `member_ref`, `member_name`, `base_type_spelling`
* `SourceMetadataRegistry.heap_call_ambiguities`
* `DebugSessionTrace.evidence_fusion: Option<EvidenceFusionSummary>`
* Profiling: `evidence_fusion_time_us`, `evidence_validation_time_us`

Schema version remains `2`. V2.1 and Phase 1–6 traces load unchanged; the new
fields are absent and `evidence_fusion` is `None`.

Example enriched fragment (from `fixtures/c/ast_source.c`):

```json
{
  "source_metadata": {
    "declarations": [{
      "id": "tu0:d0", "kind": "VarDecl", "name": "p",
      "dwarf_declaration": "debug:u0:o1a",
      "correlation": "unique_location",
      "dwarf_type_ref": "debug:u0:o2b"
    }],
    "expressions": [{
      "id": "tu0:e0", "kind": "MemberExpr",
      "referenced_dwarf_declaration": "debug:u0:o1a",
      "member_ref": {"type_ref": "debug:u0:o3c", "member_index": 0,
                     "name": "value", "byte_offset": 0}
    }],
    "heap_call_candidates": [{"event_index": 0, "expression_id": "tu0:e1",
      "evidence": "unique_direct_call_on_resolved_line_not_exact_execution"}],
    "heap_call_ambiguities": [{"event_index": 1,
      "evidence": "multiple_compatible_calls_on_resolved_line",
      "expression_ids": ["tu0:e2", "tu0:e3"]}]
  },
  "evidence_fusion": {
    "dwarf_available": true, "ast_available": true,
    "declaration_links": 13, "declaration_type_links": 15,
    "member_links": 4, "expression_dwarf_links": 26,
    "expression_type_links": 67, "runtime_source_links": 265,
    "heap_call_candidates": 2, "heap_call_ambiguities": 3,
    "rejected_links": 0
  }
}
```

## J. Trace Parser compatibility

TP-1 deserialization, TP-2 indexes, TP-3 raw/semantic views, and deep
validation remain compatible. Additions:

* `TraceStore::source_declaration(id)` and `source_expression(id)` borrowed
  accessors.
* `RawTraceView::source_metadata()` returns the optional registry.
* `CheckpointContext::variable_source_declaration(variable)` returns the linked
  source declaration for a runtime observation.
* Deep validation now checks `AST_DECLARATION_REFERENCE_INVALID`,
  `AST_EXPRESSION_REFERENCE_INVALID`, `AST_HEAP_CANDIDATE_INVALID`,
  `AST_HEAP_AMBIGUITY_INVALID`, and `VARIABLE_SOURCE_DECLARATION_INVALID`.

No persistent duplicate indexes were added to `TraceStore`. No TP-1/TP-2/TP-3
redesign occurred.

## K. Performance

Benchmark: `python3 scripts/phase7_benchmark.py --binary
target/release/c-trace-engine --directory /tmp/opencode/p7-release-final`.

13 cases × 2 modes (AST off/on), 1 repetition each, serial. All 13 pairs
passed runtime-equality and semantic-projection-equality checks.

| Case | Wall Δ s | Trace Δ B | RSS Δ KiB | AST ms | Fusion µs | Validation µs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| arithmetic | -2.03 | 3,664 | 204 | 75.1 | 43 | 4 |
| pointer | -0.25 | 7,692 | 84 | 73.0 | 53 | 6 |
| aggregates | 1.88 | 11,417 | 0 | 668.2 | 126 | 8 |
| members | 0.27 | 19,854 | 9,284 | 121.4 | 143 | 12 |
| heap | 0.57 | 8,833 | 8,952 | 138.2 | 748 | 15 |
| ambiguous_heap | -0.10 | 20,709 | 9,424 | 147.0 | 165 | 12 |
| realloc | 0.05 | 18,720 | 9,352 | 109.4 | 152 | 29 |
| nested | 0.06 | 6,628 | -128 | 60.0 | 66 | 5 |
| multiple_tu | 0.04 | 7,163 | -80 | 93.7 | 74 | 7 |
| macro | 1.04 | 6,552 | 16 | 82.0 | 51 | 6 |
| macro_alloc | 0.19 | 12,935 | 9,572 | 120.8 | 105 | 8 |
| gnu | 0.27 | 5,902 | -132 | 78.3 | 46 | 5 |
| headers | 0.14 | 11,894 | 18,388 | 234.7 | 123 | 9 |

Median fusion time across cases: 123 µs. Median validation time: 8 µs. Fusion
is an inexpensive post-processing step relative to program execution tracing.
AST extraction dominates the added wall time and RSS (Clang startup, preprocessing,
JSON generation); fusion itself adds negligible cost. Trace growth is bounded by
the small additive reference fields.

## L. Evidence coverage (real `fixtures/c/ast_source.c` probe)

* DWARF declarations linked to AST declarations: 13
* Runtime variables linked to source declarations: 265
* Expression/member references established: 4 member links, 26 expression
  DWARF-declaration links, 67 expression type links
* Heap-call associations added: 2 candidates, 3 explicit ambiguities
* Ambiguous/unresolved relationships: 3 heap ambiguities retained explicitly;
  0 rejected links

These are actual counts from the probe, not percentages.

## M. Tests

Commands and results:

* `cargo fmt --all -- --check` — pass.
* `cargo clippy --all-targets -- -D warnings` — pass.
* `cargo build --release` — pass.
* `cargo test --no-fail-fast -- --test-threads=1` — **306 passed, 0 failed,
  0 ignored**.
  * `tests/evidence_fusion.rs`: 8 tests (A/C cross-CU LTO, D/E/F/O declaration
    flags, G/H/I/J real fusion, K/L heap candidates/ambiguities, M/N runtime
    bridge, P/Q legacy + invalid refs, R/S AST on/off fidelity).
  * `tests/clang_ast.rs`: 6 tests.
  * `tests/dwarf_registry.rs`: 8 tests.
  * `tests/trace_parser_views.rs`: 14 tests.
  * `tests/v2.rs`: 27 tests.
  * Plus Phases 1–6 suites.

Real GCC/GDB/Clang integration tests cover cross-CU LTO origins, struct member
correlation, array expressions, allocation expression fusion, multiple calls on
one line, repeated call execution, heap address reuse, realloc, macros, GNU
extensions, AST disabled, missing providers, invalid references, and legacy
traces.

No timing-sensitive failures occurred in this final run. The pre-existing
load-sensitive flake
`supervisor::tests::cancellation_wakes_stalled_process_wait_and_retains_partial_output`
is documented in the Phase 6 report and was not edited.

## N. Files changed

Source:

* `src/dwarf.rs` — `OriginFacts`, `OriginResolver`, cross-CU origin resolution,
  subprogram declaration retention, `ObservationTypes` filter.
* `src/evidence/mod.rs` — fusion orchestration and summary counting.
* `src/evidence/declarations.rs` — `DwarfIndex`, CU/type/member indexes,
  FieldDecl links.
* `src/evidence/source_links.rs` — expression/member/type links and
  runtime→source bridge.
* `src/evidence/heap.rs` — indexed heap-call correlation.
* `src/evidence/validation.rs` — construction-time audit.
* `src/clang_ast.rs` — declaration correlation, extraction fields, heap
  correlation/ambiguities.
* `src/models.rs` — optional wire fields, `EvidenceFusionSummary`, profiling
  fields.
* `src/trace.rs` — fusion hook after DWARF association and heap history.
* `src/trace_parser/validation.rs` — extended deep validation.
* `src/trace_parser/views.rs` — source lookup and variable-source accessors.
* `src/lib.rs` — `evidence` export.

Tests/fixtures/scripts:

* `tests/evidence_fusion.rs` — Phase 7 A–S matrix.
* `tests/clang_ast.rs`, `tests/dwarf_registry.rs`,
  `tests/trace_parser_views.rs` — extended coverage.
* `fixtures/c/phase7_lto_{a,b,main}.c`, `fixtures/c/phase7_lto_shared.h` —
  real cross-CU LTO fixture.
* `scripts/phase7_benchmark.py` — serial AST-on/off fusion benchmark.

Docs:

* `docs/trace-engine-phase7.md` — this report.

## O. Remaining limitations

* Runtime values, addresses, extents, VLA extents, initialization state, pointer
  history, active union members, exact event-to-expression execution and
  cross-run diagnostic identity remain runtime/intrinsic evidence questions.
* A trace may be structurally valid and still incomplete; AST-disabled traces
  are complete for their configured scope.
* Pointer/array structural type spellings are not parsed by the conservative
  expression-type fallback.
* Indirect allocator calls remain unresolved unless actual evidence establishes
  the target.
* Cross-run Valgrind/sanitizer identity is not inferred.

## P. Extraction-gap ledger (final Phase 7 triage)

| Gap | Source | Classification | Phase 7 action | Evidence | Remaining work |
| --- | --- | --- | --- | --- | --- |
| Cross-CU DWARF specification/abstract-origin resolution | DWARF | Extraction | Fixed | `OriginResolver` + LTO fixture test B/C | None |
| Missing declaration correlations | DWARF + AST | Extraction | Fixed | `correlate_declarations` + line fallback | None |
| Runtime-variable to static-declaration links | GDB + AST | Extraction | Fixed | `link_runtime` bridge | None |
| AST expression/member to DWARF references | AST + DWARF | Extraction | Fixed | `link_expressions` + `member_link` | None |
| Source-call to heap-event ambiguity | AST + heap | Intrinsic uncertainty | Improved | `HeapCallAmbiguity` records | Runtime evidence still required for exact attribution |
| GCC/Clang preprocessing differences | GCC + Clang | Compatibility | Documented | `clang_flags` rejects unknown flags | None |
| Runtime-dependent DWARF expressions | DWARF | Runtime | Unchanged | `runtime_dependent` flag | Future semantic engine |
| Unsupported GNU AST nodes | Clang | Compatibility | Documented | `unsupported_nodes` count | Wider Clang coverage |
| Platform/debug-format coverage gaps | DWARF | Compatibility | Unchanged | `unsupported_die_count` | Future platform work |
| Initialization and definedness uncertainty | GDB | Intrinsic uncertainty | Unchanged | `initialization_status` | Semantic engine |
| Pointer-origin ambiguity | GDB + heap | Intrinsic uncertainty | Unchanged | `provenance_status` | Semantic engine |
| Partial heap instrumentation | Heap | Runtime | Unchanged | `heap_history` coverage | Instrumentation coverage |
| Cross-run Valgrind/sanitizer correlation | Valgrind | Intrinsic uncertainty | Unchanged | `related_allocation_ids` only | Future semantic engine |

## Q. Phase 8 handoff — verification checklist

* Compiler compatibility: GCC 16.2.1, Clang 22.1.8, GDB 18.1 on Fedora.
* Execution fidelity: 13/13 benchmark pairs runtime-equal and
  semantic-projection-equal.
* DWARF extraction: cross-CU origins resolved; same-CU behavior preserved.
* AST extraction: optional, off-by-default, per-unit status explicit.
* Memory coverage: unchanged from Phase 4.
* Heap lifetimes: unchanged from Phase 5.
* Cross-source references: all new links validated at construction and by deep
  validation.
* Backward compatibility: V2.1 and Phase 1–6 traces load; schema stays 2.
* Resource exhaustion: existing limits preserved; fusion bounded.
* Trace-size behavior: additive fields only; median growth 3.6–20.7 KiB.
* Performance regressions: fusion median 123 µs; AST extraction dominates.
* Known flaky tests: one pre-existing load-sensitive supervisor timing test.
* Unsupported platforms: non-GCC/ELF targets unverified.
* Unresolved fixable gaps: none remaining in Phase 7 scope.

Phase 8 (dedicated Trace Parser upgrade) must not begin until this report is
reviewed.

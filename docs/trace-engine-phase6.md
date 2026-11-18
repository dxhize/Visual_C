# Trace Engine V2.2 Phase 6 — Clang Source Evidence

## Integration boundary (before implementation)

`BuildConfig` supplies the primary and additional user translation units,
include directories, defines, compiler flags and working directory.
`compiler::compile_project` builds the actual GCC debug binary with a generated
allocator translation unit and a separate sanitizer binary. The generated
translation unit and object files are **not** Clang source inputs.

After successful GCC compilation, and independently of GDB, an optional
bounded Clang syntax-only pass can analyze each configured user C translation
unit. It must translate source-semantic GCC options conservatively: rejected
or omitted interpretation-affecting flags make the AST unavailable rather than
silently produce a different parse. Tool errors never change GCC compilation,
DWARF, tracing, or runtime observations.

The provider attaches one compact optional registry to `DebugSessionTrace`,
alongside the existing compilation-scoped GCC `TypeRegistry`. Source syntax
never overwrites DWARF layouts or GDB/heap runtime facts. Correlation is limited
to unique, sufficiently located declarations and calls; ambiguity remains
explicit. TP-3 borrows the same registry from the canonical trace, without a
new index or per-snapshot copies.

The remainder of this report records implementation and verification results.

## A. Repository findings

`src/compiler.rs` passes `BuildConfig.defines`, `include_dirs`, and
`compiler_flags` to GCC before user sources, the generated allocator C source,
object files and linker settings. Extra C sources are separately compiled TUs
within the same GCC link command. No compile database or per-TU flags exist.
`src/trace.rs` retains original user paths through compilation, extracts the
GCC binary's `TypeRegistry` once, then performs GDB tracing. `HeapEvent`
retains `source_file`, `source_line`, raw caller PC and interval attribution,
but not a call-site column. TP-3 views borrow one canonical `DebugSessionTrace`.
`supervisor::output` already bounds subprocess output and owns a process group.

## B. Clang feasibility

Clang 22.1.8 on this host accepted `-Xclang -ast-dump=json -fsyntax-only`
for a file with `<stdlib.h>`, a struct, `malloc(count*sizeof(int))`,
subscript, pointer addition, member access and `free`. The AST JSON was
1,516,178 bytes. Observed node kinds include `FunctionDecl`, `VarDecl`,
`CallExpr`, `BinaryOperator`, `UnaryExprOrTypeTraitExpr`, `ArraySubscriptExpr`,
`MemberExpr`, `DeclRefExpr` and `ImplicitCastExpr`. Nodes expose `range`
begin/end offsets and token lengths, often omitting repeated line/file fields.
Declaration references use pointer-like hexadecimal IDs (not portable IDs);
implicit casts wrap direct callees and arguments. Header declarations dominate
the document and sometimes omit a repeated header file name while retaining
`includedFrom`. Macro spellings and expansions have distinct location objects.
The parser treats all fields as optional, and does not promise compatibility
with arbitrary future Clang JSON versions.

## C. Architecture

`src/clang_ast.rs` contains the narrowly scoped optional provider: conservative
flag selection, bounded supervised Clang invocation, JSON filtering, a compact
registry, and two deliberately small correlation passes. It runs only after
successful GCC compilation and DWARF extraction, once per user TU, before
GDB; it does not analyze the generated wrapper or object-only inputs.
`--clang-ast` / JSON `clang_ast: true` enables it; `--clang-path` / JSON
`clang_path` selects the executable. It is off by default to keep the
previous trace/runtime budget unchanged. Missing Clang, rejected flags or
source, malformed JSON, output caps and deadlines are recorded in per-unit
status/reason without changing GCC/GDB behavior. A completed Clang pass that
encounters unsupported user expression kinds is marked `partial`, retaining
supported child records and an unsupported-node count rather than claiming
full extraction.

## D. Registry and K. JSON

Optional `DebugSessionTrace.source_metadata` holds a
`SourceMetadataRegistry`: provider and probed Clang version, TU statuses and
paths, declarations,
expressions, conservative heap-call candidates and limitations. `AstUnit`
records output bytes, traversed/unsupported nodes, JSON parse time and
registry-construction time; the registry records declaration and heap-candidate
correlation time separately. `SourceSpan` records a
canonical file, half-open source offsets, line/column endpoints and
`direct`/`macro_expansion` provenance with spelling offset/file when present.
Every retained declaration/expression receives a deterministic trace-local
`tuN:dM` / `tuN:eM` ID from TU and traversal order, never a serialized Clang
pointer ID. Cross-references to retained records are optional. No full AST
tree or expression list is copied into a checkpoint. Schema version stays 2;
absence of the optional registry in old traces is preserved. Example:

```json
{
  "source_metadata": {
    "provider": "clang-ast-json",
    "translation_units": [{"id": "tu0", "status": "complete",
      "file": "/project/main.c", "ast_bytes": 1516178,
      "nodes_visited": 2389, "parse_time_us": 15123, "reason": null}],
    "expressions": [{"id": "tu0:e12", "kind": "CallExpr",
      "callee": "malloc", "call_kind": "malloc",
      "operand_ids": ["tu0:e13"], "span": {
        "file": "/project/main.c", "start_offset": 42, "end_offset": 57,
        "start_line": 3, "start_column": 5, "end_line": 3,
        "end_column": 19, "provenance": "direct"}}]
  }
}
```

This is illustrative, not a claim that the numeric offsets above describe a
particular benchmark source file.

## E. Coverage matrix

| Construct | Recorded evidence | Limit |
| --- | --- | --- |
| Function/variable/parameter/field | Source identity, name, spelling type, lexical function, initializer reference | No execution/initialization claim |
| Direct/indirect call | Callee name if resolvable, call classification, child expression references | Header declaration required for standard allocator classification |
| `sizeof` | Type-vs-expression form, type operand when present, child expression references | No GCC layout or runtime VLA extent inference |
| Assignment/arithmetic | `BinaryOperator`/`CompoundAssignOperator`, opcode and operand references | No runtime pointer history |
| Unary/array/member | Opcode, base/operand references, `.` vs `->` | No bounds, validity or GCC member-offset inference |
| Initializer/cast/conditional/literal | Syntactic kind and relationships when retained | No static expression evaluator |
| Macro | Expansion span and spelling offset/file when available | Multiple expansion paths and inlining may remain ambiguous |

## F. Allocation calls; G. pointer/aggregate syntax

`malloc`, `calloc`, `realloc`, `free` are classified only if the direct
`DeclRefExpr` identifies a header declaration observed in that TU.
User-defined same-name functions and indirect calls remain other/unknown.
Call operands link to selected child expressions through implicit cast wrappers.
The real fixture includes multiple malloc calls on one line, `calloc`,
`realloc`, `free`, `sizeof(*q)`, address-of/dereference, pointer addition,
subscripts, struct field assignment, nested shadows and initialization.
Syntactic sizes and operators never override instrumented requested sizes,
observed addresses or GDB memory bytes.

## H. DWARF and I. heap-event correlation

A source declaration links to one GCC DWARF declaration only if name,
canonical file, declaration-name line and column match uniquely (with kind
and containing-function restrictions). Name coordinates are distinct from
the full declaration's source range. Multiple candidates are marked
`ambiguous`, and no candidate is invented to resolve them. This is *not*
broad Phase 7 metadata fusion. `HeapCallCandidate`
records a unique direct allocator call of compatible operation kind on an
event's resolved call-site line. It explicitly says
`unique_direct_call_on_resolved_line_not_exact_execution`: return-PC source
resolution lacks a call column, and a single line may contain multiple calls.
Macro-expanded calls are excluded from this narrow match; no event/lifetime or
checkpoint attribution is replaced.

## J. GCC compatibility

Clang receives the configured defines, includes, working directory, source
paths and selected user compiler options; GCC's linker/wrap/sanitizer and
generated instrumentation flags are excluded. Language/preprocessing options
are forwarded for Clang to validate; diagnostic/optimization flags that do not
change source interpretation are omitted. Unknown options are rejected rather
than silently ignored. This is not a full GCC-flag compatibility database:
Clang disagreement or unsupported GNU constructs mark AST unavailable while
the successful GCC debug build and runtime trace proceed. Object-only inputs
cannot yield source AST records.

## L. Trace Parser compatibility

`RawTraceView::source_metadata()` and
`SemanticInputView::source_metadata()` borrow the same optional registry;
TP-1 ownership, TP-2 indexes and evidence references are unchanged. Deep
validation checks registry IDs and declaration, expression and heap-candidate
references without inventing a new AST index. Older schema-2 traces load
without a registry.

## M. Resource control

Per-TU source text is limited to 1 MiB; Clang stdout and stderr are each
captured up to 8 MiB with truncation reported; subprocesses have a 3-second
per-TU deadline, a best-effort 512 MiB Clang address-space `RLIMIT_AS`,
an 8-second total budget and the existing session cancellation
mechanism. At most 16 user TUs, 100,000 traversed nodes per TU, depth 96,
2,048 declarations, 4,096 expressions and 256-byte retained strings are
accepted. JSON parsing is bounded by input size and Serde's recursion limit;
line/column lookups use a precomputed line index, and traversal checks its
deadline/cancellation at bounded intervals. Heap-call candidate correlation
has a separate 250 ms cap with an explicit limitation if stopped.
Unsupported/malformed trees fail to `unavailable` or `partial` rather
than supplying fabricated completeness. Header AST declarations are not
retained (except their IDs transiently for allocator recognition). The
registry is one static trace-level object.

## N. Performance

The release benchmark ran ten matched on/off workloads twice each, serially
with identical inputs and settings. Full traces, stderr, per-run metrics and
`comparison.json` are in `/tmp/opencode/p6-release-final/`. The projection
normalizes relocated addresses; it reports one unresolved code address per
run rather than claiming exact address equivalence. **20/20 paired runs**
matched source/instruction/checkpoint projections, variable/pointer
representations, heap-event sequences, gaps, output, exit status, GDB command
counts, snapshot counts and instruction counts. No extra GDB queries occurred.

| Workload | Median Clang+extraction ms | Median JSON parse ms | AST stdout KiB | Declarations / expressions | Median added trace KiB | Median peak RSS delta KiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Arithmetic | 92.2 | 0.2 | 9.8 | 2 / 4 | 3.0 | 116 |
| Pointer assignments | 60.1 | 0.3 | 18.1 | 4 / 10 | 6.4 | 86 |
| Arrays and structs | 106.6 | 0.4 | 25.7 | 5 / 15 | 9.0 | 8 |
| Heap allocation | 108.1 | 17.7 | 1,472.5 | 2 / 14 | 7.5 | 9,126 |
| Realloc-heavy | 108.5 | 18.1 | 1,515.9 | 4 / 31 | 15.8 | 9,100 |
| Nested scopes | 63.6 | 0.3 | 17.5 | 4 / 8 | 5.5 | 186 |
| Two TUs | 118.5 | 0.3 | 18.1 | 4 / 8 | 5.8 | 24 |
| Macro-heavy | 66.7 | 0.3 | 26.1 | 2 / 9 | 5.3 | 170 |
| GNU statement expression | 62.4 | 0.2 | 17.9 | 3 / 7 | 4.8 | 40 |
| Header-heavy | 169.6 | 34.5 | 2,622.7 | 3 / 19 | 10.2 | 18,464 |

The median Clang+extraction cost was **99.4 ms per run** across the ten
workloads (one-time static work, not per-checkpoint). The median paired
wall-time delta was **+77 ms**, but paired wall deltas varied in sign and
magnitude across full GDB
runs, so this is not a causal speedup/slowdown estimate. Clang startup,
preprocessing and AST generation are included in the Clang+extraction metric;
JSON parsing is reported separately, not counted twice. There is no separate
Clang-startup clock from the supervised output API. Header-heavy inputs
inflate temporary AST JSON and peak RSS markedly, even though the retained
registry remains small; a smaller Clang AST filter would require separate
coverage evaluation before adoption.

## O. Tests

Real Clang source extraction, macro ranges, multi-TU identity, same-line
ambiguity, output limits, malformed output, missing Clang, GCC-only-flag
fallback, unsupported GNU nodes, runtime on/off evidence and Trace Parser
validation are in `tests/clang_ast.rs` (6 tests, all passed).
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and the
release build passed. The final uncontended serial
`cargo test --no-fail-fast -- --test-threads=1` completed **292 tests,
zero failures** (180 library, 111 integration, one doc test); its output is
`/tmp/opencode/phase6-clean-tests2.log`. An earlier serial run failed one
pre-existing Phase 2 supervisor timing test
(`cancellation_wakes_stalled_process_wait_and_retains_partial_output`), which
is untouched by Phase 6, passed 5/5 individually, and passed in the final
clean suite; that flake is a load-sensitive 100 ms grace period, not a
Phase 6 regression.

## P. Files changed

`src/clang_ast.rs`, `src/config.rs`, `src/cli.rs`, `src/lib.rs`,
`src/models.rs`, `src/trace.rs`, `src/trace_parser/views.rs`,
`src/trace_parser/validation.rs`, `tests/clang_ast.rs`,
`tests/integration.rs`, `fixtures/c/ast_source.c`,
`fixtures/c/ast_second.c`, `fixtures/c/ast_gnu.c`,
`scripts/phase6_benchmark.py`, `README.md`
and this report.
Existing uncommitted Phases 1–5 and Trace Parser changes are preserved.

## Q. Limitations; R. extraction-gap ledger

| Gap | Classification | Phase 6 disposition |
| --- | --- | --- |
| Allocator expressions and `sizeof` operands | Partially improved | Static syntax linked, runtime operands still unknown |
| Pointer assignments, subscripts, member access | Partially improved | Source relationships only; no execution or validity claim |
| Shadowed declaration source locations | Partially improved | TU/offset/lexical-function identities separate; cross-compiler links require unique file/line/column |
| DWARF cross-CU origins, runtime-dependent expressions/layout | Unresolved | Clang cannot replace compiled GCC layout or evaluated runtime state |
| Macro spelling/expansion | Partially improved | Both locations retained when present; optimizer/inlining obscures runtime call correlation |
| GCC-only syntax and flags Clang cannot parse | Implementation/platform gap | Optional provider fails explicitly without degrading GCC execution |
| Header functions, system wrappers and indirect calls | Partially improved | Standard allocator names require header declaration evidence; custom allocators not inferred |
| Missing pointer-origin history / event-to-stop precision | Intrinsic evidence limitation | AST has syntax, not the actual executed pointer path or exact stop |
| Reused heap addresses, cross-process and cross-run diagnostics | Unresolved | Phase 5 uncertainty maintained unchanged |
| Complex declaration-only/definition and GCC/Clang scope reconciliation | Phase 7 implementation gap | Unique-location match only; ambiguity explicit |
| AST JSON/version changes, wider target support | Implementation gap | Clang 22 verified; defensive parsing/fallback, other versions unverified |
| GNU statement expressions and other unhandled source nodes | Implementation gap | Supported children retained; TU marked `partial` with unsupported-node count |

## S. Phase 7 handoff

Review candidate declaration matches across DWARF compilation units and
source TU contexts, missing type/declaration links, macro/inlining ambiguity,
source-expression references and evidence completeness without changing
runtime truth. Audit schema-2/Trace Parser compatibility before any broader
fusion. No Phase 7 implementation, Semantic Engine, or UI work is part of this
phase.

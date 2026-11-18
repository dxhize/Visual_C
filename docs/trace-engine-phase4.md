# Trace Engine V2.2 Phase 4

## A. Repository Findings

The existing pipeline is GCC compilation, one-time DWARF registry extraction,
GDB/MI2 execution and snapshots, allocation-timeline/independent analyses, and
schema-2 `DebugSessionTrace`. `capture_state` previously read frame-0 variables;
`push_snapshot` read live heap blocks; `capture_global` read fresh global bytes;
configured `selected_memory` sampled specified regions at instruction events.
GDB's existing lexical probe already supplies fresh `value.type.sizeof`, including
VLA extents when GDB can evaluate them. Static type/address/size metadata can be
cached within a module epoch; runtime values and bytes cannot.

## B. Multi-Frame Capture

`ExecutionSnapshot.variables` remains frame-0-only. Optional `caller_frames`
holds frame metadata, activation ID, variable observations and query errors at
source checkpoints, in nearest-caller-first order. The installed GDB Python
lexical probe walks `gdb.newest_frame().older()` and evaluates symbols using
`symbol.value(frame)`; it never changes the selected frame/thread. A failed
caller query retains frame-0 observations and records its error. Lexical shadows
have distinct scope identities; recursive calls have distinct activation IDs.
Pointer enrichment separately considers live, correctly associated caller
objects, preserves heap precedence, and leaves overlapping candidates unknown.

The bounded stack query requests one sentinel frame and retains at most 200.
When the root is missing, activation continuity cannot be proven: caller capture
is omitted and frame-0 automatic identities/activation IDs remain unknown while
actual values, addresses and bytes are retained. An explicit error marks the
gap; the next complete chain receives fresh IDs. This avoids false identity
reuse during recursion deeper than the cap.

## C. Memory Observation

Frame-0/caller variables, live heap blocks and globals use one bounded,
evidence-aware read helper. A compatible successful read can be reused within
the same stopped checkpoint, never across stops. Aggregate reads can exceed
one command but are chunked to the original per-command cap. The GDB/MI parser
validates ranges, offsets, hex and arithmetic and retains only a contiguous
prefix; holes and short responses cannot become complete observations. A read
failure midway through an aggregate preserves prior bytes and its diagnostic.
Nested structs, unions and arrays remain one raw object buffer, not duplicated
field buffers or an interpreted C object graph. Explicit instruction-memory
regions also retain partial prefixes and errors.

## D. Runtime Extents And Types

A small immutable `ObservationTypes` lookup is built once from the Phase 3
registry. Unique file/line/name/function declaration matches attach existing
`type_ref`/`declaration_ref` early enough to guide reads. Actual GDB sizes win;
`runtime_size_bytes` and `size_source: "gdb"` identify fresh runtime extents.
Only a correlated non-dynamic type with an observed address may supply a missing
static `size_bytes` (`size_source: "dwarf"`). Missing/cyclic type links and
expression-valued sizes stay unknown. Runtime VLA sizes belong to each actual
activation: no source-syntax bounds, DWARF expression evaluator, or fabricated
dimensions are used. The size of a pointer object never sizes its pointee.
Arrays of pointers are never classified as scalar pointers by array decay.

## E. Globals And Statics

Global and file-static contents are read anew at each checkpoint; same-named
file statics retain file-qualified identities. Function statics are learned
when their owner scope is observed, then remain discoverable after return.
Explicitly requested function-qualified symbols can be read before owner entry
when GDB `info address` proves static storage. A proven static alias has the
same declaration, address and extent; its canonical global-symbol identity is
stable before, during and after owner execution. Alias metadata is scoped to
the module epoch; contents are never cached. An inaccessible declaration is not
invented as a runtime object.

## F. Resource Budgets

| Limit | Default | Role |
| --- | ---: | --- |
| `max_variables_per_snapshot` | 24 | Frame-0 variables |
| `max_caller_frames` | 0 | Additional frames; zero preserves Phase 3 mode |
| `max_caller_variables` | 24 | Total additional variables, not per frame |
| `max_extra_memory_reads` | 32 | Additional caller read commands; zero means unlimited |
| `max_memory_read_bytes` | 4096 | Single command and ordinary-object/heap cap |
| `max_aggregate_bytes` | 4096 | Aggregate object cap; expanded benchmark uses 16384 |
| `max_snapshot_memory_bytes` | 1048576 | Shared checkpoint byte budget |
| `max_heap_blocks_per_snapshot` | 512 | Live heap objects retained per checkpoint |
| `max_globals` | 32 | Selected/discovered globals |
| `max_snapshot_count` | 1000 | Source checkpoints |
| `max_trace_memory_bytes` | 33554432 | Recorder retention |

Bytes are prioritized deterministically: frame 0, nearest eligible callers,
live heap timeline order, then selected/discovered globals. Cached bytes count
against retained-byte budget, not the extra read-command quota. A short failed
read counts as a command but only actual bytes are charged. Exact-fit budgets
are not automatically truncation. Named limits are recorded after *all* groups,
including globals, report omitted/partial evidence. Existing deadlines,
step/trace limits, cancellation and partial-trace behavior remain in place.

## G. JSON Contract

Schema version remains 2. Additive optional fields include `caller_frames`,
`runtime_size_bytes`, `size_source`, `initialization_status`, variable and heap
`memory_evidence`, and `memory_evidence.reason/limit`. Default new limits and
empty caller collections are omitted. Old frame-0 `variables[]` semantics,
checkpoint/stop/allocation identifiers, and parser indexes remain unchanged.
One capped aggregate observation looks like:

```json
{
  "runtime_size_bytes": 20000,
  "size_source": "gdb",
  "initialization_status": "unknown",
  "memory_evidence": {
    "extent_bytes": 20000,
    "requested_bytes": 16384,
    "captured_bytes": 16384,
    "status": "truncated",
    "reason": "per-object memory limit",
    "limit": "max_aggregate_bytes"
  }
}
```

`requested_bytes` counts actual requested/cached bytes, not forbidden reads;
`captured_bytes` is the retained contiguous prefix. Possible statuses are
`full`, `partial`, `truncated`, `unreadable`, `unavailable`, `unattempted`, and
`unknown`. Initialization is always explicitly unknown: lexical activity and
readable storage do not prove a defined initialized C value. No store tracking
or initializedness engine is part of Phase 4.

## H. Trace Parser

TP-1 ownership, TP-2 indexes and TP-3 views remain. Borrowed
`TraceStore::caller_frames(snapshot)` and `CheckpointContext::caller_frames()`
plus caller evidence references expose new observations without cloning the
trace. Deep validation checks frame/activation associations, pointer target
identity/extent/offset and captured byte counts. Missing evidence is partial,
not fabricated corruption. Older valid V2.1/Phase 1-3 traces deserialize;
obsolete V2 examples with removed register-limit configuration fields remain
incompatible under the preexisting strict configuration contract.

## I. Correctness Checks

Final verification: `cargo check --all-targets`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo build --release`, and
`cargo test -- --test-threads=1` all passed. The serial suite contained **262
passing tests**, zero failures, zero ignored: 159 library units, 1 DWARF
feasibility, 8 registry, 12 engine integration, 11 Phase 4 runtime, 1 deep
recursion, 13 TP-1, 15 TP-2, 14 TP-3, 27 V2 lifecycle and 1 doctest.
`python3 scripts/phase4_benchmark.py self-test` also passed; `git diff
--check` was clean. No flaky test occurred in this final run. Earlier
timing-sensitive tests have historically flaked under concurrent load, hence
the serial run.

Focused real GCC/GDB tests cover recursion,
shadowing, cross-frame pointers, actual aggregate bytes, VLA sizes, globals,
statics, heap refresh, resource caps, control-flow fidelity, deep stack overflow,
signals and partial traces. MI mocks cover caller-query errors, unchanged
selected frame, malformed/partial memory, read caps, caching and unavailable
addresses. Final test totals and benchmark deltas are recorded below when the
final acceptance pass finishes.

## J. Benchmarks And K. Coverage

`scripts/phase4_benchmark.py` retains actual Phase 3 binary SHA-256
`a426ba752684bd2f317839b831b80462c6f4d316003663ce5fa04f1428e2e4ba`
(original mtime 2026-11-17 15:05:49 UTC). Eleven serial release workloads include
arithmetic, nested calls, recursion, arrays/structs, an initialized 8 KiB
aggregate, VLAs, globals, pointers, heap, loops and 1,003 checkpoints.
Compatibility mode uses the same common configuration; expanded mode enables
three caller frames/24 variables/32 caller reads
and a 16 KiB aggregate cap. Complete traces, timing, RSS, command categories,
bytes, frame counts, semantic comparisons and sources/configs are preserved at
`/tmp/kilo/phase4-expanded/` for the current process lifetime. The previously captured
Phase 3 baseline artifacts were lost when temporary storage was cleared between
sessions; the SHA above is historical provenance, not a presently available
comparison executable. Accordingly the current run compares Phase 4
compatibility with Phase 4 expanded evidence only, and **does not claim a
Phase 3 before/after delta**. One run per case is descriptive, not statistically
significant. Phase 3 lacks explicit requested-byte status: its missing counts
are **unavailable**, not zero. Padding/uninitialized byte equality is not
claimed; unknown absolute addresses are not wildcarded.

Actual reproducible command:
`python3 scripts/phase4_benchmark.py current --directory /tmp/kilo/phase4-expanded`.
The machine-readable per-case deltas, full traces, exact configs, and report
are in `comparison.json`, the two suite directories, and `report.md` there.

| Workload/mode | Commands | Caller probes | Memory reads | Capture seconds | Debugger seconds | Wall seconds | Trace bytes | Engine peak RSS KiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Nested calls, compatibility | 131 | 0 | 31 | 0.0617 | 0.9011 | 3.436 | 83,318 | 6,896 |
| Nested calls, expanded | 167 | 14 | 53 | 0.0612 | 0.8344 | 3.198 | 109,719 | 7,000 |
| Recursion, compatibility | 1,546 | 0 | 395 | 0.7826 | 2.1546 | 4.532 | 1,393,246 | 8,960 |
| Recursion, expanded | 3,229 | 567 | 1,511 | 1.5510 | 2.9281 | 5.240 | 2,502,991 | 10,696 |
| Large aggregate, compatibility | 59 | 0 | 6 | 0.0331 | 0.7796 | 3.231 | 87,405 | 6,916 |
| Large aggregate, expanded | 65 | 0 | 12 | 0.0363 | 0.7776 | 3.082 | 135,787 | 6,872 |

Across eleven workloads in this single run, expanded mode added **1,813 GDB
commands**, including **617 caller probes** and **1,196 memory-read commands**;
capture time summed **0.858 seconds** higher, debugger time **0.832 seconds**
higher, and trace serialization **1,249,002 bytes** larger. Summed wall time
was **0.303 seconds** higher, but timing varied by case and is not a statistically
supported speed claim. Peak RSS is a per-process maximum, not additive: the
recursion case changed from 8,960 to 10,696 KiB. All eleven paired projections
matched execution outcome, source/instruction/stop sequence, heap events, gaps,
and program-output byte count. The two runs of the existing aggregate fixture
both exited 1, and the existing pointer fixture exited 45; CLI success is not
synonymous with inferior exit code zero.

Coverage deltas: **617** additional observed caller frames, **1,190** additional
variable observations, **29,336** additional requested and captured object
bytes, and **3** additional identity-backed cross-frame pointer observations.
The initialized 8 KiB aggregate alone gained **24,576 bytes** across six
checkpoints and changed **six truncated observations to six complete ones**.
Nested calls gained 14 caller frames, 22 variables, 88 bytes and three
identity-backed pointers; recursion gained 567 caller frames, 1,116 variables
and 4,464 bytes. The existing GDB VLA extent observations (4, 12, 20 bytes)
were present in both modes: **zero new distinct runtime VLA extents**, not a
fabricated gain. Frame-0 value comparisons differed at uninitialized recursion
stops (unknown definedness), and pointer-resolution comparisons differed where
new caller evidence resolved previously coarse stack targets. Expanded memory
limits differ as intended in the large-aggregate run; none of these differences
changed the measured execution control flow.

## L. Extraction-Gap Ledger

| Gap | Classification | Disposition |
| --- | --- | --- |
| VLA size, non-top-frame locals | Phase 4 runtime observation | Fresh GDB evidence when available; unresolved remains unknown |
| Missing/unreadable bytes, initializedness | Fundamentally uncertain from available evidence | Prefix/error or unknown; bytes do not establish definedness |
| Cross-CU origins, location-less declarations, missing DWARF correlation | Targeted DWARF work | No name-only correlation or unsupported expression evaluation |
| Expression-valued DWARF bounds/member locations, unsupported tags | Targeted DWARF work | Runtime size may complement static gaps, not replace them |
| Source-level syntax, macros and expression relationships | Phase 6 AST enrichment | Deferred; not authoritative runtime evidence |
| Non-GCC/non-ELF targets, endian/width variants | Platform support | Unverified outside GCC DWARF-v5 x86-64 ELF |
| Active union member, stale pointer ownership | Fundamentally uncertain from bytes | No guessed interpretation or historical provenance |
| GDB Python absent, >200 stack frames | Debugger/platform and resource cap | Explicit errors; no selected-frame fallback or fabricated activations |
| Function static before owner entry | Phase 4 runtime observation | Explicit qualified symbol works if GDB proves static storage; automatic discovery waits for entry |
| Journal reconstruction parity | Later parser work | Complete JSON remains authoritative; JSONL is records-only |

Phase 3's section extraction is bounded but not hard-real-time; unsupported
DWARF/platform cases are not declared solved by runtime reads.

## M. Phase 5 Handoff

Existing heap allocation IDs and wrappers are unchanged. Later work must
evaluate realloc provenance, freed-address reuse, stale pointers, and the
uncertainty in heap-event/source-checkpoint correlation. Live address containment
is not proof of historical object ownership. Phase 5, AST, metadata fusion,
Semantic Engine, registers and GUI are not implemented here.

Phase 5's heap-lifetime work and updated extraction-gap ledger are documented
in [trace-engine-phase5.md](trace-engine-phase5.md).

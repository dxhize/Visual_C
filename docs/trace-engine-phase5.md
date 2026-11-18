# Trace Engine V2.2 Phase 5: Heap Provenance

## A. Repository Findings

The debug GCC build links `c_runtime/alloc_tracker.c` with `--wrap` for malloc,
calloc, realloc and free. The sanitizer binary and Valgrind pass run separately.
The fixed-size C address table and unbuffered dedicated log avoid allocation in
the logging path. The Rust `EventFileReader` tails complete lines at debugger
stops; `HeapTimeline` retains process-local event IDs, live address/ID maps,
blocks, raw caller PCs, and heap-usage totals. Source resolution batches
`addr2line`; checkpoints record cumulative retained heap-event counts.

The Phase 4 baseline had a 131,072-slot C address table, a `max_heap_allocations`
block cap, an event cap of four times that block cap, 512 heap blocks per
checkpoint, 4,096 bytes per ordinary heap object, and a shared 1 MiB checkpoint
memory budget. The engine's session deadlines, process controls, output caps
and partial-trace behavior also apply. Phase 5 additionally caps each event
file read at 1 MiB; exceeding it preserves the parsed prefix but marks the
rest of the event history lost and disables future lifetime reconstruction.
This is a read-memory bound, not a dedicated event-log disk quota. The engine
also attempts to apply a 64 MiB per-file `RLIMIT_FSIZE` to the stopped inferior
after `exec-run` reaches its first stop; writes before that point are not
covered by this enforcement, and `prlimit` failure is reported rather than
silently treated as success. No allocation occurs in the wrapper's
formatting path. The wrapper does not intercept allocations internal to shared
libraries or custom allocation APIs.

Previously, malformed/truncated allocation logs could silently leave the
timeline looking exact, in-place realloc retained the same allocation ID,
and reverse address scans rebound pointers to newer lifetimes. The current C
wrapper holds its per-process lock across the intercepted allocator call,
table update and log emission. Sequences order those intercepted operations
within a process; they do **not** create a global order across forked processes
or include calls bypassed by the wrapper's recursion guard.

## B. Allocation Identity And C. Operations

An allocation ID identifies one observed lifetime, not a numeric address.
Process-local IDs are widened only when multiple processes emit events;
untracked sentinel ID 0 remains 0. A successful in-place or moved realloc
records the old ID and a distinct new lifetime ID; the old block ends at that
sequence. A failed realloc leaves the old block active. Successful
`realloc(NULL,n)` starts a lifetime. The target glibc's observed
`realloc(ptr,0)` NULL behavior is recorded without universalizing it to other
libcs. Zero-sized malloc/calloc results follow actual returned addresses.
Calloc logs its count and element size; overflow never fabricates a total.
`free(NULL)` is a no-op event, not a freed allocation. Untracked frees carry no
invented tracked ID. No test performs an invalid free merely to classify it.

## D. Address Reuse And E. Pointer Provenance

Historical blocks remain separate even when their numeric address is reused.
`PointerTarget.address_match_allocation_id` describes a *candidate at the
observed numeric address*, not pointer origin. `provenance_status` distinguishes
`address_only`, `known_freed_address`, `ambiguous_reuse` and
`history_incomplete`. An address previously occupied by another lifetime does
not prove that either an old pointer or newly observed pointer originated from
the latest lifetime: `allocation_id` remains absent on reuse. Without reuse or
history gaps, the existing unique-current-allocation `allocation_id` behavior
remains for compatibility, explicitly marked `address_only` rather than
assignment provenance. Heap precedence over stack/global candidates remains.
Null pointers, caller-frame identities, and Phase 1 ambiguity remain intact.

## F. Ordering And G. Attribution

Per-process sequence anomalies, malformed or partial lines, block/event caps,
and incomplete termination make history incomplete. The tracker keeps already
captured records, avoids inventing later active-block state after an uncertain
sequence, and marks heap usage as last-known rather than exact.
Malformed or oversized lines have no reliable PID: discovery of a definitive
transport error quarantines subsequent live-block reconstruction for **all**
processes at that stop, including newly seen PIDs. A merely incomplete
trailing line remains buffered until it is finished; if still incomplete at
finalization it makes the final history incomplete. Earlier observed records
are not deleted or rewritten as facts about later lifetimes.

Wrapper `raw_caller_address` is a *return* PC. A single batched addr2line pass
resolves `return_pc - 1`, with executable bias applied for PIE; the original PC
remains in the trace. The result is a source-call-site **candidate**, not an
exact instruction stop, especially with macros, inlining and optimizations.
Unresolved return PCs remain explicit. Event `checkpoint_interval` brackets
retained source checkpoints using their cumulative event counts. No event is
called exact-at-checkpoint merely because it falls in that interval; a missing
boundary stays open. Process-local order is not a global thread/process clock.

## H. Snapshots, I. JSON And J. Parser

Snapshots still read only known live blocks within Phase 4 count/byte budgets;
historical freed blocks are retained in compact final history, not copied into
every checkpoint. `observed_at_stop` identifies when bytes were last read.
Gaps leave lifetime status unknown rather than proving a block remained alive
or was freed. New optional schema-2 fields include heap lifetime status,
`HeapEvent.attribution` and calloc operands, `heap_usage_complete`,
`RecordingData.heap_history`, and pointer address/provenance evidence. Older
traces deserialize with absent optional fields. TP-2 identity indexes remain
allocation-ID-based; borrowed TP-3 views expose the original event records.
Deep validation checks interval bounds and pointer provenance consistency.

Independent Valgrind and sanitizer executions retain their raw diagnostics.
The primary-run allocation ID is **not** inferred merely because a different
Valgrind execution reports the same file/line; independent ASLR and allocator
outcomes invalidate such identity claims.

## K. Performance And L. Tests

The independently preserved Phase 4 release executable
(`c82d91da0493ac662e85d1d110e572c9e07bed1fcef6b946abac2828373514fc`)
and paired Phase 5 release executable
(`5ea05192be347a8b25ddebf8cad693d8dfa1bfe63911b343753b06483b98031a`)
ran ten identical source/config workloads, one serial run each. Inputs, traces,
journals, GNU-time output, semantic projections, per-case timings and complete
comparisons are in `/tmp/opencode/p5-final/`. This matched run kept each
session's raw event file so its byte size is measured, not inferred. The runner is
`scripts/phase5_benchmark.py`. No fixed speedup is claimed from one run.

| Workload | Events P4/P5 | GDB commands P4/P5 | Wall seconds P4/P5 | Trace KiB P4/P5 |
| --- | ---: | ---: | ---: | ---: |
| Simple malloc/free | 2/2 | 77/77 | 4.93/3.14 | 46.2/47.7 |
| Reused addresses loop | 16/16 | 534/534 | 3.43/3.37 | 288.1/297.3 |
| Realloc-heavy | 8/8 | 440/440 | 3.33/3.38 | 238.0/249.7 |
| Recursive heap | 12/12 | 775/775 | 3.89/3.53 | 439.8/453.8 |
| Heap pointer array | 8/8 | 355/355 | 3.39/3.53 | 188.2/192.4 |
| Alias/reassignment | 4/4 | 209/209 | 3.23/3.27 | 147.7/152.1 |
| Nested calls | 4/4 | 230/230 | 3.33/3.23 | 146.4/149.8 |
| Snapshot-heavy | 48/48 | 3,149/3,149 | 4.78/5.38 | 1,425.2/1,491.3 |
| Many events | 80/80 | 2,422/2,422 | 4.75/4.69 | 1,251.2/1,297.5 |
| Zero/failure cases | 7/7 | 196/196 | 3.29/3.23 | 152.7/158.5 |

In aggregate: 189/189 retained heap events, 8,387/8,387 GDB commands,
4,323.4/4,490.2 KiB JSON and **7,686/7,850 raw event-log bytes**.
All ten workloads also matched GDB *command-category* counts exactly.
The event logs differ in text-length of recorded IDs and operands, not event
count. The compact serialized `heap_events` arrays grew from 49,119 to
100,491 bytes (51,372 added bytes over 189 events, about 272 bytes/event,
including source/interval evidence); journal bytes grew from 3,799,204 to
3,910,896. Phase 5's measured file-read/parsing/ingestion,
snapshot reconstruction, interval correlation and pointer-enrichment time
across all ten cases was 68.814, 2.422, 0.027 and 3.753 ms respectively.
Individual engine peak RSS deltas ranged from −148 to +244 KiB. Debugger
time varied; the aggregate of single-run samples was 12.19/12.06 seconds and
wall time 38.34/36.74 seconds. These variations **cannot** establish a causal
speed improvement; Phase 5 intentionally changes the wrapper and trace bytes.
The C wrapper itself is not independently CPU-timed inside the inferior:
debugger/wall totals include its work, and ingestion profiling excludes that
inferior-side cost. Journal and raw event-file sizes are measured separately.

Control-flow comparison excluding code addresses (which shifted when the
instrumentation binary changed) passed **10/10** for instruction order,
checkpoint/stop order, and operation/size/usage sequence. The strict projection
reports changed code PCs and intentional realloc ID and heap-pointer value
representations; unresolved runtime addresses are marked indeterminate rather
than equal. In the realloc-heavy fixture Phase 4 recorded one lifetime across
six in-place reallocs; Phase 5 recorded seven linked lifetimes. Across the
ten benchmarks all 189 events have explicit attribution (Phase 4: none).
The final traces report 450 heap address candidates: 130 unique live
`address_only` matches, 36 known-freed-address matches and 284 ambiguous-reuse
matches; the 320 uncertain matches do **not** receive a fabricated pointer
allocation ID. Phase 4 attached an allocation ID to all 442 heap numeric
matches. These counts are observations of these particular runs, not a
claim that 442 pointer origins were previously known.

Validation commands: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`,
`cargo test --no-fail-fast -- --test-threads=1`,
`cargo test --test heap_provenance -- --test-threads=1`, and
`python3 scripts/phase5_benchmark.py self-test`. The final uncontended serial
suite passed **283 tests, zero failures** (177 library, 105 integration, and
one doc test); its output is `/tmp/opencode/phase5-clean-tests.log`.
The post-fix 35 allocation-tracker tests and three live GCC/GDB provenance
tests also passed, as did formatting, Clippy, release build and benchmark
projection self-test. An earlier contended serial run had two short-deadline
failures; each passed individually, and both passed in the final clean suite.

## M. Files Changed

Phase 5 edits: `c_runtime/alloc_tracker.c`, `src/allocation_tracker.rs`,
`src/memory.rs`, `src/models.rs`, `src/trace.rs`,
`src/trace_parser/validation.rs`, `tests/heap_provenance.rs`,
`fixtures/c/heap_provenance.c`,
`fixtures/c/heap_provenance_realloc.c`,
`scripts/phase5_benchmark.py`, and `docs/trace-engine-phase5.md`.
Earlier uncommitted Phase 1–4 and Trace Parser work is preserved.

## N. Extraction-Gap Ledger

| Gap | Classification | Phase 5 disposition |
| --- | --- | --- |
| Pointer assignment/origin not directly instrumented | Later Semantic Engine / fundamentally uncertain from a numeric value | Address-level candidate only; no invented pointer history |
| Exact allocator expression, sizeof operands and pointer arithmetic | Phase 6 AST enrichment | Deferred; AST cannot override runtime outcomes |
| Cross-source declaration/call expression correlation | Phase 7 fusion | Deferred; no broad provenance registry |
| Shared-library/internal/custom allocators | Targeted instrumentation/platform work | Missing coverage explicit; no invented block history |
| Allocations bypassed by recursion guard or custom APIs | Targeted instrumentation work | Process-local sequence orders only intercepted operations |
| Forked process IDs with unrepresentable local ID width | Platform/protocol work | Explicit incomplete history; do not silently claim uniqueness |
| Fork during another thread's wrapper critical section | Platform/instrumentation work | Inherited C lock/table state is not fork-reinitialized; no cross-fork ordering or completeness guarantee |
| Missing/duplicate/partial event log | Phase 5 transport validation | Explicit loss/quarantine; previous observations preserved |
| Event-log disk growth before inferior file-size limit applies or if `prlimit` fails | Runtime storage control | Per-stop ingestion is bounded to 1 MiB; a best-effort 64 MiB per-file `RLIMIT_FSIZE` is applied to the inferior after its first stop, not a dedicated event-log quota |
| Exact heap event-to-GDB stop | Fundamentally unavailable without synchronized instrumentation | Only bounded checkpoint intervals |
| Return-PC source ambiguity under inlining/macros/optimization | Phase 6 AST / later semantic analysis | Preserve raw PC and candidate source, not exact claim |
| Valgrind/sanitizer observations from separate runs | Later semantic analysis / fundamentally uncertain identity | Preserve original diagnostics; no same-address or same-line identity merge |
| DWARF cross-CU origin, expression-valued locations/bounds, unsupported tags | Targeted DWARF work | Unchanged from Phase 4; heap changes do not resolve them |
| Non-GCC/ELF target layouts/endian/word sizes | Platform support | Unverified; preserve failure information |

## O. Phase 6 Handoff

Clang AST could add source-level allocation/realloc expressions, `sizeof`
operands, pointer assignments and arithmetic, and declaration-to-expression
links. Those source relationships would complement, never replace, GCC/GDB
addresses, actual wrapper returns, process-local sequence evidence and explicit
unknowns. No Clang invocation, AST parser, Semantic Engine, register polling or
GUI is part of Phase 5.

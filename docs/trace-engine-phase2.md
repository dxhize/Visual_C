# Trace Engine V2.2 — Phase 2 (GDB/MI2)

This phase reduces redundant GDB commands without changing MI2 token
matching, stepping, stop IDs, checkpoint order, or observation semantics.
Phase 3 metadata extraction is **not** part of this work.

## Query inventory and lifetime

| Query | Evidence | Validity and disposition |
| --- | --- | --- |
| `-stack-list-frames 0 199` | Whole call stack, activation path | Dynamic; always required for snapshots and crash stops. Transient, single-threaded non-user callee-entry stops can use the stop frame for activation bookkeeping only. |
| `-stack-list-variables --all-values` | Current variable names and values | Dynamic; always refreshed at source checkpoints. |
| GDB-Python lexical probe | Declaration scope/type and per-stop location/size/pointer target | Probe code installed once per GDB process; executes anew for every checkpoint. Declaration memoization was rejected after measuring a capture-time regression. |
| `whatis` | Declared type | Existing session-local type cache; frame-specific fallback keyed by frame/name/activation, globals by module epoch and qualified symbol. |
| `&name`, `sizeof(name)`, pointer cast | Runtime address, runtime size (including VLAs), pointer target | Prefer fresh probe properties; retain each MI fallback independently when missing or invalid. No cross-stop reuse. |
| Global `&(...)`, `sizeof(...)` | Static-storage address/size | Cache only successful, file-qualified symbols under `(module_epoch, qualified_name)` in this session. Function-local statics remain fresh. Type must also have been observed before caching. Module epoch changes make old entries unreachable. |
| `-data-read-memory-bytes` | Variable/global/heap bytes | Fresh at every stop. Only reuse a successful read from the **same stop**, same start address, requested subset length; new map per checkpoint. Heap bytes are independently read. Failed reads never enter the map. |
| Disassembly | Static instruction metadata | Existing disassembly cache unchanged. |
| `-exec-*`, stop records | Execution position and instruction sequence | Dynamic; command scheduling, token matching and asynchronous routing unchanged. |

The global metadata cache is owned by `GdbSession`; constructing another
session produces a fresh cache. The lexical probe **function** is likewise
installed per GDB process, not reused across sessions; it does not retain
declarations, runtime values, availability, addresses, memory, or pointer
targets. Scope/activation changes are handled by fresh probe execution.
The same-checkpoint read map is discarded at the next stop. In particular,
no inference from missing write events is used to skip memory observations.

The duplicate-name count map is temporary and counts the *full* MI variable
list, even when capture is capped; shadowed symbols retain their original
order and identity handling.

## Reproduce and interpret the benchmark

Build with `cargo build --release`, then trace `same_line`, `pointer_demo`,
`shadowed`, `recursive`, `globals`, `aggregate`, `finite_loop`, `many_allocs`
and dual-mode `nested_loops` from `fixtures/c/`. Use the same limits and
configuration for the baseline and candidate. Run each workload five times,
report medians per metric, and retain full JSON traces for semantic
comparison. Baseline/optimized measurements from this phase are stored in
`/tmp/opencode/p2/` in the implementation environment.

`resource_usage.wall_time_ms` includes startup, compilation, and other
pipeline work; `profiling.debugger_time_us` and `capture_time_us` isolate
debugger and capture work. Short workloads are dominated by process startup,
so command/capture improvements need not improve total wall time. Category
counts include all issued MI commands, not inferred savings. Optional
profiling fields default to `None` for older traces.

The GDB-Python static-declaration memo was tried and rejected: it eliminated
block traversal but added block-chain computation, symbol/dictionary copies,
and serialization every stop. Despite fewer commands, its five-run medians
increased capture time. Sending the entire probe source at every stop also
caused a regression. The retained design installs code once per session,
then sends names only while refreshing all dynamic properties.

Future DWARF work could replace GDB-only declaration type/scope/location
queries where equivalence is proven. It must not replace runtime location,
value, availability, pointer-target, VLA-size, or memory observations without
per-stop evidence.

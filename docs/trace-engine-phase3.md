# Trace Engine V2.2 — Phase 3 static metadata

`src/dwarf.rs` uses `object` to load the **actual GCC debug executable**
and `gimli` to traverse its DWARF. `trace.rs` calls it **once**, after the
successful debug build and before GDB starts. The resulting optional
`DebugSessionTrace.type_registry` is serialized once, not copied into
snapshots. Failed/missing/oversized metadata produces an analysis note and
leaves GDB tracing intact. No DWARF expression is executed.
The engine-injected allocator instrumentation CU is scanned but excluded
from the serialized **user** registry by its exact generated source path;
`compilation_units_scanned` reports the scan count.

## Scope and identity

Type and declaration IDs are trace-local and combine the debug artifact
namespace, compilation-unit header offset, and unit-relative DIE offset.
Names never identify types: different CUs with identically named static
objects or types stay distinct. The registry retains provider (`gcc_dwarf`),
compilation-unit identity, DIE offsets, target address size, declaration
file/line/column, containing function, and lexical parent ID. Recursive
types use references, not recursive JSON nesting.

| Category | Captured evidence | Limitation |
| --- | --- | --- |
| Base, pointer, typedef, qualifier, function | Name, DWARF size/encoding when present, underlying type reference | No host-ABI size guesses; pointer size uses the target CU address size if DWARF omits it. |
| Arrays | Element type, each constant subrange/count, lower/upper bound | No fabricated total size if absent; VLA expression bounds are `runtime_dependent` with unknown count; incomplete arrays remain `unavailable`. |
| Struct/union | Actual size, ordered members, type references, byte offsets | Missing expression-valued location is not evaluated. Union zero offsets are marked as implicit, not presented as explicit attributes. |
| Bitfields | Recorded width, DW_AT_data_bit_offset, and legacy DW_AT_bit_offset separately | No inferred packing or conversion between legacy and data-bit semantics. |
| Enum | Explicit signed/unsigned values, including negatives and duplicates | No assumed sequential values. |
| Declarations | File, line, column, parent scope, function, type, linkage when recorded | Specification/abstract origin: one same-CU hop; unresolved cross-CU references remain limitations. |

The observation's optional `type_ref` and `declaration_ref` are set only
when full source file, line, name, function compatibility, and a unique
declared type match. An unresolved/ambiguous declaration leaves GDB values,
bytes, addresses, statuses, pointer identity, stop IDs, and activation IDs
unchanged. The lookup is built once; the registry is never traversed at each
checkpoint.

## Limits and provenance

Object input and combined decoded DWARF sections each have a 64 MiB cap.
Extraction allows at most 512 CUs, 150,000 DIEs/types/declarations, 30,000
aggregate members, 4,096 bytes per retained string, and five seconds of
traversal. Compressed sections are checked against their stated expanded
size **before** decompression. Limit violations fail supplemental extraction;
they do not produce a partial registry masquerading as complete.
`unsupported_die_count`, `unresolved_type_references`,
`unresolved_origin_references`, and bounded limitation notes distinguish
omitted metadata from a complete extraction failure.

Profiling adds optional extraction, reference-resolution, and correlation
times. Schema remains version `2`: old traces deserialize with no registry
or references; TP-1/TP-2/TP-3 continue to operate on the same runtime facts.

Phase 4 may use structural sizes and offsets to plan *additional live GDB
reads*, array element views, globals, and per-frame observations. It must
not infer heap-array type from a pointer, guess runtime VLA extents from
static DWARF, or treat type metadata as a runtime memory object.

## Measured cost

Release build, identical tracing limits, three Phase 3 runs per workload;
Phase 2 comparison uses the earlier five-run medians. Phase 3 numbers below
are independent per-metric medians. The actual registry is serialized once.

| Workload | CUs scanned/kept | Types/decls/members | Extraction ms | Association ms | Registry compact bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Arithmetic | 2/1 | 1/3/0 | 0.359 | 0.011 | 1,404 |
| Arrays/structs | 2/1 | 16/5/5 | 0.422 | 0.100 | 6,517 |
| Recursion | 2/1 | 1/3/0 | 0.345 | 0.050 | 1,426 |
| Pointer/heap | 2/1 | 15/8/0 | 0.415 | 0.081 | 6,125 |
| 1,000 snapshots | 2/1 | 13/4/0 | 0.355 | 1.391 | 4,517 |
| Type-layout fixture | 2/1 | 23/12/11 | 0.592 | 0.098 | 11,757 |
| Two translation units | 3/2 | 5/6/2 | 0.417 | 0.029 | 3,723 |

GDB category counts matched Phase 2 exactly on all five comparable
workloads. Debugger/wall medians varied in both directions despite unchanged
command counts; neither a GDB speedup nor a consistent slowdown is claimed.
Engine peak RSS was typically 0.3–0.6 MiB higher. Additional JSON bytes
include the registry and two optional trace-local references per correlated
observation; the 1,000-snapshot workload grew by about 3%.

Phase 4 runtime results and the classified extraction-gap ledger are in
[trace-engine-phase4.md](trace-engine-phase4.md). GDB runtime sizes complement,
but do not resolve, expression-valued static DWARF metadata.

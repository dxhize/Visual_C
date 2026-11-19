# Visual C V2.2 — Phase 8C-F13 Bounded JSON Export Repair

**Verdict: repaired for final JSON serialization/export.** The journal and
final JSON now have independent byte limits. An oversized export fails with
exit status 1, a diagnostic on stderr, and **no stdout bytes** from budget
rejection; existing output files survive. A valid in-budget export retains
all evidence, schema v2, and the trailing newline. This repair does not cap
memory used earlier to assemble `DebugSessionTrace`.

## Root cause and failing-before evidence

`src/recorder.rs`/`src/storage.rs` enforce `max_trace_bytes` on the JSONL
journal. Old `src/main.rs` independently serialized the entire trace to
stdout and used `File::create` (truncating an existing file) before writing
to `--output`. No final-export budget was checked, and a failed file write
could leave a truncated file. On the pre-repair release executable:

- `hello.c --max-trace-bytes 2048 --trace-mode dual --no-pretty
  --sanitizer-trigger never` wrote **22,712 bytes** to final JSON despite
  the 2,048-byte journal cap (`/tmp/cte-f13-baseline.json`; the previous 8C
  audit also recorded 22,959 bytes on its run). Session metadata differs
  across runs; neither size is claimed deterministic.
- The proposed `--max-json-export-bytes 2048` was rejected as an unknown
  argument (exit 2, CLI usage message). There was no independent export cap.

## Policy and implementation

`ExecutionLimits.max_trace_bytes`: JSONL **journal** bytes, 128 MiB default;
existing checkpoint-stop/incomplete-coverage behavior is unchanged.
`ExecutionLimits.max_trace_memory_bytes`: resident trace-recording budget,
32 MiB default; not a limit on all assembled evidence/registries.
**`ExecutionLimits.max_json_export_bytes`**: each final UTF-8 JSON document
(stdout and optional file), independent 128 MiB default; CLI
`--max-json-export-bytes N` or JSON config
`{"limits":{"max_json_export_bytes":N}}`. Must be positive. Legacy config
files/traces missing the field deserialize using the default; a default-valued
field is omitted on serialization to keep legacy JSON compatible. An explicit
nondefault value is recorded in `execution_limits`. To allow a larger
export, configure a larger finite value; no unlimited sentinel is supported.

`src/json_export.rs` serializes through a counting `Write` implementation,
first to `io::sink()` for **bounded preflight**, then to the destinations.
Every actual serialized byte is counted, including pretty-print indentation,
UTF-8 characters and **one final `\n`**. A document exactly equal to the
limit succeeds; a document one byte larger fails **before opening a file or
writing stdout**. An independent counting writer is used again on each
actual write (defense against serialization changing between passes). No
full-size JSON string/Vec is created by production export code. A limit
exhaustion is an operational CLI error, not a new trace `ExecutionStatus`, and
cannot alter the stopped-state completeness of evidence already collected.

Optional file output is serialized to a unique `create_new` sibling temp
file, buffered, flushed and `sync_all`'d, then `rename`d over the destination
where the filesystem supports atomic replacement. A drop guard removes the
temp on any error prior to the rename, including serializer failure, ENOSPC,
write/flush/sync failure, or cooperative SIGINT cancellation. An existing
valid file is not replaced by a partial export. The stdout write occurs
**after** file commit; an error on stdout still returns exit 1, although the
file may already contain a valid trace. Stdout is preflighted before either
write, but OS-level stdout write failure, pipe closure or cancellation during
its actual write may produce partial bytes; consumers must respect the
nonzero exit code rather than parse them as a successful trace. Atomicity
across both destinations is impossible without changing this CLI contract.
File-permission changes on successful replacement and lack of parent
directory fsync are outside the repair's durability guarantee. SIGKILL and
power loss cannot execute a Rust cleanup guard; a sibling temp may remain.
No GDB, provenance, AST, DWARF, fusion, Trace Parser or trace-content logic
was changed.

## Passing-after regressions and fidelity

- Original 2 KiB dual-mode journal regression still reports
  `step_limit_reached`, `max_trace_bytes` and incomplete coverage with journal
  bytes ≤2,048. Its independently measured final JSON requires more than
  2,048 bytes; the new export API rejects it with zero stdout bytes. Setting
  the export limit to its measured size succeeds, parses as a complete
  `DebugSessionTrace`, and preserves the full evidence as compared by
  `serde_json::to_value`.
- Release CLI, `hello.c --max-trace-bytes 2048
  --max-json-export-bytes 2048 --output /tmp/cte-f13/existing.json
  --trace-mode dual --no-pretty --sanitizer-trigger never`: **exit 1**, stderr
  `max_json_export_bytes exceeded (limit 2048 bytes)`, stdout **0 bytes**;
  existing 13-byte file unchanged. With independent export cap 1,000,000:
  **exit 0**, 22,737-byte stdout and file were byte-for-byte identical,
  parse as schema v2, journal bytes 1,897, journal-limit status preserved;
  no sibling temp remained.
- Unit tests independently compute `serde_json::to_vec` or
  `to_vec_pretty` length **plus the newline**, for both compact and pretty
  UTF-8 (including `é`). Equality succeeds and one byte over fails with
  zero stdout; file rejection preserves its old contents and leaves no temp.
  Existing 159 KiB legacy stage-1 `realloc_move.json` is loaded into the
  current model, streamed to file and stdout, reloaded, and checked for
  complete evidence equality against the original model. A real Clang/GCC
  release CLI trace (`phase8b_macro.c`) parsed successfully from file and
  stdout: **81,347 bytes**, 41 type-registry entries, three AST declarations,
  `ast_available=true` and a complete AST translation unit. The 25 MiB
  8B deep-runtime trace contains 1,000 snapshots and 30 type-registry entries;
  the bounded benchmark below validates its complete byte count.
- Injected serializer failure during preflight **and** after opening the
  sibling temp preserves the old output, leaves no stdout, and removes temp.
  Invalid file parent returns failure before stdout. Injected ENOSPC on
  `Write` propagates the error; a real `/dev/full` stdout test exited 1,
  reported ENOSPC and retained a valid, independently parsed file when
  `--output` had already been committed. Broken-pipe behavior and deliberate
  cancellation before file commit are separately asserted.
- Legacy config defaults, CLI-vs-JSON override priority, no field dropping,
  and schema v2 remain covered by focused and full-suite tests.

## Performance and resource measurements

`examples/phase8f13_export_bench.rs` independently counts one compact
`serde_json` serialization to a streaming counter, then times rejection,
exact-fit success and a larger budget using the **real bounded export**
with a counting stdout writer. No full-size serialized JSON buffer is used
in this benchmark. Release runs, Linux host, sequential; times are wall time
for in-memory export only, not tracing, file fsync or stdout terminal I/O.
`VmHWM` includes initial file read and JSON parsing.

| Existing baseline trace | Result / budget | Export time | Output bytes | Process peak RSS |
|---|---|---:|---:|---:|
| Deep runtime, 25,081,617 bytes | one byte under → reject | 120.088 ms | 0 | 70,984 KiB |
| Same | exact fit → success | 302.891 ms | 25,081,617 | 70,984 KiB |
| Same | 2× budget → success | 233.403 ms | 25,081,617 | 70,984 KiB |
| AST + DWARF type registry, 81,347 bytes | one byte under → reject | 0.380 ms | 0 | 3,744 KiB |
| Same | exact fit → success | 0.879 ms | 81,347 | 3,808 KiB |
| Same | 2× budget → success | 0.755 ms | 81,347 | 3,808 KiB |

Rejection still traverses the document once; successful stdout export traverses
it twice (preflight + write), or three times with file + stdout. Near-limit
serialization time varies with host load and the rejected byte's position;
this table is not a worst-case latency bound. Initial trace construction,
GDB/Clang process RSS, filesystem-space reservation, complete resident trace
size, and OS page cache are **not** bounded by this export setting.

## Verification and handoff

After the final implementation change, all required commands passed:

```
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo test --no-fail-fast -- --test-threads=1
```

**350 passed, 0 failed** across 18 test targets; full serial log
`/tmp/visual-c-f13-tests-final2.log`. Eight focused new export unit tests
plus the updated real-GDB journal/export regression pass. Existing uncommitted
Phase 8R/8B/8C changes were preserved.

Phase 8D (not begun) should separately address memory already occupied by
`DebugSessionTrace`, holistic engine/cgroup accounting, disk quota and
file-system durability semantics, and sustained/hostile workloads. Do not
confuse this successful **serialization/export** bound with those distinct
resource policies.

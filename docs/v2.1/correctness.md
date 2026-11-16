# V2.1 Correctness Report

Evidence that removing per-instruction register capture preserved every
correctness guarantee. Sources: the full test suite (see
[`test-results.md`](test-results.md)), the regenerated example traces in
`examples/`, and the V2 audit validator
(`test_reports/v2_validation/runner/validate_trace.py`, updated for V2.1
semantics and re-run against the new traces).

## 1. Source checkpoints remain accurate

- `tests/integration.rs::compiles_sample_fixture` (checkpoints, variables,
  call stack), `::source_level_features` group (locals, globals, recursion,
  multi-file builds) — all PASS.
- Example `v21_dual.json` (sample.c): 25 snapshots ↔ 25 source checkpoints,
  every one of the 80 instruction events carries a source mapping, and the
  observed lines (21–47 of sample.c) match the fixture's statement layout.
- Value observations are unchanged: `variables[]` still carry `value`,
  `address`, `raw_bytes_hex`, `read_status`, and pointer classification
  (`target.kind` ∈ stack/heap/global/null/unmapped with `points_to`) —
  verified identical in kind to the V2 audit traces
  (`bench_heap_dual_0.json`: `heap→heap` vs V2.1 `v21_heap.json`:
  `p→stack`, `heap→heap`, `third→heap`, `null→null`, `0x2→unmapped`).

## 2. Instruction events remain ordered and real

- `processor::validate` enforces contiguous `event_id`s, valid
  `instruction_id` references with address equality, valid checkpoint
  references, and instruction-count consistency — re-run on every example.
- `tests/v2.rs::loop_events_are_dynamic_and_registers_are_not_collected`:
  dynamic loop execution preserved (unique addresses < event count),
  static-disassembly caching still effective
  (`disassembly_cache_hits > disassembly_queries`).
- In every V2.1 example, `next_address` of event *N* equals event *N+1*'s
  `address` on straight-line runs; divergences occur exactly at
  `call`/branch instructions (verified against the static disassembly text,
  e.g. `call 0x400b63 <__wrap_malloc>` → next event at `0x400b63`).
- Nothing is fabricated: event count equals stop count; the audit validator's
  E1–E20 invariants (updated for V2.1) pass on all regenerated traces.

## 3. Source and assembly remain synchronized

- Same-run guarantee is structural (one GDB session, one timeline; source
  checkpoints and instruction events are `stop_id`-linked) and is exercised
  by every dual-mode test: `loop_events…`, `callbacks_and_recursion…`,
  `same_line_functions…`, `thread_limitations…` — PASS.
- `v21_dual.json`: 25 checkpoint-linked events, every event source-mapped,
  instruction addresses match their disassembly-table entries (validator
  E7).

## 4. Pointer and heap tracking still work

- `tests/v2.rs`: realloc-move detection, address-reuse identity, dangling
  classification, calloc/failed-malloc semantics — PASS (unchanged suite).
- `v21_heap.json`: allocated→freed lifecycle with stable allocation ids,
  `heap_state` final states consistent with events.
- Allocation source mapping (caller file/line via addr2line) unchanged
  (`capture_time_us` shows the expected work; `allocation_lifetimes`
  intact).

## 5. Crash locations remain correct

- `v21_crash.json` (null_write.c): `crash_report.signal_name=SIGSEGV`,
  source `line 3`, `instruction_address=0x400546`; the last instruction
  event has the same address, outcome `faulted`, signal `SIGSEGV`, and its
  disassembly entry is `movl $0x2a,(%rax)` — the null write. Exactly 5
  events, no post-crash fabrication.
- `tests/integration.rs::crash_is_reported_with_sanitizer_pass`: SIGSEGV +
  correct file/line + sanitizer reproduction — PASS.
- The faulting address is observed from the faulted stop's frame (the PC
  stays on the faulting instruction) — the same mechanism V2 used; register
  values were never part of this path.

## 6. Incomplete observations are represented honestly

- Coverage/truncation machinery untouched:
  `deadline_includes_queries…` (partial trace + gaps recorded),
  `instruction_limit_is_not_a_crash_and_journal_recovers_order` — PASS.
- F-01 fix adds honest interrupted-stage metadata for the allocation tracker
  (see [`fixes.md`](fixes.md)).
- Per the brief's §6 vocabulary: memory observations remain
  "observed at the recorded stop" — nothing was relabeled; with register
  capture gone there are simply no intermediate-state claims anymore. The
  pre-existing `RecordingData.coverage.limitations` note "Memory is exact
  only at its recorded stopped observation" already stated this policy and
  remains in every V2.1 trace.

## 7. Schema compatibility

- `tests/v2.rs::trace_roundtrips_through_json_losslessly` — PASS: V2.1
  traces serialize/deserialize losslessly.
- Old V2 traces deserialize (register slots optional with defaults);
  `examples/` keeps two V2 traces for side-by-side comparison.

## 8. Timeouts, limits, cleanup (unchanged behavior)

- `deadline_includes_queries_and_does_not_start_valgrind_after_expiry`,
  `instruction_limit…`, `max_gdb_commands` caps,
  `sigint_preserves_partial_source_trace_and_reaps_debugger_tree` — PASS.
- New F-01/F-03/F-04 assertions cover the *reporting* of those interruptions.

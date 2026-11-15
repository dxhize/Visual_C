# TRACE CORRECTNESS — deep analysis (c-trace-engine V2)

All claims below cite per-test evidence under `evidence/<TEST>/trace.json`.
Validator source: `runner/validate_trace.py` (invariants E1–E20 + per-suite
semantic checks); campaign results: `test_results.json`.

## 1. Source-level stepping

- **Coverage of user stops.** Snapshots exist for every user-code stop in all
  source/dual runs (A1: 8, A2/C3: 17, A3: 38, D4: 731). Each snapshot's
  observation carries `origin: "user_code"`, the inferior `pid`, a `stop_id`,
  `instruction_position` and `complete`/`errors` flags (probed in J1/A1D).
- **Values are real reads, not estimates.** A1's `product` observed `42` and
  `shifted` `168` at the expected stops; C4's `sum` observed `30` before `prod`
  reached `300`. B2's heap blocks show the write history byte-for-byte
  (`00000000…` → `44434241…` after `p[0]=0x41424344`, then `…4d…` after
  `p[1]=77`) — memory observations track actual program writes.
- **Call stacks / frames.** A2 observed `main`, `middle`, `leaf` with nested
  frames (depth ≥ 2 recorded in `call_stack`). A3 (recursion) shows multiple
  simultaneous `countdown` frames with distinct per-frame identities and
  distinct `n` values (`6..0`); frames disappear as recursion unwinds — no
  frame merging observed (A3 evidence).
- **Globals/statics.** A4 captures `global_counter` and file-qualified
  `'/abs/path/audit_a4_globals.c'::file_static` with correct values, and the
  function-local `static calls` survives across calls. The pointer `gp` is
  honestly `0x0`/`null` before `gp = &global_counter` executes, then reclassified
  `kind:"global"` pointing at `0x404060` with a symbol identity — no
  pre-initialization fabrication.

## 2. Instruction-level stepping

- **Event semantics match the documented design** (`models.rs:525`):
  `address` is the PC *before* the attempt, `next_address` the PC after,
  `outcome ∈ {completed, faulted, interrupted, unknown}`, plus `signal` and
  `stopped_reason`. Verified on every dual trace:
  - completed/unknown events: `rip` (captured after the attempt) equals
    `next_address` (E8b, all dual runs, 0 violations).
  - B1 faulted event: `outcome:"faulted"`, `signal:"SIGSEGV"`,
    `address == next_address` (PC stays on the faulting instruction).
- **Ordering and identity.** `event_id` dense 0..n-1; `stop_id_before`
  monotonic across events (E4/E5 across all runs); stop-id chain 1→77 in C3.
- **Bounded capture is honest.** D2 (`--max-instruction-events 25`): exactly 25
  events, `status/outcome step_limit_reached`, `limits_triggered:
  ["max_instruction_events"]`, no fabricated continuation events.
- **Unknown outcomes are recorded, not guessed.** A1D ev2 shows
  `outcome:"unknown"` where GDB reported a stop without a completion proof —
  the engine does not invent `completed`.

## 3. Source↔assembly synchronization (the headline V2 feature)

- **Same execution.** C8 cross-check on C3: one inferior pid (468389) across
  *all* instruction events and snapshots; snapshot `stop_id`s fall inside the
  event stop-id range; 76 `checkpoint_id` references validate (E6).
- **Repeats are kept, statics deduplicated.** C2's 5-iteration loop produced
  **29 events on the loop line** (dynamic history preserved) while
  `disassembly_table` stayed small (static instructions reused via
  `instruction_id`), and `profiling.disassembly_cache_hits = 24` vs 1 query.
- **Packed lines handled honestly.** C6 (five statements on one line): 21
  events all mapping to line 1 — the engine never pretends one line = one
  instruction; checkpoints are sparser than events by design.
- **Crash agreement.** B1: faulting event `source.line = 3` = crash_report
  line = snapshot line; `crash_report.instruction_address ==
  events[-1].address`; signal SIGSEGV recorded on both. No post-crash events
  (E17).
- **Gaps cover what was not observed.** Every trace opens with a
  `startup_before_main` gap; C7 adds `excluded_code` gaps around libc
  (malloc/printf) — the engine never claims library instructions as observed.
- **Crash inside untraced code** (B6, SIGABRT from glibc's double-free
  detection): last *traced* event is `completed` (user code), the abort lives
  in an excluded region; `crash_report` carries `pc=0x7ffff7e13d0c` (libc) and
  `signal=SIGABRT`, execution status `crashed`. Honest representation; the
  faulted-instruction slot is simply absent rather than fabricated.

## 4. Register reconstruction

- **Data present for reconstruction.** Dual traces carry `initial_registers`
  (full checkpoint) plus per-event register blocks with per-register
  `availability` and GDB register numbering; blocks are post-attempt
  observations (§2).
- **Internal consistency.** E8b verified rip==`next_address` (faulted:
  rip==address) on every register block in every dual trace. Full-checkpoint
  cadence beyond the initial one was not exercised at these event counts
  (<256 events; cadence config `register_checkpoint_interval: 256`) — noted
  as a limit of the fixtures, not an engine failure.
- **Independent cross-comparison caveat.** The task asked for GDB cross-
  checks "where practical". Within a black-box audit without engine-side
  synchronization hooks, per-stop register comparison is limited to the
  post-attempt rip agreement above; a dedicated cross-validation harness
  (attaching GDB between steps) is recommended future work.
- **Cost.** Register capture is the single largest dual-mode cost:
  `register_time_us = 309 611` of `debugger_time_us = 1 342 013` in A1D
  (~23%); see PERFORMANCE_REPORT.

## 5. Pointer classification

Runtime behavior matches `memory.rs:119-150` exactly:
- B2 `p`: `null` before malloc → `heap id 1, active:true, within_extent:true`
  during use → `heap id 1, active:false` after `free` (the block leaves the
  point-in-time `heap_blocks` listing but the pointer value and its
  classification remain — the honest representation of a dangling pointer).
- B4 aliasing: `p` and `q=p+1` both resolve to the **same allocation id** with
  `offset_bytes` 0 and 4 (E11 + B4 checks).
- A4 `gp`: `global` classification with `variable_identity` symbol string.
- B1 `ptr`: `null` classification; later heap events are irrelevant to it.
- Unknown targets are recorded as `unmapped` (observed on uninitialized
  locals in B3) rather than guessed.

## 6. Heap lifetimes

- Event stream types: `allocated`, `reallocated`, `freed` with `pid`,
  `allocation_id`, `size_bytes`, `source_file/source_line` (caller site),
  `old_allocation_id` for realloc, `heap_usage_after`.
- B2: id 1 allocated (16 B, caller fixture:3) → freed once; E9/E10: ids
  stable, freed ids never reactivated.
- B3: **realloc keeps the same allocation id** (`reallocated`, id 1,
  `old_allocation_id:1`) across 8→32→4 byte resizes (glibc resized in place
  at 0x705010); all of `a`, `b`, `c` legitimately alias id 1; after the final
  `free` all three show `active:false`. No assumption that realloc moves
  memory was required — the trace records what happened.
- B5 (use-after-free read): allocation freed, status preserved, pointer value
  preserved; **valgrind records `InvalidRead`**; the traced program itself
  exits 0 (true behavior — documented in the fixture) and the engine reports
  `completed` without pretending a crash. Limitation: the ASan reproduction
  stage is skipped for non-crashing runs (finding F-02).
- B6 (double free): two `free` calls → SIGABRT inside libc; valgrind and
  crash_report both carry diagnostics (B6 checks PASS).
- B7 (OOB write on an 8-byte-int block): glibc tolerates it; the traced run
  completes; **valgrind records the invalid access**; ASan stage skipped
  (finding F-02).
- B8: caller site resolved to the user source line (`audit_b2_heap.c:3`).

## 7. Memory validity / raw bytes

- B9: `known = 0x11223344` reads back as `44332211…` (little-endian x86-64)
  and `{1,-1,256}` as `01000000 ffffffff 00010000` — exact object
  representation, not padded zeros.
- Failed reads are represented via `memory_read_error` (field present on
  block records); no case in this audit required it, so "no zero-substitution
  on read failure" could not be exercised live — the field's existence plus
  E12/E19 (no fake zeros in valgrind summaries) is the available evidence.

## 8. Snapshot consistency

- E6 (checkpoint refs) and E18 (positions monotonic and within event range)
  pass on all 27 traces.
- Completeness flags are internally consistent (E12): no snapshot claims
  `complete:true` while carrying errors/truncation.
- Journal mode: `--journal-only` (J1) writes a full recoverable journal
  (E20: parsed, event kinds present) while the stdout trace honestly retains
  0 instruction events (they live in the journal) and keeps
  `instruction_count = 37` and `resident_event_bytes` accounting.

## Verdict on trace correctness

No incorrect execution history, no fabricated instructions, no invented
memory state, no source/assembly desynchronization was found anywhere in the
27-test campaign. The honesty mechanisms (unknown outcomes, gaps, truncation
reasons, completeness flags) behave as documented. Findings that remain are
metadata- and coverage-gating issues (F-01..F-05), not history corruption.

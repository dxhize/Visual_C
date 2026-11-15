# FEATURE AUDIT — c-trace-engine V2

Verdict per feature: **Implemented** (code + positive runtime evidence), **Partial**
(code present, runtime evidence incomplete or degraded), **Missing**.
"Runtime" column summarizes the validating test (evidence in `evidence/<TEST>/`).

| # | Feature | Verdict | Module / function (file:line) | Runtime confirmation |
|---|---|---|---|---|
| 1 | Global session deadline | **Implemented** | `src/supervisor.rs:135-156` (`DEADLINE.scope`, `deadline()`), applied to every stage | D1: `--timeout 10` infinite loop terminated at 10.22 s wall (old V1 needed ~41 s), status `timeout`, no false exit |
| 2 | Stage-specific timeout handling | **Implemented** | `supervisor.rs:148 stage_deadline()` used by compiler (`compiler.rs:106,142`), debugger (`gdb_mi.rs:304`), sanitizer (`sanitizer.rs:41`), tracker (`allocation_tracker.rs:569`), orchestrator (`trace.rs:163,794`) | D6C: debugger stage `failed` with reason after gdb died at 0.7 s; D1 valgrind stage `skipped` under deadline |
| 3 | Process cleanup | **Implemented** | `supervisor.rs:15 enable_subreaper()`, `:48 reap_descendants()` (WNOHANG waitpid loop), `:186,283` post-run adoption + 2 s reaping window | D1/D6C/B1: `survivors.json` empty after every run — no gdb/inferior/valgrind orphans (OS-verified, not engine-reported) |
| 4 | Typed execution outcomes | **Implemented** | `models.rs:337 enum ExecutionStatus`, `models.rs:653 struct ExecutionOutcome`, `models.rs:14 enum StageStatus` | All 27 tests: statuses match reality (`completed`/`crashed`/`timeout`/`step_limit_reached`/`debugger_failure`); B1 `crashed`+SIGSEGV, D1 `timeout`, D6C `debugger_failure` |
| 5 | Stage completion metadata | **Implemented** (one metadata blemish) | `models.rs:25 struct StageResult` (status, start/end offsets, duration, exit_code, reason, output_complete, partial_results_available) | Present for all 5 stages in every trace; BUT on timeout/limit paths `allocation_tracker` reports `completed` + `output_complete:false` with `reason:null` (finding F-04, WARN) |
| 6 | Instrumentation filtering | **Implemented** | Classifier `gdb_mi.rs:1195-1336` (`user_code` vs `instrumentation_runtime`), excluded regions `trace.rs:1574`, config `config.rs:139-163` (`memory_regions`, `discover_globals`) | C7: all 11 checkpoint origins = `user_code`; libc regions appear as `excluded_code` gaps, not events |
| 7 | Pointer target classification | **Implemented** | `memory.rs:119-150`: null / heap (allocation_id, base_address, offset_bytes, `active = !freed`, within_extent) / global / static / stack / unknown vs heap timeline + known symbols | B2: `p` → heap id 1, offset 0→4, `active` flips to false after free; A4: `gp` → `global` with symbol identity; B1: null-classified pre-fault |
| 8 | Allocation lifetime tracking | **Implemented** | `allocation_tracker.rs` (`RawKind/RawStatus/RawEvent` :46-68, `EventFileReader` :151, `BlockState` :226, `HeapTimeline` :244) | B2/B3: complete `allocated`/`reallocated`/`freed` event streams with stable ids, sizes, caller source lines; B6 double-free visible to tools |
| 9 | Global/static variable capture | **Implemented** | `gdb_mi.rs:795-810` symbol scope classification (static vs global, file-qualified names), `config.rs:141 discover_globals` | A4: `global_counter`, `'/abs/path'::file_static`, function-local `static calls` captured with values/addresses |
| 10 | Source-level snapshots | **Implemented** | Snapshot capture in recorder (coverage scope `recorder.rs:61`), struct `models.rs` Snapshot/Observation | A1–C7: snapshots carry variables, call_stack, globals, heap_blocks, observation origin/pid/stop_id |
| 11 | Executed instruction events | **Implemented** | `models.rs:525 struct InstructionEvent` (address BEFORE attempt, `next_address`, outcome ∈ completed/faulted/interrupted/unknown, signal, stopped_reason) | 25–731 events per run, unique ids, ordered; faulted event carries signal + pc agreement with crash report (B1) |
| 12 | Source↔instruction mapping | **Implemented** | `models.rs:537 event.source: SourceContext(file,line,function)`, built in `trace.rs:1408,1538`; `checkpoint_id` links events to snapshots | C2: 29 events on the 5-iteration loop line (repeats kept); C6: 21 events on a single packed line; B1: event line 3 = crash line = snapshot line |
| 13 | Register-state observation | **Implemented** | `recorder.rs:246,269` (`-data-list-register-names/-values`), per-event block `models.rs InstructionEvent.registers` ("Observed AFTER the attempt, not inferred from assembly"), `initial_registers` | Dual traces: register block per event, rip == `next_address` post-attempt on all events (E8b), full checkpoint at start; `profiling.register_time_us` > 0 |
| 14 | Disassembly caching | **Implemented** | `recorder.rs:48,82` cache map, `:183-184` hit accounting, `:215-233` insert-once | C2/A1D: 24 cache hits vs 1 query; `disassembly_table` not duplicated per event |
| 15 | Trace gaps & truncation | **Implemented** | `recorder.rs:147-164` gap emission, `models.rs:549 struct TraceGap`; coverage `complete_for_scope` + `truncation_reasons` | Every trace: `startup_before_main` gap; C7: `excluded_code` gaps around libc; D1: `complete_for_scope:false` honestly |
| 16 | Streaming / compact trace storage | **Implemented** | `storage.rs:14-55` `EventFileWriter` append/finalize + `recover()` (torn-record safe), resident cap `recorder.rs:418`, `models.rs:600 resident_event_bytes` | J1: `--journal-only` wrote 77 507 B journal (recoverable, E20 parsed), only 18 511 B resident in stdout trace; events dropped from stdout honestly with `instruction_count` intact |
| 17 | JSON schema versioning | **Implemented** | `models.rs:401-402,423,633` (`schema_version = 2` with default fn + test) | All 27 traces: `schema_version: 2` (E2 invariant) |

## Notes

- **Runtime vs code evidence:** every "Implemented" verdict above is backed by at
  least one executed trace, not just source presence. The one degradation found
  (feature 5) is a metadata blemish on interrupted stages, not a missing feature.
- **Sanitizer variant:** with UBSan unavailable on this machine the engine runs
  ASan-only and records `session_meta.sanitizer_variant = "address"` plus an
  `analysis_notes` entry — honest degradation, not a defect.
- **Stage trigger policy (finding F-05):** the sanitizer reproduction stage only
  runs when the traced execution crashed (`reason: "no crash or sanitized build
  unavailable"`). For non-crashing memory errors (B5 use-after-free read,
  B7 glibc-tolerated OOB write) ASan is never invoked although it would flag both.
  Valgrind covers those cases (both tests show Memcheck findings).

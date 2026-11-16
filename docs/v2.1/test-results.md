# V2.1 Test Results

## Summary

| Suite | Tests | Passed | Failed | Skipped | Blocked |
|---|---:|---:|---:|---:|---:|
| `cargo test --lib` (unit) | 86 | 86 | 0 | 0 | 0 |
| `cargo test --doc` | 0 | 0 | 0 | 0 | 0 |
| `tests/integration.rs` | 12 | 12 | 0 | 0 | 0 |
| `tests/v2.rs` (V2.1 regressions) | 22 | 22 | 0 | 0 | 0 |
| **Total (cargo)** | **120** | **120** | **0** | **0** | **0** |
| Audit campaign `run_audit.py` (A/B/C/D/J suites) | 30 | 29 | 0 | 0 | 1 |
| Validator E-invariants on 30 campaign traces + 6 examples | 36 traces | 36 clean | 0 | 0 | 0 |

Authoritative run: three consecutive full `cargo test` runs (default
parallelism) after the benchmark campaign — 120/120 in each. Warnings: none
from `cargo build`/`cargo fmt`/`cargo clippy --all-targets`.

Pre-modification baseline: 112 tests (86 unit + 12 integration + 14 v2).
V2.1 net change: −2 v2 tests replaced by new ones, +8 new tests → 22 v2 tests,
120 total.

## New/changed tests for V2.1

| Test | Covers |
|---|---|
| `loop_events_are_dynamic_and_registers_are_not_collected` (replaced the replay test) | Negative register proof: `register_time_us == 0`, no `registers` key in serialized event JSON, no `initial_registers`, caching + dynamics intact |
| `allocation_tracker_metadata_is_honest_when_timed_out` | F-01 (timeout) |
| `allocation_tracker_metadata_is_honest_at_step_limit` | F-01 (limit) |
| `allocation_tracker_reports_completed_for_terminal_runs` | F-01 (no false negatives) |
| `sanitizer_trigger_never_skips_the_pass` | F-02 mode 1 |
| `sanitizer_trigger_on_crash_runs_on_crash_and_skips_clean_runs` | F-02 mode 2 |
| `sanitizer_trigger_on_crash_or_valgrind_error_covers_noncrashing_valgrind_findings` | F-02 mode 3 (default) |
| `sanitizer_trigger_always_runs_even_for_clean_safe_programs` | F-02 mode 4 |
| `gdb_path_override_allows_failure_injection` | F-05 |
| `valgrind_path_override_allows_failure_injection` | F-05 |
| extended `selected_memory_output_caps…` | F-03 (single `max_output_bytes` entry) + F-04 (`execution_status == output_limit_reached`) |
| `processor.rs` unit tests | register replay tests removed with the machinery; validation tests retained |
| `cli.rs` config-merge tests | updated for removed/added flags (still passing) |

## Audit campaign (same drivers as the V2 audit)

`python3 test_reports/v2_validation/runner/run_audit.py` — suites A (basic),
B (memory), C (assembly), D (limits/tooling), J (journal): **29 PASS**.

- `D4` now reports `execution_status=output_limit_reached` with matching
  outcome (F-04 end-to-end; the V2 audit had `step_limit_reached` +
  `output_limit_reached`).
- `D1/D2/D3` interrupted runs pass the updated E13/E14 metadata invariants.
- `C4` register-capture invariants pass **inverted** for V2.1 (zero register
  queries prove removal).
- `D6A` (valgrind absent via PATH shadow) remains **BLOCKED** for the same
  documented reason as the V2 audit: execvp PATH resolution skips
  non-executable shadow files, so PATH-based absence injection is not
  achievable. V2.1 adds the correct mechanism instead — `--valgrind-path`
  failure injection, covered by `valgrind_path_override_allows_failure_injection`.
- `D6B` (garbage XML → no fabricated report) and `D6C` (gdb death) PASS.

All 30 campaign traces re-validated with the updated E-invariants: 0 fails,
0 warns.

## Example traces (validator run)

| Trace | Invariants PASS/FAIL/WARN |
|---|---|
| v21_normal.json | 15 / 0 / 0 |
| v21_loop.json | 19 / 0 / 0 |
| v21_heap.json | 19 / 0 / 0 |
| v21_crash.json | 21 / 0 / 0 |
| v21_dual.json | 19 / 0 / 0 |
| v21_bounded_infinite.json | 19 / 0 / 0 |

## Remaining warnings / known flakes

- **Load sensitivity (pre-existing).** The integration and v2 suites assert
  against real gcc/gdb/valgrind pipelines with 5–12 s session budgets. When
  the whole `cargo test` matrix runs while the machine is also churning
  benchmarks, individual tests can flake on deadline assertions (observed
  twice during development; each time a re-run without background load
  passed 120/120). This is a test-harness scheduling characteristic, not an
  engine defect: the engine correctly reports the deadline exhaustion. The
  suites already serialize the heaviest stages internally
  (`RECORDING`, `VALGRIND_SLOT` semaphores).
- D6A blocked (PATH injection impossible by design; `--valgrind-path` is the
  supported injection path — F-05).

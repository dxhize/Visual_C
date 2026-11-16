# V2.1 Audit Fixes Report (F-01 … F-06)

Root cause, change, test, and result for each finding from the V2 validation
campaign (`test_reports/v2_validation/`).

---

## F-01 — Allocation-tracker completeness metadata (MEDIUM)

**Root cause.** `trace.rs` built `stage_results.allocation_tracker` with
`output_complete = matches!(status, Completed|Crashed) && parse_errors.is_empty()`
but hardcoded `status = Completed|Failed on parse errors` and
`partial_results_available = !timeline.events().is_empty()`… in practice an
interrupted run (deadline/limit/cancel) with zero-or-some events could
serialize `status:"completed", output_complete:false,
partial_results_available:false`, implying finished analysis of an unfinished
program.

**Code changed.** `src/trace.rs` (allocation-tracker stage builder):

- `tracker_terminal = status ∈ {Completed, Crashed}` decides `Completed`.
- Interrupted states map honestly: cancelled → `Cancelled` + reason;
  `Timeout` → `TimedOut` + "program timed out; allocation events up to the
  last observed stop were ingested"; step/output/resource limits → `Failed`
  + reason naming the limit; any other non-terminal status → `Failed` +
  reason quoting the actual status.
- `output_complete = tracker_terminal && parse_errors.is_empty()`.
- `partial_results_available = !timeline.events().is_empty()` — kept, now
  accurate because the status no longer lies.
- Already-recorded allocation events are preserved in all cases (the
  `recorder.allocations()` ingest and `heap_events` assembly are unchanged).

**Test.** `tests/v2.rs::allocation_tracker_metadata_is_honest_when_timed_out`
(timeout 2 s, allocating fixture: asserts `TimedOut`, reason present,
`output_complete=false`, `partial_results_available=true`, heap events
preserved), `::allocation_tracker_metadata_is_honest_at_step_limit`
(`max_steps=50`: asserts `Failed`, reason names the resource limit, partial
events available), `::allocation_tracker_reports_completed_for_terminal_runs`
(completed run still reports `Completed`, `output_complete=true`).

**Result.** PASS (see `docs/v2.1/test-results.md`). No remaining limitation
beyond the documented one: when the program never allocated before the
interruption, `partial_results_available` is honestly `false`.

---

## F-02 — Sanitizer only runs when the program crashes (MEDIUM)

**Root cause.** The sanitizer pass was gated on `status == Crashed` only, so
a program that exits cleanly while committing Valgrind-detectable memory
errors (e.g. use-after-free) never got the independent ASan confirmation.

**Code changed.**

- New `config::SanitizerTrigger` enum (`never | on_crash |
  on_crash_or_valgrind_error | always`, serde snake_case + clap ValueEnum,
  default `on_crash_or_valgrind_error`), plumbed through `TraceConfig`,
  `ConfigFile`, and CLI `--sanitizer-trigger`.
- Stage ordering is necessity-based (`src/trace.rs`): when the trigger is
  already satisfied without Valgrind (a crash, or mode `always`) the
  sanitizer runs FIRST — the V2 schedule — so the heavier Valgrind stage
  cannot crowd it out of the session deadline; only when the sanitizer
  decision depends on Valgrind findings (`on_crash_or_valgrind_error` with a
  clean exit) does Valgrind run first and the sanitizer key off
  `!valgrind_report.errors/detailed_errors.is_empty()`.
- The two stages instrument **separate processes** (ASan runs the sanitized
  binary; Valgrind runs the debug binary), so they are never co-instrumented.
- Stage deadlines, the overall session deadline, and engine cancellation are
  respected; skipped passes record an explicit reason string (mode name and
  why the trigger did not fire).
- `crash_report.asan_summary` is merged only when a crash exists (unchanged
  `crash.as_mut()` logic); `diagnostic_observed` downgrade when the ASan
  finding does not match the recorded crash is preserved; the traced run's
  own outcome is never altered by sanitizer findings.

**Documented default.** `on_crash_or_valgrind_error`. (The brief allowed
keeping the old behavior as default if justified; we chose the honest default
and kept `on_crash` available for exact V2 behavior, including its
crash-first scheduling.)

**Test.** `tests/v2.rs`: `sanitizer_trigger_never_skips_the_pass`,
`sanitizer_trigger_on_crash_runs_on_crash_and_skips_clean_runs`,
`sanitizer_trigger_on_crash_or_valgrind_error_covers_noncrashing_valgrind_findings`
(use_after_free.c exits cleanly, Valgrind errors trigger the ASan pass),
`sanitizer_trigger_always_runs_even_for_clean_safe_programs`.

**Result.** PASS. **Remaining limitation** (documented, per brief): ASan is
not required to reproduce every Valgrind finding — inconclusive reproduction
results are preserved as `inconclusive`/`diagnostic_observed`, never
fabricated as `reproduced`.

---

## F-03 — Duplicate `limits_triggered` entries (LOW)

**Root cause.** Five independent push sites could record the same limit
twice — notably `max_output_bytes` pushed inside the stepping loop and again
after the loop.

**Code changed.** `GdbPhaseResult::push_limit_once()` (dedupe by name) is now
the single authoritative insertion path for every fixed limit name, including
error-derived entries (`error.to_string()`), the timeout-name branches, and
the post-loop `max_output_bytes` recording. `ResourceUsage.limits_triggered`
inherits the deduplicated vector.

**Test.** `tests/v2.rs::selected_memory_output_caps_and_missing_mappings_are_explicit`
now asserts the `max_output_bytes` entry appears exactly once under an
output-limit run.

**Result.** PASS.

---

## F-04 — Execution-status taxonomy (LOW)

**Root cause.** The output-limit termination was set as
`ExecutionStatus::StepLimitReached` at two sites (in-loop and post-loop) plus
two `EngineError::LimitReached("max_output_bytes")` sites, then the outcome
mapper special-cased `StepLimitReached + limits_triggered ⊇ {max_output_bytes}`
into `ProgramOutcome::OutputLimitReached` — producing the audit's mismatch
(`execution_status=step_limit_reached` vs `outcome=output_limit_reached`).

**Code changed.**

- New `models::ExecutionStatus::OutputLimitReached` variant (serializes as
  `output_limit_reached`).
- All four set-sites now use it: in-loop output check, post-loop check, and
  the two `LimitReached(name)` error paths (which now discriminate on the
  limit name).
- The outcome mapping gained the direct arm `OutputLimitReached =>
  OutputLimitReached` and dropped the `max_output_bytes` special case from
  the `StepLimitReached` arm.
- Termination handling (inferior-tree kill) treats the new status like the
  other limit statuses; the debugger stage builder maps it through the
  existing catch-all to `StageStatus::Completed` (the debugger stage itself
  did complete its work; the limit was on captured output — same treatment
  as `StepLimitReached`).

**Test.** `tests/v2.rs::selected_memory_output_caps_and_missing_mappings_are_explicit`
asserts `execution_status == OutputLimitReached` **and**
`execution.outcome == output_limit_reached` — they now agree.

**Result.** PASS.

---

## F-05 — External tool resolution (LOW)

**Root cause.** Resolution was implicit PATH semantics; the audit could not
inject failures by shadowing PATH (documented reason: `Command::new` execvp
semantics skip non-executable PATH matches, so a shadow file with mode 644
cannot satisfy discovery — the audit's D6A shadow test actually *failed to
shadow*).

**Code changed.**

- Resolution is now explicit and documented: PATH-based via execvp semantics
  (non-executable matches skipped), normal discovery unchanged.
- New overrides: `TraceConfig.gdb_path` / `TraceConfig.valgrind_path`
  (config file and CLI `--gdb-path` / `--valgrind-path`, CLI taking
  precedence). The gdb override is honored at GDB spawn (`gdb_mi.rs` launch)
  and at the version probe; the valgrind override at the version probe and
  `valgrind::run_valgrind` spawn.
- Missing executables surface as `EngineError::MissingTool` (gdb → engine
  failure with reason; valgrind → stage skipped with an explicit
  analysis note), unchanged from V2.

**Test.** `tests/v2.rs::gdb_path_override_allows_failure_injection` (bogus
gdb path → `MissingTool`),
`::valgrind_path_override_allows_failure_injection` (bogus valgrind path →
GDB trace still completes, valgrind stage skipped with note, no fabricated
report).

**Result.** PASS. Discovery was not weakened: no PATH pruning, no
which-based shortcuts added.

---

## F-06 — Profiling metadata inconsistency (LOW)

**Root cause.** `data.profiling.enabled = config.profiling` with the config
default `false`, so every default run serialized `enabled:false` alongside
populated timing metrics.

**Code changed.** `recorder.rs` sets `data.profiling.enabled = true`
unconditionally: baseline metrics (step/disassembly/capture/command times,
cache stats, engine RSS) are always collected. The `tracing.profiling` config
key and CLI `--profile` remain as deprecated no-ops for config-file
compatibility. `register_time_us` stays at `0` and is documented as the
removed mechanism's counter — it is never reused for other debugger work, so
historical V2 numbers remain comparable.

**Test.** All six regenerated example traces
(`examples/v21_manifest.json`) serialize `profiling.enabled == true` and
`register_time_us == 0`; the negative register test asserts the timing
directly.

**Result.** PASS.

---

## Summary

| Finding | Status | Key regression test |
|---|---|---|
| F-01 | fixed | allocation_tracker_metadata_is_honest_* |
| F-02 | fixed (configurable, necessity-ordered) | sanitizer_trigger_* |
| F-03 | fixed | max_output_bytes counted once |
| F-04 | fixed | execution_status == output_limit_reached |
| F-05 | fixed (documented + overridable) | *_path_override_allows_failure_injection |
| F-06 | fixed (enabled always true) | example manifest / negative register test |

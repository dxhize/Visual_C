# ISSUES — c-trace-engine V2 validation

Sorted by severity. No CRITICAL or HIGH findings. Every finding was reproduced
against the checksummed engine build; none were fixed during the audit.
"Reported-by" names the validator check; raw evidence under `evidence/<TEST>/`.

---

## F-01 · MEDIUM · Stage-completeness metadata wrong on interrupted runs

- **Tests:** D1, D2, D3, D4 (all interrupted-by-design runs)
- **Expected:** a stage that could not finish its output reports
  `status ∈ {failed, cancelled, partial}` **or** `completed` with a
  `reason` explaining the incompleteness.
- **Actual:** `allocation_tracker` reports
  `{"status":"completed", "output_complete":false, "partial_results_available":false, "reason":null}`
  whenever the traced execution was killed by deadline/limit.
- **Reported by:** `E:E14_stage_statuses` (WARN) on D1–D4.
- **Evidence:** `evidence/D1/trace.json` → `stage_results.allocation_tracker`
  = `{"status":"completed","start_offset_ms":619,"end_offset_ms":10109,
  "duration_ms":9489,"exit_code":null,"reason":null,"output_complete":false,
  "partial_results_available":false}`.
- **Reproduction:** `target/debug/c-trace-engine fixtures/c/spin_same_line.c --timeout 10 --max-steps 1000000`
- **Frequency:** 4/4 interrupted runs (deterministic); not triggered on clean runs.
- **Suspected cause (hypothesis):** the tracker stage is marked `completed`
  when its worker finishes (including deadline kill) and only `output_complete`
  is flipped; the reason field is never populated on that path.
- **Recommended action:** set `status = "cancelled"/"partial"` or populate
  `reason` (e.g. "inferior terminated before tracking completed") on the
  interrupted path; GUI consumers currently cannot distinguish a complete
  tracker pass from a truncated one except by reading `output_complete`.

## F-02 · MEDIUM · Sanitizer reproduction stage gated on crash outcome

- **Tests:** B5 (use-after-free read), B7 (OOB write tolerated by glibc)
- **Expected:** when analysis tools report memory errors (valgrind
  `InvalidRead`, invalid write), the ASan reproduction stage runs to
  independently confirm them — ASan supports both error classes.
- **Actual:** sanitizer stage is `skipped` with
  `reason:"no crash or sanitized build unavailable"` whenever the traced
  execution itself did not crash; `sanitizer_report` is `null`.
- **Reported by:** `B5_asan` (WARN), `B7_asan_overflow` (WARN); stage dump.
- **Evidence:** `evidence/B5/trace.json` → `stage_results.sanitizer`;
  valgrind DID report `InvalidRead of size 4` for B5 and an invalid access
  for B7 — the independent confirmation channel is left unused.
- **Reproduction:** `target/debug/c-trace-engine test_reports/v2_validation/fixtures/audit_b7_oob.c --timeout 20`
- **Frequency:** 2/2 non-crashing-with-findings runs.
- **Suspected cause (hypothesis):** stage scheduler triggers sanitizer
  reproduction only when `execution_status == crashed`, and takes the "no
  crash" shortcut before consulting valgrind findings.
- **Recommended action:** also trigger the ASan run when
  `valgrind_report.errors` is non-empty (or expose a config flag), so
  tool-reported memory errors get the second opinion the pipeline promises.

## F-03 · LOW · Duplicated entry in `limits_triggered`

- **Test:** D4
- **Expected:** one entry per triggered limit.
- **Actual:** `resource_usage.limits_triggered = ["max_output_bytes", "max_output_bytes"]`.
- **Reported by:** manual inspection during D4 review (E13 invariant passed).
- **Evidence:** `evidence/D4/trace.json` → `resource_usage.limits_triggered`.
- **Reproduction:** `target/debug/c-trace-engine test_reports/v2_validation/fixtures/audit_d4_output.c --trace-mode source --max-output-bytes 16384 --timeout 60`
- **Frequency:** deterministic on this run.
- **Suspected cause (hypothesis):** the limit is recorded once at detection
  and once at finalization without a uniqueness guard.
- **Recommended action:** deduplicate on insert (or record trigger count).

## F-04 · LOW · Status taxonomy conflates output limit with step limit

- **Test:** D4
- **Expected:** distinct `execution_status` (or unambiguous taxonomy) for
  output-limit termination.
- **Actual:** `execution_status = "step_limit_reached"` while
  `execution.outcome = "output_limit_reached"` (the accurate one) and
  `limits_triggered` names the real cause.
- **Reported by:** `D4_no_status_confusion` (WARN).
- **Evidence:** `evidence/D4/trace.json`.
- **Reproduction:** as F-03.
- **Frequency:** deterministic on this run.
- **Suspected cause (hypothesis):** status enum lacks an
  `output_limit_reached` variant, so the generic limit status is reused.
- **Recommended action:** add the dedicated status variant or document
  `execution.outcome` as the authoritative field.

## F-05 · LOW · External tools resolved outside PATH (blocks absence simulation)

- **Test:** D6A (valgrind-missing simulation) — **BLOCKED**
- **Expected:** removing valgrind from PATH yields a `skipped`/`failed`
  valgrind stage with a reason.
- **Actual:** the engine still executed real valgrind
  (`stage completed, exit 0, summary_available true`) despite the shadowed
  PATH; only a root/container-isolated simulation or an engine change can
  hide it.
- **Reported by:** `D6A_path_shadow_ineffective` (BLOCKED).
- **Evidence:** `evidence/D6A/trace.json` (stage ran 1 443 ms with a real
  summary) vs `evidence/shadowbin/` shadow.
- **Reproduction:** `PATH=<dir-without-valgrind> target/debug/c-trace-engine test_reports/v2_validation/fixtures/audit_a1_vars.c`
- **Frequency:** deterministic.
- **Suspected cause (hypothesis):** tool lookup uses an absolute/configured
  path rather than `PATH` (reliability upside, testability downside).
- **Recommended action:** document the resolution rule; optionally add a
  `--tools-prefix` style override for test harnesses.

## F-06 · LOW · `profiling.enabled` false while profiling data is populated

- **Tests:** all dual runs (observed on A1D, C2, C4, all bench runs)
- **Expected:** `profiling.enabled` reflects whether the profiling block is
  active.
- **Actual:** `profiling.enabled = false` while all timing fields
  (`register_time_us`, `disassembly_cache_hits`, …) are populated and used
  in this audit's analysis.
- **Reported by:** manual inspection (no dedicated invariant).
- **Evidence:** `evidence/A1D/trace.json` → `profiling`.
- **Reproduction:** any dual-mode run.
- **Frequency:** all runs.
- **Suspected cause (hypothesis):** flag means "extended profiling requested",
  while baseline instrumentation timings are always recorded.
- **Recommended action:** rename/re-document the flag (e.g.
  `extended_profiling`) or set it true when timings are collected.

---

## Observations (not defects)

- **O-01 · Crash in excluded (untraced) code** (B6): the faulted-instruction
  event is absent because the abort happened in libc; `crash_report`
  (pc + SIGABRT) and coverage gaps carry the information honestly. If the
  product ever wants `faulted` events for library-region crashes, that is a
  feature, not a fix.
- **O-02 · Register full-checkpoint cadence (256) untested**: fixtures never
  exceeded 256 events except D1/D4 (596/0 events); D1's 596 events
  exercised the cadence (E8c PASS there) — cadence beyond that and
  reconstruction cost at scale remain unmeasured.
- **O-03 · B9 "failed read" path unexercised**: no fixture produced a
  `memory_read_error`, so the no-zero-substitution behavior is verified only
  structurally (field exists; E12/E19 no-fake-zero checks).

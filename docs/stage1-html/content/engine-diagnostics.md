# Independent diagnostics · Valgrind and sanitizers

## Level 1 · Intuition

GDB shows what happened in the **primary** traced run. Valgrind and sanitizer runs can detect memory problems the primary trace does not prove, but they execute the program again. A similar address or source line across executions is not the same allocation. A diagnostic belongs to its own run; missing or truncated output is not a clean bill of health.

{{diagram:metadata}}

## Level 2 · Stages and uncertainty

Valgrind runs the compiled program independently and produces native XML. `parse_valgrind_xml` decodes complete error records, native summaries and leaks. If XML is truncated, `recover_errors` can retain complete `<error>` elements while marking output incomplete and summary numbers unknown (`observed_summary`, nullable observed leak totals and parse error); a zero legacy total when the observed field is absent is not a measured zero. The runtime wrapper is passive in a Valgrind process so its table does not hide leaks as still-reachable objects.

An ASan/UBSan-compiled executable is run separately when `sanitizer_trigger` selects `never`, `on_crash`, `on_crash_or_valgrind_error` (default) or `always`. Reports and `StageResult` distinguish skipped, timed out, failed and completed passes. An ASan diagnostic is attached to a primary crash only with supported source-site correlation; otherwise it stays an independent `diagnostic_observed`. A nonzero sanitizer exit with no diagnostic is inconclusive. Stage budget/cancellation may prevent analyses after a deadline; program `execution_status` is not overwritten by an unrelated Valgrind failure.

### Worked crash: useful prefix, separate completeness

```c
// Source: fixtures/c/phase8r3_crash.c
int observed_global = 123;
int main(void) { *(volatile int *)0 = observed_global; return 0; }
```

In a fresh audited run, this produced `execution_status="crashed"`, a `SIGSEGV` crash report, **3** snapshots and **5** retained instruction attempts. `coverage.complete_for_scope=true` for the observed user-code interval, while `heap_history.complete=false` because the allocation tracker did not receive a normal terminal ACK. These are **different dimensions**: neither the crash nor heap partiality erases the captured stops, and a complete user-code scope does not make heap history complete. With a forced sanitizer pass, the separately run sanitizer also produced a diagnostic. Do not infer unrecorded writes or a complete process-tree history from the retained prefix.

### Source site is not identity

`src/valgrind.rs::correlate_with_allocations` is a legacy-name helper that now ignores supplied primary allocation IDs; it may produce ID-free **same-absolute-file/line candidates** but never an exact cross-run allocation link. Imported legacy `related_allocation_ids` can remain in raw reports, yet TP-6 `valgrind_error(index).primary_allocation_id` is **always `None`**. See [confidence contracts](parser-semantic.html#independent-diagnostics).

## Level 3 · Models and real failure path

- `src/valgrind.rs::{parse_valgrind_xml, recover_errors, correlate_with_allocations}` and `src/sanitizer.rs::{parse_sanitizer_output, matches_crash}`.
- `src/trace.rs` schedules passes and records `stage_results`; `src/models.rs::{ValgrindReport, ValgrindError, SanitizerReport, CrashReport, StageResult}` holds outputs.
- `ValidatedSemanticView::diagnostics()` borrows the session reports; `valgrind_error(index)` provides a trace-local reference to an **independent** diagnostic.

```rust
let diagnostics = validated.diagnostics();
if let Some(report) = diagnostics.valgrind {
    for index in 0..report.detailed_errors.len() {
        let item = validated.valgrind_error(index)?;
        assert!(item.primary_allocation_id.is_none());
    }
}
```

The code assumes a previously defined `validated` view; see the runnable [usage example](usage.html#level-3-load-and-validate). In the final Stage 1 audit, a fresh Valgrind fixture produced one native diagnostic and **zero** primary allocation links. Crash/forced-sanitizer samples each retained only the reports actually observed. A missing report is never a negative diagnosis.

> WARNING: A primary-run allocation ID found in an imported Valgrind raw field is not an authorized Stage 2 identity join. Do not use matching source, address, PID or name across executions as a substitute for same-execution event evidence.

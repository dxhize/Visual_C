# 19 · Independent diagnostics · Valgrind and sanitizers

> **In one sentence:** Memory-error tools add valuable clues, but they run *separately* from the main GDB observation, so their findings belong to their own runs.

## Key terms

| Term | Meaning |
|---|---|
| **Valgrind** | Runs the program under memory checking and reports leaks or invalid accesses. Generally slower. |
| **Sanitizer** | Compiler-inserted checks in a separately built program, such as AddressSanitizer for memory misuse. |

> **Example.** A Valgrind leak at line 52 may point you at useful source code. It cannot identify "allocation 17" from the debugger's run just because that run also visited line 52.
>
> Each analysis records its own status. A **skipped or failed** analysis is not the same as "checked, no errors found."

## Level 1 · Intuition

GDB shows what happened in the **primary** traced run. Valgrind and sanitizer runs can detect memory problems the primary trace doesn't prove, but they execute the program *again*.

- A similar address or source line across executions is **not** the same allocation.
- A diagnostic belongs to its own run.
- Missing or truncated output is **not** a clean bill of health.

{{diagram:metadata}}

## Level 2 · Stages and uncertainty

### Valgrind

| Point | Detail |
|---|---|
| How it runs | Starts **another execution** and writes an XML report |
| What the engine reads | Complete error entries, leak details and summary numbers |
| Truncated output | `recover_errors` keeps the `<error>` entries that were fully written, and marks the whole report **incomplete** |
| Missing leak total | Means "we don't know the total," **not** zero (even if an older default field contains `0`) |
| The allocation wrapper | Does **not** actively track blocks in the Valgrind run, because its own table could make leaks look reachable and hide the problem |

### Sanitizer

It is compiled into a **different executable**. The `sanitizer_trigger` setting chooses when it runs:

| Setting | Runs the sanitizer… |
|---|---|
| never | Never |
| after a crash | Only if the primary run crashed |
| after a crash or Valgrind error | **Default** |
| always | Always |

The trace records whether the stage was **skipped, timed out, failed or completed**.

| Situation | How it's treated |
|---|---|
| An ASan report and a primary GDB crash have a supported source-location match | May be shown together as **related clues** |
| No such support | The sanitizer finding stays **independent** |
| Nonzero exit, no usable diagnostic | **Inconclusive**, not proof of a memory bug |
| A deadline arrives | May stop optional checks |
| Valgrind fails | Does **not** change the status of the program's primary run |

### Worked crash: a useful prefix, separate completeness

```c
// Source: fixtures/c/phase8r3_crash.c
int observed_global = 123;
int main(void) { *(volatile int *)0 = observed_global; return 0; }
```

In a fresh audited run, this produced:

| Result | Value |
|---|---|
| `execution_status` | `"crashed"` |
| Crash report | `SIGSEGV` |
| Snapshots | **3** |
| Retained instruction attempts | **5** |
| `coverage.complete_for_scope` | `true` (for the observed user-code interval) |
| `heap_history.complete` | `false` (the allocation tracker got no normal terminal ACK) |

These are **different dimensions**. Neither the crash nor the heap partiality erases the captured stops, and a complete user-code scope does not make heap history complete. With a forced sanitizer pass, the separately run sanitizer also produced a diagnostic. Don't infer unrecorded writes or a complete process-tree history from the retained prefix.

### Source site is not identity

- `src/valgrind.rs::correlate_with_allocations` is a legacy-name helper that now **ignores** supplied primary allocation IDs.
- It may produce ID-free **same-absolute-file/line candidates**, but never an exact cross-run allocation link.
- Imported legacy `related_allocation_ids` can remain in raw reports, yet TP-6 `valgrind_error(index).primary_allocation_id` is **always `None`**.

See [confidence contracts](parser-semantic.html#independent-diagnostics).

## Level 3 · Models and real failure path

| File / item | Role |
|---|---|
| `src/valgrind.rs::{parse_valgrind_xml, recover_errors, correlate_with_allocations}` | Valgrind parsing and recovery |
| `src/sanitizer.rs::{parse_sanitizer_output, matches_crash}` | Sanitizer parsing and crash matching |
| `src/trace.rs` | Schedules passes and records `stage_results` |
| `src/models.rs::{ValgrindReport, ValgrindError, SanitizerReport, CrashReport, StageResult}` | Holds outputs |
| `ValidatedSemanticView::diagnostics()` | Borrows the session reports |
| `valgrind_error(index)` | A trace-local reference to an **independent** diagnostic |

```rust
let diagnostics = validated.diagnostics();
if let Some(report) = diagnostics.valgrind {
    for index in 0..report.detailed_errors.len() {
        let item = validated.valgrind_error(index)?;
        assert!(item.primary_allocation_id.is_none());
    }
}
```

The code assumes a previously defined `validated` view; see the runnable [usage example](usage.html#level-3-load-and-validate).

In the final Stage 1 audit, a fresh Valgrind fixture produced one native diagnostic and **zero** primary allocation links. The crash and forced-sanitizer samples each kept only the reports actually observed. A missing report is never a negative diagnosis.

> **Warning:** A primary-run allocation ID found in an imported Valgrind raw field is **not** an authorized Stage 2 identity join. Never use matching source, address, PID or name across executions as a substitute for same-execution event evidence.

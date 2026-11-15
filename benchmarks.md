# V2 benchmark measurements

Reproduce with `python3 scripts/benchmark.py` (build first: `cargo build`).
The script writes JSON to stdout; the checked-in copy lives at
[`docs/benchmark-results.json`](docs/benchmark-results.json). Every number below
comes from that file — nothing is estimated or extrapolated.

## Method

- Debug build (`target/debug/c-trace-engine`), binary SHA-256 recorded in the
  JSON. The traced programs are compiled by the engine itself (`-g -O0`), so
  compiler cost is inside the pipeline, not pre-shared.
- Three **serial** samples per case on one desktop machine. All pipeline stages
  run for every sample (compile + sanitized build, GDB recording, Valgrind on
  completed runs, sanitizer crash pass when applicable, tracker ingestion,
  JSON export). No cases were removed from the run.
- `events_per_debugger_second` counts recorded instruction attempts divided by
  the debugger stage duration. Fixed per-session costs (GDB startup,
  `-symbol-info-variables` discovery, breakpoint setup) are *not* excluded.

Environment: `Linux-7.2.15-200.fc44.x86_64-x86_64-with-glibc2.43`,
GCC 16.2.1, GDB 18.1, Valgrind 3.27.1, binutils addr2line 2.46.1.

## Results (three samples per case)

| Case | Flags | Wall (s) | Events | Events/debugger-s | JSON (B) | Journal (B) |
|---|---|---|---|---|---|---|
| `hello_source` | `--trace-mode source` | 3.91 / 3.91 / 5.85 | 0 / 0 / 0 | — | ~11.6 K | — |
| `hello_dual` | default (dual) | 6.08 / 5.17 / 4.54 | 7 | 4.5–5.1 | ~100 K | — |
| `loop_dual` | default (dual) | 7.00 / 5.97 / 5.93 | 97 | 24.7–32.4 | ~256 K | — |
| `loop_no_registers` | `--no-registers` | 4.51 / 4.50 / 4.35 | 97 | 58.9–67.0 | ~149 K | — |
| `loop_journal_only` | `--journal-only` | 6.00 / 6.32 / 5.98 | 97 | 31.6–33.3 | ~118 K | ~159 K |
| `tight_loop_journal_only` | `--no-registers --journal-only --max-instruction-events 2000 --analysis-timeout 1` | 8.61 / 8.74 / 7.97 | 2000 (capped) | 309.9–354.1 | ~26.4 K | ~817 K |

`hello_source` performs source-level checkpoints only, so it records no
instruction events by design. `tight_loop_journal_only` drives
`fixtures/c/spin_same_line.c` until `max_instruction_events` terminates
collection (`outcome: step_limit_reached`).

## Observed cost structure (from the recorded profiling counters)

- **Register reads dominate dual-mode stepping.** In `loop_dual`,
  `register_time_us` was 2.06 s of 3.74 s total command time; `--no-registers`
  removes it and roughly doubles event throughput (24.7–32.4 → 58.9–67.0
  events/debugger-s).
- **The disassembly cache is effective.** Each dual case issued exactly one
  `-data-disassemble` query (`disassembly_queries: 1`) and served every later
  stop from the cache (`disassembly_cache_hits: 96` of 97 events, 1999 of 2000
  in the tight-loop case).
- **Journal mode trades resident JSON for a streaming file.** In
  `loop_journal_only`, resident instruction events are evicted (JSON 118 K vs
  256 K) while the journal holds the ordered event stream (159 K). In the
  tight-loop case the journal reaches 817 K for a 26 K index JSON.
- **Fixed per-session debugger costs are significant for short sessions.**
  Measured repeatedly with GDB 18.1 on this machine: the first MI command
  costs ~200–300 ms (GDB startup work), `-symbol-info-variables
  --max-results 256` (globals discovery, default on) ~750 ms, and
  `-exec-run` ~90–115 ms. A `--timeout 2` dual-mode session can therefore
  spend most of its budget before the first recorded event.
- **Compile-stage variance is visible.** Wall samples for identical work vary
  by up to ~1.9 s (`hello_source`), reflecting ordinary desktop load; the
  engine's own stage timings (`stage_results.*.duration_ms`) show the split
  (compiler ~0.5 s, debugger ~1.5–2 s, Valgrind ~1.6–2.2 s for `hello.c`).

These are single-machine desktop measurements with concurrent user
applications running; treat them as order-of-magnitude evidence, not SLAs.
No number above is synthesized: all values are copied from
`docs/benchmark-results.json` of the final recorded run.

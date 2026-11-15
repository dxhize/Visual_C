# PERFORMANCE REPORT — c-trace-engine V2

Method: 6 bounded workloads × {native, source-trace, dual-trace} × **3 samples,
medians reported** (`evidence/benchmarks/raw_runs.json`, per-run traces in
`evidence/benchmarks/*.json`). Engine build identical for all traced runs
(`target/debug/c-trace-engine`, dev profile); native builds `gcc -g -O0` — the
same debug configuration the engine produces. External timing and peak RSS via
`/usr/bin/time` (parent + waited children); engine-internal timings from
`resource_usage`/`profiling`. Every engine call ran under a 150 s watchdog; all
runs completed normally (no limit truncation).

## Required comparison table (medians, n=3)

| Test | Native time | Source trace time | Dual trace time | Instruction events | Trace size |
|---|---|---|---|---|---|
| Arithmetic | 0.003 s | 3.51 s | 3.61 s | 18 | 120 KB |
| Finite loop (200 it) | 0.002 s | 3.49 s | 19.11 s | 1 012 | 1 294 KB |
| Nested loops (20×10) | 0.003 s | 10.09 s | 29.93 s | 1 352 | 3 418 KB |
| Recursion (depth 50) | 0.002 s | 7.23 s | 19.96 s | 671 | 3 713 KB |
| Heap workload (50 malloc/free) | 0.003 s | 10.97 s | 31.12 s | 1 363 | 5 615 KB |
| Call-intensive (100 calls) | 0.002 s | 4.93 s | 28.45 s | 1 413 | 2 598 KB |

Supporting counts (dual medians): snapshots 4–445, GDB commands 2 077–7 977,
engine peak RSS (self-reported) 11.4–17.3 MiB, external max RSS ≈ 149 MiB
(includes analysis-tool children such as valgrind — see caveats).

## Overhead analysis

- **Fixed session floor ≈ 3.5 s** (source mode on a 3 ms program). Stage
  decomposition (A1D trace): compiler ≈ 0.42 s, debugger ≈ 1.72 s,
  valgrind ≈ 1.47 s. The 1 170–3 655× "source overhead" ratios at this scale
  are therefore dominated by the fixed cost, not by per-event costs — treat
  ratios as scale artifacts, and read the absolute numbers instead.
- **Dual-mode marginal cost ≈ 15–22 ms per instruction event**
  (loop: (19.11−3.61)/1 012 ≈ 15.3 ms; calls: ≈ 17.1 ms; heap: ≈ 20.2 ms;
  recursion: ≈ 24.4 ms/event — call/return events are the most expensive).
  Throughput: **43–53 instruction events/s** on event-heavy workloads.
- **Source-mode marginal cost scales with snapshot count** (each snapshot =
  variable/global/heap reads): nested loops 445 snapshots → 10.1 s
  (~22 ms/snapshot); heap 355 → 11.0 s. Snapshot-light runs stay at the
  ~3.5 s floor.
- **Most overhead: heap workload** (31.12 s dual). Evidence for *where* the
  time goes is in the engine's own profiling block:

| Workload | register_ms | command_ms | step_ms | capture_ms | memread_ms | disasm_ms | events/s |
|---|---|---|---|---|---|---|---|
| bench_arith | 212 | 1 290 | 29 | 97 | 13 | 3.9 | 5.0 |
| bench_loop | 13 250 | 14 621 | 1 745 | 51 | 3 | 4.4 | 52.9 |
| bench_nested | 17 782 | 23 063 | 2 207 | 4 946 | 669 | 6.2 | 45.2 |
| bench_recurse | 9 124 | 13 458 | 1 343 | 3 300 | 146 | 8.6 | 33.6 |
| bench_heap | 18 233 | 23 190 | 2 599 | 4 513 | 719 | 8.3 | 43.8 |
| bench_calls | 19 368 | 22 159 | 2 625 | 1 778 | 167 | 10.2 | 49.7 |

## Bottleneck findings (evidence-backed)

1. **Register capture dominates dual mode.** `register_time_us` reaches
   9.1–19.4 s and is 78–91 % of `command_time_us` on event-heavy workloads
   (bench_loop: 13 250/14 621 ms = 90.6 %). Each instruction event pays
   ~2 GDB commands (2077 commands / 1012 events in bench_loop) and the
   `-data-list-register-values` round trip is the expensive part.
   *Hypothesis (labeled):* sampling all registers each event via MI costs
   one or more round trips; narrowing to changed-register subsets or
   batching would cut the dominant cost — optimization left for later.
2. **Snapshot capture dominates source mode for snapshot-heavy programs**
   (`capture_time_us` ≈ 4.9–5.2 s on nested/heap with 355–445 snapshots;
   GDB command counts 5.7k–6.0k). Variables/globals/heap-block reads per
   stop are the driver.
3. **Disassembly caching is fully effective.** Queries 1–2 vs hits equal to
   event counts (1 011–1 411); total `disassembly_time_us` 4–10 ms — ~100 %
   hit rate, matching the documented cache design; static disassembly is
   never a bottleneck.
4. **Serialization is cheap.** Journal write time ~16 ms (A1D
   `journal_time_us`); stdout trace JSON for the largest run (heap dual,
   5.6 MB) is produced within the measured wall time without visible export
   spikes; `storage.bytes_written` ≈ trace size (compact, no duplication).
5. **Memory.** Engine self-peak 11.4–17.3 MiB (grows with resident events);
   external max RSS ~149 MiB — dominated by analysis-tool children
   (valgrind), not the engine itself.

## Caveats (honest measurement limits)

- Native programs complete in 2–3 ms, so ratio-style overhead figures
  (×10³–×10⁴) are dominated by the fixed session cost; per-event and
  per-snapshot marginal costs above are the meaningful scaling metrics.
- External `/usr/bin/time` RSS covers the whole process tree including
  valgrind/ASan children; `profiling.engine_peak_rss_kib` isolates the engine.
- Register full-checkpoint cadence (256 events) only partially exercised
  (bench_loop 1 012 events crossed it; per-cadence cost not separately
  isolated from per-event costs).
- Single machine, 2 CPUs, load from concurrent audit driver — medians of 3
  dampen but do not eliminate scheduler noise (±0.3–1 s run-to-run observed).

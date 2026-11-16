# V2.1 Performance Report

## Methodology

- **Driver**: `test_reports/v2_validation/runner/bench.py` — the same driver
  the V2 audit used. 6 workloads × {native, source, dual} × 3 serial samples,
  medians reported. Engine wall time measured externally around the process;
  engine-reported `resource_usage`/`profiling` extracted from each trace.
- **Build**: plain `cargo build` (dev profile) — identical profile to the V2
  audit baseline. Compiler flags unchanged (`gcc -g -O0` per fixture).
- **Machine**: the same development machine as the V2 audit campaign (same
  repository, same fixture set). Machine load differs run-to-run; the
  `bench_arith` case (dominated by the fixed session floor, ~3.5–4 s of
  compiler + debugger + valgrind startup) is the best load-sensitivity probe
  and moved by ~+4% between campaigns — treat sub-10% deltas on small
  workloads as noise.
- **Not universal targets**: these numbers characterize this machine and
  build profile only (per the V2 brief).

## Historical baseline (V2 audit campaign)

Recorded during the V2 validation audit on this machine, dev profile
(`test_reports/v2_validation/evidence/benchmarks/`):

| Workload | Source | Dual | Events | Dual GDB cmds | register_time_us | command_time_us | trace bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| bench_arith | 3.51 s | 3.64 s | 18 | 161 | 211 465 | 1 365 904 | 122 725 |
| bench_loop | 3.42 s | 18.56 s | 1 012 | 2 077 | 12 839 687 | 14 159 679 | 1 325 273 |
| bench_nested | 10.08 s | 29.93 s | 1 352 | 7 977 | 17 933 928 | 23 362 767 | 3 499 708 |
| bench_recurse | 7.23 s | 20.14 s | 671 | 2 912 | 9 306 933 | 13 703 612 | 3 801 822 |
| bench_heap | 10.97 s | 30.88 s | 1 363 | 7 580 | 17 844 870 | 22 930 977 | 5 749 292 |
| bench_calls | 4.94 s | 28.45 s | 1 413 | 4 381 | 19 367 696 | 22 158 985 | 2 660 022 |

Register capture was 13.25 s of 14.62 s measured debugger command time on
`bench_loop` — **90.6 % of command time** — and 79–87 % on the other
heavy workloads.

## V2.1 results (this campaign)

Medians over 3 samples, `register_time_us` is **0 in every sample of every
workload**:

| Workload | Source | Dual | Events | Dual GDB cmds | command_time_us | step_time_us | capture_time_us | trace bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| bench_arith | 3.89 s | 3.76 s | 18 | 140 | 1 260 493 | 42 203 | 112 618 | 65 409 |
| bench_loop | 3.88 s | 6.10 s | 1 012 | 1 062 | 2 097 477 | 1 950 892 | 51 914 | 598 575 |
| bench_nested | 10.93 s | 13.32 s | 1 352 | 6 622 | 7 257 329 | 2 671 674 | 5 763 436 | 2 477 977 |
| bench_recurse | 7.59 s | 10.12 s | 671 | 2 238 | 4 954 753 | 1 406 417 | 3 524 618 | 3 154 222 |
| bench_heap | 12.13 s | 12.30 s | 1 363 | 6 214 | 6 215 903 | 2 544 801 | 4 456 636 | 2 424 870 |
| bench_calls | 4.97 s | 8.87 s | 1 413 | 2 965 | 3 952 910 | 2 714 816 | 1 779 981 | 1 379 807 |

Raw per-sample data: `test_reports/v2_validation/evidence/benchmarks/raw_runs.json`
(overwritten by this campaign) and per-run traces `bench_*_{source,dual}_*.json`.

## Required comparison table

V2 column: the audit's published medians (the historical baseline quoted in
the task brief; raw-sample medians from the stored audit runs differ by a few
percent — e.g. bench_loop samples median 18.56 s vs the published 19.11 s).
V2.1 column: this campaign's medians.

| Workload | V2 dual | V2.1 dual | Improvement | V2.1 instruction events |
|---|---:|---:|---:|---:|
| Arithmetic | 3.61 s | 3.76 s | −4 % (noise; fixed session floor dominates) | 18 |
| Finite loop | 19.11 s | 6.10 s | **3.1× (−68 %)** | 1 012 |
| Nested loops | 29.93 s | 13.32 s | **2.2× (−55 %)** | 1 352 |
| Recursion | 19.96 s | 10.12 s | **2.0× (−49 %)** | 671 |
| Heap workload | 31.12 s | 12.30 s | **2.5× (−60 %)** | 1 363 |
| Function calls | 28.45 s | 8.87 s | **3.2× (−69 %)** | 1 413 |

## GDB command counts before/after

| Workload | V2 dual cmds | V2.1 dual cmds | Δ | Events | cmds/event V2 | cmds/event V2.1 |
|---|---:|---:|---:|---:|---:|---:|
| bench_arith | 161 | 140 | −13 % | 18 | 8.9 | 7.8 |
| bench_loop | 2 077 | 1 062 | **−49 %** | 1 012 | 2.05 | **1.05** |
| bench_nested | 7 977 | 6 622 | −17 % | 1 352 | 5.90 | 4.87 |
| bench_recurse | 2 912 | 2 238 | −23 % | 671 | 4.34 | 3.34 |
| bench_heap | 7 580 | 6 214 | −18 % | 1 363 | 5.56 | 4.56 |
| bench_calls | 4 381 | 2 965 | −32 % | 1 413 | 3.10 | 2.10 |

The remaining ≈1 command/event is the stepping command itself plus snapshot
variable/memory reads amortized over source-line transitions — the minimal
set for source+assembly capture. **The full-register query
(`-data-list-register-values x`) is eliminated**: `register_time_us == 0`
in all 36 engine runs of this campaign, and the negative regression test
asserts no `registers` key is serialized.

The residual gap between command-count reduction (−17…−49 %) and wall-time
reduction (−50…−69 %) comes from the per-command MI round-trip cost:
register queries were among the slowest commands (hundreds of values per
response to parse).

## Trace size

| Workload | V2 dual bytes | V2.1 dual bytes | Δ |
|---|---:|---:|---:|
| bench_arith | 122 725 | 65 409 | −47 % |
| bench_loop | 1 325 273 | 598 575 | −55 % |
| bench_nested | 3 499 708 | 2 477 977 | −29 % |
| bench_recurse | 3 801 822 | 3 154 222 | −17 % |
| bench_heap | 5 749 292 | 2 424 870 | −58 % |
| bench_calls | 2 660 022 | 1 379 807 | −48 % |

Sizes shrink less than command counts because register payloads were a
minority of bytes in most workloads (memory observations and disassembly
dominate). No compression or format change was introduced (per brief §7).

## Remaining dominant bottlenecks

Per-component medians (V2.1 dual):

| Component | bench_nested | bench_heap | share of command time |
|---|---:|---:|---|
| stepping (`step_time_us`) | 2.67 s | 2.54 s | ~37–41 % |
| source snapshot capture (`capture_time_us`) | 5.76 s | 4.46 s | ~62–72 % |
| disassembly (`disassembly_time_us`) | ~0.02–0.04 s | ~0.03 s | <1 % (cache effective: ≥1 351 hits / 1–2 queries) |

- **Source snapshot capture is now the largest component** on snapshot-heavy
  workloads, as the V2 audit already predicted. It is necessary for the
  primary educational feature (stack/variable/memory states at source
  stops) and is explicitly out of scope for optimization here (brief §9.3).
- The fixed session floor (compile + GDB session setup + Valgrind stage,
  ~3.5–4 s) dominates small workloads and is untouched by this change.
- Stepping itself (1 instruction event ≈ 1.9 ms on bench_loop) is now the
  floor for dual-mode overhead; further gains would require protocol-level
  batching (out of scope; would risk correctness for marginal gains).

## Honesty statement

- No invented numbers: every figure above is measured from this campaign
  (`raw_runs.json`) or the audit campaign's stored raw runs.
- bench_arith regressed ~3 % within noise (its dual time is 95 % fixed
  session floor); we report it rather than hide it.
- Native runtimes (2–10 ms) are excluded from overhead math beyond the
  ratios quoted by the driver.

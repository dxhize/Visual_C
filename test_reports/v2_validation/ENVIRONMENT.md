# ENVIRONMENT — c-trace-engine V2 validation

Captured 2026-11-15 from `evidence/environment_probe.txt` (raw probe preserved).

## Machine
- OS: Fedora Linux 44 (Workstation Edition), x86_64
- CPUs: 2 logical
- RAM: 8 020 188 kB (~7.6 GiB)
- cgroup: controllers `cpuset cpu io memory hugetlb pids rdma misc dmem` present, **cgroup root not writable** → the engine cannot delegate container-style limits; per its own note it falls back to rlimits for the inferior (`analysis_notes`: "cgroup unavailable: per-tree memory/process limits not enforced; inferior rlimits remain active").

## Toolchain
| Tool | Version |
|---|---|
| rustc | 1.99.1 (58cf8f9c3 2026-10-13) |
| cargo | 1.99.1 (5f94df478 2026-08-27) |
| gcc | 16.2.1 20261029 (Red Hat 16.2.1-3) |
| gdb | GNU gdb (Fedora Linux) 18.1-1.fc44 |
| valgrind | 3.27.1 |
| addr2line | GNU 2.46.1-1.fc44 |
| python3 | 3.14.10 (audit driver only) |
| objdump/nm/readelf | binutils 2.46.1 present |

## Sanitizers
- **ASan: available and functional** (probe compiled and ran cleanly).
- **UBSan: NOT available** — `/usr/lib64/libubsan.so.1.0.0` missing at link time.
  The engine degrades gracefully: `analysis_notes` records "sanitizer build degraded
  to address; combined runtime unavailable" and `session_meta.sanitizer_variant`
  is `"address"`. All sanitizer-runtime evidence in this audit is ASan-only.

## Engine build and baseline
- Build: `cargo build` → `target/debug/c-trace-engine` (the documented dev build; no special features).
- Existing test suite at audit start: **112/112 passed**
  (87 unit + 12 legacy integration + 13 V2 integration; see `evidence/existing_tests.log`).
- Engine source integrity: checksummed before any test ran
  (`engine_source_manifest_before.txt`, md5 over `src/*.rs`, `c_runtime/*.c`,
  `scripts/benchmark.py`); re-verified unchanged at audit end.

## CLI surface (determined from `--help` and implementation, not assumed)
Positional: one main source file. Key flags used by this audit:
`--trace-mode source|dual`, `--add-source <file>` (additional translation units),
`--timeout <s>`, `--max-steps <n>`, `--max-instruction-events <n>`,
`--max-source-checkpoints <n>`, `--max-output-bytes <n>`,
`--journal-only` (requires explicit `--journal <path>`), `--journal <path>`.

Defaults observed in `execution_limits` of traces: `timeout_seconds: 10`,
`max_steps: 1000`, `max_output_bytes: 1 048 576`, `max_snapshot_count: 1000`,
`max_heap_allocations: 10 000`, `max_memory_mb: 256`.

## Engine exit codes (documented and observed)
- `0` clean run, `2` completed with findings (crash/analysis findings), `1` engine error.

## Watchdog policy for this audit
Every engine invocation ran under the audit driver's subprocess timeout
(150 s default; 90 s for the debugger-kill test D6C). No test exceeded it
except by design (D1's 10 s deadline is enforced by the engine, not the watchdog).

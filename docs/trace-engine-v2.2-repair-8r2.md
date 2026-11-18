# Trace Engine V2.2: Phase 8R-2 Instrumentation Reliability Repairs

Scope: F04 and F05 from `docs/trace-engine-v2.2-audit-8a.md` only. This
report records actual before/after tests and bounded guarantees rather than
claiming complete POSIX fork safety or an infallible filesystem.

## Baseline

Branch `main`, commit `486410b39646ee8c4ad20a527260d3a58929e2d1`.
Phases 1-7, 8A and 8R-1 were already uncommitted and are preserved. Phase 5's
event log format, per-PID identities, sequence checks, and 1 MiB incremental
event-reader cap remain the starting point.

## Fork: root cause, reproduction and guarantee

Pending focused before/after results and child-handler review.

## Event writes: root cause, reproduction and guarantee

Pending controlled failures and terminal-status integration results.

## Independent C and Rust verification

Pending raw log, PID/sequence, timeline and trace-JSON checks.

## Compatibility and files changed

Pending final diff. Schema marker remains 2; unrelated Clang, DWARF and Phase
8R-1 repairs are outside this phase.

## Full validation and benchmarks

Pending static checks, C test fixture, real GCC/GDB integration, serial suite
and representative paired performance measurements.

## Remaining limitations and Phase 8R-3 handoff

Pending final classification. Phase 8R-3 is not implemented here.

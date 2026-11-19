# Trace Engine V2.2: Phase 8R-2 Instrumentation Reliability Repairs

Scope: audit findings F04 (fork-under-lock) and F05 (terminal event-write loss),
not Phase 8R-3. Baseline was `main` at `486410b` plus the existing Phase 1–7,
8A and 8R-1 worktree. The implementation and much of the regression harness
were already present when this task was resumed; no previous uncommitted work
was discarded. Measurements below were made on this host; they are not
portable speed claims.

## F04: fork synchronization

The wrapper serializes real allocator calls, table updates, sequence numbers
and writes with `g_spin`. A second thread can fork while the owner holds that
lock: its child inherits `g_spin == 1`, but not the owner. The pre-repair
behavior is compiled on demand with test-only `CTE_TEST_DISABLE_FORK_RESET`.
`phase8r2_stress.c` uses a pipe handshake to establish that another thread
**already holds the tracker lock** at fork; the child attempts malloc/free.
Its three-second alarm makes the original deadlock terminate with a nonzero
child result, and the test confirms the pre-repair process fails in under six
seconds. The normal build passes the same fixture in a bounded subprocess.
The test runner kills the entire process group on timeout, not just the parent.

The repaired runtime registers a **child-only** `pthread_atfork` callback at
initialization. The callback performs only simple atomic/scalar resets: unlocks
the child's private copy of the spin word (it never unlocks the parent's
lock), resets its sequence, allocation-ID and write-failure counters, and
marks the inherited address table for reset. On the child's first wrapped
allocation, it clears the table *under the child's lock* before querying it.
No allocation, formatting, table traversal, I/O, or logging occurs inside the
atfork callback; no prepare handler holds the lock across arbitrary third-party
handlers. The thread-local recursion guard prevents tracker-triggered nested
wrappers; errno is restored after real allocator calls. If atfork registration
fails, instrumentation disables itself instead of claiming safe tracking.
The parent remains unchanged. Each logged child begins at local sequence and
ID 1 and has its own PID; there is no invented cross-process order or inherited
allocation identity. Freeing a block inherited across fork may be recorded
with an unknown local ID rather than the parent's ID.

Supported model: fork invoked outside a wrapper critical section and outside
foreign atfork handlers that subsequently acquire this lock; the child must
also obey the platform's post-multithreaded-fork restrictions. This mechanism
prevents *tracker-lock inheritance* deadlock, not deadlocks in libc, other
locks, application atfork handlers, or unsafe child allocator use on every
POSIX platform. The test exercises the child allocation path on the tested
Linux/glibc toolchain; it is not a general POSIX malloc-after-fork guarantee.

## F05: event-write loss and completeness

The old sequence-gap check detects lost events only if a later event appears.
A failing final `write()` can drop the last free while leaving a contiguous
observed prefix that previously looked complete. The wrapper now counts
failed event emissions after bounded short-write/EINTR handling (at most eight
EINTR retries; zero, EIO and exhausted retries fail). It writes a separate
terminal `pid seq failures` acknowledgement through a separately opened
status FD on normal termination. It does **not** assume an in-band marker can
be written to a broken event FD. An absent/failed/truncated ACK cannot certify
completeness. The ACK is bounded by the same 1 MiB read cap, validated as
ASCII decimal records, matched by PID against the observed timeline plus the
inferior, and checked for duplicates, sequence mismatch and writer failures.
The ACK itself cannot repair or authenticate a corrupt event stream.

The Rust `EventFileReader` still retains only a verified complete-line prefix,
buffers a short trailing line, limits each read to 1 MiB, and detects malformed
or oversized lines, replacement (device/inode) and truncation after a prior
read. Definitive transport damage quarantines future lifetime reconstruction;
missing terminal ACK or a failure count downgrades global heap history after
final ingestion. The recorder marks the tracker stage incomplete/failed and
propagates uncertainty into JSON `heap_history`, event usage completeness,
heap-block lifetime status and heap pointer enrichment. Previous observed
bytes/events are preserved; a lost tail is not invented. Temporary EINTR and
short writes that finish successfully leave the failure count at zero. The
user's malloc/free return value and errno remain those of the real allocator,
not the logging operation.

Controlled failure modes: a partial write followed by success, one EINTR,
EIO, zero-length write, repeated EINTR, a partial line followed by EIO, a
failed status write, and abnormal exit. The `error`, `zero` and
`eintr_exhausted` fixtures explicitly show the **old** sequence-only oracle
accepting the one-event prefix as complete before ACK validation rejects it;
`partial_error` also leaves a detectable incomplete line. No disk exhaustion
or unbounded retry is required.

## Independent verification and tests

- C raw-log oracle (`tests/heap_instrumentation.rs`): normal 2 events; repeated
  forks produce sorted per-PID counts `[2,2,2,4]` with seq/ID restart; four
  threads produce 160 contiguous events; fork while lock held produces child
  and parent counts `[2,4]`. Each PID has exactly one successful terminal ACK
  and its ACK sequence equals its raw event count. Partial/EINTR recover;
  terminal failure ACK says two attempted operations and one failed write.
- The same test now ingests **actual C-written** failed logs with production
  Rust `EventFileReader`/`HeapTimeline`, verifies the independent ACK, retains
  the one complete prefix event, and downgrades timeline completeness.
  `tests/heap_provenance.rs` checks malformed/missing/duplicate/wrong-PID
  ACKs; allocation-tracker units cover replaced/truncated/oversized logs and
  quarantine. `src/trace.rs` tests round-trip JSON after terminal failure:
  `heap_history.complete=false`, heap pointer `history_incomplete`, block
  lifetime `unknown` (not stale active provenance).
- Real GCC/GDB tests in the serial suite include `heap_provenance`, `v2` and
  `integration`. Separate release CLI probes: `allocator_edges.c` completed
  with 6 heap events, 245 GDB commands and complete history; `phase8r2_basic.c
  -- forks` completed with 10 retained events, 380 commands and complete
  history. The standalone wrapper oracle, not GDB's followed inferior, checks
  *all* children and their raw logs.

## Compatibility and file scope

Schema marker remains 2; no required new trace fields. Existing legacy
traces/Trace Parser tests pass. Event rows remain Phase 5 format; a separate
status file is internal instrumentation transport metadata. Realloc chains,
address-reuse evidence, Phase 8R-1 identity fixes, and multi-frame snapshots
remain governed by their existing tests. Relevant implementation:
`c_runtime/alloc_tracker.c`, `src/allocation_tracker.rs`, `src/trace.rs`,
`src/memory.rs`; fixtures `phase8r2_basic.c`, `phase8r2_stress.c`; tests
`heap_instrumentation.rs`, `heap_provenance.rs`, `v2.rs`. This continuation
added cross-layer raw-file ingestion assertions in `tests/heap_instrumentation.rs`
and completed this report; earlier worktree edits in those files were retained.

## Validation and performance

On this host, `cargo check --all-targets`, `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, and `cargo build --release` passed.
`cargo test --no-fail-fast -- --test-threads=1` passed 327 tests across all
targets, zero failures; log: `/tmp/visual-c-8r2-tests.log`. The focused
nine-test C wrapper suite was rerun after the last assertion change and passed;
format and Clippy were also rerun. The full suite predates that last
assertion-only test change. No failure was dismissed as flaky.

Paired **wrapper** measurements (three serial trials each) compare the same
C source with versus without the child-reset handler, using the test-only
pre-repair build switch. `/usr/bin/time` wall/RSS medians; raw event counts
and PID counts checked against ACKs on *every* trial:

| Input | Pre-repair wall / peak RSS | Repaired wall / peak RSS | Events / PIDs (both) |
|---|---:|---:|---:|
| 10,000 malloc/free pairs | 0.06 s / 1616 KiB | 0.06 s / 1608 KiB | 20,000 / 1 |
| 100 successive forks | 0.34 s / 4300 KiB | 0.35 s / 4260 KiB | 204 / 101 |
| 4 threads × 1,000 pairs | 0.04 s / 1872 KiB | 0.04 s / 1816 KiB | 8,000 / 1 |

Median raw event-log sizes: 986,682 / 986,682 bytes (allocation-heavy),
8,772 / 8,772 bytes (fork), 420,679 / 420,679 bytes (threads); PID digit
width varied between runs. Fork status logs were 1,111 bytes in both builds.
The ~0.01 s fork median difference is below meaningful resolution with three
short trials; no causal overhead claim. Release **engine** probes (single
runs; not paired pre/post engine comparisons): allocation fixture 3.52 s,
152,664 KiB `/usr/bin/time` peak RSS, 216,687 JSON bytes; fork fixture
3.92 s, 152,696 KiB, 249,510 JSON bytes. Their command counts and event
fidelity appear above. The two different workloads cannot yield an engine
trace-size or debugger-command *change* attributable to the repair; a
comparable pre-repair engine binary was not available. Benchmark artifacts:
`/tmp/cte-8r2-bench-m7im1m33/` (temporary, not committed).

## Limitations and Phase 8R-3 handoff

Abnormal termination or `_exit` produces no destructor ACK, so final history
is conservatively incomplete. A child that forks and never logs or ACKs is
undiscoverable to this transport; it cannot be claimed to have complete heap
coverage. A failed or missing independent status write also downgrades rather
than proving which event was lost. An attacker modifying log and ACK files
can forge consistency; the mechanism is failure detection, not authentication.
Concurrent writes by different PIDs are not a global order; damaged/interleaved
rows fail validation. No guarantee is made for allocator calls bypassing
`--wrap`, crashes before ACK, arbitrary custom fork handlers or fork invoked
from inside a wrapped allocation. Preserve these as explicit evidence gaps.
Phase 8R-3 is not implemented here.

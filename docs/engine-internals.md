# Engine internals

How `c-trace-engine` observes a program. The guiding rules are:

* **Never fabricate.** Everything in the trace came from a real observation
  (compiler output, a GDB response, an event line, a tool report).
* **Unobservable = `null`.** Fields the engine could not learn are absent or
  `null`, not guessed.
* **Partial results always survive.** Any stop condition degrades
  gracefully and the trace records what happened in `analysis_notes`.

## 1. Build & allocation instrumentation (`compiler.rs`, `c_runtime/alloc_tracker.c`)

Each translation unit is compiled with `-g -O0 -fno-omit-frame-pointer` plus
GCC 16's SARIF diagnostics on stderr (`-fdiagnostics-format=sarif-stderr`);
older GCC falls back to `-fdiagnostics-format=json` and then to plain text.
GCC 16 emits **one SARIF document per translation unit** concatenated on
stderr, so the parser consumes a *stream* of JSON documents
(`serde_json::Deserializer::from_str(..).into_iter::<SarifDoc>()`).

The debug link embeds the C runtime (`alloc_tracker.c`) and redirects the
program's allocator calls with `-Wl,--wrap=malloc --wrap=calloc
--wrap=realloc --wrap=free`. The wrappers:

* keep a small fixed-size process-local table of live blocks,
* write **one line per operation** to the file named by
  `C_TRACE_EVENT_FILE` (a dedicated file, never stdout/stderr, so program
  output and telemetry cannot interleave or deadlock),
* use a spinlock, no recursion and no allocation of their own — the tracker
  must not perturb the allocator it observes,
* assign per-process allocation ids; when an address is reused by a new
  block it gets a fresh id, so `address` alone is never an identity,
* report a **failed realloc** without freeing the original (glibc
  semantics), and `realloc(p, 0)` as a free.

Event line format:

```
<pid> <seq> <kind> <id> <addr-hex> <size> <old_id> <old_addr-hex> <caller-hex> <status>
```

`kind` ∈ `malloc|calloc|realloc|free`; `status` ∈ `ok|ok_same|ok_moved|new|
zero|failed|unknown|null`. `caller` is the return address of the allocator
call, resolved to `file:line` later by one batched `addr2line` run.

`__wrap_malloc` forwards to `__real_malloc` etc.; this covers the
*program's own* calls (and direct libc calls that land in the wrapped
symbols). Memory allocated inside libraries already linked against the real
allocator is invisible to the timeline — Valgrind is the independent check
that sees everything.

## 2. GDB session (`gdb_mi.rs`)

The session runs `gdb --nx --quiet --interpreter=mi2 <binary>` and speaks
MI2 with:

* **Token-matched commands**: every command is prefixed with a monotonically
  increasing token; the matching result record resolves a oneshot channel.
  Stream records (`~`, `&`, `@`) captured between command and response are
  delivered with the response (needed for console-mode results such as
  `whatis`).
* **A dedicated pty for the inferior** via `-inferior-tty-set`: program
  output never enters the MI stream. The master end is read by a tokio task
  into a capped ring buffer (`max_output_bytes`).
* **One channel** between reader task and session for async records
  (`*running`, `*stopped`, `=notify`); the reader fails all pending
  commands on EOF so a dead GDB never strands a wait.

Observed GDB 18 behaviors the engine adapts to:

* execution commands answer `^running` + async `*stopped` (including for
  exits: `*stopped,reason="exited-normally"`),
* a `finish` from the outermost frame returns
  `^error,msg="…finish… not meaningful in the outermost frame."`,
* GDB occasionally reports a `*stopped` **without reason** after failing to
  insert a temporary breakpoint while unwinding the outermost libc frame
  (`Cannot insert breakpoint 0. Cannot access memory at address 0x101`).
  Such stops cannot be finished; the engine continues instead, which lets
  the imminent exit happen naturally,
* GDB puts the inferior in its own process group, so killing GDB's group
  does not kill the inferior — the engine kills both groups.

Termination: every wait is timeout-bounded (per-command 20s, per-step 10s,
session wall = `4 × timeout_seconds + 30s`). On limit endings the engine
kills the process groups (GDB's and the inferior's) and the cgroup subtree
when available.

## 3. Execution loop & snapshots (`trace.rs`)

`run_gdb_phase` breaks at `main`, then:

1. ingest allocation events that accumulated while the inferior ran,
2. classify the stop: exit → `Completed` (+ exit code from the stop record
   or `=thread-group-exited`); signal → fatal (`SIGSEGV`, `SIGABRT`, … →
   `Crashed` + crash report), resource (`SIGXCPU`, `SIGKILL` →
   `ResourceLimitReached`), benign (`SIGPIPE`, … → continue, bounded by the
   step budget), no-reason stop → continue (see above),
3. capture one snapshot:
   * `-stack-list-frames` for the call stack,
   * `-stack-list-variables --all-values` + `whatis` per variable for types,
     own address, value and pointed-to address (`points_to`) — with raw
     bytes read from the inferior within the per-snapshot budget,
   * live heap blocks: for each tracked alive block one
     `-data-read-memory-bytes` bounded by `max_memory_read_bytes` and the
     remaining snapshot budget (blocks beyond the budget are recorded with
     `memory_read_error`, not dropped),
   * program output head (64 KiB cap per snapshot),
4. step: `-exec-step` in user code; in non-user frames `-exec-finish`
   (capped chain of 32, then `-exec-continue`).

Snapshots are capped by `max_snapshot_count`; the memory budget is shared
between variables and heap reads so one snapshot can never blow up the
trace.

## 4. Heap reconstruction (`allocation_tracker.rs`)

`HeapTimeline` ingests the raw event stream and reconstructs, per process:

* **blocks** — one `BlockState` per allocation with allocation id, address,
  size, type (`malloc`/`calloc`/`realloc`), realloc chain link
  (`realloc_from_id`), caller pc, and final status (freed or still alive),
* **events** — the full ordered history (allocated / reallocated / freed /
  allocation_failed) with optional human-readable details (double free,
  `free(NULL)`, failed realloc preserving the original),
* **live byte counters** per process.

Ids are per-process; when a run produced more than one pid the serialized
ids are widened to `(pid << 32) | id` to stay unique across the trace —
with a single pid they stay plain (matches the C runtime's view).

The final `heap_state` lists every reconstructed block. A block's
`raw_bytes_hex` is the last capture taken **while it was alive**; freed
blocks keep that historical capture.

## 5. Independent analyses

**Valgrind** (`valgrind.rs`) runs the debug binary (without instrumentation
wrappers — Valgrind replaces the allocator itself) under memcheck with
`--xml=file --xml-user-prog-args`-style options, parses the XML with
`roxmltree` (heap summary, error records with stacks, leak records) and
correlates findings with allocation ids by `(file, line)` pairs resolved by
addr2line. A Valgrind timeout leaves the partial XML; the engine notes it.

**Sanitizer pass** (`sanitizer.rs`) runs only after a crash: the ASan-built
binary executes once with the same arguments and working directory; its
stderr (`ERROR: AddressSanitizer: …`) is parsed into a structured report and
the `SUMMARY:` line is attached to the crash report as `asan_summary`.
Sanitizer runtimes are detected at link time: if libubsan is missing the
build degrades to ASan-only, and if libasan is missing entirely the pass is
disabled with a note. Sanitizer and Valgrind children get CPU/file-size
rlimits but **no address-space cap** — ASan maps terabytes of shadow memory
and aborts under a small `RLIMIT_AS`.

## 6. Resource controls (`resource_limits.rs`)

Layered enforcement:

1. **Engine clocks** — wall deadline, per-command and per-step timeouts;
   every path is bounded, verified with real GDB.
2. **rlimits** — `RLIMIT_CPU` (from `timeout_seconds`), `RLIMIT_FSIZE`,
   `RLIMIT_NOFILE` on sanitizer/valgrind children. `RLIMIT_NPROC` is
   deliberately never used: on Linux it is enforced per *real user id*, so
   a low value breaks every `fork` on the whole desktop session
   (empirically: `vfork: Resource temporarily unavailable` with 173 user
   processes).
3. **cgroup v2** — when a delegated subtree is writable, the inferior pid
   is moved into it with `pids.max`, `memory.max` and `cpu.max`; on teardown
   the subtree is killed (`cgroup.kill`). Best effort; its availability is
   reported in `session_meta.cgroup_enforced`.

These limits protect the engine and the session — they are **not a
security sandbox** for hostile binaries.

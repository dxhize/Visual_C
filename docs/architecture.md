# Engine architecture

A conceptual map of `c-trace-engine` — how it is organized and *why* it is
organized that way. It stays one level above code: no struct or function
names, no JSON fields. For the next level of detail see:

- [`v2.md`](v2.md) — what exactly gets recorded and what each observation means
- [`../README.md`](../README.md) — running and configuring the engine
- [`../benchmarks.md`](../benchmarks.md) — what things cost, measured

---

## 1. The one-sentence view

The engine is a **transcription service**: it compiles a real C program, runs
it once under real debugging and analysis tools, and writes down everything
those tools observed — as evidence, with honest holes where nothing was
observed.

Three commitments follow from that and shape every part of the system:

1. **Record, don't interpret.** The engine never executes C code, never
   guesses a state, never synthesizes a value. Anything unobserved is
   labeled as unobserved.
2. **One run, two lenses.** Source-level and instruction-level views come
   from a *single* execution under the debugger — never from a second pass,
   because a second run would be a different execution.
3. **Always produce a document.** Whatever happens — compile error, timeout,
   interruption, tool death — the output is a well-formed record of what
   did and did not happen. Failure is content, not an error page.

## 2. The mental model: three channels, one clock, one scribe

```
                 ┌──────────────────────────────────────────┐
   one clock ───►│            bounded session               │
                 │                                          │
   channel A ───►│  program under the debugger              │──┐
   channel B ───►│  allocation telemetry from inside the    │  │   scribe
                 │  program (its own file, own process)     │  ├──► one JSON
   channel C ───►│  independent examiners (Valgrind,        │  │   document
                 │  sanitizer) run after, never inside      │──┘
                 └──────────────────────────────────────────┘
```

- **The clock** bounds the whole session before anything starts. Every
  activity derives its own smaller deadline from the same starting instant;
  a stage may start late but can never push the end out. User interruption
  (SIGINT/SIGTERM) is a separate, gentler path: stop, clean up, publish what
  was gathered.
- **Channel A** — the debugger — is the only channel that advances the
  program. It is treated as a protocol peer, not a terminal: every request
  is matched to exactly one answer, and background events arriving out of
  band are routed separately so they can never be mistaken for an answer.
- **Channel B** — heap telemetry — never touches the debugger. The program
  itself is instrumented at link time to append one line per allocation
  event to a private file; the engine reads that file only while the
  program is stopped, so its contents are always consistent with the
  observation being made.
- **Channel C** — the independent examiners — run *after* the recorded
  execution, under their own budgets, never nested inside each other or the
  debugger. Their findings are correlated with a crash only when actual
  evidence (matching source sites) supports it; otherwise they stand as
  separate observations.

Keeping the channels separate is the point: if the debugger were also the
allocator observer, a debugger bug could fabricate heap history; if
Valgrind ran under the engine's instrumentation, the instrument's own
bookkeeping would masquerade as reachable memory. Independence is what
makes cross-checking meaningful.

## 3. The heart: the stop loop

All execution advances inside one loop. Each iteration is a complete,
self-contained cycle — there is no hidden state carried between stops
beyond the recorded history itself:

```
 program stops
   ├─ ABSORB    read whatever the program produced since the last stop
   │            (allocation events, output) — it is stopped, so the
   │            data is now complete and stable
   ├─ CLASSIFY  why did it stop? exit / crash signal / resource signal /
   │            harmless signal / ordinary user-code stop
   ├─ ACCOUNT   record the interval skipped since the previous stop as an
   │            explicit gap; at source transitions, capture a full
   │            checkpoint (stack, variables, memory, heap state)
   ├─ BOUND     check every budget: time, steps, instruction count,
   │            storage, output size — exhaustion ends collection with a
   │            typed reason, it never quietly lowers fidelity
   └─ ADVANCE   choose one command: instruction-step, source-step, step
                out of non-user frames, or continue
```

Two conventions make the resulting timeline trustworthy:

- **Observe before stepping.** At a stop, the recorded position is the
  instruction about to run; the outcome of stepping it (`completed`,
  `faulted`, `interrupted`, `unknown`) is written only when the next stop
  provides evidence for it.
- **Skipping is visible.** Code the engine does not step through (libraries,
  its own instrumentation) is crossed quickly, and every crossed interval
  is recorded as a gap with known endpoints. The timeline shows the hole
  rather than pretending the code was read.

## 4. The scribe: two artifacts, one accounting

Everything observed is written twice, incrementally:

- **The document** — the final, self-contained JSON result, streamed
  directly to its destination (never built as one giant string in memory).
- **The journal** — an append-only event stream written *as things happen*,
  so an interruption, deadline or crash still leaves an ordered, recoverable
  record of everything completed up to that point.

Budgets apply across both: when storage, instruction counts or time run
out, collection stops with an explicit, typed reason attached to the
record — a trace is always either complete or *marked* incomplete, never
silently diluted.

## 5. The trust model: three axes that must not be conflated

The output separates three questions that ordinary tools merge:

| Axis | Question | Example distinction |
|---|---|---|
| **Outcome** | what happened to the *program* | a debugger timeout is not proof the program loops forever |
| **Stage health** | did each *tool* run, and finish properly? | a skipped analysis leaves no fake timestamps |
| **Coverage** | what part of the run was actually *observed* | gaps are stated, not smoothed to zero |

Because the axes are separate, the consumer (the visual IDE) can decide for
itself what to trust and what to gray out. The engine's job is to keep them
from bleeding into each other.

## 6. What the engine deliberately does not do

- No GUI, no natural-language explanations, no AI-assisted fixing — it is a
  backend that returns data.
- No custom interpreter, no static prediction — real execution or nothing.
- No security sandbox — limits protect the engine and the machine from
  accidents; they are stated as engineering constraints, not guarantees
  against hostile code.
- No multi-run trickery — one program, one recorded execution.

(Per-feature limitations — threads, forked children, JIT code, and so on —
are listed explicitly in [`v2.md`](v2.md), not repeated here.)

## 7. Where each idea lives in the code

The layering is strict — entry and orchestration depend on services;
services depend on the data model; nothing skips downward twice:

| Idea | Module(s) |
|---|---|
| Clock, budgets, cancellation, process cleanup | `supervisor`, `resource_limits` |
| Compilation and diagnostics | `compiler` |
| Debugger protocol and session | `mi_parser`, `gdb_mi` |
| The stop loop and stage sequencing | `trace` |
| The scribe (records, budgets, journal) | `recorder`, `storage` |
| Heap telemetry and memory enrichment | `allocation_tracker`, `c_runtime/`, `memory` |
| Independent examiners | `valgrind`, `sanitizer` |
| Data contracts and verification | `models`, `processor` |
| Entry, configuration, error taxonomy | `main`, `cli`, `config`, `error` |

## 8. Reading order for a new contributor

1. This document — the system as ideas.
2. [`v2.md`](v2.md) — the recording contract in field-level detail.
3. `src/trace.rs` — watch the stop loop run; everything else serves it.
4. `src/recorder.rs` and `src/models.rs` — what an observation becomes.
5. The test suites — each one is an executable statement about the
   architecture's promises (deadlines, interruption, gaps, budgets).

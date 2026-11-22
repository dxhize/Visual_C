# 38 · Algorithms and invariants

> **In one sentence:** An algorithm is the rule the code follows. An invariant is a condition the code tries to keep true while following that rule.

## Key terms

| Term | Plain meaning |
|---|---|
| **Invariant** | A checkable rule, such as 'a later recorded stop must not be relabeled as an earlier one.' |
| **Bounded** | A request is limited by time, record count or bytes, and the limitation is reported rather than silently ignored. |

> **Example.** When a GDB step times out, the algorithm records that failure instead of turning the attempted instruction into a completed instruction. This preserves the invariant that attempts and completions are different.
>
> **What not to assume:** A proof about an algorithm under recorded inputs is not a promise that external tools never fail or that all processes were observed.

## The big picture

This page collects the mechanisms that are easy to misread from a single function name: supervised process control, MI dispatch, stop classification, bounded capture, DWARF reference chasing, source correlation, heap prefix reconstruction, fusion auditing and export preflight. Each section names the state that makes the algorithm correct and the evidence it refuses to invent.

## Supervised bounded subprocesses

`supervisor::output` starts an owned process group, drains stdout and stderr concurrently into separate bounded buffers, and waits for both the direct child and pipe EOF (`src/supervisor.rs:226-304`). It does not let a successful parent hide a descendant holding a pipe open. On cancellation or deadline it terminates the group, reaps what it can, and gives unfinished pipes a short bounded drain. `ProcessOutput.output_truncated` is a capture fact, not permission to claim complete diagnostics (`src/supervisor.rs:200-304`).

The invariant is **deadline plus ownership**, not a sandbox. A compiler or diagnostic stage can time out after producing useful bytes; those bytes remain partial evidence. A later stage must not infer success from the earlier stage's retained prefix.

## MI tokens and asynchronous stops

A command receives a monotonically increasing token. `GdbSession::execute` registers a pending waiter, writes a tokened MI command to stdin, and waits for the matching `^` result. The stdout reader task parses complete records and routes tokened results separately from asynchronous `*stopped` records (`src/gdb_mi.rs:532-671,2302-2484`). This separation is the central correctness rule: `-exec-step-instruction` returning `^done` means the command was accepted, not that the next stop was observed.

The reader preserves unknown classes and nested tuple/list structure through `mi_parser`; a malformed or partial line is an error/diagnostic, never a fabricated stopped frame (`src/mi_parser.rs:53-205,396-636`). Cancellation and EOF can leave a pending command unresolved; the caller must treat that as no sufficient completion evidence.

## Stop-loop classification and checkpoints

At each observed stop, `run_gdb_phase` first ingests available complete heap rows, then classifies the stop. A fatal signal, an exit, a normal source change and a dual-mode instruction attempt are different branches (`src/trace.rs:1371-1568,1720-1929`). The frame PC at a stop is the **next** address; an instruction event stores the pre-attempt PC plus the next observed address and stop reason. An absent after-stop is not proof that the attempted instruction completed.

Source checkpoints are retained when the implementation's source/origin and change rules select them. Repeated loop lines can remain distinct events. A checkpoint's `instruction_position` counts attempts before that snapshot; it is not an allocation timestamp or an exact instruction index into an unrecorded interval.

## Bounded stopped-state and memory capture

`gdb_mi::capture_state` builds a `CapturedState` from the selected frame, bounded frame-zero locals/arguments, optional caller frames, globals and memory requests (`src/gdb_mi.rs:1840-2227`). `recorder::capture_variable_memory` performs bounded inferior reads and retains raw bytes plus readability/truncation status (`src/recorder.rs:45-185`). A variable's static type from DWARF can supply a fallback extent only when the runtime extent is absent and the type is known; GDB remains the source of live values (`src/dwarf.rs:973-1040`).

The capture invariant is **preserve the observation, including its failure**. A missing read or an over-budget object remains visible as unavailable/partial; captured bytes do not prove initialization or an active union member. Pointer enrichment compares addresses with observed maps and heap extents, producing candidates, not ownership proofs (`src/memory.rs:141-205`).

## DWARF origin and specification resolution

`OriginResolver` follows `DW_AT_specification` or `DW_AT_abstract_origin`, filling only missing name/location/type attributes. A `UnitRef` stays in the current CU; an absolute `DebugInfoRef` is mapped through an index of absolute DIE offsets to its owning CU header (`src/dwarf.rs:213-423,535-575`). The visited set is `(CU header offset, unit DIE offset)`, with a four-hop limit and cycle detection.

The invariant is **bounded reference identity**. A missing target, unsupported form or cycle becomes an unresolved limitation; it does not fall back to a same-name declaration in another translation unit. CU-qualified keys preserve distinct declarations that happen to share a basename or symbol spelling.

## Clang spans and heap-call candidates

`span` computes source byte offsets and rejects zero/out-of-range or included-header locations; macro expansions retain a separate spelling location (`src/clang_ast.rs:397-488`). `correlate_heap_calls` indexes retained direct and macro allocator calls by canonical file and covered line, then accepts only a unique direct compatible call on a resolved return-address-minus-one line. Multiple calls or macro-only calls become explicit ambiguities (`src/clang_ast.rs:947-1054`).

The invariant is **site-only candidacy**. A resolved line chooses neither a particular dynamic call nor an allocation ID. An AST declaration/expression ID is static syntax identity, never a runtime activation identity.

## Heap verified prefixes and realloc state

`EventFileReader` accepts only complete newline-terminated rows; `parse_event_line` requires the exact 10-field or calloc 12-field shape (`src/allocation_tracker.rs:82-160,262-400`). `HeapTimeline::ingest` requires per-PID sequences to start at 1 and increase by one. A gap or malformed transport marks that PID/global history incomplete; later rows remain available but cannot retroactively certify missing transitions (`src/allocation_tracker.rs:521-756`).

Realloc is a branch table, not pointer arithmetic: failure preserves the old block; same-address success retires and recreates a linked lifetime; moved success links a successor; terminal zero-size free retires without a successor (`src/allocation_tracker.rs:606-699`). JSON IDs are widened to `(pid << 32) | local_id` only when multiple event PIDs were observed, with overflow reported rather than silently colliding (`src/allocation_tracker.rs:897-921`).

## Fusion and construction-time audit

`evidence::fuse` runs once after runtime and heap records exist. It indexes DWARF declarations/types, links AST fields and expressions only through unique CU/DIE-compatible matches, bridges runtime variables only through an already exact DWARF declaration ID, and delegates heap-call candidacy to the AST matcher (`src/evidence/mod.rs:33-91`). The final `audit` removes dangling references and verifies member name plus byte offset (`src/evidence/validation.rs:15-197`).

The invariant is **additive, qualified references**. Fusion does not rewrite observed values, addresses, activation IDs or allocation lifetimes, and it does not promote a source candidate to execution proof.

## Exact JSON preflight

`serialize_limited` writes through a byte-counting writer and rejects output above the configured cap before the destination is committed. For a file, `export_json` serializes to a sibling temporary file, flushes and renames it, then serializes to stdout (`src/json_export.rs:9-168`). The budget is per serialized JSON document, not a bound on the already assembled trace or on arbitrary serializer memory.

The invariant is **no partial final JSON from a known size violation**. A late stdout error after a successful rename is an operational failure with a committed file, not a rollback; a capped capture journal is a different, earlier boundary.

## Where to read next

Use the [source handbook](engine-handbook.html) for file-by-file ownership, the [function reference](engine-functions.html) for reviewed call relationships, and the [walkthroughs](engine-walkthroughs.html) for six subsystem-level sequences. The [limitations](limitations.html) page is part of every algorithm claim.

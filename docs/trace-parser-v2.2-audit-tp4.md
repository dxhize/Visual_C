# TP-4 — Trace Parser compatibility audit against Trace Engine V2.2

**Scope:** read-only assessment of TP-1/2/3, not TP-5 implementation. Engine V2.2 release decision: `docs/trace-engine-v2.2-release-closure.md` (A under its stated scope). This document assesses a *different* question: whether the current parser can safely interpret all of that engine's evidence. No engine production code was changed. Audit-only probe: `examples/trace_parser_tp4_audit.rs`.

## Architecture and contracts

| Layer | Current implementation | Boundary |
|---|---|---|
| TP-1 input | `trace_parser/{mod,json,journal,error}.rs`: bounded JSON file/reader/bytes loading, shared engine Serde `DebugSessionTrace`, schema-version gate; JSONL reader validates records and torn tail | Journals are **not** reconstructed into full traces. `from_trace` gates schema but does not deep-validate. JSON has its own input cap (default 512 MiB), distinct from the engine's journal/export caps. |
| Canonical ownership | `TraceStore` holds one immutable `Arc<DebugSessionTrace>`; `shared`/clones share it; `trace()` borrows it | Raw data is not rewritten into semantic conclusions. Parsing/index building allocate; views do not copy records. |
| TP-2 lookup | `indexes.rs` builds shared position-only indexes; `mod.rs` offers snapshot/stop/checkpoint, source↔instruction, heap-event/allocation and range queries | `event_id`, `instruction_id`, `checkpoint_id` are vector positions; GDB stop IDs are separately 1-based; heap allocation IDs are identities, **not** addresses or Valgrind IDs. Not an index of all nested V2.2 evidence. |
| TP-3 consumer access | `views.rs`, `evidence.rs`: borrowed raw and semantic-input views; trace-local evidence references; borrowed caller frames, AST declarations/expressions and checkpoint contexts | Evidence reference identity is local to a trace. AST source lookup is bounded linear, not a global source graph. No pointer/allocator/provenance inference. |
| Integrity | `validation.rs`: optional `validate_deep` invokes engine structural validation then checks event, checkpoint, heap, AST, caller and pointer references, coverage and diagnostics; severity is structural corruption / partial coverage / unavailable evidence / optional absence | **Not** invoked by normal load/query; a clean report is not a certificate that every V2.2 semantic invariant or every process was verified. |

## Field-level compatibility matrix

Legend: **R** = deserializes and retains (checked JSON-value round trip); **Q** = exposed by typed/raw access, possibly no dedicated index; **V** = some explicit deep validation; **G** = gap in V2.2 interpretation. Field paths below are serialized root paths unless prefixed by `snapshots[]`.

| V2.2 fields / evidence | TP-1 | TP-2/3 and deep validation | TP-4 assessment |
|---|---|---|---|
| `schema_version`, `execution`, `execution_status`, `session_meta`, `stage_results`, `internal_diagnostics` | R: version 2 accepted; absent version defaults to 2 | Q outcome/stages; diagnostics optional | Version alone does not attest a V2.2 feature set. Older `examples/v21_*.json` work; older `examples/v2_*.json` with removed limit fields do not (see below). |
| `instruction_events`, `instruction_count`, `disassembly_table`, `source_checkpoints`, `trace_gaps`, `snapshots[].observation.stop_id` | R | Q indexed positions, stop/checkpoint joins; V referential integrity and retention gaps | Do not conflate attempted instructions with retained events, or stop IDs with vector positions. |
| `snapshots[].variables`, `.globals`, `.caller_frames`, `.memory_evidence`, `type_ref`, `declaration_ref`, `activation_id`, extents, scope/read statuses | R | Q borrowed checkpoint and evidence refs; V caller/variable/AST links and some pointer extents | Available evidence is not necessarily complete; capped/missing references can be unavailable rather than corrupt. No semantic variable graph. |
| `type_registry`, `source_metadata` translation units/declarations/expressions, `heap_call_candidates`, `heap_call_ambiguities` | R | Q typed trace, borrowed source declaration/expression lookup; V registry/link checks | AST-off has no registry; same-line and macro calls are *candidates*, not proof of executed calls. No AST cross-run normalization. |
| `heap_events`, `heap_state`, `allocation_lifetimes`, realloc IDs, checkpoint interval, `snapshots[].variables[].target` | R | Q indexed allocation identity and events; V duplicate IDs, realloc missing references, intervals and some pointer provenance | Address-only/ambiguous/freed targets must not be promoted to definite IDs. **Verified gap:** an imported address-only target with matching `allocation_id` passes deep validation (below). |
| `heap_history.complete`, `sequence_gaps`, terminal transport/retention fields | R | Q via `trace().recording.heap_history`; V gaps/incomplete history | `complete` only covers verified *observed* event PIDs and followed inferior, not all forks. |
| `heap_history.process_scope`, `unobserved_process_coverage`, `followed_inferior_pid`, `observed_event_pids` | R | Q via typed trace; **no explicit TP-side scope/unknown validator** | **Verified gap:** invalid `unobserved_process_coverage="complete_all_processes"` in a silent-child trace passes deep validation. Never infer complete multiprocess coverage. |
| `valgrind_report.detailed_errors[].source_site_candidates`, `related_allocation_ids`; sanitizer/crash reports | R | Q typed reports; V report presence only (plus crash absence) | Source-site candidates are *not* definitive allocation links. No Valgrind same-execution join or verification of externally supplied definitive IDs. |
| `evidence_fusion`, `coverage`, `storage`, `resource_usage`, `execution_limits.max_json_export_bytes` | R | Q typed raw/coverage/storage/limits; V selected coverage/finalization | Limit bounds each exported UTF-8 document, **not** assembled trace RAM or parser load RAM. Limits/diagnostic annotations are not a parser-derived completeness proof. |

## Independent fixture observations and gaps

Generated new traces from `target/release/c-trace-engine`, GCC/GDB runtime, with `--no-pretty --sanitizer-trigger never`; inputs in `fixtures/c/`, output corpus `/tmp/cte-tp4/*.json`. Probe compares selected raw JSON fields to reserialized typed model, checks snapshot/event counts, runs `validate_deep`, and prints typed findings. Full probe output: `/tmp/cte-tp4-probe.log`; adversarial mutation output: `/tmp/cte-tp4-adversarial.log`. These files are local reproducibility artifacts, **not** checked-in golden PIDs. In particular fork observations came from separate live runs. Matrix round trips held for every tested present field.

| Corpus (fresh V2.2) | Observed behavior and validator classification |
|---|---|
| AST-on/off `phase8b_macro.c` | 7 snapshots / 22 instruction events / 4 heap events each; AST-on 19 expressions and one heap-call ambiguity; AST-off no source metadata. Neither reports structural corruption. Do not compare independent process IDs literally. |
| `heap_provenance.c`, `realloc_move.c` | 44/12 snapshots, 7/5 heap events; query and serialization preserve evidence; no structural findings. Reuse has one optional unrecorded heap-event address. Pointer targets retain `address_only` without definite identity. |
| `recursive.c` via `/tmp/cte-tp4/caller-config.json` | 26 snapshots, 69 events, 45 caller frames; no structural findings. Caller references are available via checkpoint context; no assumption that all frame data are always present. |
| `phase8r3_crash.c`, `hello.c --max-trace-bytes 2048` | Crash: 3 snapshots/5 events, partial heap-history finding; small-journal limit: 1 snapshot/0 events, `SCOPE_INCOMPLETE` and `HEAP_HISTORY_INCOMPLETE`; neither reported as corruption. A 2048-byte journal budget did **not** mean a 2048-byte JSON file (22,821 bytes). |
| `phase8dr_fork.c` observed/silent/incomplete | Observed child: 2 event PIDs, scoped complete, `unobserved_process_coverage=unknown`; silent: 1 event PID, scoped complete, still unknown; logged child missing ACK: 2 PIDs, `complete=false`, `HEAP_HISTORY_INCOMPLETE`. No structural findings. The snapshot scope remains the followed inferior, not a child census. |
| `phase8dr_address.c` | Valgrind `InvalidRead` detailed diagnostic preserved, with zero definitive links and zero automatic source-site candidates; independent mapping coincidence cannot prove allocation identity. TP-1/2/3 do not generate candidate matches. |
| `runtime_memory_deep.c` source mode, 1000 snapshots | 22,059,776-byte JSON, instruction capture disabled and step scope incomplete; parser flags partial coverage, not corruption. This is a load/performance sample, **not** a large instruction-index benchmark. |
| Legacy fixtures | All six `examples/v21_{normal,loop,heap,crash,dual,bounded_infinite}.json` deserialize as schema 2 (missing version defaults to 2). Four older `examples/v2_{success,null_pointer,dual,bounded_infinite}.json` fail with `MalformedJson`: e.g. removed `execution_limits.max_register_snapshot_bytes`. This is an existing pre-V2.1 fixture limitation, not a regression found in the fresh V2.2 traces; no claim of arbitrary historic V2 compatibility. |

**Adversarial failing-oracle demonstrations (no production repair):** clone a real `reuse.json` trace, set an `address_only` pointer's definite allocation ID to its address-match candidate, import with `TraceStore::from_trace`, validate: `structural=false`. Clone real silent-child trace and replace `unobserved_process_coverage` with `complete_all_processes`: also `structural=false`. The first would mislead a consumer trusting a clean report as provenance proof; the second would mislead a consumer interpreting coverage free text as certified multiprocess completeness. Neither is asserted as an engine-produced trace. `from_trace` deliberately only schema-gates; both omissions are TP-5 validation/consumer-contract work, **not** grounds to rewrite the engine's historical evidence.

## Performance baseline, not a TP-5 target

Three sequential release-profile runs of existing `examples/trace_parser_tp3_bench.rs` on fresh traces; median of run medians (timing per operation unless stated), local machine, `/tmp/cte-tp4-bench-{0,1,2}.log`. `tp2_index_build` is separately timed from JSON parse; per-query allocation counts in these runs were zero for listed serial TP-3 queries. RSS is process RSS after indexing, **not** an index-only or whole-trace RAM bound. Existing `phase8r3_validation_bench` ran 100 validations (single sample); those measurements exclude parse. No before/after TP-5 speedup is claimed.

| Sample | JSON bytes / snapshots / events | parse ms | TP-2 index µs | after-index RSS KiB | raw view ns | resolve evidence ns | traverse checkpoint+events ns | deep validate 100 |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| AST-on | 81,473 / 7 / 22 | 1.24 | 84.29 | 3,584 | 2.0 | 35.2 | 424.1 | 1.755 ms |
| heap reuse | 424,971 / 44 / 125 | 4.66 | 114.09 | 4,960 | 2.0 | 35.1 | 3,660.0 | 0.822 ms |
| source-large | 22,059,776 / 1,000 / 0 | 175.70 | 23.86 | 65,188 | 2.0 | 35.5 | 13,951.5 | 9.081 ms |

The 4-reader example includes thread/bench allocator overhead, so do not extrapolate its allocations to borrowed views. Source-large's small instruction index is expected with zero retained events; establish a *separate* large-instruction fixture before making scalability claims about instruction indexing.

## Upgrade boundaries and acceptance gates (not implemented)

- **TP-5 — V2.2 ingestion/index/validation hardening:** preserve schema v2/legacy V2.1 and bounded loading, add explicit process-scope and provenance-status semantic invariants; treat absent optional V2.2 fields as unknown, not false/complete. Add nested queries only where measured; surface candidates separately from identities. Adversarial mutations above must be rejected or explicitly marked unsafe while legitimate live and legacy fixtures remain queryable. Keep JSONL journal record-only unless reconstruction is separately designed and bounded.
- **TP-6 — semantic evidence layer:** build *trace-scoped*, uncertainty-preserving pointer/lifetime/source-call interpretations on TP-5, without inferring execution from AST candidates or converting addresses/Valgrind cross-run matches into IDs; consumers should be able to distinguish missing, partial, ambiguous, and definite evidence.
- **TP-7 — consumer-facing composition:** define stable query/serialization contracts for checkpoints, diagnostics, coverage, and evidence citations, with explicit process and time scope. Do not promise all-fork views from single-inferior snapshots.
- **TP-8 — performance and release verification:** bounded/multi-size benchmarks, allocation/RSS and repeated-reader tests, adversarial import/partial evidence regressions, full serial suite and compatibility checks. Improve memory only against measured workloads; the engine export cap remains unrelated to loaded-trace RAM.

**Risks:** stringly typed process annotations permit future invalid values; parser consumers may silently equate a successful deserialize or soundness report with semantic completeness; 22 MB JSON produced ~65 MiB process RSS; legacy pre-V2.1 fixtures already fail; no untrusted-code sandbox or full multiprocess census; no same-execution exact Valgrind join. Preserve the engine release closure's exclusions.

## Verification and verdict

After the audit-only example was added and formatted: `cargo check --all-targets` **pass**, `cargo fmt --all -- --check` **pass**, `cargo clippy --all-targets -- -D warnings` **pass**, `cargo test --no-fail-fast -- --test-threads=1` **pass**: **356 passed, 0 failed, 0 ignored, 19 targets** (including 1 doctest). Logs: `/tmp/cte-tp4-{check,fmt,clippy,tests}.log`. An audit-probe attempt against the older `v2_success.json` fails for the specified unknown legacy field; this is *not* a full-suite test failure. No TP-5 or engine implementation changes were made.

**Verdict: B — Conditional readiness to START TP-5, not to SHIP TP-5 semantics.** TP-1/2/3 can ingest and borrow the observed V2.2 traces, with good initial query performance, but completeness/provenance import validation is not yet sufficient for consumers to treat a clean report as a safety certificate. Address those two concrete gaps and re-run the acceptance gates before claiming parser-level V2.2 semantic compatibility.

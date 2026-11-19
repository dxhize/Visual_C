# Schema v2 and identity reference

## Level 1 · Intuition

A source line, stopped PC, snapshot position, instruction attempt, allocation lifetime and process address are different kinds of information. Mixing their integers creates convincing but false explanations. The model deliberately records explicit references, nullable fields, scopes and gaps so readers can say **what was observed and what remains unknown**.

{{diagram:records}}

## Level 2 · Top-level JSON

The Rust type is `src/models.rs::DebugSessionTrace`; `RecordingData` is `#[serde(flatten)]` into JSON. Typical top-level fields: `schema_version=2`, `compiled_successfully`, `compile_errors`, `snapshots`, `heap_events`, `heap_state`, `allocation_lifetimes`, `execution`, `execution_status`, `disassembly_table`, `instruction_events`, `source_checkpoints`, `trace_gaps`, `instruction_count`, `coverage`, `storage`, `heap_history`, `stage_results`, `resource_usage`, `execution_limits`, `tracing_configuration`, optional `type_registry`, `source_metadata`, `evidence_fusion`, `crash_report`, `valgrind_report`, and `sanitizer_report`. `Option`/null and missing optional/defaulted fields are not evidence of successful capture. `schema_version=2` covers supported V2.1/V2.2; the four older pre-V2.1 example documents with removed limit keys are not supported on import. Deprecated per-instruction register observations are parse-only: new output omits them.

### Identity domains

| Domain | Where recorded | What it is **not** |
|---|---|---|
| Snapshot vector index / `step_index` | `snapshots[i]` | GDB stop counter or instruction attempt |
| GDB `stop_id` | Snapshot observation, event before/after, journal wrapper | Snapshot index; not necessarily consecutive |
| `instruction_position`, `instruction_count` | Stop/gap/trace | Retained event position; journal-only can retain zero in JSON |
| `event_id` / event vector position | `instruction_events[j]` | `instruction_id` (disassembly table) or unique source line |
| `checkpoint_id` | InstructionEvent | Exact execution at that stop; only most recent checkpoint tag |
| `source_checkpoints[n]` | RecordingData | Duplicated snapshot; it stores snapshot index |
| `allocation_id`, `old_allocation_id` | Heap events, state, lifetime | Process address, event index or pointer provenance; `0` untracked |
| `sequence`, `pid` | Heap runtime | Stop/event positions; sequences start per PID |
| `activation_id`, `lexical_scope_id` | Frame/variable | Source variable name or address |
| `address`, `points_to`, `next_address` | Memory/event/variable | Identity; numeric addresses can recur |

### Heap records

`HeapEvent` stores `event_type=allocated|reallocated|freed|allocation_failed`, per-PID `sequence`, `pid`, `allocation_id`, optional `old_allocation_id`, address, size, caller and detail. `AllocationLifetime` stores allocation ID, PID, allocation sequence and optional free sequence. `HeapBlockInfo` stores final/historical state and optionally `realloc_from_id`; each snapshot can store alive `heap_blocks` as observed at a stop. These are complementary, **not one-to-one byte replicas**. Successful realloc requires [relationship verification](parser-validation.html#realloc-link-verification). `HeapHistoryCoverage.complete` is scoped to verified observed event PIDs plus followed inferior, with unknown descendants.

## Level 3 · Accurate field-level examples

The following is a **projection of fields from a captured `phase8b_defined.c` run**, not a complete importable trace. OS-specific PID/address values differ between runs. Its first two events describe an in-place realloc that starts a new lifetime ID; the third frees the successor:

```json
{"sequence":1,"allocation_id":1,"old_allocation_id":null,"event_type":"allocated","address":"0x705010","size_bytes":12}
{"sequence":2,"allocation_id":2,"old_allocation_id":1,"event_type":"reallocated","address":"0x705010","size_bytes":16}
{"sequence":3,"allocation_id":2,"old_allocation_id":null,"event_type":"freed","address":"0x705010","size_bytes":16}
```

`raw_bytes_hex="0a000000"` at an audited stop is a byte capture, not proof the C initializer ran. A separately recorded `PointerTarget` can report `address_match_allocation_id=1` and `allocation_id=null`, because **address containment is not origin**. To load a full trace, run the [CLI walkthrough](usage.html#level-2-run-a-recording); do not paste these projections into `TraceStore::open`.

> NOTE: JSON export's `max_json_export_bytes` bounds the serialized document; neither that nor the parser's input cap bounds already assembled engine trace RAM. [Resource policy](engine-limits.html#level-2-budgets-across-stages) lists the separate limits.

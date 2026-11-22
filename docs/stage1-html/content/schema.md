# 64 · Schema v2 and identity reference

> **In one sentence:** The schema lists the JSON fields that save a recording. The most important reading habit is to ask what each number identifies.

## Key terms

| Term | Plain meaning |
|---|---|
| **Schema version** | The format version. The supported V2.1 and V2.2 traces here use version 2; not every older example file can be imported. |
| **Nullable** | A field can say 'no value recorded.' That is not the same as 'the value was zero' or 'nothing happened.' |

> **Example.** An event can have `allocation_id=2`, while its `address` is `0x705010`. A later block can also live at `0x705010` with a different ID. Keep the ID and address separate.
>
> **What not to assume:** Short JSON projections show selected fields for study, not full documents to paste into the parser.

## Level 1 · Intuition

A source line, stopped PC, snapshot position, instruction attempt, allocation lifetime and process address are different kinds of information. Mixing their integers creates convincing but false explanations. The model deliberately records explicit references, nullable fields, scopes and gaps so readers can say **what was observed and what remains unknown**.

{{diagram:records}}

## Level 2 · Top-level JSON

The complete JSON trace is represented in Rust by `DebugSessionTrace` in `src/models.rs`. Its `RecordingData` fields are **flattened**: instead of appearing under a `recording` object, items such as `snapshots` appear at the top level. Think of the fields in groups. `compiled_successfully` and `compile_errors` describe the build; `snapshots`, `instruction_events`, `source_checkpoints`, `trace_gaps` and `coverage` describe the debugger recording; `heap_events`, `heap_state`, `allocation_lifetimes` and `heap_history` describe observed allocator activity. `stage_results`, `execution_status`, `resource_usage`, `execution_limits` and `storage` describe outcomes and limits. `type_registry`, `source_metadata`, `evidence_fusion` and crash or memory-error reports may be optional. **Null or missing optional data does not mean the check succeeded.** `schema_version=2` works for supported V2.1/V2.2 files, not every older schema-2 example: four pre-V2.1 files contain a removed limit key and do not import. Older register fields may be parsed for compatibility but are no longer exported for each instruction.

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

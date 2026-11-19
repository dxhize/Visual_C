# DebugSessionTrace · the recorded data model

## Level 1 · Intuition

A trace is not a video of a program. It is a set of timestamped/stopped observations, bounded attempts and gaps, heap telemetry, optional static metadata and diagnostics. Links between them use **different identity domains**. Preserving absent evidence is just as important as preserving a successful read.

{{diagram:records}}

## Level 2 · Record relationships

`DebugSessionTrace` flattens `RecordingData` into the top-level JSON object (`#[serde(flatten)]`), while keeping `snapshots`, `heap_events`, `heap_state`, stage results, compilation diagnostics, optional reports and static metadata alongside it. `recording.instruction_events[]` in Rust appears as top-level `instruction_events` in JSON. `source_checkpoints[]` stores **indices into snapshots** rather than cloned snapshots. `disassembly_table` contains unique address/text table entries; instruction events refer by `instruction_id`, while repeated dynamic visits retain separate event positions. `trace_gaps` record unknown intervals at attempted positions, not fake events. `allocation_lifetimes`, final `heap_state` and chronological `heap_events` are distinct views of observed heap activity. `coverage` (user-code instruction scope), `heap_history` (verified observed-PID events), `storage` and `stage_results` are independent dimensions.

### Events, gaps and checkpoints

- `snapshots[i].step_index=i`: captured checkpoint position. `snapshots[i].observation.stop_id`: processed debugger stop counter; may have holes. `instruction_position`: number of attempts preceding it.
- `instruction_events[j].event_id`: ordered attempted instruction ID; optional `checkpoint_id` references a **recent snapshot**, not an exact temporal join. `stop_id_before/after` refer to debugger observations.
- `trace_gaps[k].instruction_position` is a boundary where instructions were not captured; its unknown instruction count is **never guessed**.

### Variables, bytes and heap

`VariableInfo.read_status`, `bytes_status` and `memory_evidence` describe distinct observations. Pointer `address` is storage and `points_to` is the pointee. `HeapEvent.sequence` is **per-PID**, not event-vector position. `HeapBlockInfo.observed_at_stop` timestamps heap byte reads; a freed final block may retain historical bytes. `AllocationLifetime` has an allocation sequence and optional free sequence; a successful realloc can link old and new lifetime IDs. The schema does not create pointer origin from address equality.

## Level 3 · Actual schema excerpt and paths

```rust
// Reduced field excerpt from src/models.rs; other fields omitted.
pub struct DebugSessionTrace {
    pub schema_version: u32,
    pub snapshots: Vec<ExecutionSnapshot>,
    pub heap_events: Vec<HeapEvent>,
    pub heap_state: Vec<HeapBlockInfo>,
    pub execution_status: ExecutionStatus,
    pub stage_results: StageResults,
    // #[serde(flatten)] pub recording: RecordingData,
}
```

**Illustrative reduced excerpt**, not a copy/paste Rust definition or complete JSON: `DebugSessionTrace` also contains typed `execution_limits`, `execution`, compilation results, optional metadata and reports, usage and notes. For the actual declaration, see `src/models.rs::DebugSessionTrace` and [schema reference](schema.html).

```json
{"schema_version":2,"execution_status":"step_limit_reached","instruction_count":203,"coverage":{"scope":"user_code","complete_for_scope":false,"truncation_reasons":["max_steps"]}}
```

This is an **illustrative partial JSON projection**: no unspecified keys are implied, and a real trace's truncation-reason list may include additional causes. The importer requires the full model, not this excerpt. `schema_version` is 2 for supported V2.1/V2.2 documents; a missing marker defaults to 2, but unknown old `execution_limits` fields can still make pre-V2.1 files fail to deserialize. See [TP-1 compatibility](parser-ingestion.html#level-2-architecture-and-compatibility).

> NOTE: The wire model records uncertainty as `Option`/nullable values and explicit status fields. Never rewrite a missing `stop_id_after`, unloaded AST registry or unreadable bytes to a successful observation. Find identity semantics in the [reference dictionary](schema.html#identity-domains).

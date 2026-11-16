# V2.1 Trace Schema Update

`schema_version` remains **2**. V2.1 is a backward-compatible refinement of
the V2 schema: nothing a V2 consumer read is renamed or reinterpreted, and
register payloads became optional rather than invalid. Old V2 traces still
parse with the V2.1 engine (`#[serde(default)]` + optional register slots).

## 1. Changed fields

### `instruction_events[]`

| Field | V2 | V2.1 |
|---|---|---|
| `registers` | `RegisterObservation` (POST-attempt full snapshot or delta) | **deprecated**, always absent from output (`#[serde(default, skip_serializing_if = "Option::is_none")]`); retained for deserializing V2 traces |
| `next_address` | observed from the next stop's frame | unchanged (still observed; never inferred) |
| `address`, `event_id`, `instruction_id`, `outcome`, `signal`, `stopped_reason`, `memory`, `source`, `checkpoint_id`, `thread_id`, `pid`, `stop_id_before/after` | unchanged | unchanged |

### Top level (recording)

| Field | V2 | V2.1 |
|---|---|---|
| `initial_registers` | startup register checkpoint | **deprecated**, always absent from output; retained for deserializing V2 traces |
| `profiling.enabled` | V2 bug: `false` even when metrics were populated (audit F-06) | always `true` — baseline metrics are always collected |
| `profiling.register_time_us` | time in `-data-list-register-values` polling | documented as always `0` since V2.1; kept so historical metrics stay comparable |
| `execution_status` | `step_limit_reached` was reported for output-limit termination (F-04) | new value **`output_limit_reached`** used for `max_output_bytes` termination |
| `resource_usage.limits_triggered` | could contain duplicates (F-03) | deduplicated; each limit appears at most once |
| `stage_results.allocation_tracker` | interrupted runs could claim `completed`/`output_complete:false`/`partial_results_available:false` (F-01) | interrupted runs report `timed_out`/`failed`/`cancelled` + reason + correct `partial_results_available` |
| `execution_limits` | contained `max_register_snapshot_bytes` | field removed (old traces keep their embedded copy; consumers ignore unknown keys) |

### Configuration (`TracingConfig`, `ExecutionLimits`, CLI)

| Key | V2 | V2.1 |
|---|---|---|
| `tracing.registers` | enabled register capture | **deprecated no-op** (kept so old config files parse; `deny_unknown_fields` preserved) |
| `tracing.register_checkpoint_interval` | checkpoint cadence | **deprecated no-op** |
| `tracing.profiling` | toggled `profiling.enabled` | **deprecated no-op** (baseline metrics always on) |
| `limits.max_register_snapshot_bytes` | register snapshot budget | **removed** |
| CLI `--no-registers`, `--profile`, `--max-register-snapshot-bytes` | existed | **removed** |
| `sanitizer_trigger` | — | new: `never \| on_crash \| on_crash_or_valgrind_error \| always` (default `on_crash_or_valgrind_error`) |
| CLI/config `--sanitizer-trigger` | — | new |
| `valgrind_path`, `gdb_path` (+ CLI `--valgrind-path`, `--gdb-path`) | — | new: explicit tool paths overriding PATH resolution (F-05) |

## 2. Example V2.1 dual-level trace

Valid, generated traces live in `examples/` with the outcome manifest
[`examples/v21_manifest.json`](../../examples/v21_manifest.json):

- [`v21_normal.json`](../../examples/v21_normal.json) — source-only, completed.
- [`v21_loop.json`](../../examples/v21_loop.json) — finite loop, dual.
- [`examples/v21_heap.json`](../../examples/v21_heap.json) — heap allocation/free/realloc, dual.
- [`examples/v21_crash.json`](../../examples/v21_crash.json) — null-pointer write crash, dual.
- [`examples/v21_dual.json`](../../examples/v21_dual.json) — synchronized C/assembly + heap, dual.
- [`examples/v21_bounded_infinite.json`](../../examples/v21_bounded_infinite.json) — deadline partial trace.

An abridged valid V2.1 dual-level event with every changed field shown:

```json
{
  "schema_version": 2,
  "execution_status": "completed",
  "execution": { "outcome": "completed" },
  "profiling": {
    "enabled": true,
    "step_time_us": 1184203,
    "disassembly_time_us": 91240,
    "register_time_us": 0,
    "disassembly_queries": 41,
    "disassembly_cache_hits": 97
  },
  "disassembly_table": [
    {
      "instruction_id": 0,
      "module_identity": "a.out",
      "address": "0x0000000000401106",
      "bytes": "8b 45 fc",
      "text": "mov    eax,DWORD PTR [rbp-0x4]",
      "function": "sum_of_three"
    }
  ],
  "instruction_events": [
    {
      "stop_id_before": 12,
      "stop_id_after": 13,
      "event_id": 4,
      "instruction_id": 0,
      "address": "0x0000000000401106",
      "next_address": "0x0000000000401109",
      "thread_id": "1",
      "pid": 12345,
      "source": { "file": "sample.c", "line": 22, "func": "sum_of_three" },
      "checkpoint_id": null,
      "outcome": "completed",
      "stopped_reason": "end-stepping-range",
      "signal": null,
      "memory": [
        {
          "expression": "&x",
          "address": "0x00007fffffffd8f8",
          "requested_bytes": 4,
          "raw_bytes_hex": "0a000000",
          "status": "valid",
          "error": null
        }
      ]
    }
  ]
}
```

Note the event has **no `registers` key at all** — it is not `null`, it is
omitted, and the timing metric proves the query never ran.

## 3. Compatibility statement

- V2 trace → V2.1 engine: parses (register slots are optional with
  `#[serde(default)]`).
- V2.1 trace → V2 engine: not guaranteed (new `ExecutionStatus` value and
  removed `execution_limits` key) — acceptable, since V2 is superseded.
- External consumers (Python validators, future GUI) ignore unknown keys and
  treat absent register payloads as "not captured", matching the V2.1
  semantics in section 6 of the brief.

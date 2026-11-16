# BuildConfig reference

`BuildConfig` (in `src/config.rs`) is the single description of *what to
build and how*. It is populated from three sources, in increasing
precedence:

1. **Engine defaults**
2. **JSON config file** (`--config build.json`) — the same schema the future
   GUI panel will edit and persist
3. **CLI flags** — scalars override the file; list flags **append**

## Field → CLI flag → GUI panel mapping

| Field (JSON key) | CLI flag | GUI panel field | Notes |
|---|---|---|---|
| `source_file` (`source`) | positional arg or `--source` | "Primary source" file picker | required; must exist |
| `additional_sources[]` (`additional_sources`) | `--add-source` (repeatable) | "Sources" list with add/remove | each is a separate translation unit |
| `object_files[]` (`object_files`) | `--object-file` (repeatable) | "Object files" list | linked without recompilation |
| `include_dirs[]` (`include_dirs`) | `--include-dir` (repeatable) | "Include paths" list | becomes `-I<dir>` |
| `defines[]` (`defines`) | `--define` (repeatable) | "Preprocessor defines" key/value rows | `NAME` or `NAME=value` → `-D...` |
| `compiler_flags[]` (`compiler_flags`) | `--compiler-flag` (repeatable) | "Compiler flags" free-text list | passed to gcc verbatim |
| `linker_flags[]` (`linker_flags`) | `--linker-flag` (repeatable) | "Linker flags" free-text list | e.g. `-pthread` |
| `library_dirs[]` (`library_dirs`) | `--library-dir` (repeatable) | "Library paths" list | becomes `-L<dir>` |
| `libraries[]` (`libraries`) | `--library` (repeatable) | "Libraries" list | names without `l` → `-l<name>` |
| `output_name` | `--output-name` | "Executable name" text field | defaults to a session-unique name |
| `working_directory` | `--working-directory` | "Build directory" picker | build cwd, default for the run cwd |

### Trace-level fields (outside `BuildConfig`)

| Field (JSON key) | CLI flag | GUI |
|---|---|---|
| `program_args[]` (`program_args`) | args after `--`, or `--arg` (repeatable) | "Program arguments" list |
| `inferior_working_directory` | `--inferior-working-directory` | "Run directory" picker (defaults to build dir) |
| `output_file` | `--output` | "Save trace to…" |
| `pretty` | `--no-pretty` (inverse) | "Pretty-print JSON" checkbox |
| `keep_temp` | `--keep-temp` | "Keep session workspace" (debugging aid) |

### Limits (nested `limits` object)

| JSON key | CLI flag | GUI |
|---|---|---|
| `max_steps` | `--max-steps` | "Max steps" number |
| `timeout_seconds` | `--timeout` | "Timeout (s)" number |
| `max_memory_mb` | `--max-memory-mb` | "Memory limit (MiB)" number |
| `max_output_bytes` | `--max-output-bytes` | "Output capture (bytes)" |
| `max_heap_allocations` | `--max-heap-allocations` | "Tracked allocations" |
| `max_snapshot_count` | `--max-snapshots` | "Max snapshots" |
| `max_snapshot_memory_bytes` | `--max-snapshot-memory-bytes` | "Memory per snapshot (bytes)" |
| `max_memory_read_bytes` | `--max-memory-read-bytes` | "Max single read (bytes)" |
| `max_process_count` | `--max-processes` | "Child processes" |
| `max_variables_per_snapshot` | `--max-variables` | "Variables per snapshot" |

## Example config file

```json
{
  "source": "sample.c",
  "additional_sources": ["helper.c"],
  "include_dirs": ["./include"],
  "defines": ["DEBUG=1", "HAVE_MMAP"],
  "compiler_flags": ["-std=c17", "-Wall"],
  "linker_flags": ["-pthread"],
  "libraries": ["m"],
  "output_name": "mysim",
  "working_directory": ".",
  "program_args": ["--iterations", "5"],
  "inferior_working_directory": "data",
  "limits": { "timeout_seconds": 5, "max_steps": 500 },
  "output_file": "trace.json",
  "pretty": true,
  "keep_temp": false
}
```

Notes:

* Source/include/object paths follow GCC's build working directory. Run/output
  paths are relative to the engine cwd unless absolute. The GUI should store
  absolute paths or consistently select its project root as the build cwd.
* Unknown configuration keys are rejected to catch typos. Store GUI editor
  state separately.
* A compile failure never aborts the run: the trace still carries
  `compile_errors` with file/line/column/kind/message for the editor to
  surface inline (GCC 16 SARIF is parsed; older `-fdiagnostics-format=json`
  and plain-text diagnostics are fallbacks).

V2 adds the `tracing` object and additional `limits` fields. See
[V2 configuration and defaults](v2.md#configuration-example) and `--help`.
Notable flags: `--trace-mode source|dual`, `--no-disassembly`,
`--journal`, `--journal-only`, `--global`, `--no-global-discovery`, `--memory-region`,
`--sanitizer-trigger never|on_crash|on_crash_or_valgrind_error|always`,
`--gdb-path`, `--valgrind-path`,
`--max-instruction-events`, `--max-gdb-commands`, `--max-source-checkpoints`,
`--max-trace-bytes`, `--max-trace-memory-bytes`,
`--max-globals`, `--compiler-timeout`, `--debugger-timeout`, `--analysis-timeout`.
(V2's `--no-registers`, `--profile` and `--max-register-snapshot-bytes` were
removed with per-instruction register capture in V2.1.) All stage deadlines
are clamped to `--timeout`.

# Visual C · Stage 1 Documentation — Part 5

**Covers chapters 22–25:** Trace Engine Internals (entry/configuration/errors, compiler and diagnostics, GDB controller and stop loop, MI2 parser).

This is where the documentation goes from *ideas* to *code*. Each chapter walks through one source file at a time. Every entry uses the same pattern so you can scan quickly:

> **Per file:** what it's for → principal types/functions with `file:line` references → how it behaves → what it does **not** prove.

All `file:line` references, signatures and code excerpts are kept exactly as in the original, so you can still jump to the source. Line numbers refer to the inspected working tree and may shift.

---

<!-- BEGIN CHAPTER: content/engine-entry-deep.md -->
# 22 · Engine entry, configuration, and errors

> **In one sentence:** The front door. Command-line arguments become a tracing request (`TraceConfig`), and errors become meaningful exit results.

## Key terms

| Term | Meaning |
|---|---|
| **CLI** | Command-line interface: the flags and file names you pass when starting `c-trace-engine`. |
| **Configuration precedence** | The rule that decides who wins when a CLI flag and a config-file value set the same thing. |

> **Example.** You pass a C file and `--max-steps 50`. `main` converts them into a `TraceConfig`, and the tracing code receives those *values* instead of re-reading command-line text.
>
> **A bad flag** is an input error. **A compiler diagnostic** is a recorded C build result. They are not the same failure.

## The big picture

The executable is a **thin adapter** over the library. It:

1. parses flags,
2. builds a `TraceConfig`,
3. awaits `trace::trace_program`, then
4. exports the returned `DebugSessionTrace`.

| Outcome | What you get |
|---|---|
| A compilation **rejection** | A *trace result* (with diagnostics) |
| A configuration, tool or export **failure** | An error, possibly with **no** usable trace |

Next boundaries: [compilation](engine-compiler-deep.html), [process ownership](engine-supervision-deep.html), [export](engine-export-deep.html).

## File 1 · `src/main.rs`: from CLI to exit status

**Principal function:** `async fn main() -> ExitCode` (`src/main.rs:22-80`)

- `Cli::parse()` belongs to Clap (the argument-parsing library). `to_config()` is the first engine-specific step that can fail.
- The CLI owns the effective config, clones it into the tracing call, then *borrows* the resulting trace for a synchronous JSON export.
- After tracing, it installs an independent **SIGINT watcher** and passes its `AtomicBool` to `export_json_cancellable`. It aborts that watcher once export returns (`src/main.rs:34-59`).
- This export watcher is **distinct** from the supervised collection-time SIGINT/SIGTERM token.

```rust
let config = cli.to_config()?; // schematic: main prints the error and returns exit 1
let trace = trace::trace_program(config.clone()).await?;
let output = json_export::export_json_cancellable(
    &trace, config.pretty, config.limits.max_json_export_bytes,
    config.output_file.as_deref(), &mut std::io::stdout().lock(), Some(&cancelled),
);
```

This shortens the actual `match` arms at `src/main.rs:26-79`. `main` does not itself return `Result`.

### Exit codes

| Situation | Exit code |
|---|:---:|
| Bad config, operational tracing error, failed export, or inability to install the export signal watcher (message on stderr) | **1** |
| Export succeeded **and** `execution_status == CompileFailed` (a diagnostic JSON trace is still emitted) | **2** |
| Any other successfully exported trace, **including crashes and timeouts** | **0** |

(`src/main.rs:59-78`.) So a shell exit of 0 does **not** certify complete execution.

There's no `main.rs` unit test. The [final audit](../stage1-final-audit.md) independently exercised compile-failure exit 2 and export-cap exit 1.

## File 2 · `src/lib.rs`: the public crate boundary

| Item | Detail |
|---|---|
| **Principal exports** | `pub use config::{BuildConfig, TraceConfig};` `pub use error::{EngineError, Result};` `pub use models::DebugSessionTrace;` `pub use resource_limits::ExecutionLimits;` (`src/lib.rs:49-52`) |
| **Public modules** | `cli`, `compiler`, `supervisor`, `processor`, `storage`, `json_export`, `trace` (`src/lib.rs:25-47`) |
| **Ownership** | The library owns **no global trace instance**: callers supply a `TraceConfig` and own the returned trace |
| **Embedding** | You can call `trace_program` without Clap or `main`'s stdout/exit-code policy |
| **Tests** | None in `lib.rs`; behavior is tested in the modules and integration suite |

The module map at `src/lib.rs:8-23` is orientation, not an execution guarantee.

## File 3 · `src/cli.rs`: flag precedence and source validation

**Principal items:**

- `#[derive(Parser, Debug)] pub struct Cli` (`src/cli.rs:21-192`)
- `pub fn to_config(&self) -> Result<TraceConfig>` (`src/cli.rs:194-383`)
- `fn file_build(file: &ConfigFile, source: &Path) -> BuildConfig` (`src/cli.rs:385-399`)

### How `to_config` decides things

| Step | Rule |
|---|---|
| 1. Config file | Reads and deserializes the optional `--config` first. Unreadable or invalid JSON is `EngineError::Config`. |
| 2. Primary source | Chosen in this order: `--source`, then the positional source, then the JSON file's `source`. (Clap forbids giving both CLI forms together.) |
| 3. Lists | Copies the file's build lists, then **appends** repeated CLI `--add-source`, `--define`, `--library`, etc. |
| 4. Scalar settings | Replaced **only when the matching CLI option is present** (`src/cli.rs:199-240`). |
| 5. Source check | The primary source is checked to be a file, relative to `build.working_directory` when set. This does **not** validate every auxiliary input or canonicalize the stored path (`src/cli.rs:242-256`). |

```rust
let source = self.source.clone()
    .or_else(|| self.source_positional.clone())
    .or_else(|| file.source.clone())
    .ok_or_else(|| EngineError::Config("no source file given ...".into()))?;
let mut build = file_build(&file, &source);
build.defines.extend(self.define.iter().cloned());
let mut program_args = file.program_args.clone();
program_args.extend(self.arg.iter().cloned());
program_args.extend(self.program_args.iter().cloned());
```

This abbreviated walkthrough follows `src/cli.rs:211-261`. The real missing-source message also names the supported CLI forms.

### Details worth knowing

- Arguments after `--` and repeated `--arg` go to the **inferior** (your C program), *not* to GCC.
- Limits start from `file.limits.unwrap_or_default()`, and each CLI scalar that is present overrides one field (`src/cli.rs:263-293,330-359`).
- `--no-pretty`, `--keep-temp` and `--clang-ast` are **one-way switches**. When unset, the file or default value stays in force (`src/cli.rs:295-301,378-380`).
- `--journal-only` turns off retained instruction events, and Clap requires `--journal` with it. `trace_program_inner` also refuses journal-only configuration without a path for non-CLI callers (`src/cli.rs:154-157,306-312`; `src/trace.rs:172-176`).
- An invalid `--memory-region` syntax or byte count gives a `Config` error **before tracing** (`src/cli.rs:319-329`).
- **Tests:** `positional_source`, `missing_source_is_config_error`, `flag_accumulation_and_program_args`, `limit_flags_override_defaults`, `config_file_merge_precedence` (`src/cli.rs:415-550`).

## File 4 · `src/config.rs`: request types and defaults

| Type / function | Location | What it is |
|---|---|---|
| `BuildConfig` | `src/config.rs:20-45` | Project inputs and linking options |
| `user_sources(&self) -> Vec<PathBuf>` | `src/config.rs:47-56` | Collects primary plus additional sources, dropping empty paths |
| `TraceConfig` + `impl Default` | `src/config.rs:58-112` | Adds inferior args/cwd, limits, output destination, tool overrides, optional AST, tracing policy |
| `ConfigFile` | `src/config.rs:114-144` | A **different**, flat JSON-file schema with optional scalars and `deny_unknown_fields` |
| `SanitizerTrigger` | `src/config.rs:154-174` | Defaults to `OnCrashOrValgrindError` |
| `TracingConfig` + `Default` | `src/config.rs:176-221` | Defaults: `Dual` mode, static disassembly on, global discovery on, resident instruction events retained |

Things to watch for:

> **Don't paste a serialized `TraceConfig` into `--config`.** `ConfigFile` has different keys.

- `BuildConfig` deliberately places **user compiler flags after the engine defaults**, and that ordering can change observation fidelity.
- These are owned, cloned configuration values, not global mutable knobs.
- There are no tests in `config.rs`; CLI merge tests and `ExecutionLimits` validation cover adjacent behavior.
- A comment on `TraceConfig::output_file` mentions `stdout_json`, but **no such field exists**. The current CLI writes stdout on a successful export (`src/config.rs:70-75`; `src/main.rs:50-57`).

## File 5 · `src/error.rs`: the failure vocabulary

**Principal type and alias:** `pub enum EngineError` and `pub type Result<T> = std::result::Result<T, EngineError>` (`src/error.rs:10-59`).

| Variant | Meaning |
|---|---|
| `Config` | Bad configuration |
| `MissingTool` | A required tool isn't available |
| `Deadline { operation }` | A deadline expired |
| `Cancelled` | The session was cancelled |
| `LimitReached` | A configured limit was hit |
| `Io`, `Json` | I/O and JSON failures (converted via `#[from]`) |
| *others* | GDB/MI, allocation-event, Valgrind and resource-limit stage errors |

`#[from]` converts I/O, JSON and MI-parse errors rather than flattening everything into strings. The **caller** decides whether an error is returned directly or recorded in a partial trace:

- A GCC source error produces `CompilationOutput { success: false, diagnostics }`, then `CompileFailed`.
- `probe_tool` can return `MissingTool`, and the CLI exits 1 (`src/compiler.rs:99-128,518-527`; `src/trace.rs:336-350`).

`error.rs` has no local tests; CLI, compiler and supervisor tests exercise the error branches. A bare `EngineError` never proves that a serialized trace was produced.

For the wire vocabulary, continue with [model and exporter internals](engine-export-deep.html). For limits governing the request, see [supervision](engine-supervision-deep.html).

## Diagram notes

| Diagram | What it shows |
|---|---|
| **The CLI boundary**<br>{{diagram:d2-cli}} | `main` owns process exit policy; `trace_program` owns the bounded session; export is a separate fallible operation. |
| **Configuration precedence**<br>{{diagram:d2-config}} | Defaults, JSON-file settings and explicit CLI values meet in `Cli::to_config`; later stages consume the owned effective configuration. |
| **Session control flow**<br>{{diagram:d2-session}} | Compilation, static metadata, the GDB stop loop, optional diagnostics and trace assembly are distinct phases under one deadline. |
| **Failure propagation**<br>{{diagram:d2-error}} | A compile failure can produce a trace; an operational error or export-cap failure need not produce a usable JSON document. |
<!-- END CHAPTER: content/engine-entry-deep.md -->

<!-- BEGIN CHAPTER: content/engine-compiler-deep.md -->
# 23 · Compiler invocation and diagnostic evidence

> **In one sentence:** The code-level story of how GCC commands are assembled, started and interpreted.

## Key terms

| Term | Meaning |
|---|---|
| **Argv** | The exact list of arguments given to an OS process. Reading it is safer than imagining what shell command ran. |
| **Diagnostic fallback** | If GCC can't produce one structured warning format, the engine tries another supported one instead of silently losing errors. |

> **Example.** The debug GCC command includes the wrapper source and the `--wrap` linker switches. The separate sanitizer command uses different flags. A warning from either build has a file, line and message when GCC supplies them.
>
> A compiler subprocess **timing out** is not the same event as GCC **finishing and rejecting** invalid C.

## The big picture

The engine builds an **instrumented debug executable** before any debugger run, then *attempts* a separate sanitized executable.

- Compilation **success** is a prerequisite for runtime evidence.
- Sanitizer **build** success is **not** proof that a sanitizer later found or reproduced a defect.

The request is assembled in [entry/configuration](engine-entry-deep.html), and subprocesses are owned by [supervision](engine-supervision-deep.html).

## Step 1 · Options and tool probes (`src/compiler.rs`)

**`CompileOptions`** holds `{ sanitized, sanitizer_variant, include_alloc_tracker, tracker_source, output_path }`, with three constructors (`src/compiler.rs:28-73`):

| Constructor | Use |
|---|---|
| `debug(output_path, tracker_source)` | The debug build |
| `sanitized(output_path)` | A sanitizer build |
| `sanitized_with_variant(output_path, variant)` | A specific sanitizer variant |

**`CompilationOutput`** retains: success, native exit code, binary path, parsed diagnostics, raw stderr, the selected format, and an optional sanitizer variant (`src/compiler.rs:75-97`).

The orchestrator creates the tracker source and passes it to `compile_project`. The debug build wraps `malloc`/`calloc`/`realloc`/`free`; sanitized builds **omit** the tracker (`src/trace.rs:226-237`; `src/compiler.rs:543-566`).

### Probing tools

| Function | Location | Behavior |
|---|---|---|
| `probe_tool(tool, version_arg) -> Result<String>` | `src/compiler.rs:99-128` | Requires a successful status; returns the first stdout line; turns most spawn/probe failures into `MissingTool`, preserving cancellation |
| `detect_diagnostics_format(cc) -> DiagnosticsFormat` | `src/compiler.rs:130-158` | Tries `-fdiagnostics-format=json`, then `sarif-stderr`, then falls back to `Text` |

Both launch processes through `supervisor::output` with a **three-second** stage deadline and **1 MiB per-pipe** capture.

Format detection tests candidate commands and recognizes an unsupported-argument message, rather than comparing a hard-coded GCC version. A probe that returns output *without* that message can still select the attempted format, because the later parse has a text fallback.

`compile_project` itself probes `gcc --version`. `trace_program_inner` also probes required GCC and GDB **before** creating the session directory (`src/compiler.rs:535-537`; `src/trace.rs:203-229`).

## Step 2 · Assembling the exact argv

**Signature:** `fn build_argv(build: &BuildConfig, opts: &CompileOptions, diag_flag: Option<&str>) -> Vec<String>` (`src/compiler.rs:392-452`).

Every argument is passed **individually** to `tokio::process::Command`, never interpolated into a shell. The order matters:

```text
gcc -g -O0 -fno-omit-frame-pointer [-fsanitize=address,undefined]
    [-fdiagnostics-format=json | -fdiagnostics-format=sarif-stderr]
    [-Wl,--wrap=malloc -Wl,--wrap=calloc -Wl,--wrap=realloc -Wl,--wrap=free]
    [-D...] [-I...] [compiler_flags...]
    source_file [additional_sources...] [tracker_source] [object_files...]
    [-L...] [linker_flags...] [-l...] -o output_path
```

Brackets mark conditional or vector segments, not literal shell syntax.

| Fact | Detail |
|---|---|
| Sanitizer vs wrapper | Mutually exclusive in the standard constructors |
| User compiler flags | Come **after** `-g -O0 -fno-omit-frame-pointer`, so they **can override defaults** |
| Linking options | Come after the inputs (`src/compiler.rs:396-450`) |
| `program_args` | **Absent**, since they're for the inferior |
| Working directory | `compile` sets `current_dir` only when `build.working_directory` is present (`src/compiler.rs:471-475`) |
| Validation | Only checks the primary source path is nonempty; GCC handles unresolved extra inputs and linkage |
| Output paths | Debug: `<session_dir>/<output_name or program>_debug`; sanitized: `_asan` (`src/compiler.rs:539-565`) |

## Step 3 · Running GCC and handling failures

**Signature:** `pub async fn compile(build, cc, opts, format) -> Result<CompilationOutput>` (`src/compiler.rs:454-527`).

It runs the command via `supervisor::output(&mut cmd, supervisor::deadline(), 8 * 1024 * 1024)`.

| Situation | Result |
|---|---|
| Timeout or cancellation | An **engine error**, not a fabricated GCC diagnostic |
| Nonzero GCC status | Normally `Ok(CompilationOutput { success: false, ... })`; exit code `-1` if no numeric code exists |
| Success | Requires **both** a successful process status **and** no parsed `error`, `linker-error` or `fatal error` diagnostics (`src/compiler.rs:478-526`) |
| Output beyond the capture cap | The output carries `output_truncated`, but `compile` does **not** propagate that flag into `CompilationOutput`. So diagnostics or stderr beyond the cap can't be claimed complete |

`trace_program_inner` separately clamps the **whole build future** to `compiler_timeout_seconds` within the global deadline (`src/trace.rs:230-238`). Process cleanup can briefly outlast a deadline.

## Step 4 · The sanitizer fallback builds

```rust
let debug_result = compile(build, "gcc", &debug_opts, format).await?;
if !debug_result.success {
    return Ok((debug_result, not_built()));
}
for variant in ["address,undefined", "address", "undefined"] {
    let opts = CompileOptions::sanitized_with_variant(asan_out.clone(), variant);
    let result = compile(build, "gcc", &opts, format).await;
    // Schematic: actual loop breaks on a successful variant or an engine error.
}
```

This follows `pub async fn compile_project(build, session_dir, tracker_source) -> Result<(CompilationOutput, CompilationOutput)>` (`src/compiler.rs:529-581`).

| Behavior | Detail |
|---|---|
| Variant order | Combined ASan/UBSan, then ASan-only, then UBSan-only |
| A failed variant's output | Does **not** become the returned sanitizer result |
| If none succeeds | The returned sentinel is `not_built()` (with an error string in `raw_stderr` only when a compile call returned `Err`) |
| Debug build vs sanitizer failure | The successful debug build is **retained** even if optional sanitizer compilation fails |
| If the debug build failed | `trace_program_inner` records its diagnostics and stops without snapshots (`src/trace.rs:291-350`); optional sanitizer unavailability is marked separately |

Sanitizer runs, when triggered, are **separate executions**, not observations of the GDB process. See [diagnostics](engine-diagnostics.html).

## Step 5 · Normalizing diagnostic formats

| Parser | Location | Handles |
|---|---|---|
| `parse_legacy_gcc_json(stderr)` | `src/compiler.rs:232-259` | Legacy GCC JSON: first location/caret, flattens child notes |
| `parse_sarif(stderr)` | `src/compiler.rs:317-366` | SARIF: first physical location, maps camel-case region fields |
| `text_diagnostics(stderr)` | `src/compiler.rs:368-390` | Plain-text fallback |

Rules shared by all three:

- Legacy JSON and SARIF each support **consecutive documents** from multiple translation units.
- A **missing location** becomes `<unknown>`/zero, never a guessed source position.
- If structured parsing fails, it falls back to a **single text diagnostic** (up to 4,000 characters). Known linker phrases map to `linker-error`; any other nonempty stderr maps to `error`.
- The normalized payload is `CompilerDiagnostic { file, line, column, kind, message }` (`src/models.rs:50-59`).

**Tests:** `gcc_legacy_caret_locations_and_streams_are_preserved` and `gcc_sarif_stream_and_camel_case_region` (`src/compiler.rs:595-611`). The [final audit](../stage1-final-audit.md) independently confirms a live compilation failure produced diagnostics without debugger events.

> **Reminder:** No compiler output or tool version is evidence that a source expression executed. Runtime and static metadata have separate provenance (see [schema](schema.html)).

## Diagram notes

| Diagram | What it shows |
|---|---|
| **Compilation lifecycle**<br>{{diagram:d2-compile}} | GCC invocation, bounded output capture, diagnostic normalization and optional sanitizer variants. |
| **Build artifacts**<br>{{diagram:d2-build-artifacts}} | The wrapped debug executable is the **only** binary GDB executes; sanitizer artifacts and Clang syntax are separate. |
<!-- END CHAPTER: content/engine-compiler-deep.md -->

<!-- BEGIN CHAPTER: content/engine-gdb-deep.md -->
# 24 · GDB/MI2 controller and the recorded stop loop

> **In one sentence:** The machinery that sends GDB commands and waits for replies while the program runs and pauses.

## Key terms

| Term | Meaning |
|---|---|
| **Token** | A number attached to a debugger command so its reply can be routed back to the command that asked. |
| **Asynchronous stop** | A separate GDB message announcing the program paused, possibly after a command was already acknowledged. |

> **Example.** GDB may answer `4^running` to command 4. That only means it *started* the requested continuation. A later `*stopped` tells us where the program actually paused.
>
> A **missing reply** must never let late output answer the next command. A **missing stop** is a different timeout condition.

## The big picture

The debugger subsystem does two separate jobs:

1. **Own one GDB process** and turn its MI2 output into typed debugger facts.
2. **Run the stop loop** that decides what to retain.

A command acknowledgement is *never* treated as a `*stopped` event.

| Piece | File |
|---|---|
| Orchestration | `src/trace.rs` |
| Process, token and PTY machinery | `src/gdb_mi.rs` |
| Syntax decoding | `src/mi_parser.rs` |

## Part 1 · Orchestration around GDB (`src/trace.rs`)

**Principal signatures:**

- `pub(crate) async fn trace_program_inner(config: TraceConfig) -> Result<DebugSessionTrace>` (`src/trace.rs:167`)
- `pub async fn run_gdb_phase(config, limits, classifier, binary, event_file, event_status_file, reader, timeline, notes, type_metadata) -> Result<GdbPhaseResult>` (`src/trace.rs:1113-1124`)

### Who does what

| Function | Does |
|---|---|
| `trace_program_inner` | Probes tools, writes the C tracker source, compiles, optionally extracts static metadata, then calls `run_gdb_phase` with the event/status paths (`src/trace.rs:203-439`) |
| `run_gdb_phase` | Creates the `Recorder`, clones the `UserCodeClassifier`, records the injected allocator source as `instrumentation_file`, optionally creates a cgroup, builds `GdbLaunchOptions` (`src/trace.rs:1125-1189`) |

### Startup failures

`GdbSession::launch` is called at `src/trace.rs:1191-1212`.

| Failure | Becomes |
|---|---|
| Launch failure after the wall deadline expired | `Timeout` |
| Launch failure otherwise | `DebuggerFailure` |
| An output limit | `OutputLimitReached` |
| Missing `main` breakpoint | A **recorded limitation**, not a fabricated source checkpoint (`src/trace.rs:1226-1249`) |

### The stop loop

The `loop` starts at `src/trace.rs:1371`. Each iteration follows a **strict evidence order**:

| # | Action | Reference |
|---|---|---|
| 1 | Increment `stop_id`; check command and terminal conditions | |
| 2 | Drain `EventFileReader::read_new` | `src/trace.rs:1387-1405` |
| 3 | Mark transport damage | |
| 4 | Ingest rows into `HeapTimeline` | |
| 5 | Journal newly observed events with `Recorder::allocations(..., stop_id)` | |
| 6 | **Only then** classify an exit, signal, user-code stop or non-user frame | |

Why heap events come first: allocation events logged before this stop must be known **before** the snapshot is taken.

| Case | Handling |
|---|---|
| Fatal signals | Capture state and a crash report (`src/trace.rs:1441-1518`) |
| Resource vs benign signals | Distinguished by `classify_signal` (`src/trace.rs:2031-2063`) |
| Nonterminal limits | Break to cleanup and a gap |
| Normal exit / crash | Separate terminal branches (`src/trace.rs:1954-2024`) |

After termination, a **final `read_new`** preserves destructor and exit-handler events (`src/trace.rs:1977-1991`). Then `finish_phase`, debugger counters, profiling categories and `recorder.finish()` produce `GdbPhaseResult` (`src/trace.rs:1991-2024`). If multiple threads were observed, the recorder is truncated to **selected-thread-only** coverage.

## Part 2 · Session types and launch (`src/gdb_mi.rs`)

### Principal types

| Type | Location | Owns |
|---|---|---|
| `GdbSession` | `:270-318` | The child process, stdin, shared routing state, async receiver, activation map, caches |
| `Shared` | `:117-138` | Pending oneshot waiters, stream accumulation, program buffer, EOF flag, module epoch |
| `StoppedInfo` | `:139-164` | The typed asynchronous stop |
| `GdbLaunchOptions` | `:165-178` | Binary, cwd, args, env, limits |
| `MiCommandResponse` | `:179-221` | The tokened record plus stream text |
| `CheckpointReads` | `:222-269` | The per-stop byte-read reuse map |

### Launching GDB

**Signature:** `pub async fn launch(opts: &GdbLaunchOptions) -> Result<GdbSession>` (`src/gdb_mi.rs:389-512`).

- Refuses cancelled or expired sessions.
- Creates a **PTY** (a pseudo-terminal, so the inferior's output can be captured) and enables subreaper ownership.
- Starts `gdb --nx --quiet --interpreter=mi2 <binary>` with stdin/stdout piped and stderr null (`:396-423`).
- GDB gets its **own process group** and `kill_on_drop(true)`. No inferior rlimits are installed on GDB itself (`:411-416`).
- The reader task and PTY task are spawned **before** session configuration (`:446-450`).

### Configuration commands sent at launch (`:483-505`)

| Command | Why |
|---|---|
| `-gdb-set pagination off`, `confirm off` | No interactive prompts |
| `debuginfod enabled off` | No network symbol fetching |
| `mi-async on`, `non-stop off` | Asynchronous stops, all threads stop together |
| `style enabled off` | Plain output |
| `may-call-functions off` | GDB won't call functions in the inferior |
| `auto-load off` | No auto-loaded scripts |
| Set the inferior TTY; optionally pass program arguments | |

A configuration failure **kills** the session rather than returning a half-configured debugger. The inferior inherits `C_TRACE_EVENT_FILE` and `C_TRACE_EVENT_STATUS_FILE` from `run_gdb_phase` (`src/trace.rs:1176-1187`).

## Part 3 · Command tokens and asynchronous stops

### `execute`

`pub async fn execute(&mut self, command: &str) -> Result<MiCommandResponse>` (`src/gdb_mi.rs:533-621`)

1. Checks cancellation, poison, deadline and command cap.
2. Allocates `next_token`.
3. Registers a oneshot in `Shared.pending`.
4. Clears stale stream text.
5. Writes `"{token}{command}\n"`.
6. Waits **only** for that token's result.

> A response **timeout sets `poisoned = true`**, because a late stream can't safely be matched to a later command (`:600-608`).

### `wait_stopped`

`pub async fn wait_stopped(&mut self, timeout: Duration) -> Result<StoppedInfo>` (`:624-671`)

- Loops over the asynchronous channel and returns only `MiRecord::Exec { class: "stopped" }`.
- Records `selected_thread`, handles notifications such as `gdb-eof`, and buffers unrelated async records up to **4,096** (`:624-670`).
- `StoppedInfo::is_exit`, `is_signal`, `value` and `error_message` classify the record (`:139-191`).

### Reader and routing

| Component | Behavior | Location |
|---|---|---|
| `reader_task` | 16 MiB line bound; EOF fails every pending command and emits `gdb-eof` | `src/gdb_mi.rs:2302-2338` |
| `handle_line` | Ignores blank lines and GDB prompts; sends malformed lines to the bounded console log | `:2340-2372` |
| `route_record` | Sends tokened `^` results to the matching waiter; routes async records to the async channel; tracks module/thread/inferior notifications; treats console/log stream text as diagnostics | `:2374-2445` |
| PTY reader | Owns the master fd, appends inferior output to `ProgramBuffer`, notifies waiters when the output cap is first exceeded | `:2447-2484` |

> An asynchronous token is **not** a command acknowledgement, even if the numbers happen to match.

## Part 4 · Frame and variable capture

### Types

| Type | Location | Role |
|---|---|---|
| `UserCodeClassifier` | `src/gdb_mi.rs:1550-1559` | Distinguishes instrumentation runtime, user code, system library and unknown frames |
| `CaptureOptions` | `:1698-1717` | Variable/read/aggregate budgets, optional DWARF `ObservationTypes`, caller-frame limits, the transient-stop optimization |
| `CapturedState` | `:1740-1749` | Owns frames, variables, caller frames, memory usage and errors |

### `capture_state` (`src/gdb_mi.rs:1840-2227`)

`pub async fn capture_state(session, opts, reads) -> Result<CapturedState>`

| Step | What it does | Reference |
|---|---|---|
| 1 | Handles the derived-chain transient stop | `:1847-1862` |
| 2 | Otherwise runs `-stack-list-frames 0 200`, parses bounded frames, computes an **activation path** oldest-to-youngest | `:1864-1910` |
| 3 | A truncated chain **clears** activation continuity instead of reusing sliding prefixes | `:1889-1895` |
| 4 | Non-user frames return after frame/activation bookkeeping | `:1911-1913` |

**For user frames**, it then:

- executes `-stack-list-variables --all-values`,
- computes **duplicate-name counts** over the full list, and probes lexical symbols for the selected cap (`:1914-1947`),
- keeps duplicate lexical declarations as **separate variables** with activation/scoped identities. An unresolved collision becomes `unresolved:...`, not a guessed value (`:1981-2051`),
- otherwise uses fresh probe address/size/target data, falling back to `&name`, `sizeof(name)` or console `whatis` (`:2054-2127`).

**Variable identity** is `static:<scope>:<name>` or `activation:<id>:<scope>:<name>` (`:2129-2171`).

`prepare_variable_memory` preserves GDB's runtime extent and picks aggregate vs scalar read limits (`:1719-1736`). The actual bytes are read by `recorder::capture_variable_memory` (`src/recorder.rs:45-185`).

### Caller frames

`capture_caller` (`src/gdb_mi.rs:1753-1834`) never changes GDB's selected frame or thread, records per-caller errors, and assigns separate activation IDs. `capture_or_empty` in `trace.rs` keeps a stop even if capture fails (`src/trace.rs:2164-2175`).

## Part 5 · Memory, globals, shutdown and failure

| Function | Behavior | Location |
|---|---|---|
| `evaluate` | Runs `-data-evaluate-expression` | `src/gdb_mi.rs:1104-1118` |
| `read_memory_bytes_partial` | Validates address/range, returns empty for size zero, runs `-data-read-memory-bytes`, parses a **contiguous prefix** with an error | `:1136-1179` |
| `parse_memory_read` | Combines contiguous segments in address order but **never bridges holes** (tests at `:3330-3390`) | `:1415-1497` |
| `capture_global` (`recorder.rs`) | Proves static storage for qualified names when possible, evaluates the symbol, and caches only file-qualified globals within a module epoch | `src/recorder.rs:512-647` |
| `shutdown` | Tries `-gdb-exit`, waits up to **3 seconds**, kills the group on failure, aborts reader tasks, reaps | `src/gdb_mi.rs:1199-1216` |
| `kill` | The forced path | |

The **module epoch** advances when library-loaded/unloaded notifications arrive (`src/gdb_mi.rs:2418-2421`), which invalidates cached globals.

### Tests and limits

`src/gdb_mi.rs` tests cover: token/async separation, late results, malformed records, bounded frames, recursive caller shadowing, partial memory reads, budget exhaustion, output cap and module epoch (`:2494-3732`). The MI parser has independent stream/result tests (`src/mi_parser.rs:699-1043`).

These prove **implementation branches**, not that every C execution reaches every branch. **No DWARF location expression is evaluated here**; static metadata only qualifies observations.

See [MI2 parser](engine-mi-deep.html), [recorder](engine-recorder-deep.html), [stack/memory](engine-stack-deep.html) and [failure walkthrough](engine-walkthroughs.html).

## Diagram notes

| Diagram | What it shows |
|---|---|
| **Stop-loop choices**<br>{{diagram:d2-stop}} | Heap ingestion precedes capture; exit, signal, user-code and non-user frames follow different branches. |
| **Checkpoint construction**<br>{{diagram:d2-checkpoint}} | A retained frame plus bounded variable/byte capture become one snapshot and source-checkpoint index. |
| **MI command lifecycle**<br>{{diagram:d2-mi-cycle}} | Tokened results and asynchronous stops are separate channels. |
<!-- END CHAPTER: content/engine-gdb-deep.md -->

<!-- BEGIN CHAPTER: content/engine-mi-deep.md -->
# 25 · GDB/MI2 lexer and record parser

> **In one sentence:** MI2 looks like text, but it has a grammar. This parser turns one complete GDB line into typed fields *before* the engine interprets it.

## Key terms

| Term | Meaning |
|---|---|
| **Lexer** | The first pass that splits characters into pieces: numbers, commas, quoted strings, braces. |
| **Record** | One parsed GDB message. `^done` is a command **result**; `*stopped` reports the program **stopped**. |

> **Example.** For `4^done,frame={func="main"}`, the lexer sees token 4, the result marker and a `frame` field. The parser builds a structured result **for command 4**, *not* a new program stop.
>
> A partial or malformed line never becomes a valid frame or a fabricated checkpoint.

## The big picture

`src/mi_parser.rs` decodes **one complete line** of GDB's machine interface into a typed `MiRecord`.

- It is **not** the Trace Parser's JSON reader. It parses debugger *protocol* syntax before any engine evidence exists.
- The caller in `src/gdb_mi.rs` supplies newline-terminated lines from GDB's stdout.
- Malformed input is **logged**, never converted into a stop.

## Part 1 · Tokens and typed values

| Type | Location | Represents |
|---|---|---|
| `Token` | `src/mi_parser.rs:55-73` | An optional numeric command token, MI punctuation (`^ * = ~ @ & , { } [ ]`), a decoded quoted string, or a bare identifier |
| `MiValue` | `:80-85` | A constant, a tuple of results, or a list |
| `MiResult` | `:88-92` | A `variable=value` binding |
| `MiListItem` | `:96-99` | Distinguishes a plain list value from an inline result |
| `MiRecord` | `:117-137` | `Result`, `Exec`, `Notify` and `Stream { Console \| Target \| Log }` variants |

The parser **deliberately preserves unknown record classes and fields** instead of discarding them. Protocol extensions stay visible, without the engine inventing an interpretation.

## Part 2 · Lexing

| Function | Location | Job |
|---|---|---|
| `lex_line(input: &[u8]) -> Result<Vec<Token>, MiParseError>` | `src/mi_parser.rs:361-390` | Skips ASCII whitespace, tracks byte offsets for errors, repeatedly applies `next_token` |
| `next_token(input: &[u8]) -> IResult<&[u8], Token>` | `:340-358` | Tries number, quoted string, identifier, then each punctuation token |
| `lex_quoted_string` | `:259-329` | Decodes C-style escapes |
| `lex_ident` / `lex_number` | `:216-258` | MI identifier characters / the decimal command prefix |

**Escapes decoded:** `\n`, `\r`, `\t`, `\\`, `\"`, `\'`, `\e`, `\b`, `\f`, `\v`, `\a`, `\0`, and two-digit `\xHH`.

- An **unknown escape** degrades to the escaped character.
- An **unterminated string** is an error.

The GDB reader adds a separate **16 MiB line bound** and UTF-8-lossy conversion before calling `parse_line` (`src/gdb_mi.rs:2302-2323`). The parser itself expects a *complete* line; a partial line must never be parsed as a record.

## Part 3 · Recursive-descent parsing

**Signature:** `pub fn parse_line(line: &str) -> Result<Option<MiRecord>, MiParseError>` (`src/mi_parser.rs:559-637`)

Blank lines and `(gdb)` prompts return `Ok(None)`. Otherwise it lexes, optionally consumes a leading number as the token, then dispatches on the first punctuation:

| Prefix | Produced record |
|---|---|
| `^class,...` | `MiRecord::Result` |
| `*class,...` | `MiRecord::Exec` |
| `=class,...` | `MiRecord::Notify` |
| `~"text"` | console stream |
| `@"text"` | target stream |
| `&"text"` | log stream |

### The helper functions

| Function | Location | Requires / accepts |
|---|---|---|
| `expect_class` | `:639-647` | An identifier |
| `parse_to_end` | `:665` | Zero or more comma-separated result bindings |
| `parse_result` | `:474-493` | `name=value` |
| `parse_value` | `:428-471` | A quoted constant, tuple, list, or bare identifier |
| `parse_result_list` | `:496-521` | The MI list-of-results comma forms |
| `parse_list_items` | `:524-557` | Disambiguates `name=value` from a plain value by looking **one token ahead** |

## Worked example

This is an *illustrative* MI fragment, not captured output:

```text
4^done,frame={level="0",func="main",file="main.c",line="7",address="0x401136"}
```

| Stage | What happens |
|---|---|
| Lexer | Produces `Number(4)`, `Caret`, `Ident("done")`, `Comma`, `Ident("frame")`, `Equal`, `LBrace`, then the quoted fields and separators |
| `parse_line` | Consumes the token, emits a `Result` with token `Some(4)` and class `done`, stores a tuple under `frame` |
| `gdb_mi::route_record` | Sends it **only** to the pending command 4 |

A following `*stopped,reason="end-stepping-range",frame={...}` is an `Exec` record. It is delivered through the **asynchronous stop channel** and can never acknowledge command 4.

## Failure behavior and tests

`MiParseError` distinguishes (`src/mi_parser.rs:34-53`):

- unterminated strings,
- unexpected tokens,
- trailing characters,
- unknown records, and
- token overflow.

The GDB reader logs the line and error in a bounded console buffer and continues. A malformed line **never** fabricates a frame or a completion. EOF fails all pending commands (`src/gdb_mi.rs:2328-2337`).

**Tests** cover simple, result, exec, notify and stream records; empty containers; deep nesting; escapes; unknown classes; real GDB transcript fixtures; whitespace; token overflow; and seeded mutations (`src/mi_parser.rs:699-1043`). They validate **syntax preservation**. They do not claim GDB produced a particular record in a live session.

See the [GDB controller](engine-gdb-deep.html) for token routing and stop consumption.

## Diagram notes

| Diagram | What it shows |
|---|---|
| **MI parsing pipeline**<br>{{diagram:d2-mi-parse}} | Lexing, recursive value parsing, typed record classification and caller routing. |
<!-- END CHAPTER: content/engine-mi-deep.md -->

//! Build and execution configuration.
//!
//! [`BuildConfig`] is the single description of "what to compile and link"
//! and is designed to map 1:1 onto the future GUI build/linking panel (see
//! `docs/build-config.md`). [`TraceConfig`] wraps it with runtime settings
//! that do not affect compilation (program arguments, limits, output).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::resource_limits::ExecutionLimits;

/// Complete, configurable C build + link description.
///
/// Engine-required flags (debug info, frame pointers, allocation wrapping)
/// are kept separate from user flags by the compiler module; user flags are
/// always passed through verbatim so they can override engine defaults when
/// the user explicitly asks for it (see README "Flag precedence").
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct BuildConfig {
    /// Primary C source file (contains `main`).
    pub source_file: PathBuf,
    /// Additional C sources compiled and linked into the same binary.
    pub additional_sources: Vec<PathBuf>,
    /// Pre-built object files linked in without recompilation.
    pub object_files: Vec<PathBuf>,
    /// Include search directories (`-I`).
    pub include_dirs: Vec<PathBuf>,
    /// Preprocessor definitions (`-D`), given as `NAME` or `NAME=value`.
    pub defines: Vec<String>,
    /// Custom compiler flags, each element passed to gcc verbatim.
    pub compiler_flags: Vec<String>,
    /// Custom linker flags, each element passed to gcc verbatim.
    pub linker_flags: Vec<String>,
    /// Library search directories (`-L`).
    pub library_dirs: Vec<PathBuf>,
    /// Libraries to link (`-l`), without the leading `l`.
    pub libraries: Vec<String>,
    /// Optional executable name (defaults to a session-unique name).
    pub output_name: Option<String>,
    /// Working directory for compilation and (by default) execution.
    pub working_directory: Option<PathBuf>,
}

impl BuildConfig {
    /// All C sources that belong to the user's project (used to classify
    /// "user code" frames when stepping).
    pub fn user_sources(&self) -> Vec<PathBuf> {
        let mut v = vec![self.source_file.clone()];
        v.extend(self.additional_sources.iter().cloned());
        v.retain(|p| !p.as_os_str().is_empty());
        v
    }
}

/// Full request for one tracing session.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct TraceConfig {
    /// Build + link configuration.
    pub build: BuildConfig,
    /// Arguments handed to the traced program (never to the compiler).
    pub program_args: Vec<String>,
    /// Working directory for the inferior; defaults to the build working
    /// directory (or the process cwd when unset).
    pub inferior_working_directory: Option<PathBuf>,
    /// Resource limits.
    pub limits: ExecutionLimits,
    /// Optional path for the JSON trace file (stdout always gets the trace
    /// unless `stdout_json` is disabled).
    pub output_file: Option<PathBuf>,
    /// Write pretty-printed JSON (default true).
    pub pretty: bool,
    /// Keep the session temporary directory after the run (default false).
    pub keep_temp: bool,
    /// When to run the sanitizer reproduction pass (default: on crash or
    /// when Valgrind reports memory errors).
    pub sanitizer_trigger: SanitizerTrigger,
    /// Optional explicit path to the valgrind executable; resolved via PATH
    /// when unset (execvp semantics: non-executable matches are skipped).
    pub valgrind_path: Option<String>,
    /// Optional explicit path to the gdb executable; resolved via PATH when
    /// unset (execvp semantics).
    pub gdb_path: Option<String>,
    /// Optional supplementary Clang source metadata pass (default off).
    pub clang_ast: bool,
    /// Optional explicit Clang executable, for toolchain selection/tests.
    pub clang_path: Option<String>,
    pub tracing: TracingConfig,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            build: BuildConfig::default(),
            program_args: Vec::new(),
            inferior_working_directory: None,
            limits: ExecutionLimits::default(),
            output_file: None,
            pretty: true,
            keep_temp: false,
            sanitizer_trigger: SanitizerTrigger::default(),
            valgrind_path: None,
            gdb_path: None,
            clang_ast: false,
            clang_path: None,
            tracing: TracingConfig::default(),
        }
    }
}

/// JSON configuration file schema (`--config file.json`).
///
/// CLI flags that name the same field override the file; list-valued CLI
/// flags are appended to the file's lists.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigFile {
    pub source: Option<PathBuf>,
    pub additional_sources: Vec<PathBuf>,
    pub object_files: Vec<PathBuf>,
    pub include_dirs: Vec<PathBuf>,
    pub defines: Vec<String>,
    pub compiler_flags: Vec<String>,
    pub linker_flags: Vec<String>,
    pub library_dirs: Vec<PathBuf>,
    pub libraries: Vec<String>,
    pub output_name: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub program_args: Vec<String>,
    pub inferior_working_directory: Option<PathBuf>,
    pub limits: Option<ExecutionLimits>,
    pub output_file: Option<PathBuf>,
    pub pretty: Option<bool>,
    pub keep_temp: Option<bool>,
    pub sanitizer_trigger: Option<SanitizerTrigger>,
    pub valgrind_path: Option<String>,
    pub gdb_path: Option<String>,
    pub clang_ast: Option<bool>,
    pub clang_path: Option<String>,
    pub tracing: TracingConfig,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum TraceMode {
    Source,
    #[default]
    Dual,
}

/// When the sanitizer (ASan) reproduction pass runs (V2.1, fixes audit F-02).
///
/// `never` disables the pass; `on_crash` restores V2 behavior; the default
/// `on_crash_or_valgrind_error` additionally runs ASan when Valgrind reported
/// memory errors even though the traced run exited cleanly; `always` runs it
/// for every successful build, deadline permitting.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum SanitizerTrigger {
    Never,
    OnCrash,
    #[default]
    OnCrashOrValgrindError,
    Always,
}

impl SanitizerTrigger {
    pub fn runs_on_crash(self) -> bool {
        !matches!(self, SanitizerTrigger::Never)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct TracingConfig {
    pub mode: TraceMode,
    /// DEPRECATED since V2.1: per-instruction CPU register capture was
    /// removed (V2 audit: ~90% of debugger command time, no GUI consumer).
    /// Retained only so older config files keep parsing; has no effect.
    pub registers: bool,
    pub static_disassembly: bool,
    /// DEPRECATED since V2.1: was the periodic register checkpoint cadence.
    /// Retained only so older config files keep parsing; has no effect.
    pub register_checkpoint_interval: usize,
    /// User-specified expressions read at instruction stops. Never evaluated as writes.
    pub memory_regions: Vec<MemoryRegion>,
    pub globals: Vec<String>,
    pub discover_globals: bool,
    /// Durable JSONL journal; dynamic events need not remain resident.
    pub journal: Option<PathBuf>,
    pub retain_instruction_events: bool,
    /// DEPRECATED since V2.1: baseline profiling metrics are always collected
    /// (`profiling.enabled` is always `true`). Retained for config compat.
    pub profiling: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryRegion {
    pub address: String,
    pub size: usize,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            mode: TraceMode::Dual,
            registers: false,
            static_disassembly: true,
            register_checkpoint_interval: 0,
            memory_regions: Vec::new(),
            globals: Vec::new(),
            discover_globals: true,
            journal: None,
            retain_instruction_events: true,
            profiling: false,
        }
    }
}

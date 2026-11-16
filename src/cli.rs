//! CLI argument parsing and configuration merging.
//!
//! The model is deliberately kept front-end-agnostic so the future Tauri
//! desktop IDE can reuse `TraceConfig` directly: every CLI flag maps onto a
//! field of [`TraceConfig`]/[`BuildConfig`]/[`ExecutionLimits`], with the
//! documented precedence CLI > JSON config file > defaults (list-valued
//! flags append to the file's lists; scalar flags override).

use std::path::{Path, PathBuf};

use clap::Parser;

use crate::config::{ConfigFile, TraceConfig};
use crate::error::{EngineError, Result};

/// Command line interface.
///
/// Program arguments come after `--` (e.g. `cargo run -- sample.c -- 1 2 3`)
/// or via repeated `--arg` flags; they are handed to the traced program and
/// never to the compiler.
#[derive(Parser, Debug)]
#[command(
    name = "c-trace-engine",
    version,
    about = "Compile a C project, record GDB/MI2 execution snapshots and heap allocation events, and run independent memory analysis (Valgrind + sanitizers)."
)]
pub struct Cli {
    /// Primary C source file (positional form of --source).
    pub source_positional: Option<PathBuf>,

    /// Primary C source file (contains main).
    #[arg(long, conflicts_with = "source_positional")]
    pub source: Option<PathBuf>,

    /// Additional C sources compiled and linked into the same binary.
    #[arg(long = "add-source")]
    pub add_source: Vec<PathBuf>,

    /// Include search directories (-I).
    #[arg(long = "include-dir")]
    pub include_dir: Vec<PathBuf>,

    /// Preprocessor definitions (-D), NAME or NAME=value.
    #[arg(long = "define")]
    pub define: Vec<String>,

    /// Custom compiler flags, passed to gcc verbatim.
    #[arg(long = "compiler-flag")]
    pub compiler_flag: Vec<String>,

    /// Custom linker flags, passed to gcc verbatim.
    #[arg(long = "linker-flag")]
    pub linker_flag: Vec<String>,

    /// Library search directories (-L).
    #[arg(long = "library-dir")]
    pub library_dir: Vec<PathBuf>,

    /// Libraries to link (-l), without the leading `l`.
    #[arg(long = "library")]
    pub library: Vec<String>,

    /// Pre-built object files linked without recompilation.
    #[arg(long = "object-file")]
    pub object_file: Vec<PathBuf>,

    /// Executable name (defaults to a session-unique name).
    #[arg(long)]
    pub output_name: Option<String>,

    /// Working directory for compilation (and execution by default).
    #[arg(long)]
    pub working_directory: Option<PathBuf>,

    /// Working directory for the traced program (defaults to the build
    /// working directory).
    #[arg(long = "inferior-working-directory")]
    pub inferior_working_directory: Option<PathBuf>,

    /// Argument handed to the traced program (repeatable; same as trailing
    /// args after `--`).
    #[arg(long = "arg")]
    pub arg: Vec<String>,

    /// Program arguments: everything after the `--` separator
    /// (e.g. `c-trace-engine sample.c -- --flag 42`).
    #[arg(last = true, allow_hyphen_values = true)]
    pub program_args: Vec<String>,

    /// JSON configuration file; CLI flags take precedence.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Also write the JSON trace to this file (stdout always gets it).
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Compact JSON on stdout (default: pretty).
    #[arg(long)]
    pub no_pretty: bool,

    /// Keep the session temporary directory after the run.
    #[arg(long)]
    pub keep_temp: bool,

    // ---- resource limits ----------------------------------------------------
    /// Maximum GDB single-line steps (default 1000).
    #[arg(long)]
    pub max_steps: Option<usize>,
    /// Execution wall timeout in seconds (default 10).
    #[arg(long, alias = "max-execution-wall-time")]
    pub timeout: Option<u64>,
    /// Inferior address-space limit in MiB (default 256).
    #[arg(long)]
    pub max_memory_mb: Option<usize>,
    /// Captured stdout/stderr bytes (default 1 MiB).
    #[arg(long)]
    pub max_output_bytes: Option<usize>,
    /// Tracked allocations (default 10000).
    #[arg(long)]
    pub max_heap_allocations: Option<usize>,
    /// Maximum execution snapshots (default 1000).
    #[arg(long)]
    pub max_snapshots: Option<usize>,
    /// Raw memory captured per snapshot (default 1 MiB).
    #[arg(long)]
    pub max_snapshot_memory_bytes: Option<usize>,
    /// Maximum individual memory read (default 4096).
    #[arg(long)]
    pub max_memory_read_bytes: Option<usize>,
    /// Child processes the inferior may spawn (default 16).
    #[arg(long)]
    pub max_processes: Option<usize>,
    /// Variables introspected per snapshot (default 24).
    #[arg(long)]
    pub max_variables: Option<usize>,
    #[arg(long, value_enum)]
    pub trace_mode: Option<crate::config::TraceMode>,
    #[arg(long)]
    pub max_instruction_events: Option<usize>,
    #[arg(long)]
    pub max_gdb_commands: Option<u64>,
    #[arg(long)]
    pub max_trace_bytes: Option<usize>,
    #[arg(long)]
    pub max_trace_memory_bytes: Option<usize>,
    #[arg(long)]
    pub max_source_checkpoints: Option<usize>,
    #[arg(long)]
    pub journal: Option<PathBuf>,
    /// Keep dynamic instruction events only in the journal.
    #[arg(long, requires = "journal")]
    pub journal_only: bool,
    /// When to run the ASan reproduction pass (default: on crash or when
    /// Valgrind reports memory errors).
    #[arg(long, value_enum)]
    pub sanitizer_trigger: Option<crate::config::SanitizerTrigger>,
    /// Explicit path to the valgrind executable (default: resolve via PATH).
    #[arg(long)]
    pub valgrind_path: Option<String>,
    /// Explicit path to the gdb executable (default: resolve via PATH).
    #[arg(long)]
    pub gdb_path: Option<String>,
    /// Extra global/static GDB expression (repeatable).
    #[arg(long = "global")]
    pub globals: Vec<String>,
    #[arg(long)]
    pub max_globals: Option<usize>,
    #[arg(long)]
    pub compiler_timeout: Option<u64>,
    #[arg(long)]
    pub debugger_timeout: Option<u64>,
    #[arg(long)]
    pub analysis_timeout: Option<u64>,
    #[arg(long)]
    pub no_disassembly: bool,
    #[arg(long)]
    pub no_global_discovery: bool,
    /// Observe ADDRESS_EXPRESSION:BYTE_COUNT at each instruction stop.
    #[arg(long = "memory-region")]
    pub memory_regions: Vec<String>,
}

impl Cli {
    /// Build the effective [`TraceConfig`] with precedence
    /// CLI > JSON config file > defaults.
    pub fn to_config(&self) -> Result<TraceConfig> {
        // 1. JSON config file (optional).
        let file: ConfigFile = match &self.config {
            Some(path) => {
                let text = std::fs::read_to_string(path).map_err(|e| {
                    EngineError::Config(format!("cannot read config file {}: {e}", path.display()))
                })?;
                serde_json::from_str(&text).map_err(|e| {
                    EngineError::Config(format!("invalid config file {}: {e}", path.display()))
                })?
            }
            None => ConfigFile::default(),
        };

        // 2. Source resolution: --source > positional > config file.
        let source = self
            .source
            .clone()
            .or_else(|| self.source_positional.clone())
            .or_else(|| file.source.clone())
            .ok_or_else(|| {
                EngineError::Config(
                    "no source file given (pass a .c file or --source <file>)".into(),
                )
            })?;
        let mut build = file_build(&file, &source);
        // CLI list flags APPEND to the config file's lists.
        build
            .additional_sources
            .extend(self.add_source.iter().cloned());
        build.include_dirs.extend(self.include_dir.iter().cloned());
        build.defines.extend(self.define.iter().cloned());
        build
            .compiler_flags
            .extend(self.compiler_flag.iter().cloned());
        build.linker_flags.extend(self.linker_flag.iter().cloned());
        build.library_dirs.extend(self.library_dir.iter().cloned());
        build.libraries.extend(self.library.iter().cloned());
        build.object_files.extend(self.object_file.iter().cloned());
        if self.output_name.is_some() {
            build.output_name = self.output_name.clone();
        }
        if self.working_directory.is_some() {
            build.working_directory = self.working_directory.clone();
        }
        let resolved_source = if source.is_relative() {
            build
                .working_directory
                .as_ref()
                .map(|d| d.join(&source))
                .unwrap_or(source.clone())
        } else {
            source.clone()
        };
        if !resolved_source.is_file() {
            return Err(EngineError::Config(format!(
                "source file not found: {}",
                resolved_source.display()
            )));
        }

        // 3. Program arguments: config file + --arg flags + trailing args.
        let mut program_args = file.program_args.clone();
        program_args.extend(self.arg.iter().cloned());
        program_args.extend(self.program_args.iter().cloned());

        let mut limits = file.limits.clone().unwrap_or_default();
        if let Some(v) = self.max_steps {
            limits.max_steps = v;
        }
        if let Some(v) = self.timeout {
            limits.timeout_seconds = v;
        }
        if let Some(v) = self.max_memory_mb {
            limits.max_memory_mb = v;
        }
        if let Some(v) = self.max_output_bytes {
            limits.max_output_bytes = v;
        }
        if let Some(v) = self.max_heap_allocations {
            limits.max_heap_allocations = v;
        }
        if let Some(v) = self.max_snapshots {
            limits.max_snapshot_count = v;
        }
        if let Some(v) = self.max_snapshot_memory_bytes {
            limits.max_snapshot_memory_bytes = v;
        }
        if let Some(v) = self.max_memory_read_bytes {
            limits.max_memory_read_bytes = v;
        }
        if let Some(v) = self.max_processes {
            limits.max_process_count = v;
        }
        if let Some(v) = self.max_variables {
            limits.max_variables_per_snapshot = v;
        }

        let pretty = if self.no_pretty {
            false
        } else {
            file.pretty.unwrap_or(true)
        };
        let keep_temp = self.keep_temp || file.keep_temp.unwrap_or(false);
        let output_file = self.output.clone().or(file.output_file.clone());
        let mut tracing = file.tracing.clone();
        if let Some(mode) = self.trace_mode {
            tracing.mode = mode;
        }
        tracing.globals.extend(self.globals.clone());
        if self.journal.is_some() {
            tracing.journal = self.journal.clone();
        }
        if self.journal_only {
            tracing.retain_instruction_events = false;
        }
        if self.no_disassembly {
            tracing.static_disassembly = false;
        }
        if self.no_global_discovery {
            tracing.discover_globals = false;
        }
        for region in &self.memory_regions {
            let (address, size) = region.rsplit_once(':').ok_or_else(|| {
                EngineError::Config("memory-region requires expression:bytes".into())
            })?;
            tracing.memory_regions.push(crate::config::MemoryRegion {
                address: address.into(),
                size: size
                    .parse()
                    .map_err(|_| EngineError::Config("invalid memory-region byte count".into()))?,
            });
        }
        if let Some(v) = self.max_instruction_events {
            limits.max_instruction_events = v;
        }
        if let Some(v) = self.max_gdb_commands {
            limits.max_gdb_commands = v;
        }
        if let Some(v) = self.max_trace_bytes {
            limits.max_trace_bytes = v;
        }
        if let Some(v) = self.max_trace_memory_bytes {
            limits.max_trace_memory_bytes = v;
        }
        if let Some(v) = self.max_source_checkpoints {
            limits.max_source_checkpoints = v;
        }
        if let Some(v) = self.max_globals {
            limits.max_globals = v;
        }
        if let Some(v) = self.compiler_timeout {
            limits.compiler_timeout_seconds = v;
        }
        if let Some(v) = self.debugger_timeout {
            limits.debugger_timeout_seconds = v;
        }
        if let Some(v) = self.analysis_timeout {
            limits.analysis_timeout_seconds = v;
        }

        Ok(TraceConfig {
            build,
            program_args,
            inferior_working_directory: self
                .inferior_working_directory
                .clone()
                .or(file.inferior_working_directory.clone()),
            limits,
            output_file,
            pretty,
            keep_temp,
            sanitizer_trigger: self
                .sanitizer_trigger
                .or(file.sanitizer_trigger)
                .unwrap_or_default(),
            valgrind_path: self.valgrind_path.clone().or(file.valgrind_path.clone()),
            gdb_path: self.gdb_path.clone().or(file.gdb_path.clone()),
            tracing,
        })
    }
}

fn file_build(file: &ConfigFile, source: &Path) -> crate::config::BuildConfig {
    crate::config::BuildConfig {
        source_file: source.to_path_buf(),
        additional_sources: file.additional_sources.clone(),
        object_files: file.object_files.clone(),
        include_dirs: file.include_dirs.clone(),
        defines: file.defines.clone(),
        compiler_flags: file.compiler_flags.clone(),
        linker_flags: file.linker_flags.clone(),
        library_dirs: file.library_dirs.clone(),
        libraries: file.libraries.clone(),
        output_name: file.output_name.clone(),
        working_directory: file.working_directory.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Result<TraceConfig> {
        Cli::try_parse_from(std::iter::once("c-trace-engine").chain(args.iter().copied()))
            .map_err(|e| EngineError::Config(e.to_string()))?
            .to_config()
    }

    #[test]
    fn positional_source() {
        let dir = std::env::temp_dir().join(format!("cte-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("sample.c");
        std::fs::write(&src, "int main(void){return 0;}").unwrap();
        let cfg = cli(&[src.to_str().unwrap()]).unwrap();
        assert_eq!(cfg.build.source_file, src);
        assert!(cfg.pretty);
        assert!(!cfg.keep_temp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_source_is_config_error() {
        let err = cli(&["/definitely/not/here.c"]).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn no_source_is_config_error() {
        let err = cli(&[]).unwrap_err();
        assert!(err.to_string().contains("no source file"));
    }

    #[test]
    fn flag_accumulation_and_program_args() {
        let dir = std::env::temp_dir().join(format!("cte-cli2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("main.c");
        std::fs::write(&src, "int main(void){return 0;}").unwrap();
        let helper = dir.join("helper.c");
        std::fs::write(&helper, "int f(void){return 1;}").unwrap();
        let cfg = cli(&[
            "--source",
            src.to_str().unwrap(),
            "--add-source",
            helper.to_str().unwrap(),
            "--include-dir",
            "./include",
            "--define",
            "FOO=1",
            "--library",
            "m",
            "--linker-flag=-pthread",
            "--",
            "--verbose",
            "42",
        ])
        .unwrap();
        assert_eq!(cfg.build.additional_sources, vec![helper]);
        assert_eq!(cfg.build.include_dirs, vec![PathBuf::from("./include")]);
        assert_eq!(cfg.build.defines, vec!["FOO=1".to_string()]);
        assert_eq!(cfg.build.libraries, vec!["m".to_string()]);
        assert_eq!(cfg.build.linker_flags, vec!["-pthread".to_string()]);
        assert_eq!(
            cfg.program_args,
            vec!["--verbose".to_string(), "42".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn limit_flags_override_defaults() {
        let dir = std::env::temp_dir().join(format!("cte-cli3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("s.c");
        std::fs::write(&src, "int main(void){return 0;}").unwrap();
        let cfg = cli(&[
            src.to_str().unwrap(),
            "--timeout",
            "3",
            "--max-steps",
            "17",
            "--max-memory-mb",
            "64",
            "--max-snapshots",
            "5",
        ])
        .unwrap();
        assert_eq!(cfg.limits.timeout_seconds, 3);
        assert_eq!(cfg.limits.max_steps, 17);
        assert_eq!(cfg.limits.max_memory_mb, 64);
        assert_eq!(cfg.limits.max_snapshot_count, 5);
        assert_eq!(cfg.limits.max_output_bytes, 1024 * 1024); // untouched default
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_file_merge_precedence() {
        let dir = std::env::temp_dir().join(format!("cte-cli4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("s.c");
        std::fs::write(&src, "int main(void){return 0;}").unwrap();
        let cfg_path = dir.join("cfg.json");
        std::fs::write(
            &cfg_path,
            r#"{
                "source": "SHOULD_BE_OVERRIDDEN",
                "defines": ["FROM_FILE=1"],
                "limits": {"timeout_seconds": 7},
                "pretty": false
            }"#,
        )
        .unwrap();
        let cfg = cli(&[
            "--config",
            cfg_path.to_str().unwrap(),
            "--source",
            src.to_str().unwrap(),
            "--define",
            "FROM_CLI=2",
            "--timeout",
            "2",
        ])
        .unwrap();
        // CLI scalar beats file
        assert_eq!(cfg.limits.timeout_seconds, 2);
        // CLI list appends to file list
        assert_eq!(cfg.build.defines, vec!["FROM_FILE=1", "FROM_CLI=2"]);
        // file-only scalar applies
        assert!(!cfg.pretty);
        // limits not mentioned in the file keep engine defaults
        assert_eq!(cfg.limits.max_steps, 1000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

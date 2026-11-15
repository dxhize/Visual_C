//! GCC invocation, build configuration, and diagnostics.
//!
//! The engine compiles the project twice: a **debug binary** (with the
//! allocation instrumentation runtime linked in via `-Wl,--wrap=...`) and a
//! **sanitized binary** (AddressSanitizer + UBSan, without the runtime).
//! Engine-required flags are kept separate from user flags: engine flags go
//! first, user flags follow, so an explicit user flag can override an engine
//! default (documented behavior, see README "Flag precedence").
//!
//! Diagnostics: GCC ≥ 16 removed `-fdiagnostics-format=json` in favor of
//! SARIF. The compiler module *probes* the actual toolchain once per session
//! (`json` → `sarif-stderr` → plain text) and normalizes every supported
//! format into [`CompilerDiagnostic`]. Linker errors (not covered by either
//! JSON format) are captured as raw stderr with kind `linker-error`.
//!
//! All arguments are passed individually to `tokio::process::Command`; shell
//! command strings are never constructed.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::process::Command;

use crate::config::BuildConfig;
use crate::error::{EngineError, Result};
use crate::models::CompilerDiagnostic;

/// Compilation-time options chosen by the orchestrator.
#[derive(Clone, Debug)]
pub struct CompileOptions {
    /// Build the ASan/UBSan variant instead of the debug variant.
    pub sanitized: bool,
    /// Sanitizer selection for the sanitized build (`-fsanitize=` value).
    pub sanitizer_variant: &'static str,
    /// Link the allocation instrumentation runtime (debug build only).
    pub include_alloc_tracker: bool,
    /// Path of the pre-written `alloc_tracker.c` translation unit.
    pub tracker_source: Option<PathBuf>,
    /// Where to place the executable.
    pub output_path: PathBuf,
}

impl CompileOptions {
    pub fn debug(output_path: PathBuf, tracker_source: Option<PathBuf>) -> Self {
        Self {
            sanitized: false,
            sanitizer_variant: "",
            include_alloc_tracker: tracker_source.is_some(),
            tracker_source,
            output_path,
        }
    }

    pub fn sanitized(output_path: PathBuf) -> Self {
        Self {
            sanitized: true,
            sanitizer_variant: "address,undefined",
            include_alloc_tracker: false,
            tracker_source: None,
            output_path,
        }
    }

    pub fn sanitized_with_variant(output_path: PathBuf, variant: &'static str) -> Self {
        Self {
            sanitized: true,
            sanitizer_variant: variant,
            include_alloc_tracker: false,
            tracker_source: None,
            output_path,
        }
    }
}

/// Result of one compilation.
#[derive(Clone, Debug)]
pub struct CompilationOutput {
    pub sanitizer_variant: Option<String>,
    pub success: bool,
    pub exit_code: i32,
    pub binary_path: PathBuf,
    pub diagnostics: Vec<CompilerDiagnostic>,
    /// Raw compiler stderr (always captured; used for linker errors).
    pub raw_stderr: String,
    /// Diagnostics format that the toolchain accepted.
    pub diagnostics_format: DiagnosticsFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticsFormat {
    /// `-fdiagnostics-format=json` (GCC ≤ 15).
    LegacyJson,
    /// `-fdiagnostics-format=sarif-stderr` (GCC ≥ 15/16).
    Sarif,
    /// No machine-readable diagnostics available.
    Text,
}

/// Verify that a tool exists and can be executed; returns its version line.
pub async fn probe_tool(tool: &str, version_arg: &str) -> Result<String> {
    let tool_owned = tool.to_string();
    let version_arg = version_arg.to_string();
    let version_display = version_arg.clone();
    let output = crate::supervisor::output(
        Command::new(&tool_owned).arg(version_arg),
        crate::supervisor::stage_deadline(3),
        1024 * 1024,
    )
    .await
    .map_err(|e| match e {
        EngineError::Cancelled => EngineError::Cancelled,
        _ => EngineError::MissingTool {
            tool: tool_owned.clone(),
            reason: format!("could not spawn `{}`: {}", tool_owned, e),
        },
    })?;
    if output.cancelled {
        return Err(EngineError::Cancelled);
    }
    if !output.status.is_some_and(|s| s.success()) {
        return Err(EngineError::MissingTool {
            tool: tool_owned.clone(),
            reason: format!("`{} {}` failed", tool_owned, version_display),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().next().unwrap_or_default().to_string())
}

/// Probe which diagnostics format the compiler supports. GCC 16 removed
/// `json`; GCC ≥ 15 offers SARIF; older versions offer JSON.
pub async fn detect_diagnostics_format(cc: &str) -> DiagnosticsFormat {
    for (fmt, flag) in [
        (DiagnosticsFormat::LegacyJson, "-fdiagnostics-format=json"),
        (
            DiagnosticsFormat::Sarif,
            "-fdiagnostics-format=sarif-stderr",
        ),
    ] {
        let probe = crate::supervisor::output(
            Command::new(cc).args(["-x", "c", "-fsyntax-only", flag, "/dev/null"]),
            crate::supervisor::stage_deadline(3),
            1024 * 1024,
        )
        .await;
        match probe {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("unrecognized argument in option") {
                    continue; // flag unsupported, try next
                }
                return fmt;
            }
            Err(_) => continue,
        }
    }
    DiagnosticsFormat::Text
}

#[derive(Deserialize, Debug)]
struct GccJsonLocation {
    #[serde(default)]
    caret: Option<Box<GccJsonLocation>>,
    #[serde(default)]
    file: Option<GccJsonFile>,
    #[serde(default)]
    line: Option<serde_json::Number>,
    #[serde(default)]
    column: Option<serde_json::Number>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum GccJsonFile {
    Path(String),
    Object { name: Option<String> },
}
impl GccJsonFile {
    fn name(&self) -> Option<String> {
        match self {
            Self::Path(path) => Some(path.clone()),
            Self::Object { name } => name.clone(),
        }
    }
}

#[derive(Deserialize, Debug)]
struct GccJsonDiagnostic {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    locations: Vec<GccJsonLocation>,
    #[serde(default)]
    children: Vec<GccJsonDiagnostic>,
}

fn diag_from_gcc_json(d: &GccJsonDiagnostic) -> CompilerDiagnostic {
    let (file, line, column) = d
        .locations
        .first()
        .map(|l| l.caret.as_deref().unwrap_or(l))
        .map(|l| {
            (
                l.file
                    .as_ref()
                    .and_then(|f| f.name())
                    .unwrap_or_else(|| "<unknown>".into()),
                l.line
                    .as_ref()
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
                l.column
                    .as_ref()
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0)
                    .min(u32::MAX as u64) as u32,
            )
        })
        .unwrap_or(("<unknown>".into(), 0, 0));
    CompilerDiagnostic {
        file,
        line,
        column,
        kind: d.kind.clone().unwrap_or_else(|| "message".into()),
        message: d.message.clone().unwrap_or_default(),
    }
}

fn parse_legacy_gcc_json(stderr: &str) -> Option<Vec<CompilerDiagnostic>> {
    let trimmed = stderr.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    // GCC prints one JSON document per translation unit; parse the stream of
    // consecutive documents.
    let stream = serde_json::Deserializer::from_str(trimmed).into_iter::<Vec<GccJsonDiagnostic>>();
    let mut out = Vec::new();
    let mut any = false;
    for doc in stream {
        let diags = doc.ok()?;
        any = true;
        for d in diags {
            out.push(diag_from_gcc_json(&d));
            // GCC attaches notes as `children`; flatten them so the IDE can
            // show them as separate entries.
            for child in &d.children {
                let mut c = diag_from_gcc_json(child);
                if c.kind.is_empty() {
                    c.kind = "note".into();
                }
                out.push(c);
            }
        }
    }
    any.then_some(out)
}

#[derive(Deserialize, Debug)]
struct SarifMessage {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize, Debug)]
struct SarifRegion {
    #[serde(default, rename = "startLine")]
    startline: Option<serde_json::Number>,
    #[serde(default, rename = "startColumn")]
    startcolumn: Option<serde_json::Number>,
}

#[derive(Deserialize, Debug)]
struct SarifArtifactLocation {
    #[serde(default, rename = "uri")]
    uri: Option<String>,
}

#[derive(Deserialize, Debug)]
struct SarifPhysicalLocation {
    #[serde(default, rename = "artifactLocation")]
    artifact_location: Option<SarifArtifactLocation>,
    #[serde(default)]
    region: Option<SarifRegion>,
}

#[derive(Deserialize, Debug)]
struct SarifLocation {
    #[serde(default, rename = "physicalLocation")]
    physical_location: Option<SarifPhysicalLocation>,
}

#[derive(Deserialize, Debug)]
struct SarifResult {
    #[serde(default, rename = "level")]
    level: Option<String>,
    #[serde(default)]
    message: Option<SarifMessage>,
    #[serde(default)]
    locations: Vec<SarifLocation>,
}

#[derive(Deserialize, Debug)]
struct SarifRun {
    #[serde(default)]
    results: Vec<SarifResult>,
}

#[derive(Deserialize, Debug)]
struct SarifDoc {
    #[serde(default)]
    runs: Vec<SarifRun>,
}

fn parse_sarif(stderr: &str) -> Option<Vec<CompilerDiagnostic>> {
    let trimmed = stderr.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    // GCC prints one SARIF document per translation unit (even when a unit
    // has no diagnostics); parse the stream of consecutive documents.
    let stream = serde_json::Deserializer::from_str(trimmed).into_iter::<SarifDoc>();
    let mut out = Vec::new();
    let mut any = false;
    for doc in stream {
        let doc = doc.ok()?;
        any = true;
        for run in doc.runs {
            for r in run.results {
                let (file, line, column) = r
                    .locations
                    .first()
                    .and_then(|l| l.physical_location.as_ref())
                    .map(|p| {
                        (
                            p.artifact_location
                                .as_ref()
                                .and_then(|a| a.uri.clone())
                                .unwrap_or_else(|| "<unknown>".into()),
                            p.region
                                .as_ref()
                                .and_then(|rg| rg.startline.as_ref().and_then(|n| n.as_u64()))
                                .unwrap_or(0)
                                .min(u32::MAX as u64) as u32,
                            p.region
                                .as_ref()
                                .and_then(|rg| rg.startcolumn.as_ref().and_then(|n| n.as_u64()))
                                .unwrap_or(0)
                                .min(u32::MAX as u64) as u32,
                        )
                    })
                    .unwrap_or(("<unknown>".into(), 0, 0));
                out.push(CompilerDiagnostic {
                    file,
                    line,
                    column,
                    kind: r.level.clone().unwrap_or_else(|| "message".into()),
                    message: r.message.and_then(|m| m.text).unwrap_or_default(),
                });
            }
        }
    }
    any.then_some(out)
}

/// Build a fallback diagnostic from plain-text stderr (e.g. linker errors).
fn text_diagnostics(stderr: &str) -> Vec<CompilerDiagnostic> {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let kind = if trimmed.contains("undefined reference")
        || trimmed.contains("multiple definition")
        || trimmed.contains("ld returned")
    {
        "linker-error"
    } else {
        "error"
    };
    let message: String = trimmed.chars().take(4000).collect();
    vec![CompilerDiagnostic {
        file: String::new(),
        line: 0,
        column: 0,
        kind: kind.into(),
        message,
    }]
}

/// Build the full gcc argument vector for one binary.
fn build_argv(build: &BuildConfig, opts: &CompileOptions, diag_flag: Option<&str>) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();

    // --- engine-required flags (kept separate from user flags) ---
    argv.push("-g".into());
    argv.push("-O0".into());
    argv.push("-fno-omit-frame-pointer".into());
    if opts.sanitized {
        argv.push(format!("-fsanitize={}", opts.sanitizer_variant));
    }
    if let Some(flag) = diag_flag {
        argv.push(flag.into());
    }
    if opts.include_alloc_tracker {
        argv.extend(
            [
                "-Wl,--wrap=malloc",
                "-Wl,--wrap=calloc",
                "-Wl,--wrap=realloc",
                "-Wl,--wrap=free",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
    }

    // --- user configuration ---
    for d in &build.defines {
        argv.push(format!("-D{}", d));
    }
    for dir in &build.include_dirs {
        argv.push(format!("-I{}", dir.display()));
    }
    argv.extend(build.compiler_flags.iter().cloned());

    // --- inputs ---
    argv.push(build.source_file.display().to_string());
    for s in &build.additional_sources {
        argv.push(s.display().to_string());
    }
    if let Some(tracker) = &opts.tracker_source {
        argv.push(tracker.display().to_string());
    }
    for o in &build.object_files {
        argv.push(o.display().to_string());
    }

    // --- linking ---
    for dir in &build.library_dirs {
        argv.push(format!("-L{}", dir.display()));
    }
    argv.extend(build.linker_flags.iter().cloned());
    for lib in &build.libraries {
        argv.push(format!("-l{}", lib));
    }

    argv.push("-o".into());
    argv.push(opts.output_path.display().to_string());
    argv
}

/// Compile (and link) the configured project once.
pub async fn compile(
    build: &BuildConfig,
    cc: &str,
    opts: &CompileOptions,
    format: DiagnosticsFormat,
) -> Result<CompilationOutput> {
    if build.source_file.as_os_str().is_empty() {
        return Err(EngineError::Config("no source file configured".into()));
    }
    let diag_flag = match format {
        DiagnosticsFormat::LegacyJson => Some("-fdiagnostics-format=json"),
        DiagnosticsFormat::Sarif => Some("-fdiagnostics-format=sarif-stderr"),
        DiagnosticsFormat::Text => None,
    };
    let argv = build_argv(build, opts, diag_flag);

    let mut cmd = Command::new(cc);
    cmd.args(&argv);
    if let Some(dir) = &build.working_directory {
        cmd.current_dir(dir);
    }
    // Compilation gets its own generous ceiling; the execution limits apply
    // to the inferior, not to the compiler.
    let output =
        crate::supervisor::output(&mut cmd, crate::supervisor::deadline(), 8 * 1024 * 1024).await?;
    if output.timed_out {
        return Err(crate::supervisor::timeout_error("compilation"));
    }
    if output.cancelled {
        return Err(EngineError::Cancelled);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = stdout; // gcc diagnostics go to stderr

    let success = output.status.is_some_and(|s| s.success());

    let diagnostics = match format {
        DiagnosticsFormat::LegacyJson => {
            parse_legacy_gcc_json(&stderr).unwrap_or_else(|| text_diagnostics(&stderr))
        }
        DiagnosticsFormat::Sarif => {
            parse_sarif(&stderr).unwrap_or_else(|| text_diagnostics(&stderr))
        }
        DiagnosticsFormat::Text => {
            // Text mode: only produce a diagnostic when the build failed or
            // stderr is non-empty (warnings are unparseable in text mode but
            // must not be lost).
            if success && stderr.trim().is_empty() {
                Vec::new()
            } else {
                text_diagnostics(&stderr)
            }
        }
    };
    // In JSON modes gcc prints only the diagnostic JSON; if the toolchain
    // succeeded but no diagnostics came back, that is normal (no warnings).
    let has_errors = diagnostics
        .iter()
        .any(|d| d.kind == "error" || d.kind == "linker-error" || d.kind == "fatal error");
    let success = success && !has_errors;

    Ok(CompilationOutput {
        sanitizer_variant: opts.sanitized.then(|| opts.sanitizer_variant.to_string()),
        success,
        exit_code: output.status.and_then(|s| s.code()).unwrap_or(-1),
        binary_path: opts.output_path.clone(),
        diagnostics,
        raw_stderr: stderr,
        diagnostics_format: format,
    })
}

/// Convenience: compile with the standard engine layout for one session dir.
pub async fn compile_project(
    build: &BuildConfig,
    session_dir: &Path,
    tracker_source: Option<PathBuf>,
) -> Result<(CompilationOutput, CompilationOutput)> {
    let cc = "gcc";
    let _gcc_version = probe_tool(cc, "--version").await?;
    let format = detect_diagnostics_format(cc).await;

    let debug_name = build
        .output_name
        .clone()
        .unwrap_or_else(|| "program".to_string());
    let debug_out = session_dir.join(format!("{}_debug", debug_name));

    let debug_opts = CompileOptions::debug(debug_out, tracker_source);
    let debug_result = compile(build, cc, &debug_opts, format).await?;

    if !debug_result.success {
        return Ok((debug_result, not_built()));
    }

    // The sanitized build reuses the user's configuration but not the
    // instrumentation runtime: ASan replaces the allocator at interception
    // level, and wrapping the allocator underneath ASan would double-report.
    // Sanitizer runtimes may be partially installed (e.g. libasan present but
    // libubsan missing), so degrade: combined → asan-only → ubsan-only → none.
    let asan_out = session_dir.join(format!("{}_asan", debug_name));
    let variants: [(&'static str, &'static str); 3] = [
        ("address,undefined", "asan+ubsan"),
        ("address", "asan"),
        ("undefined", "ubsan"),
    ];
    let mut asan_result = not_built();
    for (flag, _label) in variants {
        let opts = CompileOptions::sanitized_with_variant(asan_out.clone(), flag);
        let r = match compile(build, cc, &opts, format).await {
            Ok(result) => result,
            Err(e) => {
                // The already completed debug build must survive a failed optional build.
                asan_result.raw_stderr = e.to_string();
                break;
            }
        };
        if r.success {
            asan_result = r;
            break;
        }
    }

    Ok((debug_result, asan_result))
}

fn not_built() -> CompilationOutput {
    CompilationOutput {
        sanitizer_variant: None,
        success: false,
        exit_code: -1,
        binary_path: PathBuf::new(),
        diagnostics: Vec::new(),
        raw_stderr: String::new(),
        diagnostics_format: DiagnosticsFormat::Text,
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn gcc_legacy_caret_locations_and_streams_are_preserved() {
        let diagnostics = parse_legacy_gcc_json(r#"[{"kind":"error","message":"native error","locations":[{"caret":{"file":"input.c","line":7,"column":3}}]}] []"#).unwrap();
        assert_eq!(diagnostics[0].file, "input.c");
        assert_eq!(diagnostics[0].line, 7);
        assert_eq!(diagnostics[0].column, 3);
    }
    #[test]
    fn gcc_sarif_stream_and_camel_case_region() {
        let diagnostics = parse_sarif(r#"{"runs":[{"results":[{"level":"error","message":{"text":"native error"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"x.c"},"region":{"startLine":5,"startColumn":9}}}]}]}]} {"runs":[]}"#).unwrap();
        assert_eq!(diagnostics[0].line, 5);
        assert_eq!(diagnostics[0].column, 9);
    }
}

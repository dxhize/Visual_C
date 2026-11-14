//! Independent Valgrind memcheck execution and XML report parsing.
//!
//! Valgrind runs in its own subprocess with `--xml-file` capturing the
//! machine-readable report away from both the program's stdout (captured
//! separately) and its stderr (captured separately). The XML is parsed with
//! `roxmltree` — a real XML parser, never regex.
//!
//! The documented command template in the spec used `--xml-fd=2`; the engine
//! deliberately uses `--xml-file` instead: with `--xml-fd=2` the client's
//! stderr writes land on the same descriptor as the XML stream and can
//! corrupt it mid-document. `--xml-file` keeps XML, program stdout and
//! program stderr on three separate channels (see README "Deviations").
//!
//! Valgrind reports allocation *summaries* and memory *errors* with source
//! locations. It does not report a time-indexed heap history — that is the
//! job of the allocation instrumentation + GDB memory reads.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::AsyncReadExt;

use crate::error::{EngineError, Result};
use crate::models::{ValgrindError, ValgrindFrame, ValgrindReport};

/// Run Valgrind memcheck against the debug binary.
///
/// Returns the parsed report plus raw captured output. The event-trace file
/// environment variable is set so the instrumented binary records its own
/// allocation history during the Valgrind run (used for allocation totals
/// cross-checking; the authoritative totals come from the XML itself).
pub struct ValgrindRun {
    pub report: Option<ValgrindReport>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub xml_path: Option<PathBuf>,
}

pub async fn run_valgrind(
    binary: &Path,
    program_args: &[String],
    working_directory: Option<&Path>,
    limits: &crate::resource_limits::ExecutionLimits,
    xml_path: &Path,
    event_file: &Path,
) -> Result<ValgrindRun> {
    let mut cmd = tokio::process::Command::new("valgrind");
    cmd.args([
        "--tool=memcheck",
        "--leak-check=full",
        "--show-leak-kinds=all",
        "--track-origins=yes",
        "--xml=yes",
    ])
    .arg(format!("--xml-file={}", xml_path.display()))
    .arg(binary)
    .args(program_args)
    .env("C_TRACE_EVENT_FILE", event_file)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());
    if let Some(dir) = working_directory {
        cmd.current_dir(dir);
    }
    cmd.process_group(0);
    crate::resource_limits::apply_diagnostic_limits(&mut cmd, limits);

    let mut child = cmd.spawn().map_err(|e| EngineError::MissingTool {
        tool: "valgrind".into(),
        reason: format!("could not spawn valgrind: {e}"),
    })?;

    let mut stdout_pipe = child.stdout.take().expect("valgrind stdout");
    let mut stderr_pipe = child.stderr.take().expect("valgrind stderr");
    let out_cap = limits.max_output_bytes;

    let out_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stdout_pipe.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > out_cap * 2 {
                        buf.truncate(out_cap * 2);
                    }
                }
            }
        }
        buf
    });
    let err_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stderr_pipe.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > out_cap * 2 {
                        buf.truncate(out_cap * 2);
                    }
                }
            }
        }
        buf
    });

    let mut timed_out = false;
    let status: Option<std::process::ExitStatus> = match tokio::time::timeout(
        Duration::from_secs(limits.timeout_seconds.max(1) * 2 + 5),
        child.wait(),
    )
    .await
    {
        Ok(status) => status.ok(),
        Err(_) => {
            timed_out = true;
            if let Some(pid) = child.id() {
                crate::resource_limits::kill_process_group(pid);
            }
            let _ = child.wait().await;
            None
        }
    };

    let stdout_bytes = out_task.await.unwrap_or_default();
    let stderr_bytes = err_task.await.unwrap_or_default();

    let xml = std::fs::read_to_string(xml_path).unwrap_or_default();
    let report = if xml.contains("<valgrindoutput>") {
        parse_valgrind_xml(&xml).ok()
    } else {
        None
    };
    if let Some(r) = report.as_ref() {
        let mut r = clone_report(r);
        r.exit_code = status.as_ref().and_then(|s| s.code());
        r.timed_out = timed_out;
        return Ok(ValgrindRun {
            report: Some(r),
            exit_code: status.as_ref().and_then(|s| s.code()),
            timed_out,
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            xml_path: Some(xml_path.to_path_buf()),
        });
    }

    Ok(ValgrindRun {
        report: None,
        exit_code: status.as_ref().and_then(|s| s.code()),
        timed_out,
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        xml_path: Some(xml_path.to_path_buf()),
    })
}

fn clone_report(r: &ValgrindReport) -> ValgrindReport {
    r.clone()
}

/// Parse a Valgrind memcheck XML document.
pub fn parse_valgrind_xml(xml: &str) -> Result<ValgrindReport> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| EngineError::Valgrind(format!("XML parse error: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "valgrindoutput" {
        return Err(EngineError::Valgrind(
            "unexpected XML root (expected valgrindoutput)".into(),
        ));
    }

    let mut report = ValgrindReport::default();

    for child in root.children() {
        if !child.is_element() {
            continue;
        }
        match child.tag_name().name() {
            "heap_summary" => {
                if let Some(in_use) = child.children().find(|c| c.tag_name().name() == "memory_in_use_at_exit") {
                    report.bytes_in_use_at_exit = child_text_u64(in_use, "bytes");
                    report.blocks_in_use_at_exit = child_text_u64(in_use, "blocks");
                }
                if let Some(total) = child.children().find(|c| c.tag_name().name() == "total_heap_usage") {
                    report.total_allocs = child_text_u64(total, "allocs");
                    report.total_frees = child_text_u64(total, "frees");
                    report.bytes_allocated = child_text_u64(total, "bytes_allocated");
                }
            }
            "all_heap_blocks_freed" => {
                report.all_heap_blocks_freed = child.text().map(|t| t.trim() == "true");
            }
            "error" => {
                let err = parse_error(child);
                // Leak classification by kind.
                match err.kind.as_str() {
                    "Leak_DefinitelyLost" => report.definitely_lost_bytes += err.leaked_bytes,
                    "Leak_IndirectlyLost" => report.indirectly_lost_bytes += err.leaked_bytes,
                    "Leak_PossiblyLost" => report.possibly_lost_bytes += err.leaked_bytes,
                    "Leak_StillReachable" => report.still_reachable_bytes += err.leaked_bytes,
                    _ => {}
                }
                // Human-readable summary string with the first user site.
                let site = err
                    .stack
                    .iter()
                    .find(|f| f.file.is_some())
                    .map(|f| format!("{}:{}", f.file.clone().unwrap_or_default(), f.line.unwrap_or(0)))
                    .unwrap_or_default();
                report.errors.push(if site.is_empty() {
                    format!("{}: {}", err.kind, err.what)
                } else {
                    format!("{}: {} ({})", err.kind, err.what, site)
                });
                report.detailed_errors.push(err);
            }
            _ => {}
        }
    }
    Ok(report)
}

fn child_text_u64(node: roxmltree::Node, tag: &str) -> u64 {
    node.children()
        .find(|c| c.tag_name().name() == tag)
        .and_then(|c| c.text())
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0)
}

fn parse_error(node: roxmltree::Node) -> ValgrindError {
    let mut err = ValgrindError {
        unique: node
            .children()
            .find(|c| c.tag_name().name() == "unique")
            .and_then(|c| c.text())
            .map(|s| s.trim().to_string()),
        kind: node
            .children()
            .find(|c| c.tag_name().name() == "kind")
            .and_then(|c| c.text())
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        what: node
            .children()
            .find(|c| c.tag_name().name() == "what" || c.tag_name().name() == "xwhat")
            .map(|c| {
                // `xwhat` carries `<text>`; `what` carries plain text.
                c.children()
                    .find(|s| s.tag_name().name() == "text")
                    .and_then(|t| t.text())
                    .map(|s| s.trim().to_string())
                    .or_else(|| c.text().map(|s| s.trim().to_string()))
                    .unwrap_or_default()
            })
            .unwrap_or_default(),
        auxwhat: node
            .children()
            .find(|c| c.tag_name().name() == "auxwhat")
            .and_then(|c| c.text())
            .map(|s| s.trim().to_string()),
        stack: Vec::new(),
        aux_stacks: Vec::new(),
        leaked_bytes: 0,
        leaked_blocks: 0,
        related_allocation_ids: Vec::new(),
    };

    // Leak sizes live under <xwhat><leakedbytes>/<leakedblocks>.
    if let Some(xwhat) = node
        .children()
        .find(|c| c.tag_name().name() == "xwhat")
    {
        err.leaked_bytes = child_text_u64(xwhat, "leakedbytes");
        err.leaked_blocks = child_text_u64(xwhat, "leakedblocks");
    }

    let mut first_stack_done = false;
    for child in node.children() {
        if child.tag_name().name() == "stack" {
            let frames: Vec<ValgrindFrame> = child
                .children()
                .filter(|c| c.tag_name().name() == "frame")
                .map(parse_frame)
                .collect();
            if !first_stack_done {
                err.stack = frames;
                first_stack_done = true;
            } else {
                err.aux_stacks.push(frames);
            }
        }
    }
    err
}

fn parse_frame(node: roxmltree::Node) -> ValgrindFrame {
    let text = |tag: &str| -> Option<String> {
        node.children()
            .find(|c| c.tag_name().name() == tag)
            .and_then(|c| c.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    ValgrindFrame {
        ip: text("ip"),
        obj: text("obj"),
        func: text("fn"),
        dir: text("dir"),
        file: text("file"),
        line: text("line").and_then(|s| s.parse().ok()),
    }
}

/// Correlate Valgrind errors with allocation ids by matching (file, line)
/// sites from the allocation timeline against error stack frames.
///
/// This is deliberately conservative: only reliable (file, line) matches are
/// used; no fuzzy or address-based guessing.
pub fn correlate_with_allocations(
    report: &mut ValgrindReport,
    allocation_sites: &[(String, u32, u64)],
) {
    for err in &mut report.detailed_errors {
        let mut ids: Vec<u64> = Vec::new();
        let mut stacks: Vec<&ValgrindFrame> = err.stack.iter().collect();
        for aux in &err.aux_stacks {
            stacks.extend(aux.iter());
        }
        for frame in stacks {
            let Some(file) = frame.file.clone() else { continue };
            let Some(line) = frame.line else { continue };
            for (a_file, a_line, id) in allocation_sites {
                if *a_line == line && file_matches(&file, a_file) {
                    ids.push(*id);
                }
            }
        }
        ids.sort_unstable();
        ids.dedup();
        err.related_allocation_ids = ids;
    }
}

fn file_matches(xml_file: &str, alloc_file: &str) -> bool {
    if alloc_file.ends_with(xml_file) || xml_file == alloc_file {
        return true;
    }
    // Compare by basename as a last resort (valgrind may report only the
    // basename while the timeline resolved an absolute path).
    match (alloc_file.rsplit('/').next(), xml_file.rsplit('/').next()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const UAF_XML: &str = include_str!("../fixtures/valgrind/use_after_free.xml");
    const LEAK_XML: &str = include_str!("../fixtures/valgrind/leak.xml");

    #[test]
    fn parses_uaf_report() {
        let r = parse_valgrind_xml(UAF_XML).expect("parse");
        // crash_bin: one malloc(8) + one free.
        assert_eq!(r.total_allocs, 1);
        assert_eq!(r.total_frees, 1);
        assert_eq!(r.bytes_allocated, 8);
        assert_eq!(r.all_heap_blocks_freed, Some(true));
        assert!(r.errors.iter().any(|e| e.contains("InvalidWrite")));
        let write_err = r
            .detailed_errors
            .iter()
            .find(|e| e.kind == "InvalidWrite")
            .expect("invalid write error");
        assert!(write_err.what.contains("Invalid write of size 4"));
        assert!(write_err.auxwhat.as_deref().unwrap().contains("free'd"));
        // primary stack points at crash.c:5 (the UAF write)
        assert!(write_err.stack.iter().any(|f| f.file.as_deref() == Some("crash.c") && f.line == Some(5)));
        // aux stack records the free at crash.c:4
        assert!(write_err
            .aux_stacks
            .iter()
            .flatten()
            .any(|f| f.file.as_deref() == Some("crash.c") && f.line == Some(4)));
    }

    #[test]
    fn parses_leak_report() {
        let r = parse_valgrind_xml(LEAK_XML).expect("parse");
        // leaky_bin: malloc(40) + calloc(16), both lost.
        assert_eq!(r.total_allocs, 2);
        assert_eq!(r.total_frees, 0);
        assert_eq!(r.definitely_lost_bytes, 56, "40 + 16 definitely lost");
        assert!(r
            .detailed_errors
            .iter()
            .all(|e| e.kind == "Leak_DefinitelyLost"));
        let leak = r
            .detailed_errors
            .iter()
            .find(|e| e.leaked_bytes == 40)
            .expect("40-byte leak record");
        assert_eq!(leak.leaked_blocks, 1);
        assert!(leak.what.contains("definitely lost"));
        // leak stacks cite leaky.c:3
        assert!(leak.stack.iter().any(|f| f.file.as_deref() == Some("leaky.c") && f.line == Some(3)));
    }

    #[test]
    fn rejects_bad_xml() {
        assert!(parse_valgrind_xml("<not-valgrind/>").is_err());
        assert!(parse_valgrind_xml("<<broken").is_err());
        assert!(parse_valgrind_xml("").is_err());
    }

    #[test]
    fn correlation_matches_by_file_line() {
        let mut r = parse_valgrind_xml(LEAK_XML).unwrap();
        let sites = vec![("src/leaky.c".to_string(), 3u32, 7u64)];
        correlate_with_allocations(&mut r, &sites);
        let leak = r
            .detailed_errors
            .iter()
            .find(|e| e.leaked_bytes == 40)
            .unwrap();
        assert_eq!(leak.related_allocation_ids, vec![7]);
    }

    #[test]
    fn empty_summary_report() {
        let xml = r#"<valgrindoutput>
<protocolversion>4</protocolversion>
<protocoltool>memcheck</protocoltool>
<preamble><line>a</line></preamble>
<pid>1</pid><ppid>0</ppid><tool>memcheck</tool>
<args><vargv><exe>valgrind</exe></vargv><argv><exe>x</exe></argv></args>
<status><state>RUNNING</state><time>0</time></status>
<status><state>FINISHED</state><time>1</time></status>
<heap_summary>
<memory_in_use_at_exit><bytes>0</bytes><blocks>0</blocks></memory_in_use_at_exit>
<total_heap_usage><allocs>0</allocs><frees>0</frees><bytes_allocated>0</bytes_allocated></total_heap_usage>
</heap_summary>
<all_heap_blocks_freed>true</all_heap_blocks_freed>
<errorcounts></errorcounts><suppcounts></suppcounts>
<error_summary><errors>0</errors><error_contexts>0</error_contexts><suppressed>0</suppressed><suppressed_contexts>0</suppressed_contexts></error_summary>
</valgrindoutput>"#;
        let r = parse_valgrind_xml(xml).expect("parse");
        assert_eq!(r.total_allocs, 0);
        assert_eq!(r.errors.len(), 0);
        assert_eq!(r.all_heap_blocks_freed, Some(true));
    }
}

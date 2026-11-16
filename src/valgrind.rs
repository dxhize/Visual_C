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

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{EngineError, Result};
use crate::models::{ValgrindError, ValgrindFrame, ValgrindReport};

/// Run Valgrind memcheck against the debug binary.
///
/// Returns native observations and completeness. Tracking is passive so
/// instrumentation table roots cannot alter leak classification.
pub struct ValgrindRun {
    pub cancelled: bool,
    pub failure_reason: Option<String>,
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
    _event_file: &Path,
    valgrind_path: Option<&str>,
) -> Result<ValgrindRun> {
    // V2.1 (F-05): explicit `valgrind_path` overrides PATH resolution; the
    // default resolves via PATH with execvp semantics (non-executable PATH
    // matches are skipped, so shadowing with a non-executable file fails the
    // launch and is reported as a missing tool).
    let mut cmd = tokio::process::Command::new(valgrind_path.unwrap_or("valgrind"));
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
    // Do not retain allocator-table roots during independent leak analysis.
    .env_remove("C_TRACE_EVENT_FILE")
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());
    if let Some(dir) = working_directory {
        cmd.current_dir(dir);
    }
    cmd.process_group(0);
    crate::resource_limits::apply_diagnostic_limits(&mut cmd, limits);

    let output = crate::supervisor::output(
        &mut cmd,
        crate::supervisor::stage_deadline(limits.analysis_timeout_seconds),
        limits.max_output_bytes,
    )
    .await?;
    let stdout_bytes = output.stdout;
    let stderr_bytes = output.stderr;
    let status = output.status;
    let timed_out = output.timed_out;

    let mut xml = String::new();
    let cap = limits.max_trace_memory_bytes.min(16 * 1024 * 1024) as u64;
    let read =
        std::fs::File::open(xml_path).and_then(|file| file.take(cap).read_to_string(&mut xml));
    let mut failure_reason = read.err().map(|e| format!("cannot read Valgrind XML: {e}"));
    let report = if xml.contains("<valgrindoutput>") {
        match parse_valgrind_xml(&xml) {
            Ok(report) => Some(report),
            Err(e) => {
                failure_reason = Some(e.to_string());
                recover_errors(&xml).map(|mut r| {
                    r.parse_error = Some(e.to_string());
                    r
                })
            }
        }
    } else {
        if failure_reason.is_none() {
            failure_reason = Some("Valgrind XML root is missing".into());
        }
        None
    };
    if let Some(r) = report.as_ref() {
        let mut r = clone_report(r);
        r.exit_code = status.as_ref().and_then(|s| s.code());
        r.timed_out = timed_out;
        return Ok(ValgrindRun {
            cancelled: output.cancelled,
            failure_reason,
            report: Some(r),
            exit_code: status.as_ref().and_then(|s| s.code()),
            timed_out,
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            xml_path: Some(xml_path.to_path_buf()),
        });
    }

    Ok(ValgrindRun {
        cancelled: output.cancelled,
        failure_reason,
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

/// Recover only complete native error elements; never invent summary elements.
pub fn recover_errors(xml: &str) -> Option<ValgrindReport> {
    let mut report = ValgrindReport {
        parse_error: Some("truncated XML: only complete native error records recovered".into()),
        ..Default::default()
    };
    let mut rest = xml;
    while let Some(start) = rest.find("<error>") {
        rest = &rest[start..];
        let Some(end) = rest.find("</error>") else {
            break;
        };
        let fragment = &rest[..end + "</error>".len()];
        if let Ok(doc) = roxmltree::Document::parse(fragment) {
            let error = parse_error(doc.root_element());
            report
                .errors
                .push(format!("{}: {}", error.kind, error.what));
            report.detailed_errors.push(error);
        }
        rest = &rest[end + "</error>".len()..];
    }
    if report.detailed_errors.is_empty() {
        None
    } else {
        Some(report)
    }
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

    let mut report = ValgrindReport {
        output_complete: root
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "status")
            .flat_map(|n| n.children())
            .any(|n| {
                n.is_element()
                    && n.tag_name().name() == "state"
                    && n.text().is_some_and(|t| t.trim() == "FINISHED")
            }),
        ..Default::default()
    };
    if !report.output_complete {
        report.parse_error = Some("Valgrind XML lacks a FINISHED status".into());
    }

    for child in root.children() {
        if !child.is_element() {
            continue;
        }
        match child.tag_name().name() {
            "heap_summary" => {
                report.summary_available = true;
                if let Some(in_use) = child
                    .children()
                    .find(|c| c.tag_name().name() == "memory_in_use_at_exit")
                {
                    let summary = report.observed_summary.get_or_insert_with(Default::default);
                    summary.bytes_in_use_at_exit = child_u64(in_use, "bytes");
                    summary.blocks_in_use_at_exit = child_u64(in_use, "blocks");
                    report.bytes_in_use_at_exit = summary.bytes_in_use_at_exit.unwrap_or(0);
                    report.blocks_in_use_at_exit = summary.blocks_in_use_at_exit.unwrap_or(0);
                }
                if let Some(total) = child
                    .children()
                    .find(|c| c.tag_name().name() == "total_heap_usage")
                {
                    let summary = report.observed_summary.get_or_insert_with(Default::default);
                    summary.total_allocs = child_u64(total, "allocs");
                    summary.total_frees = child_u64(total, "frees");
                    summary.bytes_allocated = child_u64(total, "bytes_allocated");
                    report.total_allocs = summary.total_allocs.unwrap_or(0);
                    report.total_frees = summary.total_frees.unwrap_or(0);
                    report.bytes_allocated = summary.bytes_allocated.unwrap_or(0);
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
                    .map(|f| {
                        format!(
                            "{}:{}",
                            f.file.clone().unwrap_or_default(),
                            f.line.unwrap_or(0)
                        )
                    })
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

fn child_u64(node: roxmltree::Node, tag: &str) -> Option<u64> {
    node.children()
        .find(|c| c.tag_name().name() == tag)
        .and_then(|c| c.text())
        .and_then(parse_native_u64)
}

fn parse_native_u64(text: &str) -> Option<u64> {
    let text = text.trim();
    if !text.contains(',') {
        return text.parse().ok();
    }
    // Valgrind 3.27 emits grouped counters even in XML (e.g. 4,152).
    let mut groups = text.split(',');
    let first = groups.next()?;
    if first.is_empty() || first.len() > 3 || !first.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if !groups.all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    text.replace(',', "").parse().ok()
}

fn parse_error(node: roxmltree::Node) -> ValgrindError {
    let mut err = ValgrindError {
        observed_leaked_bytes: None,
        observed_leaked_blocks: None,
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
    if let Some(xwhat) = node.children().find(|c| c.tag_name().name() == "xwhat") {
        err.observed_leaked_bytes = child_u64(xwhat, "leakedbytes");
        err.observed_leaked_blocks = child_u64(xwhat, "leakedblocks");
        err.leaked_bytes = err.observed_leaked_bytes.unwrap_or(0);
        err.leaked_blocks = err.observed_leaked_blocks.unwrap_or(0);
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
        line: text("line")
            .and_then(|s| parse_native_u64(&s))
            .and_then(|n| n.try_into().ok()),
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
            let Some(file) = frame.file.clone() else {
                continue;
            };
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

    #[test]
    fn grouped_native_counters_are_not_zero_and_missing_fields_remain_unknown() {
        assert_eq!(parse_native_u64("4,152"), Some(4152));
        assert_eq!(parse_native_u64("1,000,000"), Some(1_000_000));
        assert_eq!(parse_native_u64("1,23"), None);
        assert_eq!(parse_native_u64("unknown42"), None);
        let xml = r#"<valgrindoutput><status><state>FINISHED</state></status>
            <heap_summary><total_heap_usage><allocs>4</allocs><bytes_allocated>4,152</bytes_allocated></total_heap_usage></heap_summary>
            </valgrindoutput>"#;
        let report = parse_valgrind_xml(xml).unwrap();
        assert_eq!(report.bytes_allocated, 4152);
        let observed = report.observed_summary.unwrap();
        assert_eq!(observed.bytes_allocated, Some(4152));
        assert_eq!(observed.total_frees, None);
    }

    #[test]
    fn truncated_xml_preserves_only_complete_native_errors() {
        let xml = "<valgrindoutput><error><kind>InvalidWrite</kind><what>Invalid write of size 4</what></error><error><kind>Leak_";
        assert!(parse_valgrind_xml(xml).is_err());
        let report = recover_errors(xml).unwrap();
        assert_eq!(report.detailed_errors.len(), 1);
        assert_eq!(report.detailed_errors[0].kind, "InvalidWrite");
        assert!(!report.output_complete);
        assert!(!report.summary_available);
        assert!(recover_errors("<valgrindoutput><error>").is_none());
    }

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
        assert!(write_err
            .stack
            .iter()
            .any(|f| f.file.as_deref() == Some("crash.c") && f.line == Some(5)));
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
        assert!(leak
            .stack
            .iter()
            .any(|f| f.file.as_deref() == Some("leaky.c") && f.line == Some(3)));
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

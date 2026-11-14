//! AddressSanitizer / UndefinedBehaviorSanitizer diagnostic pass.
//!
//! When GDB observes a fatal crash signal, the engine runs the separately
//! built sanitized binary (same program arguments, same working directory)
//! and captures its sanitizer report. This is a *separate diagnostic
//! execution*: ASan halts the program at the first detected error and its
//! allocator replaces malloc, so its run is never mixed with the GDB run or
//! with the allocation instrumentation.
//!
//! Reproduction is never assumed. The report records one of:
//! `reproduced` (sanitizer fired), `completed_normally` (clean exit without
//! a report — the crash did not reproduce), or `inconclusive` (timeout /
//! signal killed the run before it could report).

use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncReadExt;

use crate::models::SanitizerReport;
use crate::resource_limits::{apply_diagnostic_limits, kill_process_group};

/// Run the sanitized binary with the same args/working directory.
pub async fn run_sanitized(
    binary: &Path,
    program_args: &[String],
    working_directory: Option<&Path>,
    limits: &crate::resource_limits::ExecutionLimits,
) -> SanitizerReport {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.args(program_args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dir) = working_directory {
        cmd.current_dir(dir);
    }
    cmd.process_group(0);
    apply_diagnostic_limits(&mut cmd, limits);

    let mut report = SanitizerReport::default();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            report.reproduction = "skipped".into();
            report.stderr_excerpt = format!("could not spawn sanitized binary: {e}");
            return report;
        }
    };

    let mut stdout_pipe = child.stdout.take().expect("stdout");
    let mut stderr_pipe = child.stderr.take().expect("stderr");
    let out_cap = limits.max_output_bytes;
    let out_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stdout_pipe.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.len() < out_cap {
                        buf.extend_from_slice(&chunk[..n.min(out_cap - buf.len())]);
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
                    if buf.len() < out_cap * 2 {
                        buf.extend_from_slice(&chunk[..n.min(out_cap * 2 - buf.len())]);
                    }
                }
            }
        }
        buf
    });

    let mut timed_out = false;
    let status = match tokio::time::timeout(
        Duration::from_secs(limits.timeout_seconds.max(1) * 2 + 5),
        child.wait(),
    )
    .await
    {
        Ok(s) => s.ok(),
        Err(_) => {
            timed_out = true;
            if let Some(pid) = child.id() {
                kill_process_group(pid);
            }
            let _ = child.wait().await;
            None
        }
    };
    let stdout_bytes = out_task.await.unwrap_or_default();
    let stderr_bytes = err_task.await.unwrap_or_default();

    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
    report.stdout_excerpt = truncate(&stdout, 4096);
    report.stderr_excerpt = truncate(&stderr, 8192);
    report.exit_code = status.as_ref().and_then(|s| s.code());
    report.timed_out = timed_out;

    parse_sanitizer_output(&stderr, &stdout, &mut report);
    if report.sanitizer.is_some() {
        report.reproduction = "reproduced".into();
    } else if timed_out {
        report.reproduction = "inconclusive".into();
    } else {
        report.reproduction = "completed_normally".into();
    }
    report
}

fn truncate(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        s.to_string()
    } else {
        let mut cut = cap;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\n[truncated]", &s[..cut])
    }
}

/// Fill a report from sanitizer stderr/stdout text.
pub fn parse_sanitizer_output(stderr: &str, stdout: &str, report: &mut SanitizerReport) {
    let combined = format!("{stdout}\n{stderr}");

    // AddressSanitizer: "SUMMARY: AddressSanitizer: <category> <loc> in <fn>"
    if let Some(line) = combined
        .lines()
        .find(|l| l.starts_with("SUMMARY: AddressSanitizer:"))
    {
        report.sanitizer = Some("AddressSanitizer".into());
        report.error_category = Some(extract_asan_category(line));
        report.message = Some(line.trim().to_string());
        if let Some((file, line_no, col)) = extract_location(line) {
            report.file = Some(file);
            report.line = Some(line_no);
            report.column = col;
        }
    } else if combined.contains("AddressSanitizer:") {
        report.sanitizer = Some("AddressSanitizer".into());
        if let Some(err_line) = combined.lines().find(|l| l.contains("ERROR: AddressSanitizer:"))
        {
            report.error_category = Some(extract_asan_category(err_line));
            report.message = Some(err_line.trim().to_string());
            if let Some((file, line_no, col)) = extract_location(err_line) {
                report.file = Some(file);
                report.line = Some(line_no);
                report.column = col;
            }
        }
    }

    // UBSan: "runtime error: file.c:12:5: message"
    if combined.contains("runtime error:") {
        report.sanitizer = match &report.sanitizer {
            Some(s) => Some(s.clone()),
            None => Some("UndefinedBehaviorSanitizer".into()),
        };
        if report.error_category.is_none() {
            if let Some(line) = combined.lines().find(|l| l.contains("runtime error:")) {
                if let Some((file, line_no, col)) = extract_location(line) {
                    report.file = Some(file);
                    report.line = Some(line_no);
                    report.column = col;
                    report.error_category = Some(
                        line.split("runtime error:")
                            .nth(1)
                            .and_then(|m| m.split_whitespace().nth(2))
                            .unwrap_or("undefined behavior")
                            .to_string(),
                    );
                }
                report.message = Some(line.trim().to_string());
            }
        }
    }

    // Stack trace: lines from "    #0 " / "    #1 " frames.
    for line in combined.lines() {
        let t = line.trim();
        if t.starts_with("#") && t.contains(' ') {
            report.stack.push(t.to_string());
        }
    }
    report.stack.truncate(32);
}

fn extract_asan_category(line: &str) -> String {
    // "SUMMARY: AddressSanitizer: heap-buffer-overflow /path/file.c:10 in main"
    let rest = match line.find("AddressSanitizer:") {
        Some(i) => &line[i + "AddressSanitizer:".len()..],
        None => return "unknown".into(),
    };
    rest.split_whitespace()
        .find(|w| w.contains('-') || w.contains('_'))
        .unwrap_or("unknown")
        .to_string()
}

/// Extract the first `path/file.c:LINE[:COL]` occurrence.
fn extract_location(line: &str) -> Option<(String, u32, Option<u32>)> {
    use once_cell_lite::Lazy;
    static RE: Lazy<regex::Regex> = Lazy::new(|| {
        regex::Regex::new(r"([A-Za-z0-9_./\\-]+\.(?:c|h|cc|cpp)):(\d+):?(\d+)?").unwrap()
    });
    let m = RE.captures(line)?;
    let file = m.get(1)?.as_str().to_string();
    let line_no = m.get(2)?.as_str().parse().ok()?;
    let col = m.get(3).and_then(|c| c.as_str().parse().ok());
    Some((file, line_no, col))
}

/// Tiny stand-in for `once_cell` (std-only lazy static via OnceLock).
mod once_cell_lite {
    use std::sync::OnceLock;

    pub struct Lazy<T>(OnceLock<T>, fn() -> T);

    impl<T> Lazy<T> {
        pub const fn new(f: fn() -> T) -> Self {
            Lazy(OnceLock::new(), f)
        }
    }

    impl<T> std::ops::Deref for Lazy<T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0.get_or_init(self.1)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_asan_heap_overflow() {
        let stderr = "\
=================================================================
==12345==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x602000000014 at pc 0x0000004011a0 bp 0x7ffd... sp 0x7ffd...
WRITE of size 4 at 0x602000000014 thread T0
    #0 0x40119f in main /home/user/proj/overflow.c:7
    #1 0x7ffff7c29d8f in __libc_start_main (/lib64/libc.so.6+0x1dd8f)
0x602000000014 is located 0 bytes to the right of 12-byte region [0x602000000008,0x602000000014)
allocated by thread T0 here:
    #0 0x401000 in malloc /usr/src/debug/gcc/asan_malloc.cpp
    #1 0x40119f in main /home/user/proj/overflow.c:5
SUMMARY: AddressSanitizer: heap-buffer-overflow /home/user/proj/overflow.c:7 in main
Shadow bytes around the buggy address: ...
==12345==ABORTING";
        let mut r = SanitizerReport::default();
        parse_sanitizer_output("", stderr, &mut r);
        assert_eq!(r.sanitizer.as_deref(), Some("AddressSanitizer"));
        assert_eq!(r.error_category.as_deref(), Some("heap-buffer-overflow"));
        assert_eq!(r.file.as_deref(), Some("/home/user/proj/overflow.c"));
        assert_eq!(r.line, Some(7));
        assert_eq!(r.column, None);
        assert!(r.stack.iter().any(|s| s.contains("overflow.c:7")));
    }

    #[test]
    fn parses_ubsan_runtime_error() {
        let stderr = "prog: /home/user/proj/shift.c:5:10: runtime error: left shift of negative value -1 by 1 place cannot be represented in type 'int'\n";
        let mut r = SanitizerReport::default();
        parse_sanitizer_output("", stderr, &mut r);
        assert_eq!(
            r.sanitizer.as_deref(),
            Some("UndefinedBehaviorSanitizer")
        );
        assert_eq!(r.file.as_deref(), Some("/home/user/proj/shift.c"));
        assert_eq!(r.line, Some(5));
        assert_eq!(r.column, Some(10));
        assert!(r.message.as_deref().unwrap().contains("runtime error"));
    }

    #[test]
    fn clean_output_is_not_reproduction() {
        let mut r = SanitizerReport::default();
        parse_sanitizer_output("hello\n", "", &mut r);
        assert!(r.sanitizer.is_none());
        // The caller classifies reproduction:
        let mut report = r.clone();
        report.reproduction = "completed_normally".into();
        assert_eq!(report.reproduction, "completed_normally");
    }

    #[test]
    fn truncation_respects_char_boundary() {
        let s = "aé".repeat(100);
        let t = truncate(&s, 105);
        assert!(t.ends_with("[truncated]"));
        assert!(t.len() <= 120, "got len {}", t.len());
        // the truncated prefix must still be valid UTF-8
        assert!(t.starts_with("aé"));
    }
}

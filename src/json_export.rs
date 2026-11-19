//! Bounded final JSON export, independent of the JSONL journal budget.
//! Preflight writes to a counting sink; no full-size JSON buffer is allocated.
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct BoundedWriter<'a, W> {
    inner: W,
    count: usize,
    limit: usize,
    cancel: Option<&'a AtomicBool>,
}

fn check_cancel(cancel: Option<&AtomicBool>) -> io::Result<()> {
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        // `write_all` retries Interrupted indefinitely. Use Other so
        // serde_json propagates cooperative cancellation immediately.
        Err(io::Error::other("JSON export cancelled"))
    } else {
        Ok(())
    }
}

impl<W: Write> Write for BoundedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        check_cancel(self.cancel)?;
        if bytes.len() > self.limit - self.count {
            return Err(io::Error::other(format!(
                "max_json_export_bytes exceeded (limit {} bytes)",
                self.limit
            )));
        }
        let written = self.inner.write(bytes)?;
        self.count += written;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        check_cancel(self.cancel)?;
        self.inner.flush()?;
        check_cancel(self.cancel)
    }
}

fn serialize_limited<T: Serialize, W: Write>(
    trace: &T,
    pretty: bool,
    limit: usize,
    output: W,
    cancel: Option<&AtomicBool>,
) -> io::Result<usize> {
    check_cancel(cancel)?;
    let mut bounded = BoundedWriter {
        inner: output,
        count: 0,
        limit,
        cancel,
    };
    let map_json_error =
        |e: serde_json::Error| io::Error::new(e.io_error_kind().unwrap_or(io::ErrorKind::Other), e);
    if pretty {
        serde_json::to_writer_pretty(&mut bounded, trace).map_err(map_json_error)?;
    } else {
        serde_json::to_writer(&mut bounded, trace).map_err(map_json_error)?;
    }
    bounded.write_all(b"\n")?;
    bounded.flush()?;
    Ok(bounded.count)
}

/// Count the actual serialized UTF-8 bytes, including the terminal newline.
/// Exceeding the budget fails before either output destination is touched.
pub fn preflight<T: Serialize>(trace: &T, pretty: bool, limit: usize) -> io::Result<usize> {
    serialize_limited(trace, pretty, limit, io::sink(), None)
}

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempPath(PathBuf);
impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn create_sibling(path: &Path) -> io::Result<(TempPath, File)> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output path has no file name")
    })?;
    for _ in 0..16 {
        let mut basename = std::ffi::OsString::from(".");
        basename.push(name);
        basename.push(format!(
            ".cte-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = parent.join(basename);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((TempPath(candidate), file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate unique output temp file",
    ))
}

fn write_file_atomic<T: Serialize>(
    trace: &T,
    pretty: bool,
    limit: usize,
    path: &Path,
    cancel: Option<&AtomicBool>,
) -> io::Result<()> {
    let (temp, mut file) = create_sibling(path)?;
    {
        let mut buffered = BufWriter::new(&mut file);
        serialize_limited(trace, pretty, limit, &mut buffered, cancel)?;
        buffered.flush()?;
    }
    file.sync_all()?;
    check_cancel(cancel)?;
    std::fs::rename(&temp.0, path)?;
    Ok(())
}

/// Preflight both destinations, atomically commit an optional sibling file,
/// then write stdout. An I/O failure *during* stdout cannot be rolled back;
/// the CLI returns a nonzero exit code and never reports export success.
pub fn export_json<T: Serialize, W: Write>(
    trace: &T,
    pretty: bool,
    limit: usize,
    file: Option<&Path>,
    stdout: &mut W,
) -> io::Result<usize> {
    export_json_cancellable(trace, pretty, limit, file, stdout, None)
}

/// Like `export_json`, but checks a cooperative cancellation flag during
/// preflight, each output write and before committing the file.
pub fn export_json_cancellable<T: Serialize, W: Write>(
    trace: &T,
    pretty: bool,
    limit: usize,
    file: Option<&Path>,
    stdout: &mut W,
    cancel: Option<&AtomicBool>,
) -> io::Result<usize> {
    let bytes = serialize_limited(trace, pretty, limit, io::sink(), cancel)?;
    if let Some(path) = file {
        write_file_atomic(trace, pretty, limit, path, cancel)?;
    }
    serialize_limited(trace, pretty, limit, stdout, cancel)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::Error as _;
    use std::cell::Cell;

    fn dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "cte-f13-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        p
    }

    #[test]
    fn exact_limit_includes_newline_and_pretty_whitespace() {
        for pretty in [false, true] {
            let evidence = serde_json::json!({"dwarf_registry": ["symbol"], "ast_registry": ["é"], "snapshots": [1, 2]});
            // Independent standard serde_json byte count, plus our newline.
            let expected = if pretty {
                serde_json::to_vec_pretty(&evidence).unwrap().len() + 1
            } else {
                serde_json::to_vec(&evidence).unwrap().len() + 1
            };
            let mut stdout = Vec::new();
            assert_eq!(
                export_json(&evidence, pretty, expected, None, &mut stdout).unwrap(),
                expected
            );
            assert_eq!(stdout.len(), expected);
            assert_eq!(stdout.last(), Some(&b'\n'));
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&stdout).unwrap(),
                evidence
            );
            stdout.clear();
            assert!(
                export_json(&evidence, pretty, expected - 1, None, &mut stdout)
                    .unwrap_err()
                    .to_string()
                    .contains("max_json_export_bytes")
            );
            assert!(
                stdout.is_empty(),
                "preflight must prevent partial stdout on limit rejection"
            );
        }
    }

    #[test]
    fn large_registries_and_legacy_trace_preserve_all_evidence() {
        let file = include_bytes!("../tests/fixtures/stage1/realloc_move.json");
        let trace: crate::models::DebugSessionTrace = serde_json::from_slice(file).unwrap();
        let dir = dir();
        let output = dir.join("trace.json");
        let expected = serde_json::to_vec_pretty(&trace).unwrap();
        let mut stdout = Vec::new();
        export_json(&trace, true, expected.len() + 1, Some(&output), &mut stdout).unwrap();
        assert_eq!(&stdout[..expected.len()], expected);
        assert_eq!(std::fs::read(&output).unwrap(), stdout);
        let decoded: crate::models::DebugSessionTrace = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(
            serde_json::to_value(&decoded).unwrap(),
            serde_json::to_value(&trace).unwrap()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn one_byte_over_preserves_existing_file_and_leaves_no_temp() {
        let dir = dir();
        let path = dir.join("existing.json");
        std::fs::write(&path, b"{\"old\":true}\n").unwrap();
        let evidence = serde_json::json!({"registry": "é", "x": [1, 2]});
        let exact = serde_json::to_vec(&evidence).unwrap().len() + 1;
        let mut stdout = Vec::new();
        assert!(
            export_json(&evidence, false, exact - 1, Some(&path), &mut stdout)
                .unwrap_err()
                .to_string()
                .contains("max_json_export_bytes")
        );
        assert!(stdout.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"old\":true}\n");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        export_json(&evidence, false, exact, Some(&path), &mut stdout).unwrap();
        assert_eq!(stdout.len(), exact);
        assert_eq!(std::fs::read(&path).unwrap(), stdout);
        std::fs::remove_dir_all(dir).unwrap();
    }

    struct FailOnCall {
        calls: Cell<usize>,
        failure: usize,
    }
    impl Serialize for FailOnCall {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let call = self.calls.get() + 1;
            self.calls.set(call);
            if call == self.failure {
                return Err(S::Error::custom("injected serializer error"));
            }
            serializer.serialize_str("valid")
        }
    }

    #[test]
    fn failing_serialization_cleans_temp_and_preserves_existing_file() {
        let dir = dir();
        let path = dir.join("valid.json");
        std::fs::write(&path, b"old valid json").unwrap();
        for call in [1, 2] {
            // preflight failure, then failure with temp file open
            let value = FailOnCall {
                calls: Cell::new(0),
                failure: call,
            };
            let mut stdout = Vec::new();
            assert!(export_json(&value, false, 1024, Some(&path), &mut stdout)
                .unwrap_err()
                .to_string()
                .contains("injected serializer error"));
            assert!(stdout.is_empty());
            assert_eq!(std::fs::read(&path).unwrap(), b"old valid json");
            assert_eq!(
                std::fs::read_dir(&dir).unwrap().count(),
                1,
                "no leaked sibling temp files"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    struct RejectWrites;
    impl Write for RejectWrites {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn stdout_write_failure_is_explicit_and_file_commit_is_valid() {
        let dir = dir();
        let path = dir.join("output.json");
        let err = export_json(
            &serde_json::json!({"valid": true}),
            false,
            1024,
            Some(&path),
            &mut RejectWrites,
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap()
                ["valid"],
            true
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn simulated_disk_full_write_error_is_not_a_success() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from_raw_os_error(libc::ENOSPC))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let err = export_json(&[1, 2, 3], false, 1024, None, &mut Full).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::StorageFull);
        assert!(err.to_string().contains("28"));
    }

    #[test]
    fn cancellation_before_file_commit_cleans_temp() {
        let dir = dir();
        let path = dir.join("existing.json");
        std::fs::write(&path, b"previous").unwrap();
        let cancelled = AtomicBool::new(false);
        let value = CancelDuringFile {
            calls: Cell::new(0),
            cancelled: &cancelled,
        };
        let mut stdout = Vec::new();
        let err = export_json_cancellable(
            &value,
            false,
            1024,
            Some(&path),
            &mut stdout,
            Some(&cancelled),
        )
        .unwrap_err();
        assert!(err.to_string().contains("JSON export cancelled"));
        assert!(stdout.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    struct CancelDuringFile<'a> {
        calls: Cell<usize>,
        cancelled: &'a AtomicBool,
    }
    impl Serialize for CancelDuringFile<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let calls = self.calls.get() + 1;
            self.calls.set(calls);
            if calls == 2 {
                self.cancelled.store(true, Ordering::Relaxed);
            }
            serializer.serialize_str("ok")
        }
    }

    #[test]
    fn unwritable_file_destination_does_not_emit_stdout() {
        let dir = dir();
        let invalid = dir.join("not-a-directory");
        std::fs::write(&invalid, b"unchanged").unwrap();
        let mut stdout = Vec::new();
        assert!(export_json(
            &"hello",
            false,
            1024,
            Some(&invalid.join("output.json")),
            &mut stdout
        )
        .is_err());
        assert!(stdout.is_empty());
        assert_eq!(std::fs::read(&invalid).unwrap(), b"unchanged");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

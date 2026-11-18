//! JSON trace document ingestion and input-format detection.
//!
//! The parser deliberately does not re-serialize, normalize, or "repair" the
//! trace: `DebugSessionTrace` is deserialized directly from the input bytes,
//! so what Stage 1 recorded is exactly what the [`TraceStore`](super::TraceStore)
//! exposes. No event renumbering, no address rewriting, no invented defaults.

use crate::models::DebugSessionTrace;

use super::error::{Result, TraceParseError};

/// The one schema version this engine family currently emits and accepts.
/// The serialized marker is the integer `2` — V2 and V2.1 share it.
pub(crate) const SUPPORTED_SCHEMA_VERSION: u32 = 2;

/// Human-readable summary used in error messages.
pub(crate) const SUPPORTED_SCHEMA_VERSION_LABEL: &str = "2 (V2 / V2.1 traces)";

/// Deserialize one complete JSON trace document, then gate its schema version.
pub(crate) fn decode_document(text: &str) -> Result<DebugSessionTrace> {
    let trace: DebugSessionTrace =
        serde_json::from_str(text).map_err(|e| TraceParseError::MalformedJson {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        })?;
    validate_schema_version(&trace)?;
    Ok(trace)
}

/// Schema-version gate.
///
/// A *missing* `schema_version` field is not an error: `DebugSessionTrace`
/// declares `#[serde(default = ...)]` returning 2, which is the engine's
/// documented compatibility contract for legacy V2 traces (they omit the
/// field and still parse). Anything other than 2 is rejected explicitly —
/// an incompatible version is never silently interpreted as the current one.
pub(crate) fn validate_schema_version(trace: &DebugSessionTrace) -> Result<()> {
    if trace.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(TraceParseError::UnsupportedSchemaVersion {
            found: trace.schema_version,
            supported: SUPPORTED_SCHEMA_VERSION_LABEL,
        });
    }
    Ok(())
}

/// The input kind decided by content inspection (never by file name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetectedFormat {
    /// One complete JSON document — the trace file Stage 1 prints to stdout
    /// and writes to `--output`.
    JsonDocument,
    /// Line-oriented `{"kind":...,"data":...}` JSONL session journal.
    Journal,
}

/// Decide whether `text` is a JSON trace document or a JSONL journal.
///
/// Policy (documented in `docs/trace-parser.md`):
/// * journal-shaped means every complete line parses as a JSON object
///   carrying a string `kind` member and a `data` member;
/// * a journal may end with ONE torn (incomplete) final line — writes are
///   append-only, so a crashed session leaves exactly that — but only when
///   at least one valid record precedes it. Malformed JSON is therefore
///   never reinterpreted as a journal;
/// * anything else is attempted as a JSON document (and a failure there is
///   reported as `MalformedJson`, the honest diagnosis).
pub(crate) fn detect_format(text: &str) -> DetectedFormat {
    let mut total = 0usize;
    let mut valid_records = 0usize;
    let mut bad_first: Option<usize> = None;
    let mut bad_last: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        total += 1;
        if journal_record_shape(trimmed) {
            valid_records += 1;
        } else {
            if bad_first.is_none() {
                bad_first = Some(total);
            }
            bad_last = Some(total);
        }
    }

    // A bad line is tolerable only as the single torn final line, and only
    // when at least one valid record precedes it. A bad line mid-file, two
    // bad lines at the end, or a bad line with no valid record before it
    // (truncated compact or pretty JSON) is treated as a JSON document
    // attempt — malformed JSON is never reinterpreted as a journal.
    if let (Some(first), Some(last)) = (bad_first, bad_last) {
        if first != last || last != total || valid_records == 0 {
            return DetectedFormat::JsonDocument;
        }
    }
    if valid_records >= 1 {
        DetectedFormat::Journal
    } else {
        DetectedFormat::JsonDocument
    }
}

/// Cheap shape check for one journal line. No typed decoding happens here;
/// the journal parser re-validates every line with full error context.
fn journal_record_shape(line: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return false;
    };
    let Some(obj) = value.as_object() else {
        return false;
    };
    obj.get("kind").is_some_and(|k| k.is_string()) && obj.contains_key("data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_pretty_json_document() {
        let text = "{\n  \"compiled_successfully\": true\n}";
        assert_eq!(detect_format(text), DetectedFormat::JsonDocument);
    }

    #[test]
    fn detects_compact_json_document() {
        let text = r#"{"compiled_successfully":true,"snapshots":[]}"#;
        assert_eq!(detect_format(text), DetectedFormat::JsonDocument);
    }

    #[test]
    fn detects_journal_with_trailing_newline() {
        let text = "{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"gap\",\"data\":{}}\n";
        assert_eq!(detect_format(text), DetectedFormat::Journal);
    }

    #[test]
    fn detects_journal_with_one_torn_tail() {
        let text = "{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"gap\",\"da";
        assert_eq!(detect_format(text), DetectedFormat::Journal);
    }

    #[test]
    fn malformed_json_is_never_a_journal() {
        // Truncated compact JSON: one bad line, no valid record before it.
        assert_eq!(
            detect_format("{\"execution_li"),
            DetectedFormat::JsonDocument
        );
        // Truncated pretty JSON: several bad lines, no valid record.
        assert_eq!(
            detect_format("{\n  \"snapshots\": [\n    {\n      \"step_index\": 0,\n"),
            DetectedFormat::JsonDocument
        );
        // Garbage final line without a valid record before it.
        assert_eq!(
            detect_format("{\"kind\":\"checkpoint\"}\nnot json"),
            DetectedFormat::JsonDocument
        );
        // A bad line mid-file disqualifies the journal reading even when a
        // valid record precedes it.
        assert_eq!(
            detect_format("{\"kind\":\"configuration\",\"data\":{}}\nnot json\n{\"kind\":\"gap\",\"data\":{}}\n"),
            DetectedFormat::JsonDocument
        );
        // Two bad lines at the end: the first is not the final line.
        assert_eq!(
            detect_format("{\"kind\":\"configuration\",\"data\":{}}\n{broken\n{broken2"),
            DetectedFormat::JsonDocument
        );
    }

    #[test]
    fn bad_line_after_records_is_journal_with_torn_tail() {
        // A single-record journal whose torn tail is garbage bytes.
        assert_eq!(
            detect_format("{\"kind\":\"configuration\",\"data\":{}}\n{broken"),
            DetectedFormat::Journal
        );
    }
}

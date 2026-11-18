//! Stage 1 JSONL session journal: validated decoding (no reconstruction).
//!
//! Writer side: `crate::storage::Journal` appends one self-contained JSON
//! object per line — `{"kind":"<record>","data":<value>}` — with these
//! record kinds:
//!
//! | kind            | payload                                        |
//! |-----------------|------------------------------------------------|
//! | `configuration` | `config::TracingConfig` (always the first record) |
//! | `checkpoint`    | `models::ExecutionSnapshot`                     |
//! | `allocation`    | `{"observed_at_stop": u64, "event": HeapEvent}` |
//! | `gap`           | `models::TraceGap`                              |
//! | `disassembly`   | `models::DisassembledInstruction`               |
//! | `instruction`   | `models::InstructionEvent`                      |
//! | `finalized`     | `true` (terminal control record)                |
//!
//! Writes are append-only and unbuffered, so a session that dies mid-write
//! leaves a torn final line.
//!
//! Reader policy for TP-1 (documented in `docs/trace-parser.md`):
//! * every complete line must decode to a known record kind — anything else
//!   is a typed error (no silent skipping, unlike `storage::recover`);
//! * a final line *without a trailing newline* is a suspected torn write: it
//!   is accepted only when it fully decodes, otherwise it is reported via
//!   [`JournalTrace::torn_tail`] and its bytes are never interpreted;
//! * the first record of a non-empty journal must be `configuration`;
//! * full `DebugSessionTrace` reconstruction is impossible — the finalized
//!   fields Stage 1 computes at session end are never journaled — so this
//!   module yields the validated record stream, and nothing pretends it is
//!   a complete trace.

use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, Read};

use crate::config::TracingConfig;
use crate::models::{
    DisassembledInstruction, ExecutionSnapshot, HeapEvent, InstructionEvent, TraceGap,
};

use super::error::{Result, TraceParseError};
use super::TraceLoadOptions;

/// One decoded journal record, preserving file order.
#[derive(Debug, Clone)]
pub enum JournalRecord {
    /// Session configuration (`TracingConfig`); always the first record.
    Configuration(TracingConfig),
    /// One source-level stop.
    Checkpoint(ExecutionSnapshot),
    /// One heap event, with the stop it was observed at.
    Allocation {
        observed_at_stop: u64,
        event: HeapEvent,
    },
    /// One recorded capture gap.
    Gap(TraceGap),
    /// One disassembly cache entry.
    Disassembly(DisassembledInstruction),
    /// One executed-instruction event.
    Instruction(InstructionEvent),
    /// Terminal control record written by `Journal::finalize`.
    Finalized,
    /// Unknown kind, accepted only when
    /// `TraceLoadOptions::allow_unknown_journal_records` is set. The data is
    /// preserved verbatim; the parser asserts nothing about its contents.
    Unknown { kind: String, data: Value },
}

impl JournalRecord {
    /// The record's `kind` string, for error messages.
    pub fn kind(&self) -> &str {
        match self {
            Self::Configuration(_) => "configuration",
            Self::Checkpoint(_) => "checkpoint",
            Self::Allocation { .. } => "allocation",
            Self::Gap(_) => "gap",
            Self::Disassembly(_) => "disassembly",
            Self::Instruction(_) => "instruction",
            Self::Finalized => "finalized",
            Self::Unknown { kind, .. } => kind,
        }
    }
}

/// Per-kind record counts of a parsed journal.
///
/// Sums to `records.len()` (including `finalized`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JournalCounts {
    pub configuration: usize,
    pub checkpoints: usize,
    pub allocations: usize,
    pub gaps: usize,
    pub disassembly: usize,
    pub instructions: usize,
    /// Terminal control records (mirrors `JournalTrace::finalized`).
    pub finalized: usize,
    pub unknown: usize,
}

/// Validated contents of a Stage 1 session journal.
///
/// This is the journal's honest TP-1 representation: evidence in recorded
/// order, with explicit completeness flags. It is deliberately **not** a
/// `DebugSessionTrace`; see [`TraceParseError::JournalReconstructionUnsupported`].
#[derive(Debug, Clone)]
pub struct JournalTrace {
    /// Decoded records in file order (ordering preserved exactly).
    pub records: Vec<JournalRecord>,
    /// Per-kind counts (sums to `records.len()`).
    pub counts: JournalCounts,
    /// True when the writer appended its `finalized` control record, i.e.
    /// the session reached orderly shutdown. `false` for a partial journal.
    pub finalized: bool,
    /// True when the final physical line had no trailing newline and could
    /// not be decoded (a crashed session's torn write). The line's bytes
    /// are reported here and never interpreted.
    pub torn_tail: bool,
    /// Number of physical lines inspected.
    pub line_count: usize,
}

impl JournalTrace {
    /// The session configuration (always the first record when present).
    pub fn configuration(&self) -> Option<&TracingConfig> {
        self.records.iter().find_map(|r| match r {
            JournalRecord::Configuration(config) => Some(config),
            _ => None,
        })
    }
}

/// Writer-side envelope of the `allocation` record (`recorder::Observed`).
#[derive(Deserialize)]
struct ObservedAllocation {
    observed_at_stop: u64,
    event: HeapEvent,
}

/// Parse a journal from an in-memory buffer.
pub(crate) fn parse_bytes(bytes: &[u8], options: &TraceLoadOptions) -> Result<JournalTrace> {
    if bytes.len() as u64 > options.max_bytes {
        return Err(TraceParseError::LimitExceeded {
            limit: options.max_bytes,
        });
    }
    parse_reader(std::io::Cursor::new(bytes), options)
}

/// Parse a journal line-by-line from a reader.
///
/// Memory stays bounded: one line is buffered at a time (a line may never
/// exceed `options.max_bytes`), and decoded records are typed (the raw JSON
/// value of each record is dropped after decoding, not retained).
pub(crate) fn parse_reader(
    mut reader: impl BufRead,
    options: &TraceLoadOptions,
) -> Result<JournalTrace> {
    let mut records = Vec::new();
    let mut counts = JournalCounts::default();
    let mut line_count = 0usize;
    let mut torn_tail = false;
    let mut buf = Vec::new();
    let mut saw_configuration = false;

    loop {
        buf.clear();
        // Bound one record: no line may exceed the whole-input byte budget.
        // `Take<&mut R>` is `BufRead` when `R` is, so `read_until` works.
        let n = (&mut reader)
            .take(options.max_bytes)
            .read_until(b'\n', &mut buf)
            .map_err(|e| TraceParseError::Io { source: e })?;
        if n == 0 {
            break; // EOF
        }
        line_count += 1;

        let ends_with_newline = buf.last() == Some(&b'\n');
        if !ends_with_newline {
            if buf.len() as u64 >= options.max_bytes {
                // Filled the whole budget without finding a newline: this is
                // an oversized record, not a torn tail.
                return Err(TraceParseError::LimitExceeded {
                    limit: options.max_bytes,
                });
            }
            // No trailing newline: a suspected torn write. Accept it only
            // when it fully decodes; otherwise report and stop.
            match decode_line(&buf, line_count, options) {
                Ok(Some(record)) => {
                    push_record(
                        record,
                        &mut records,
                        &mut counts,
                        line_count,
                        options.max_journal_records,
                        &mut saw_configuration,
                    )?;
                }
                _ => {
                    torn_tail = true;
                    break;
                }
            }
            continue;
        }

        if let Some(record) = decode_line(&buf, line_count, options)? {
            push_record(
                record,
                &mut records,
                &mut counts,
                line_count,
                options.max_journal_records,
                &mut saw_configuration,
            )?;
        }
    }

    let finalized = counts.finalized > 0;
    Ok(JournalTrace {
        records,
        counts,
        finalized,
        torn_tail,
        line_count,
    })
}

/// Insert one record, enforcing the record limit and the
/// configuration-first writer invariant, and updating per-kind counts.
fn push_record(
    record: JournalRecord,
    records: &mut Vec<JournalRecord>,
    counts: &mut JournalCounts,
    line: usize,
    limit: usize,
    saw_configuration: &mut bool,
) -> Result<()> {
    if records.len() >= limit {
        return Err(TraceParseError::JournalRecordLimitExceeded { limit });
    }
    if records.is_empty() && !*saw_configuration {
        // The very first record must be the session configuration.
        if !matches!(record, JournalRecord::Configuration(_)) {
            return Err(TraceParseError::JournalMissingConfiguration {
                line,
                kind: record.kind().to_owned(),
            });
        }
        *saw_configuration = true;
    }
    match &record {
        JournalRecord::Configuration(_) => counts.configuration += 1,
        JournalRecord::Checkpoint(_) => counts.checkpoints += 1,
        JournalRecord::Allocation { .. } => counts.allocations += 1,
        JournalRecord::Gap(_) => counts.gaps += 1,
        JournalRecord::Disassembly(_) => counts.disassembly += 1,
        JournalRecord::Instruction(_) => counts.instructions += 1,
        JournalRecord::Finalized => counts.finalized += 1,
        JournalRecord::Unknown { .. } => counts.unknown += 1,
    }
    records.push(record);
    Ok(())
}

/// Decode one journal line. `Ok(None)` = blank line (tolerated, skipped).
fn decode_line(
    buf: &[u8],
    line: usize,
    options: &TraceLoadOptions,
) -> Result<Option<JournalRecord>> {
    let text = std::str::from_utf8(buf).map_err(|e| TraceParseError::MalformedJournalRecord {
        line,
        reason: format!("record is not valid UTF-8 ({e})"),
    })?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let mut value: Value =
        serde_json::from_str(trimmed).map_err(|e| TraceParseError::MalformedJournalRecord {
            line,
            reason: format!("invalid JSON ({e})"),
        })?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| TraceParseError::MalformedJournalRecord {
            line,
            reason: "record is not a JSON object".into(),
        })?;
    let kind = obj
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| TraceParseError::MalformedJournalRecord {
            line,
            reason: "missing or non-string `kind`".into(),
        })?
        .to_owned();
    let data = obj
        .remove("data")
        .ok_or_else(|| TraceParseError::MalformedJournalRecord {
            line,
            reason: "missing `data`".into(),
        })?;

    let payload_err = |err: serde_json::Error| TraceParseError::MalformedJournalRecord {
        line,
        reason: format!("`{kind}` payload does not match the recorded schema: {err}"),
    };

    let record = match kind.as_str() {
        "configuration" => {
            JournalRecord::Configuration(serde_json::from_value(data).map_err(payload_err)?)
        }
        "checkpoint" => {
            JournalRecord::Checkpoint(serde_json::from_value(data).map_err(payload_err)?)
        }
        "allocation" => {
            let observed: ObservedAllocation = serde_json::from_value(data).map_err(payload_err)?;
            JournalRecord::Allocation {
                observed_at_stop: observed.observed_at_stop,
                event: observed.event,
            }
        }
        "gap" => JournalRecord::Gap(serde_json::from_value(data).map_err(payload_err)?),
        "disassembly" => {
            JournalRecord::Disassembly(serde_json::from_value(data).map_err(payload_err)?)
        }
        "instruction" => {
            JournalRecord::Instruction(serde_json::from_value(data).map_err(payload_err)?)
        }
        "finalized" => {
            let flag: bool = serde_json::from_value(data).map_err(payload_err)?;
            if !flag {
                return Err(TraceParseError::MalformedJournalRecord {
                    line,
                    reason: "`finalized` control record must carry `true`".into(),
                });
            }
            JournalRecord::Finalized
        }
        other => {
            if options.allow_unknown_journal_records {
                JournalRecord::Unknown {
                    kind: other.to_owned(),
                    data,
                }
            } else {
                return Err(TraceParseError::UnsupportedJournalRecord {
                    line,
                    kind: other.to_owned(),
                });
            }
        }
    };
    Ok(Some(record))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> TraceLoadOptions {
        TraceLoadOptions::default()
    }

    fn parse(text: &str) -> Result<JournalTrace> {
        parse_reader(std::io::Cursor::new(text), &options())
    }

    fn valid_gap() -> String {
        // TraceGap requires instruction_position, origin and reason.
        r#"{"kind":"gap","data":{"instruction_position":0,"origin":"startup","reason":"not recorded"}}"#.into()
    }

    #[test]
    fn parses_configuration_checkpoints_and_finalized() {
        let journal = parse("{\"kind\":\"configuration\",\"data\":{\"mode\":\"dual\"}}\n").unwrap();
        assert_eq!(journal.counts.configuration, 1);
        assert!(!journal.finalized);
        assert!(!journal.torn_tail);
        assert_eq!(journal.line_count, 1);
        assert_eq!(
            journal.configuration().unwrap().mode,
            crate::config::TraceMode::Dual
        );

        let journal = parse(&format!(
            "{{\"kind\":\"configuration\",\"data\":{{\"mode\":\"dual\"}}}}\n{}\n{{\"kind\":\"finalized\",\"data\":true}}\n",
            valid_gap()
        ))
        .unwrap();
        assert_eq!(journal.counts.configuration, 1);
        assert_eq!(journal.counts.gaps, 1);
        assert!(journal.finalized);
        assert!(!journal.torn_tail);
        assert_eq!(journal.line_count, 3);
    }

    #[test]
    fn blank_lines_are_tolerated() {
        let journal = parse(
            "{\"kind\":\"configuration\",\"data\":{}}\n\n{\"kind\":\"finalized\",\"data\":true}\n",
        )
        .unwrap();
        assert_eq!(journal.counts.configuration, 1);
        assert_eq!(journal.counts.unknown, 0);
    }

    #[test]
    fn non_configuration_first_record_is_rejected() {
        let text = format!("{}\n", valid_gap());
        let err = parse(&text).unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::JournalMissingConfiguration { line: 1, ref kind } if kind == "gap"
        ));
    }

    #[test]
    fn torn_tail_is_reported_not_interpreted() {
        let journal =
            parse("{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"instruct").unwrap();
        assert!(journal.torn_tail);
        assert!(!journal.finalized);
        assert_eq!(journal.counts.configuration, 1);
    }

    #[test]
    fn complete_but_final_line_without_newline_is_accepted() {
        let journal = parse(
            "{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"finalized\",\"data\":true}",
        );
        assert!(journal.unwrap().finalized);
    }

    #[test]
    fn malformed_complete_line_is_an_error() {
        let err = parse("{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"checkpoint\"\n")
            .unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::MalformedJournalRecord { line: 2, .. }
        ));
    }

    #[test]
    fn non_object_record_is_an_error() {
        let err = parse("{\"kind\":\"configuration\",\"data\":{}}\n[1,2,3]\n").unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::MalformedJournalRecord { line: 2, .. }
        ));
    }

    #[test]
    fn unknown_kind_is_rejected_unless_allowed() {
        let text = "{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"future\",\"data\":42}\n";
        let err = parse(text).unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::UnsupportedJournalRecord { line: 2, ref kind } if kind == "future"
        ));

        let mut opts = options();
        opts.allow_unknown_journal_records = true;
        let journal = parse_reader(std::io::Cursor::new(text), &opts).unwrap();
        assert_eq!(journal.counts.unknown, 1);
        match &journal.records[1] {
            JournalRecord::Unknown { kind, data } => {
                assert_eq!(kind, "future");
                assert_eq!(*data, serde_json::json!(42));
            }
            other => panic!("unexpected record: {other:?}"),
        }
    }

    #[test]
    fn finalized_control_record_must_be_true() {
        let err = parse(
            "{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"finalized\",\"data\":false}\n",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::MalformedJournalRecord { line: 2, .. }
        ));
    }

    #[test]
    fn typed_payload_mismatches_are_errors() {
        let err = parse("{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"gap\",\"data\":{\"nonsense\":1}}\n")
            .unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::MalformedJournalRecord { line: 2, .. }
        ));
    }

    #[test]
    fn record_limit_is_enforced() {
        let mut opts = options();
        opts.max_journal_records = 2;
        let body = format!(
            "{{\"kind\":\"configuration\",\"data\":{{}}}}\n{gap}\n{gap}\n",
            gap = valid_gap()
        );
        let err = parse_reader(std::io::Cursor::new(body), &opts).unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::JournalRecordLimitExceeded { limit: 2 }
        ));
    }

    #[test]
    fn empty_journal_is_valid_but_empty() {
        let journal = parse("").unwrap();
        assert!(journal.records.is_empty());
        assert!(!journal.finalized);
        assert!(!journal.torn_tail);
        assert_eq!(journal.line_count, 0);
    }

    #[test]
    fn oversized_line_is_rejected() {
        let mut opts = options();
        opts.max_bytes = 32;
        let err = parse_reader(
            std::io::Cursor::new(concat!(
                "{\"kind\":\"configuration\",\"data\":{}}\n",
                "{\"kind\":\"checkpoint\",\"data\":{\"pad\":\"",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "\"}}\n"
            )),
            &opts,
        )
        .unwrap_err();
        assert!(matches!(err, TraceParseError::LimitExceeded { limit: 32 }));
    }
}

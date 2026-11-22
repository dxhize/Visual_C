//! Structured errors for trace ingestion (Trace Parser TP-1).
//!
//! Distinct from the engine's [`crate::error::EngineError`]: the parser never
//! compiles, executes, or debugs anything, so its failure modes are about
//! *inputs*. Variants are typed so later backend components (Query/Replay API,
//! Semantic Engine) can match on category instead of parsing strings, and no
//! variant panics on adversarial input. JSON failures carry serde_json's
//! line/column; journal failures carry the offending line number.
//!
//! Hard errors are reserved for conditions that prevent trustworthy
//! ingestion. Partial traces, skipped tool stages, unreadable memory, and
//! similar *recorded* conditions are data on the store, never errors here.

use std::path::PathBuf;
use thiserror::Error;

/// Errors returned while ingesting a Stage 1 trace.
#[derive(Debug, Error)]
pub enum TraceParseError {
    /// Identifies which input a failure belongs to (file ingestion paths
    /// wrap every inner error in this). Inspect with
    /// [`std::error::Error::source`].
    #[error("while ingesting `{path}`: {source}")]
    Source {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Filesystem or stream failure while reading the input
    /// (missing file, permission denied, broken pipe, ...).
    #[error("cannot read trace input: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },

    /// The input contains zero bytes (or only whitespace).
    #[error("trace input is empty")]
    EmptyInput,

    /// The input is larger than [`TraceLoadOptions::max_bytes`](super::TraceLoadOptions::max_bytes).
    #[error("trace input exceeds the configured size limit of {limit} bytes")]
    LimitExceeded { limit: u64 },

    /// The input is not valid UTF-8. UTF-8 is the only supported encoding;
    /// the parser never guesses encodings.
    #[error("trace input is not valid UTF-8 (unsupported encoding): {source}")]
    InvalidEncoding {
        #[source]
        source: std::str::Utf8Error,
    },

    /// Text that is not a well-formed JSON trace document (bad JSON, wrong
    /// top-level shape, or a required field of the wrong type). `line` and
    /// `column` come from serde_json.
    #[error("malformed JSON trace at line {line}, column {column}: {message}")]
    MalformedJson {
        line: usize,
        column: usize,
        message: String,
    },

    /// Content-based format detection could not classify the input, or the
    /// input was handed to the parser for the other format.
    #[error("input is neither a valid JSON trace nor a valid journal: {detail}")]
    UnrecognizedFormat { detail: String },

    /// The declared (or defaulted) schema version is not supported.
    #[error("unsupported trace schema version {found} (supported: {supported})")]
    UnsupportedSchemaVersion { found: u32, supported: &'static str },

    /// Structurally invalid beyond what serde deserialization can check.
    #[error("structurally invalid trace: {reason}")]
    InvalidTrace { reason: String },

    /// A journal line that should be a complete record is not valid.
    #[error("malformed journal record on line {line}: {reason}")]
    MalformedJournalRecord { line: usize, reason: String },

    /// A journal record kind outside the set produced by this engine family.
    /// Accepted (and preserved verbatim) only when
    /// [`TraceLoadOptions::allow_unknown_journal_records`](super::TraceLoadOptions::allow_unknown_journal_records)
    /// is set.
    #[error("unsupported journal record `{kind}` on line {line}")]
    UnsupportedJournalRecord { line: usize, kind: String },

    /// The journal holds more records than
    /// [`TraceLoadOptions::max_journal_records`](super::TraceLoadOptions::max_journal_records).
    #[error("journal exceeds the configured record limit of {limit}")]
    JournalRecordLimitExceeded { limit: usize },

    /// Journal input cannot be turned into a complete `DebugSessionTrace`.
    ///
    /// The journal records per-stop evidence only (configuration,
    /// checkpoints, allocations, gaps, disassembly, instructions). The
    /// finalized fields Stage 1 computes at session end are never journaled:
    /// execution outcome/status, compile diagnostics, `heap_state`,
    /// allocation lifetimes, crash/Valgrind/sanitizer reports, resource
    /// usage and profiling totals, coverage, `session_meta`, analysis
    /// notes, and stage results. Use
    /// [`parse_journal`](super::parse_journal) to obtain the validated
    /// record stream instead — the parser never fabricates those fields.
    #[error(
        "full trace reconstruction from a journal is not supported: the journal records \
             per-stop evidence only (configuration, checkpoints, allocations, gaps, disassembly, \
             instructions), while the finalized fields Stage 1 computes at session end (execution \
             outcome/status, compile diagnostics, heap_state, allocation lifetimes, crash/valgrind/\
             sanitizer reports, resource usage and profiling totals, coverage, session_meta, \
             analysis notes, stage results) are never journaled; use parse_journal() to obtain the \
             validated record stream instead"
    )]
    JournalReconstructionUnsupported,

    /// A non-empty journal whose first record is not `configuration`
    /// (the journal writer always appends that record first).
    #[error("journal must start with a `configuration` record (first record on line {line} is `{kind}`)")]
    JournalMissingConfiguration { line: usize, kind: String },
}

/// Errors returned by TP-2 indexed queries on a [`TraceStore`](super::TraceStore).
///
/// Distinct from [`TraceParseError`]: ingestion succeeded, and these describe
/// *query* outcomes. The variants separate the situations the TP-2 contract
/// requires callers to tell apart:
///
/// * an invalid query argument (`InvalidRange`),
/// * an out-of-range positional index (`OutOfRange`),
/// * a reference that cannot resolve in the recorded data
///   (`InvalidReference`),
/// * a relationship the trace simply does not record (`Unavailable`),
/// * a recorded ordering invariant the index relies on being violated —
///   detected corruption, surfaced instead of concealed
///   (`OrderingViolation`).
///
/// A *successful but empty* result (empty slice, empty iterator, `None`) is
/// never returned to conceal corruption: it always means "valid query, no
/// such records were recorded".
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// A consumer page would exceed its explicit per-call result bound.
    #[error("{collection} page requests {requested} entries (maximum {max})")]
    LimitExceeded {
        collection: &'static str,
        requested: usize,
        max: usize,
    },
    /// An evidence reference was created by another loaded trace. Identical
    /// positions in two independent stores are not interchangeable.
    #[error("evidence reference belongs to a different loaded trace")]
    ForeignTrace,
    /// A positional index is beyond the recorded records.
    #[error("{collection} index {index} is out of range (recorded len {len})")]
    OutOfRange {
        /// Which canonical collection was addressed.
        collection: &'static str,
        /// The rejected index.
        index: usize,
        /// How many records were actually recorded.
        len: usize,
    },

    /// A range argument is malformed for the addressed collection: `start`
    /// must be `<= end` and `end` must be `<= len`. Ranges are half-open and
    /// are never silently clamped.
    #[error("invalid {collection} range {start}..{end} (recorded len {len})")]
    InvalidRange {
        /// Which canonical collection was addressed.
        collection: &'static str,
        /// Range start as given.
        start: usize,
        /// Range end as given.
        end: usize,
        /// How many records were actually recorded.
        len: usize,
    },

    /// A cross-record reference (stored in a record) does not resolve within
    /// the collection it names. The store never panics on such a reference
    /// and never guesses a nearest match; deep validation reports the same
    /// references as findings.
    #[error("`{field}` reference {value} does not resolve in {collection} (recorded len {len})")]
    InvalidReference {
        /// The reference field name, for diagnostics only (semantics never
        /// come from names — see the reference dictionary).
        field: &'static str,
        /// The unresolvable value as recorded.
        value: u64,
        /// Which canonical collection the reference points into.
        collection: &'static str,
        /// How many records the collection holds.
        len: usize,
    },

    /// The trace does not record the evidence this query needs. This is
    /// *unavailable data*, not corruption: partial traces legitimately lack
    /// observations (retention gaps, missing mappings, dropped snapshots).
    #[error("the trace does not record {what}")]
    Unavailable {
        /// What evidence is absent, phrased as recorded-data language.
        what: &'static str,
    },

    /// A recorded ordering the index build verified (and the reference
    /// dictionary guarantees for engine-written traces) does not hold. The
    /// query refuses to answer from a broken ordering rather than returning
    /// results that could silently be wrong. Callers that must cope with a
    /// corrupt trace can still scan the canonical arrays via
    /// [`TraceStore::trace`](super::TraceStore::trace).
    #[error("recorded {ordering} ordering is violated: {detail}")]
    OrderingViolation {
        /// Which ordering was verified at index build (`snapshot stop_ids`,
        /// `event stop_id_before`, `event checkpoint_id`, ...).
        ordering: &'static str,
        /// Where the violation was detected.
        detail: String,
    },
}

/// Alias for results produced by the parser.
pub type Result<T> = std::result::Result<T, TraceParseError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn source_wrapper_names_the_file() {
        let err = TraceParseError::Source {
            path: "/tmp/trace.json".into(),
            source: Box::new(TraceParseError::EmptyInput),
        };
        let text = err.to_string();
        assert!(text.contains("/tmp/trace.json"), "{text}");
        assert!(text.contains("empty"), "{text}");
        // The cause chain must remain intact for `source()` consumers.
        assert!(matches!(
            err.source()
                .and_then(|e| e.downcast_ref::<TraceParseError>()),
            Some(TraceParseError::EmptyInput)
        ));
    }
}

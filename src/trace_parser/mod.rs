//! TP-1: ingestion, structural validation and canonical store.
//! TP-2: compact indexes, cross-record resolution and raw execution queries.
//! TP-3: borrowed raw and semantic-input views with trace-local evidence.
//!
//! This module is the **Trace Parser** foundation: it receives Stage 1
//! (`c-trace-engine`) trace inputs, identifies the input format, deserializes
//! the recorded data with the existing Serde models, validates structural
//! correctness (schema version, required structures), and hands back one
//! canonical, immutable, shareable owner of the loaded trace —
//! [`TraceStore`].
//!
//! On top of the canonical store, TP-2 attaches [`TraceIndexes`] — compact
//! position-only indexes built once at load time and shared by every clone —
//! and exposes the raw query layer: positional and range access, checkpoint
//! and GDB-stop navigation, instruction↔disassembly and source↔assembly
//! resolution, allocation lookup by identity, and deep cross-record
//! validation ([`TraceStore::validate_deep`]) with typed severities. The
//! indexes never duplicate trace records; the reference-semantics dictionary
//! (documented in `docs/trace-parser.md`) separates vector positions
//! (`event_id`, `instruction_id`, `checkpoint_id`) from the 1-based GDB stop
//! counter (`stop_id*`, `observed_at_stop`) and engine allocation
//! identities (`allocation_id`).
//!
//! It intentionally does **not**: execute programs, invoke tools, infer
//! object identities or pointer graphs, reconstruct missing events, or
//! expose GUI/Tauri shapes. Recorded evidence is preserved verbatim; deriving
//! C-language meaning from it is the future Semantic Engine's job. TP-3's
//! consumer views organize recorded evidence but never infer C semantics.
//!
//! # Supported inputs
//!
//! | input             | support | notes                                     |
//! |-------------------|---------|-------------------------------------------|
//! | JSON trace file   | full    | schema version `2` (V2 / V2.1); a missing `schema_version` field defaults to 2 per the model's serde contract |
//! | Session journal   | records | JSONL journal is validated and decoded per record; full-trace reconstruction is explicitly unsupported (see [`parse_journal`]) |
//!
//! # Example
//!
//! ```
//! use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};
//!
//! let store = TraceStore::open("examples/v21_loop.json", TraceLoadOptions::default())?;
//!
//! // The canonical trace: immutable, cheap to share, never re-parsed.
//! let trace = store.trace();
//! assert_eq!(trace.schema_version, 2);
//!
//! // TP-2: positional access, cross-record resolution, indexed lookups.
//! let first = store.instruction_event(0)?;
//! let entry = store.disassembly_for_event(0)?;
//! assert_eq!(entry.address, first.address);
//! let loop_line = store.instructions_for_source("fixtures/c/finite_loop.c", 4)?;
//! assert!(!loop_line.is_empty());
//! let raw = store.raw(); // delegates TP-2 queries; no copies
//! let events = raw.instruction_range(0..2)?;
//! let semantic = store.semantic_input();
//! let context = semantic.checkpoint_context(0)?;
//! let original = semantic.resolve_evidence(&context.snapshot_evidence()?)?;
//! assert!(matches!(original, c_trace_engine::trace_parser::Evidence::Snapshot(_)));
//! assert_eq!(events.len(), 2);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::TracingConfig;
use crate::models::{
    AllocationLifetime, CompilerDiagnostic, Coverage, CrashReport, DebugSessionTrace,
    DisassembledInstruction, ExecutionOutcome, ExecutionSnapshot, ExecutionStatus, FrameVariables,
    HeapBlockInfo, HeapEvent, InstructionEvent, Profiling, ResourceUsage, SessionMeta,
    SourceContext, StageResults, StorageInfo, TraceGap,
};
use crate::resource_limits::ExecutionLimits;

pub mod error;
pub mod evidence;
pub mod indexes;
pub mod journal;
pub mod validation;
pub mod views;

mod json;

pub use error::{QueryError, Result, TraceParseError};
pub use evidence::{Evidence, EvidenceLocation, EvidenceRef};

/// Two-generic result alias for query methods: every TP-2 query returns
/// `QueryResult<T>` = `std::result::Result<T, QueryError>`, keeping query
/// errors (`QueryError`) distinct from ingestion errors
/// ([`TraceParseError`]).
pub type QueryResult<T> = std::result::Result<T, QueryError>;
pub use indexes::{
    AllocEvents, AllocationIndex, Executions, ExecutionsIndex, OrderingFacts, SourceEvents,
    SourceIndex, SourceKey, SourceKeys, TraceIndexes,
};
pub use journal::{JournalCounts, JournalRecord, JournalTrace};
pub use validation::{
    validate_deep, ValidationFinding, ValidationLocation, ValidationReport, ValidationSeverity,
};
pub use views::{CheckpointContext, RawTraceView, SemanticInputView};

// ---------------------------------------------------------------------------
// Options and format selection
// ---------------------------------------------------------------------------

/// How the parser should decide between the supported input formats.
///
/// Selection is always content-based — file names and extensions are never
/// consulted — and malformed JSON is never silently reinterpreted as a
/// journal ([`json::detect_format`] enforces the signature rules).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TraceFormatSelection {
    /// Inspect the content: one JSON document, or line-oriented journal.
    #[default]
    Auto,
    /// Require a single JSON `DebugSessionTrace` document.
    Json,
    /// Require the existing JSONL session journal.
    Journal,
}

/// The format an input was actually ingested as (reported in
/// [`TraceStore::source`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceFormat {
    /// One complete JSON document.
    Json,
    /// Line-oriented JSONL session journal.
    Journal,
}

/// Ingestion limits and toggles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceLoadOptions {
    /// Input-format policy. Default: [`TraceFormatSelection::Auto`].
    pub format: TraceFormatSelection,
    /// Maximum accepted input size in bytes. A larger input is rejected
    /// before parsing (file loads check the size first; stream loads stop
    /// at the cap), so no unbounded buffering happens. Default: 512 MiB.
    pub max_bytes: u64,
    /// Maximum journal record count. Default: 2,000,000.
    pub max_journal_records: usize,
    /// Accept journal record kinds unknown to this engine version and
    /// preserve them verbatim ([`JournalRecord::Unknown`]) instead of
    /// rejecting the record. Default: `false` (reject with
    /// [`TraceParseError::UnsupportedJournalRecord`]).
    pub allow_unknown_journal_records: bool,
}

impl Default for TraceLoadOptions {
    fn default() -> Self {
        Self {
            format: TraceFormatSelection::Auto,
            max_bytes: 512 * 1024 * 1024,
            max_journal_records: 2_000_000,
            allow_unknown_journal_records: false,
        }
    }
}

/// Compact provenance metadata for an ingested input.
///
/// Deliberately *not* a copy of the input: after successful ingestion the
/// parsed representation is authoritative, and only this small descriptor is
/// retained alongside it.
#[derive(Debug, Clone)]
pub struct TraceSourceMetadata {
    /// Filesystem path when loaded from a file; `None` for reader/slice
    /// ingestion and pre-parsed traces.
    pub path: Option<PathBuf>,
    /// Exact input size in bytes as read (0 for pre-parsed traces).
    pub byte_len: u64,
    /// The format the input was actually ingested as.
    pub format: TraceFormat,
}

// ---------------------------------------------------------------------------
// Canonical store
// ---------------------------------------------------------------------------

/// The single canonical owner of one successfully ingested Stage 1 trace.
///
/// The store wraps the deserialized [`DebugSessionTrace`] in an [`Arc`]: all
/// access is immutable, repeated reads never re-parse or copy, and future
/// consumers (Query/Replay API, Semantic Engine) can each hold
/// [`TraceStore::shared`] handles over the same authoritative data — one
/// trace, one store, many views. A trace whose C program crashed, timed out,
/// or hit a step limit is still a valid store; those outcomes are recorded
/// data (see [`TraceStore::execution_status`]), not ingestion failures.
///
/// # Ownership model
///
/// * Loading parses once; afterwards the store is a cheap `Clone` handle
///   (one `Arc` increment) plus small provenance metadata.
/// * The original input text is not retained — only
///   [`TraceSourceMetadata`] (path, byte length, detected format).
/// * There are no `&mut` methods; the trace is immutable from the moment of
///   ingestion, so shared access across threads is safe.
#[derive(Debug, Clone)]
pub struct TraceStore {
    trace: Arc<DebugSessionTrace>,
    indexes: Arc<TraceIndexes>,
    source: TraceSourceMetadata,
}

impl TraceStore {
    /// Common assembly path: index construction is eager and happens exactly
    /// once, here. The indexes are position-only views of the canonical trace
    /// (a few bytes per record; measured in the TP-2 performance findings),
    /// built with a small number of linear passes and shared by every clone
    /// of the resulting store.
    fn assemble(trace: DebugSessionTrace, source: TraceSourceMetadata) -> Self {
        let trace = Arc::new(trace);
        let indexes = Arc::new(TraceIndexes::build(&trace));
        Self {
            trace,
            indexes,
            source,
        }
    }

    /// Load and validate a JSON trace from a filesystem path.
    ///
    /// The file size is checked against `options.max_bytes` before reading.
    /// Every failure is wrapped in [`TraceParseError::Source`] naming `path`.
    ///
    /// Journal input is detected but cannot become a `TraceStore` — full
    /// reconstruction from a journal is explicitly unsupported; use
    /// [`parse_journal`] for the validated record stream.
    pub fn open(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<Self> {
        let path = path.as_ref();
        Self::open_inner(path, &options).map_err(|e| TraceParseError::Source {
            path: path.to_path_buf(),
            source: Box::new(e),
        })
    }

    fn open_inner(path: &Path, options: &TraceLoadOptions) -> Result<Self> {
        let file = File::open(path).map_err(|e| TraceParseError::Io { source: e })?;
        let declared_len = file
            .metadata()
            .map_err(|e| TraceParseError::Io { source: e })?
            .len();
        if declared_len > options.max_bytes {
            return Err(TraceParseError::LimitExceeded {
                limit: options.max_bytes,
            });
        }
        let mut bytes = Vec::new();
        // +1 so an over-limit stream is *detected* rather than silently
        // truncated into a confusing parse error (also covers files whose
        // reported metadata length lies, e.g. some /proc entries).
        let read_len = (&file)
            .take(options.max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| TraceParseError::Io { source: e })?;
        if read_len as u64 > options.max_bytes {
            return Err(TraceParseError::LimitExceeded {
                limit: options.max_bytes,
            });
        }
        let byte_len = read_len as u64;
        let trace = load_trace_document(&bytes, options)?;
        Ok(Self::assemble(
            trace,
            TraceSourceMetadata {
                path: Some(path.to_path_buf()),
                byte_len,
                format: TraceFormat::Json,
            },
        ))
    }

    /// Load a JSON trace from any byte reader (no filesystem access).
    ///
    /// Reads at most `options.max_bytes + 1` bytes: over-limit inputs are
    /// rejected with [`TraceParseError::LimitExceeded`] instead of being
    /// buffered unboundedly. Errors carry no path context (there is none).
    pub fn from_reader(mut reader: impl Read, options: TraceLoadOptions) -> Result<Self> {
        let mut bytes = Vec::new();
        let read_len = (&mut reader)
            .take(options.max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| TraceParseError::Io { source: e })?;
        if read_len as u64 > options.max_bytes {
            return Err(TraceParseError::LimitExceeded {
                limit: options.max_bytes,
            });
        }
        let byte_len = read_len as u64;
        let trace = load_trace_document(&bytes, &options)?;
        Ok(Self::assemble(
            trace,
            TraceSourceMetadata {
                path: None,
                byte_len,
                format: TraceFormat::Json,
            },
        ))
    }

    /// Parse an in-memory JSON trace document.
    ///
    /// The name is explicit on purpose: the input is always parsed as a JSON
    /// document, and `options.format` selection is not consulted
    /// (`Auto` detection also never classifies a real trace as a journal,
    /// so behavior matches `Auto` for genuine trace files). Convenient for
    /// unit tests and pre-fetched bytes.
    pub fn from_json_bytes(bytes: &[u8], options: TraceLoadOptions) -> Result<Self> {
        if bytes.len() as u64 > options.max_bytes {
            return Err(TraceParseError::LimitExceeded {
                limit: options.max_bytes,
            });
        }
        let byte_len = bytes.len() as u64;
        let text = gates(bytes)?;
        let trace = json::decode_document(text)?;
        Ok(Self::assemble(
            trace,
            TraceSourceMetadata {
                path: None,
                byte_len,
                format: TraceFormat::Json,
            },
        ))
    }

    /// Adopt an already-deserialized trace (e.g. produced in-process by the
    /// engine). Only the schema-version gate runs; the model is trusted as
    /// Stage 1's own output. No byte length is known, so
    /// `TraceSourceMetadata::byte_len` is 0.
    pub fn from_trace(trace: DebugSessionTrace) -> Result<Self> {
        json::validate_schema_version(&trace)?;
        Ok(Self::assemble(
            trace,
            TraceSourceMetadata {
                path: None,
                byte_len: 0,
                format: TraceFormat::Json,
            },
        ))
    }

    /// The canonical trace. Reads never re-parse, never copy, and never
    /// mutate: every call borrows into the same `Arc`-owned structure.
    pub fn trace(&self) -> &DebugSessionTrace {
        &self.trace
    }

    /// Cheap shareable handle to the canonical trace for future consumers.
    /// Sharing never requires reparsing; the callers all see one structure.
    pub fn shared(&self) -> Arc<DebugSessionTrace> {
        Arc::clone(&self.trace)
    }

    /// Provenance metadata of the ingested input.
    pub fn source(&self) -> &TraceSourceMetadata {
        &self.source
    }

    /// Execution status exactly as Stage 1 recorded it (`Completed`,
    /// `Crashed`, `Timeout`, `StepLimitReached`, `OutputLimitReached`, ...).
    /// Partial-trace statuses are data, not errors.
    pub fn execution_status(&self) -> &ExecutionStatus {
        &self.trace.execution_status
    }

    /// Program-level outcome details (exit code, signal, stop address,
    /// stop reason).
    pub fn execution(&self) -> &ExecutionOutcome {
        &self.trace.execution
    }

    /// Session metadata (build config, toolchain versions, program args)
    /// when Stage 1 recorded it.
    pub fn session_meta(&self) -> Option<&SessionMeta> {
        self.trace.session_meta.as_ref()
    }

    // -----------------------------------------------------------------------
    // TP-2: index access and raw queries
    // -----------------------------------------------------------------------

    /// The shared index set (also accessible directly for TP-3 facades).
    /// Position-only views of the canonical trace; built once at load time,
    /// shared by every clone of this store.
    pub fn indexes(&self) -> &TraceIndexes {
        &self.indexes
    }

    /// Deep cross-record referential validation (explicit, not run at
    /// ingestion). See [`validation`] for the severity contract; the result
    /// never rejects a trace — corruption is reported as findings, while
    /// partial coverage, unavailable evidence, and optional absence are
    /// reported as the legitimate recorded states they are.
    pub fn validate_deep(&self) -> ValidationReport {
        validation::validate_deep(&self.trace)
    }

    // -- positional access (no index: already constant-time vector access) --

    /// The snapshot at `index` (== its recorded `step_index`).
    pub fn snapshot(&self, index: usize) -> QueryResult<&ExecutionSnapshot> {
        self.trace
            .snapshots
            .get(index)
            .ok_or(QueryError::OutOfRange {
                collection: "snapshots",
                index,
                len: self.trace.snapshots.len(),
            })
    }

    /// Borrow recorded caller evidence without changing frame-0 variables or
    /// TP-2 indexes. Empty on traces predating caller capture.
    pub fn caller_frames(&self, snapshot: usize) -> QueryResult<&[FrameVariables]> {
        Ok(&self.snapshot(snapshot)?.caller_frames)
    }

    /// A half-open range of snapshots, borrowed from the canonical array.
    /// Bounds are explicit: `start <= end <= len`, never clamped.
    pub fn snapshots_in_range(&self, range: Range<usize>) -> QueryResult<&[ExecutionSnapshot]> {
        let len = self.trace.snapshots.len();
        if range.start > range.end || range.end > len {
            return Err(QueryError::InvalidRange {
                collection: "snapshots",
                start: range.start,
                end: range.end,
                len,
            });
        }
        Ok(&self.trace.snapshots[range])
    }

    /// The instruction event at `index` — identical to
    /// `trace().instruction_events[index]` because `event_id` **is** the
    /// vector position (recorder-enforced contiguity); no index is involved.
    pub fn instruction_event(&self, index: usize) -> QueryResult<&InstructionEvent> {
        self.trace
            .recording
            .instruction_events
            .get(index)
            .ok_or(QueryError::OutOfRange {
                collection: "instruction_events",
                index,
                len: self.trace.recording.instruction_events.len(),
            })
    }

    /// A half-open range of instruction events as one borrowed subslice —
    /// no copies, original recorded order, O(1). Bounds are explicit:
    /// `start <= end <= retained_event_count()`, never clamped. An empty
    /// range (`start == end`) is valid.
    pub fn instruction_range(&self, range: Range<usize>) -> QueryResult<&[InstructionEvent]> {
        let len = self.trace.recording.instruction_events.len();
        if range.start > range.end || range.end > len {
            return Err(QueryError::InvalidRange {
                collection: "instruction_events",
                start: range.start,
                end: range.end,
                len,
            });
        }
        Ok(&self.trace.recording.instruction_events[range])
    }

    /// Total **attempted** instruction events (the recorder's
    /// `instruction_count`). May exceed [`Self::retained_event_count`] when
    /// retention settings or caps dropped events; the difference is a
    /// recorded retention gap, never synthesized.
    pub fn attempted_instruction_count(&self) -> u64 {
        self.trace.recording.instruction_count
    }

    /// Number of **retained** instruction events
    /// (`instruction_events.len()`).
    pub fn retained_event_count(&self) -> usize {
        self.trace.recording.instruction_events.len()
    }

    /// The disassembly entry at `instruction_id` — a direct vector access
    /// (`instruction_id` is the 0-based table position), no index involved.
    pub fn disassembly_entry(&self, instruction_id: u64) -> QueryResult<&DisassembledInstruction> {
        let len = self.trace.recording.disassembly_table.len();
        self.trace
            .recording
            .disassembly_table
            .get(usize::try_from(instruction_id).unwrap_or(usize::MAX))
            .ok_or(QueryError::InvalidReference {
                field: "instruction_id",
                value: instruction_id,
                collection: "disassembly_table",
                len,
            })
    }

    /// Number of recorded disassembly entries.
    pub fn disassembly_len(&self) -> usize {
        self.trace.recording.disassembly_table.len()
    }

    // -- checkpoint and stop navigation ------------------------------------

    /// The authoritative source-checkpoint list: snapshot indices, in
    /// captured order (borrowed from the canonical trace; empty lists are
    /// normal).
    pub fn source_checkpoints(&self) -> &[usize] {
        &self.trace.recording.source_checkpoints
    }

    /// The snapshot index behind source-checkpoint `position` — the
    /// position list itself is the mapping (source-checkpoint #2 of
    /// `[0, 4, 9, 15]` is `snapshots[9]`, not `snapshots[2]`).
    pub fn source_checkpoint(&self, position: usize) -> QueryResult<usize> {
        let len = self.trace.recording.source_checkpoints.len();
        self.trace
            .recording
            .source_checkpoints
            .get(position)
            .copied()
            .ok_or(QueryError::OutOfRange {
                collection: "source_checkpoints",
                index: position,
                len,
            })
    }

    /// The snapshot behind source-checkpoint `position` (no clone).
    pub fn source_checkpoint_snapshot(&self, position: usize) -> QueryResult<&ExecutionSnapshot> {
        let index = self.source_checkpoint(position)?;
        self.snapshot(index)
    }

    /// The snapshot captured at GDB stop `stop_id` (1-based stop counter).
    ///
    /// `Ok(None)` means the stop has no retained snapshot — normal for
    /// partial traces (stops outside user code, retention caps), never an
    /// error. `Err(QueryError::OrderingViolation)` means the recorded
    /// stop ordering is broken (corruption; see [`OrderingFacts`]).
    ///
    /// Zero memory: a binary search over the canonical snapshot array,
    /// valid because the stop counter strictly increases across
    /// snapshots (verified at index build).
    pub fn snapshot_at_stop(&self, stop_id: u64) -> QueryResult<Option<&ExecutionSnapshot>> {
        if !self.indexes.ordering.snapshots_by_stop_sorted {
            return Err(QueryError::OrderingViolation {
                ordering: "snapshot stop_ids",
                detail: "snapshot_at_stop requires strictly increasing stop_ids".into(),
            });
        }
        let snapshots = &self.trace.snapshots;
        let position = snapshots.partition_point(|snapshot| snapshot.observation.stop_id < stop_id);
        match snapshots.get(position) {
            Some(snapshot) if snapshot.observation.stop_id == stop_id => Ok(Some(snapshot)),
            _ => Ok(None),
        }
    }

    /// The instruction events recorded **around stop `stop_id`**: the
    /// contiguous run of events whose `stop_id_before` equals it, as one
    /// borrowed subslice in recorded order. One instruction is attempted per
    /// stop, so the run is typically length 0 or 1 — but the subslice is
    /// returned as recorded, with no uniqueness assumption.
    ///
    /// Zero memory (binary search over the verified non-decreasing
    /// `stop_id_before` sequence). No snapshot needs to exist at the stop:
    /// event↔stop association is recorded on the events themselves.
    pub fn events_at_stop(&self, stop_id: u64) -> QueryResult<&[InstructionEvent]> {
        if !self.indexes.ordering.events_by_stop_sorted {
            return Err(QueryError::OrderingViolation {
                ordering: "event stop_id_before",
                detail: "events_at_stop requires non-decreasing stop_id_before".into(),
            });
        }
        let events = &self.trace.recording.instruction_events;
        let start = events.partition_point(|event| event.stop_id_before < stop_id);
        let end = events.partition_point(|event| event.stop_id_before <= stop_id);
        Ok(&events[start..end])
    }

    /// The instruction events recorded between the capture of snapshot
    /// `snapshot_index` and the next one (events whose `checkpoint_id`
    /// equals it), as one borrowed subslice. `snapshot_index` must resolve;
    /// `Err(QueryError::OutOfRange)` on a nonexistent checkpoint also guards
    /// against corrupt out-of-range `checkpoint_id` references.
    pub fn events_at_checkpoint(&self, snapshot_index: usize) -> QueryResult<&[InstructionEvent]> {
        if snapshot_index >= self.trace.snapshots.len() {
            return Err(QueryError::OutOfRange {
                collection: "snapshots",
                index: snapshot_index,
                len: self.trace.snapshots.len(),
            });
        }
        if !self.indexes.ordering.events_by_checkpoint_sorted {
            return Err(QueryError::OrderingViolation {
                ordering: "event checkpoint_id",
                detail: "events_at_checkpoint requires non-decreasing checkpoint_ids".into(),
            });
        }
        let events = &self.trace.recording.instruction_events;
        let target = Some(snapshot_index);
        let start = events.partition_point(|event| event.checkpoint_id < target);
        let end = events.partition_point(|event| event.checkpoint_id <= target);
        Ok(&events[start..end])
    }

    /// The heap events associated with stop `stop_id`, as one borrowed
    /// subslice of the chronological heap-event array.
    ///
    /// Recorded evidence: `snapshot.observation.heap_event_count` is the
    /// cumulative number of heap events ingested by that stop, so the stop's
    /// events are the slice between the previous retained snapshot's count
    /// and this one's. The association granularity is therefore the
    /// *retained snapshot* boundary — when snapshots were dropped by caps in
    /// between, the events of the dropped stops coarsen into the next
    /// retained boundary (the parser does not fabricate finer attribution;
    /// the journal's `observed_at_stop` markers are the fine-grained
    /// evidence, and journals are not reconstructible — see `parse_journal`).
    ///
    /// `Err(QueryError::Unavailable)` when no retained snapshot marks that
    /// stop (the boundary evidence does not exist);
    /// `Err(QueryError::OrderingViolation)` when the recorded counter is not
    /// monotone (corruption).
    pub fn heap_events_at_stop(&self, stop_id: u64) -> QueryResult<&[HeapEvent]> {
        if !self.indexes.ordering.heap_counts_sorted {
            return Err(QueryError::OrderingViolation {
                ordering: "snapshot heap_event_count",
                detail: "heap_events_at_stop requires a non-decreasing cumulative counter".into(),
            });
        }
        let Some(snapshot) = self.snapshot_at_stop(stop_id)? else {
            return Err(QueryError::Unavailable {
                what: "a retained snapshot boundary at this stop (heap-event association is recorded via snapshot.observation.heap_event_count)",
            });
        };
        let current = snapshot.observation.heap_event_count;
        let position = self
            .trace
            .snapshots
            .partition_point(|s| s.observation.stop_id < stop_id);
        let previous = position
            .checked_sub(1)
            .and_then(|previous| self.trace.snapshots.get(previous))
            .map_or(0, |snapshot| snapshot.observation.heap_event_count);
        if previous > current || current > self.trace.heap_events.len() {
            return Err(QueryError::InvalidReference {
                field: "heap_event_count",
                value: current as u64,
                collection: "heap_events",
                len: self.trace.heap_events.len(),
            });
        }
        Ok(&self.trace.heap_events[previous..current])
    }

    // -- source ↔ assembly --------------------------------------------------

    /// Distinct recorded source-file identities (interned, first-appearance
    /// order). Identity is the recorded string, verbatim: distinct files
    /// with identical basenames appear separately, and no normalization
    /// ever merges them.
    pub fn source_files(&self) -> &[Arc<str>] {
        self.indexes.sources.files()
    }

    /// **Static** side of the source relationship: the disassembly entry ids
    /// associated with `(file, line)` — deduplicated, in first-appearance
    /// order. One source line often maps to several machine instructions.
    /// Empty (a valid result, not an error) when the location was never
    /// recorded or its events carried no resolvable `instruction_id`.
    pub fn instructions_for_source(&self, file: &str, line: u32) -> QueryResult<&[u32]> {
        match self.indexes.sources.key_position(file, line) {
            Some(key) => {
                if let Some(value) = self.indexes.sources.invalid_instruction_ref(key) {
                    return Err(QueryError::InvalidReference {
                        field: "instruction_id",
                        value,
                        collection: "disassembly_table",
                        len: self.disassembly_len(),
                    });
                }
                Ok(self.indexes.sources.instruction_ids_of(key))
            }
            None => Ok(&[]),
        }
    }

    /// **Dynamic** side of the source relationship: positions of the events
    /// that executed instructions associated with `(file, line)` — every
    /// visit, ascending, repeated visits preserved. Use
    /// [`Self::executed_events_for_source`] to iterate the events themselves
    /// without allocating.
    pub fn executed_event_positions_for_source(&self, file: &str, line: u32) -> &[u32] {
        match self.indexes.sources.key_position(file, line) {
            Some(key) => self.indexes.sources.event_positions_of(key),
            None => &[],
        }
    }

    /// Like [`Self::executed_event_positions_for_source`], yielding the
    /// borrowed events directly (recorded order, no allocation).
    pub fn executed_events_for_source(&self, file: &str, line: u32) -> SourceEvents<'_> {
        let positions = self.executed_event_positions_for_source(file, line);
        SourceEvents::new(&self.trace.recording.instruction_events, positions)
    }

    /// The source locations associated with one disassembly entry, resolved
    /// through the recorded event evidence (the disassembly table itself
    /// carries no source fields in this schema). Usually one location;
    /// multiple can occur and are all reported.
    pub fn sources_for_instruction(&self, instruction_id: u64) -> QueryResult<SourceKeys<'_>> {
        let values = self
            .indexes
            .sources
            .source_key_positions_of(instruction_id)?;
        Ok(SourceKeys::new(
            self.indexes.sources.key_list(),
            self.indexes.sources.file_identities(),
            values,
        ))
    }

    /// The recorded source context of one event (its `source` field — the
    /// file/line/function the debugger reported at the attempt).
    pub fn event_source(&self, index: usize) -> QueryResult<&SourceContext> {
        Ok(&self.instruction_event(index)?.source)
    }

    // -- disassembly ↔ executions --------------------------------------------

    /// Positions of the events that executed `instruction_id` — one static
    /// instruction's distinct executions, ascending. A loop body executed
    /// 500 times yields 500 positions. Invalid ids are rejected explicitly.
    pub fn instruction_execution_positions(&self, instruction_id: u64) -> QueryResult<&[u32]> {
        self.indexes.executions.positions_of(instruction_id)
    }

    /// Like [`Self::instruction_execution_positions`], yielding the borrowed
    /// events directly (recorded order, no allocation).
    pub fn instruction_executions(&self, instruction_id: u64) -> QueryResult<Executions<'_>> {
        let positions = self.indexes.executions.positions_of(instruction_id)?;
        Ok(Executions::new(
            &self.trace.recording.instruction_events,
            positions,
        ))
    }

    /// The disassembly entry an event executed, via its `instruction_id`.
    ///
    /// `Err(QueryError::Unavailable)` when the event recorded no reference
    /// (static disassembly disabled or unresolved at record time);
    /// `Err(QueryError::InvalidReference)` when the reference is out of
    /// table range (corruption — also a deep-validation finding).
    pub fn disassembly_for_event(&self, index: usize) -> QueryResult<&DisassembledInstruction> {
        match self.instruction_event(index)?.instruction_id {
            Some(instruction_id) => {
                let entry = self.disassembly_entry(instruction_id)?;
                if entry.address != self.instruction_event(index)?.address {
                    return Err(QueryError::InvalidReference {
                        field: "instruction_id (address mismatch)",
                        value: instruction_id,
                        collection: "disassembly_table",
                        len: self.disassembly_len(),
                    });
                }
                Ok(entry)
            }
            None => Err(QueryError::Unavailable {
                what: "a disassembly reference on this event",
            }),
        }
    }

    // -- heap by allocation identity ------------------------------------------

    /// Positions of the heap events recorded for `allocation_id`
    /// (chronological). Empty for unknown ids — valid result. Id `0`
    /// returns the untracked events exactly as recorded (the identity
    /// contract: 0 = untracked); addresses are never keys, so two
    /// allocations that reused one address never merge.
    pub fn heap_event_positions_for_allocation(&self, allocation_id: u64) -> &[u32] {
        self.indexes.allocations.event_positions_of(allocation_id)
    }

    /// Like [`Self::heap_event_positions_for_allocation`], yielding the
    /// borrowed events directly (no allocation).
    pub fn heap_events_for_allocation(&self, allocation_id: u64) -> AllocEvents<'_> {
        let positions = self.heap_event_positions_for_allocation(allocation_id);
        AllocEvents::new(&self.trace.heap_events, positions)
    }

    /// The `heap_state` block of `allocation_id` (its final recorded state).
    /// `None` for unknown ids, including id `0` (untracked traffic has no
    /// reconstructed block).
    pub fn heap_block(&self, allocation_id: u64) -> Option<&HeapBlockInfo> {
        let position = self.indexes.allocations.state_position_of(allocation_id)?;
        self.trace.heap_state.get(position as usize)
    }

    /// The recorded allocation lifetime of `allocation_id` (raw caller
    /// address, sequence numbers, resolution) when present.
    pub fn allocation_lifetime(&self, allocation_id: u64) -> Option<&AllocationLifetime> {
        let position = self
            .indexes
            .allocations
            .lifetime_position_of(allocation_id)?;
        self.trace
            .recording
            .allocation_lifetimes
            .get(position as usize)
    }

    // -- gaps and diagnostics ---------------------------------------------------

    /// The recorded trace gaps (unrecorded execution intervals), in
    /// recorded order; `instruction_position` is an attempted-events
    /// position (never a retained-event index).
    pub fn trace_gaps(&self) -> &[TraceGap] {
        &self.trace.recording.trace_gaps
    }

    /// The crash report when Stage 1 recorded one.
    pub fn crash_report(&self) -> Option<&CrashReport> {
        self.trace.crash_report.as_ref()
    }

    /// The Valgrind report when Stage 1 recorded one (`None` is a recorded
    /// skipped/unavailable state, not an error).
    pub fn valgrind_report(&self) -> Option<&crate::models::ValgrindReport> {
        self.trace.valgrind_report.as_ref()
    }

    /// The sanitizer-pass report when Stage 1 recorded one (the pass only
    /// runs after a crash).
    pub fn sanitizer_report(&self) -> Option<&crate::models::SanitizerReport> {
        self.trace.sanitizer_report.as_ref()
    }

    /// Compiler diagnostics as recorded (typed records with file/line/
    /// column/kind/message; positional access into this collection is the
    /// documented form — the schema defines no stable diagnostic ids).
    pub fn compile_errors(&self) -> &[CompilerDiagnostic] {
        &self.trace.compile_errors
    }

    /// Per-tool-stage completion statuses as recorded.
    pub fn stage_results(&self) -> &StageResults {
        &self.trace.stage_results
    }

    /// Engine-internal diagnostic notes (recorder-level).
    pub fn internal_diagnostics(&self) -> &[String] {
        &self.trace.recording.internal_diagnostics
    }

    /// Recording coverage facts (scope, completeness, truncation reasons,
    /// documented limitations).
    pub fn coverage(&self) -> &Coverage {
        &self.trace.recording.coverage
    }

    /// Storage facts (journal path, finalization, byte counts).
    pub fn storage(&self) -> &StorageInfo {
        &self.trace.recording.storage
    }

    /// Engine profiling counters as recorded.
    pub fn profiling(&self) -> &Profiling {
        &self.trace.recording.profiling
    }

    /// The chronological heap-event array (borrowed; see
    /// [`Self::heap_events_for_allocation`] for identity-based lookup).
    pub fn heap_events(&self) -> &[HeapEvent] {
        &self.trace.heap_events
    }

    /// The reconstructed heap blocks with their final states (borrowed; see
    /// [`Self::heap_block`] for identity-based lookup).
    pub fn heap_state(&self) -> &[HeapBlockInfo] {
        &self.trace.heap_state
    }

    /// The recorded allocation lifetimes (borrowed; see
    /// [`Self::allocation_lifetime`] for identity-based lookup).
    pub fn allocation_lifetimes(&self) -> &[AllocationLifetime] {
        &self.trace.recording.allocation_lifetimes
    }

    /// Resource usage as recorded (deadlines hit, limits triggered).
    pub fn resource_usage(&self) -> &ResourceUsage {
        &self.trace.resource_usage
    }

    /// The execution resource limits the engine recorded.
    pub fn execution_limits(&self) -> &ExecutionLimits {
        &self.trace.execution_limits
    }

    /// The tracing configuration the session ran with.
    pub fn tracing_configuration(&self) -> &TracingConfig {
        &self.trace.tracing_configuration
    }
}

// ---------------------------------------------------------------------------
// Journal ingestion
// ---------------------------------------------------------------------------

/// Load and validate a session journal from a filesystem path.
///
/// Returns the validated record stream ([`JournalTrace`]) — never a
/// `DebugSessionTrace`. See [`TraceParseError::JournalReconstructionUnsupported`]
/// for why full reconstruction is out of scope. Failures are wrapped in
/// [`TraceParseError::Source`] naming `path`.
pub fn parse_journal(path: impl AsRef<Path>, options: TraceLoadOptions) -> Result<JournalTrace> {
    let path = path.as_ref();
    parse_journal_inner(path, &options).map_err(|e| TraceParseError::Source {
        path: path.to_path_buf(),
        source: Box::new(e),
    })
}

fn parse_journal_inner(path: &Path, options: &TraceLoadOptions) -> Result<JournalTrace> {
    let file = File::open(path).map_err(|e| TraceParseError::Io { source: e })?;
    let declared_len = file
        .metadata()
        .map_err(|e| TraceParseError::Io { source: e })?
        .len();
    if declared_len > options.max_bytes {
        return Err(TraceParseError::LimitExceeded {
            limit: options.max_bytes,
        });
    }
    let reader = std::io::BufReader::with_capacity(64 * 1024, file);
    journal::parse_reader(reader, options)
}

/// Parse a session journal from an in-memory buffer.
///
/// Like [`TraceStore::from_json_bytes`], the name is explicit: the input is
/// always parsed as a journal, `options.format` is not consulted.
pub fn parse_journal_bytes(bytes: &[u8], options: TraceLoadOptions) -> Result<JournalTrace> {
    journal::parse_bytes(bytes, &options)
}
/// Parse a session journal from a line-oriented reader (no filesystem access).
pub fn parse_journal_reader(
    reader: impl std::io::BufRead,
    options: TraceLoadOptions,
) -> Result<JournalTrace> {
    journal::parse_reader(reader, &options)
}

// ---------------------------------------------------------------------------
// Ingestion dispatch
// ---------------------------------------------------------------------------

/// Load a JSON trace document, honoring the format-selection policy.
///
/// Journal-shaped input can never become a `TraceStore` (full reconstruction
/// from a journal is unsupported), so it is rejected up front — without
/// wasting effort decoding records that would be discarded.
fn load_trace_document(bytes: &[u8], options: &TraceLoadOptions) -> Result<DebugSessionTrace> {
    let text = gates(bytes)?;
    match options.format {
        TraceFormatSelection::Json => json::decode_document(text),
        TraceFormatSelection::Journal | TraceFormatSelection::Auto => {
            if options.format == TraceFormatSelection::Auto
                && json::detect_format(text) == json::DetectedFormat::JsonDocument
            {
                return json::decode_document(text);
            }
            Err(TraceParseError::JournalReconstructionUnsupported)
        }
    }
}

/// Input gates shared by every entry point: reject empty input and non-UTF-8
/// encodings before any format dispatch happens.
fn gates(bytes: &[u8]) -> Result<&str> {
    if bytes.iter().all(|&b| b.is_ascii_whitespace()) {
        return Err(TraceParseError::EmptyInput);
    }
    std::str::from_utf8(bytes).map_err(|e| TraceParseError::InvalidEncoding { source: e })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_match_documented_values() {
        let opts = TraceLoadOptions::default();
        assert_eq!(opts.max_bytes, 512 * 1024 * 1024);
        assert_eq!(opts.max_journal_records, 2_000_000);
        assert!(!opts.allow_unknown_journal_records);
        assert_eq!(opts.format, TraceFormatSelection::Auto);
    }

    #[test]
    fn journal_input_cannot_become_a_trace_store() {
        let journal_text =
            b"{\"kind\":\"configuration\",\"data\":{}}\n{\"kind\":\"finalized\",\"data\":true}\n";
        let err = TraceStore::from_reader(
            std::io::Cursor::new(&journal_text[..]),
            TraceLoadOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TraceParseError::JournalReconstructionUnsupported
        ));
    }

    #[test]
    fn json_trace_cannot_become_a_journal_report() {
        // A pretty-printed JSON trace fed to the journal parser: the first
        // complete line is not a journal record.
        let pretty = br#"{
  "compiled_successfully": true,
  "snapshots": []
}
"#;
        let err = parse_journal_bytes(pretty, TraceLoadOptions::default()).unwrap_err();
        assert!(
            matches!(err, TraceParseError::MalformedJournalRecord { line: 1, .. }),
            "{err}"
        );

        // A compact one-line JSON trace has no trailing newline, so the
        // journal parser reports it as an undecodable torn tail rather than
        // pretending it is journal data.
        let compact = br#"{"compiled_successfully":true,"snapshots":[]}"#;
        let journal = parse_journal_bytes(compact, TraceLoadOptions::default()).unwrap();
        assert!(journal.records.is_empty());
        assert!(journal.torn_tail);
        assert!(!journal.finalized);
    }

    #[test]
    fn empty_and_non_utf8_inputs_are_rejected() {
        let opts = TraceLoadOptions::default();
        assert!(matches!(
            TraceStore::from_json_bytes(b"", opts.clone()).unwrap_err(),
            TraceParseError::EmptyInput
        ));
        assert!(matches!(
            TraceStore::from_json_bytes(b" \n\t ", opts.clone()).unwrap_err(),
            TraceParseError::EmptyInput
        ));
        assert!(matches!(
            TraceStore::from_json_bytes(&b"{\"a\": \xff}"[..], opts.clone()).unwrap_err(),
            TraceParseError::InvalidEncoding { .. }
        ));
        assert!(matches!(
            parse_journal_bytes(&b"{\"kind\": \xff}\n"[..], opts).unwrap_err(),
            // The journal path is line-oriented: the bad encoding is
            // reported with the offending line number.
            TraceParseError::MalformedJournalRecord { line: 1, .. }
        ));
    }

    #[test]
    fn oversize_input_is_rejected_before_parsing() {
        let opts = TraceLoadOptions {
            max_bytes: 8,
            ..TraceLoadOptions::default()
        };
        assert!(matches!(
            TraceStore::from_json_bytes(br#"{"compiled_successfully":true}"#, opts.clone())
                .unwrap_err(),
            TraceParseError::LimitExceeded { limit: 8 }
        ));
        assert!(matches!(
            TraceStore::from_reader(std::io::Cursor::new(vec![b' '; 16]), opts).unwrap_err(),
            TraceParseError::LimitExceeded { limit: 8 }
        ));
    }

    #[test]
    fn tracestore_from_trace_roundtrips() {
        // Build a minimal-but-valid model exactly as the engine's own
        // serializer would emit it, then adopt it directly.
        let trace: crate::models::DebugSessionTrace = serde_json::from_str(
            r#"{
                "compiled_successfully": true,
                "compile_errors": [],
                "snapshots": [],
                "heap_events": [],
                "heap_state": [],
                "execution_status": "completed",
                "resource_usage": {
                    "wall_time_ms": 1,
                    "gdb_commands_issued": 0,
                    "snapshots_captured": 0,
                    "allocation_events_ingested": 0,
                    "program_output_bytes": 0,
                    "program_output_truncated": false,
                    "limits_triggered": [],
                    "inferior_exit_code": null
                },
                "analysis_notes": []
            }"#,
        )
        .unwrap();
        let store = TraceStore::from_trace(trace).unwrap();
        assert!(matches!(
            store.execution_status(),
            ExecutionStatus::Completed
        ));
        assert_eq!(store.source().byte_len, 0);
        assert_eq!(store.source().format, TraceFormat::Json);
        assert!(store.session_meta().is_none());
    }
}

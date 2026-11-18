//! Custom C allocation instrumentation runtime and Rust-side event
//! processing.
//!
//! The C runtime (`c_runtime/alloc_tracker.c`, embedded with `include_str!`)
//! is linked into the debug binary with `-Wl,--wrap=malloc|calloc|realloc|free`
//! and appends one event line per allocation operation to a dedicated file.
//! This module:
//!
//! * tails that file incrementally with an input cap (other processes may
//!   still write while the selected inferior is stopped),
//! * parses the event format into [`RawEvent`]s,
//! * reconstructs the chronological heap timeline: live blocks, freed
//!   blocks, lifetimes, realloc chains, per-event heap usage,
//! * resolves caller return addresses to `file:line` with a single batched
//!   `addr2line` invocation at the end of the run.
//!
//! Allocation ids are per-process counters; `(pid, id)` is the unique key
//! internally. When more than one pid appears (forked children), serialized
//! ids are widened to `(pid << 32) | id` by the trace serializer when multiple
//! processes are observed.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::error::{EngineError, Result};
use crate::models::{HeapEvent, HeapEventAttribution, HeapEventType};

/// The embedded C instrumentation runtime source.
pub const ALLOC_RUNTIME_C: &str = include_str!("../c_runtime/alloc_tracker.c");

/// Write the runtime translation unit into the session directory.
pub fn write_runtime_source(session_dir: &Path) -> Result<PathBuf> {
    let path = session_dir.join("alloc_tracker.c");
    std::fs::write(&path, ALLOC_RUNTIME_C)?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Raw event parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    Malloc,
    Calloc,
    Realloc,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawStatus {
    Overflow,
    Ok,
    Failed,
    OkSame,
    OkMoved,
    New,
    Zero,
    Null,
    Unknown,
}

/// One parsed instrumentation event line.
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub pid: u32,
    pub seq: u64,
    pub kind: RawKind,
    pub id: u64,
    pub addr: u64,
    pub size: u64,
    pub old_id: u64,
    pub old_addr: u64,
    pub caller: u64,
    pub status: RawStatus,
    pub calloc_count: Option<u64>,
    pub calloc_element_size: Option<u64>,
}

/// Parse one event line
/// (`pid seq kind id addr size old_id old_addr caller status [nmemb element_size]`).
pub fn parse_event_line(line: &str) -> Result<RawEvent> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 10 && fields.len() != 12 {
        return Err(crate::error::EngineError::AllocationEventParse {
            line: 0,
            message: format!("expected 10 or 12 fields, found {}", fields.len()),
        });
    }
    let num = |i: usize| -> Result<u64> {
        fields[i]
            .parse::<u64>()
            .map_err(|_| crate::error::EngineError::AllocationEventParse {
                line: 0,
                message: format!("invalid decimal field {}: {:?}", i, fields[i]),
            })
    };
    let hex = |i: usize| -> Result<u64> {
        u64::from_str_radix(fields[i], 16).map_err(|_| {
            crate::error::EngineError::AllocationEventParse {
                line: 0,
                message: format!("invalid hex field {}: {:?}", i, fields[i]),
            }
        })
    };
    let kind = match fields[2] {
        "malloc" => RawKind::Malloc,
        "calloc" => RawKind::Calloc,
        "realloc" => RawKind::Realloc,
        "free" => RawKind::Free,
        other => {
            return Err(crate::error::EngineError::AllocationEventParse {
                line: 0,
                message: format!("unknown kind {other:?}"),
            })
        }
    };
    if fields.len() == 12 && kind != RawKind::Calloc {
        return Err(EngineError::AllocationEventParse {
            line: 0,
            message: "only calloc events carry original operands".into(),
        });
    }
    let status = match fields[9] {
        "ok" => RawStatus::Ok,
        "failed" => RawStatus::Failed,
        "overflow" => RawStatus::Overflow,
        "ok_same" => RawStatus::OkSame,
        "ok_moved" => RawStatus::OkMoved,
        "new" => RawStatus::New,
        "zero" => RawStatus::Zero,
        "null" => RawStatus::Null,
        "unknown" => RawStatus::Unknown,
        other => {
            return Err(crate::error::EngineError::AllocationEventParse {
                line: 0,
                message: format!("unknown status {other:?}"),
            })
        }
    };
    Ok(RawEvent {
        pid: u32::try_from(num(0)?).map_err(|_| EngineError::AllocationEventParse {
            line: 0,
            message: "pid exceeds u32 range".into(),
        })?,
        seq: num(1)?,
        kind,
        id: num(3)?,
        addr: hex(4)?,
        size: num(5)?,
        old_id: num(6)?,
        old_addr: hex(7)?,
        caller: hex(8)?,
        status,
        calloc_count: (fields.len() == 12).then(|| num(10)).transpose()?,
        calloc_element_size: (fields.len() == 12).then(|| num(11)).transpose()?,
    })
}

/// Incremental reader for the event trace file.
pub struct EventFileReader {
    path: PathBuf,
    offset: u64,
    #[cfg(unix)]
    file_identity: Option<(u64, u64)>,
    partial: Vec<u8>,
    pub parse_errors: Vec<String>,
    pub parse_error_count: u64,
    oversized_line: bool,
    input_truncated: bool,
}

/// A stop cannot make the engine ingest an unbounded log segment into RAM.
const MAX_EVENT_READ_BYTES: u64 = 1024 * 1024;

/// Check the independent destructor ACKs against every observed process and
/// the main inferior (which can have zero events). This is intentionally a
/// separate bounded read; an ACK cannot repair a damaged event stream.
pub fn verify_event_status(
    path: &Path,
    timeline: &HeapTimeline,
    inferior_pid: Option<u32>,
) -> Vec<String> {
    let mut expected = timeline.last_seq_by_pid.clone();
    if let Some(pid) = inferior_pid {
        expected.entry(pid).or_insert(0);
    } else if expected.is_empty() {
        return vec!["inferior PID unavailable for allocation event status verification".into()];
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => return vec![format!("allocation event status file unavailable: {error}")],
    };
    let mut bytes = Vec::new();
    if let Err(error) = file
        .take(MAX_EVENT_READ_BYTES + 1)
        .read_to_end(&mut bytes)
    {
        return vec![format!("allocation event status read failed: {error}")];
    }
    if bytes.len() as u64 > MAX_EVENT_READ_BYTES {
        return vec!["allocation event status exceeds 1 MiB input cap".into()];
    }
    if bytes.is_empty() || bytes.last() != Some(&b'\n') {
        return vec!["allocation event status missing final newline (truncated ACK)".into()];
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(_) => return vec!["invalid allocation event status UTF-8".into()],
    };
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    for line in text.strip_suffix('\n').unwrap().split('\n') {
        let fields: Vec<&str> = line.split(' ').collect();
        if fields.len() != 3
            || fields
                .iter()
                .any(|field| field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()))
        {
            return vec!["invalid allocation event status ACK syntax".into()];
        }
        let (Ok(pid), Ok(seq), Ok(failures)) = (
            fields[0].parse::<u32>(),
            fields[1].parse::<u64>(),
            fields[2].parse::<u64>(),
        ) else {
            return vec!["invalid allocation event status ACK number".into()];
        };
        if pid == 0 {
            return vec!["invalid allocation event status PID 0".into()];
        }
        if !seen.insert(pid) {
            errors.push(format!("duplicate allocation event status ACK for PID {pid}"));
        }
        match expected.get(&pid) {
            Some(&observed) if seq != observed => errors.push(format!(
                "allocation event status sequence mismatch for PID {pid}: ACK {seq}, observed {observed}"
            )),
            None => errors.push(format!(
                "allocation event status ACK for unobserved PID {pid}"
            )),
            _ => {}
        }
        if failures != 0 {
            errors.push(format!(
                "allocation event status writer failure for PID {pid}: {failures} failed writes"
            ));
        }
        if errors.len() >= 50 {
            return errors;
        }
    }
    for pid in expected.keys() {
        if !seen.contains(pid) && errors.len() < 50 {
            errors.push(format!("missing allocation event status ACK for PID {pid}"));
        }
    }
    errors
}

impl EventFileReader {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            #[cfg(unix)]
            file_identity: None,
            partial: Vec::new(),
            parse_errors: Vec::new(),
            parse_error_count: 0,
            oversized_line: false,
            input_truncated: false,
        }
    }

    pub fn incomplete_line(&self) -> bool {
        !self.partial.is_empty() || self.oversized_line
    }

    /// A definitive transport loss, unlike a short line still being appended.
    pub fn transport_damaged(&self) -> bool {
        self.parse_error_count != 0 || self.oversized_line || self.input_truncated
    }

    fn record_error(&mut self, error: String) {
        self.parse_error_count = self.parse_error_count.saturating_add(1);
        if self.parse_errors.len() < 50 {
            self.parse_errors.push(error);
        }
    }

    /// Read at most one bounded batch appended since the last call. If it
    /// exceeds the cap, quarantine the remaining history; partial trailing
    /// lines are buffered until complete when no cap has been reached.
    pub fn read_new(&mut self) -> Vec<RawEvent> {
        let mut events = Vec::new();
        if self.input_truncated {
            return events;
        }
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(_) => {
                if self.offset != 0 {
                    self.input_truncated = true;
                    self.record_error("allocation log disappeared after prior observations".into());
                }
                return events;
            }
        };
        let metadata = file.metadata();
        #[cfg(unix)]
        if let Ok(metadata) = &metadata {
            use std::os::unix::fs::MetadataExt;
            let identity = (metadata.dev(), metadata.ino());
            if self.offset != 0 && self.file_identity.is_some_and(|previous| previous != identity) {
                self.input_truncated = true;
                self.record_error("allocation log replaced after prior observations".into());
                return events;
            }
            self.file_identity = Some(identity);
        }
        if metadata.is_ok_and(|metadata| metadata.len() < self.offset) {
            self.input_truncated = true;
            self.record_error("allocation log truncated after prior observations".into());
            return events;
        }
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            self.input_truncated = true;
            self.record_error("allocation log seek failed".into());
            return events;
        }
        let mut buffer = Vec::new();
        // The extra byte detects overflow without trusting file metadata or
        // allocating an unbounded vector. On loss the prefix remains evidence,
        // but all future heap-lifetime conclusions are quarantined.
        if reader
            .take(MAX_EVENT_READ_BYTES + 1)
            .read_to_end(&mut buffer)
            .is_err()
        {
            self.input_truncated = true;
            self.record_error("allocation log read failed".into());
            return events;
        }
        if buffer.len() as u64 > MAX_EVENT_READ_BYTES {
            self.input_truncated = true;
            self.record_error("allocation log exceeds per-stop 1 MiB input cap".into());
            buffer.truncate(MAX_EVENT_READ_BYTES as usize);
        }

        let mut data = std::mem::take(&mut self.partial);
        data.extend_from_slice(&buffer);
        let mut consumed = 0usize;
        let mut pos = 0usize;
        while pos < data.len() {
            match data[pos..].iter().position(|&b| b == b'\n') {
                Some(nl) => {
                    let line = &data[pos..pos + nl];
                    consumed = pos + nl + 1;
                    pos += nl + 1;
                    if self.oversized_line {
                        self.oversized_line = false;
                        self.record_error("allocation event line exceeds 4096 bytes".into());
                        continue;
                    }
                    if line.len() > 4096 {
                        self.record_error("allocation event line exceeds 4096 bytes".into());
                        continue;
                    }
                    if line.is_empty() {
                        continue;
                    }
                    match parse_event_line(&String::from_utf8_lossy(line)) {
                        Ok(ev) => events.push(ev),
                        Err(e) => self.record_error(e.to_string()),
                    }
                }
                None => break, // incomplete line, keep for next call
            }
        }
        // Keep the unconsumed partial tail in memory; the file offset always
        // points past the bytes we have already READ (not parsed), so the
        // buffered partial is never re-read from disk.
        let tail = &data[consumed..];
        if tail.len() > 4096 {
            self.oversized_line = true;
            self.partial.clear();
        } else if !self.oversized_line {
            self.partial = tail.to_vec();
        }
        self.offset += buffer.len() as u64;
        events
    }
}

// ---------------------------------------------------------------------------
// Heap timeline reconstruction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BlockState {
    pub pid: u32,
    pub allocation_id: u64,
    pub addr: u64,
    pub size: u64,
    pub alloc_type: &'static str,
    pub freed: bool,
    pub realloc_from_id: Option<u64>,
    pub alloc_seq: u64,
    pub free_seq: u64,
    /// Raw caller return address of the allocation (resolved later).
    pub alloc_caller: u64,
    /// Resolved caller location (filled by `apply_caller_locations`).
    pub caller_file: Option<String>,
    pub caller_line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapSequenceGap {
    pub pid: u32,
    pub expected_seq: u64,
    pub observed_seq: u64,
}

/// Reconstructed chronological heap state.
pub struct HeapTimeline {
    blocks: Vec<BlockState>,
    alive_by_key: HashMap<(u32, u64), usize>, // (pid, id) -> block index
    alive_by_addr: HashMap<(u32, u64), usize>, // (pid, addr) -> block index
    blocks_by_id: HashMap<(u32, u64), usize>,
    blocks_by_addr: HashMap<u32, BTreeMap<u64, Vec<usize>>>,
    live_bytes: HashMap<u32, u64>,
    events: Vec<HeapEvent>,
    /// Caller pc per stored event (parallel to `events`).
    event_callers: Vec<u64>,
    last_seq_by_pid: HashMap<u32, u64>,
    incomplete_pids: HashSet<u32>,
    /// Malformed transport has no trustworthy pid: quarantine every process,
    /// including processes first observed after the damaged line.
    transport_incomplete: bool,
    pub history_gaps: Vec<HeapSequenceGap>,
    pub sequence_gaps: u64,
    pub duplicate_events: u64,
    pub out_of_order_events: u64,
    pub event_count_total: u64,
    pub id_widening_overflow: bool,
    max_local_id: u64,
    pub max_blocks: usize,
    pub max_events: usize,
    pub blocks_capped: bool,
    pub events_capped: bool,
}

impl HeapTimeline {
    pub fn new(limits_max_blocks: usize) -> Self {
        Self {
            blocks: Vec::new(),
            alive_by_key: HashMap::new(),
            alive_by_addr: HashMap::new(),
            blocks_by_id: HashMap::new(),
            blocks_by_addr: HashMap::new(),
            live_bytes: HashMap::new(),
            events: Vec::new(),
            event_callers: Vec::new(),
            last_seq_by_pid: HashMap::new(),
            incomplete_pids: HashSet::new(),
            transport_incomplete: false,
            history_gaps: Vec::new(),
            sequence_gaps: 0,
            duplicate_events: 0,
            out_of_order_events: 0,
            event_count_total: 0,
            id_widening_overflow: false,
            max_local_id: 0,
            max_blocks: limits_max_blocks,
            max_events: limits_max_blocks.saturating_mul(4),
            blocks_capped: false,
            events_capped: false,
        }
    }

    pub fn blocks(&self) -> &[BlockState] {
        &self.blocks
    }

    pub fn events(&self) -> &[HeapEvent] {
        &self.events
    }

    /// Caller pc for each stored event (parallel to `events()`).
    pub fn event_callers(&self) -> &[u64] {
        &self.event_callers
    }

    pub fn history_complete(&self) -> bool {
        !self.transport_incomplete
            && self.incomplete_pids.is_empty()
            && !self.blocks_capped
            && !self.events_capped
            && !self.id_widening_overflow
    }

    pub fn history_reliable(&self) -> bool {
        self.history_complete()
    }

    pub fn is_pid_complete(&self, pid: u32) -> bool {
        !self.transport_incomplete && !self.incomplete_pids.contains(&pid) && !self.blocks_capped
    }

    pub fn mark_transport_incomplete(&mut self) {
        self.transport_incomplete = true;
    }

    /// Ingest one raw event, updating block state and event history.
    pub fn ingest(&mut self, ev: &RawEvent) {
        let previous = self.last_seq_by_pid.get(&ev.pid).copied().unwrap_or(0);
        if ev.seq <= previous {
            if ev.seq == previous {
                self.duplicate_events = self.duplicate_events.saturating_add(1);
            } else {
                self.out_of_order_events = self.out_of_order_events.saturating_add(1);
            }
            self.incomplete_pids.insert(ev.pid);
            return;
        }
        if ev.seq != previous.saturating_add(1) {
            self.sequence_gaps = self
                .sequence_gaps
                .saturating_add(ev.seq.saturating_sub(previous).saturating_sub(1));
            if self.history_gaps.len() < 50 {
                self.history_gaps.push(HeapSequenceGap {
                    pid: ev.pid,
                    expected_seq: previous.saturating_add(1),
                    observed_seq: ev.seq,
                });
            }
            self.incomplete_pids.insert(ev.pid);
        }
        self.last_seq_by_pid.insert(ev.pid, ev.seq);
        self.event_count_total = self.event_count_total.saturating_add(1);
        self.max_local_id = self.max_local_id.max(ev.id).max(ev.old_id);
        if self.last_seq_by_pid.len() > 1 && self.max_local_id > u32::MAX as u64 {
            self.id_widening_overflow = true;
            self.incomplete_pids.insert(ev.pid);
        }
        if self.transport_incomplete || self.incomplete_pids.contains(&ev.pid) {
            self.push_event(
                match ev.kind {
                    RawKind::Free => HeapEventType::Freed,
                    _ if matches!(ev.status, RawStatus::Failed | RawStatus::Overflow) => {
                        HeapEventType::AllocationFailed
                    }
                    RawKind::Realloc => HeapEventType::Reallocated,
                    _ => HeapEventType::Allocated,
                },
                ev,
                Some("block state not inferred".into()),
            );
            return;
        }
        let key = (ev.pid, ev.id);
        let addr_key = (ev.pid, ev.addr);
        let mut detail: Option<String> = None;

        match (ev.kind, ev.status) {
            (RawKind::Calloc, RawStatus::Overflow) => {
                self.push_event(
                    HeapEventType::AllocationFailed,
                    ev,
                    Some(
                        "calloc size product overflow; requested byte size is not representable"
                            .into(),
                    ),
                );
            }
            (RawKind::Malloc, RawStatus::Ok) | (RawKind::Calloc, RawStatus::Ok) => {
                self.add_block(
                    ev,
                    if ev.kind == RawKind::Malloc {
                        "malloc"
                    } else {
                        "calloc"
                    },
                    None,
                );
                self.push_event(HeapEventType::Allocated, ev, detail);
            }
            (RawKind::Malloc, RawStatus::Failed) | (RawKind::Calloc, RawStatus::Failed) => {
                detail = Some(
                    if ev.size == 0 {
                        "zero-size allocation returned NULL; no allocation observed"
                    } else {
                        "allocation failed (out of memory or invalid request)"
                    }
                    .into(),
                );
                self.push_event(HeapEventType::AllocationFailed, ev, detail);
            }
            (RawKind::Realloc, RawStatus::New) => {
                if ev.old_addr != 0 && ev.old_addr == ev.addr {
                    if let Some(&idx) = self.alive_by_addr.get(&(ev.pid, ev.addr)) {
                        let old_id = self.blocks[idx].allocation_id;
                        let mut emitted = ev.clone();
                        emitted.old_id = old_id;
                        if ev.id == old_id {
                            let old_size = self.blocks[idx].size;
                            self.blocks[idx].size = ev.size;
                            let usage = self.live_bytes.entry(ev.pid).or_insert(0);
                            *usage = usage.saturating_sub(old_size).saturating_add(ev.size);
                        } else {
                            self.retire_block(idx, ev.seq);
                            self.add_block(ev, "realloc", Some(old_id));
                        }
                        self.push_event(HeapEventType::Reallocated, &emitted, None);
                        return;
                    }
                }
                self.add_block(ev, "realloc", None);
                self.push_event(HeapEventType::Reallocated, ev, None);
            }
            (RawKind::Realloc, RawStatus::OkSame) => {
                if let Some(&idx) = self
                    .alive_by_key
                    .get(&key)
                    .or_else(|| self.alive_by_addr.get(&(ev.pid, ev.old_addr)))
                {
                    let old_id = self.blocks[idx].allocation_id;
                    let mut emitted = ev.clone();
                    emitted.old_id = old_id;
                    if ev.id == old_id {
                        // Historical wrapper lines reused the ID for an
                        // in-place resize; retain that serialized identity.
                        let old_size = self.blocks[idx].size;
                        self.blocks[idx].size = ev.size;
                        let usage = self.live_bytes.entry(ev.pid).or_insert(0);
                        *usage = usage.saturating_sub(old_size).saturating_add(ev.size);
                    } else {
                        self.retire_block(idx, ev.seq);
                        self.add_block(ev, "realloc", Some(old_id));
                    }
                    self.push_event(HeapEventType::Reallocated, &emitted, None);
                } else {
                    self.incomplete_pids.insert(ev.pid);
                    self.push_event(HeapEventType::Reallocated, ev, None);
                }
            }
            (RawKind::Realloc, RawStatus::OkMoved) => {
                // Retire the old block; the new block keeps a link to it.
                let old_idx = self
                    .alive_by_addr
                    .get(&(ev.pid, ev.old_addr))
                    .or_else(|| self.alive_by_key.get(&(ev.pid, ev.old_id)))
                    .copied();
                let old_id = old_idx
                    .map(|idx| self.blocks[idx].allocation_id)
                    .or((ev.old_id != 0).then_some(ev.old_id));
                if let Some(old_idx) = old_idx {
                    self.retire_block(old_idx, ev.seq);
                }
                self.add_block(ev, "realloc", old_id);
                let mut emitted = ev.clone();
                emitted.old_id = old_id.unwrap_or(0);
                self.push_event(HeapEventType::Reallocated, &emitted, None);
            }
            (RawKind::Realloc, RawStatus::Zero) => {
                // glibc: realloc(p, 0) frees p and returns NULL.
                let mut emitted = ev.clone();
                if let Some(&old_idx) = self
                    .alive_by_addr
                    .get(&(ev.pid, ev.old_addr))
                    .or_else(|| self.alive_by_key.get(&(ev.pid, ev.old_id)))
                {
                    emitted.id = self.blocks[old_idx].allocation_id;
                    emitted.old_id = emitted.id;
                    self.retire_block(old_idx, ev.seq);
                }
                detail = Some("realloc(ptr, 0): glibc freed the block and returned NULL".into());
                self.push_event(HeapEventType::Reallocated, &emitted, detail);
            }
            (RawKind::Realloc, RawStatus::Failed) => {
                // Failure preserves the original allocation — never treat
                // this as a free.
                detail = Some(
                    if ev.old_addr == 0 {
                        "realloc(NULL, n) failed: no prior allocation"
                    } else {
                        "realloc failed: original allocation preserved"
                    }
                    .into(),
                );
                self.push_event(HeapEventType::AllocationFailed, ev, detail);
            }
            (RawKind::Free, RawStatus::Ok) => {
                if let Some(&idx) = self
                    .alive_by_addr
                    .get(&addr_key)
                    .or_else(|| self.alive_by_key.get(&key))
                {
                    let mut emitted = ev.clone();
                    emitted.id = self.blocks[idx].allocation_id;
                    self.retire_block(idx, ev.seq);
                    self.push_event(HeapEventType::Freed, &emitted, None);
                } else {
                    self.incomplete_pids.insert(ev.pid);
                    self.push_event(
                        HeapEventType::Freed,
                        ev,
                        Some("free has no observed live block".into()),
                    );
                }
            }
            (RawKind::Free, RawStatus::Null) => {
                detail = Some("free(NULL) is a no-op".into());
                self.push_event(HeapEventType::Freed, ev, detail);
            }
            (RawKind::Free, RawStatus::Unknown) => {
                // Not in the C-side table. Match by address on the Rust side;
                // if the address belongs to a freed block it's a double free.
                if let Some(&idx) = self.alive_by_addr.get(&addr_key) {
                    let id = self.blocks[idx].allocation_id;
                    self.retire_block(idx, ev.seq);
                    let mut ev2 = ev.clone();
                    ev2.id = id;
                    self.push_event(HeapEventType::Freed, &ev2, None);
                    return;
                }
                detail = if !self.blocks_capped
                    && self
                        .blocks_by_addr
                        .get(&ev.pid)
                        .and_then(|addresses| addresses.get(&ev.addr))
                        .is_some_and(|indexes| indexes.iter().any(|&idx| self.blocks[idx].freed))
                {
                    Some("possible double free: address matches an observed freed block; unobserved reuse is possible".into())
                } else {
                    Some("free of an untracked address (allocated outside instrumentation, e.g. inside a shared library)".into())
                };
                self.push_event(HeapEventType::Freed, ev, detail);
            }
            // Unreachable pairs (kernel enum space) are recorded verbatim.
            _ => {
                self.push_event(
                    HeapEventType::AllocationFailed,
                    ev,
                    Some("unexpected event status".into()),
                );
            }
        }
    }

    fn retire_block(&mut self, idx: usize, seq: u64) {
        let block = &mut self.blocks[idx];
        block.freed = true;
        block.free_seq = seq;
        self.alive_by_key.remove(&(block.pid, block.allocation_id));
        self.alive_by_addr.remove(&(block.pid, block.addr));
        let usage = self.live_bytes.entry(block.pid).or_insert(0);
        *usage = usage.saturating_sub(block.size);
    }

    fn add_block(&mut self, ev: &RawEvent, alloc_type: &'static str, realloc_from: Option<u64>) {
        if ev.id == 0
            || ev.addr == 0
            || self.blocks_by_id.contains_key(&(ev.pid, ev.id))
            || self.alive_by_addr.contains_key(&(ev.pid, ev.addr))
        {
            self.incomplete_pids.insert(ev.pid);
            return;
        }
        if self.blocks.len() >= self.max_blocks {
            self.blocks_capped = true;
            self.incomplete_pids.insert(ev.pid);
            return;
        }
        let idx = self.blocks.len();
        self.blocks.push(BlockState {
            pid: ev.pid,
            allocation_id: ev.id,
            addr: ev.addr,
            size: ev.size,
            alloc_type,
            freed: false,
            realloc_from_id: realloc_from,
            alloc_seq: ev.seq,
            free_seq: 0,
            alloc_caller: ev.caller,
            caller_file: None,
            caller_line: None,
        });
        self.alive_by_key.insert((ev.pid, ev.id), idx);
        self.alive_by_addr.insert((ev.pid, ev.addr), idx);
        self.blocks_by_id.insert((ev.pid, ev.id), idx);
        self.blocks_by_addr
            .entry(ev.pid)
            .or_default()
            .entry(ev.addr)
            .or_default()
            .push(idx);
        let usage = self.live_bytes.entry(ev.pid).or_insert(0);
        *usage = usage.saturating_add(ev.size);
    }

    fn push_event(&mut self, event_type: HeapEventType, ev: &RawEvent, detail: Option<String>) {
        if self.events.len() >= self.max_events {
            self.events_capped = true;
            return;
        }
        let usage = self.usage_after_pid(ev.pid);
        let addr_str = |a: u64| -> Option<String> {
            if a == 0 {
                None
            } else {
                Some(format!("0x{a:x}"))
            }
        };
        self.event_callers.push(ev.caller);
        self.events.push(HeapEvent {
            raw_caller_address: Some(format!("0x{:x}", ev.caller)),
            sequence: ev.seq,
            pid: ev.pid,
            allocation_id: ev.id,
            event_type,
            address: addr_str(ev.addr),
            size_bytes: (ev.status != RawStatus::Overflow).then_some(ev.size),
            calloc_count: ev.calloc_count,
            calloc_element_size: ev.calloc_element_size,
            source_file: None,
            source_line: None,
            old_allocation_id: if ev.old_id != 0 {
                Some(ev.old_id)
            } else {
                None
            },
            heap_usage_after: usage,
            heap_usage_complete: Some(self.is_pid_complete(ev.pid) && !self.id_widening_overflow),
            detail: if self.transport_incomplete || self.incomplete_pids.contains(&ev.pid) {
                Some(match detail {
                    Some(detail) => {
                        format!("{detail}; allocation history incomplete for this process")
                    }
                    None => "allocation history incomplete for this process".into(),
                })
            } else {
                detail
            },
            attribution: None,
        });
    }

    fn usage_after_pid(&self, pid: u32) -> u64 {
        *self.live_bytes.get(&pid).unwrap_or(&0)
    }

    /// Blocks currently alive for `pid`, in allocation order.
    pub fn alive_blocks(&self, pid: u32) -> Vec<&BlockState> {
        if !self.is_pid_complete(pid) {
            return Vec::new();
        }
        let mut indexes: Vec<usize> = self
            .alive_by_key
            .iter()
            .filter_map(|(&(block_pid, _), &idx)| (block_pid == pid).then_some(idx))
            .collect();
        indexes.sort_unstable();
        indexes.into_iter().map(|idx| &self.blocks[idx]).collect()
    }

    /// Look up a block by (pid, local allocation id).
    pub fn block_by_id(&self, pid: u32, id: u64) -> Option<&BlockState> {
        self.blocks_by_id
            .get(&(pid, id))
            .map(|&idx| &self.blocks[idx])
    }

    /// All observed blocks containing an address, including historical blocks
    /// at a reused base. A match is address evidence, not proof of lifetime.
    pub fn address_candidates(&self, pid: u32, addr: u64) -> Vec<&BlockState> {
        let Some(bases) = self.blocks_by_addr.get(&pid) else {
            return Vec::new();
        };
        let mut indexes: Vec<usize> = bases
            .range(..=addr)
            .flat_map(|(_, entries)| entries.iter().copied())
            .filter(|&idx| addr - self.blocks[idx].addr < self.blocks[idx].size.max(1))
            .collect();
        indexes.sort_unstable();
        indexes.into_iter().map(|idx| &self.blocks[idx]).collect()
    }

    /// Distinct pids seen so far.
    pub fn pids_seen(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self.last_seq_by_pid.keys().copied().collect();
        pids.sort_unstable();
        pids.dedup();
        pids
    }

    /// Whether serialized ids must be widened with the pid (more than one
    /// allocating process observed).
    pub fn needs_pid_widening(&self) -> bool {
        self.pids_seen().len() > 1
    }

    /// Widen a (pid, local id) into the serialized global id when needed.
    pub fn global_id(&self, pid: u32, local_id: u64) -> u64 {
        if local_id == 0 {
            return 0;
        }
        if self.needs_pid_widening() {
            ((pid as u64) << 32) | local_id
        } else {
            local_id
        }
    }
}

// ---------------------------------------------------------------------------
// Caller location resolution (addr2line, batched)
// ---------------------------------------------------------------------------

/// Resolved caller location: function, file, line.
#[derive(Debug, Clone, PartialEq)]
pub struct CallerInfo {
    pub func: String,
    pub file: String,
    pub line: u32,
}

/// Resolve caller return addresses to `file:line` using ONE batched
/// `addr2line -f -C -e <binary>` invocation.
///
/// addr2line prints two lines per requested address: the function name and
/// the `file:line` pair. Addresses that cannot be resolved (no debug info,
/// foreign module) yield "??"/"?:0" and are absent from the returned map.
pub async fn resolve_caller_locations(
    binary: &Path,
    callers: &[u64],
) -> Result<HashMap<u64, CallerInfo>> {
    let mut unique: Vec<u64> = callers.to_vec();
    unique.sort_unstable();
    unique.dedup();
    let mut out = HashMap::new();
    for chunk in unique.chunks(512) {
        let mut cmd = tokio::process::Command::new("addr2line");
        cmd.args(["-f", "-C", "-e"])
            .arg(binary)
            .args(chunk.iter().map(|pc| format!("0x{pc:x}")));
        let output =
            crate::supervisor::output(&mut cmd, crate::supervisor::stage_deadline(3), 1024 * 1024)
                .await?;
        if output.timed_out || !output.status.is_some_and(|s| s.success()) {
            return Err(EngineError::Other("addr2line failed or timed out".into()));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        // Group pairwise: (func, "file:line") for each requested address.
        for (i, pc) in chunk.iter().enumerate() {
            let func_line = lines.get(i * 2).cloned().unwrap_or_default();
            let loc_line = lines.get(i * 2 + 1).cloned().unwrap_or_default();
            let (file, line_no) = split_addr2line_location(&loc_line);
            if file.is_empty() || line_no == 0 {
                continue;
            }
            out.insert(
                *pc,
                CallerInfo {
                    func: if func_line == "??" {
                        String::new()
                    } else {
                        func_line
                    },
                    file,
                    line: line_no,
                },
            );
        }
    }
    Ok(out)
}

/// Split "path/to/file.c:42" (or "file.c:42", "?:0", "file.c:?").
fn split_addr2line_location(loc: &str) -> (String, u32) {
    let loc = loc.trim();
    if loc.is_empty() || loc == "??:0" || loc == "?:0" {
        return (String::new(), 0);
    }
    match loc.rfind(':') {
        Some(pos) => {
            let file = loc[..pos].to_string();
            let line = loc[pos + 1..].trim().parse::<u32>().unwrap_or(0);
            (file, line)
        }
        None => (String::new(), 0),
    }
}

/// Apply resolved caller locations to timeline blocks and events.
///
/// Events are located at their *call site* (file:line of the allocation or
/// free), which is the information the C runtime captured per event.
pub fn apply_caller_locations(timeline: &mut HeapTimeline, resolved: &HashMap<u64, CallerInfo>) {
    for block in timeline.blocks.iter_mut() {
        if let Some(info) = resolved.get(&block.alloc_caller) {
            block.caller_file = Some(info.file.clone());
            block.caller_line = Some(info.line);
        }
    }
    for (ev, caller) in timeline
        .events
        .iter_mut()
        .zip(timeline.event_callers.iter())
    {
        if let Some(info) = resolved.get(caller) {
            ev.source_file = Some(info.file.clone());
            ev.source_line = Some(info.line);
            ev.attribution = Some(HeapEventAttribution {
                call_site_resolution: "resolved_return_address_minus_one".into(),
                call_site_function: (!info.func.is_empty()).then(|| info.func.clone()),
                checkpoint_interval: None,
                history_reliable: false,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Tests: event parsing and heap timeline reconstruction
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(line: &str) -> RawEvent {
        parse_event_line(line).expect("event line should parse")
    }

    fn timeline(lines: &[&str]) -> HeapTimeline {
        let mut t = HeapTimeline::new(10_000);
        for l in lines {
            t.ingest(&ev(l));
        }
        t
    }

    #[test]
    fn parse_event_line_malloc() {
        let e = ev("102772 1 malloc 1 7f3a2b4012a0 24 0 0 4011b6 ok");
        assert_eq!(e.pid, 102772);
        assert_eq!(e.seq, 1);
        assert_eq!(e.kind, RawKind::Malloc);
        assert_eq!(e.id, 1);
        assert_eq!(e.addr, 0x7f3a2b4012a0);
        assert_eq!(e.size, 24);
        assert_eq!(e.status, RawStatus::Ok);
        assert_eq!(e.caller, 0x4011b6);
    }

    #[test]
    fn resolved_callsite_keeps_raw_return_pc_and_function() {
        let mut t = timeline(&["100 1 malloc 1 1000 12 0 0 401000 ok"]);
        let resolved = HashMap::from([(
            0x401000,
            CallerInfo {
                func: "example".into(),
                file: "example.c".into(),
                line: 42,
            },
        )]);
        apply_caller_locations(&mut t, &resolved);
        assert_eq!(
            t.events()[0].raw_caller_address.as_deref(),
            Some("0x401000")
        );
        assert_eq!(t.events()[0].source_file.as_deref(), Some("example.c"));
        assert_eq!(t.events()[0].source_line, Some(42));
        let attribution = t.events()[0].attribution.as_ref().unwrap();
        assert_eq!(attribution.call_site_function.as_deref(), Some("example"));
        assert_eq!(
            attribution.call_site_resolution,
            "resolved_return_address_minus_one"
        );
        assert_eq!(t.blocks()[0].alloc_caller, 0x401000);
    }

    #[test]
    fn parse_event_line_realloc_moved() {
        let e = ev("102772 4 realloc 2 7f3a2b402000 64 1 7f3a2b4012a0 401230 ok_moved");
        assert_eq!(e.kind, RawKind::Realloc);
        assert_eq!(e.old_id, 1);
        assert_eq!(e.old_addr, 0x7f3a2b4012a0);
        assert_eq!(e.status, RawStatus::OkMoved);
    }

    #[test]
    fn calloc_operands_preserve_overflow_and_zero_size_evidence() {
        let overflow = ev("100 1 calloc 1 0 0 0 0 401000 overflow 18446744073709551615 4");
        assert_eq!(overflow.calloc_count, Some(u64::MAX));
        assert_eq!(overflow.calloc_element_size, Some(4));
        let zero = ev("100 2 calloc 2 1000 0 0 0 401010 ok 0 40");
        assert_eq!(zero.calloc_count, Some(0));
        let t = timeline(&[
            "100 1 calloc 1 0 0 0 0 401000 overflow 18446744073709551615 4",
            "100 2 calloc 2 1000 0 0 0 401010 ok 0 40",
        ]);
        assert_eq!(t.events()[0].size_bytes, None);
        assert_eq!(t.events()[0].calloc_count, Some(u64::MAX));
        assert_eq!(t.events()[1].size_bytes, Some(0));
        assert_eq!(t.blocks()[0].size, 0);
        assert!(t
            .events()
            .iter()
            .all(|e| e.heap_usage_complete == Some(true)));
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_event_line("not an event").is_err());
        assert!(parse_event_line("1 2 bogus 4 5 6 7 8 9 ok").is_err());
        assert!(parse_event_line("1 2 malloc 4 zz 6 7 8 9 ok").is_err());
        assert!(parse_event_line("1 2 malloc 4 5 6 7 8 9 strange").is_err());
        assert!(parse_event_line("1 2 malloc 4 5 6 7 8 9 ok 2 4").is_err());
        assert!(parse_event_line("4294967296 1 malloc 4 5 6 7 8 9 ok").is_err());
    }

    #[test]
    fn malloc_free_lifecycle() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 12 0 0 401000 ok",
            "102772 2 free 1 7f0000000000 12 0 0 401100 ok",
        ]);
        assert_eq!(t.blocks().len(), 1);
        let b = &t.blocks()[0];
        assert!(b.freed);
        assert_eq!(b.allocation_id, 1);
        assert_eq!(b.free_seq, 2);
        assert_eq!(t.events().len(), 2);
        assert_eq!(t.events()[0].event_type, HeapEventType::Allocated);
        assert_eq!(t.events()[1].event_type, HeapEventType::Freed);
        // heap usage goes 12 -> 0
        assert_eq!(t.events()[0].heap_usage_after, 12);
        assert_eq!(t.events()[1].heap_usage_after, 0);
        assert!(t.alive_blocks(102772).is_empty());
    }

    #[test]
    fn allocation_id_stable_and_reused_addresses_get_new_ids() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 12 0 0 401000 ok",
            "102772 2 free 1 7f0000000000 12 0 0 401100 ok",
            // same address reused after free -> new id
            "102772 3 malloc 2 7f0000000000 40 0 0 401000 ok",
            "102772 4 free 2 7f0000000000 40 0 0 401100 ok",
        ]);
        let blocks = t.blocks();
        assert_eq!(blocks.len(), 2, "two distinct allocation records");
        assert_ne!(blocks[0].allocation_id, blocks[1].allocation_id);
        assert_eq!(blocks[0].addr, blocks[1].addr, "same address, new identity");
        assert!(blocks[0].freed && blocks[1].freed);
    }

    #[test]
    fn legacy_realloc_in_place_keeps_id_and_updates_size() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 12 0 0 401000 ok",
            "102772 2 realloc 1 7f0000000000 48 1 7f0000000000 401010 ok_same",
        ]);
        assert_eq!(t.blocks().len(), 1, "same block object");
        let b = &t.blocks()[0];
        assert_eq!(b.allocation_id, 1, "id stable through in-place realloc");
        assert_eq!(b.size, 48);
        assert!(!b.freed);
        assert_eq!(t.events()[1].event_type, HeapEventType::Reallocated);
        assert_eq!(t.events()[1].heap_usage_after, 48);
    }

    #[test]
    fn realloc_in_place_creates_a_new_linked_lifetime() {
        let t = timeline(&[
            "100 1 malloc 1 1000 12 0 0 401000 ok",
            "100 2 realloc 2 1000 48 1 1000 401010 ok_same",
            "100 3 realloc 2 0 18446744073709551615 2 1000 401020 failed",
            "100 4 realloc 3 2000 96 2 1000 401030 ok_moved",
        ]);
        assert!(t.history_complete());
        assert_eq!(t.blocks().len(), 3);
        let initial = t.block_by_id(100, 1).unwrap();
        let same = t.block_by_id(100, 2).unwrap();
        let moved = t.block_by_id(100, 3).unwrap();
        assert_eq!(initial.addr, same.addr);
        assert!(initial.freed && same.freed);
        assert_eq!(initial.free_seq, 2);
        assert_eq!(same.free_seq, 4);
        assert_eq!(same.realloc_from_id, Some(1));
        assert_eq!(moved.realloc_from_id, Some(2));
        assert!(!moved.freed);
        assert_eq!(t.events()[1].allocation_id, 2);
        assert_eq!(t.events()[1].old_allocation_id, Some(1));
        assert_eq!(t.events()[1].heap_usage_after, 48);
        assert_eq!(t.events()[2].heap_usage_after, 48);
        assert_eq!(t.events()[3].heap_usage_after, 96);
        assert_eq!(t.address_candidates(100, 0x1000).len(), 2);
    }

    #[test]
    fn realloc_move_retires_old_block_and_links_new() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 12 0 0 401000 ok",
            "102772 2 realloc 2 7f0000000004 96 1 7f0000000000 401010 ok_moved",
        ]);
        assert_eq!(t.blocks().len(), 2);
        let old = t.block_by_id(102772, 1).unwrap();
        let new = t.block_by_id(102772, 2).unwrap();
        assert!(old.freed);
        assert!(!new.freed);
        assert_eq!(new.realloc_from_id, Some(1));
        assert_eq!(t.events()[1].event_type, HeapEventType::Reallocated);
        assert_eq!(t.events()[1].old_allocation_id, Some(1));
        assert_eq!(t.events()[1].heap_usage_after, 96);
    }

    #[test]
    fn realloc_null_behaves_like_malloc() {
        let t = timeline(&["102772 1 realloc 1 7f0000000000 32 0 0 401000 new"]);
        assert_eq!(t.blocks().len(), 1);
        assert_eq!(t.blocks()[0].alloc_type, "realloc");
        assert_eq!(t.events()[0].event_type, HeapEventType::Reallocated);
    }

    #[test]
    fn failed_realloc_preserves_original() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 12 0 0 401000 ok",
            // huge requested size: returns NULL, original must survive
            "102772 2 realloc 1 0 18446744073709551615 1 7f0000000000 401010 failed",
        ]);
        let b = t.block_by_id(102772, 1).unwrap();
        assert!(!b.freed, "failed realloc must NOT be treated as a free");
        assert_eq!(b.size, 12, "original size preserved");
        assert_eq!(t.events()[1].event_type, HeapEventType::AllocationFailed);
        assert_eq!(t.events()[1].heap_usage_after, 12);
        assert_eq!(t.alive_blocks(102772).len(), 1);
    }

    #[test]
    fn realloc_zero_frees_on_glibc() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 12 0 0 401000 ok",
            "102772 2 realloc 1 0 0 1 7f0000000000 401010 zero",
        ]);
        let b = t.block_by_id(102772, 1).unwrap();
        assert!(b.freed, "glibc realloc(p, 0) frees p");
        assert_eq!(t.events()[1].heap_usage_after, 0);
    }

    #[test]
    fn failed_malloc_recorded() {
        let t = timeline(&["102772 1 malloc 1 0 999999999999 0 0 401000 failed"]);
        assert!(t.blocks().is_empty(), "failed allocation creates no block");
        assert_eq!(t.events().len(), 1);
        assert_eq!(t.events()[0].event_type, HeapEventType::AllocationFailed);
    }

    #[test]
    fn calloc_records_total_size() {
        let t = timeline(&["102772 1 calloc 1 7f0000000000 40 0 0 401000 ok"]);
        assert_eq!(t.blocks()[0].size, 40);
        assert_eq!(t.blocks()[0].alloc_type, "calloc");
    }

    #[test]
    fn double_free_detected() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 12 0 0 401000 ok",
            "102772 2 free 1 7f0000000000 12 0 0 401100 ok",
            "102772 3 free 0 7f0000000000 0 0 0 401200 unknown",
        ]);
        let last = t.events().last().unwrap();
        assert_eq!(last.event_type, HeapEventType::Freed);
        assert!(last
            .detail
            .as_deref()
            .unwrap()
            .contains("possible double free"));
    }

    #[test]
    fn untracked_free_recorded_with_zero_id() {
        let t = timeline(&["102772 1 free 0 7f0000000abc 0 0 0 401100 unknown"]);
        let e = t.events().last().unwrap();
        assert_eq!(e.allocation_id, 0, "0 = no tracked allocation");
        assert!(e.detail.as_deref().unwrap().contains("untracked"));
    }

    #[test]
    fn free_null_is_noop() {
        let t = timeline(&["102772 1 free 0 0 0 0 0 401100 null"]);
        let e = t.events().last().unwrap();
        assert_eq!(e.event_type, HeapEventType::Freed);
        assert_eq!(e.detail.as_deref(), Some("free(NULL) is a no-op"));
        assert_eq!(e.heap_usage_after, 0);
    }

    #[test]
    fn multiple_allocations_keep_ordering_and_usage() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 10 0 0 401000 ok",
            "102772 2 malloc 2 7f0000000010 20 0 0 401001 ok",
            "102772 3 malloc 3 7f0000000030 30 0 0 401002 ok",
            "102772 4 free 2 7f0000000010 20 0 0 401003 ok",
        ]);
        let seqs: Vec<u64> = t.events().iter().map(|e| e.sequence).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4], "chronological order preserved");
        assert_eq!(t.events()[2].heap_usage_after, 60);
        assert_eq!(t.events()[3].heap_usage_after, 40);
        assert_eq!(t.alive_blocks(102772).len(), 2);
    }

    #[test]
    fn block_cap_marks_flag() {
        let mut t = HeapTimeline::new(2);
        for i in 1..=5u64 {
            let line = format!(
                "102772 {} malloc {} 7f00000000{:02x} 10 0 0 401000 ok",
                i,
                i,
                i * 16
            );
            t.ingest(&ev(&line));
        }
        assert_eq!(t.blocks().len(), 2);
        assert!(t.blocks_capped);
        assert!(
            t.events().len() >= 2,
            "events still recorded past block cap"
        );
    }

    #[test]
    fn global_id_widening_only_when_multi_pid() {
        let t = timeline(&["102772 1 malloc 1 7f0000000000 10 0 0 401000 ok"]);
        assert!(!t.needs_pid_widening());
        assert_eq!(t.global_id(102772, 1), 1);

        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 10 0 0 401000 ok",
            "102900 1 malloc 1 7f0000000010 10 0 0 401000 ok",
        ]);
        assert!(t.needs_pid_widening());
        assert_eq!(t.global_id(102772, 1), (102772u64 << 32) | 1);
        assert_ne!(t.global_id(102772, 1), t.global_id(102900, 1));
        assert_eq!(
            t.global_id(102900, 0),
            0,
            "untracked ID stays sentinel zero"
        );
    }

    #[test]
    fn interleaved_process_sequences_are_independent() {
        let t = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "200 1 malloc 1 2000 20 0 0 402000 ok",
            "100 2 free 1 1000 10 0 0 401100 ok",
            "200 2 free 1 2000 20 0 0 402100 ok",
        ]);
        assert!(t.history_complete());
        assert_eq!(t.sequence_gaps, 0);
        assert_eq!(t.event_count_total, 4);
    }

    #[test]
    fn first_event_after_missing_prefix_does_not_invent_prior_blocks() {
        let t = timeline(&["100 4 free 3 1000 12 0 0 401000 ok"]);
        assert_eq!(t.sequence_gaps, 3);
        assert_eq!(t.history_gaps[0].expected_seq, 1);
        assert_eq!(t.history_gaps[0].observed_seq, 4);
        assert!(t.blocks().is_empty());
        assert_eq!(t.event_count_total, 1);
        assert_eq!(t.events()[0].heap_usage_complete, Some(false));
    }

    #[test]
    fn missing_sequence_quarantines_only_affected_process() {
        let t = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "200 1 malloc 1 2000 20 0 0 402000 ok",
            "100 3 free 1 1000 10 0 0 401100 ok",
            "100 4 malloc 2 1010 40 0 0 401200 ok",
            "200 2 free 1 2000 20 0 0 402100 ok",
        ]);
        assert_eq!(
            t.history_gaps,
            vec![HeapSequenceGap {
                pid: 100,
                expected_seq: 2,
                observed_seq: 3
            }]
        );
        assert_eq!(t.sequence_gaps, 1);
        assert!(!t.history_complete());
        assert!(!t.is_pid_complete(100));
        assert!(t.is_pid_complete(200));
        assert!(
            t.alive_blocks(100).is_empty(),
            "unknown lifetime cannot be reported alive"
        );
        assert_eq!(
            t.blocks().len(),
            2,
            "no blocks inferred after missing event"
        );
        assert_eq!(
            t.events()[2].heap_usage_after,
            10,
            "last known usage is not adjusted"
        );
        assert!(t.events()[2]
            .detail
            .as_deref()
            .unwrap()
            .contains("incomplete"));
        assert_eq!(t.events()[2].heap_usage_complete, Some(false));
        assert!(
            t.blocks()[0].free_seq == 0,
            "a missed event could have freed/reused it"
        );
    }

    #[test]
    fn malformed_transport_quarantines_existing_and_future_heap_lifetimes() {
        let mut timeline = HeapTimeline::new(8);
        timeline.ingest(&ev("100 1 malloc 1 1000 8 0 0 401000 ok"));
        assert_eq!(timeline.alive_blocks(100).len(), 1);
        timeline.mark_transport_incomplete();
        timeline.ingest(&ev("100 2 free 1 1000 8 0 0 401000 ok"));
        timeline.ingest(&ev("200 1 malloc 1 2000 16 0 0 401000 ok"));
        assert!(!timeline.history_complete());
        assert!(timeline.alive_blocks(100).is_empty());
        assert!(timeline.alive_blocks(200).is_empty());
        assert!(
            !timeline.blocks()[0].freed,
            "old observation stays historical"
        );
        assert_eq!(timeline.blocks().len(), 1, "no new block reconstructed");
        assert!(timeline.events()[1..]
            .iter()
            .all(|e| e.heap_usage_complete == Some(false)));
    }

    #[test]
    fn unfinished_line_is_buffered_but_malformed_line_is_definitive_loss() {
        let path = std::env::temp_dir().join(format!(
            "heap-event-reader-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, b"100 1 malloc 1 1000 8 0 0 401000 ok\nbad").unwrap();
        let mut reader = EventFileReader::new(path.clone());
        assert_eq!(reader.read_new().len(), 1);
        assert!(reader.incomplete_line());
        assert!(!reader.transport_damaged());
        use std::io::Write as _;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b" event\n")
            .unwrap();
        assert!(reader.read_new().is_empty());
        assert!(!reader.incomplete_line());
        assert!(reader.transport_damaged());
        assert_eq!(reader.parse_error_count, 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn oversized_log_segment_keeps_bounded_prefix_and_marks_permanent_loss() {
        let path = std::env::temp_dir().join(format!(
            "heap-event-cap-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let prefix = b"100 1 malloc 1 1000 8 0 0 401000 ok\n";
        let mut input = Vec::with_capacity(MAX_EVENT_READ_BYTES as usize + 2);
        input.extend_from_slice(prefix);
        input.resize(MAX_EVENT_READ_BYTES as usize + 2, b' ');
        std::fs::write(&path, input).unwrap();
        let mut reader = EventFileReader::new(path.clone());
        let events = reader.read_new();
        assert_eq!(events.len(), 1);
        assert!(reader.transport_damaged());
        assert_eq!(reader.parse_error_count, 1);
        assert!(reader.read_new().is_empty(), "never retry a lost segment");
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn equal_sized_replacement_of_observed_event_log_is_transport_loss() {
        let path = std::env::temp_dir().join(format!(
            "heap-replaced-log-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let replacement = path.with_extension("replacement");
        let line = b"100 1 malloc 1 1000 8 0 0 401000 ok\n";
        std::fs::write(&path, line).unwrap();
        let mut reader = EventFileReader::new(path.clone());
        assert_eq!(reader.read_new().len(), 1);
        std::fs::write(&replacement, line).unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        assert!(reader.read_new().is_empty());
        assert!(reader.transport_damaged());
        assert_eq!(reader.parse_error_count, 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn duplicate_and_out_of_order_events_do_not_mutate_blocks() {
        let t = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "100 2 realloc 1 1000 40 1 1000 401000 ok_same",
            "100 2 realloc 1 1000 80 1 1000 401000 ok_same",
            "100 1 free 1 1000 10 0 0 401000 ok",
            "100 3 free 1 1000 40 0 0 401000 ok",
        ]);
        assert_eq!(t.duplicate_events, 1);
        assert_eq!(t.out_of_order_events, 1);
        assert_eq!(t.event_count_total, 3);
        assert_eq!(t.events().len(), 3);
        assert_eq!(t.blocks()[0].size, 40);
        assert!(!t.blocks()[0].freed);
        assert!(!t.history_reliable());
    }

    #[test]
    fn capped_history_counts_unique_events_without_unbounded_retention() {
        let mut t = HeapTimeline::new(1);
        for i in 1..=100 {
            t.ingest(&ev(&format!("100 {i} free 0 0 0 0 0 401000 null")));
        }
        assert_eq!(t.events().len(), 4);
        assert_eq!(t.event_callers().len(), 4);
        assert_eq!(t.event_count_total, 100);
        assert!(t.events_capped);
        assert!(!t.history_complete());
    }

    #[test]
    fn capped_block_history_does_not_claim_stale_address_is_live() {
        let t = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "100 2 free 1 1000 10 0 0 401000 ok",
            "100 3 malloc 2 1000 20 0 0 401000 ok",
        ]);
        let matches = t.address_candidates(100, 0x1005);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].allocation_id, 1);
        assert_eq!(matches[1].allocation_id, 2);
        assert!(t.address_candidates(100, 0x1020).is_empty());

        let mut capped = HeapTimeline::new(1);
        for line in [
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "100 2 free 1 1000 10 0 0 401000 ok",
            "100 3 malloc 2 1000 20 0 0 401000 ok",
        ] {
            capped.ingest(&ev(line));
        }
        assert!(capped.blocks_capped);
        assert!(capped.alive_blocks(100).is_empty());
        assert_eq!(capped.address_candidates(100, 0x1000).len(), 1);
        assert!(!capped.is_pid_complete(100));
    }

    #[test]
    fn realloc_table_overflow_fallback_matches_old_address() {
        let moved = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "100 2 realloc 2 2000 20 0 1000 401010 ok_moved",
        ]);
        assert!(moved.blocks()[0].freed);
        assert_eq!(moved.blocks()[1].realloc_from_id, Some(1));
        assert_eq!(moved.events()[1].old_allocation_id, Some(1));
        assert_eq!(moved.events()[1].heap_usage_after, 20);

        let same = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "100 2 realloc 2 1000 30 0 1000 401010 new",
            "100 3 free 2 1000 30 0 0 401020 ok",
        ]);
        assert_eq!(same.blocks().len(), 2);
        assert_eq!(same.blocks()[0].size, 10);
        assert_eq!(same.blocks()[1].size, 30);
        assert_eq!(same.blocks()[1].realloc_from_id, Some(1));
        assert_eq!(same.events()[1].allocation_id, 2);
        assert_eq!(same.events()[1].old_allocation_id, Some(1));
        assert!(same.blocks().iter().all(|b| b.freed));

        let zero = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "100 2 realloc 0 0 0 0 1000 401010 zero",
        ]);
        assert!(zero.blocks()[0].freed);
        assert_eq!(zero.events()[1].heap_usage_after, 0);
        assert_eq!(zero.events()[1].allocation_id, 1);
        assert_eq!(zero.events()[1].old_allocation_id, Some(1));
    }

    #[test]
    fn failed_only_process_still_requires_pid_widening() {
        let t = timeline(&[
            "100 1 malloc 1 1000 10 0 0 401000 ok",
            "200 1 malloc 1 0 999 0 0 402000 failed",
        ]);
        assert!(t.needs_pid_widening());
        assert_eq!(t.pids_seen(), vec![100, 200]);
        assert_ne!(t.global_id(100, 1), t.global_id(200, 1));
        assert_eq!(t.global_id(200, 0), 0);
    }

    #[test]
    fn oversized_local_id_in_multi_pid_trace_flags_nonunique_serialization() {
        let t = timeline(&[
            "100 1 malloc 4294967296 1000 10 0 0 401000 ok",
            "200 1 malloc 1 2000 20 0 0 402000 ok",
        ]);
        assert!(t.id_widening_overflow);
        assert!(!t.history_complete());
        assert_eq!(t.global_id(200, 0), 0);
    }

    #[test]
    fn partial_line_is_buffered_not_lost() {
        let dir = std::env::temp_dir().join(format!("cte-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events_partial.txt");
        std::fs::write(
            &path,
            b"102772 1 malloc 1 7f00 12 0 0 401000 ok\n102772 2 m",
        )
        .unwrap();
        let mut reader = EventFileReader::new(path.clone());
        let first = reader.read_new();
        assert_eq!(first.len(), 1);
        // complete the second event
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"alloc 2 7f10 20 0 0 401000 ok\n").unwrap();
        let second = reader.read_new();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].seq, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_partial_line_is_bounded_and_errors_counted() {
        let dir = std::env::temp_dir().join(format!("cte-oversized-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events.txt");
        std::fs::write(&path, vec![b'x'; 5000]).unwrap();
        let mut reader = EventFileReader::new(path.clone());
        assert!(reader.read_new().is_empty());
        assert!(reader.incomplete_line());
        assert!(reader.partial.is_empty());
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"\n100 1 free 0 0 0 0 0 401000 null\n")
            .unwrap();
        assert_eq!(reader.read_new().len(), 1);
        assert_eq!(reader.parse_error_count, 1);
        assert!(!reader.incomplete_line());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_error_diagnostics_are_bounded_but_total_is_exact() {
        let dir = std::env::temp_dir().join(format!("cte-errors-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events.txt");
        std::fs::write(&path, "not-an-event\n".repeat(60)).unwrap();
        let mut reader = EventFileReader::new(path.clone());
        assert!(reader.read_new().is_empty());
        assert_eq!(reader.parse_errors.len(), 50);
        assert_eq!(reader.parse_error_count, 60);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

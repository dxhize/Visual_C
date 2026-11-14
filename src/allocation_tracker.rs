//! Custom C allocation instrumentation runtime and Rust-side event
//! processing.
//!
//! The C runtime (`c_runtime/alloc_tracker.c`, embedded with `include_str!`)
//! is linked into the debug binary with `-Wl,--wrap=malloc|calloc|realloc|free`
//! and appends one event line per allocation operation to a dedicated file.
//! This module:
//!
//! * tails that file incrementally (the inferior is *stopped* whenever we
//!   read, so the file content is exactly the event history up to the
//!   current stop),
//! * parses the event format into [`RawEvent`]s,
//! * reconstructs the chronological heap timeline: live blocks, freed
//!   blocks, lifetimes, realloc chains, per-event heap usage,
//! * resolves caller return addresses to `file:line` with a single batched
//!   `addr2line` invocation at the end of the run.
//!
//! Allocation ids are per-process counters; `(pid, id)` is the unique key
//! internally. When more than one pid appears (forked children), serialized
//! ids are widened to `(pid << 32) | id` at finalization so the trace never
//! contains a duplicate id (see `HeapTimeline::finalize`).

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{EngineError, Result};
use crate::models::{HeapEvent, HeapEventType};

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
}

/// Parse one event line
/// (`pid seq kind id addr size old_id old_addr caller status`).
pub fn parse_event_line(line: &str) -> Result<RawEvent> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 10 {
        return Err(crate::error::EngineError::AllocationEventParse {
            line: 0,
            message: format!("expected 10 fields, found {}", fields.len()),
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
    let status = match fields[9] {
        "ok" => RawStatus::Ok,
        "failed" => RawStatus::Failed,
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
        pid: num(0)?.min(u32::MAX as u64) as u32,
        seq: num(1)?,
        kind,
        id: num(3)?,
        addr: hex(4)?,
        size: num(5)?,
        old_id: num(6)?,
        old_addr: hex(7)?,
        caller: hex(8)?,
        status,
    })
}

/// Incremental reader for the event trace file.
pub struct EventFileReader {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    pub parse_errors: Vec<String>,
}

impl EventFileReader {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: 0,
            partial: Vec::new(),
            parse_errors: Vec::new(),
        }
    }

    /// Read and parse all events appended since the last call. Partial
    /// trailing lines (crash mid-write) are buffered until complete.
    pub fn read_new(&mut self) -> Vec<RawEvent> {
        let mut events = Vec::new();
        let file = match File::open(&self.path) {
            Ok(f) => f,
            Err(_) => return events, // not created yet: inferior not started
        };
        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.offset)).is_err() {
            return events;
        }
        let mut buffer = Vec::new();
        // The event file is bounded by the program's allocation count and
        // the engine's caps; read everything appended since last time.
        if reader.read_to_end(&mut buffer).is_err() {
            return events;
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
                    if line.is_empty() {
                        continue;
                    }
                    match parse_event_line(&String::from_utf8_lossy(line)) {
                        Ok(ev) => events.push(ev),
                        Err(e) => {
                            if self.parse_errors.len() < 50 {
                                self.parse_errors.push(e.to_string());
                            }
                        }
                    }
                }
                None => break, // incomplete line, keep for next call
            }
        }
        // Keep the unconsumed partial tail in memory; the file offset always
        // points past the bytes we have already READ (not parsed), so the
        // buffered partial is never re-read from disk.
        self.partial = data[consumed..].to_vec();
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

/// Reconstructed chronological heap state.
pub struct HeapTimeline {
    blocks: Vec<BlockState>,
    alive_by_key: HashMap<(u32, u64), usize>,  // (pid, id) -> block index
    alive_by_addr: HashMap<(u32, u64), usize>, // (pid, addr) -> block index
    live_bytes: HashMap<u32, u64>,
    events: Vec<HeapEvent>,
    /// Caller pc per stored event (parallel to `events`).
    event_callers: Vec<u64>,
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
            live_bytes: HashMap::new(),
            events: Vec::new(),
            event_callers: Vec::new(),
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

    /// Ingest one raw event, updating block state and event history.
    pub fn ingest(&mut self, ev: &RawEvent) {
        let key = (ev.pid, ev.id);
        let addr_key = (ev.pid, ev.addr);
        let mut detail: Option<String> = None;

        match (ev.kind, ev.status) {
            (RawKind::Malloc, RawStatus::Ok) | (RawKind::Calloc, RawStatus::Ok) => {
                self.add_block(ev, if ev.kind == RawKind::Malloc { "malloc" } else { "calloc" }, None);
                self.push_event(HeapEventType::Allocated, ev, detail);
            }
            (RawKind::Malloc, RawStatus::Failed) | (RawKind::Calloc, RawStatus::Failed) => {
                detail = Some("allocation failed (out of memory or invalid request)".into());
                self.push_event(HeapEventType::AllocationFailed, ev, detail);
            }
            (RawKind::Realloc, RawStatus::New) => {
                self.add_block(ev, "realloc", None);
                self.push_event(HeapEventType::Reallocated, ev, None);
            }
            (RawKind::Realloc, RawStatus::OkSame) => {
                if let Some(&idx) = self.alive_by_key.get(&key) {
                    let old_size = self.blocks[idx].size;
                    self.blocks[idx].size = ev.size;
                    let usage = self.live_bytes.entry(ev.pid).or_insert(0);
                    *usage = usage.saturating_sub(old_size).saturating_add(ev.size);
                }
                self.push_event(HeapEventType::Reallocated, ev, None);
            }
            (RawKind::Realloc, RawStatus::OkMoved) => {
                // Retire the old block; the new block keeps a link to it.
                if ev.old_id != 0 {
                    if let Some(&old_idx) = self.alive_by_key.get(&(ev.pid, ev.old_id)) {
                        self.blocks[old_idx].freed = true;
                        self.blocks[old_idx].free_seq = ev.seq;
                        self.alive_by_key.remove(&(ev.pid, ev.old_id));
                        self.alive_by_addr.remove(&(ev.pid, ev.old_addr));
                        let usage = self.live_bytes.entry(ev.pid).or_insert(0);
                        *usage = usage.saturating_sub(self.blocks[old_idx].size);
                    }
                }
                self.add_block(ev, "realloc", Some(ev.old_id));
                self.push_event(HeapEventType::Reallocated, ev, None);
            }
            (RawKind::Realloc, RawStatus::Zero) => {
                // glibc: realloc(p, 0) frees p and returns NULL.
                if let Some(&old_idx) = self.alive_by_key.get(&(ev.pid, ev.old_id)) {
                    self.blocks[old_idx].freed = true;
                    self.blocks[old_idx].free_seq = ev.seq;
                    self.alive_by_key.remove(&(ev.pid, ev.old_id));
                    self.alive_by_addr.remove(&(ev.pid, ev.old_addr));
                    let usage = self.live_bytes.entry(ev.pid).or_insert(0);
                    *usage = usage.saturating_sub(self.blocks[old_idx].size);
                }
                detail = Some("realloc(ptr, 0): glibc freed the block and returned NULL".into());
                self.push_event(HeapEventType::Reallocated, ev, detail);
            }
            (RawKind::Realloc, RawStatus::Failed) => {
                // Failure preserves the original allocation — never treat
                // this as a free.
                detail = Some("realloc failed: original allocation preserved".into());
                self.push_event(HeapEventType::AllocationFailed, ev, detail);
            }
            (RawKind::Free, RawStatus::Ok) => {
                if let Some(&idx) = self.alive_by_key.get(&key) {
                    self.blocks[idx].freed = true;
                    self.blocks[idx].free_seq = ev.seq;
                    self.alive_by_key.remove(&key);
                    self.alive_by_addr.remove(&addr_key);
                    let usage = self.live_bytes.entry(ev.pid).or_insert(0);
                    *usage = usage.saturating_sub(self.blocks[idx].size);
                }
                self.push_event(HeapEventType::Freed, ev, None);
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
                    self.blocks[idx].freed = true;
                    self.blocks[idx].free_seq = ev.seq;
                    self.alive_by_key.remove(&(ev.pid, id));
                    self.alive_by_addr.remove(&addr_key);
                    let usage = self.live_bytes.entry(ev.pid).or_insert(0);
                    *usage = usage.saturating_sub(self.blocks[idx].size);
                    let mut ev2 = ev.clone();
                    ev2.id = id;
                    self.push_event(HeapEventType::Freed, &ev2, None);
                    return;
                }
                detail = if self.blocks.iter().any(|b| b.pid == ev.pid && b.addr == ev.addr && b.freed) {
                    Some("double free detected".into())
                } else {
                    Some("free of an untracked address (allocated outside instrumentation, e.g. inside a shared library)".into())
                };
                self.push_event(HeapEventType::Freed, ev, detail);
            }
            // Unreachable pairs (kernel enum space) are recorded verbatim.
            _ => {
                self.push_event(HeapEventType::AllocationFailed, ev, Some("unexpected event status".into()));
            }
        }
    }

    fn add_block(&mut self, ev: &RawEvent, alloc_type: &'static str, realloc_from: Option<u64>) {
        if self.blocks.len() >= self.max_blocks {
            self.blocks_capped = true;
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
            sequence: ev.seq,
            pid: ev.pid,
            allocation_id: ev.id,
            event_type,
            address: addr_str(ev.addr),
            size_bytes: Some(ev.size),
            source_file: None,
            source_line: None,
            old_allocation_id: if ev.old_id != 0 { Some(ev.old_id) } else { None },
            heap_usage_after: usage,
            detail,
        });
    }

    fn usage_after_pid(&self, pid: u32) -> u64 {
        *self.live_bytes.get(&pid).unwrap_or(&0)
    }

    /// Blocks currently alive for `pid`, in allocation order.
    pub fn alive_blocks(&self, pid: u32) -> Vec<&BlockState> {
        self.blocks
            .iter()
            .filter(|b| b.pid == pid && !b.freed)
            .collect()
    }

    /// Look up a block by (pid, local allocation id).
    pub fn block_by_id(&self, pid: u32, id: u64) -> Option<&BlockState> {
        self.blocks.iter().find(|b| b.pid == pid && b.allocation_id == id)
    }

    /// Distinct pids seen so far.
    pub fn pids_seen(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self.blocks.iter().map(|b| b.pid).collect();
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
    let mut out = HashMap::new();
    if callers.is_empty() {
        return Ok(out);
    }
    let mut unique: Vec<u64> = callers.to_vec();
    unique.sort_unstable();
    unique.dedup();

    let mut child = tokio::process::Command::new("addr2line")        .args(["-f", "-C", "-e"])
        .arg(binary)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| crate::error::EngineError::MissingTool {
            tool: "addr2line".into(),
            reason: format!("could not spawn addr2line: {e}"),
        })?;

    let mut stdin = child.stdin.take().expect("addr2line stdin");
    let stdout = child.stdout.take().expect("addr2line stdout");
    const ADDR2LINE_TIMEOUT: Duration = Duration::from_secs(30);

    let request: String = unique.iter().map(|pc| format!("0x{pc:x}\n")).collect();
    // Stage 1: send the request, then DROP the stdin handle — dropping is the
    // only reliable way to signal EOF (`ChildStdin::shutdown()` only flushes
    // and does not close the descriptor, so addr2line would wait forever).
    let written = tokio::time::timeout(ADDR2LINE_TIMEOUT, async {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(request.as_bytes()).await
    })
    .await;
    match written {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(EngineError::Other(format!("writing to addr2line failed: {e}"))),
        Err(_) => {
            let _ = child.kill().await;
            return Err(EngineError::Other(
                "addr2line did not accept input within 30s".into(),
            ));
        }
    }
    drop(stdin);

    // Stage 2: read every answer to EOF, then reap the child.
    let read = tokio::time::timeout(ADDR2LINE_TIMEOUT, async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = Vec::new();
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            lines.push(line.trim_end_matches('\n').to_string());
        }
        lines
    })
    .await;
    let lines = match read {
        Ok(lines) => lines,
        Err(_) => {
            let _ = child.kill().await;
            return Err(EngineError::Other(
                "addr2line did not answer within 30s".into(),
            ));
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;

    // Group pairwise: (func, "file:line") for each requested address.
    for (i, pc) in unique.iter().enumerate() {
        let func_line = lines.get(i * 2).cloned().unwrap_or_default();
        let loc_line = lines.get(i * 2 + 1).cloned().unwrap_or_default();
        let (file, line_no) = split_addr2line_location(&loc_line);
        if file.is_empty() || line_no == 0 {
            continue;
        }
        out.insert(
            *pc,
            CallerInfo {
                func: if func_line == "??" { String::new() } else { func_line },
                file,
                line: line_no,
            },
        );
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
pub fn apply_caller_locations(
    timeline: &mut HeapTimeline,
    resolved: &HashMap<u64, CallerInfo>,
) {
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
    fn parse_event_line_realloc_moved() {
        let e = ev("102772 4 realloc 2 7f3a2b402000 64 1 7f3a2b4012a0 401230 ok_moved");
        assert_eq!(e.kind, RawKind::Realloc);
        assert_eq!(e.old_id, 1);
        assert_eq!(e.old_addr, 0x7f3a2b4012a0);
        assert_eq!(e.status, RawStatus::OkMoved);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_event_line("not an event").is_err());
        assert!(parse_event_line("1 2 bogus 4 5 6 7 8 9 ok").is_err());
        assert!(parse_event_line("1 2 malloc 4 zz 6 7 8 9 ok").is_err());
        assert!(parse_event_line("1 2 malloc 4 5 6 7 8 9 strange").is_err());
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
    fn realloc_in_place_keeps_id_and_updates_size() {
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
        let t = timeline(&[
            "102772 1 realloc 1 7f0000000000 32 0 0 401000 new",
        ]);
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
        let t = timeline(&[
            "102772 1 malloc 1 0 999999999999 0 0 401000 failed",
        ]);
        assert!(t.blocks().is_empty(), "failed allocation creates no block");
        assert_eq!(t.events().len(), 1);
        assert_eq!(t.events()[0].event_type, HeapEventType::AllocationFailed);
    }

    #[test]
    fn calloc_records_total_size() {
        let t = timeline(&[
            "102772 1 calloc 1 7f0000000000 40 0 0 401000 ok",
        ]);
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
        assert_eq!(last.detail.as_deref(), Some("double free detected"));
    }

    #[test]
    fn untracked_free_recorded_with_zero_id() {
        let t = timeline(&[
            "102772 1 free 0 7f0000000abc 0 0 0 401100 unknown",
        ]);
        let e = t.events().last().unwrap();
        assert_eq!(e.allocation_id, 0, "0 = no tracked allocation");
        assert!(e.detail.as_deref().unwrap().contains("untracked"));
    }

    #[test]
    fn free_null_is_noop() {
        let t = timeline(&[
            "102772 1 free 0 0 0 0 0 401100 null",
        ]);
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
            let line = format!("102772 {} malloc {} 7f00000000{:02x} 10 0 0 401000 ok", i, i, i * 16);
            t.ingest(&ev(&line));
        }
        assert_eq!(t.blocks().len(), 2);
        assert!(t.blocks_capped);
        assert!(t.events().len() >= 2, "events still recorded past block cap");
    }

    #[test]
    fn global_id_widening_only_when_multi_pid() {
        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 10 0 0 401000 ok",
        ]);
        assert!(!t.needs_pid_widening());
        assert_eq!(t.global_id(102772, 1), 1);

        let t = timeline(&[
            "102772 1 malloc 1 7f0000000000 10 0 0 401000 ok",
            "102900 1 malloc 1 7f0000000010 10 0 0 401000 ok",
        ]);
        assert!(t.needs_pid_widening());
        assert_eq!(t.global_id(102772, 1), (102772u64 << 32) | 1);
        assert_ne!(t.global_id(102772, 1), t.global_id(102900, 1));
    }

    #[test]
    fn partial_line_is_buffered_not_lost() {
        let dir = std::env::temp_dir().join(format!("cte-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events_partial.txt");
        std::fs::write(&path, b"102772 1 malloc 1 7f00 12 0 0 401000 ok\n102772 2 m").unwrap();
        let mut reader = EventFileReader::new(path.clone());
        let first = reader.read_new();
        assert_eq!(first.len(), 1);
        // complete the second event
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"alloc 2 7f10 20 0 0 401000 ok\n").unwrap();
        let second = reader.read_new();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].seq, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! TP-2: compact indexes over the canonical [`DebugSessionTrace`].
//!
//! The indexes hold **positions only** — never copies of events, snapshots,
//! disassembly records, heap events, or their strings (source paths are
//! interned once per distinct recorded path; everything else is a `u32`
//! position into a canonical array). Lookups that are already constant-time
//! vector accesses (`event_id` → `instruction_events`, `instruction_id` →
//! `disassembly_table`, snapshot index → `snapshots`,
//! `source_checkpoints[pos]` → snapshot index) deliberately get **no index**
//! — see the Trace Parser documentation for the full indexing policy.
//!
//! # Reference semantics (verified against Stage 1, never inferred from names)
//!
//! * `event_id` is the 0-based position in `instruction_events` (the recorder
//!   enforces contiguity) — *not* an opaque id.
//! * `instruction_id` is the 0-based position in `disassembly_table`.
//! * `checkpoint_id` on an instruction event and `source_checkpoints[]` /
//!   `step_index` are 0-based positions in `snapshots`.
//! * `stop_id`, `stop_id_before`, `stop_id_after`, and allocation
//!   `observed_at_stop` are the **1-based GDB stop counter** — a different
//!   sequence from event or snapshot positions, with holes whenever a stop
//!   did not produce a retained snapshot.
//! * `allocation_id` is a stable engine identity (0 = untracked); it is
//!   never a vector index, and reused addresses always carry distinct ids.
//! * `address` fields are recorded process addresses (hex strings), never
//!   identities of executions.
//!
//! # Ordering preconditions
//!
//! Three zero-memory lookups binary-search the *canonical* arrays instead of
//! storing anything: snapshot-by-stop (`observation.stop_id` strictly
//! increasing), events-by-stop (`stop_id_before` non-decreasing) and
//! events-by-checkpoint (`checkpoint_id` non-decreasing). The build verifies
//! each ordering in one linear pass and records the result in
//! [`TraceIndexes::ordering`]; queries that need an ordering refuse to
//! answer (`QueryError::OrderingViolation`) when it does not hold, instead
//! of returning potentially wrong results from a broken array.
//!
//! Construction is a small constant number of linear passes over the
//! relevant collections (O(E + D + H + B + L + G + S)); no sorting of
//! canonical data happens, in place or otherwise.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::models::{DebugSessionTrace, HeapEvent, InstructionEvent};

use super::error::QueryError;

// ---------------------------------------------------------------------------
// Index set
// ---------------------------------------------------------------------------

/// The compact index set built once per [`TraceStore`](super::TraceStore) and
/// shared by every clone of it (two `Arc`s, zero rebuilds).
///
/// Everything here references canonical data by position; the only strings
/// retained are the interned source paths in [`SourceIndex::files`], one
/// `Arc<str>` per *distinct* recorded path (never per event).
#[derive(Debug, Clone)]
pub struct TraceIndexes {
    /// Reverse index from a disassembly entry to the event positions that
    /// executed it (its recorded executions, in event order).
    pub executions: ExecutionsIndex,
    /// Source-location relationships (static instruction ids and dynamic
    /// event positions per recorded `(file, line)`), plus the reverse
    /// instruction → source mapping.
    pub sources: SourceIndex,
    /// Heap lookup by allocation identity (never by address).
    pub allocations: AllocationIndex,
    /// Ordering preconditions verified at build time; consumed by the
    /// zero-memory stop/checkpoint lookups on `TraceStore`.
    pub ordering: OrderingFacts,
}

impl TraceIndexes {
    /// Build all indexes over a canonical trace.
    ///
    /// Complexity: one linear pass each over `instruction_events`,
    /// `disassembly_table`, `heap_events`, `heap_state`,
    /// `allocation_lifetimes`, `snapshots`, and `trace_gaps` (plus a
    /// transient interning pass over the same events). No canonical array
    /// is sorted or copied; index memory is a few bytes per record —
    /// see [`TraceIndexes::estimated_bytes`] and the performance findings
    /// in the Trace Parser documentation for measured numbers.
    ///
    /// Record counts are stored as `u32` positions: the ingestion byte cap
    /// (`TraceLoadOptions::max_bytes`) bounds any deserialized record count
    /// far below `u32::MAX`, so positions cannot overflow for any
    /// ingestable input.
    pub fn build(trace: &DebugSessionTrace) -> Self {
        Self {
            executions: ExecutionsIndex::build(
                &trace.recording.instruction_events,
                trace.recording.disassembly_table.len(),
            ),
            sources: SourceIndex::build(
                &trace.recording.instruction_events,
                trace.recording.disassembly_table.len(),
            ),
            allocations: AllocationIndex::build(
                &trace.heap_events,
                &trace.heap_state,
                &trace.recording.allocation_lifetimes,
            ),
            ordering: OrderingFacts::build(
                &trace.snapshots,
                &trace.recording.instruction_events,
                &trace.recording.trace_gaps,
            ),
        }
    }

    /// Rough in-memory size of the index structures (bytes), for reporting
    /// next to the measured value. The formula counts position arrays and
    /// hash-map entries; hash overhead is approximated per entry. The
    /// measured truth (process RSS delta in a release build) is reported in
    /// the Trace Parser documentation.
    pub fn estimated_bytes(&self) -> usize {
        let executions = (self.executions.offsets.len() + self.executions.positions.len()) * 4;
        let sources = {
            let csr = (self.sources.instruction_offsets.len()
                + self.sources.instruction_values.len()
                + self.sources.event_offsets.len()
                + self.sources.event_values.len()
                + self.sources.instr_source_offsets.len()
                + self.sources.instr_source_values.len())
                * 4;
            let keys = self.sources.keys.len() * 8;
            let lookup = self.sources.key_lookup.len() * 48
                + self.sources.invalid_instruction_refs.len() * 32;
            let files = self
                .sources
                .files
                .iter()
                .map(|f| 16 + f.len())
                .sum::<usize>();
            csr + keys + lookup + files
        };
        let allocations = {
            let positions: usize = self
                .allocations
                .events_by_id
                .values()
                .map(|v| v.capacity() * 4)
                .sum();
            let state = self.allocations.state_by_id.len() * 56;
            let lifetime = self.allocations.lifetime_by_id.len() * 56;
            positions + state + lifetime
        };
        executions + sources + allocations + std::mem::size_of::<OrderingFacts>()
    }
}

// ---------------------------------------------------------------------------
// Disassembly → executions (CSR)
// ---------------------------------------------------------------------------

/// Reverse index: disassembly entry → event positions that executed it.
///
/// Compact CSR form: `offsets[instruction_id] .. offsets[instruction_id+1]`
/// slice `positions` in ascending event order. Events whose
/// `instruction_id` is absent, or out of table range (detectable corruption),
/// are simply not present here — deep validation reports them; queries
/// surface them as [`QueryError`].
#[derive(Debug, Clone, Default)]
pub struct ExecutionsIndex {
    offsets: Vec<u32>,
    positions: Vec<u32>,
}

impl ExecutionsIndex {
    fn build(events: &[InstructionEvent], disassembly_len: usize) -> Self {
        let mut counts = vec![0u32; disassembly_len];
        let mut indexed = 0u64;
        for event in events {
            if let Some(id) = event.instruction_id {
                if id < disassembly_len as u64 {
                    counts[id as usize] += 1;
                    indexed += 1;
                }
            }
        }
        // offsets[id]..offsets[id+1] is the position range of id;
        // exactly `disassembly_len + 1` entries.
        let mut offsets = Vec::with_capacity(disassembly_len + 1);
        let mut running = 0u32;
        for count in &counts {
            offsets.push(running);
            running += *count;
        }
        offsets.push(running);
        let mut positions = vec![0u32; indexed as usize];
        let mut cursor = offsets.clone();
        for (position, event) in events.iter().enumerate() {
            if let Some(id) = event.instruction_id {
                if id < disassembly_len as u64 {
                    let slot = cursor[id as usize] as usize;
                    positions[slot] = pos(position);
                    cursor[id as usize] += 1;
                }
            }
        }
        Self { offsets, positions }
    }

    /// The event positions that executed `instruction_id`, ascending —
    /// repeated executions of one static instruction stay distinct.
    pub fn positions_of(&self, instruction_id: u64) -> Result<&[u32], QueryError> {
        if instruction_id >= self.disassembly_len() as u64 {
            return Err(QueryError::InvalidReference {
                field: "instruction_id",
                value: instruction_id,
                collection: "disassembly_table",
                len: self.offsets.len() - 1,
            });
        }
        let id = instruction_id as usize;
        let start = self.offsets[id] as usize;
        let end = self.offsets[id + 1] as usize;
        Ok(&self.positions[start..end])
    }

    /// Number of disassembly entries covered (== `disassembly_table.len()`).
    pub fn disassembly_len(&self) -> usize {
        self.offsets.len() - 1
    }

    /// Total indexed executions (events with a resolvable `instruction_id`).
    pub fn indexed_executions(&self) -> usize {
        self.positions.len()
    }
}

// ---------------------------------------------------------------------------
// Source locations
// ---------------------------------------------------------------------------

/// One recorded source location: the exact file identity as recorded (no
/// normalization — two distinct files with identical basenames stay distinct)
/// and the recorded line.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceKey {
    /// The recorded file path, interned once per distinct path.
    pub file: Arc<str>,
    /// The recorded line number.
    pub line: u32,
}

impl std::fmt::Display for SourceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

/// Source-location relationships built from the recorded
/// `InstructionEvent::source` contexts.
///
/// Two distinct questions get two distinct value arrays over the same keys
/// (they must never be confused):
///
/// * **static** — which disassembly entries are associated with the source
///   location (`instruction_values`, deduplicated, ordered by first
///   appearance in the event timeline);
/// * **dynamic** — which recorded execution events ran instructions
///   associated with the location (`event_values`, ascending event order,
///   repeated visits preserved).
///
/// Events without a recorded `source.file`/`source.line` are *unavailable*
/// for source lookup (not indexed); the reverse `instruction → source keys`
/// CSR covers the same evidence from the other direction.
#[derive(Debug, Clone, Default)]
pub struct SourceIndex {
    /// Distinct source locations, ordered by first appearance in the event
    /// timeline.
    keys: Vec<(u32, u32)>,
    /// `(file id, line)` → position in `keys`.
    key_lookup: HashMap<(u32, u32), u32>,
    /// First invalid disassembly id per source key, when present.
    invalid_instruction_refs: HashMap<u32, u64>,
    /// Distinct recorded file identities (interned), first-appearance order.
    files: Vec<Arc<str>>,
    /// CSR over `keys`: associated disassembly entry ids (deduplicated).
    instruction_offsets: Vec<u32>,
    instruction_values: Vec<u32>,
    /// CSR over `keys`: executed event positions (ascending, with repeats).
    event_offsets: Vec<u32>,
    event_values: Vec<u32>,
    /// CSR over `disassembly_table`: positions in `keys` whose recorded
    /// events associated the instruction with that source location.
    instr_source_offsets: Vec<u32>,
    instr_source_values: Vec<u32>,
}

impl SourceIndex {
    fn build(events: &[InstructionEvent], disassembly_len: usize) -> Self {
        // Transient per-key accumulation, flattened into CSR arrays below.
        #[derive(Default)]
        struct Slots {
            instruction_ids: Vec<u32>,
            seen_ids: HashSet<u32>,
            event_positions: Vec<u32>,
        }
        let mut file_lookup: HashMap<Arc<str>, u32> = HashMap::new();
        let mut files: Vec<Arc<str>> = Vec::new();
        let mut key_lookup: HashMap<(u32, u32), u32> = HashMap::new();
        let mut invalid_instruction_refs = HashMap::new();
        let mut keys: Vec<(u32, u32)> = Vec::new();
        let mut slots: Vec<Slots> = Vec::new();
        // Transient reverse accumulation: instruction id → (key slots, seen).
        let mut instr_slots: HashMap<u32, (Vec<u32>, HashSet<u32>)> = HashMap::new();

        for (position, event) in events.iter().enumerate() {
            let (Some(file), Some(line)) = (&event.source.file, event.source.line) else {
                continue;
            };
            let file_id = match file_lookup.get(file.as_str()) {
                Some(&id) => id,
                None => {
                    // One byte-copy per distinct recorded path.
                    let interned: Arc<str> = Arc::from(file.as_str());
                    let id = files.len() as u32;
                    files.push(interned.clone());
                    file_lookup.insert(interned, id);
                    id
                }
            };
            let key_index = match key_lookup.get(&(file_id, line)) {
                Some(&ki) => ki as usize,
                None => {
                    let ki = keys.len();
                    keys.push((file_id, line));
                    slots.push(Slots::default());
                    key_lookup.insert((file_id, line), ki as u32);
                    ki
                }
            };
            let slot = &mut slots[key_index];
            slot.event_positions.push(pos(position));
            if let Some(id) = event.instruction_id {
                if id < disassembly_len as u64 {
                    if slot.seen_ids.insert(id as u32) {
                        slot.instruction_ids.push(id as u32);
                    }
                    let entry = instr_slots.entry(id as u32).or_default();
                    if entry.1.insert(key_index as u32) {
                        entry.0.push(key_index as u32);
                    }
                } else {
                    invalid_instruction_refs
                        .entry(key_index as u32)
                        .or_insert(id);
                }
            }
        }

        // Flatten key → {instruction ids, event positions} into CSR.
        let mut instruction_offsets = Vec::with_capacity(keys.len() + 1);
        let mut instruction_values = Vec::new();
        let mut event_offsets = Vec::with_capacity(keys.len() + 1);
        let mut event_values = Vec::new();
        for slot in &slots {
            instruction_offsets.push(instruction_values.len() as u32);
            instruction_values.extend_from_slice(&slot.instruction_ids);
            event_offsets.push(event_values.len() as u32);
            event_values.extend_from_slice(&slot.event_positions);
        }
        instruction_offsets.push(instruction_values.len() as u32);
        event_offsets.push(event_values.len() as u32);

        // Flatten instruction id → source-key positions into CSR.
        let mut instr_source_offsets = Vec::with_capacity(disassembly_len + 1);
        let mut instr_source_values = Vec::new();
        let mut running = 0u32;
        for id in 0..disassembly_len {
            instr_source_offsets.push(running);
            if let Some((list, _)) = instr_slots.get(&(id as u32)) {
                instr_source_values.extend_from_slice(list);
                running += list.len() as u32;
            }
        }
        instr_source_offsets.push(running);

        Self {
            keys,
            key_lookup,
            invalid_instruction_refs,
            files,
            instruction_offsets,
            instruction_values,
            event_offsets,
            event_values,
            instr_source_offsets,
            instr_source_values,
        }
    }

    /// Position of `(file, line)` in the key list, or `None` when the trace
    /// never recorded that exact location. File identity is the recorded
    /// string, compared verbatim; no basename or path normalization happens.
    /// The file scan is over *distinct* recorded paths only (typically a
    /// handful), so no per-query allocation is needed.
    pub fn key_position(&self, file: &str, line: u32) -> Option<u32> {
        let file_id = self.files.iter().position(|f| f.as_ref() == file)? as u32;
        self.key_lookup.get(&(file_id, line)).copied()
    }

    /// The distinct recorded source location, first-appearance order.
    pub fn source_key(&self, key_index: u32) -> Option<SourceKey> {
        let (file_id, line) = *self.keys.get(key_index as usize)?;
        Some(SourceKey {
            file: Arc::clone(&self.files[file_id as usize]),
            line,
        })
    }

    /// Internal key list as `(interned file id, line)` pairs — crate-visible
    /// for the store's borrowing iterators.
    pub(crate) fn key_list(&self) -> &[(u32, u32)] {
        &self.keys
    }

    /// Interned file identities by id (crate-visible companion of
    /// [`Self::key_list`]).
    pub(crate) fn file_identities(&self) -> &[Arc<str>] {
        &self.files
    }

    /// Number of distinct recorded source locations.
    pub fn key_count(&self) -> usize {
        self.keys.len()
    }

    /// The distinct recorded file identities (interned, first-appearance
    /// order). Identical basenames of different paths appear separately.
    pub fn files(&self) -> &[Arc<str>] {
        &self.files
    }

    /// **Static** side of the source relationship: disassembly entry ids
    /// associated with the location (deduplicated, first-appearance order).
    /// Empty when the location is unknown or no executed instruction carried
    /// a resolvable `instruction_id`. Call through
    /// [`TraceStore::instructions_for_source`](super::TraceStore::instructions_for_source)
    /// to surface *invalid* references as `QueryError` rather than treating
    /// them as valid missing evidence.
    pub fn instruction_ids_of(&self, key_index: u32) -> &[u32] {
        let ki = key_index as usize;
        if ki + 1 >= self.instruction_offsets.len() {
            return &[];
        }
        let start = self.instruction_offsets[ki] as usize;
        let end = self.instruction_offsets[ki + 1] as usize;
        &self.instruction_values[start..end]
    }

    /// Broken source→disassembly references at this source key are surfaced
    /// by the store query, not concealed as an empty static mapping.
    pub(crate) fn invalid_instruction_ref(&self, key_index: u32) -> Option<u64> {
        self.invalid_instruction_refs.get(&key_index).copied()
    }

    /// **Dynamic** side of the source relationship: event positions that ran
    /// instructions associated with the location, ascending (repeated visits
    /// preserved, event order intact).
    pub fn event_positions_of(&self, key_index: u32) -> &[u32] {
        let ki = key_index as usize;
        if ki + 1 >= self.event_offsets.len() {
            return &[];
        }
        let start = self.event_offsets[ki] as usize;
        let end = self.event_offsets[ki + 1] as usize;
        &self.event_values[start..end]
    }

    /// **Reverse** mapping: source-key positions associated with
    /// `instruction_id`, in first-appearance order. Resolves through the
    /// recorded event evidence — a disassembly entry itself carries no
    /// source fields in this schema.
    pub fn source_key_positions_of(&self, instruction_id: u64) -> Result<&[u32], QueryError> {
        if instruction_id >= (self.instr_source_offsets.len() - 1) as u64 {
            return Err(QueryError::InvalidReference {
                field: "instruction_id",
                value: instruction_id,
                collection: "disassembly_table",
                len: self.instr_source_offsets.len() - 1,
            });
        }
        let id = instruction_id as usize;
        let start = self.instr_source_offsets[id] as usize;
        let end = self.instr_source_offsets[id + 1] as usize;
        Ok(&self.instr_source_values[start..end])
    }

    /// Total events indexed by source location (events with a recorded
    /// `source.file` and `source.line`).
    pub fn indexed_events(&self) -> usize {
        self.event_values.len()
    }
}

// ---------------------------------------------------------------------------
// Heap allocation identity
// ---------------------------------------------------------------------------

/// Heap lookups keyed by **allocation identity** — never by address
/// (reused addresses are different allocations) and never by vector
/// position (ids are sparse, engine-assigned, and `0` means untracked).
#[derive(Debug, Clone, Default)]
pub struct AllocationIndex {
    /// allocation id → heap-event positions, in recorded (chronological)
    /// order. Id `0` indexes the untracked events (`AllocationFailed`,
    /// unwrapped-library traffic) exactly as recorded.
    events_by_id: HashMap<u64, Vec<u32>>,
    /// allocation id → position in `heap_state`. First occurrence wins;
    /// duplicates are a corruption finding, never silently merged.
    state_by_id: HashMap<u64, u32>,
    /// allocation id → position in `allocation_lifetimes`.
    lifetime_by_id: HashMap<u64, u32>,
}

impl AllocationIndex {
    fn build(
        heap_events: &[HeapEvent],
        heap_state: &[crate::models::HeapBlockInfo],
        lifetimes: &[crate::models::AllocationLifetime],
    ) -> Self {
        let mut events_by_id: HashMap<u64, Vec<u32>> = HashMap::new();
        for (position, event) in heap_events.iter().enumerate() {
            events_by_id
                .entry(event.allocation_id)
                .or_default()
                .push(pos(position));
        }
        let mut state_by_id = HashMap::new();
        for (position, block) in heap_state.iter().enumerate() {
            state_by_id
                .entry(block.allocation_id)
                .or_insert(pos(position));
        }
        let mut lifetime_by_id = HashMap::new();
        for (position, lifetime) in lifetimes.iter().enumerate() {
            lifetime_by_id
                .entry(lifetime.allocation_id)
                .or_insert(pos(position));
        }
        Self {
            events_by_id,
            state_by_id,
            lifetime_by_id,
        }
    }

    /// Heap-event positions recorded for `allocation_id`, ascending
    /// (chronological). Empty for unknown ids — a valid empty result. Id `0`
    /// returns the untracked events, exactly as the identity contract says.
    pub fn event_positions_of(&self, allocation_id: u64) -> &[u32] {
        self.events_by_id
            .get(&allocation_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Distinct allocation ids that have at least one recorded heap event
    /// (unordered — the id domain is sparse and unordered by contract).
    pub fn allocation_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.events_by_id.keys().copied()
    }

    /// `heap_state` position of the block with this allocation id.
    pub fn state_position_of(&self, allocation_id: u64) -> Option<u32> {
        self.state_by_id.get(&allocation_id).copied()
    }

    /// `allocation_lifetimes` position of this allocation id.
    pub fn lifetime_position_of(&self, allocation_id: u64) -> Option<u32> {
        self.lifetime_by_id.get(&allocation_id).copied()
    }

    /// Distinct allocation ids present in `heap_state`.
    pub fn state_ids(&self) -> usize {
        self.state_by_id.len()
    }
}

// ---------------------------------------------------------------------------
// Verified ordering facts (zero-memory lookups rely on them)
// ---------------------------------------------------------------------------

/// Ordering preconditions verified in one linear pass at index build.
///
/// These are recording guarantees for engine-written traces (snapshots are
/// appended per 1-based stop counter; events record the current stop and the
/// most recent snapshot). When a trace violates one — detectable corruption,
/// not partial coverage — the dependent queries return
/// [`QueryError::OrderingViolation`] instead of answering from the broken
/// array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderingFacts {
    /// `snapshots[i].observation.stop_id` strictly increasing (holes are
    /// normal — stops without retained snapshots; strictness is required
    /// for the binary search).
    pub snapshots_by_stop_sorted: bool,
    /// `instruction_events[i].stop_id_before` non-decreasing.
    pub events_by_stop_sorted: bool,
    /// `checkpoint_id` key over events non-decreasing (`None` sorts first).
    pub events_by_checkpoint_sorted: bool,
    /// `snapshot.observation.heap_event_count` non-decreasing.
    pub heap_counts_sorted: bool,
    /// `trace_gaps[i].instruction_position` non-decreasing.
    pub gaps_by_position_sorted: bool,
}

impl OrderingFacts {
    fn build(
        snapshots: &[crate::models::ExecutionSnapshot],
        events: &[InstructionEvent],
        gaps: &[crate::models::TraceGap],
    ) -> Self {
        let snapshots_by_stop_sorted = snapshots
            .windows(2)
            .all(|w| w[0].observation.stop_id < w[1].observation.stop_id);
        let events_by_stop_sorted = events
            .windows(2)
            .all(|w| w[0].stop_id_before <= w[1].stop_id_before);
        let events_by_checkpoint_sorted = events
            .windows(2)
            .all(|w| checkpoint_key(&w[0]) <= checkpoint_key(&w[1]));
        let heap_counts_sorted = snapshots
            .windows(2)
            .all(|w| w[0].observation.heap_event_count <= w[1].observation.heap_event_count);
        let gaps_by_position_sorted = gaps
            .windows(2)
            .all(|w| w[0].instruction_position <= w[1].instruction_position);
        Self {
            snapshots_by_stop_sorted,
            events_by_stop_sorted,
            events_by_checkpoint_sorted,
            heap_counts_sorted,
            gaps_by_position_sorted,
        }
    }
}

/// Sort key for an event's checkpoint reference: `None` (recorded before any
/// snapshot existed) orders before every real checkpoint.
fn checkpoint_key(event: &InstructionEvent) -> Option<usize> {
    event.checkpoint_id
}

/// Checked position conversion (see [`TraceIndexes::build`] for the bound).
fn pos(index: usize) -> u32 {
    u32::try_from(index)
        .expect("record count exceeds u32::MAX: impossible under the ingestion byte cap")
}

// ---------------------------------------------------------------------------
// Borrowing iterators (no per-query allocation)
// ---------------------------------------------------------------------------

/// Iterator over the recorded executions of one disassembly entry —
/// repeated executions stay distinct, in event order.
#[derive(Debug, Clone)]
pub struct Executions<'a> {
    events: &'a [InstructionEvent],
    positions: &'a [u32],
    next: usize,
}

impl<'a> Executions<'a> {
    pub(crate) fn new(events: &'a [InstructionEvent], positions: &'a [u32]) -> Self {
        Self {
            events,
            positions,
            next: 0,
        }
    }
}

impl<'a> Iterator for Executions<'a> {
    type Item = &'a InstructionEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let position = *self.positions.get(self.next)?;
        self.next += 1;
        self.events.get(position as usize)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.positions.len() - self.next;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Executions<'_> {}

/// Iterator over the events that executed instructions associated with one
/// source location — event order preserved, repeated visits distinct.
#[derive(Debug, Clone)]
pub struct SourceEvents<'a> {
    events: &'a [InstructionEvent],
    positions: &'a [u32],
    next: usize,
}

impl<'a> SourceEvents<'a> {
    pub(crate) fn new(events: &'a [InstructionEvent], positions: &'a [u32]) -> Self {
        Self {
            events,
            positions,
            next: 0,
        }
    }
}

impl<'a> Iterator for SourceEvents<'a> {
    type Item = &'a InstructionEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let position = *self.positions.get(self.next)?;
        self.next += 1;
        self.events.get(position as usize)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.positions.len() - self.next;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for SourceEvents<'_> {}

/// Iterator over the heap events of one allocation identity — recorded
/// (chronological) order, never merged with a different allocation that
/// reused the address.
#[derive(Debug, Clone)]
pub struct AllocEvents<'a> {
    events: &'a [HeapEvent],
    positions: &'a [u32],
    next: usize,
}

impl<'a> AllocEvents<'a> {
    pub(crate) fn new(events: &'a [HeapEvent], positions: &'a [u32]) -> Self {
        Self {
            events,
            positions,
            next: 0,
        }
    }
}

impl<'a> Iterator for AllocEvents<'a> {
    type Item = &'a HeapEvent;

    fn next(&mut self) -> Option<Self::Item> {
        let position = *self.positions.get(self.next)?;
        self.next += 1;
        self.events.get(position as usize)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.positions.len() - self.next;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for AllocEvents<'_> {}

/// Iterator over the source locations associated with one disassembly entry
/// (resolved through recorded event evidence).
#[derive(Debug, Clone)]
pub struct SourceKeys<'a> {
    keys: &'a [(u32, u32)],
    files: &'a [Arc<str>],
    values: &'a [u32],
    next: usize,
}

impl<'a> SourceKeys<'a> {
    pub(crate) fn new(keys: &'a [(u32, u32)], files: &'a [Arc<str>], values: &'a [u32]) -> Self {
        Self {
            keys,
            files,
            values,
            next: 0,
        }
    }
}

impl<'a> Iterator for SourceKeys<'a> {
    type Item = SourceKey;

    fn next(&mut self) -> Option<Self::Item> {
        let key_index = *self.values.get(self.next)?;
        self.next += 1;
        self.keys
            .get(key_index as usize)
            .map(|&(file_id, line)| SourceKey {
                file: Arc::clone(&self.files[file_id as usize]),
                line,
            })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.values.len() - self.next;
        (remaining, Some(remaining))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DisassembledInstruction, HeapEventType, SourceContext, TraceGap};

    fn instruction(
        event_id: u64,
        id: Option<u64>,
        file: Option<&str>,
        line: Option<u32>,
    ) -> InstructionEvent {
        InstructionEvent {
            stop_id_before: 0,
            stop_id_after: None,
            event_id,
            instruction_id: id,
            address: format!("0x{event_id:x}"),
            next_address: None,
            thread_id: None,
            pid: None,
            source: SourceContext {
                file: file.map(Into::into),
                line,
                function: None,
            },
            checkpoint_id: None,
            outcome: "completed".into(),
            stopped_reason: None,
            signal: None,
            registers: None,
            memory: Vec::new(),
        }
    }

    fn heap_event(
        allocation_id: u64,
        address: &str,
        kind: HeapEventType,
        old: Option<u64>,
    ) -> HeapEvent {
        HeapEvent {
            raw_caller_address: None,
            sequence: 0,
            pid: 1,
            allocation_id,
            event_type: kind,
            address: Some(address.into()),
            size_bytes: Some(16),
            calloc_count: None,
            calloc_element_size: None,
            source_file: None,
            source_line: None,
            old_allocation_id: old,
            heap_usage_after: 16,
            heap_usage_complete: None,
            detail: None,
            attribution: None,
        }
    }

    fn disassembly(entries: usize) -> Vec<DisassembledInstruction> {
        (0..entries)
            .map(|id| DisassembledInstruction {
                instruction_id: id as u64,
                module_identity: "m".into(),
                address: format!("0x{id:x}"),
                bytes: None,
                text: format!("inst{id}"),
                function: None,
            })
            .collect()
    }

    fn empty_trace() -> DebugSessionTrace {
        serde_json::from_str(
            r#"{
                "compiled_successfully": true,
                "compile_errors": [],
                "snapshots": [],
                "heap_events": [],
                "heap_state": [],
                "execution_status": "completed",
                "resource_usage": {
                    "wall_time_ms": 1, "gdb_commands_issued": 0, "snapshots_captured": 0,
                    "allocation_events_ingested": 0, "program_output_bytes": 0,
                    "program_output_truncated": false, "limits_triggered": [],
                    "inferior_exit_code": null
                },
                "analysis_notes": []
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn empty_trace_builds_empty_indexes() {
        let trace = empty_trace();
        let indexes = TraceIndexes::build(&trace);
        assert_eq!(indexes.executions.disassembly_len(), 0);
        assert_eq!(
            indexes.executions.positions_of(0),
            Err(QueryError::InvalidReference {
                field: "instruction_id",
                value: 0,
                collection: "disassembly_table",
                len: 0,
            })
        );
        assert_eq!(indexes.sources.key_count(), 0);
        assert!(indexes.allocations.event_positions_of(1).is_empty());
        assert!(indexes.ordering.snapshots_by_stop_sorted);
        assert!(indexes.ordering.events_by_stop_sorted);
        let bytes = indexes.estimated_bytes();
        assert!(bytes > 0 && bytes < 1 << 20, "{bytes}");
    }

    #[test]
    fn executions_index_groups_repeated_executions_in_order() {
        let mut trace = empty_trace();
        trace.recording.disassembly_table = disassembly(3);
        // Executions: 0, 1, 0, 2, 1, 0 — the loop pattern.
        for (position, id) in [0u64, 1, 0, 2, 1, 0].into_iter().enumerate() {
            trace.recording.instruction_events.push(instruction(
                position as u64,
                Some(id),
                Some("loop.c"),
                Some(7),
            ));
        }
        let indexes = TraceIndexes::build(&trace);
        assert_eq!(indexes.executions.positions_of(0).unwrap(), &[0, 2, 5]);
        assert_eq!(indexes.executions.positions_of(1).unwrap(), &[1, 4]);
        assert_eq!(indexes.executions.positions_of(2).unwrap(), &[3]);
        assert!(matches!(
            indexes.executions.positions_of(3),
            Err(QueryError::InvalidReference { .. })
        ));
        // The borrowing iterator yields the same events, in event order.
        let executions: Vec<u64> = Executions::new(
            &trace.recording.instruction_events,
            indexes.executions.positions_of(0).unwrap(),
        )
        .map(|event| event.event_id)
        .collect();
        assert_eq!(executions, [0, 2, 5]);
    }

    #[test]
    fn source_index_splits_static_and_dynamic_and_reverse() {
        let mut trace = empty_trace();
        trace.recording.disassembly_table = disassembly(4);
        // Line 10: instructions {0, 1} executed at positions 0, 1, 3 and one
        // id-less event at 4. Line 11: instruction {2} at position 2. One
        // event has no source (5) and stays out of the source index.
        type Plan = &'static [(u64, Option<u64>, Option<&'static str>, Option<u32>)];
        let plan: Plan = &[
            (0, Some(0), Some("a.c"), Some(10)),
            (1, Some(1), Some("a.c"), Some(10)),
            (2, Some(2), Some("a.c"), Some(11)),
            (3, Some(0), Some("a.c"), Some(10)),
            (4, None, Some("a.c"), Some(10)),
            (5, Some(3), None, None),
        ];
        for &(id, instr, file, line) in plan {
            trace
                .recording
                .instruction_events
                .push(instruction(id, instr, file, line));
        }
        let indexes = TraceIndexes::build(&trace);
        let key10 = indexes.sources.key_position("a.c", 10).unwrap();
        let key11 = indexes.sources.key_position("a.c", 11).unwrap();
        // Static: deduplicated instruction ids, first-appearance order.
        assert_eq!(indexes.sources.instruction_ids_of(key10), &[0, 1]);
        assert_eq!(indexes.sources.instruction_ids_of(key11), &[2]);
        // Dynamic: every visit, in event order.
        assert_eq!(indexes.sources.event_positions_of(key10), &[0, 1, 3, 4]);
        assert_eq!(indexes.sources.event_positions_of(key11), &[2]);
        // Unknown location: valid empty result.
        assert!(indexes.sources.key_position("a.c", 99).is_none());
        assert!(indexes.sources.key_position("b.c", 10).is_none());
        // Reverse: instruction 0 → one key, line 10.
        let keys0: Vec<SourceKey> = SourceKeys::new(
            &indexes.sources.keys,
            &indexes.sources.files,
            indexes.sources.source_key_positions_of(0).unwrap(),
        )
        .collect();
        assert_eq!(keys0.len(), 1);
        assert_eq!(keys0[0].line, 10);
        assert_eq!(keys0[0].file.as_ref(), "a.c");
    }

    #[test]
    fn allocation_index_keeps_identities_apart_from_addresses() {
        let mut trace = empty_trace();
        // Two allocations reusing one address; plus id 0 untracked traffic.
        let plan: &[(u64, HeapEventType, Option<u64>)] = &[
            (12, HeapEventType::Allocated, None),
            (19, HeapEventType::Reallocated, Some(12)),
            (0, HeapEventType::AllocationFailed, None),
            (12, HeapEventType::Freed, None),
            (19, HeapEventType::Freed, None),
        ];
        for (position, item) in plan.iter().enumerate() {
            let mut event = heap_event(item.0, "0x1000", item.1.clone(), item.2);
            if event.event_type == HeapEventType::AllocationFailed {
                event.address = Some("0xff0".into());
            }
            let _ = position;
            trace.heap_events.push(event);
        }
        let indexes = TraceIndexes::build(&trace);
        assert_eq!(
            indexes.allocations.event_positions_of(12),
            &[0, 3],
            "allocation 12's events only — no address merge with 19"
        );
        assert_eq!(indexes.allocations.event_positions_of(19), &[1, 4]);
        assert_eq!(indexes.allocations.event_positions_of(0), &[2]);
        assert!(indexes.allocations.event_positions_of(7).is_empty());
        assert_eq!(indexes.allocations.allocation_ids().count(), 3);
        // The iterator yields the same chronological order.
        let freed: Vec<HeapEventType> = AllocEvents::new(
            &trace.heap_events,
            indexes.allocations.event_positions_of(12),
        )
        .map(|event| event.event_type.clone())
        .collect();
        assert_eq!(freed, [HeapEventType::Allocated, HeapEventType::Freed]);
    }

    #[test]
    fn ordering_facts_detect_holes_and_violations() {
        let mut trace = empty_trace();
        // Stop ids 1, 3 (hole at 2) — strictly increasing still holds.
        let snap = |stop: u64| crate::models::ExecutionSnapshot {
            observation: crate::models::SnapshotObservation {
                stop_id: stop,
                ..Default::default()
            },
            step_index: 0,
            ..Default::default()
        };
        trace.snapshots.push(snap(1));
        trace.snapshots.push(snap(3));
        assert!(OrderingFacts::build(&trace.snapshots, &[], &[]).snapshots_by_stop_sorted);
        // A repeated stop id breaks strictness.
        trace.snapshots.push(snap(3));
        assert!(!OrderingFacts::build(&trace.snapshots, &[], &[]).snapshots_by_stop_sorted);
        // Events with a descending stop_id_before.
        let mut first = instruction(0, None, None, None);
        first.stop_id_before = 5;
        let mut second = instruction(1, None, None, None);
        second.stop_id_before = 4;
        assert!(!OrderingFacts::build(&[], &[first, second], &[]).events_by_stop_sorted);
        // Gaps out of order.
        let gap = |position: u64| TraceGap {
            instruction_position: position,
            start_address: None,
            end_address: None,
            origin: "o".into(),
            reason: "r".into(),
        };
        assert!(!OrderingFacts::build(&[], &[], &[gap(9), gap(2)]).gaps_by_position_sorted);
    }
}

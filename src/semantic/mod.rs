//! SE-1: trace-scoped, read-only runtime object identity and observation history.
//!
//! Requires an explicitly validated TP-6 view. This is NOT a pointer graph,
//! byte interpreter, static declaration instance registry, or proof of exact
//! stack birth/death. All large values/bytes remain in the canonical trace.

use std::collections::{BTreeMap, HashMap, HashSet};

pub mod layout;
pub use layout::{
    ByteOrder, Capture, DecodeResult, DecodeStatus, LayoutEngine, LayoutError, LayoutKind,
    LayoutNode, MemoryLayout, NodePath, TargetFormat, TypedValue,
};

use thiserror::Error;

use crate::models::{
    AllocationLifetime, ExecutionSnapshot, HeapBlockInfo, HeapEventType, ScopeStatus, VariableInfo,
};
use crate::trace_parser::semantic::{HeapEvidence, VariableEvidence};
use crate::trace_parser::{
    Evidence, EvidenceConfidence, EvidenceLocation, EvidenceRef, QueryError, TraceStore,
    ValidatedSemanticView, VariableScope,
};

/// A semantic-session-local object citation. Cloning does not clone the trace.
/// No numeric-only constructor or cross-trace equality is provided: resolve
/// through `ObjectRegistry::object`, which checks the weak trace identity.
#[derive(Clone, Debug)]
pub struct ObjectId {
    position: u32,
    anchor: EvidenceRef,
}

impl ObjectId {
    pub fn evidence(&self) -> &EvidenceRef {
        &self.anchor
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Stack,
    Global,
    StaticLocal,
    Heap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    InScope,
    OutOfScope,
    UnknownScope,
    StaticStorageObserved,
    HeapObservedActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointPresence {
    Observed {
        count: usize,
    },
    /// No retained observation here; never a claim of destruction or return.
    NotObserved {
        snapshot_complete: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapLifetimeStatus {
    RecordedFreed,
    RecordedActiveAtEndOfCapture,
    Unavailable,
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("cannot build trusted object registry: {corruptions} structural findings")]
    Unsound { corruptions: usize },
    #[error("object/observation position cannot fit in a u32 index")]
    IndexOverflow,
    #[error("{collection} index {index} is out of range (len {len})")]
    OutOfRange {
        collection: &'static str,
        index: usize,
        len: usize,
    },
    #[error(transparent)]
    Evidence(#[from] QueryError),
}

type Result<T> = std::result::Result<T, RegistryError>;

/// One observed storage object, not a C value or declaration. `first_record`
/// is an immutable, trace-scoped citation. String fields borrow original
/// variable identity/metadata; the first address is metadata, never a key.
pub struct RuntimeObject<'a> {
    pub id: ObjectId,
    pub kind: ObjectKind,
    pub pid: Option<u32>,
    pub activation_id: Option<u64>,
    pub allocation_id: Option<u64>,
    pub name: Option<&'a str>,
    pub runtime_identity: Option<&'a str>,
    pub type_ref: Option<&'a str>,
    pub declaration_ref: Option<&'a str>,
    pub source_declaration_ref: Option<&'a str>,
    pub declaration_file: Option<&'a str>,
    pub declaration_line: Option<u32>,
    /// Address of the first observed variable, not pointer target or object key.
    pub first_address: Option<&'a str>,
    pub first_extent: Option<u64>,
    pub identity_confidence: EvidenceConfidence,
}

/// A retained observation, not an inferred lifetime boundary. Absence at a
/// checkpoint never manufactures OutOfScope/Freed. `location` is the exact
/// canonical variable/global/caller/heap-block position. No bytes are copied.
pub struct ObjectObservation {
    object: u32,
    pub checkpoint: usize,
    pub location: EvidenceLocation,
    pub visibility: Visibility,
    /// Only the snapshot's own recorded capture completeness, not a complete
    /// execution, initialization proof or lifetime guarantee.
    pub snapshot_complete: bool,
}

/// Event history and lifetime are distinct from checkpoint observations.
/// A historical final block's bytes need not have been captured at a queried
/// checkpoint. `None` for a boundary means unknown, not open forever.
pub struct HeapLifetime<'a> {
    pub record: Option<&'a AllocationLifetime>,
    pub final_block: Option<&'a HeapBlockInfo>,
    pub identity_confidence: EvidenceConfidence,
    pub realloc_confidence: EvidenceConfidence,
    pub recorded_predecessor: Option<u64>,
    pub status: HeapLifetimeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key<'a> {
    Stack(u32, u64, &'a str),
    Global(u32, &'a str),
    Static(u32, &'a str),
}

fn kind(scope: VariableScope, var: &VariableInfo) -> ObjectKind {
    match var.scope.as_deref() {
        Some("static") => ObjectKind::StaticLocal,
        Some("global") => ObjectKind::Global,
        _ if scope == VariableScope::Global => ObjectKind::Global,
        _ => ObjectKind::Stack,
    }
}

/// Only *recorded native identities* with verified activation/frame scope
/// may group across checkpoints. Fallback activation:name:address, bare names,
/// declaration IDs and unqualified/ambiguous statics never become keys.
fn key<'a>(
    var: &'a VariableInfo,
    object_kind: ObjectKind,
    pid: Option<u32>,
    trusted_activation: Option<u64>,
) -> Option<Key<'a>> {
    let pid = pid.filter(|pid| *pid != 0)?;
    let identity = var.identity.as_deref()?;
    match object_kind {
        ObjectKind::Stack => {
            let activation = trusted_activation.filter(|id| *id != 0)?;
            let (encoded, suffix) = identity.strip_prefix("activation:")?.split_once(':')?;
            if var.activation_id != Some(activation)
                || encoded.parse::<u64>().ok() != Some(activation)
                || !suffix.starts_with("block:")
            {
                return None;
            }
            Some(Key::Stack(pid, activation, identity))
        }
        ObjectKind::Global if identity.starts_with("symbol:") && identity.len() > 7 => {
            Some(Key::Global(pid, identity))
        }
        ObjectKind::StaticLocal
            if identity.starts_with("static:block:")
                || (identity.starts_with("symbol:") && identity.len() > 7) =>
        {
            Some(Key::Static(pid, identity))
        }
        _ => None,
    }
}

fn u32_index(index: usize) -> Result<u32> {
    u32::try_from(index).map_err(|_| RegistryError::IndexOverflow)
}

/// Every entry is borrowed from `ValidatedSemanticView`; one registry borrows
/// one validated view, which in turn borrows one canonical TP-1 TraceStore.
/// Construction is linear in observed variable/heap records (plus hashing).
/// No pointer edges, address lookup or duplicate memory buffers are built.
pub struct ObjectRegistry<'v, 'a> {
    view: &'v ValidatedSemanticView<'a>,
    objects: Vec<RuntimeObject<'a>>,
    observations: Vec<ObjectObservation>,
    per_object: Vec<Vec<u32>>,
    per_checkpoint: Vec<Vec<u32>>,
    checkpoint_objects: Vec<Vec<u32>>,
    heap_ids: HashMap<u64, u32>,
}

/// A borrowed observation with on-demand TP-6 value/memory confidence.
/// Accessors require no heap allocation beyond the small Stage-1 EvidenceRef
/// handle returned for a specific query.
pub struct ObservationView<'r, 'v, 'a> {
    registry: &'r ObjectRegistry<'v, 'a>,
    pub record: &'r ObjectObservation,
}

impl<'r, 'v, 'a> ObservationView<'r, 'v, 'a> {
    pub fn object(&self) -> &'r RuntimeObject<'a> {
        &self.registry.objects[self.record.object as usize]
    }

    pub fn evidence(&self) -> Result<EvidenceRef> {
        Ok(self.registry.store().evidence_ref(self.record.location)?)
    }

    pub fn variable(&self) -> Result<Option<VariableEvidence<'a>>> {
        if self.object().kind == ObjectKind::Heap {
            return Ok(None);
        }
        let reference = self.evidence()?;
        Ok(Some(self.registry.view.variable(&reference)?))
    }

    /// Snapshot heap-block bytes (if recorded) are historical at this stop,
    /// not interchangeable with final-state bytes in HeapEvidence::memory.
    pub fn heap_block(&self) -> Result<Option<&'a HeapBlockInfo>> {
        if self.object().kind != ObjectKind::Heap {
            return Ok(None);
        }
        let reference = self.evidence()?;
        let Evidence::SnapshotHeapBlock(block) =
            self.registry.store().resolve_evidence(&reference)?
        else {
            return Err(QueryError::Unavailable {
                what: "an observed snapshot heap block",
            }
            .into());
        };
        Ok(Some(block))
    }
}

impl<'v, 'a> ObjectRegistry<'v, 'a> {
    pub fn build(view: &'v ValidatedSemanticView<'a>) -> Result<Self> {
        if !view.report().is_structurally_sound() {
            return Err(RegistryError::Unsound {
                corruptions: view.report().corruption_count(),
            });
        }
        let store = view.store();
        let snapshots = &store.trace().snapshots;
        let mut registry = Self {
            view,
            objects: Vec::new(),
            observations: Vec::new(),
            per_object: Vec::new(),
            per_checkpoint: vec![Vec::new(); snapshots.len()],
            checkpoint_objects: vec![Vec::new(); snapshots.len()],
            heap_ids: HashMap::new(),
        };
        let mut keys: HashMap<Key<'a>, u32> = HashMap::new();
        // Ephemeral deduplication keeps checkpoint construction O(records),
        // without retaining a second per-checkpoint hash index.
        let mut seen_at_checkpoint = vec![HashSet::new(); snapshots.len()];
        let mut collisions = HashSet::new();
        // A duplicate native identity *within one snapshot* is not a reason
        // to choose the first record. Disable merging that key everywhere.
        for (index, snapshot) in snapshots.iter().enumerate() {
            let (local, callers) = trusted_activations(view, index)?;
            let mut seen = HashSet::new();
            for (scope, vars, activation) in variable_groups(snapshot, local, &callers) {
                for var in vars {
                    if let Some(k) =
                        key(var, kind(scope, var), snapshot.observation.pid, activation)
                    {
                        if !seen.insert(k) {
                            collisions.insert(k);
                        }
                    }
                }
            }
        }
        for (index, snapshot) in snapshots.iter().enumerate() {
            let (local, callers) = trusted_activations(view, index)?;
            for (scope, vars, activation) in variable_groups(snapshot, local, &callers) {
                for (position, var) in vars.iter().enumerate() {
                    let location = match scope {
                        VariableScope::Local => EvidenceLocation::SnapshotVariable {
                            snapshot: index,
                            variable: position,
                        },
                        VariableScope::Global => EvidenceLocation::SnapshotGlobal {
                            snapshot: index,
                            variable: position,
                        },
                        VariableScope::Caller(frame) => EvidenceLocation::SnapshotCallerVariable {
                            snapshot: index,
                            frame,
                            variable: position,
                        },
                    };
                    let object_kind = kind(scope, var);
                    let identity_key = key(var, object_kind, snapshot.observation.pid, activation)
                        .filter(|k| !collisions.contains(k));
                    let object = if let Some(existing) = identity_key.and_then(|k| keys.get(&k)) {
                        *existing
                    } else {
                        let slot = u32_index(registry.objects.len())?;
                        let anchor = store.evidence_ref(location)?;
                        registry.objects.push(RuntimeObject {
                            id: ObjectId {
                                position: slot,
                                anchor,
                            },
                            kind: object_kind,
                            pid: snapshot.observation.pid,
                            activation_id: (object_kind == ObjectKind::Stack)
                                .then_some(var.activation_id)
                                .flatten(),
                            allocation_id: None,
                            name: Some(var.name.as_str()),
                            runtime_identity: var.identity.as_deref(),
                            type_ref: var.type_ref.as_deref(),
                            declaration_ref: var.declaration_ref.as_deref(),
                            source_declaration_ref: var.source_declaration_ref.as_deref(),
                            declaration_file: var.declaration_file.as_deref(),
                            declaration_line: var.declaration_line,
                            first_address: var.address.as_deref(),
                            first_extent: var.size_bytes,
                            identity_confidence: if identity_key.is_some() {
                                EvidenceConfidence::Definite
                            } else {
                                EvidenceConfidence::Unavailable
                            },
                        });
                        registry.per_object.push(Vec::new());
                        if let Some(k) = identity_key {
                            keys.insert(k, slot);
                        }
                        slot
                    };
                    let visibility = if object_kind != ObjectKind::Stack {
                        Visibility::StaticStorageObserved
                    } else {
                        match var.scope_status {
                            Some(ScopeStatus::Active) => Visibility::InScope,
                            Some(ScopeStatus::Inactive) => Visibility::OutOfScope,
                            Some(ScopeStatus::Unknown) | None => Visibility::UnknownScope,
                        }
                    };
                    registry.push_observation(
                        object,
                        index,
                        location,
                        visibility,
                        snapshot,
                        &mut seen_at_checkpoint[index],
                    )?;
                }
            }
        }
        registry.build_heap(&mut seen_at_checkpoint)?;
        Ok(registry)
    }

    fn store(&self) -> &'a TraceStore {
        self.view.store()
    }

    fn push_observation(
        &mut self,
        object: u32,
        checkpoint: usize,
        location: EvidenceLocation,
        visibility: Visibility,
        snapshot: &ExecutionSnapshot,
        seen: &mut HashSet<u32>,
    ) -> Result<()> {
        let position = u32_index(self.observations.len())?;
        self.observations.push(ObjectObservation {
            object,
            checkpoint,
            location,
            visibility,
            snapshot_complete: snapshot.observation.complete && !snapshot.memory_truncated,
        });
        self.per_object[object as usize].push(position);
        self.per_checkpoint[checkpoint].push(position);
        if seen.insert(object) {
            self.checkpoint_objects[checkpoint].push(object);
        }
        Ok(())
    }

    fn build_heap(&mut self, seen_at_checkpoint: &mut [HashSet<u32>]) -> Result<()> {
        let store = self.view.store();
        let trace = store.trace();
        let mut anchors = BTreeMap::new();
        for (i, record) in trace.recording.allocation_lifetimes.iter().enumerate() {
            if record.allocation_id != 0 {
                anchors
                    .entry(record.allocation_id)
                    .or_insert((EvidenceLocation::AllocationLifetime(i), Some(record.pid)));
            }
        }
        for (i, record) in trace.heap_state.iter().enumerate() {
            if record.allocation_id != 0 {
                anchors
                    .entry(record.allocation_id)
                    .or_insert((EvidenceLocation::HeapState(i), None));
            }
        }
        for (i, event) in trace.heap_events.iter().enumerate() {
            if event.allocation_id != 0
                && event.address.is_some()
                && matches!(
                    event.event_type,
                    HeapEventType::Allocated | HeapEventType::Reallocated
                )
            {
                anchors
                    .entry(event.allocation_id)
                    .or_insert((EvidenceLocation::HeapEvent(i), Some(event.pid)));
            }
        }
        for (i, snapshot) in trace.snapshots.iter().enumerate() {
            for (j, block) in snapshot.heap_blocks.iter().enumerate() {
                if block.allocation_id != 0 {
                    anchors.entry(block.allocation_id).or_insert((
                        EvidenceLocation::SnapshotHeapBlock {
                            snapshot: i,
                            block: j,
                        },
                        snapshot.observation.pid,
                    ));
                }
            }
        }
        for (allocation_id, (location, origin_pid)) in anchors {
            let heap = self.view.heap(allocation_id);
            let position = u32_index(self.objects.len())?;
            let pid = heap
                .lifetime
                .map(|l| l.pid)
                .or(origin_pid)
                .filter(|p| *p != 0);
            let block = heap.block;
            self.objects.push(RuntimeObject {
                id: ObjectId {
                    position,
                    anchor: store.evidence_ref(location)?,
                },
                kind: ObjectKind::Heap,
                pid,
                activation_id: None,
                allocation_id: Some(allocation_id),
                name: None,
                runtime_identity: None,
                type_ref: None,
                declaration_ref: None,
                source_declaration_ref: None,
                declaration_file: block.and_then(|b| b.allocation_file.as_deref()),
                declaration_line: block.and_then(|b| b.allocation_line),
                first_address: block.map(|b| b.address.as_str()).filter(|s| !s.is_empty()),
                first_extent: block.map(|b| b.size_bytes),
                identity_confidence: heap.identity_confidence,
            });
            self.per_object.push(Vec::new());
            self.heap_ids.insert(allocation_id, position);
        }
        for (index, snapshot) in trace.snapshots.iter().enumerate() {
            for (position, block) in snapshot.heap_blocks.iter().enumerate() {
                if let Some(&object) = self.heap_ids.get(&block.allocation_id) {
                    self.push_observation(
                        object,
                        index,
                        EvidenceLocation::SnapshotHeapBlock {
                            snapshot: index,
                            block: position,
                        },
                        Visibility::HeapObservedActive,
                        snapshot,
                        &mut seen_at_checkpoint[index],
                    )?;
                }
            }
        }
        Ok(())
    }

    /// O(1). ObjectId checks the Stage-1 weak trace identity *and* its anchor;
    /// same numeric slot in a separate run cannot resolve here.
    pub fn object(&self, id: &ObjectId) -> Result<&RuntimeObject<'a>> {
        self.store().resolve_evidence(&id.anchor)?;
        let object = self
            .objects
            .get(id.position as usize)
            .ok_or(RegistryError::OutOfRange {
                collection: "semantic objects",
                index: id.position as usize,
                len: self.objects.len(),
            })?;
        if object.id.anchor.location() != id.anchor.location() {
            return Err(QueryError::Unavailable {
                what: "an object identity in this registry",
            }
            .into());
        }
        Ok(object)
    }

    pub fn objects(&self) -> &[RuntimeObject<'a>] {
        &self.objects
    }

    pub fn observation_count(&self) -> usize {
        self.observations.len()
    }

    /// Approximate retained index/vector payload, excluding allocator bucket
    /// metadata, padding and the shared TP-1 trace/TP-2 indexes. Not an RSS or
    /// process-memory bound; included to make SE-1 overhead measurable.
    pub fn estimated_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            + self.objects.capacity() * size_of::<RuntimeObject<'a>>()
            + self.observations.capacity() * size_of::<ObjectObservation>()
            + self.per_object.capacity() * size_of::<Vec<u32>>()
            + self.per_checkpoint.capacity() * size_of::<Vec<u32>>()
            + self.checkpoint_objects.capacity() * size_of::<Vec<u32>>()
            + self
                .per_object
                .iter()
                .map(|v| v.capacity() * size_of::<u32>())
                .sum::<usize>()
            + self
                .per_checkpoint
                .iter()
                .map(|v| v.capacity() * size_of::<u32>())
                .sum::<usize>()
            + self
                .checkpoint_objects
                .iter()
                .map(|v| v.capacity() * size_of::<u32>())
                .sum::<usize>()
            + self.heap_ids.capacity() * size_of::<(u64, u32)>()
    }

    pub fn objects_at_checkpoint(
        &self,
        checkpoint: usize,
    ) -> Result<impl Iterator<Item = &RuntimeObject<'a>> + '_> {
        let indices = self
            .checkpoint_objects
            .get(checkpoint)
            .ok_or(RegistryError::OutOfRange {
                collection: "checkpoints",
                index: checkpoint,
                len: self.checkpoint_objects.len(),
            })?;
        Ok(indices.iter().map(|&i| &self.objects[i as usize]))
    }

    pub fn observations_at_checkpoint(
        &self,
        checkpoint: usize,
    ) -> Result<impl Iterator<Item = ObservationView<'_, 'v, 'a>> + '_> {
        let indices = self
            .per_checkpoint
            .get(checkpoint)
            .ok_or(RegistryError::OutOfRange {
                collection: "checkpoints",
                index: checkpoint,
                len: self.per_checkpoint.len(),
            })?;
        Ok(indices.iter().map(|&i| ObservationView {
            registry: self,
            record: &self.observations[i as usize],
        }))
    }

    pub fn observations(
        &self,
        id: &ObjectId,
    ) -> Result<impl Iterator<Item = ObservationView<'_, 'v, 'a>> + '_> {
        let object = self.object(id)?;
        Ok(self.per_object[object.id.position as usize]
            .iter()
            .map(|&i| ObservationView {
                registry: self,
                record: &self.observations[i as usize],
            }))
    }

    /// O(log observations-of-object + matches), returning *all* observations
    /// at this checkpoint rather than silently selecting the first duplicate.
    /// Empty means no recorded observation, NOT dead or out of scope.
    pub fn observation_at(
        &self,
        id: &ObjectId,
        checkpoint: usize,
    ) -> Result<impl Iterator<Item = ObservationView<'_, 'v, 'a>> + '_> {
        self.checkpoint(checkpoint)?;
        let object = self.object(id)?;
        let indices = &self.per_object[object.id.position as usize];
        let first =
            indices.partition_point(|&i| self.observations[i as usize].checkpoint < checkpoint);
        let last =
            indices.partition_point(|&i| self.observations[i as usize].checkpoint <= checkpoint);
        Ok(indices[first..last].iter().map(|&i| ObservationView {
            registry: self,
            record: &self.observations[i as usize],
        }))
    }

    fn checkpoint(&self, checkpoint: usize) -> Result<()> {
        if checkpoint >= self.per_checkpoint.len() {
            return Err(RegistryError::OutOfRange {
                collection: "checkpoints",
                index: checkpoint,
                len: self.per_checkpoint.len(),
            });
        }
        Ok(())
    }

    pub fn heap_object(&self, reference: &EvidenceRef) -> Result<Option<&RuntimeObject<'a>>> {
        let heap = self.view.heap_reference(reference)?; // foreign checked first
        Ok(self
            .heap_ids
            .get(&heap.allocation_id)
            .map(|&i| &self.objects[i as usize]))
    }

    pub fn heap_by_id(&self, allocation_id: u64) -> Option<&RuntimeObject<'a>> {
        self.heap_ids
            .get(&allocation_id)
            .map(|&i| &self.objects[i as usize])
    }

    pub fn heap_lifetime(&self, id: &ObjectId) -> Result<Option<HeapLifetime<'a>>> {
        let object = self.object(id)?;
        let Some(allocation_id) = object.allocation_id else {
            return Ok(None);
        };
        let heap = self.view.heap(allocation_id);
        let status = if heap.lifetime.is_some_and(|l| l.free_sequence.is_some())
            || heap.block.is_some_and(|b| b.is_freed)
        {
            HeapLifetimeStatus::RecordedFreed
        } else if heap.identity_confidence == EvidenceConfidence::Definite
            && heap.block.is_some_and(|b| !b.is_freed)
        {
            HeapLifetimeStatus::RecordedActiveAtEndOfCapture
        } else {
            HeapLifetimeStatus::Unavailable
        };
        Ok(Some(HeapLifetime {
            record: heap.lifetime,
            final_block: heap.block,
            identity_confidence: heap.identity_confidence,
            realloc_confidence: heap.realloc_confidence,
            recorded_predecessor: heap.old_id,
            status,
        }))
    }

    /// O(log observations-of-object). Missing means only that this snapshot
    /// did not record the object; frame retention, scope and capture may vary.
    pub fn presence_at(&self, id: &ObjectId, checkpoint: usize) -> Result<CheckpointPresence> {
        self.checkpoint(checkpoint)?;
        let object = self.object(id)?;
        let indices = &self.per_object[object.id.position as usize];
        let first =
            indices.partition_point(|&i| self.observations[i as usize].checkpoint < checkpoint);
        let last =
            indices.partition_point(|&i| self.observations[i as usize].checkpoint <= checkpoint);
        if first != last {
            Ok(CheckpointPresence::Observed {
                count: last - first,
            })
        } else {
            Ok(CheckpointPresence::NotObserved {
                snapshot_complete: {
                    let snapshot = &self.store().trace().snapshots[checkpoint];
                    snapshot.observation.complete && !snapshot.memory_truncated
                },
            })
        }
    }

    /// Only a verified/candidate **event-and-lifetime** TP-6 relationship is
    /// exposed as an edge. A raw old_id with missing evidence is not an edge.
    pub fn realloc_predecessor(
        &self,
        id: &ObjectId,
    ) -> Result<Option<(&RuntimeObject<'a>, EvidenceConfidence)>> {
        let object = self.object(id)?;
        let Some(allocation_id) = object.allocation_id else {
            return Ok(None);
        };
        let heap = self.view.heap(allocation_id);
        if !matches!(
            heap.realloc_confidence,
            EvidenceConfidence::Definite | EvidenceConfidence::Candidate
        ) {
            return Ok(None);
        }
        Ok(heap
            .old_id
            .and_then(|id| self.heap_by_id(id))
            .map(|prior| (prior, heap.realloc_confidence)))
    }

    /// TP-2 indexed, borrowed heap events for exactly this lifetime. Failed
    /// allocations without a created lifetime do not become heap objects.
    pub fn heap_events(&self, id: &ObjectId) -> Result<Option<HeapEvidence<'a>>> {
        let object = self.object(id)?;
        Ok(object
            .allocation_id
            .map(|allocation_id| self.view.heap(allocation_id)))
    }

    /// O(objects recorded at one checkpoint). Caller frame index is the
    /// `caller_frames` *position*, not GDB level. Absent/uncertain activation
    /// returns Unavailable instead of grouping objects by function name.
    pub fn stack_objects(
        &self,
        frame: &EvidenceRef,
    ) -> Result<impl Iterator<Item = &RuntimeObject<'a>> + '_> {
        self.store().resolve_evidence(frame)?;
        let (snapshot, activation) = match frame.location() {
            EvidenceLocation::SnapshotCallerFrame {
                snapshot,
                frame: position,
            } => {
                let summary = self.view.checkpoint_summary(snapshot)?;
                let caller = summary.caller_frame(position)?;
                if caller.confidence != EvidenceConfidence::Definite {
                    return Err(QueryError::Unavailable {
                        what: "a verified caller activation",
                    }
                    .into());
                }
                (snapshot, caller.frame.activation_id)
            }
            EvidenceLocation::SnapshotFrame { snapshot, frame: 0 } => {
                let summary = self.view.checkpoint_summary(snapshot)?;
                let first = summary
                    .checkpoint
                    .context
                    .variables()
                    .iter()
                    .find(|v| kind(VariableScope::Local, v) == ObjectKind::Stack)
                    .and_then(|v| v.activation_id);
                let Some(act) = first else {
                    return Err(QueryError::Unavailable {
                        what: "a verified frame-zero activation",
                    }
                    .into());
                };
                let Some(activation) = summary.frame_activation(act)? else {
                    return Err(QueryError::Unavailable {
                        what: "a verified frame-zero activation",
                    }
                    .into());
                };
                if activation.confidence != EvidenceConfidence::Definite
                    || activation.scope != VariableScope::Local
                {
                    return Err(QueryError::Unavailable {
                        what: "a verified frame-zero activation",
                    }
                    .into());
                }
                (snapshot, act)
            }
            _ => {
                return Err(QueryError::Unavailable {
                    what: "a stack-frame evidence reference",
                }
                .into())
            }
        };
        let indices = &self.checkpoint_objects[snapshot];
        Ok(indices
            .iter()
            .map(|&i| &self.objects[i as usize])
            .filter(move |o| o.kind == ObjectKind::Stack && o.activation_id == Some(activation)))
    }
}

/// Check the recorded TP-7 activation association *once per frame per stop*,
/// rather than doing per-variable O(n) scans. Erroneous/partial frames get no
/// cross-checkpoint stack merge key, but their raw observations are retained.
fn trusted_activations<'a>(
    view: &ValidatedSemanticView<'a>,
    index: usize,
) -> Result<(Option<u64>, Vec<Option<u64>>)> {
    let summary = view.checkpoint_summary(index)?;
    let locals = summary.checkpoint.context.variables();
    let first = locals
        .iter()
        .find(|v| kind(VariableScope::Local, v) == ObjectKind::Stack)
        .and_then(|v| v.activation_id);
    let local = if let Some(id) = first {
        match summary.frame_activation(id) {
            Ok(Some(frame))
                if frame.scope == VariableScope::Local
                    && frame.confidence == EvidenceConfidence::Definite =>
            {
                Some(id)
            }
            _ => None,
        }
    } else {
        None
    };
    let mut callers = Vec::with_capacity(summary.checkpoint.context.caller_frames().len());
    for (position, frame) in summary
        .checkpoint
        .context
        .caller_frames()
        .iter()
        .enumerate()
    {
        let accepted = if frame.activation_id == 0 {
            false
        } else {
            matches!(summary.caller_frame(position),Ok(c) if c.confidence==EvidenceConfidence::Definite)
                && matches!(summary.frame_activation(frame.activation_id),Ok(Some(f)) if f.scope==VariableScope::Caller(position) && f.confidence==EvidenceConfidence::Definite)
        };
        callers.push(accepted.then_some(frame.activation_id));
    }
    Ok((local, callers))
}

fn variable_groups<'a>(
    snapshot: &'a ExecutionSnapshot,
    local: Option<u64>,
    callers: &[Option<u64>],
) -> Vec<(VariableScope, &'a [VariableInfo], Option<u64>)> {
    let mut groups = Vec::with_capacity(2 + snapshot.caller_frames.len());
    groups.push((VariableScope::Local, snapshot.variables.as_slice(), local));
    groups.push((
        VariableScope::Global,
        snapshot.observation.globals.as_slice(),
        None,
    ));
    for (i, frame) in snapshot.caller_frames.iter().enumerate() {
        groups.push((
            VariableScope::Caller(i),
            frame.variables.as_slice(),
            callers[i],
        ));
    }
    groups
}

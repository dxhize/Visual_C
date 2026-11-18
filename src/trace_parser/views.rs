//! TP-3: two organization-only views over the *same* trace and TP-2 indexes.
//! Creating either view copies one reference and allocates no records.

use std::ops::Deref;

use crate::models::{
    Coverage, ExecutionSnapshot, FrameVariables, HeapBlockInfo, HeapEvent, InstructionEvent,
    StackFrameInfo, VariableInfo,
};

use super::evidence::{Evidence, EvidenceLocation, EvidenceRef};
use super::{QueryError, QueryResult, TraceStore};

/// General-purpose read-only raw trace access. `Deref<Target = TraceStore>`
/// exposes existing TP-1/TP-2 metadata, bounded ranges, source/disassembly,
/// allocation and diagnostic queries *without duplicating their APIs or
/// implementation*. Neither the view nor its creation allocates.
#[derive(Clone, Copy, Debug)]
pub struct RawTraceView<'a> {
    store: &'a TraceStore,
}

impl<'a> RawTraceView<'a> {
    /// The same immutable store shared with semantic consumers.
    pub fn store(self) -> &'a TraceStore {
        self.store
    }

    /// Borrow the optional static source registry from the canonical trace.
    pub fn source_metadata(self) -> Option<&'a crate::clang_ast::SourceMetadataRegistry> {
        self.store.trace().source_metadata.as_ref()
    }

    /// Look up a source declaration by its recorded identity. This is a
    /// bounded linear lookup (the registry retains at most 2048 records); it
    /// deliberately adds no duplicate index to TP-2.
    pub fn source_declaration(
        self,
        id: &str,
    ) -> QueryResult<&'a crate::clang_ast::SourceDeclaration> {
        self.store.source_declaration(id)
    }

    pub fn source_expression(
        self,
        id: &str,
    ) -> QueryResult<&'a crate::clang_ast::SourceExpression> {
        self.store.source_expression(id)
    }
}

impl Deref for RawTraceView<'_> {
    type Target = TraceStore;
    fn deref(&self) -> &Self::Target {
        self.store
    }
}

/// Evidence-preserving checkpoint traversal for future semantic processing.
/// This does **not** infer C objects, lifetimes, pointers or state changes.
#[derive(Clone, Copy, Debug)]
pub struct SemanticInputView<'a> {
    store: &'a TraceStore,
}

impl<'a> SemanticInputView<'a> {
    /// Direct access to the same canonical store (and TP-2 indexes).
    pub fn store(self) -> &'a TraceStore {
        self.store
    }

    /// Borrow source syntax without inferring that any expression executed.
    pub fn source_metadata(self) -> Option<&'a crate::clang_ast::SourceMetadataRegistry> {
        self.store.trace().source_metadata.as_ref()
    }

    /// Borrow a sibling raw view, not a second pipeline or owner.
    pub fn raw(self) -> RawTraceView<'a> {
        RawTraceView { store: self.store }
    }

    /// Context for any snapshot position, including snapshots *not* in
    /// `source_checkpoints`. No source-checkpoint membership is asserted.
    /// O(1), no allocation.
    pub fn checkpoint_context(self, snapshot_index: usize) -> QueryResult<CheckpointContext<'a>> {
        let snapshot = self.store.snapshot(snapshot_index)?;
        Ok(CheckpointContext {
            store: self.store,
            snapshot_index,
            snapshot,
            source_checkpoint_position: None,
        })
    }

    /// Context reached via the explicit source-checkpoint indirection:
    /// `source_checkpoints[position]` → `snapshots[index]`. The returned
    /// position identifies this traversal, not an invented checkpoint.
    /// O(1), no allocation.
    pub fn source_checkpoint_context(self, position: usize) -> QueryResult<CheckpointContext<'a>> {
        let index = self.store.source_checkpoint(position)?;
        let mut context = self.checkpoint_context(index)?;
        context.source_checkpoint_position = Some(position);
        Ok(context)
    }

    /// Iterate **all** retained snapshots in canonical order, even those
    /// without a source-checkpoint entry. O(1) work per context, no buffer.
    pub fn snapshots(self) -> impl ExactSizeIterator<Item = CheckpointContext<'a>> + 'a {
        (0..self.store.trace().snapshots.len()).map(move |index| CheckpointContext {
            store: self.store,
            snapshot_index: index,
            snapshot: &self.store.trace().snapshots[index],
            source_checkpoint_position: None,
        })
    }

    /// Iterate the journaled source-checkpoint list in its *explicit* order.
    /// Bad snapshot references return errors at their actual position rather
    /// than silently skipping them. O(1) per item, no buffer.
    pub fn source_checkpoints(
        self,
    ) -> impl ExactSizeIterator<Item = QueryResult<CheckpointContext<'a>>> + 'a {
        (0..self.store.source_checkpoints().len())
            .map(move |position| self.source_checkpoint_context(position))
    }

    /// Trace-local evidence reference, validated against canonical records.
    pub fn evidence_ref(self, location: EvidenceLocation) -> QueryResult<EvidenceRef> {
        self.store.evidence_ref(location)
    }

    /// Resolve to the exact canonical observation, after checking the
    /// reference belongs to this store or one of its clones.
    pub fn resolve_evidence(self, reference: &EvidenceRef) -> QueryResult<Evidence<'a>> {
        self.store.resolve_evidence(reference)
    }
}

/// Borrowed observations at one retained snapshot. `snapshot_index` and
/// `source_checkpoint_position` are separate domains: the latter is `Some`
/// only when the context was obtained *through* that explicit list.
///
/// Accessors for event runs and heap boundaries are **on demand** and use
/// TP-2's verified binary searches: O(log events) and O(log snapshots)
/// respectively, no per-checkpoint O(events) scan or materialization.
#[derive(Clone, Copy, Debug)]
pub struct CheckpointContext<'a> {
    store: &'a TraceStore,
    snapshot_index: usize,
    snapshot: &'a ExecutionSnapshot,
    source_checkpoint_position: Option<usize>,
}

impl<'a> CheckpointContext<'a> {
    pub fn snapshot_index(self) -> usize {
        self.snapshot_index
    }

    pub fn source_checkpoint_position(self) -> Option<usize> {
        self.source_checkpoint_position
    }

    pub fn snapshot(self) -> &'a ExecutionSnapshot {
        self.snapshot
    }

    /// GDB's one-based stop counter, when recorded. Zero is the model's
    /// legacy/default absence value, never a real debugger stop.
    pub fn stop_id(self) -> Option<u64> {
        (self.snapshot.observation.stop_id != 0).then_some(self.snapshot.observation.stop_id)
    }

    /// Recorded source identity only; an empty file or line 0 does not
    /// identify a source line. No path normalization or substitution.
    pub fn source_location(self) -> Option<(&'a str, u32)> {
        (!self.snapshot.file.is_empty() && self.snapshot.line != 0)
            .then_some((self.snapshot.file.as_str(), self.snapshot.line))
    }

    pub fn frames(self) -> &'a [StackFrameInfo] {
        &self.snapshot.call_stack
    }

    pub fn variables(self) -> &'a [VariableInfo] {
        &self.snapshot.variables
    }

    /// Borrow caller-frame observations separately; `variables()` remains
    /// frame 0 only. Missing capture in older traces is an empty slice, not
    /// evidence that the call stack had no callers.
    pub fn caller_frames(self) -> &'a [FrameVariables] {
        &self.snapshot.caller_frames
    }

    pub fn globals(self) -> &'a [VariableInfo] {
        &self.snapshot.observation.globals
    }

    /// Optional source declaration attached through the variable's unique
    /// DWARF declaration. `Ok(None)` means no source declaration was linked,
    /// not that no declaration exists in the source.
    pub fn variable_source_declaration(
        self,
        variable: usize,
    ) -> QueryResult<Option<&'a crate::clang_ast::SourceDeclaration>> {
        let info = self
            .snapshot
            .variables
            .get(variable)
            .ok_or(QueryError::OutOfRange {
                collection: "snapshot variables",
                index: variable,
                len: self.snapshot.variables.len(),
            })?;
        match info.source_declaration_ref.as_deref() {
            Some(id) => self.store.source_declaration(id).map(Some),
            None => Ok(None),
        }
    }

    /// Heap blocks observed at this snapshot, not necessarily blocks whose
    /// *final-state bytes* were captured at this stop.
    pub fn heap_blocks(self) -> &'a [HeapBlockInfo] {
        &self.snapshot.heap_blocks
    }

    /// Attempted-event count preceding this stop; not necessarily a
    /// position in the *retained* instruction event vector.
    pub fn instruction_position(self) -> u64 {
        self.snapshot.observation.instruction_position
    }

    pub fn coverage(self) -> &'a Coverage {
        self.store.coverage()
    }

    /// Exactly the Stage 1 completeness flag for this snapshot. The caller
    /// should also inspect `memory_truncated`, `errors`, variable read and
    /// bytes statuses, and overall `coverage` for fuller uncertainty.
    pub fn observation_complete(self) -> bool {
        self.snapshot.observation.complete
    }

    /// Events explicitly tagged with this snapshot's `checkpoint_id`.
    /// Empty does **not** imply the checkpoint represents an unexecuted
    /// interval (instruction retention may be disabled). No nearest-event
    /// or nearest-timestamp join is performed.
    pub fn events_at_checkpoint(self) -> QueryResult<&'a [InstructionEvent]> {
        self.store.events_at_checkpoint(self.snapshot_index)
    }

    /// Heap events between this and the previous *retained* snapshot's
    /// cumulative heap counters. This is a **boundary interval**, not proof
    /// that all events happened at this exact GDB stop; skipped snapshots
    /// coarsen attribution. `Unavailable` if no stop identity was recorded.
    pub fn heap_events_in_snapshot_interval(self) -> QueryResult<&'a [HeapEvent]> {
        let stop = self.stop_id().ok_or(QueryError::Unavailable {
            what: "a GDB stop identity on this snapshot",
        })?;
        self.store.heap_events_at_stop(stop)
    }

    pub fn snapshot_evidence(self) -> QueryResult<EvidenceRef> {
        // Position is known to be valid: this context borrowed the snapshot
        // from the store. Do not clone the snapshot or trace.
        self.store
            .evidence_ref(EvidenceLocation::Snapshot(self.snapshot_index))
    }

    pub fn variable_evidence(self, index: usize) -> QueryResult<EvidenceRef> {
        self.store.evidence_ref(EvidenceLocation::SnapshotVariable {
            snapshot: self.snapshot_index,
            variable: index,
        })
    }

    pub fn global_evidence(self, index: usize) -> QueryResult<EvidenceRef> {
        self.store.evidence_ref(EvidenceLocation::SnapshotGlobal {
            snapshot: self.snapshot_index,
            variable: index,
        })
    }

    /// `frame` is a caller_frames vector position, not a GDB frame level.
    pub fn caller_frame_evidence(self, frame: usize) -> QueryResult<EvidenceRef> {
        self.store
            .evidence_ref(EvidenceLocation::SnapshotCallerFrame {
                snapshot: self.snapshot_index,
                frame,
            })
    }

    pub fn caller_variable_evidence(
        self,
        frame: usize,
        variable: usize,
    ) -> QueryResult<EvidenceRef> {
        self.store
            .evidence_ref(EvidenceLocation::SnapshotCallerVariable {
                snapshot: self.snapshot_index,
                frame,
                variable,
            })
    }

    pub fn heap_block_evidence(self, index: usize) -> QueryResult<EvidenceRef> {
        self.store
            .evidence_ref(EvidenceLocation::SnapshotHeapBlock {
                snapshot: self.snapshot_index,
                block: index,
            })
    }
}

impl TraceStore {
    /// Bounded source-registry lookup by recorded identity. No new TP-2 index
    /// is built: the registry itself is capped at 2048 declarations.
    pub fn source_declaration(
        &self,
        id: &str,
    ) -> QueryResult<&crate::clang_ast::SourceDeclaration> {
        self.trace
            .source_metadata
            .as_ref()
            .and_then(|registry| {
                registry
                    .declarations
                    .iter()
                    .find(|declaration| declaration.id == id)
            })
            .ok_or(QueryError::Unavailable {
                what: "a source declaration with that identity",
            })
    }

    pub fn source_expression(&self, id: &str) -> QueryResult<&crate::clang_ast::SourceExpression> {
        self.trace
            .source_metadata
            .as_ref()
            .and_then(|registry| {
                registry
                    .expressions
                    .iter()
                    .find(|expression| expression.id == id)
            })
            .ok_or(QueryError::Unavailable {
                what: "a source expression with that identity",
            })
    }

    /// Zero-allocation raw evidence view. Delegates all TP-2 methods through
    /// `Deref`; no duplicate index or full trace representation.
    pub fn raw(&self) -> RawTraceView<'_> {
        RawTraceView { store: self }
    }

    /// Zero-allocation semantic-*input* view (not a Semantic Engine).
    pub fn semantic_input(&self) -> SemanticInputView<'_> {
        SemanticInputView { store: self }
    }
}

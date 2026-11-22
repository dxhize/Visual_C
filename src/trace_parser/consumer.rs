//! TP-7: bounded checkpoint composition over the *existing* validated TP-6
//! view. Positions and EvidenceRef remain the only runtime record identities.
//! No guessed temporal join, global graph, or second canonical data model.
use std::ops::Range;

use crate::models::{FrameVariables, HeapEvent, InstructionEvent, TraceGap, VariableInfo};

use super::evidence::{EvidenceLocation, EvidenceRef};
use super::semantic::{
    CheckpointEvidence, DiagnosticEvidence, EvidenceConfidence, ExecutionEvidence, HeapEvidence,
    ProcessCoverageEvidence, SourceExpressionEvidence, ValgrindEvidence, ValidatedSemanticView,
    VariableEvidence,
};
use super::{QueryError, QueryResult};

/// Maximum records in any one consumer query. Pagination is explicit and
/// rejects over-limit / out-of-range requests; it never silently truncates.
pub const MAX_CONSUMER_PAGE: usize = 128;

fn page(range: Range<usize>, len: usize, collection: &'static str) -> QueryResult<Range<usize>> {
    if range.start > range.end || range.end > len {
        return Err(QueryError::InvalidRange {
            collection,
            start: range.start,
            end: range.end,
            len,
        });
    }
    if range.end - range.start > MAX_CONSUMER_PAGE {
        return Err(QueryError::LimitExceeded {
            collection,
            requested: range.end - range.start,
            max: MAX_CONSUMER_PAGE,
        });
    }
    Ok(range)
}

/// Collection within *one* snapshot; caller position is a caller_frames
/// vector position, not a GDB stack level or activation ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableScope {
    Local,
    Global,
    Caller(usize),
}

/// Composed evidence for one retained snapshot, never an inferred stop. It
/// borrows a validated view, so no query repeats whole-trace validation.
pub struct CheckpointSummary<'v, 'a> {
    view: &'v ValidatedSemanticView<'a>,
    pub checkpoint: CheckpointEvidence<'a>,
    /// TP-3 trace-scoped reference to `snapshots[index]`.
    pub reference: EvidenceRef,
}

impl<'a> ValidatedSemanticView<'a> {
    pub fn checkpoint_summary(&self, index: usize) -> QueryResult<CheckpointSummary<'_, 'a>> {
        let checkpoint = self.checkpoint(index)?;
        let reference = self
            .store()
            .evidence_ref(EvidenceLocation::Snapshot(index))?;
        Ok(CheckpointSummary {
            view: self,
            checkpoint,
            reference,
        })
    }

    /// Session-wide diagnostics, not causal explanations for a checkpoint.
    pub fn valgrind_errors_page(
        &self,
        range: Range<usize>,
    ) -> QueryResult<impl ExactSizeIterator<Item = QueryResult<ValgrindEvidence<'a>>> + '_> {
        let report = self
            .store()
            .valgrind_report()
            .ok_or(QueryError::Unavailable {
                what: "an independent Valgrind report",
            })?;
        let range = page(range, report.detailed_errors.len(), "valgrind diagnostics")?;
        Ok(range.map(move |index| self.valgrind_error(index)))
    }

    /// Page the recorded events for a trace-scoped allocation reference.
    /// A foreign reference is rejected *before* consulting numeric IDs.
    /// Every returned row is an observation, not proof that history is full.
    pub fn heap_events_page(
        &self,
        allocation: &EvidenceRef,
        range: Range<usize>,
    ) -> QueryResult<impl ExactSizeIterator<Item = QueryResult<AllocationEventEvidence<'a>>> + '_>
    {
        let heap = self.heap_reference(allocation)?;
        let positions = self
            .store()
            .heap_event_positions_for_allocation(heap.allocation_id);
        let range = page(range, positions.len(), "allocation heap events")?;
        Ok(range.map(move |index| {
            let position = positions[index] as usize;
            let reference = self
                .store()
                .evidence_ref(EvidenceLocation::HeapEvent(position))?;
            let event = &self.store().heap_events()[position];
            Ok(AllocationEventEvidence {
                event,
                position,
                reference,
                observed_confidence: self.qualify(EvidenceConfidence::Definite),
                history_confidence: heap.identity_confidence,
            })
        }))
    }
}

impl<'v, 'a> CheckpointSummary<'v, 'a> {
    pub fn snapshot_index(&self) -> usize {
        self.checkpoint.context.snapshot_index()
    }

    /// Recorded one-based debugger stop, not an instruction/event position.
    pub fn stop_id(&self) -> Option<u64> {
        self.checkpoint.context.stop_id()
    }

    pub fn attempted_instruction_position(&self) -> u64 {
        self.checkpoint.context.instruction_position()
    }

    pub fn pid(&self) -> Option<u32> {
        self.checkpoint.observation.pid
    }

    pub fn observation_complete(&self) -> bool {
        self.checkpoint.context.observation_complete()
    }

    pub fn source_location(&self) -> Option<(&str, u32, &str)> {
        self.checkpoint.location()
    }

    pub fn coverage(&self) -> (ExecutionEvidence<'a>, ProcessCoverageEvidence<'a>) {
        (self.view.execution(), self.view.process_coverage())
    }

    /// Reports are *session-wide*. In particular Valgrind ran in another
    /// process; no diagnostic returned here is joined to this stop.
    pub fn session_diagnostics(&self) -> DiagnosticEvidence<'a> {
        self.view.diagnostics()
    }

    /// Session-level report citations. Availability is independent of this
    /// checkpoint; these are not claims that a tool diagnostic occurred here.
    pub fn session_report_refs(&self) -> SessionReportRefs {
        let store = self.view.store();
        SessionReportRefs {
            crash: store.evidence_ref(EvidenceLocation::CrashReport).ok(),
            sanitizer: store.evidence_ref(EvidenceLocation::SanitizerReport).ok(),
            valgrind: store.evidence_ref(EvidenceLocation::ValgrindReport).ok(),
        }
    }

    /// Only findings whose **first recorded location** is exactly this
    /// snapshot/nested record. Aggregated occurrences can involve other
    /// snapshots; absence here does not establish local soundness. Consult
    /// the entire report for trace-wide findings and confidence downgrades.
    pub fn first_location_findings(
        &self,
    ) -> impl Iterator<Item = &'v super::validation::ValidationFinding> + '_ {
        use super::validation::ValidationLocation as L;
        let index = self.snapshot_index();
        self.view.report().findings().iter().filter(move |finding| {
            matches!(finding.location,
                L::Snapshot(i) if i == index)
                || matches!(finding.location,
                    L::SnapshotVariable {snapshot, ..}
                    | L::SnapshotGlobal {snapshot, ..}
                    | L::SnapshotCallerFrame {snapshot, ..}
                    | L::SnapshotCallerVariable {snapshot, ..}
                    | L::SnapshotHeapBlock {snapshot, ..} if snapshot == index)
        })
    }

    pub fn caller_frame(&self, position: usize) -> QueryResult<CallerEvidence<'a>> {
        let frames = self.checkpoint.context.caller_frames();
        let frame = frames.get(position).ok_or(QueryError::OutOfRange {
            collection: "snapshot.caller_frames",
            index: position,
            len: frames.len(),
        })?;
        Ok(CallerEvidence {
            frame,
            reference: self.checkpoint.context.caller_frame_evidence(position)?,
            position,
            confidence: self
                .view
                .qualify(if frame.errors.is_empty() && frame.activation_id != 0 {
                    EvidenceConfidence::Definite // observed frame association, not every variable read
                } else {
                    EvidenceConfidence::Unavailable
                }),
        })
    }

    fn scope_len(&self, scope: VariableScope) -> QueryResult<usize> {
        Ok(match scope {
            VariableScope::Local => self.checkpoint.context.variables().len(),
            VariableScope::Global => self.checkpoint.context.globals().len(),
            VariableScope::Caller(position) => self.caller_frame(position)?.frame.variables.len(),
        })
    }

    pub fn variable(
        &self,
        scope: VariableScope,
        index: usize,
    ) -> QueryResult<VariableEvidence<'a>> {
        let len = self.scope_len(scope)?;
        if index >= len {
            return Err(QueryError::OutOfRange {
                collection: "checkpoint variables",
                index,
                len,
            });
        }
        let reference = match scope {
            VariableScope::Local => self.checkpoint.context.variable_evidence(index)?,
            VariableScope::Global => self.checkpoint.context.global_evidence(index)?,
            VariableScope::Caller(frame) => self
                .checkpoint
                .context
                .caller_variable_evidence(frame, index)?,
        };
        self.view.variable(&reference)
    }

    /// Half-open page into one canonical variable vector. `0..0` is valid;
    /// over-limit and past-end requests fail instead of truncating.
    pub fn variables_page(
        &self,
        scope: VariableScope,
        range: Range<usize>,
    ) -> QueryResult<impl ExactSizeIterator<Item = QueryResult<VariableEvidence<'a>>> + '_> {
        let range = page(range, self.scope_len(scope)?, "checkpoint variables")?;
        Ok(range.map(move |index| self.variable(scope, index)))
    }

    /// A unique activation *at this checkpoint*, not a cross-stop lifetime.
    /// Missing legacy activation IDs are unavailable. Globals have no frame
    /// activation and are available separately via `VariableScope::Global`.
    pub fn frame_activation(&self, id: u64) -> QueryResult<Option<FrameActivation<'v, 'a>>> {
        if id == 0 {
            return Ok(None);
        }
        let locals = self.checkpoint.context.variables();
        let local_match = locals.iter().any(|v| v.activation_id == Some(id));
        let caller = self
            .checkpoint
            .context
            .caller_frames()
            .iter()
            .enumerate()
            .find(|(_, frame)| frame.activation_id == id);
        let duplicate = self
            .checkpoint
            .context
            .caller_frames()
            .iter()
            .filter(|frame| frame.activation_id == id)
            .count()
            > 1;
        if duplicate
            || (local_match && caller.is_some())
            || (local_match
                && locals
                    .iter()
                    .any(|v| v.activation_id.is_some_and(|other| other != id)))
        {
            return Err(QueryError::Unavailable {
                what: "a unique frame activation at this checkpoint",
            });
        }
        let (scope, variables, confidence) = if local_match {
            (
                VariableScope::Local,
                locals,
                if locals.iter().any(|v| v.activation_id.is_none()) {
                    EvidenceConfidence::Candidate
                } else {
                    EvidenceConfidence::Definite
                },
            )
        } else if let Some((position, frame)) = caller {
            (
                VariableScope::Caller(position),
                frame.variables.as_slice(),
                if frame.errors.is_empty() {
                    EvidenceConfidence::Definite
                } else {
                    EvidenceConfidence::Unavailable
                },
            )
        } else {
            return Ok(None);
        };
        Ok(Some(FrameActivation {
            summary: self.view,
            snapshot: self.snapshot_index(),
            scope,
            variables,
            activation_id: id,
            confidence: self.view.qualify(confidence),
        }))
    }

    /// Only events explicitly tagged `checkpoint_id=Some(snapshot_index)`.
    /// The tag is definite; it is NOT proof of execution at this exact GDB
    /// stop or that an AST expression on the source line executed.
    pub fn instruction_events_page(
        &self,
        range: Range<usize>,
    ) -> QueryResult<impl ExactSizeIterator<Item = QueryResult<InstructionEvidence<'a>>> + '_> {
        let events = self.checkpoint.context.events_at_checkpoint()?;
        let range = page(range, events.len(), "checkpoint instruction events")?;
        let start = self
            .view
            .store()
            .trace()
            .recording
            .instruction_events
            .partition_point(|event| event.checkpoint_id < Some(self.snapshot_index()));
        Ok(range.map(move |offset| {
            let position = start + offset;
            Ok(InstructionEvidence {
                event: &events[offset],
                position,
                reference: self
                    .view
                    .store()
                    .evidence_ref(EvidenceLocation::InstructionEvent(position))?,
                tag_confidence: self.view.qualify(EvidenceConfidence::Definite),
            })
        }))
    }

    /// Heap-event counts bracket *retained* snapshots. Events inside the
    /// interval may have happened at earlier omitted stops. Never equate the
    /// interval with an exact event timestamp or pointer provenance.
    pub fn heap_interval_page(
        &self,
        range: Range<usize>,
    ) -> QueryResult<impl ExactSizeIterator<Item = QueryResult<HeapIntervalEvidence<'a>>> + '_>
    {
        let events = self.checkpoint.context.heap_events_in_snapshot_interval()?;
        let range = page(range, events.len(), "checkpoint heap interval")?;
        let start = self
            .snapshot_index()
            .checked_sub(1)
            .and_then(|i| self.view.store().trace().snapshots.get(i))
            .map_or(0, |s| s.observation.heap_event_count);
        Ok(range.map(move |offset| {
            let position = start + offset;
            Ok(HeapIntervalEvidence {
                event: &events[offset],
                position,
                reference: self
                    .view
                    .store()
                    .evidence_ref(EvidenceLocation::HeapEvent(position))?,
                interval_confidence: self.view.qualify(EvidenceConfidence::Candidate),
            })
        }))
    }

    /// Recorded gaps after the preceding retained checkpoint's attempted
    /// position through this one's attempted position (first checkpoint
    /// includes position zero). This is a *position window*, not a causal
    /// assignment to this GDB stop. Binary search requires recorded ordering.
    pub fn gaps_in_attempted_window(
        &self,
        range: Range<usize>,
    ) -> QueryResult<impl ExactSizeIterator<Item = QueryResult<GapEvidence<'a>>> + '_> {
        let store = self.view.store();
        if !store.indexes().ordering.gaps_by_position_sorted {
            return Err(QueryError::OrderingViolation {
                ordering: "trace gap attempted positions",
                detail: "gap window requires non-decreasing positions".into(),
            });
        }
        let end_pos = self.attempted_instruction_position();
        let previous = self
            .snapshot_index()
            .checked_sub(1)
            .and_then(|i| store.trace().snapshots.get(i))
            .map(|s| s.observation.instruction_position);
        if previous.is_some_and(|start| start > end_pos) {
            return Err(QueryError::OrderingViolation {
                ordering: "snapshot attempted positions",
                detail: "checkpoint position preceded its previous retained snapshot".into(),
            });
        }
        let gaps = store.trace_gaps();
        let first = gaps
            .partition_point(|gap| previous.is_some_and(|start| gap.instruction_position <= start));
        let last = gaps.partition_point(|gap| gap.instruction_position <= end_pos);
        let range = page(range, last - first, "checkpoint attempted-position gaps")?;
        Ok(range.map(move |offset| {
            let position = first + offset;
            Ok(GapEvidence {
                gap: &gaps[position],
                position,
                reference: store.evidence_ref(EvidenceLocation::TraceGap(position))?,
                window_confidence: self.view.qualify(EvidenceConfidence::Candidate),
            })
        }))
    }

    /// Exact recorded source file + spanned line only. Missing optional AST
    /// returns Unavailable, not a negative finding. A match never proves
    /// expression execution. Registry is capped at 4096 expressions; this
    /// bounded, on-demand scan does not build a line-expanded index.
    pub fn source_expressions_page(
        &self,
        range: Range<usize>,
    ) -> QueryResult<impl Iterator<Item = SourceLocationEvidence<'a>> + '_> {
        let registry =
            self.view
                .store()
                .trace()
                .source_metadata
                .as_ref()
                .ok_or(QueryError::Unavailable {
                    what: "optional Clang AST source expressions",
                })?;
        let (recorded_file, line, _) = self.source_location().ok_or(QueryError::Unavailable {
            what: "a recorded checkpoint source location",
        })?;
        // GDB often reports a relative snapshot file while its frame retains
        // the absolute fullname also recorded at this same stop. Never join
        // by basename or fabricate a canonical path from another execution.
        let file = self
            .checkpoint
            .context
            .frames()
            .first()
            .and_then(|frame| frame.full_file.as_deref())
            .unwrap_or(recorded_file);
        let match_confidence = if registry.cross_provider_correlation_unavailable.is_some() {
            EvidenceConfidence::Unavailable
        } else {
            EvidenceConfidence::Candidate
        };
        let matches = |expr: &&crate::clang_ast::SourceExpression| {
            expr.span.file == file && expr.span.start_line <= line && line <= expr.span.end_line
        };
        let len = registry.expressions.iter().filter(matches).count();
        let range = page(range, len, "checkpoint source expressions")?;
        Ok(registry
            .expressions
            .iter()
            .filter(move |expr| {
                expr.span.file == file && expr.span.start_line <= line && line <= expr.span.end_line
            })
            .skip(range.start)
            .take(range.end - range.start)
            .map(move |expr| SourceLocationEvidence {
                expression: self.view.source_expression(&expr.id),
                source_match_confidence: self.view.qualify(match_confidence),
            }))
    }
}

/// Optional citations for separately recorded session reports; none is a
/// causal stop-to-tool-diagnostic join.
pub struct SessionReportRefs {
    pub crash: Option<EvidenceRef>,
    pub sanitizer: Option<EvidenceRef>,
    pub valgrind: Option<EvidenceRef>,
}

pub struct CallerEvidence<'a> {
    pub frame: &'a FrameVariables,
    pub reference: EvidenceRef,
    pub position: usize,
    pub confidence: EvidenceConfidence,
}

pub struct FrameActivation<'v, 'a> {
    summary: &'v ValidatedSemanticView<'a>,
    snapshot: usize,
    pub scope: VariableScope,
    pub variables: &'a [VariableInfo],
    pub activation_id: u64,
    pub confidence: EvidenceConfidence,
}

impl<'v, 'a> FrameActivation<'v, 'a> {
    /// O(1) page selection, no per-activation materialization.
    pub fn variables_page(
        &self,
        range: Range<usize>,
    ) -> QueryResult<impl ExactSizeIterator<Item = QueryResult<VariableEvidence<'a>>> + '_> {
        let range = page(range, self.variables.len(), "frame activation variables")?;
        Ok(range.map(move |index| {
            let location = match self.scope {
                VariableScope::Local => EvidenceLocation::SnapshotVariable {
                    snapshot: self.snapshot,
                    variable: index,
                },
                VariableScope::Caller(frame) => EvidenceLocation::SnapshotCallerVariable {
                    snapshot: self.snapshot,
                    frame,
                    variable: index,
                },
                VariableScope::Global => unreachable!("globals have no activation"),
            };
            let reference = self.summary.store().evidence_ref(location)?;
            self.summary.variable(&reference)
        }))
    }
}

/// One event belonging to a recorded allocation ID (not an address join).
/// The event's observation and overall history confidence are separate.
pub struct AllocationEventEvidence<'a> {
    pub event: &'a HeapEvent,
    pub position: usize,
    pub reference: EvidenceRef,
    pub observed_confidence: EvidenceConfidence,
    pub history_confidence: EvidenceConfidence,
}

pub struct InstructionEvidence<'a> {
    pub event: &'a InstructionEvent,
    pub position: usize,
    pub reference: EvidenceRef,
    pub tag_confidence: EvidenceConfidence,
}

pub struct HeapIntervalEvidence<'a> {
    pub event: &'a HeapEvent,
    pub position: usize,
    pub reference: EvidenceRef,
    /// Candidate timing in a retained-snapshot interval, not origin proof.
    pub interval_confidence: EvidenceConfidence,
}

impl<'a> HeapIntervalEvidence<'a> {
    /// Does not upgrade an interval to an exact-stop association.
    pub fn allocation(&self, view: &ValidatedSemanticView<'a>) -> QueryResult<HeapEvidence<'a>> {
        view.heap_reference(&self.reference)
    }
}

pub struct GapEvidence<'a> {
    pub gap: &'a TraceGap,
    pub position: usize,
    pub reference: EvidenceRef,
    pub window_confidence: EvidenceConfidence,
}

pub struct SourceLocationEvidence<'a> {
    pub expression: SourceExpressionEvidence<'a>,
    /// Exact recorded source location match, not executed expression.
    pub source_match_confidence: EvidenceConfidence,
}

//! TP-6: validated, trace-local *evidence*, not a C Semantic Engine.
//! Queries borrow canonical records. Validation runs once when the context is
//! created, never on each query. No address-to-identity or AST-to-execution join.

use crate::models::{
    AllocationLifetime, CompilerDiagnostic, Coverage, DataValidity, EvidenceFusionSummary,
    ExecutionStatus, HeapBlockInfo, HeapHistoryCoverage, MemoryEvidence, PointerTarget,
    ScopeStatus, StageResults, TraceGap, ValgrindError, VariableInfo,
};
use crate::{clang_ast, dwarf};

use super::evidence::{Evidence, EvidenceLocation, EvidenceRef};
use super::validation::{realloc_link, ReallocLink, ValidationReport};
use super::views::{CheckpointContext, HeapProcessScope, SemanticInputView};
use super::{AllocEvents, QueryError, QueryResult, TraceStore};

/// Confidence in the *specific stated relationship*, not execution success,
/// memory initialization, completeness of other processes or tool availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvidenceConfidence {
    Definite,
    Candidate,
    Ambiguous,
    Unavailable,
    Contradictory,
}

/// A single validation pass tied to one immutable TraceStore. Borrowed results
/// cannot outlive the store; a foreign EvidenceRef is rejected on resolution.
/// An unsound report conservatively prevents any query from claiming Definite.
pub struct ValidatedSemanticView<'a> {
    store: &'a TraceStore,
    report: ValidationReport,
}

impl TraceStore {
    /// Opt in to one deep-validation pass. `from_trace` remains schema-only.
    pub fn validated_semantic(&self) -> ValidatedSemanticView<'_> {
        ValidatedSemanticView {
            store: self,
            report: self.validate_deep(),
        }
    }
}

impl<'a> SemanticInputView<'a> {
    pub fn validated(self) -> ValidatedSemanticView<'a> {
        self.store().validated_semantic()
    }
}

impl<'a> ValidatedSemanticView<'a> {
    pub fn report(&self) -> &ValidationReport {
        &self.report
    }

    pub fn store(&self) -> &'a TraceStore {
        self.store
    }

    pub(crate) fn qualify(&self, confidence: EvidenceConfidence) -> EvidenceConfidence {
        if !self.report.is_structurally_sound() {
            EvidenceConfidence::Contradictory
        } else {
            confidence
        }
    }

    /// Exactly the referenced local, global or caller-frame observation.
    /// DWARF and AST links are optional static metadata, not execution proof.
    /// Metadata indexes resolve IDs to positions; no eager per-variable join.
    pub fn variable(&self, reference: &EvidenceRef) -> QueryResult<VariableEvidence<'a>> {
        let info = match self.store.resolve_evidence(reference)? {
            Evidence::SnapshotVariable(info)
            | Evidence::SnapshotGlobal(info)
            | Evidence::SnapshotCallerVariable(info) => info,
            _ => {
                return Err(QueryError::Unavailable {
                    what: "a variable evidence reference",
                })
            }
        };
        let type_entry = info
            .type_ref
            .as_deref()
            .and_then(|id| self.store.type_entry(id));
        let declaration = info
            .declaration_ref
            .as_deref()
            .and_then(|id| self.store.dwarf_declaration(id));
        let source_declaration = info
            .source_declaration_ref
            .as_deref()
            .and_then(|id| self.store.source_declaration(id).ok());
        let value_confidence = if info.read_status == DataValidity::Valid && !info.value.is_empty()
        {
            EvidenceConfidence::Definite // recorded debugger observation, not initialization
        } else {
            EvidenceConfidence::Unavailable
        };
        let memory = MemoryEvidenceView::from_variable(info, self.report.is_structurally_sound());
        let pointer = (info.points_to.is_some() || info.target.is_some())
            .then(|| PointerEvidence::new(info, self.qualify(EvidenceConfidence::Definite)));
        Ok(VariableEvidence {
            info,
            reference: reference.clone(),
            type_entry,
            declaration,
            source_declaration,
            value_confidence: self.qualify(value_confidence),
            type_confidence: self.qualify(if type_entry.is_some() {
                EvidenceConfidence::Definite
            } else {
                EvidenceConfidence::Unavailable
            }),
            declaration_confidence: self.qualify(if declaration.is_some() {
                EvidenceConfidence::Definite
            } else {
                EvidenceConfidence::Unavailable
            }),
            source_declaration_confidence: self.qualify(if source_declaration.is_some() {
                EvidenceConfidence::Definite
            } else {
                EvidenceConfidence::Unavailable
            }),
            memory,
            pointer,
        })
    }

    /// Explicit same-checkpoint resolution of a recorded variable identity.
    /// Never searches a different stop/trace or equates names/addresses. A
    /// duplicate identity remains ambiguous instead of picking a first hit.
    pub fn variable_target(
        &self,
        pointer: &VariableEvidence<'_>,
    ) -> QueryResult<Relationship<'a, VariableInfo>> {
        // Re-resolve first, so a reference from another store cannot pass.
        let source = self.variable(&pointer.reference)?;
        let (snapshot, _) = match pointer.reference.location() {
            EvidenceLocation::SnapshotVariable { snapshot, variable }
            | EvidenceLocation::SnapshotGlobal { snapshot, variable }
            | EvidenceLocation::SnapshotCallerVariable {
                snapshot, variable, ..
            } => (snapshot, variable),
            _ => {
                return Err(QueryError::Unavailable {
                    what: "a checkpoint pointer",
                })
            }
        };
        let Some(identity) = source
            .info
            .target
            .as_ref()
            .and_then(|target| target.variable_identity.as_deref())
        else {
            return Ok(Relationship {
                record: None,
                confidence: self.qualify(EvidenceConfidence::Unavailable),
            });
        };
        let s = self.store.snapshot(snapshot)?;
        let mut found = None;
        for info in s
            .variables
            .iter()
            .chain(&s.observation.globals)
            .chain(s.caller_frames.iter().flat_map(|frame| &frame.variables))
        {
            if info.identity.as_deref() == Some(identity) {
                if found.is_some() {
                    return Ok(Relationship {
                        record: None,
                        confidence: self.qualify(EvidenceConfidence::Ambiguous),
                    });
                }
                found = Some(info);
            }
        }
        let confidence = match found {
            None => EvidenceConfidence::Unavailable,
            Some(info) if info.scope_status == Some(ScopeStatus::Inactive) => {
                EvidenceConfidence::Contradictory
            }
            Some(_)
                if source.info.read_status != DataValidity::Valid
                    || source.info.points_to.is_none() =>
            {
                EvidenceConfidence::Unavailable
            }
            Some(info) if info.scope_status == Some(ScopeStatus::Active) => {
                EvidenceConfidence::Definite
            } // identity at this stop only
            Some(_) => EvidenceConfidence::Candidate, // scope evidence unavailable
        };
        Ok(Relationship {
            record: found,
            confidence: self.qualify(confidence),
        })
    }

    /// Resolve a trace-scoped allocation event/lifetime/block reference to
    /// its identity view. Foreign references fail before any numeric ID join.
    pub fn heap_reference(&self, reference: &EvidenceRef) -> QueryResult<HeapEvidence<'a>> {
        let id = match self.store.resolve_evidence(reference)? {
            Evidence::AllocationLifetime(record) => record.allocation_id,
            Evidence::HeapEvent(record) => record.allocation_id,
            Evidence::HeapState(record) | Evidence::SnapshotHeapBlock(record) => {
                record.allocation_id
            }
            _ => {
                return Err(QueryError::Unavailable {
                    what: "an allocation evidence reference",
                })
            }
        };
        Ok(self.heap(id))
    }

    /// Identity lookup uses TP-2's position-only heap index. A bare ID is a
    /// key in *this* trace, not a cross-trace reference. Returned events
    /// remain individually observed even when history is incomplete; a final
    /// state does not assert that intermediate events were retained.
    pub fn heap(&self, allocation_id: u64) -> HeapEvidence<'a> {
        let lifetime = self.store.allocation_lifetime(allocation_id);
        let block = self.store.heap_block(allocation_id);
        let old_id = block.and_then(|b| b.realloc_from_id);
        let old_lifetime = old_id.and_then(|id| self.store.allocation_lifetime(id));
        let history = self.store.heap_history();
        let pid_conflict = lifetime.is_some_and(|item| {
            self.store
                .heap_events_for_allocation(allocation_id)
                .any(|event| event.pid != item.pid)
        });
        let identity_confidence = if pid_conflict {
            EvidenceConfidence::Contradictory
        } else if allocation_id == 0 || (lifetime.is_none() && block.is_none()) {
            EvidenceConfidence::Unavailable
        } else if history.is_some_and(|h| h.complete)
            && lifetime.is_some_and(|item| {
                history.is_some_and(|h| h.observed_event_pids.contains(&item.pid))
            })
            && matches!(
                self.store.heap_process_scope(),
                HeapProcessScope::VerifiedObservedPidsOnly { .. }
            )
        {
            EvidenceConfidence::Definite // explicit observed-run ID only
        } else {
            EvidenceConfidence::Candidate // recorded ID, incomplete or legacy history
        };
        HeapEvidence {
            store: self.store,
            allocation_id,
            lifetime,
            block,
            old_id,
            old_lifetime,
            history,
            memory: block
                .map(|b| MemoryEvidenceView::from_block(b, self.report.is_structurally_sound())),
            identity_confidence: self.qualify(identity_confidence),
            realloc_confidence: self.qualify(
                match block.map(|block| {
                    realloc_link(
                        block,
                        lifetime,
                        old_lifetime,
                        self.store.heap_events_for_allocation(allocation_id),
                        self.store
                            .heap_events_for_allocation(block.realloc_from_id.unwrap_or(0)),
                    )
                }) {
                    Some(ReallocLink::Contradictory) => EvidenceConfidence::Contradictory,
                    Some(ReallocLink::Ambiguous) => EvidenceConfidence::Ambiguous,
                    Some(ReallocLink::Verified)
                        if identity_confidence == EvidenceConfidence::Definite =>
                    {
                        EvidenceConfidence::Definite
                    }
                    Some(ReallocLink::Verified) => EvidenceConfidence::Candidate, // incomplete/legacy scope
                    Some(ReallocLink::Unavailable) | None => EvidenceConfidence::Unavailable,
                },
            ),
        }
    }

    /// An AST expression is static source evidence; execution is never proven
    /// by a compatible source call or a matching checkpoint.
    pub fn source_expression(&self, id: &str) -> SourceExpressionEvidence<'a> {
        let expression = self.store.source_expression(id).ok();
        let registry = self.store.trace().source_metadata.as_ref();
        let candidates = self.store.indexes().metadata.candidate_positions(id);
        let ambiguities = self.store.indexes().metadata.ambiguity_positions(id);
        let candidate = !candidates.is_empty();
        let ambiguous = !ambiguities.is_empty();
        SourceExpressionEvidence {
            expression,
            registry,
            events: self.store.heap_events(),
            candidates,
            ambiguities,
            referenced_declaration: expression
                .and_then(|e| e.referenced_declaration.as_deref())
                .and_then(|id| self.store.source_declaration(id).ok()),
            fusion: self.store.evidence_fusion(),
            heap_call_confidence: self.qualify(if ambiguous {
                EvidenceConfidence::Ambiguous
            } else if candidate {
                EvidenceConfidence::Candidate
            } else {
                EvidenceConfidence::Unavailable
            }),
            execution_confidence: self.qualify(EvidenceConfidence::Unavailable),
        }
    }

    /// Returns the canonical checkpoint plus stop-local source and attempted
    /// instruction position; gaps/retention are separate evidence below.
    pub fn checkpoint(&self, index: usize) -> QueryResult<CheckpointEvidence<'a>> {
        let context = self.store.semantic_input().checkpoint_context(index)?;
        Ok(CheckpointEvidence {
            observation: &context.snapshot().observation,
            context,
            confidence: self.qualify(EvidenceConfidence::Definite),
        })
    }

    pub fn execution(&self) -> ExecutionEvidence<'a> {
        ExecutionEvidence {
            status: self.store.execution_status(),
            coverage: self.store.coverage(),
            gaps: self.store.trace_gaps(),
            attempted: self.store.attempted_instruction_count(),
            retained: self.store.retained_event_count(),
            confidence: self.qualify(if self.store.coverage().complete_for_scope {
                EvidenceConfidence::Definite
            } else {
                EvidenceConfidence::Unavailable
            }),
        }
    }

    pub fn diagnostics(&self) -> DiagnosticEvidence<'a> {
        DiagnosticEvidence {
            compile: self.store.compile_errors(),
            stages: self.store.stage_results(),
            crash: self.store.crash_report(),
            sanitizer: self.store.sanitizer_report(),
            valgrind: self.store.valgrind_report(),
        }
    }

    /// Independent Valgrind error. Neither raw related IDs nor source-site
    /// candidates are translated into primary-run allocation identity.
    pub fn valgrind_error(&self, index: usize) -> QueryResult<ValgrindEvidence<'a>> {
        let reference = self
            .store
            .evidence_ref(EvidenceLocation::ValgrindError(index))?;
        let Evidence::ValgrindError(error) = self.store.resolve_evidence(&reference)? else {
            unreachable!()
        };
        Ok(ValgrindEvidence {
            error,
            reference,
            source_confidence: self.qualify(if error.source_site_candidates.is_empty() {
                EvidenceConfidence::Unavailable
            } else {
                EvidenceConfidence::Candidate
            }),
            primary_allocation_id: None,
        })
    }

    /// Unknown child activity stays unknown even if observed PID history is
    /// complete. Execution coverage is a separate dimension.
    pub fn process_coverage(&self) -> ProcessCoverageEvidence<'a> {
        ProcessCoverageEvidence {
            scope: self.store.heap_process_scope(),
            history: self.store.heap_history(),
            unobserved_descendants: self.qualify(EvidenceConfidence::Unavailable),
        }
    }
}

/// Borrowed join at one variable location. `reference` is trace-scoped;
/// `info.identity` identifies a runtime object, not a source declaration.
pub struct VariableEvidence<'a> {
    pub info: &'a VariableInfo,
    pub reference: EvidenceRef,
    pub type_entry: Option<&'a dwarf::TypeEntry>,
    pub declaration: Option<&'a dwarf::Declaration>,
    pub source_declaration: Option<&'a clang_ast::SourceDeclaration>,
    pub value_confidence: EvidenceConfidence,
    pub type_confidence: EvidenceConfidence,
    pub declaration_confidence: EvidenceConfidence,
    pub source_declaration_confidence: EvidenceConfidence,
    pub memory: MemoryEvidenceView<'a>,
    pub pointer: Option<PointerEvidence<'a>>,
}

pub struct Relationship<'a, T> {
    pub record: Option<&'a T>,
    pub confidence: EvidenceConfidence,
}

/// Hex bytes belong to the variable itself. `valid_range` is a prefix of
/// captured bytes only, never the object's full extent or initialized bytes.
pub struct MemoryEvidenceView<'a> {
    pub hex: Option<&'a str>,
    pub observation: Option<&'a MemoryEvidence>,
    pub bytes_status: DataValidity,
    pub valid_range: Option<std::ops::Range<u64>>,
    /// Requested but not captured: inspect the reason, do not assume unreadable.
    pub unobserved_range: Option<std::ops::Range<u64>>,
    pub confidence: EvidenceConfidence,
}

impl<'a> MemoryEvidenceView<'a> {
    fn from_variable(info: &'a VariableInfo, sound: bool) -> Self {
        Self::new(
            info.raw_bytes_hex.as_deref(),
            info.memory_evidence.as_ref(),
            info.bytes_status.clone(),
            sound,
        )
    }

    fn from_block(block: &'a HeapBlockInfo, sound: bool) -> Self {
        let bytes_status = match block.memory_evidence.as_ref().map(|m| m.status.as_str()) {
            Some("full") => DataValidity::Valid,
            Some("partial" | "truncated") => DataValidity::Truncated,
            _ => DataValidity::NotCaptured,
        };
        Self::new(
            block.raw_bytes_hex.as_deref(),
            block.memory_evidence.as_ref(),
            bytes_status,
            sound,
        )
    }

    fn new(
        hex: Option<&'a str>,
        observation: Option<&'a MemoryEvidence>,
        bytes_status: DataValidity,
        sound: bool,
    ) -> Self {
        let valid_range = observation.and_then(|m| {
            let bytes = hex?;
            let readable = matches!(bytes_status, DataValidity::Valid | DataValidity::Truncated)
                && matches!(m.status.as_str(), "full" | "partial" | "truncated")
                && m.captured_bytes <= m.requested_bytes
                && bytes.len() as u64 == m.captured_bytes.checked_mul(2)?
                && bytes.bytes().all(|byte| byte.is_ascii_hexdigit());
            readable.then_some(0..m.captured_bytes)
        });
        let unobserved_range = observation.and_then(|m| {
            (m.captured_bytes < m.requested_bytes).then_some(m.captured_bytes..m.requested_bytes)
        });
        let confidence = if !sound {
            EvidenceConfidence::Contradictory
        } else if valid_range.is_some() {
            if observation
                .is_some_and(|m| m.status == "full" && m.captured_bytes == m.requested_bytes)
                && bytes_status == DataValidity::Valid
            {
                EvidenceConfidence::Definite // captured bytes, not value initialization
            } else {
                EvidenceConfidence::Candidate // prefix only
            }
        } else {
            EvidenceConfidence::Unavailable
        };
        Self {
            hex,
            observation,
            bytes_status,
            valid_range,
            unobserved_range,
            confidence,
        }
    }
}

pub struct PointerEvidence<'a> {
    pub address: Option<&'a str>,
    pub target: Option<&'a PointerTarget>, // raw recorded target; never certification
    pub region: Option<&'a str>,
    pub address_candidate: Option<u64>,
    pub provenance_status: Option<&'a str>,
    /// Always None with the present V2.2 status contract: address-only and
    /// legacy status absence cannot establish pointer origin.
    pub definitive_allocation: Option<u64>,
    pub confidence: EvidenceConfidence,
}

impl<'a> PointerEvidence<'a> {
    fn new(info: &'a VariableInfo, qualified: EvidenceConfidence) -> Self {
        let target = info.target.as_ref();
        let status = target.and_then(|t| t.provenance_status.as_deref());
        let confidence = if qualified == EvidenceConfidence::Contradictory {
            qualified
        } else if info.read_status != DataValidity::Valid
            || info
                .points_to
                .as_deref()
                .and_then(crate::memory::address)
                .is_none()
        {
            EvidenceConfidence::Unavailable
        } else if target.is_some_and(|t| {
            (info.points_to.as_deref().and_then(crate::memory::address) == Some(0))
                != (t.kind == "null")
        }) {
            EvidenceConfidence::Contradictory
        } else {
            match status {
                Some("ambiguous_reuse") => EvidenceConfidence::Ambiguous,
                Some("address_only" | "known_freed_address") => EvidenceConfidence::Candidate,
                Some("history_incomplete") => EvidenceConfidence::Unavailable,
                Some(_) => EvidenceConfidence::Unavailable,
                None if target.is_some_and(|t| t.kind == "null") => EvidenceConfidence::Definite, // null address, never heap identity
                None if target.is_some_and(|t| t.variable_identity.is_some()) => {
                    EvidenceConfidence::Candidate
                }
                _ => EvidenceConfidence::Unavailable,
            }
        };
        Self {
            address: info.points_to.as_deref(),
            target,
            region: target.map(|t| t.kind.as_str()),
            address_candidate: target.and_then(|t| t.address_match_allocation_id),
            provenance_status: status,
            definitive_allocation: None,
            confidence,
        }
    }
}

pub struct HeapEvidence<'a> {
    store: &'a TraceStore,
    pub allocation_id: u64,
    pub lifetime: Option<&'a AllocationLifetime>,
    pub block: Option<&'a HeapBlockInfo>,
    pub old_id: Option<u64>,
    pub old_lifetime: Option<&'a AllocationLifetime>,
    pub history: Option<&'a HeapHistoryCoverage>,
    /// Historical bytes at `block.observed_at_stop`, not live memory now.
    pub memory: Option<MemoryEvidenceView<'a>>,
    pub identity_confidence: EvidenceConfidence,
    pub realloc_confidence: EvidenceConfidence,
}

impl HeapEvidence<'_> {
    pub fn events(&self) -> AllocEvents<'_> {
        self.store.heap_events_for_allocation(self.allocation_id)
    }
}

pub struct SourceExpressionEvidence<'a> {
    pub expression: Option<&'a clang_ast::SourceExpression>,
    registry: Option<&'a clang_ast::SourceMetadataRegistry>,
    events: &'a [crate::models::HeapEvent],
    candidates: &'a [u32],
    ambiguities: &'a [u32],
    pub referenced_declaration: Option<&'a clang_ast::SourceDeclaration>,
    pub fusion: Option<&'a EvidenceFusionSummary>,
    /// Relation to a recorded heap event's compatible *source line* only.
    pub heap_call_confidence: EvidenceConfidence,
    /// A source expression, even as a call candidate, never proves execution.
    pub execution_confidence: EvidenceConfidence,
}

/// Candidate relationship between an observed heap event and compatible
/// source expression. `event_position` is not an execution proof for `expression`.
pub struct SourceCallSite<'a> {
    pub event_position: usize,
    pub event: &'a crate::models::HeapEvent,
    pub expression: &'a clang_ast::SourceExpression,
    pub evidence: &'a str,
    pub confidence: EvidenceConfidence,
}

impl<'a> SourceExpressionEvidence<'a> {
    pub fn heap_call_candidates(
        &self,
    ) -> impl Iterator<Item = &'a clang_ast::HeapCallCandidate> + '_ {
        self.expression
            .into_iter()
            .flat_map(move |_| self.candidates.iter())
            .filter_map(move |position| self.registry?.heap_call_candidates.get(*position as usize))
    }

    pub fn call_sites(&self) -> impl Iterator<Item = SourceCallSite<'a>> + '_ {
        self.heap_call_candidates().filter_map(move |candidate| {
            Some(SourceCallSite {
                event_position: candidate.event_index,
                event: self.events.get(candidate.event_index)?,
                expression: self.expression?,
                evidence: &candidate.evidence,
                confidence: if self.heap_call_confidence == EvidenceConfidence::Contradictory {
                    EvidenceConfidence::Contradictory
                } else {
                    EvidenceConfidence::Candidate
                },
            })
        })
    }

    pub fn ambiguous_call_sites(&self) -> impl Iterator<Item = SourceCallSite<'a>> + '_ {
        self.expression
            .into_iter()
            .flat_map(move |_| self.ambiguities.iter())
            .filter_map(move |position| {
                let entry = self
                    .registry?
                    .heap_call_ambiguities
                    .get(*position as usize)?;
                Some(SourceCallSite {
                    event_position: entry.event_index,
                    event: self.events.get(entry.event_index)?,
                    expression: self.expression?,
                    evidence: &entry.evidence,
                    confidence: if self.heap_call_confidence == EvidenceConfidence::Contradictory {
                        EvidenceConfidence::Contradictory
                    } else {
                        EvidenceConfidence::Ambiguous
                    },
                })
            })
    }
}

pub struct CheckpointEvidence<'a> {
    pub context: CheckpointContext<'a>,
    pub observation: &'a crate::models::SnapshotObservation,
    pub confidence: EvidenceConfidence,
}

impl CheckpointEvidence<'_> {
    pub fn location(&self) -> Option<(&str, u32, &str)> {
        let (file, line) = self.context.source_location()?;
        Some((file, line, &self.context.snapshot().func))
    }
}

pub struct ExecutionEvidence<'a> {
    pub status: &'a ExecutionStatus,
    pub coverage: &'a Coverage,
    pub gaps: &'a [TraceGap],
    pub attempted: u64,
    pub retained: usize,
    /// Completeness only for the engine's declared tracing scope.
    pub confidence: EvidenceConfidence,
}

pub struct DiagnosticEvidence<'a> {
    pub compile: &'a [CompilerDiagnostic],
    pub stages: &'a StageResults,
    pub crash: Option<&'a crate::models::CrashReport>,
    pub sanitizer: Option<&'a crate::models::SanitizerReport>,
    pub valgrind: Option<&'a crate::models::ValgrindReport>,
}

pub struct ValgrindEvidence<'a> {
    pub error: &'a ValgrindError,
    pub reference: EvidenceRef,
    pub source_confidence: EvidenceConfidence,
    /// No same-execution exact Valgrind↔primary-run join is supported.
    pub primary_allocation_id: Option<u64>,
}

pub struct ProcessCoverageEvidence<'a> {
    pub scope: HeapProcessScope<'a>,
    pub history: Option<&'a HeapHistoryCoverage>,
    pub unobserved_descendants: EvidenceConfidence,
}

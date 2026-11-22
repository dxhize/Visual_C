//! TP-2: deep cross-record referential validation.
//!
//! TP-1 ingestion stays permissive (schema-version gate plus structural
//! shape); this layer is the *explicit* referential check —
//! [`TraceStore::validate_deep`](super::TraceStore::validate_deep) runs it on
//! demand. It never rejects a trace: it returns a typed report whose
//! findings are separated into the four TP-2 severities, so legitimately
//! partial traces (crashes, timeouts, retention gaps, skipped tool stages,
//! unavailable memory) validate as *sound but incomplete*, while genuinely
//! corrupted records are reported as corruption.
//!
//! # Severity contract
//!
//! * [`ValidationSeverity::StructuralCorruption`] — a record violates an
//!   invariant Stage 1 guarantees (verified against the recording code
//!   paths, never inferred from field names). The dependent queries refuse
//!   to answer rather than return wrong results.
//! * [`ValidationSeverity::PartialCoverage`] — the trace legitimately does
//!   not contain every observation (retention settings, caps, journal
//!   truncation). Recorded behavior, not a defect.
//! * [`ValidationSeverity::UnavailableEvidence`] — a relationship cannot be
//!   established because the evidence was not recorded (missing source
//!   context, unreadable memory, unresolved allocation references). The
//!   query layer models the same situations with
//!   [`QueryError::Unavailable`](super::error::QueryError::Unavailable).
//! * [`ValidationSeverity::OptionalAbsence`] — the schema permits the field
//!   to be missing; noted for consumers, never a defect.
//!
//! Findings are aggregated by severity + code with the first occurrence's
//! location and a repetition count, so one pass over a 94k-event trace
//! yields a handful of findings, not one per record.
//!
//! The engine's own structural validator
//! ([`crate::processor::validate`]) is *reused*, not duplicated: its failure
//! surfaces as the `ENGINE_STRUCTURAL_VALIDATION` finding.

use std::collections::{HashMap, HashSet};

use crate::models::{
    DataValidity, DebugSessionTrace, ExecutionSnapshot, HeapEventType, InstructionEvent,
    ScopeStatus, VariableInfo,
};

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

/// How a finding relates to the recording contract (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValidationSeverity {
    /// A guaranteed invariant is violated — corruption, not partiality.
    StructuralCorruption,
    /// The trace legitimately does not contain every observation.
    PartialCoverage,
    /// A relationship cannot be established from recorded evidence.
    UnavailableEvidence,
    /// The schema permits the field to be absent; noted for consumers.
    OptionalAbsence,
}

impl ValidationSeverity {
    /// Human label used in reports and documentation.
    pub fn label(self) -> &'static str {
        match self {
            ValidationSeverity::StructuralCorruption => "structural corruption",
            ValidationSeverity::PartialCoverage => "partial coverage",
            ValidationSeverity::UnavailableEvidence => "unavailable evidence",
            ValidationSeverity::OptionalAbsence => "optional absence",
        }
    }
}

/// Where a finding was first observed. Indices are positions in the
/// canonical collections (see the reference dictionary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationLocation {
    /// Whole-trace context (coverage, reports, counts).
    Trace,
    /// `instruction_events[position]`.
    InstructionEvent(usize),
    /// `snapshots[position]`.
    Snapshot(usize),
    /// `snapshots[snapshot].variables[variable]`.
    SnapshotVariable {
        snapshot: usize,
        variable: usize,
    },
    SnapshotGlobal {
        snapshot: usize,
        variable: usize,
    },
    /// `snapshots[snapshot].caller_frames[frame]` (position, not GDB level).
    SnapshotCallerFrame {
        snapshot: usize,
        frame: usize,
    },
    SnapshotCallerVariable {
        snapshot: usize,
        frame: usize,
        variable: usize,
    },
    /// `snapshots[snapshot].heap_blocks[block]`.
    SnapshotHeapBlock {
        snapshot: usize,
        block: usize,
    },
    /// `heap_events[position]`.
    HeapEvent(usize),
    /// `heap_state[position]`.
    HeapState(usize),
    /// `recording.allocation_lifetimes[position]`.
    AllocationLifetime(usize),
    /// `recording.trace_gaps[position]`.
    TraceGap(usize),
    /// `recording.source_checkpoints[position]`.
    SourceCheckpoint(usize),
}

impl std::fmt::Display for ValidationLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationLocation::Trace => write!(f, "trace"),
            ValidationLocation::InstructionEvent(i) => write!(f, "instruction_events[{i}]"),
            ValidationLocation::Snapshot(i) => write!(f, "snapshots[{i}]"),
            ValidationLocation::SnapshotVariable { snapshot, variable } => {
                write!(f, "snapshots[{snapshot}].variables[{variable}]")
            }
            ValidationLocation::SnapshotGlobal { snapshot, variable } => {
                write!(f, "snapshots[{snapshot}].observation.globals[{variable}]")
            }
            ValidationLocation::SnapshotCallerFrame { snapshot, frame } => {
                write!(f, "snapshots[{snapshot}].caller_frames[{frame}]")
            }
            ValidationLocation::SnapshotCallerVariable {
                snapshot,
                frame,
                variable,
            } => {
                write!(
                    f,
                    "snapshots[{snapshot}].caller_frames[{frame}].variables[{variable}]"
                )
            }
            ValidationLocation::SnapshotHeapBlock { snapshot, block } => {
                write!(f, "snapshots[{snapshot}].heap_blocks[{block}]")
            }
            ValidationLocation::HeapEvent(i) => write!(f, "heap_events[{i}]"),
            ValidationLocation::HeapState(i) => write!(f, "heap_state[{i}]"),
            ValidationLocation::AllocationLifetime(i) => {
                write!(f, "allocation_lifetimes[{i}]")
            }
            ValidationLocation::TraceGap(i) => write!(f, "trace_gaps[{i}]"),
            ValidationLocation::SourceCheckpoint(i) => write!(f, "source_checkpoints[{i}]"),
        }
    }
}

/// One aggregated validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationFinding {
    /// Severity class (never conflated — see the module contract).
    pub severity: ValidationSeverity,
    /// Stable machine-readable code (e.g. `EVENT_STOP_ORDER`).
    pub code: &'static str,
    /// Human explanation with the recorded values involved.
    pub detail: String,
    /// Where the finding was first observed.
    pub location: ValidationLocation,
    /// How many records exhibited this code (1 when unrepeatable).
    pub occurrences: u32,
}

impl std::fmt::Display for ValidationFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} at {} (x{}): {}",
            self.severity.label(),
            self.code,
            self.location,
            self.occurrences,
            self.detail
        )
    }
}

/// The complete result of one deep-validation pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    findings: Vec<ValidationFinding>,
}

impl ValidationReport {
    /// All findings, corruptions first, then partial-coverage, unavailable,
    /// and optional-absence groups; code order within each group.
    pub fn findings(&self) -> &[ValidationFinding] {
        &self.findings
    }

    /// Findings of one severity.
    pub fn of_severity(
        &self,
        severity: ValidationSeverity,
    ) -> impl Iterator<Item = &ValidationFinding> {
        self.findings
            .iter()
            .filter(move |finding| finding.severity == severity)
    }

    /// `true` when no structural corruption was found. Partial coverage,
    /// unavailable evidence, and optional absence do **not** make a trace
    /// unsound — that is the point of the severity split.
    pub fn is_structurally_sound(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|finding| finding.severity == ValidationSeverity::StructuralCorruption)
    }

    /// Number of corruption findings (0 for a sound trace).
    pub fn corruption_count(&self) -> usize {
        self.of_severity(ValidationSeverity::StructuralCorruption)
            .count()
    }

    /// `true` when any partial-coverage finding was recorded.
    pub fn is_partial(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == ValidationSeverity::PartialCoverage)
    }
}

// ---------------------------------------------------------------------------
// Validation pass
// ---------------------------------------------------------------------------

/// Run the deep referential validation over a canonical trace.
///
/// Complexity: a constant number of linear passes over the trace
/// collections (O(E + S + H + B + L + G)); no indexes are required, so it
/// also runs over stores built before this layer existed.
pub fn validate_deep(trace: &DebugSessionTrace) -> ValidationReport {
    let mut builder = Builder::default();

    // (0) The engine's own structural validator, reused verbatim.
    if let Err(reason) = crate::processor::validate(trace) {
        builder.push(
            ValidationSeverity::StructuralCorruption,
            "ENGINE_STRUCTURAL_VALIDATION",
            reason,
            ValidationLocation::Trace,
        );
    }

    validate_instruction_events(trace, &mut builder);
    validate_snapshots(trace, &mut builder);
    validate_source_checkpoints(trace, &mut builder);
    validate_heap(trace, &mut builder);
    validate_dwarf_registry(trace, &mut builder);
    validate_ast_registry(trace, &mut builder);
    validate_source_links(trace, &mut builder);
    validate_gaps(trace, &mut builder);
    validate_coverage(trace, &mut builder);
    validate_diagnostics(trace, &mut builder);

    builder.finish()
}

fn validate_dwarf_registry(trace: &DebugSessionTrace, builder: &mut Builder) {
    let Some(registry) = &trace.type_registry else {
        return;
    };
    for (label, ids, count) in [
        (
            "type",
            registry
                .types
                .iter()
                .map(|item| item.id.as_str())
                .collect::<HashSet<_>>()
                .len(),
            registry.types.len(),
        ),
        (
            "declaration",
            registry
                .declarations
                .iter()
                .map(|item| item.id.as_str())
                .collect::<HashSet<_>>()
                .len(),
            registry.declarations.len(),
        ),
        (
            "compilation unit",
            registry
                .units
                .iter()
                .map(|item| item.id.as_str())
                .collect::<HashSet<_>>()
                .len(),
            registry.units.len(),
        ),
    ] {
        if ids != count {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "DWARF_DUPLICATE_ID",
                format!("duplicate trace-local DWARF {label} identity"),
                ValidationLocation::Trace,
            );
        }
    }
}

fn validate_ast_registry(trace: &DebugSessionTrace, builder: &mut Builder) {
    let Some(registry) = &trace.source_metadata else {
        return;
    };
    let declarations: std::collections::HashSet<_> = registry
        .declarations
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    let expressions: std::collections::HashSet<_> = registry
        .expressions
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    if declarations.len() != registry.declarations.len()
        || expressions.len() != registry.expressions.len()
    {
        builder.push(
            ValidationSeverity::StructuralCorruption,
            "AST_DUPLICATE_ID",
            "duplicate source-registry identity",
            ValidationLocation::Trace,
        );
    }
    let units: HashMap<&str, &str> = registry
        .translation_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit.file.as_str()))
        .collect();
    let declaration_by_id: HashMap<&str, &crate::clang_ast::SourceDeclaration> = registry
        .declarations
        .iter()
        .map(|decl| (decl.id.as_str(), decl))
        .collect();
    let expression_by_id: HashMap<&str, &crate::clang_ast::SourceExpression> = registry
        .expressions
        .iter()
        .map(|expr| (expr.id.as_str(), expr))
        .collect();
    let dwarf_by_id: HashMap<&str, &crate::dwarf::Declaration> = trace
        .type_registry
        .as_ref()
        .map(|r| {
            r.declarations
                .iter()
                .map(|decl| (decl.id.as_str(), decl))
                .collect()
        })
        .unwrap_or_default();
    let dwarf_units: HashMap<&str, &crate::dwarf::UnitInfo> = trace
        .type_registry
        .as_ref()
        .map(|r| {
            r.units
                .iter()
                .map(|unit| (unit.id.as_str(), unit))
                .collect()
        })
        .unwrap_or_default();
    let dwarf_ids: std::collections::HashSet<_> = trace
        .type_registry
        .as_ref()
        .map(|registry| {
            registry
                .declarations
                .iter()
                .map(|item| item.id.as_str())
                .collect()
        })
        .unwrap_or_default();
    let dwarf_types: HashMap<&str, &crate::dwarf::TypeEntry> = trace
        .type_registry
        .as_ref()
        .map(|registry| {
            registry
                .types
                .iter()
                .map(|entry| (entry.id.as_str(), entry))
                .collect()
        })
        .unwrap_or_default();
    for decl in &registry.declarations {
        if !source_unit_consistent(&decl.unit_id, &decl.span.file, &units)
            || decl
                .dwarf_declaration
                .as_deref()
                .and_then(|id| dwarf_by_id.get(id))
                .is_some_and(|dwarf| {
                    decl.name != dwarf.name.as_deref().unwrap_or(&decl.name)
                        || dwarf
                            .file
                            .as_deref()
                            .is_some_and(|file| !same_source_file(&decl.span.file, file))
                        || (decl.span.provenance == "direct"
                            && decl
                                .name_line
                                .zip(dwarf.line)
                                .is_some_and(|(left, right)| left != right))
                        || decl
                            .function
                            .as_deref()
                            .zip(dwarf.function.as_deref())
                            .is_some_and(|(left, right)| left != right)
                        || dwarf_units
                            .get(dwarf.unit_id.as_str())
                            .zip(units.get(decl.unit_id.as_str()).copied())
                            .is_some_and(|(unit, file)| !dwarf_unit_matches(unit, file))
                })
            || decl
                .dwarf_declaration
                .as_deref()
                .and_then(|id| dwarf_by_id.get(id))
                .is_some_and(|dwarf| {
                    decl.kind != "FieldDecl"
                        && decl
                            .dwarf_type_ref
                            .as_deref()
                            .zip(dwarf.type_ref.as_deref())
                            .is_some_and(|(left, right)| left != right)
                })
            || decl.dwarf_member.as_ref().is_some_and(|member| {
                !member_parent_matches(decl.parent_type_spelling.as_deref(), member, &dwarf_types)
                    || decl.name != member.name
                    || decl
                        .dwarf_type_ref
                        .as_deref()
                        .zip(member_type_ref(member, &dwarf_types))
                        .is_some_and(|(left, right)| left != right)
            })
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "AST_DECLARATION_RELATION_INVALID",
                "source declaration contradicts its unit, DWARF identity or containing member type",
                ValidationLocation::Trace,
            );
        }
        if decl
            .initializer
            .as_deref()
            .is_some_and(|id| !expressions.contains(id))
            || decl
                .dwarf_declaration
                .as_deref()
                .is_some_and(|id| !dwarf_ids.contains(id))
            || decl
                .dwarf_type_ref
                .as_deref()
                .is_some_and(|id| !dwarf_types.contains_key(id))
            || decl
                .dwarf_member
                .as_ref()
                .is_some_and(|member| !valid_member_ref(member, &dwarf_types))
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "AST_DECLARATION_REFERENCE_INVALID",
                "source declaration has an unresolved serialized reference",
                ValidationLocation::Trace,
            );
        }
    }
    for expr in &registry.expressions {
        let referenced = expr
            .referenced_declaration
            .as_deref()
            .and_then(|id| declaration_by_id.get(id).copied());
        if !source_unit_consistent(&expr.unit_id, &expr.span.file, &units)
            || expr
                .operand_ids
                .iter()
                .filter_map(|id| expression_by_id.get(id.as_str()))
                .any(|operand| operand.unit_id != expr.unit_id)
            || (expr.referenced_dwarf_declaration.is_some() && referenced.is_none())
            || (expr.dwarf_type_evidence.as_deref() == Some("referenced_declaration_type")
                && referenced.is_none())
            || referenced.is_some_and(|decl| {
                decl.unit_id != expr.unit_id
                    || expr
                        .referenced_dwarf_declaration
                        .as_deref()
                        .is_some_and(|id| decl.dwarf_declaration.as_deref() != Some(id))
                    || expr.dwarf_type_evidence.as_deref() == Some("referenced_declaration_type")
                        && expr.dwarf_type_ref.as_deref() != decl.dwarf_type_ref.as_deref()
                    || expr
                        .member_ref
                        .as_ref()
                        .zip(decl.dwarf_member.as_ref())
                        .is_some_and(|(left, right)| left != right)
            })
            || expr.member_ref.as_ref().is_some_and(|member| {
                expr.kind != "MemberExpr"
                    || expr
                        .member_name
                        .as_deref()
                        .is_some_and(|name| name != member.name)
                    || !member_parent_matches(
                        expr.base_type_spelling.as_deref(),
                        member,
                        &dwarf_types,
                    )
                    || expr.dwarf_type_evidence.as_deref()
                        == Some("unique_parent_type_and_member_name")
                        && expr
                            .dwarf_type_ref
                            .as_deref()
                            .zip(member_type_ref(member, &dwarf_types))
                            .is_some_and(|(left, right)| left != right)
            })
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "AST_EXPRESSION_RELATION_INVALID",
                "source expression contradicts its unit, declaration, member or type evidence",
                ValidationLocation::Trace,
            );
        }
        if expr
            .operand_ids
            .iter()
            .any(|id| !expressions.contains(id.as_str()))
            || expr
                .referenced_declaration
                .as_deref()
                .is_some_and(|id| !declarations.contains(id))
            || expr
                .referenced_dwarf_declaration
                .as_deref()
                .is_some_and(|id| !dwarf_ids.contains(id))
            || expr
                .dwarf_type_ref
                .as_deref()
                .is_some_and(|id| !dwarf_types.contains_key(id))
            || expr
                .member_ref
                .as_ref()
                .is_some_and(|member| !valid_member_ref(member, &dwarf_types))
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "AST_EXPRESSION_REFERENCE_INVALID",
                "source expression has an unresolved serialized reference",
                ValidationLocation::Trace,
            );
        }
    }
    for candidate in &registry.heap_call_candidates {
        if trace
            .heap_events
            .get(candidate.event_index)
            .zip(expression_by_id.get(candidate.expression_id.as_str()))
            .is_some_and(|(event, expr)| !compatible_heap_call(event, expr))
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "AST_HEAP_CANDIDATE_RELATION_INVALID",
                "heap-call candidate contradicts operation or resolved source site",
                ValidationLocation::Trace,
            );
        }
        if candidate.event_index >= trace.heap_events.len()
            || !expressions.contains(candidate.expression_id.as_str())
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "AST_HEAP_CANDIDATE_INVALID",
                "source-call candidate references an absent record",
                ValidationLocation::Trace,
            );
        }
    }
    for ambiguity in &registry.heap_call_ambiguities {
        if let Some(event) = trace.heap_events.get(ambiguity.event_index) {
            if ambiguity
                .expression_ids
                .iter()
                .filter_map(|id| expression_by_id.get(id.as_str()))
                .any(|expr| !compatible_heap_call(event, expr))
            {
                builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "AST_HEAP_AMBIGUITY_RELATION_INVALID",
                    "heap-call ambiguity includes an incompatible call",
                    ValidationLocation::Trace,
                );
            }
        }
        if ambiguity.event_index >= trace.heap_events.len()
            || ambiguity.expression_ids.is_empty()
            || ambiguity
                .expression_ids
                .iter()
                .any(|id| !expressions.contains(id.as_str()))
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "AST_HEAP_AMBIGUITY_INVALID",
                "source-call ambiguity references an absent record",
                ValidationLocation::Trace,
            );
        }
    }
}

/// Compare only source locations that carry explicit file identity; never
/// identify two files by basename. Missing optional metadata is not a fault.
fn same_source_file(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let left = std::path::Path::new(a);
    let right = std::path::Path::new(b);
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        // Absolute paths still give an explicit identity when the source is
        // no longer on disk. A relative path without its base is ambiguous.
        _ if left.is_absolute() && right.is_absolute() => left == right,
        _ => true,
    }
}

fn dwarf_unit_matches(unit: &crate::dwarf::UnitInfo, source: &str) -> bool {
    let Some(name) = unit.name.as_deref().filter(|name| *name != "<artificial>") else {
        return true;
    };
    let path = std::path::Path::new(name);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(dir) = unit.compilation_dir.as_deref() {
        std::path::Path::new(dir).join(path)
    } else {
        // A basename with no compilation directory cannot prove a CU mismatch.
        return true;
    };
    same_source_file(&resolved.to_string_lossy(), source)
}

fn source_unit_consistent(unit: &str, _span_file: &str, units: &HashMap<&str, &str>) -> bool {
    // A header or macro span can legitimately differ from its TU main file;
    // unit existence is still required when a TU registry is available.
    units.contains_key(unit) || units.is_empty()
}

fn member_type_ref<'a>(
    member: &crate::clang_ast::DwarfMemberRef,
    types: &HashMap<&str, &'a crate::dwarf::TypeEntry>,
) -> Option<&'a str> {
    types
        .get(member.type_ref.as_str())?
        .members
        .get(member.member_index)?
        .type_ref
        .as_deref()
}

fn member_parent_matches(
    spelling: Option<&str>,
    member: &crate::clang_ast::DwarfMemberRef,
    types: &HashMap<&str, &crate::dwarf::TypeEntry>,
) -> bool {
    let (Some(spelling), Some(parent)) = (spelling, types.get(member.type_ref.as_str())) else {
        return true;
    };
    let mut words = spelling.split_whitespace();
    let Some(kind @ ("struct" | "union")) = words.next() else {
        return true;
    };
    let Some(name) = words.next() else {
        return true;
    };
    parent.kind == kind && parent.name.as_deref() == Some(name.trim_end_matches('*'))
}

fn compatible_heap_call(
    event: &crate::models::HeapEvent,
    expr: &crate::clang_ast::SourceExpression,
) -> bool {
    let operation_matches = matches!(
        (expr.call_kind.as_deref(), &event.event_type),
        (Some("malloc" | "calloc"), HeapEventType::Allocated)
            | (Some("realloc"), HeapEventType::Reallocated)
            | (Some("free"), HeapEventType::Freed)
    );
    operation_matches
        && expr.kind == "CallExpr"
        && event
            .source_file
            .as_deref()
            .zip(event.source_line)
            .is_none_or(|(file, line)| {
                same_source_file(file, &expr.span.file)
                    && line >= expr.span.start_line
                    && line
                        <= expr
                            .span
                            .end_line
                            .min(expr.span.start_line.saturating_add(64))
            })
}

fn valid_member_ref(
    member: &crate::clang_ast::DwarfMemberRef,
    types: &HashMap<&str, &crate::dwarf::TypeEntry>,
) -> bool {
    let Some(parent) = types.get(member.type_ref.as_str()) else {
        return false;
    };
    matches!(parent.kind.as_str(), "struct" | "union")
        && parent
            .members
            .get(member.member_index)
            .is_some_and(|entry| {
                entry.name.as_deref() == Some(member.name.as_str())
                    && entry.byte_offset == member.byte_offset
            })
}

/// Runtime observations may only cite a source declaration that claims the
/// same exact serialized DWARF declaration. Activation identity is untouched.
fn validate_source_links(trace: &DebugSessionTrace, builder: &mut Builder) {
    let declarations: HashMap<&str, &crate::clang_ast::SourceDeclaration> = trace
        .source_metadata
        .as_ref()
        .map(|registry| {
            registry
                .declarations
                .iter()
                .map(|declaration| (declaration.id.as_str(), declaration))
                .collect()
        })
        .unwrap_or_default();
    for (position, snapshot) in trace.snapshots.iter().enumerate() {
        for (variable, info) in snapshot.variables.iter().enumerate() {
            validate_variable_source(
                info,
                &declarations,
                ValidationLocation::SnapshotVariable {
                    snapshot: position,
                    variable,
                },
                builder,
            );
        }
        for (frame, caller) in snapshot.caller_frames.iter().enumerate() {
            for (variable, info) in caller.variables.iter().enumerate() {
                validate_variable_source(
                    info,
                    &declarations,
                    ValidationLocation::SnapshotCallerVariable {
                        snapshot: position,
                        frame,
                        variable,
                    },
                    builder,
                );
            }
        }
        for (variable, info) in snapshot.observation.globals.iter().enumerate() {
            validate_variable_source(
                info,
                &declarations,
                ValidationLocation::SnapshotGlobal {
                    snapshot: position,
                    variable,
                },
                builder,
            );
        }
    }
}

fn validate_variable_source(
    info: &VariableInfo,
    declarations: &HashMap<&str, &crate::clang_ast::SourceDeclaration>,
    location: ValidationLocation,
    builder: &mut Builder,
) {
    let Some(id) = info.source_declaration_ref.as_deref() else {
        return;
    };
    let valid = declarations.get(id).is_some_and(|declaration| {
        declaration.dwarf_declaration.is_some()
            && declaration.dwarf_declaration.as_deref() == info.declaration_ref.as_deref()
    });
    if !valid {
        builder.push(
            ValidationSeverity::StructuralCorruption,
            "VARIABLE_SOURCE_DECLARATION_INVALID",
            format!(
                "variable `{}` source declaration does not resolve to its exact DWARF declaration",
                info.name
            ),
            location,
        );
    }
}

#[derive(Default)]
struct Builder {
    findings: Vec<ValidationFinding>,
    seen: std::collections::HashMap<(&'static str, ValidationSeverity), usize>,
}

impl Builder {
    fn push(
        &mut self,
        severity: ValidationSeverity,
        code: &'static str,
        detail: impl Into<String>,
        location: ValidationLocation,
    ) {
        match self.seen.get(&(code, severity)) {
            Some(&index) => self.findings[index].occurrences += 1,
            None => {
                self.seen.insert((code, severity), self.findings.len());
                self.findings.push(ValidationFinding {
                    severity,
                    code,
                    detail: detail.into(),
                    location,
                    occurrences: 1,
                });
            }
        }
    }

    fn finish(self) -> ValidationReport {
        // Corruption first; the remaining severities in declaration order,
        // code order within each group.
        let order = |severity: ValidationSeverity| match severity {
            ValidationSeverity::StructuralCorruption => 0,
            ValidationSeverity::PartialCoverage => 1,
            ValidationSeverity::UnavailableEvidence => 2,
            ValidationSeverity::OptionalAbsence => 3,
        };
        let mut findings = self.findings;
        findings.sort_by_key(|finding| (order(finding.severity), finding.code));
        ValidationReport { findings }
    }
}

// -- instruction events -----------------------------------------------------

fn validate_instruction_events(trace: &DebugSessionTrace, builder: &mut Builder) {
    let events: &[InstructionEvent] = &trace.recording.instruction_events;
    let disassembly_len = trace.recording.disassembly_table.len();
    let snapshot_len = trace.snapshots.len();

    // Ordering guarantees the zero-memory lookups rely on. Events append per
    // processed stop and carry the stop counter (`stop_id_before`) and the
    // most recent snapshot index (`checkpoint_id`) — both monotone by
    // construction (recorder.rs, trace.rs call sites).
    for (position, window) in events.windows(2).enumerate() {
        if window[0].stop_id_before > window[1].stop_id_before {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "EVENT_STOP_ORDER",
                format!(
                    "stop_id_before {} is followed by {} (must be non-decreasing)",
                    window[0].stop_id_before, window[1].stop_id_before
                ),
                ValidationLocation::InstructionEvent(position + 1),
            );
            break;
        }
    }
    let checkpoint_key = |event: &InstructionEvent| event.checkpoint_id;
    for (position, window) in events.windows(2).enumerate() {
        if checkpoint_key(&window[0]) > checkpoint_key(&window[1]) {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "EVENT_CHECKPOINT_ORDER",
                format!(
                    "checkpoint_id {:?} is followed by {:?} (must be non-decreasing)",
                    window[0].checkpoint_id, window[1].checkpoint_id
                ),
                ValidationLocation::InstructionEvent(position + 1),
            );
            break;
        }
    }

    for (position, event) in events.iter().enumerate() {
        if event.stop_id_before == 0 || event.stop_id_after == Some(0) {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "EVENT_STOP_ZERO",
                "GDB stop counters are one-based".to_string(),
                ValidationLocation::InstructionEvent(position),
            );
        }
        // stop_id_after is recorded as "the next processed stop" — when
        // present it must be exactly before+1; absent means the step end was
        // never observed (e.g. debugger deadline mid-step), which the schema
        // allows.
        match event.stop_id_after {
            Some(after) if event.stop_id_before.checked_add(1) != Some(after) => {
                builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "STOP_AFTER_NOT_NEXT",
                    format!(
                        "stop_id_after {after} with stop_id_before {} (next stop must be its successor)",
                        event.stop_id_before,
                    ),
                    ValidationLocation::InstructionEvent(position),
                );
            }
            None => builder.push(
                ValidationSeverity::OptionalAbsence,
                "STOP_AFTER_UNRECORDED",
                "step end was not observed (e.g. debugger deadline hit mid-step)".to_string(),
                ValidationLocation::InstructionEvent(position),
            ),
            Some(_) => {}
        }
        // Static disassembly relationship.
        match event.instruction_id {
            Some(id) if id >= disassembly_len as u64 => {
                builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "INSTRUCTION_REFERENCE_INVALID",
                    format!(
                        "instruction_id {id} does not resolve in disassembly_table (len {disassembly_len})"
                    ),
                    ValidationLocation::InstructionEvent(position),
                );
            }
            None => builder.push(
                ValidationSeverity::UnavailableEvidence,
                "INSTRUCTION_REFERENCE_UNRECORDED",
                "no disassembly entry was recorded for this execution".to_string(),
                ValidationLocation::InstructionEvent(position),
            ),
            Some(_) => {}
        }
        // The source context is the only recorded source↔assembly evidence.
        if event.source.file.is_none() || event.source.line.is_none() {
            builder.push(
                ValidationSeverity::UnavailableEvidence,
                "SOURCE_CONTEXT_UNRECORDED",
                "source file or line was not recorded for this execution".to_string(),
                ValidationLocation::InstructionEvent(position),
            );
        }
        // checkpoint_id, when present, must resolve.
        if let Some(checkpoint) = event.checkpoint_id {
            if checkpoint >= snapshot_len {
                builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "CHECKPOINT_REFERENCE_INVALID",
                    format!(
                        "checkpoint_id {checkpoint} does not resolve in snapshots (len {snapshot_len})"
                    ),
                    ValidationLocation::InstructionEvent(position),
                );
            }
        }
        // Recorded memory observations keep their unavailable states.
        for observation in &event.memory {
            if observation.raw_bytes_hex.is_none() {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "MEMORY_OBSERVATION_UNAVAILABLE",
                    format!(
                        "memory observation `{}` was not captured ({})",
                        observation.expression,
                        observation.error.as_deref().unwrap_or("no bytes recorded")
                    ),
                    ValidationLocation::InstructionEvent(position),
                );
            }
        }
        if event.next_address.is_none() {
            builder.push(
                ValidationSeverity::OptionalAbsence,
                "NEXT_ADDRESS_UNRECORDED",
                "the stop following this attempt recorded no frame address".to_string(),
                ValidationLocation::InstructionEvent(position),
            );
        }
    }
}

// -- snapshots ---------------------------------------------------------------

fn validate_snapshots(trace: &DebugSessionTrace, builder: &mut Builder) {
    let heap_event_len = trace.heap_events.len();
    let attempted = trace.recording.instruction_count;

    for (position, snapshot) in trace.snapshots.iter().enumerate() {
        if snapshot.observation.stop_id == 0 {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "SNAPSHOT_STOP_ZERO",
                "GDB stop counters are one-based".to_string(),
                ValidationLocation::Snapshot(position),
            );
        }
        // step_index is assigned as the push position (verified at the
        // recorder call sites); snapshots are never removed.
        if snapshot.step_index != position {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "STEP_INDEX_NOT_POSITIONAL",
                format!(
                    "step_index {} at snapshot position {position}",
                    snapshot.step_index
                ),
                ValidationLocation::Snapshot(position),
            );
        }
        // Attempted-event positions compare against the attempted count,
        // never against the retained event vector length.
        if snapshot.observation.instruction_position > attempted {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "SNAPSHOT_POSITION_BEYOND_TIMELINE",
                format!(
                    "instruction_position {} exceeds the attempted count {attempted}",
                    snapshot.observation.instruction_position
                ),
                ValidationLocation::Snapshot(position),
            );
        }
        // The cumulative heap-event counter must stay within the recorded
        // events and never decrease.
        if snapshot.observation.heap_event_count > heap_event_len {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "HEAP_COUNT_OVER_RECORDED",
                format!(
                    "heap_event_count {} exceeds the {} recorded heap events",
                    snapshot.observation.heap_event_count, heap_event_len
                ),
                ValidationLocation::Snapshot(position),
            );
        }
        for (variable, info) in snapshot.variables.iter().enumerate() {
            // NotCaptured bytes are the default (nothing was requested) and
            // are not findings; real unavailability is.
            if info.read_status != DataValidity::Valid
                || info.read_error.is_some()
                || info.bytes_status == DataValidity::Truncated
            {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "VARIABLE_OBSERVATION_UNAVAILABLE",
                    format!(
                        "variable `{}`: read={:?} bytes={:?}{}",
                        info.name,
                        info.read_status,
                        info.bytes_status,
                        info.read_error
                            .as_deref()
                            .map(|error| format!(" ({error})"))
                            .unwrap_or_default()
                    ),
                    ValidationLocation::SnapshotVariable {
                        snapshot: position,
                        variable,
                    },
                );
            }
        }
        validate_caller_frames(snapshot, position, builder);
        for (block, info) in snapshot.heap_blocks.iter().enumerate() {
            if info.raw_bytes_hex.is_none() {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "HEAP_BYTES_UNAVAILABLE",
                    format!(
                        "block {}: bytes not captured{}",
                        info.allocation_id,
                        info.memory_read_error
                            .as_deref()
                            .map(|error| format!(" ({error})"))
                            .unwrap_or_else(|| " (limit, freed, or not read)".to_string())
                    ),
                    ValidationLocation::SnapshotHeapBlock {
                        snapshot: position,
                        block,
                    },
                );
            }
        }
    }

    // Stop counter strictly increasing over snapshots (holes are normal).
    for window in trace.snapshots.windows(2) {
        if window[0].observation.stop_id >= window[1].observation.stop_id {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "SNAPSHOT_STOP_ORDER",
                format!(
                    "stop_id {} is followed by {} (must strictly increase)",
                    window[0].observation.stop_id, window[1].observation.stop_id
                ),
                ValidationLocation::Snapshot(0),
            );
            break;
        }
    }
    // Cumulative heap counter monotonicity.
    for window in trace.snapshots.windows(2) {
        if window[0].observation.heap_event_count > window[1].observation.heap_event_count {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "HEAP_COUNT_NOT_MONOTONIC",
                format!(
                    "heap_event_count {} is followed by {} (cumulative counter)",
                    window[0].observation.heap_event_count, window[1].observation.heap_event_count
                ),
                ValidationLocation::Snapshot(0),
            );
            break;
        }
    }
}

fn validate_caller_frames(snapshot: &ExecutionSnapshot, position: usize, builder: &mut Builder) {
    // No caller capture is normal in older traces or under capture limits.
    // Still validate frame-0 locals and globals: their pointer evidence does
    // not depend on whether any caller was retained.
    let observed = snapshot
        .variables
        .iter()
        .chain(
            snapshot
                .caller_frames
                .iter()
                .flat_map(|frame| &frame.variables),
        )
        .chain(&snapshot.observation.globals);
    if snapshot.caller_frames.is_empty() && observed.clone().all(|info| info.target.is_none()) {
        return;
    }
    // Most snapshots do not carry a variable-identity pointer. Do not build
    // a per-snapshot identity table just to check address-only heap evidence.
    let needs_identities = observed.clone().any(|info| {
        info.target
            .as_ref()
            .and_then(|target| target.variable_identity.as_ref())
            .is_some()
    });
    let mut identities: HashMap<&str, Vec<&VariableInfo>> = HashMap::new();
    if needs_identities {
        for info in observed {
            if let Some(identity) = info.identity.as_deref() {
                identities.entry(identity).or_default().push(info);
            }
        }
    }
    let mut levels = HashSet::new();
    let mut activations: HashSet<_> = snapshot
        .variables
        .iter()
        .filter_map(|var| var.activation_id)
        .collect();
    for (frame, caller) in snapshot.caller_frames.iter().enumerate() {
        let location = ValidationLocation::SnapshotCallerFrame {
            snapshot: position,
            frame,
        };
        let associated: Vec<_> = snapshot
            .call_stack
            .iter()
            .filter(|recorded| recorded.level == caller.frame.level)
            .collect();
        if caller.frame.level == 0
            || !levels.insert(caller.frame.level)
            || associated.len() != 1
            || !crate::memory::same_frame(associated[0], &caller.frame)
        {
            builder.push(ValidationSeverity::StructuralCorruption, "CALLER_FRAME_ASSOCIATION_INVALID",
                "caller must uniquely match a nonzero call_stack frame level and its recorded metadata", location.clone());
        }
        if caller.activation_id == 0 || !activations.insert(caller.activation_id) {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "CALLER_ACTIVATION_INVALID",
                format!(
                    "activation {} must be nonzero and distinct from other current frames",
                    caller.activation_id
                ),
                location.clone(),
            );
        }
        if !caller.errors.is_empty() {
            builder.push(
                ValidationSeverity::UnavailableEvidence,
                "CALLER_CAPTURE_INCOMPLETE",
                caller.errors.join("; "),
                location,
            );
        }
        for (variable, info) in caller.variables.iter().enumerate() {
            let location = ValidationLocation::SnapshotCallerVariable {
                snapshot: position,
                frame,
                variable,
            };
            match info.activation_id {
                Some(id) if id != caller.activation_id => builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "CALLER_VARIABLE_ACTIVATION_MISMATCH",
                    format!(
                        "variable activation {id} differs from frame activation {}",
                        caller.activation_id
                    ),
                    location.clone(),
                ),
                None => builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "CALLER_VARIABLE_ACTIVATION_UNRECORDED",
                    "caller variable has no recorded activation association",
                    location.clone(),
                ),
                _ => {}
            }
            if info.read_status != DataValidity::Valid
                || info.read_error.is_some()
                || info.bytes_status == DataValidity::Truncated
            {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "CALLER_VARIABLE_OBSERVATION_UNAVAILABLE",
                    format!(
                        "variable `{}`: read={:?} bytes={:?}",
                        info.name, info.read_status, info.bytes_status
                    ),
                    location.clone(),
                );
            }
            if let Some(identity_activation) = info
                .identity
                .as_deref()
                .and_then(|id| id.strip_prefix("activation:"))
                .and_then(|suffix| suffix.split(':').next())
                .and_then(|id| id.parse::<u64>().ok())
            {
                if identity_activation != caller.activation_id {
                    builder.push(
                        ValidationSeverity::StructuralCorruption,
                        "CALLER_IDENTITY_ACTIVATION_MISMATCH",
                        "activation-scoped identity belongs to a different frame activation",
                        location.clone(),
                    );
                }
            }
            validate_pointer_reference(info, &identities, location.clone(), builder);
            if let Some(memory) = &info.memory_evidence {
                if memory.captured_bytes > memory.requested_bytes
                    || info.raw_bytes_hex.as_ref().is_some_and(|bytes| {
                        bytes.len() % 2 != 0
                            || bytes.len() as u64 / 2 != memory.captured_bytes
                            || !bytes.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
                    || (memory.captured_bytes != 0 && info.raw_bytes_hex.is_none())
                {
                    builder.push(ValidationSeverity::StructuralCorruption, "CALLER_MEMORY_EVIDENCE_INVALID",
                        "captured byte count must fit the request and agree with the retained hex bytes", location);
                }
            }
        }
    }
    for (variable, info) in snapshot.variables.iter().enumerate() {
        validate_pointer_reference(
            info,
            &identities,
            ValidationLocation::SnapshotVariable {
                snapshot: position,
                variable,
            },
            builder,
        );
    }
    for (variable, info) in snapshot.observation.globals.iter().enumerate() {
        validate_pointer_reference(
            info,
            &identities,
            ValidationLocation::SnapshotGlobal {
                snapshot: position,
                variable,
            },
            builder,
        );
    }
}

fn validate_pointer_reference(
    info: &VariableInfo,
    identities: &HashMap<&str, Vec<&VariableInfo>>,
    location: ValidationLocation,
    builder: &mut Builder,
) {
    let Some(target) = &info.target else { return };
    // Every recorded numeric-address match is a candidate, never evidence of
    // pointer origin. A missing status is legitimate legacy metadata; do not
    // manufacture a proven identity from it. Unknown future statuses cannot
    // support an imported definite identity either.
    if target.allocation_id.is_some() && target.provenance_status.is_none() && target.kind == "heap"
    {
        builder.push(
            ValidationSeverity::UnavailableEvidence,
            "POINTER_HEAP_PROVENANCE_UNRECORDED",
            "legacy definite heap ID has no recorded origin-confidence status; do not infer provenance",
            location.clone(),
        );
    }
    if target.allocation_id.is_some()
        && (target.provenance_status.is_some() || target.kind != "heap")
    {
        builder.push(ValidationSeverity::StructuralCorruption, "POINTER_HEAP_PROVENANCE_CONTRADICTORY",
            "numeric-address, uncertain, freed, unknown-status or non-heap evidence cannot claim a definite heap identity", location.clone());
    }
    if let (Some(allocation), Some(candidate)) =
        (target.allocation_id, target.address_match_allocation_id)
    {
        if allocation != candidate {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "POINTER_HEAP_ADDRESS_MISMATCH",
                "allocation identity and numeric-address candidate disagree",
                location.clone(),
            );
        }
    }
    let Some(identity) = target.variable_identity.as_deref() else {
        return;
    };
    if identity.is_empty() || identity.starts_with("unresolved:") || target.allocation_id.is_some()
    {
        builder.push(ValidationSeverity::StructuralCorruption, "POINTER_VARIABLE_REFERENCE_INVALID",
            "a resolved variable target needs a definite identity and cannot also identify a heap allocation", location.clone());
    }
    let Some(candidates) = identities.get(identity) else {
        // Capture caps can omit a referenced variable; absence is not proof
        // that an activation died between retained checkpoints.
        builder.push(
            ValidationSeverity::UnavailableEvidence,
            "POINTER_VARIABLE_REFERENCE_UNRESOLVED",
            format!("target identity `{identity}` is not observed at this checkpoint"),
            location,
        );
        return;
    };
    if candidates
        .iter()
        .all(|candidate| candidate.scope_status == Some(ScopeStatus::Inactive))
    {
        builder.push(
            ValidationSeverity::StructuralCorruption,
            "POINTER_VARIABLE_REFERENCE_INACTIVE",
            "resolved pointer identity refers only to inactive objects",
            location.clone(),
        );
    }
    let Some(pointer) = info.points_to.as_deref().and_then(crate::memory::address) else {
        return;
    };
    if let (Some(base), Some(offset)) = (
        target
            .base_address
            .as_deref()
            .and_then(crate::memory::address),
        target.offset_bytes,
    ) {
        if base.checked_add(offset) != Some(pointer) {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "POINTER_VARIABLE_OFFSET_MISMATCH",
                "target base plus offset does not equal the recorded pointer address",
                location.clone(),
            );
        }
    }
    if candidates.iter().all(|candidate| {
        match (
            candidate
                .address
                .as_deref()
                .and_then(crate::memory::address),
            candidate.size_bytes,
        ) {
            (Some(base), Some(size)) => !pointer
                .checked_sub(base)
                .is_some_and(|offset| offset < size),
            // An unobserved extent cannot contradict the reference.
            _ => false,
        }
    }) {
        builder.push(
            ValidationSeverity::StructuralCorruption,
            "POINTER_VARIABLE_EXTENT_MISMATCH",
            "pointer address lies outside every recorded extent of its referenced identity",
            location,
        );
    }
}

// -- source checkpoints -------------------------------------------------------

fn validate_source_checkpoints(trace: &DebugSessionTrace, builder: &mut Builder) {
    let snapshot_len = trace.snapshots.len();
    for (position, &index) in trace.recording.source_checkpoints.iter().enumerate() {
        if index >= snapshot_len {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "SOURCE_CHECKPOINT_INVALID",
                format!(
                    "source checkpoint {index} does not resolve in snapshots (len {snapshot_len})"
                ),
                ValidationLocation::SourceCheckpoint(position),
            );
        }
        if position > 0 && trace.recording.source_checkpoints[position - 1] >= index {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "SOURCE_CHECKPOINT_ORDER",
                format!(
                    "source checkpoint {index} does not follow {} (captured in stop order)",
                    trace.recording.source_checkpoints[position - 1]
                ),
                ValidationLocation::SourceCheckpoint(position),
            );
        }
    }
    // A snapshot can legitimately miss its journal checkpoint entry when the
    // trace-memory cap interrupted `recorder.checkpoint` after the push —
    // recorded partiality, not corruption (the journal then also lacks it).
    if trace.recording.source_checkpoints.len() < snapshot_len {
        builder.push(
            ValidationSeverity::PartialCoverage,
            "CHECKPOINT_JOURNAL_INCOMPLETE",
            format!(
                "{} of {} snapshots have journal checkpoint entries (the trace-memory cap can stop journaling after a snapshot is retained)",
                trace.recording.source_checkpoints.len(),
                snapshot_len
            ),
            ValidationLocation::Trace,
        );
    }
}

// -- heap ----------------------------------------------------------------------

/// Confidence of an *explicitly recorded* predecessor, not of a numeric
/// address match. The caller supplies only events indexed for this allocation
/// ID and its predecessor; neither validation nor a semantic lookup scans
/// the entire event log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReallocLink {
    Verified,
    Unavailable,
    Ambiguous,
    Contradictory,
}

// glibc realloc(ptr, 0) may free the *existing* lifetime and return NULL.
// The engine records a Reallocated event with the SAME ID, no address and
// zero size. This is a terminal event, not a new lifetime/predecessor edge.
fn terminal_zero_realloc(event: &crate::models::HeapEvent) -> bool {
    event.event_type == HeapEventType::Reallocated
        && event.old_allocation_id == Some(event.allocation_id)
        && event.address.is_none()
        && event.size_bytes == Some(0)
}

pub(crate) fn realloc_link<'a>(
    block: &crate::models::HeapBlockInfo,
    current: Option<&crate::models::AllocationLifetime>,
    previous: Option<&crate::models::AllocationLifetime>,
    events: impl Iterator<Item = &'a crate::models::HeapEvent>,
    previous_events: impl Iterator<Item = &'a crate::models::HeapEvent>,
) -> ReallocLink {
    let Some(old_id) = block.realloc_from_id else {
        return ReallocLink::Unavailable;
    };
    if old_id == block.allocation_id {
        return ReallocLink::Contradictory;
    }
    if old_id == 0 {
        return ReallocLink::Unavailable; // untracked, not a lifetime identity
    }
    let mut matches = 0usize;
    let mut missing = false;
    for event in events {
        if terminal_zero_realloc(event) {
            continue;
        }
        if event.event_type == HeapEventType::Allocated {
            return ReallocLink::Contradictory;
        }
        if event.event_type != HeapEventType::Reallocated {
            continue; // free or failed realloc cannot establish a new lifetime
        }
        match event.old_allocation_id {
            Some(id) if id != old_id => return ReallocLink::Contradictory,
            None => {
                missing = true;
                continue;
            }
            Some(_) => {}
        }
        if !block.allocation_type.is_empty() && block.allocation_type != "realloc" {
            return ReallocLink::Contradictory;
        }
        if current.is_some_and(|l| {
            l.pid != event.pid
                || (l.allocation_sequence != 0 && l.allocation_sequence != event.sequence)
                || l.free_sequence.is_some_and(|free| free < event.sequence)
        }) || previous.is_some_and(|l| {
            l.pid != event.pid
                || (l.allocation_sequence != 0 && l.allocation_sequence >= event.sequence)
                || l.free_sequence.is_some_and(|free| free != event.sequence)
        }) {
            return ReallocLink::Contradictory;
        }
        matches += 1;
    }
    let mut prior_creations = 0;
    for prior in previous_events.filter(|e| {
        matches!(
            e.event_type,
            HeapEventType::Allocated | HeapEventType::Reallocated
        ) && !terminal_zero_realloc(e)
    }) {
        if previous.is_some_and(|l| {
            l.pid != prior.pid
                || (l.allocation_sequence != 0 && l.allocation_sequence != prior.sequence)
        }) {
            return ReallocLink::Contradictory;
        }
        prior_creations += 1;
    }
    if matches > 1 || prior_creations > 1 {
        ReallocLink::Ambiguous
    } else if matches == 0
        || prior_creations == 0
        || missing
        || current.is_none()
        || previous.is_none()
        || current.is_some_and(|l| l.allocation_sequence == 0)
        || previous.is_some_and(|l| l.allocation_sequence == 0 || l.free_sequence.is_none())
    {
        ReallocLink::Unavailable
    } else {
        ReallocLink::Verified
    }
}

fn validate_heap(trace: &DebugSessionTrace, builder: &mut Builder) {
    // Identity uniqueness: one heap_state entry and one lifetime per
    // allocation id (guaranteed by the timeline reconstruction).
    let mut state_ids: HashSet<u64> = HashSet::new();
    for (position, block) in trace.heap_state.iter().enumerate() {
        if !state_ids.insert(block.allocation_id) {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "HEAP_STATE_ID_DUPLICATE",
                format!(
                    "allocation id {} appears more than once",
                    block.allocation_id
                ),
                ValidationLocation::HeapState(position),
            );
        }
    }
    let mut lifetime_ids: HashSet<u64> = HashSet::new();
    for (position, lifetime) in trace.recording.allocation_lifetimes.iter().enumerate() {
        if !lifetime_ids.insert(lifetime.allocation_id) {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "LIFETIME_ID_DUPLICATE",
                format!(
                    "allocation id {} appears more than once",
                    lifetime.allocation_id
                ),
                ValidationLocation::AllocationLifetime(position),
            );
        }
    }

    let known: HashSet<u64> = state_ids
        .iter()
        .chain(lifetime_ids.iter())
        .copied()
        .collect();
    let linked: HashSet<u64> = trace
        .heap_state
        .iter()
        .filter(|block| block.realloc_from_id.is_some())
        .map(|block| block.allocation_id)
        .collect();
    let related: HashSet<u64> = linked
        .iter()
        .copied()
        .chain(trace.heap_state.iter().filter_map(|b| b.realloc_from_id))
        .collect();
    let lifetimes: HashMap<u64, &crate::models::AllocationLifetime> = trace
        .recording
        .allocation_lifetimes
        .iter()
        .map(|item| (item.allocation_id, item))
        .collect();
    let mut creations: HashMap<u64, Vec<&crate::models::HeapEvent>> = HashMap::new();
    for event in &trace.heap_events {
        if related.contains(&event.allocation_id)
            && matches!(
                event.event_type,
                HeapEventType::Allocated | HeapEventType::Reallocated
            )
        {
            creations
                .entry(event.allocation_id)
                .or_default()
                .push(event);
        }
    }
    // Reallocation references must resolve somewhere in the recorded
    // identity space; when they do not, the relationship is unavailable
    // (caps or partial recording) — never silently reinterpreted.
    let unresolved = |id: u64| id != 0 && !known.contains(&id);
    let mut last_sequence_by_pid = HashMap::new();
    for (position, event) in trace.heap_events.iter().enumerate() {
        if last_sequence_by_pid
            .insert(event.pid, event.sequence)
            .is_some_and(|previous| event.sequence <= previous)
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "HEAP_EVENT_SEQUENCE_ORDER",
                "retained heap events for one PID are not in strictly increasing sequence order",
                ValidationLocation::HeapEvent(position),
            );
        }
        if let Some(attribution) = &event.attribution {
            if let Some(interval) = &attribution.checkpoint_interval {
                if interval
                    .before_checkpoint
                    .is_some_and(|i| i >= trace.snapshots.len())
                    || interval
                        .after_checkpoint
                        .is_some_and(|i| i >= trace.snapshots.len())
                    || interval
                        .before_checkpoint
                        .zip(interval.after_checkpoint)
                        .is_some_and(|(a, b)| a >= b)
                {
                    builder.push(
                        ValidationSeverity::StructuralCorruption,
                        "HEAP_CHECKPOINT_INTERVAL_INVALID",
                        "event checkpoint interval references missing or reversed snapshots",
                        ValidationLocation::HeapEvent(position),
                    );
                } else {
                    let before = interval
                        .before_checkpoint
                        .and_then(|i| trace.snapshots.get(i));
                    let after = interval
                        .after_checkpoint
                        .and_then(|i| trace.snapshots.get(i));
                    if before.is_some_and(|s| {
                        interval.before_stop != Some(s.observation.stop_id)
                            || s.observation.heap_event_count > position
                    }) || after.is_some_and(|s| {
                        interval.after_stop != Some(s.observation.stop_id)
                            || s.observation.heap_event_count <= position
                    }) {
                        builder.push(ValidationSeverity::StructuralCorruption, "HEAP_CHECKPOINT_BOUNDARY_INVALID",
                            "event count/stop does not agree with its bracketed checkpoint interval", ValidationLocation::HeapEvent(position));
                    }
                }
            }
        }
        if let Some(origin) = event.old_allocation_id {
            if event.event_type == HeapEventType::Reallocated
                && origin != 0
                && ((!terminal_zero_realloc(event) && origin == event.allocation_id)
                    || lifetimes.get(&origin).is_some_and(|old| {
                        old.pid != event.pid
                            || (old.allocation_sequence != 0
                                && old.allocation_sequence >= event.sequence)
                            || old.free_sequence.is_some_and(|free| free != event.sequence)
                    }))
            {
                builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "REALLOC_EVENT_ORIGIN_CONTRADICTORY",
                    "realloc event predecessor contradicts its PID or recorded lifetime direction/boundary",
                    ValidationLocation::HeapEvent(position),
                );
            }
            if unresolved(origin) {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "REALLOC_ORIGIN_UNRESOLVED",
                    format!(
                        "old_allocation_id {origin} does not resolve in heap_state or allocation_lifetimes"
                    ),
                    ValidationLocation::HeapEvent(position),
                );
            }
        }
        if event.event_type != HeapEventType::AllocationFailed && event.address.is_none() {
            builder.push(
                ValidationSeverity::OptionalAbsence,
                "HEAP_EVENT_ADDRESS_UNRECORDED",
                format!(
                    "{:?} event for allocation {} recorded no address",
                    event.event_type, event.allocation_id
                ),
                ValidationLocation::HeapEvent(position),
            );
        }
    }
    if let Some(history) = &trace.recording.heap_history {
        validate_heap_process_coverage(trace, history, builder);
        for gap in &history.sequence_gaps {
            if gap.expected_sequence >= gap.observed_sequence {
                builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "HEAP_SEQUENCE_GAP_INVALID",
                    "sequence gap must name a strictly later observed event",
                    ValidationLocation::Trace,
                );
            } else {
                builder.push(
                    ValidationSeverity::PartialCoverage,
                    "HEAP_SEQUENCE_GAP",
                    format!(
                        "pid {} missing allocation events {}..{}",
                        gap.pid,
                        gap.expected_sequence,
                        gap.observed_sequence - 1
                    ),
                    ValidationLocation::Trace,
                );
            }
        }
        if !history.complete && history.sequence_gaps.is_empty() {
            builder.push(
                ValidationSeverity::PartialCoverage,
                "HEAP_HISTORY_INCOMPLETE",
                "allocation telemetry has transport, retention, or terminal-coverage gaps",
                ValidationLocation::Trace,
            );
        }
    }
    for (position, block) in trace.heap_state.iter().enumerate() {
        if let Some(origin) = block.realloc_from_id {
            if unresolved(origin) {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "REALLOC_ORIGIN_UNRESOLVED",
                    format!(
                        "realloc_from_id {origin} does not resolve in heap_state or allocation_lifetimes"
                    ),
                    ValidationLocation::HeapState(position),
                );
            }
            match realloc_link(block, lifetimes.get(&block.allocation_id).copied(),
                lifetimes.get(&origin).copied(),
                creations.get(&block.allocation_id).into_iter().flatten().copied(),
                creations.get(&origin).into_iter().flatten().copied()) {
                ReallocLink::Contradictory => builder.push(
                    ValidationSeverity::StructuralCorruption,
                    "REALLOC_LINK_CONTRADICTORY",
                    "recorded realloc predecessor contradicts allocation type, event or PID/sequence/lifetime evidence",
                    ValidationLocation::HeapState(position),
                ),
                ReallocLink::Ambiguous | ReallocLink::Unavailable => builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "REALLOC_LINK_UNVERIFIED",
                    "recorded predecessor lacks unique corroborating event and lifetime evidence",
                    ValidationLocation::HeapState(position),
                ),
                ReallocLink::Verified => {},
            }
        }
    }
    // A cycle is impossible even when events/lifetimes are absent (legacy or
    // partial imports). Visit each recorded block at most once.
    let blocks: HashMap<u64, (usize, &crate::models::HeapBlockInfo)> = trace
        .heap_state
        .iter()
        .enumerate()
        .filter(|(_, b)| b.realloc_from_id.is_some())
        .map(|(i, b)| (b.allocation_id, (i, b)))
        .collect();
    let mut visited = HashSet::new();
    for &id in blocks.keys() {
        if visited.contains(&id) {
            continue;
        }
        let mut path = Vec::new();
        let mut in_path = HashSet::new();
        let mut next = Some(id);
        while let Some(current) = next {
            if visited.contains(&current) || !in_path.insert(current) {
                if in_path.contains(&current) && !visited.contains(&current) {
                    builder.push(
                        ValidationSeverity::StructuralCorruption,
                        "REALLOC_LINK_CYCLE",
                        "reallocation predecessor chain contains a cycle",
                        ValidationLocation::HeapState(blocks[&current].0),
                    );
                }
                break;
            }
            path.push(current);
            next = blocks
                .get(&current)
                .and_then(|(_, b)| b.realloc_from_id)
                .filter(|old| blocks.contains_key(old));
        }
        visited.extend(path);
    }
}

/// Only the observed event PIDs plus a known followed inferior can be
/// verified. No event/ACK transport provides a census of silent fork children.
fn validate_heap_process_coverage(
    trace: &DebugSessionTrace,
    history: &crate::models::HeapHistoryCoverage,
    builder: &mut Builder,
) {
    use super::views::HeapProcessScope;
    match HeapProcessScope::from_history(Some(history)) {
        HeapProcessScope::Unsupported(value) => builder.push(
            ValidationSeverity::StructuralCorruption,
            "HEAP_PROCESS_SCOPE_UNSUPPORTED",
            format!("unsupported heap process scope `{value}`"),
            ValidationLocation::Trace,
        ),
        HeapProcessScope::Unknown => {
            // Legacy missing metadata cannot certify descendant coverage.
            if history.unobserved_process_coverage.is_some() {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "HEAP_PROCESS_SCOPE_UNRECORDED",
                    "unobserved-process annotation has no recorded process scope",
                    ValidationLocation::Trace,
                );
            }
        }
        HeapProcessScope::VerifiedObservedPidsOnly { .. } => {
            if history.unobserved_process_coverage.is_none() {
                builder.push(
                    ValidationSeverity::UnavailableEvidence,
                    "HEAP_UNOBSERVED_PROCESS_COVERAGE_UNRECORDED",
                    "observed-PID coverage cannot establish coverage of unobserved processes",
                    ValidationLocation::Trace,
                );
            }
        }
    }
    if history
        .unobserved_process_coverage
        .as_deref()
        .is_some_and(|value| value != "unknown")
    {
        builder.push(
            ValidationSeverity::StructuralCorruption,
            "HEAP_PROCESS_COVERAGE_UNSUPPORTED",
            "unobserved child processes cannot be certified from event/ACK records",
            ValidationLocation::Trace,
        );
    }
    if history.process_scope.is_some() {
        let mut seen = HashSet::new();
        if history
            .observed_event_pids
            .iter()
            .any(|pid| *pid == 0 || !seen.insert(*pid))
            || history.followed_inferior_pid == Some(0)
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "HEAP_OBSERVED_PID_INVALID",
                "observed event PIDs must be unique and nonzero; followed inferior PID must be nonzero",
                ValidationLocation::Trace,
            );
        }
        if trace
            .heap_events
            .iter()
            .any(|event| !seen.contains(&event.pid))
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "HEAP_EVENT_PID_UNOBSERVED",
                "a retained heap event PID is absent from the observed-event PID list",
                ValidationLocation::Trace,
            );
        }
        if history.complete
            && (!history.sequence_gaps.is_empty()
                || history.missing_events != 0
                || history.malformed_lines != 0
                || !history.transport_errors.is_empty()
                || history.events_truncated
                || history.blocks_truncated
                || history.id_widening_overflow)
        {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "HEAP_OBSERVED_COVERAGE_CONTRADICTORY",
                "observed-PID history marked complete despite known transport or retention losses",
                ValidationLocation::Trace,
            );
        }
    }
}

// -- gaps ----------------------------------------------------------------------

fn validate_gaps(trace: &DebugSessionTrace, builder: &mut Builder) {
    let attempted = trace.recording.instruction_count;
    for (position, gap) in trace.recording.trace_gaps.iter().enumerate() {
        if gap.instruction_position > attempted {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "GAP_POSITION_BEYOND_TIMELINE",
                format!(
                    "gap position {} exceeds the attempted count {attempted}",
                    gap.instruction_position
                ),
                ValidationLocation::TraceGap(position),
            );
        }
    }
    for window in trace.recording.trace_gaps.windows(2) {
        if window[0].instruction_position > window[1].instruction_position {
            builder.push(
                ValidationSeverity::StructuralCorruption,
                "GAP_ORDER",
                format!(
                    "gap position {} is followed by {} (recorded in timeline order)",
                    window[0].instruction_position, window[1].instruction_position
                ),
                ValidationLocation::TraceGap(0),
            );
            break;
        }
    }
}

// -- coverage -------------------------------------------------------------------

fn validate_coverage(trace: &DebugSessionTrace, builder: &mut Builder) {
    let coverage = &trace.recording.coverage;
    let retained = trace.recording.instruction_events.len();
    let attempted = trace.recording.instruction_count;

    if !coverage.complete_for_scope {
        let reasons = if coverage.truncation_reasons.is_empty() {
            String::new()
        } else {
            format!("; reasons: {}", coverage.truncation_reasons.join(", "))
        };
        builder.push(
            ValidationSeverity::PartialCoverage,
            "SCOPE_INCOMPLETE",
            format!("scope `{}` recorded as incomplete{reasons}", coverage.scope),
            ValidationLocation::Trace,
        );
    }
    if !coverage.instruction_capture_enabled {
        builder.push(
            ValidationSeverity::PartialCoverage,
            "INSTRUCTION_CAPTURE_DISABLED",
            "the trace mode did not record instruction events".to_string(),
            ValidationLocation::Trace,
        );
    } else if retained == 0 && attempted > 0 {
        // Retention gap: instructions were attempted but nothing retained.
        // (When events were retained at all, an attempted/retained mismatch
        // is corruption and the engine validator already reported it.)
        builder.push(
            ValidationSeverity::PartialCoverage,
            "INSTRUCTION_RETENTION_GAP",
            format!("{attempted} instructions were attempted but no events were retained"),
            ValidationLocation::Trace,
        );
    }
    if !trace.recording.storage.finalized {
        builder.push(
            ValidationSeverity::PartialCoverage,
            "JOURNAL_NOT_FINALIZED",
            "the session journal never reached its finalize record".to_string(),
            ValidationLocation::Trace,
        );
    }
}

// -- diagnostics -----------------------------------------------------------------

fn validate_diagnostics(trace: &DebugSessionTrace, builder: &mut Builder) {
    use crate::models::ExecutionStatus;
    if trace.crash_report.is_none() && trace.execution_status == ExecutionStatus::Crashed {
        builder.push(
            ValidationSeverity::UnavailableEvidence,
            "CRASH_REPORT_UNRECORDED",
            "the execution crashed but no crash report was recorded".to_string(),
            ValidationLocation::Trace,
        );
    }
    if trace.valgrind_report.is_none() {
        builder.push(
            ValidationSeverity::OptionalAbsence,
            "VALGRIND_REPORT_UNRECORDED",
            "no Valgrind report attached (skipped, unavailable, or non-crash run)".to_string(),
            ValidationLocation::Trace,
        );
    }
    if trace.sanitizer_report.is_none() {
        builder.push(
            ValidationSeverity::OptionalAbsence,
            "SANITIZER_REPORT_UNRECORDED",
            "no sanitizer report attached (the pass only runs after a crash)".to_string(),
            ValidationLocation::Trace,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ExecutionSnapshot, HeapBlockInfo, InstructionEvent, SnapshotObservation, SourceContext,
    };

    fn minimal_trace() -> DebugSessionTrace {
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
                "analysis_notes": [],
                "coverage": {
                    "scope": "session",
                    "instruction_capture_enabled": true,
                    "complete_for_scope": true,
                    "truncation_reasons": [],
                    "stop_model": "none",
                    "limitations": []
                },
                "storage": {
                    "journal": null,
                    "finalized": true,
                    "bytes_written": 0,
                    "resident_event_bytes": 0
                }
            }"#,
        )
        .unwrap()
    }

    fn event(position: u64, stop_before: u64, checkpoint: Option<usize>) -> InstructionEvent {
        InstructionEvent {
            stop_id_before: stop_before,
            stop_id_after: Some(stop_before + 1),
            event_id: position,
            instruction_id: None,
            address: format!("0x{position:x}"),
            next_address: None,
            thread_id: None,
            pid: None,
            source: SourceContext::default(),
            checkpoint_id: checkpoint,
            outcome: "completed".into(),
            stopped_reason: None,
            signal: None,
            registers: None,
            memory: Vec::new(),
        }
    }

    #[test]
    fn empty_trace_is_sound() {
        let report = validate_deep(&minimal_trace());
        assert!(report.is_structurally_sound());
        assert!(!report.is_partial());
        assert_eq!(report.corruption_count(), 0);
        let codes: Vec<&str> = report.findings().iter().map(|f| f.code).collect();
        assert!(codes.contains(&"VALGRIND_REPORT_UNRECORDED"));
        assert!(codes.contains(&"SANITIZER_REPORT_UNRECORDED"));
    }

    #[test]
    fn broken_event_order_is_corruption_not_partiality() {
        let mut trace = minimal_trace();
        trace.recording.instruction_events = vec![event(0, 5, None), event(1, 4, None)];
        let report = validate_deep(&trace);
        assert!(!report.is_structurally_sound());
        let stop_order = report
            .findings()
            .iter()
            .find(|f| f.code == "EVENT_STOP_ORDER")
            .expect("EVENT_STOP_ORDER finding");
        assert_eq!(
            stop_order.severity,
            ValidationSeverity::StructuralCorruption
        );
        assert_eq!(stop_order.occurrences, 1);
    }

    #[test]
    fn missing_snapshot_journal_entry_is_partial_coverage() {
        let mut trace = minimal_trace();
        for stop in [1u64, 2] {
            trace.snapshots.push(ExecutionSnapshot {
                observation: SnapshotObservation {
                    stop_id: stop,
                    ..Default::default()
                },
                step_index: trace.snapshots.len(),
                ..Default::default()
            });
        }
        // Only the first snapshot made it into the journal checkpoint list.
        trace.recording.source_checkpoints = vec![0];
        let report = validate_deep(&trace);
        assert!(report.is_structurally_sound(), "partial, not corrupt");
        assert!(report.is_partial());
        let finding = report
            .findings()
            .iter()
            .find(|f| f.code == "CHECKPOINT_JOURNAL_INCOMPLETE")
            .expect("CHECKPOINT_JOURNAL_INCOMPLETE");
        assert_eq!(finding.severity, ValidationSeverity::PartialCoverage);
        assert_eq!(finding.occurrences, 1);
    }

    #[test]
    fn duplicate_heap_state_identity_is_aggregated_corruption() {
        let mut trace = minimal_trace();
        for _ in 0..3 {
            trace.heap_state.push(HeapBlockInfo {
                allocation_id: 7,
                address: "0x1000".into(),
                size_bytes: 16,
                allocation_type: "malloc".into(),
                is_freed: false,
                observed_at_stop: None,
                allocation_file: None,
                allocation_line: None,
                raw_bytes_hex: None,
                memory_read_error: None,
                realloc_from_id: None,
                memory_evidence: None,
                lifetime_status: None,
            });
        }
        let report = validate_deep(&trace);
        assert!(!report.is_structurally_sound());
        let finding = report
            .findings()
            .iter()
            .find(|f| f.code == "HEAP_STATE_ID_DUPLICATE")
            .expect("HEAP_STATE_ID_DUPLICATE");
        assert_eq!(
            finding.occurrences, 2,
            "aggregated (first entry seeds the set)"
        );
        assert_eq!(finding.location, ValidationLocation::HeapState(1));
    }

    #[test]
    fn engine_validator_failure_is_surfaced() {
        let mut trace = minimal_trace();
        // event_id must equal the vector position: corrupt it.
        let mut broken = event(0, 1, None);
        broken.event_id = 42;
        trace.recording.instruction_events = vec![broken];
        let report = validate_deep(&trace);
        assert!(!report.is_structurally_sound());
        assert!(
            report
                .findings()
                .iter()
                .any(|f| f.code == "ENGINE_STRUCTURAL_VALIDATION"),
            "engine validator failure must surface as a finding"
        );
    }
}

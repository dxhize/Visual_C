//! V2.2 Phase 7: bounded, one-time cross-provider evidence reconciliation.
//!
//! This layer only adds optional references between records that already
//! exist: GCC DWARF, optional Clang source syntax, GDB observations, and heap
//! instrumentation events. It never overwrites runtime values, addresses,
//! extents, activation identities, pointer provenance, heap lifetimes, or GCC
//! layouts, and it never turns source syntax into execution evidence.
//!
//! All lookup tables are built once per trace from already-serialized records.
//! No GDB command is issued, no checkpoint is revisited, and no registry is
//! copied into snapshots.

mod declarations;
mod heap;
mod source_links;
mod validation;

use std::path::PathBuf;
use std::time::Instant;

use crate::models::{DebugSessionTrace, EvidenceFusionSummary};

fn working_directory(trace: &DebugSessionTrace) -> PathBuf {
    trace
        .session_meta
        .as_ref()
        .and_then(|meta| meta.working_directory.as_ref())
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Runs once after DWARF association and heap reconstruction. A trace with no
/// static provider remains untouched (`evidence_fusion` stays absent).
pub fn fuse(trace: &mut DebugSessionTrace) {
    if trace.type_registry.is_none() && trace.source_metadata.is_none() {
        return;
    }
    let started = Instant::now();
    let cwd = working_directory(trace);
    let mut summary = EvidenceFusionSummary {
        dwarf_available: trace.type_registry.is_some(),
        ast_available: trace.source_metadata.is_some(),
        ..Default::default()
    };
    let cross_provider_compatible = trace
        .source_metadata
        .as_ref()
        .is_some_and(|registry| registry.cross_provider_correlation_unavailable.is_none());

    if cross_provider_compatible {
        if let (Some(dwarf), Some(registry)) =
            (trace.type_registry.as_ref(), trace.source_metadata.as_mut())
        {
            let index = declarations::DwarfIndex::new(dwarf, &cwd);
            declarations::link_source_members(registry, &index);
            source_links::link_expressions(registry, &index);
        }
    }

    if cross_provider_compatible {
        summary.runtime_source_links =
            source_links::link_runtime(trace.source_metadata.as_ref(), &mut trace.snapshots);
    }

    if cross_provider_compatible {
        if let Some(registry) = trace.source_metadata.as_mut() {
            heap::correlate(registry, &trace.heap_events, &cwd);
        }
    }

    let validation_started = Instant::now();
    summary.rejected_links = validation::audit(
        &mut trace.source_metadata,
        trace.type_registry.as_ref(),
        &mut trace.snapshots,
        &trace.heap_events,
    );
    summary.validation_time_us = validation_started.elapsed().as_micros() as u64;

    count_links(trace, &mut summary);
    summary.fusion_time_us = started.elapsed().as_micros() as u64;
    trace.recording.profiling.evidence_fusion_time_us = Some(summary.fusion_time_us);
    trace.recording.profiling.evidence_validation_time_us = Some(summary.validation_time_us);
    trace.evidence_fusion = Some(summary);
}

fn count_links(trace: &DebugSessionTrace, summary: &mut EvidenceFusionSummary) {
    if let Some(registry) = trace.source_metadata.as_ref() {
        summary.declaration_links = registry
            .declarations
            .iter()
            .filter(|declaration| declaration.dwarf_declaration.is_some())
            .count();
        summary.declaration_line_fallbacks = registry
            .declarations
            .iter()
            .filter(|declaration| declaration.correlation == "unique_line_with_missing_column")
            .count();
        summary.declaration_type_links = registry
            .declarations
            .iter()
            .filter(|declaration| declaration.dwarf_type_ref.is_some())
            .count();
        summary.expression_dwarf_links = registry
            .expressions
            .iter()
            .filter(|expression| expression.referenced_dwarf_declaration.is_some())
            .count();
        summary.expression_type_links = registry
            .expressions
            .iter()
            .filter(|expression| expression.dwarf_type_ref.is_some())
            .count();
        summary.member_links = registry
            .declarations
            .iter()
            .filter(|declaration| declaration.dwarf_member.is_some())
            .count()
            + registry
                .expressions
                .iter()
                .filter(|expression| expression.member_ref.is_some())
                .count();
        summary.heap_call_candidates = registry.heap_call_candidates.len();
        summary.heap_call_ambiguities = registry.heap_call_ambiguities.len();
    }

    let mut examined = 0;
    let mut linked = 0;
    let mut visit = |variables: &[crate::models::VariableInfo]| {
        examined += variables.len();
        linked += variables
            .iter()
            .filter(|variable| variable.source_declaration_ref.is_some())
            .count();
    };
    for snapshot in &trace.snapshots {
        visit(&snapshot.variables);
        visit(&snapshot.observation.globals);
        for frame in &snapshot.caller_frames {
            visit(&frame.variables);
        }
    }
    summary.runtime_variables_examined = examined;
    summary.runtime_source_links = linked;
}

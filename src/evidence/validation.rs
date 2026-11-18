//! Construction-time audit of every Phase 7 reference.
//!
//! The fusion pass builds these links itself, so an invalid link indicates a
//! construction defect or a hand-edited trace. Invalid new references are
//! cleared rather than serialized; the count is reported in the summary. The
//! Trace Parser separately reports dangling references as structural
//! corruption.

use std::collections::{HashMap, HashSet};

use crate::clang_ast::{DwarfMemberRef, SourceMetadataRegistry};
use crate::dwarf::{TypeEntry, TypeRegistry};
use crate::models::{ExecutionSnapshot, HeapEvent, VariableInfo};

pub(super) fn audit(
    source: &mut Option<SourceMetadataRegistry>,
    dwarf: Option<&TypeRegistry>,
    snapshots: &mut [ExecutionSnapshot],
    events: &[HeapEvent],
) -> usize {
    let dwarf_declarations: HashSet<&str> = dwarf
        .map(|registry| {
            registry
                .declarations
                .iter()
                .map(|declaration| declaration.id.as_str())
                .collect()
        })
        .unwrap_or_default();
    let types: HashMap<&str, &TypeEntry> = dwarf
        .map(|registry| {
            registry
                .types
                .iter()
                .map(|entry| (entry.id.as_str(), entry))
                .collect()
        })
        .unwrap_or_default();
    let mut rejected = 0;

    if let Some(registry) = source.as_mut() {
        for declaration in &mut registry.declarations {
            if declaration
                .dwarf_declaration
                .as_deref()
                .is_some_and(|id| !dwarf_declarations.contains(id))
            {
                declaration.dwarf_declaration = None;
                declaration.correlation = "unresolved".into();
                rejected += 1;
            }
            if declaration
                .dwarf_type_ref
                .as_deref()
                .is_some_and(|id| !types.contains_key(id))
            {
                declaration.dwarf_type_ref = None;
                rejected += 1;
            }
            if declaration
                .dwarf_member
                .as_ref()
                .is_some_and(|member| !valid_member(member, &types))
            {
                declaration.dwarf_member = None;
                if declaration.kind == "FieldDecl" {
                    declaration.dwarf_type_ref = None;
                }
                rejected += 1;
            }
        }
        for expression in &mut registry.expressions {
            if expression
                .referenced_dwarf_declaration
                .as_deref()
                .is_some_and(|id| !dwarf_declarations.contains(id))
            {
                expression.referenced_dwarf_declaration = None;
                rejected += 1;
            }
            if expression
                .dwarf_type_ref
                .as_deref()
                .is_some_and(|id| !types.contains_key(id))
            {
                expression.dwarf_type_ref = None;
                expression.dwarf_type_evidence = None;
                rejected += 1;
            }
            if expression
                .member_ref
                .as_ref()
                .is_some_and(|member| !valid_member(member, &types))
            {
                expression.member_ref = None;
                if expression.dwarf_type_evidence.as_deref()
                    == Some("unique_parent_type_and_member_name")
                {
                    expression.dwarf_type_ref = None;
                    expression.dwarf_type_evidence = None;
                }
                rejected += 1;
            }
            if expression.dwarf_type_ref.is_none() {
                expression.dwarf_type_evidence = None;
            }
        }
        let expression_ids: HashSet<&str> = registry
            .expressions
            .iter()
            .map(|expression| expression.id.as_str())
            .collect();

        let before = registry.heap_call_candidates.len();
        registry.heap_call_candidates.retain(|candidate| {
            candidate.event_index < events.len()
                && expression_ids.contains(candidate.expression_id.as_str())
        });
        rejected += before - registry.heap_call_candidates.len();

        let mut ambiguities = Vec::with_capacity(registry.heap_call_ambiguities.len());
        for mut ambiguity in std::mem::take(&mut registry.heap_call_ambiguities) {
            if ambiguity.event_index >= events.len() {
                rejected += 1;
                continue;
            }
            let before = ambiguity.expression_ids.len();
            ambiguity
                .expression_ids
                .retain(|id| expression_ids.contains(id.as_str()));
            rejected += before - ambiguity.expression_ids.len();
            if ambiguity.expression_ids.is_empty() {
                rejected += 1;
                continue;
            }
            ambiguities.push(ambiguity);
        }
        registry.heap_call_ambiguities = ambiguities;
    }

    let declarations: HashMap<&str, &crate::clang_ast::SourceDeclaration> = source
        .as_ref()
        .map(|registry| {
            registry
                .declarations
                .iter()
                .map(|declaration| (declaration.id.as_str(), declaration))
                .collect()
        })
        .unwrap_or_default();
    let visit = |variables: &mut [VariableInfo]| -> usize {
        let mut rejected = 0;
        for variable in variables {
            let bridge_valid =
                variable
                    .source_declaration_ref
                    .as_deref()
                    .is_some_and(|source_id| {
                        let Some(declaration) = declarations.get(source_id) else {
                            return false;
                        };
                        declaration.dwarf_declaration.is_some()
                            && declaration.dwarf_declaration.as_deref()
                                == variable.declaration_ref.as_deref()
                    });
            if variable.source_declaration_ref.is_some() && !bridge_valid {
                variable.source_declaration_ref = None;
                rejected += 1;
            }
        }
        rejected
    };
    for snapshot in snapshots {
        rejected += visit(&mut snapshot.variables);
        for frame in &mut snapshot.caller_frames {
            rejected += visit(&mut frame.variables);
        }
        rejected += visit(&mut snapshot.observation.globals);
    }
    rejected
}

fn valid_member(member: &DwarfMemberRef, types: &HashMap<&str, &TypeEntry>) -> bool {
    let Some(parent) = types.get(member.type_ref.as_str()) else {
        return false;
    };
    if !matches!(parent.kind.as_str(), "struct" | "union") {
        return false;
    }
    parent
        .members
        .get(member.member_index)
        .is_some_and(|entry| {
            entry.name.as_deref() == Some(member.name.as_str())
                && entry.byte_offset == member.byte_offset
        })
}

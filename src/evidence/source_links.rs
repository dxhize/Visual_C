//! AST expression links and runtime-observation → source-declaration bridges.
//!
//! Runtime links are bridged only through the exact serialized DWARF
//! declaration identity attached by `dwarf::associate`; activation IDs and
//! every observed value, address, extent and status stay untouched.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::clang_ast::SourceMetadataRegistry;
use crate::models::{ExecutionSnapshot, VariableInfo};

use super::declarations::DwarfIndex;

pub(super) fn link_expressions(registry: &mut SourceMetadataRegistry, index: &DwarfIndex<'_>) {
    let unit_files: HashMap<&str, &str> = registry
        .translation_units
        .iter()
        .map(|unit| (unit.id.as_str(), unit.file.as_str()))
        .collect();
    let declarations: HashMap<&str, usize> = registry
        .declarations
        .iter()
        .enumerate()
        .map(|(position, declaration)| (declaration.id.as_str(), position))
        .collect();

    for expression in &mut registry.expressions {
        expression.referenced_dwarf_declaration = None;
        expression.dwarf_type_ref = None;
        expression.dwarf_type_evidence = None;
        expression.member_ref = None;

        let referenced = expression
            .referenced_declaration
            .as_deref()
            .and_then(|id| declarations.get(id))
            .map(|&position| &registry.declarations[position]);
        if let Some(declaration) = referenced {
            expression.referenced_dwarf_declaration = declaration.dwarf_declaration.clone();
            if let Some(type_ref) = declaration.dwarf_type_ref.clone() {
                expression.dwarf_type_ref = Some(type_ref);
                expression.dwarf_type_evidence = Some("referenced_declaration_type".into());
            }
            if let Some(member) = declaration.dwarf_member.clone() {
                expression.member_ref = Some(member);
            }
        }

        let unit_file = unit_files.get(expression.unit_id.as_str()).copied();
        if expression.kind == "MemberExpr" && expression.member_ref.is_none() {
            if let (Some(file), Some(parent), Some(name)) = (
                unit_file,
                expression.base_type_spelling.as_deref(),
                expression.member_name.as_deref(),
            ) {
                if let Some((member, member_type)) = index.member_link(file, parent, name) {
                    expression.member_ref = Some(member);
                    if expression.dwarf_type_ref.is_none() {
                        if let Some(type_ref) = member_type {
                            expression.dwarf_type_ref = Some(type_ref.to_owned());
                            expression.dwarf_type_evidence =
                                Some("unique_parent_type_and_member_name".into());
                        }
                    }
                }
            }
        }

        if expression.dwarf_type_ref.is_none() {
            let candidate = if expression.sizeof_operand_kind.as_deref() == Some("type") {
                expression
                    .sizeof_operand
                    .as_deref()
                    .map(|spelling| (spelling, "sizeof_operand_type"))
            } else {
                None
            }
            .or_else(|| {
                expression
                    .type_spelling
                    .as_deref()
                    .map(|spelling| (spelling, "unique_type_name_in_ast_translation_unit"))
            });
            if let (Some((spelling, evidence)), Some(file)) = (candidate, unit_file) {
                if let Some(entry) = index.unique_type(file, spelling) {
                    expression.dwarf_type_ref = Some(entry.id.clone());
                    expression.dwarf_type_evidence = Some(evidence.into());
                }
            }
        }
    }
}

/// Adds `source_declaration_ref` only where a unique AST declaration claims the
/// same exact DWARF declaration identity. Returns the linked observation count.
pub(super) fn link_runtime(
    source: Option<&SourceMetadataRegistry>,
    snapshots: &mut [ExecutionSnapshot],
) -> usize {
    let Some(source) = source else { return 0 };
    let mut bridge: HashMap<&str, Option<&str>> = HashMap::new();
    for declaration in &source.declarations {
        let Some(dwarf) = declaration.dwarf_declaration.as_deref() else {
            continue;
        };
        match bridge.entry(dwarf) {
            Entry::Vacant(slot) => {
                slot.insert(Some(declaration.id.as_str()));
            }
            Entry::Occupied(mut slot) => {
                // Two source declarations claiming one DWARF declaration stay
                // unlinked instead of silently choosing one.
                slot.insert(None);
            }
        }
    }
    let apply = |variables: &mut [VariableInfo]| -> usize {
        let mut linked = 0;
        for variable in variables {
            variable.source_declaration_ref = None;
            if let Some(source_id) = variable
                .declaration_ref
                .as_deref()
                .and_then(|id| bridge.get(id))
                .copied()
                .flatten()
            {
                variable.source_declaration_ref = Some(source_id.to_owned());
                linked += 1;
            }
        }
        linked
    };
    let mut linked = 0;
    for snapshot in snapshots {
        linked += apply(&mut snapshot.variables);
        for frame in &mut snapshot.caller_frames {
            linked += apply(&mut frame.variables);
        }
        linked += apply(&mut snapshot.observation.globals);
    }
    linked
}

//! DWARF declarations, unique parent records and member identities.
//!
//! Member links require both a unique struct/union in the AST translation
//! unit's GCC compilation unit and a unique member name inside it. A field
//! name alone is never enough.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::clang_ast::{DwarfMemberRef, SourceMetadataRegistry};
use crate::dwarf::{Member, TypeEntry, TypeRegistry, UnitInfo};

pub(super) struct DwarfIndex<'a> {
    types: HashMap<&'a str, &'a TypeEntry>,
    units_by_file: HashMap<String, Vec<&'a str>>,
    named: HashMap<(String, String), Vec<&'a TypeEntry>>,
}

impl<'a> DwarfIndex<'a> {
    pub(super) fn new(dwarf: &'a TypeRegistry, cwd: &Path) -> Self {
        let mut units_by_file: HashMap<String, Vec<&'a str>> = HashMap::new();
        for unit in &dwarf.units {
            if let Some(file) = unit_file(unit, cwd) {
                units_by_file
                    .entry(file)
                    .or_default()
                    .push(unit.id.as_str());
            }
        }
        let mut named: HashMap<(String, String), Vec<&'a TypeEntry>> = HashMap::new();
        for entry in &dwarf.types {
            if let Some(key) = entry_key(entry) {
                named
                    .entry((entry.unit_id.clone(), key))
                    .or_default()
                    .push(entry);
            }
        }
        Self {
            types: dwarf
                .types
                .iter()
                .map(|entry| (entry.id.as_str(), entry))
                .collect(),
            units_by_file,
            named,
        }
    }

    fn unique_unit(&self, file: &str) -> Option<&'a str> {
        match self.units_by_file.get(file)?.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    /// The unique complete struct/union matching an AST parent spelling in
    /// the translation unit's own GCC compilation unit.
    fn unique_record(&self, file: &str, spelling: &str) -> Option<&'a TypeEntry> {
        let key = spelling_key(spelling)?;
        if !(key.starts_with("struct:") || key.starts_with("union:")) {
            return None;
        }
        let unit = self.unique_unit(file)?;
        let candidates: Vec<_> = self
            .named
            .get(&(unit.to_string(), key))?
            .iter()
            .copied()
            .filter(|entry| !entry.declaration_only)
            .collect();
        match candidates.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }

    fn unique_member(&self, parent: &'a TypeEntry, name: &str) -> Option<(usize, &'a Member)> {
        let mut matches = parent
            .members
            .iter()
            .enumerate()
            .filter(|(_, member)| member.name.as_deref() == Some(name));
        let (index, member) = matches.next()?;
        matches.next().is_none().then_some((index, member))
    }

    /// Member identity plus the member's own DWARF type, when both resolve.
    pub(super) fn member_link(
        &self,
        file: &str,
        parent_spelling: &str,
        member_name: &str,
    ) -> Option<(DwarfMemberRef, Option<&'a str>)> {
        let parent = self.unique_record(file, parent_spelling)?;
        let (member_index, member) = self.unique_member(parent, member_name)?;
        let member_type = member
            .type_ref
            .as_deref()
            .filter(|id| self.types.contains_key(id));
        Some((
            DwarfMemberRef {
                type_ref: parent.id.clone(),
                member_index,
                name: member_name.to_owned(),
                byte_offset: member.byte_offset,
            },
            member_type,
        ))
    }

    /// Conservative expression-type fallback: only a plain, unique named type
    /// in the TU's GCC compilation unit. Pointer/array syntax is not parsed
    /// here and stays unresolved.
    pub(super) fn unique_type(&self, file: &str, spelling: &str) -> Option<&'a TypeEntry> {
        let key = spelling_key(spelling)?;
        let unit = self.unique_unit(file)?;
        let candidates: Vec<_> = self
            .named
            .get(&(unit.to_string(), key))?
            .iter()
            .copied()
            .filter(|entry| !entry.declaration_only)
            .collect();
        match candidates.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }
}

/// Links FieldDecl source declarations to unique DWARF members.
pub(super) fn link_source_members(registry: &mut SourceMetadataRegistry, index: &DwarfIndex<'_>) {
    for declaration in &mut registry.declarations {
        if declaration.kind != "FieldDecl" {
            continue;
        }
        declaration.dwarf_member = None;
        let Some(parent) = declaration.parent_type_spelling.as_deref() else {
            continue;
        };
        let Some((member, member_type)) =
            index.member_link(&declaration.span.file, parent, &declaration.name)
        else {
            continue;
        };
        declaration.dwarf_member = Some(member);
        if let Some(type_ref) = member_type {
            declaration.dwarf_type_ref = Some(type_ref.to_owned());
        }
        declaration.correlation = "unique_parent_type_and_member_name".into();
    }
}

fn unit_file(unit: &UnitInfo, cwd: &Path) -> Option<String> {
    let name = unit.name.as_deref()?;
    if name.is_empty() || name == "<artificial>" {
        return None;
    }
    let path = Path::new(name);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        let base = unit
            .compilation_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| cwd.to_path_buf());
        base.join(path)
    };
    Some(
        joined
            .canonicalize()
            .unwrap_or(joined)
            .display()
            .to_string(),
    )
}

fn entry_key(entry: &TypeEntry) -> Option<String> {
    let name = entry.name.as_deref()?;
    match entry.kind.as_str() {
        "struct" | "union" | "enum" => Some(format!("{}:{name}", entry.kind)),
        "base" | "typedef" => Some(format!("name:{name}")),
        _ => None,
    }
}

/// Strips cv-qualifiers and a struct/union/enum tag, rejecting any spelling
/// that needs structural C type parsing (`*`, arrays, calls, multi-word base
/// types). Ambiguity remains explicit rather than guessed.
fn spelling_key(spelling: &str) -> Option<String> {
    let cleaned = spelling
        .split_whitespace()
        .filter(|word| {
            !matches!(
                *word,
                "const" | "volatile" | "restrict" | "_Atomic" | "__restrict"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty()
        || cleaned
            .chars()
            .any(|character| matches!(character, '*' | '[' | ']' | '(' | ')'))
    {
        return None;
    }
    let (tag, name) = match cleaned.split_once(' ') {
        Some((tag @ ("struct" | "union" | "enum"), rest)) => (Some(tag), rest),
        _ => (None, cleaned.as_str()),
    };
    if name.is_empty() || name.split_whitespace().count() != 1 {
        return None;
    }
    Some(match tag {
        Some(tag) => format!("{tag}:{name}"),
        None => format!("name:{name}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dwarf::{Member, UnitInfo};

    fn unit() -> UnitInfo {
        UnitInfo {
            id: "debug:u0".into(),
            name: Some("/tmp/phase7-members.c".into()),
            compilation_dir: Some("/tmp".into()),
            dwarf_version: 5,
            address_size: 8,
        }
    }

    fn record(id: &str, members: Vec<Member>) -> TypeEntry {
        TypeEntry {
            id: id.into(),
            unit_id: "debug:u0".into(),
            die_offset: 2,
            kind: "struct".into(),
            name: Some("Pair".into()),
            byte_size: Some(8),
            encoding: None,
            referenced_type: None,
            members,
            dimensions: Vec::new(),
            enumerators: Vec::new(),
            declaration_only: false,
            runtime_dependent: false,
        }
    }

    fn member(name: &str, offset: u64, type_ref: Option<&str>) -> Member {
        Member {
            name: Some(name.into()),
            type_ref: type_ref.map(str::to_owned),
            byte_offset: Some(offset),
            implicit_union_zero: false,
            bit_size: None,
            data_bit_offset: None,
            legacy_bit_offset: None,
            byte_size: Some(4),
        }
    }

    #[test]
    fn spelling_key_rejects_structural_c_types() {
        assert_eq!(spelling_key("struct Pair"), Some("struct:Pair".into()));
        assert_eq!(spelling_key("const int"), Some("name:int".into()));
        assert_eq!(spelling_key("int *"), None);
        assert_eq!(spelling_key("int [3]"), None);
        assert_eq!(spelling_key("unsigned long"), None);
    }

    #[test]
    fn member_link_requires_one_parent_record_and_one_member_name() {
        let int = TypeEntry {
            id: "debug:u0:o1".into(),
            unit_id: "debug:u0".into(),
            die_offset: 1,
            kind: "base".into(),
            name: Some("int".into()),
            byte_size: Some(4),
            encoding: Some(5),
            referenced_type: None,
            members: Vec::new(),
            dimensions: Vec::new(),
            enumerators: Vec::new(),
            declaration_only: false,
            runtime_dependent: false,
        };
        let registry = TypeRegistry {
            units: vec![unit()],
            types: vec![
                int,
                record(
                    "debug:u0:o2",
                    vec![
                        member("x", 0, Some("debug:u0:o1")),
                        member("y", 4, Some("debug:u0:o1")),
                    ],
                ),
            ],
            ..Default::default()
        };
        let index = DwarfIndex::new(&registry, Path::new("/tmp"));
        let (link, member_type) = index
            .member_link("/tmp/phase7-members.c", "struct Pair", "x")
            .unwrap();
        assert_eq!(link.type_ref, "debug:u0:o2");
        assert_eq!(link.member_index, 0);
        assert_eq!(link.byte_offset, Some(0));
        assert_eq!(member_type, Some("debug:u0:o1"));
        assert!(index
            .member_link("/tmp/phase7-members.c", "struct Pair", "missing")
            .is_none());

        // A second complete record with the same name in the same CU makes the
        // parent ambiguous; the member name alone must never resolve it.
        let mut ambiguous = registry.clone();
        ambiguous.types.push(record(
            "debug:u0:o3",
            vec![member("x", 0, Some("debug:u0:o1"))],
        ));
        let index = DwarfIndex::new(&ambiguous, Path::new("/tmp"));
        assert!(index
            .member_link("/tmp/phase7-members.c", "struct Pair", "x")
            .is_none());
    }
}

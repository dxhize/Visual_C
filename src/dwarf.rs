//! Bounded, compilation-scoped DWARF metadata extraction. No runtime
//! location expression is evaluated here: GDB remains the runtime authority.
use crate::models::{DebugSessionTrace, VariableInfo};
use gimli::{AttributeValue, EndianRcSlice, Reader, RunTimeEndian};
use object::{Object, ObjectSection};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

const MAX_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_UNITS: usize = 512;
const MAX_ENTRIES: usize = 150_000;
const MAX_CHILDREN: usize = 30_000;
const MAX_TEXT: usize = 4096;
const MAX_EXTRACT_TIME: Duration = Duration::from_secs(5);
/// Specification/abstract-origin chains are metadata indirection, not nested
/// source scopes. Four hops covers GCC declaration/definition chains while
/// keeping malformed/cyclic input bounded.
const MAX_ORIGIN_HOPS: usize = 4;

/// One trace-local registry, stored once at session level.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeRegistry {
    pub provider: String,
    pub artifact: String,
    pub units: Vec<UnitInfo>,
    /// Includes CUs omitted from the serialized user registry (for example
    /// the engine's allocator instrumentation source).
    pub compilation_units_scanned: usize,
    pub types: Vec<TypeEntry>,
    pub declarations: Vec<Declaration>,
    pub aggregate_members: usize,
    pub unsupported_die_count: usize,
    pub unresolved_type_references: usize,
    pub unresolved_origin_references: usize,
    /// Unsupported references/attributes are not silently treated as facts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitInfo {
    pub id: String,
    pub name: Option<String>,
    pub compilation_dir: Option<String>,
    pub dwarf_version: u16,
    pub address_size: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeEntry {
    pub id: String,
    pub unit_id: String,
    pub die_offset: u64,
    pub kind: String,
    pub name: Option<String>,
    pub byte_size: Option<u64>,
    pub encoding: Option<u64>,
    pub referenced_type: Option<String>,
    pub members: Vec<Member>,
    pub dimensions: Vec<Dimension>,
    pub enumerators: Vec<Enumerator>,
    pub declaration_only: bool,
    /// True when an unhandled DWARF expression is present. No result guessed.
    pub runtime_dependent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub name: Option<String>,
    pub type_ref: Option<String>,
    pub byte_offset: Option<u64>,
    /// Union member's zero offset was supplied by DWARF's union rule, not an
    /// explicit DW_AT_data_member_location attribute.
    pub implicit_union_zero: bool,
    pub bit_size: Option<u64>,
    /// DW_AT_data_bit_offset: offset in bits from the containing type start.
    pub data_bit_offset: Option<u64>,
    /// Older DW_AT_bit_offset has storage-unit-relative semantics; preserve
    /// separately rather than converting without storage/endianness evidence.
    pub legacy_bit_offset: Option<u64>,
    pub byte_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dimension {
    pub lower_bound: Option<i64>,
    pub upper_bound: Option<i64>,
    pub count: Option<u64>,
    /// `static`, `runtime_dependent`, or `unavailable`.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enumerator {
    pub name: String,
    pub value: i128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Declaration {
    pub id: String,
    pub unit_id: String,
    pub kind: String,
    pub name: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub function: Option<String>,
    pub scope_id: String,
    pub type_ref: Option<String>,
    pub external: Option<bool>,
    pub declaration_only: bool,
    /// Set only when name/file/line/column/type were completed from a
    /// DW_AT_specification or DW_AT_abstract_origin chain, e.g.
    /// `specification_same_cu` or `abstract_origin_cross_cu`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_origin: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ExtractionMetrics {
    pub extraction_us: u64,
    pub resolution_us: u64,
    pub association_us: u64,
}

type R = EndianRcSlice<RunTimeEndian>;

fn text(reader: R) -> Option<String> {
    let s = reader.to_string_lossy().ok()?;
    (s.len() <= MAX_TEXT).then(|| s.into_owned())
}
fn attr_text(d: &gimli::Dwarf<R>, u: &gimli::Unit<R>, v: AttributeValue<R>) -> Option<String> {
    text(d.attr_string(u, v).ok()?)
}
fn name(
    d: &gimli::Dwarf<R>,
    u: &gimli::Unit<R>,
    e: &gimli::DebuggingInformationEntry<R>,
) -> Option<String> {
    e.attr(gimli::DW_AT_name)
        .and_then(|a| attr_text(d, u, a.value()))
}
fn unsigned(e: &gimli::DebuggingInformationEntry<R>, a: gimli::DwAt) -> Option<u64> {
    e.attr(a).and_then(|v| v.udata_value())
}
fn signed(e: &gimli::DebuggingInformationEntry<R>, a: gimli::DwAt) -> Option<i64> {
    e.attr(a).and_then(|v| {
        v.sdata_value()
            .or_else(|| v.udata_value().and_then(|n| n.try_into().ok()))
    })
}
fn decl_flag(e: &gimli::DebuggingInformationEntry<R>) -> bool {
    e.attr(gimli::DW_AT_declaration)
        .is_some_and(|v| matches!(v.value(), AttributeValue::Flag(true)))
}
fn type_kind(tag: gimli::DwTag) -> Option<&'static str> {
    Some(match tag {
        gimli::DW_TAG_base_type => "base",
        gimli::DW_TAG_pointer_type => "pointer",
        gimli::DW_TAG_array_type => "array",
        gimli::DW_TAG_structure_type => "struct",
        gimli::DW_TAG_union_type => "union",
        gimli::DW_TAG_enumeration_type => "enum",
        gimli::DW_TAG_typedef => "typedef",
        gimli::DW_TAG_const_type => "const",
        gimli::DW_TAG_volatile_type => "volatile",
        gimli::DW_TAG_restrict_type => "restrict",
        gimli::DW_TAG_atomic_type => "atomic",
        gimli::DW_TAG_subroutine_type => "function",
        gimli::DW_TAG_unspecified_type => "unspecified",
        _ => return None,
    })
}
fn ref_key(
    value: AttributeValue<R>,
    unit: &str,
    absolute: &HashMap<usize, String>,
) -> Option<String> {
    match value {
        AttributeValue::UnitRef(o) => Some(format!("{unit}:o{:x}", o.0)),
        AttributeValue::DebugInfoRef(o) => absolute.get(&o.0).cloned(),
        _ => None,
    }
}
/// Attributes completed from a specification/abstract-origin chain. Only DIEs
/// that actually reference an origin are merged, so ordinary DIEs pay nothing.
#[derive(Default)]
struct OriginFacts {
    name: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
    type_ref: Option<String>,
    descriptions: Vec<String>,
    origin_present: bool,
    used_origin: bool,
    failed: bool,
}

impl OriginFacts {
    fn resolved_origin(&self) -> Option<String> {
        self.used_origin.then(|| self.descriptions.join("+"))
    }
}

/// Resolves specification/abstract-origin DIEs, including absolute
/// `DW_AT_*origin` references that cross compilation units. Units are cached
/// by absolute `.debug_info` offset; chains are bounded and cycle-checked.
struct OriginResolver<'a> {
    dwarf: &'a gimli::Dwarf<R>,
    absolute: &'a HashMap<usize, String>,
    units: RefCell<HashMap<u64, Rc<gimli::Unit<R>>>>,
}

impl<'a> OriginResolver<'a> {
    fn new(dwarf: &'a gimli::Dwarf<R>, absolute: &'a HashMap<usize, String>) -> Self {
        Self {
            dwarf,
            absolute,
            units: RefCell::new(HashMap::new()),
        }
    }

    fn header_key(header: &gimli::UnitHeader<R>) -> u64 {
        header
            .offset()
            .to_debug_info_offset(header)
            .map_or(u64::MAX, |offset| offset.0 as u64)
    }

    fn unit_id(header: &gimli::UnitHeader<R>) -> String {
        format!(
            "debug:u{:x}",
            header
                .offset()
                .to_debug_info_offset(header)
                .map_or(0, |offset| offset.0)
        )
    }

    fn unit_from_header(
        &self,
        header: gimli::UnitHeader<R>,
    ) -> std::result::Result<Rc<gimli::Unit<R>>, String> {
        let key = Self::header_key(&header);
        if key != u64::MAX {
            if let Some(hit) = self.units.borrow().get(&key) {
                return Ok(hit.clone());
            }
        }
        let unit = Rc::new(self.dwarf.unit(header).map_err(|e| e.to_string())?);
        if key != u64::MAX && self.units.borrow().len() < MAX_UNITS {
            self.units.borrow_mut().insert(key, unit.clone());
        }
        Ok(unit)
    }

    /// Absolute `DW_AT_*origin` references point at a DIE, not a unit header.
    /// The indexed DIE → unit identity map gives the owning header without
    /// another section scan. Failure stays explicit and never falls back to
    /// name/line guessing.
    fn unit_at(&self, offset: usize) -> Option<Rc<gimli::Unit<R>>> {
        let unit_id = self.absolute.get(&offset)?;
        // Values are `debug:u<hex>:o<hex>`; the prefix before `:o` is the
        // owning unit identity.
        let header_offset = unit_id
            .split_once(":o")
            .map(|(unit, _)| unit)
            .unwrap_or(unit_id)
            .strip_prefix("debug:u")
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())?;
        let header = self
            .dwarf
            .debug_info
            .header_from_offset(gimli::DebugInfoOffset(header_offset as usize))
            .ok()?;
        self.unit_from_header(header).ok()
    }

    fn merge(
        &self,
        unit: &gimli::Unit<R>,
        unit_id: &str,
        entry: &gimli::DebuggingInformationEntry<R>,
        supplemental: bool,
        facts: &mut OriginFacts,
    ) {
        let mut took = false;
        if facts.name.is_none() {
            if let Some(value) = name(self.dwarf, unit, entry) {
                facts.name = Some(value);
                took |= supplemental;
            }
        }
        if facts.file.is_none() {
            if let Some(value) = source_file(self.dwarf, unit, entry) {
                facts.file = Some(value);
                took |= supplemental;
            }
        }
        if facts.line.is_none() {
            if let Some(value) =
                unsigned(entry, gimli::DW_AT_decl_line).and_then(|value| value.try_into().ok())
            {
                facts.line = Some(value);
                took |= supplemental;
            }
        }
        if facts.column.is_none() {
            if let Some(value) =
                unsigned(entry, gimli::DW_AT_decl_column).and_then(|value| value.try_into().ok())
            {
                facts.column = Some(value);
                took |= supplemental;
            }
        }
        if facts.type_ref.is_none() {
            if let Some(value) = entry
                .attr(gimli::DW_AT_type)
                .and_then(|attr| ref_key(attr.value(), unit_id, self.absolute))
            {
                facts.type_ref = Some(value);
                took |= supplemental;
            }
        }
        if took {
            facts.used_origin = true;
        }
    }

    fn resolve(
        &self,
        unit: &Rc<gimli::Unit<R>>,
        unit_id: &str,
        entry: &gimli::DebuggingInformationEntry<R>,
    ) -> OriginFacts {
        let mut facts = OriginFacts::default();
        let has_origin = entry.attr(gimli::DW_AT_specification).is_some()
            || entry.attr(gimli::DW_AT_abstract_origin).is_some();
        if !has_origin {
            return facts;
        }
        facts.origin_present = true;
        self.merge(unit, unit_id, entry, false, &mut facts);

        let mut current_unit = unit.clone();
        let mut current_entry = entry.clone();
        let mut visited = HashSet::from([(
            Self::header_key(&current_unit.header),
            current_entry.offset().0 as u64,
        )]);
        let mut hops = 0;
        while let Some((value, kind)) = current_entry
            .attr(gimli::DW_AT_specification)
            .map(|attr| (attr.value(), "specification"))
            .or_else(|| {
                current_entry
                    .attr(gimli::DW_AT_abstract_origin)
                    .map(|attr| (attr.value(), "abstract_origin"))
            })
        {
            if hops >= MAX_ORIGIN_HOPS {
                facts.failed = true;
                break;
            }
            let (next_unit, next_offset) = match value {
                AttributeValue::UnitRef(offset) => (current_unit.clone(), offset),
                AttributeValue::DebugInfoRef(offset) => {
                    let Some(target) = self.unit_at(offset.0) else {
                        facts.failed = true;
                        break;
                    };
                    let Some(unit_offset) =
                        gimli::DebugInfoOffset(offset.0).to_unit_offset(&target.header)
                    else {
                        facts.failed = true;
                        break;
                    };
                    (target, unit_offset)
                }
                _ => {
                    facts.failed = true;
                    break;
                }
            };
            let cross_cu =
                Self::header_key(&next_unit.header) != Self::header_key(&current_unit.header);
            if !visited.insert((Self::header_key(&next_unit.header), next_offset.0 as u64)) {
                facts.failed = true;
                break;
            }
            let Ok(next_entry) = next_unit.entry(next_offset) else {
                facts.failed = true;
                break;
            };
            let next_unit_id = Self::unit_id(&next_unit.header);
            self.merge(&next_unit, &next_unit_id, &next_entry, true, &mut facts);
            facts.descriptions.push(format!(
                "{kind}_{}",
                if cross_cu { "cross_cu" } else { "same_cu" }
            ));
            hops += 1;
            current_unit = next_unit;
            current_entry = next_entry;
        }
        if hops == 0 {
            facts.failed = true;
        }
        // A same-CU chain keeps the pre-Phase-7 wording; cross-CU chains make
        // the absolute reference resolution visible.
        if facts.failed && facts.descriptions.is_empty() {
            facts.descriptions.push("unresolved".into());
        }
        facts
    }
}
fn source_file(
    d: &gimli::Dwarf<R>,
    u: &gimli::Unit<R>,
    e: &gimli::DebuggingInformationEntry<R>,
) -> Option<String> {
    let idx = unsigned(e, gimli::DW_AT_decl_file)?;
    let header = u.line_program.as_ref()?.header();
    let file = header.file(idx)?;
    let file_path = attr_text(d, u, file.path_name())?;
    let path = Path::new(&file_path);
    if path.is_absolute() {
        return Some(file_path);
    }
    let dir = file.directory(header).and_then(|v| attr_text(d, u, v));
    let comp_dir = u.comp_dir.clone().and_then(text);
    let candidate = match (comp_dir, dir) {
        (_, Some(dir)) if Path::new(&dir).is_absolute() => Path::new(&dir).join(path),
        (Some(base), Some(dir)) => Path::new(&base).join(dir).join(path),
        (Some(base), None) => Path::new(&base).join(path),
        (None, Some(dir)) => Path::new(&dir).join(path),
        (None, None) => return Some(file_path),
    };
    Some(
        candidate
            .canonicalize()
            .unwrap_or(candidate)
            .to_string_lossy()
            .into_owned(),
    )
}

// Only absolute paths identify the same source independently of a debugger's
// working directory. Keep lexical normalization for sources moved after build.
fn absolute_source_path(path: &str) -> Option<PathBuf> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return None;
    }
    if let Ok(canonical) = path.canonicalize() {
        return Some(canonical);
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    Some(normalized)
}

/// Reader boundary: own section bytes so no object-file lifetime escapes.
/// Enforce compressed section sizes before any decompression allocation.
fn load_dwarf(path: &Path) -> Result<gimli::Dwarf<R>, String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > MAX_OBJECT_BYTES {
        return Err("debug executable exceeds 64 MiB metadata input limit".into());
    }
    let binary = std::fs::read(path).map_err(|e| e.to_string())?;
    let obj = object::File::parse(&*binary).map_err(|e| e.to_string())?;
    if obj.section_by_name(".debug_info").is_none() && obj.section_by_name(".zdebug_info").is_none()
    {
        return Err("executable has no .debug_info section".into());
    }
    let endian = if obj.is_little_endian() {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    };
    let decoded_total = Cell::new(0usize);
    gimli::Dwarf::load(|section| {
        let legacy_name = section
            .name()
            .strip_prefix(".debug_")
            .map(|suffix| format!(".zdebug_{suffix}"));
        let object_section = obj.section_by_name(section.name()).or_else(|| {
            legacy_name
                .as_deref()
                .and_then(|name| obj.section_by_name(name))
        });
        let bytes = object_section
            .map(|s| {
                let compressed = s.compressed_data().map_err(|_| gimli::Error::Io)?;
                let size: usize = compressed
                    .uncompressed_size
                    .try_into()
                    .map_err(|_| gimli::Error::Io)?;
                let total = decoded_total
                    .get()
                    .checked_add(size)
                    .ok_or(gimli::Error::Io)?;
                if total > MAX_OBJECT_BYTES as usize {
                    return Err(gimli::Error::Io);
                }
                decoded_total.set(total);
                compressed
                    .decompress()
                    .map(|b| b.into_owned())
                    .map_err(|_| gimli::Error::Io)
            })
            .transpose()
            .map_err(|_| gimli::Error::Io)?
            .unwrap_or_default();
        Ok::<R, gimli::Error>(R::new(Rc::from(bytes), endian))
    })
    .map_err(|e| format!("DWARF section load: {e}"))
}

/// Index every DIE once for forward and cross-CU type references.
fn index_dies(
    dwarf: &gimli::Dwarf<R>,
    begun: Instant,
) -> Result<(Vec<String>, HashMap<usize, String>), String> {
    let mut units = dwarf.units();
    let mut absolute = HashMap::new();
    let mut entry_count = 0;
    let mut ids = Vec::new();
    while let Some(header) = units.next().map_err(|e| e.to_string())? {
        if ids.len() >= MAX_UNITS {
            return Err("DWARF compilation-unit limit reached".into());
        }
        if begun.elapsed() > MAX_EXTRACT_TIME {
            return Err("DWARF extraction deadline reached".into());
        }
        let unit = dwarf.unit(header).map_err(|e| e.to_string())?;
        let unit_id = format!(
            "debug:u{:x}",
            unit.header
                .offset()
                .to_debug_info_offset(&unit.header)
                .map_or(0, |o| o.0)
        );
        let mut cursor = unit.entries();
        while let Some(entry) = cursor.next_dfs().map_err(|e| e.to_string())? {
            entry_count += 1;
            if entry_count > MAX_ENTRIES {
                return Err("DWARF DIE limit reached".into());
            }
            if entry_count % 256 == 0 && begun.elapsed() > MAX_EXTRACT_TIME {
                return Err("DWARF extraction deadline reached".into());
            }
            if let Some(offset) = entry.offset().to_debug_info_offset(&unit.header) {
                absolute.insert(offset.0, format!("{unit_id}:o{:x}", entry.offset().0));
            }
        }
        ids.push(unit_id);
    }
    Ok((ids, absolute))
}

/// Load each section once; bounded by object size and a decoded-section cap.
pub fn extract(path: &Path) -> Result<(TypeRegistry, ExtractionMetrics), String> {
    extract_impl(path, None)
}

/// Exclude only the exact compiler-injected instrumentation CU. Other
/// compilation units, including identically named user files, remain.
pub fn extract_excluding(
    path: &Path,
    instrumentation_source: &Path,
) -> Result<(TypeRegistry, ExtractionMetrics), String> {
    extract_impl(path, Some(instrumentation_source))
}

fn extract_impl(
    path: &Path,
    instrumentation: Option<&Path>,
) -> Result<(TypeRegistry, ExtractionMetrics), String> {
    let begun = Instant::now();
    let dwarf = load_dwarf(path)?;

    let mut reg = TypeRegistry {
        provider: "gcc_dwarf".into(),
        artifact: "debug_executable".into(),
        ..Default::default()
    };
    let (headers, absolute) = index_dies(&dwarf, begun)?;
    reg.compilation_units_scanned = headers.len();
    let origins = OriginResolver::new(&dwarf, &absolute);
    // Reopen unit iterator: small header vector, no repeated section decode.
    let mut units = dwarf.units();
    let mut index = 0;
    let mut unsupported_tags = std::collections::HashSet::new();
    while let Some(header) = units.next().map_err(|e| e.to_string())? {
        if begun.elapsed() > MAX_EXTRACT_TIME {
            return Err("DWARF extraction deadline reached".into());
        }
        let unit = origins.unit_from_header(header)?;
        let unit_id = &headers[index];
        index += 1;
        if instrumentation.is_some_and(|excluded| {
            unit.name
                .as_ref()
                .and_then(|n| text(n.clone()))
                .is_some_and(|name| Path::new(&name) == excluded)
        }) {
            continue;
        }
        reg.units.push(UnitInfo {
            id: unit_id.clone(),
            name: unit.name.clone().and_then(text),
            compilation_dir: unit.comp_dir.clone().and_then(text),
            dwarf_version: unit.header.version(),
            address_size: unit.header.encoding().address_size,
        });
        let mut cursor = unit.entries();
        let mut parents: Vec<(isize, Option<usize>, Option<String>, String)> = Vec::new();
        while let Some(e) = cursor.next_dfs().map_err(|er| er.to_string())? {
            if begun.elapsed() > MAX_EXTRACT_TIME {
                return Err("DWARF extraction deadline reached".into());
            }
            while parents.last().is_some_and(|p| p.0 >= e.depth()) {
                parents.pop();
            }
            let parent_type = parents.last().and_then(|p| p.1);
            let function = parents.iter().rev().find_map(|p| p.2.clone());
            let parent_scope = parents
                .last()
                .map(|p| p.3.clone())
                .unwrap_or_else(|| unit_id.clone());
            let id = format!("{unit_id}:o{:x}", e.offset().0);
            let facts = origins.resolve(&unit, unit_id, e);
            if facts.origin_present && facts.failed {
                reg.unresolved_origin_references += 1;
            }
            let typeref = if facts.origin_present {
                facts.type_ref.clone()
            } else {
                e.attr(gimli::DW_AT_type)
                    .and_then(|a| ref_key(a.value(), unit_id, &absolute))
            };
            let kind = type_kind(e.tag());
            if kind.is_none()
                && !matches!(
                    e.tag(),
                    gimli::DW_TAG_compile_unit
                        | gimli::DW_TAG_subprogram
                        | gimli::DW_TAG_lexical_block
                        | gimli::DW_TAG_inlined_subroutine
                        | gimli::DW_TAG_variable
                        | gimli::DW_TAG_formal_parameter
                        | gimli::DW_TAG_member
                        | gimli::DW_TAG_subrange_type
                        | gimli::DW_TAG_enumerator
                )
            {
                reg.unsupported_die_count += 1;
                if unsupported_tags.len() < 16 {
                    unsupported_tags.insert(format!("{:?}", e.tag()));
                }
            }
            let mut this_type = None;
            if let Some(kind) = kind {
                if reg.types.len() >= MAX_ENTRIES {
                    return Err("DWARF type limit reached".into());
                }
                let byte_size = unsigned(e, gimli::DW_AT_byte_size).or_else(|| {
                    (kind == "pointer").then_some(unit.header.encoding().address_size as u64)
                });
                reg.types.push(TypeEntry {
                    id: id.clone(),
                    unit_id: unit_id.clone(),
                    die_offset: e.offset().0 as u64,
                    kind: kind.into(),
                    name: name(&dwarf, &unit, e),
                    byte_size,
                    encoding: unsigned(e, gimli::DW_AT_encoding),
                    referenced_type: typeref.clone(),
                    members: Vec::new(),
                    dimensions: Vec::new(),
                    enumerators: Vec::new(),
                    declaration_only: decl_flag(e),
                    runtime_dependent: false,
                });
                this_type = Some(reg.types.len() - 1);
            } else if e.tag() == gimli::DW_TAG_member {
                if let Some(i) = parent_type
                    .filter(|&i| matches!(reg.types[i].kind.as_str(), "struct" | "union"))
                {
                    if reg.aggregate_members >= MAX_CHILDREN {
                        return Err("DWARF aggregate member limit reached".into());
                    }
                    reg.aggregate_members += 1;
                    let location = e.attr(gimli::DW_AT_data_member_location);
                    let implicit_union_zero = reg.types[i].kind == "union" && location.is_none();
                    let byte_offset = location
                        .and_then(|a| a.udata_value())
                        .or_else(|| implicit_union_zero.then_some(0));
                    if location.is_some() && byte_offset.is_none() {
                        reg.types[i].runtime_dependent = true;
                    }
                    reg.types[i].members.push(Member {
                        name: name(&dwarf, &unit, e),
                        type_ref: typeref.clone(),
                        byte_offset,
                        implicit_union_zero,
                        bit_size: unsigned(e, gimli::DW_AT_bit_size),
                        data_bit_offset: unsigned(e, gimli::DW_AT_data_bit_offset),
                        legacy_bit_offset: unsigned(e, gimli::DW_AT_bit_offset),
                        byte_size: unsigned(e, gimli::DW_AT_byte_size),
                    });
                }
            } else if e.tag() == gimli::DW_TAG_subrange_type {
                if let Some(i) = parent_type.filter(|&i| reg.types[i].kind == "array") {
                    let lower = signed(e, gimli::DW_AT_lower_bound).or(Some(0)); // DWARF C default lower bound
                    let upper = signed(e, gimli::DW_AT_upper_bound);
                    let count = unsigned(e, gimli::DW_AT_count).or_else(|| {
                        upper
                            .zip(lower)
                            .and_then(|(hi, lo)| (hi >= lo).then_some((hi - lo + 1) as u64))
                    });
                    let dynamic = [gimli::DW_AT_count, gimli::DW_AT_upper_bound]
                        .iter()
                        .any(|&a| {
                            e.attr(a).is_some_and(|v| {
                                v.udata_value().is_none() && v.sdata_value().is_none()
                            })
                        });
                    reg.types[i].runtime_dependent |= dynamic;
                    reg.types[i].dimensions.push(Dimension {
                        lower_bound: lower,
                        upper_bound: upper,
                        count,
                        status: if count.is_some() {
                            "static"
                        } else if dynamic {
                            "runtime_dependent"
                        } else {
                            "unavailable"
                        }
                        .into(),
                    });
                }
            } else if e.tag() == gimli::DW_TAG_enumerator {
                if let Some(i) = parent_type.filter(|&i| reg.types[i].kind == "enum") {
                    if let (Some(n), Some(v)) =
                        (name(&dwarf, &unit, e), e.attr(gimli::DW_AT_const_value))
                    {
                        if let Some(value) = v
                            .sdata_value()
                            .map(i128::from)
                            .or_else(|| v.udata_value().map(i128::from))
                        {
                            reg.types[i].enumerators.push(Enumerator { name: n, value });
                        }
                    }
                }
            }
            if matches!(
                e.tag(),
                gimli::DW_TAG_variable | gimli::DW_TAG_formal_parameter | gimli::DW_TAG_subprogram
            ) {
                if reg.declarations.len() >= MAX_ENTRIES {
                    return Err("DWARF declaration limit reached".into());
                }
                reg.declarations.push(Declaration {
                    id: id.clone(),
                    unit_id: unit_id.clone(),
                    kind: match e.tag() {
                        gimli::DW_TAG_formal_parameter => "parameter",
                        // Kept for source-level function correlation. Runtime
                        // variable association filters this kind out.
                        gimli::DW_TAG_subprogram => "subprogram",
                        _ => "variable",
                    }
                    .into(),
                    name: facts.name.clone().or_else(|| name(&dwarf, &unit, e)),
                    file: facts.file.clone().or_else(|| source_file(&dwarf, &unit, e)),
                    line: facts.line.or_else(|| {
                        unsigned(e, gimli::DW_AT_decl_line).and_then(|n| n.try_into().ok())
                    }),
                    column: facts.column.or_else(|| {
                        unsigned(e, gimli::DW_AT_decl_column).and_then(|n| n.try_into().ok())
                    }),
                    function,
                    scope_id: parent_scope,
                    type_ref: typeref,
                    external: e.attr(gimli::DW_AT_external).and_then(|a| match a.value() {
                        AttributeValue::Flag(b) => Some(b),
                        _ => None,
                    }),
                    declaration_only: decl_flag(e),
                    resolved_origin: facts.resolved_origin(),
                });
            }
            let func = if e.tag() == gimli::DW_TAG_subprogram {
                facts.name.clone().or_else(|| name(&dwarf, &unit, e))
            } else {
                None
            };
            parents.push((e.depth(), this_type, func, id));
        }
    }
    let resolving = Instant::now();
    let valid: std::collections::HashSet<_> = reg.types.iter().map(|t| t.id.clone()).collect();
    // A dangling reference remains explicitly absent; it must not masquerade
    // as an existing type.
    for t in &mut reg.types {
        if t.referenced_type
            .as_deref()
            .is_some_and(|r| !valid.contains(r))
        {
            reg.unresolved_type_references += 1;
            t.referenced_type = None;
        }
        for m in &mut t.members {
            if m.type_ref.as_deref().is_some_and(|r| !valid.contains(r)) {
                reg.unresolved_type_references += 1;
                m.type_ref = None;
            }
        }
    }
    for d in &mut reg.declarations {
        if d.type_ref.as_deref().is_some_and(|r| !valid.contains(r)) {
            reg.unresolved_type_references += 1;
            d.type_ref = None;
        }
    }
    if !unsupported_tags.is_empty() {
        let mut tags: Vec<_> = unsupported_tags.into_iter().collect();
        tags.sort();
        reg.limitations.push(format!(
            "unsupported DIE tags (first 16): {}",
            tags.join(", ")
        ));
    }
    if reg.unresolved_type_references > 0 {
        reg.limitations.push(format!(
            "{} type references unresolved",
            reg.unresolved_type_references
        ));
    }
    if reg.unresolved_origin_references > 0 {
        reg.limitations.push(format!(
            "{} specification/abstract-origin references unsupported or unavailable",
            reg.unresolved_origin_references
        ));
    }
    let metrics = ExtractionMetrics {
        extraction_us: begun.elapsed().as_micros() as u64,
        resolution_us: resolving.elapsed().as_micros() as u64,
        ..Default::default()
    };
    if reg.units.is_empty() {
        return Err("no eligible DWARF compilation units".into());
    }
    Ok((reg, metrics))
}

/// Small, immutable observation lookup built once, before GDB starts. This is
/// not another registry or a runtime object graph.
#[derive(Debug)]
pub struct ObservationTypes {
    declarations: HashMap<(String, u32, String), Vec<ObservationDeclaration>>,
    symbols: HashMap<(String, String), Vec<ObservationDeclaration>>,
    pointer_types: HashMap<String, bool>,
}

#[derive(Debug, Clone)]
struct ObservationDeclaration {
    id: String,
    type_ref: String,
    function: Option<String>,
    file: String,
    line: u32,
    size: Option<u64>,
    aggregate: bool,
}

impl ObservationTypes {
    pub fn new(reg: &TypeRegistry) -> Self {
        let types: HashMap<_, _> = reg.types.iter().map(|t| (t.id.as_str(), t)).collect();
        let mut out = Self {
            declarations: HashMap::new(),
            symbols: HashMap::new(),
            pointer_types: HashMap::new(),
        };
        for decl in &reg.declarations {
            let (Some(file), Some(line), Some(name), Some(id)) =
                (&decl.file, decl.line, &decl.name, &decl.type_ref)
            else {
                continue;
            };
            if decl.declaration_only || decl.kind.contains("subprogram") {
                continue;
            }
            let mut current = types.get(id.as_str()).copied();
            let mut shape = (None, false, None);
            // Qualifier/typedef cycles and missing links must not yield a size.
            for _ in 0..64 {
                let Some(t) = current else { break };
                if t.declaration_only {
                    break;
                }
                if t.runtime_dependent {
                    shape = (
                        None,
                        matches!(t.kind.as_str(), "array" | "struct" | "union"),
                        Some(t.kind == "pointer"),
                    );
                    break;
                }
                if matches!(
                    t.kind.as_str(),
                    "typedef" | "const" | "volatile" | "restrict" | "atomic"
                ) {
                    current = t
                        .referenced_type
                        .as_deref()
                        .and_then(|id| types.get(id).copied());
                } else {
                    shape = (
                        t.byte_size,
                        matches!(t.kind.as_str(), "array" | "struct" | "union"),
                        Some(t.kind == "pointer"),
                    );
                    break;
                }
            }
            if !types.contains_key(id.as_str()) {
                continue;
            }
            if let Some(pointer) = shape.2 {
                out.pointer_types.insert(id.clone(), pointer);
            }
            let entry = ObservationDeclaration {
                id: decl.id.clone(),
                type_ref: id.clone(),
                function: decl.function.clone(),
                file: file.clone(),
                line,
                size: shape.0,
                aggregate: shape.1,
            };
            out.declarations
                .entry((file.clone(), line, name.clone()))
                .or_default()
                .push(entry.clone());
            out.symbols
                .entry((decl.function.clone().unwrap_or_default(), name.clone()))
                .or_default()
                .push(entry);
        }
        out
    }

    /// Attach only uniquely correlated facts. A runtime size always wins;
    /// pointer sizes describe the pointer object, never its pointee.
    pub fn enrich(&self, var: &mut VariableInfo, func: &str) -> bool {
        let qualified = var.name.rsplit_once("::");
        let quoted_file = qualified.and_then(|(qualifier, _)| {
            qualifier
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        });
        let same_file = |file: &str| {
            quoted_file.is_none_or(|qualified| {
                absolute_source_path(qualified)
                    .zip(absolute_source_path(file))
                    .is_some_and(|(qualified, declaration)| qualified == declaration)
            })
        };
        let candidates = var
            .declaration_file
            .as_ref()
            .zip(var.declaration_line)
            .and_then(|(file, line)| {
                self.declarations
                    .get(&(file.clone(), line, var.name.clone()))
            });
        let mut plausible = candidates
            .into_iter()
            .flatten()
            .filter(|d| d.function.as_deref().is_none_or(|f| f == func) && same_file(&d.file));
        let first = plausible.next();
        if plausible.next().is_some() {
            return false;
        }
        let declaration = first.or_else(|| {
            // Only GDB-qualified symbols may be linked without a declaration
            // line. Bare names could refer to a shadowing automatic variable.
            let (qualifier, name) = qualified?;
            let key = (
                if quoted_file.is_some() { "" } else { qualifier }.to_string(),
                name.to_string(),
            );
            let mut candidates = self
                .symbols
                .get(&key)?
                .iter()
                .filter(|d| same_file(&d.file));
            let first = candidates.next()?;
            candidates.next().is_none().then_some(first)
        });
        let Some(d) = declaration else { return false };
        var.type_ref = Some(d.type_ref.clone());
        var.declaration_ref = Some(d.id.clone());
        var.declaration_file.get_or_insert_with(|| d.file.clone());
        var.declaration_line.get_or_insert(d.line);
        if var.size_bytes.is_none() && var.address.is_some() {
            if let Some(size) = d.size {
                var.size_bytes = Some(size);
                var.size_source = Some("dwarf".into());
            }
        }
        d.aggregate
    }

    pub fn is_pointer(&self, var: &VariableInfo) -> Option<bool> {
        var.type_ref
            .as_ref()
            .and_then(|id| self.pointer_types.get(id).copied())
    }
}

/// Conservative declaration association. Full file + declaration line +
/// variable name are mandatory. Function is checked when available. Multiple
/// plausible declarations leave the observation untouched.
pub fn associate(trace: &mut DebugSessionTrace) -> u64 {
    let started = Instant::now();
    let Some(reg) = trace.type_registry.as_ref() else {
        return 0;
    };
    let types: std::collections::HashSet<_> = reg.types.iter().map(|t| t.id.as_str()).collect();
    let mut lookup: HashMap<(String, u32, String), Vec<&Declaration>> = HashMap::new();
    for decl in &reg.declarations {
        if let (Some(file), Some(line), Some(name), Some(_)) =
            (&decl.file, decl.line, &decl.name, &decl.type_ref)
        {
            if !decl.declaration_only
                && !decl.kind.contains("subprogram")
                && decl
                    .type_ref
                    .as_deref()
                    .is_some_and(|id| types.contains(id))
            {
                lookup
                    .entry((file.clone(), line, name.clone()))
                    .or_default()
                    .push(decl);
            }
        }
    }
    fn link(
        var: &mut VariableInfo,
        func: &str,
        lookup: &HashMap<(String, u32, String), Vec<&Declaration>>,
    ) {
        let (Some(file), Some(line)) = (&var.declaration_file, var.declaration_line) else {
            return;
        };
        let Some(candidates) = lookup.get(&(file.clone(), line, var.name.clone())) else {
            return;
        };
        let plausible: Vec<_> = candidates
            .iter()
            .filter(|d| d.function.as_deref().is_none_or(|f| f == func))
            .collect();
        if let [d] = plausible.as_slice() {
            var.type_ref = d.type_ref.clone();
            var.declaration_ref = Some(d.id.clone());
        }
    }
    for snap in &mut trace.snapshots {
        for var in &mut snap.variables {
            link(var, &snap.func, &lookup);
        }
        for frame in &mut snap.caller_frames {
            for var in &mut frame.variables {
                link(var, &frame.frame.func, &lookup);
            }
        }
        // Globals without a trustworthy declaration file/line stay unresolved.
        for var in &mut snap.observation.globals {
            link(var, &snap.func, &lookup);
        }
    }
    started.elapsed().as_micros() as u64
}

#[cfg(test)]
mod observation_tests {
    use super::*;

    fn registry(runtime_dependent: bool) -> TypeRegistry {
        TypeRegistry {
            types: vec![TypeEntry {
                id: "type:array".into(),
                unit_id: "unit".into(),
                die_offset: 1,
                kind: "array".into(),
                name: None,
                byte_size: Some(8192),
                encoding: None,
                referenced_type: None,
                members: vec![],
                dimensions: vec![],
                enumerators: vec![],
                declaration_only: false,
                runtime_dependent,
            }],
            declarations: vec![Declaration {
                id: "decl:values".into(),
                unit_id: "unit".into(),
                kind: "variable".into(),
                name: Some("values".into()),
                file: Some("/code/main.c".into()),
                line: Some(4),
                column: None,
                function: Some("work".into()),
                scope_id: "block".into(),
                type_ref: Some("type:array".into()),
                external: None,
                declaration_only: false,
                resolved_origin: None,
            }],
            ..Default::default()
        }
    }

    fn variable() -> VariableInfo {
        VariableInfo {
            name: "values".into(),
            address: Some("0x1000".into()),
            declaration_file: Some("/code/main.c".into()),
            declaration_line: Some(4),
            ..Default::default()
        }
    }

    #[test]
    fn runtime_extent_wins_and_static_fallback_requires_unique_declaration() {
        let reg = registry(false);
        let types = ObservationTypes::new(&reg);
        let mut var = variable();
        assert!(types.enrich(&mut var, "work"));
        assert_eq!(var.size_bytes, Some(8192));
        assert_eq!(var.runtime_size_bytes, None);
        assert_eq!(var.size_source.as_deref(), Some("dwarf"));
        let mut runtime = variable();
        runtime.size_bytes = Some(24);
        types.enrich(&mut runtime, "work");
        assert_eq!(runtime.size_bytes, Some(24));
        let mut ambiguous = reg;
        ambiguous
            .declarations
            .push(ambiguous.declarations[0].clone());
        let mut var = variable();
        assert!(!ObservationTypes::new(&ambiguous).enrich(&mut var, "work"));
        assert!(var.type_ref.is_none() && var.size_bytes.is_none());
    }

    #[test]
    fn dynamic_extent_and_wrong_frame_are_never_guessed() {
        let mut var = variable();
        assert!(ObservationTypes::new(&registry(true)).enrich(&mut var, "work"));
        assert_eq!(var.size_bytes, None);
        let mut var = variable();
        assert!(!ObservationTypes::new(&registry(false)).enrich(&mut var, "other"));
        assert_eq!(var.size_bytes, None);
    }
}

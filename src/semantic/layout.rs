//! SE-2: lazy, bounded descriptions of *observed* C storage. No pointer
//! graph, inferred heap type, copied byte buffer, or host-endian decoding.
use std::ops::Range;
use std::path::Path;

use object::Object;
use thiserror::Error;

use super::{ObjectId, ObjectKind, ObjectRegistry, ObservationView, RegistryError};
use crate::dwarf::{Member, TypeEntry};
use crate::models::{DataValidity, MemoryEvidence};
use crate::trace_parser::{EvidenceConfidence, EvidenceLocation, QueryError};

pub const MAX_LAYOUT_DEPTH: usize = 16;
pub const MAX_CHILD_PAGE: u64 = 64;
pub const MAX_COMPOSITE_MEMBERS: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteOrder {
    Little,
    Big,
}

/// The trace does NOT record target byte order. This descriptor is obtained
/// from a caller-provided, independently inspected ELF, not from the host.
/// The caller must establish that this ELF actually represents this trace's
/// target; DWARF CU address size is checked again for each decoded type.
#[derive(Clone, Copy, Debug)]
pub struct TargetFormat {
    pub byte_order: ByteOrder,
    pub address_bytes: u8,
    /// Only the recorded ELF machine's known IEEE-754 32/64-bit formats.
    pub ieee_binary32_64: bool,
    pub flat_pointer: bool,
}
impl TargetFormat {
    pub fn inspect_elf(path: &Path) -> Result<Self> {
        let data = std::fs::read(path)?;
        let elf =
            object::File::parse(data.as_slice()).map_err(|_| LayoutError::Target("invalid ELF"))?;
        if elf.format() != object::BinaryFormat::Elf {
            return Err(LayoutError::Target("not an ELF"));
        }
        let known = matches!(
            elf.architecture(),
            object::Architecture::X86_64 | object::Architecture::I386
        );
        Ok(Self {
            byte_order: if elf.is_little_endian() {
                ByteOrder::Little
            } else {
                ByteOrder::Big
            },
            address_bytes: if elf.is_64() { 8 } else { 4 },
            ieee_binary32_64: known,
            flat_pointer: known,
        })
    }
}

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Evidence(#[from] QueryError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("target descriptor error: {0}")]
    Target(&'static str),
    #[error("object has no observation at this checkpoint")]
    Absent,
    #[error("multiple observations at this checkpoint; select an exact observation")]
    Ambiguous,
    #[error("layout depth or typedef/metadata cycle limit reached")]
    DepthLimit,
    #[error("requested child page exceeds the explicit 64-element bound")]
    PageLimit,
    #[error("child index is outside the recorded extent")]
    Index,
    #[error("child extent is not established")]
    UnknownExtent,
    #[error("offset, extent or dimension arithmetic overflow")]
    Overflow,
    #[error("metadata is inconsistent with observed storage: {0}")]
    Inconsistent(&'static str),
}
type Result<T> = std::result::Result<T, LayoutError>;
type MemberExtent = (Option<u64>, Option<Range<u64>>, Option<&'static str>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutKind {
    Scalar,
    Pointer,
    Array,
    Struct,
    Union,
    Enum,
    Opaque,
    Padding,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeStatus {
    DecodedBytesNotInitializationProof,
    NoTargetByteOrder,
    MissingType,
    MissingBytes,
    PartialBytes,
    UnsupportedRepresentation,
    AmbiguousBitOffset,
    InvalidRepresentation,
    OutOfBounds,
}
#[derive(Clone, Debug, PartialEq)]
pub enum TypedValue<'a> {
    Signed(i128),
    Unsigned(u128),
    Boolean(bool),
    Character(u8),
    Enum {
        number: i128,
        symbol: Option<&'a str>,
    },
    Address(u128),
    Float32(f32),
    Float64(f64),
}
#[derive(Clone, Debug, PartialEq)]
pub struct DecodeResult<'a> {
    pub value: Option<TypedValue<'a>>,
    pub status: DecodeStatus,
    /// A decoded *byte pattern*, never evidence of C initialization or union
    /// active-member validity. External target identity needs separate proof.
    pub confidence: EvidenceConfidence,
}
impl<'a> DecodeResult<'a> {
    fn unavailable(status: DecodeStatus) -> Self {
        Self {
            value: None,
            status,
            confidence: EvidenceConfidence::Unavailable,
        }
    }
    fn pattern(value: TypedValue<'a>) -> Self {
        Self {
            value: Some(value),
            status: DecodeStatus::DecodedBytesNotInitializationProof,
            confidence: EvidenceConfidence::Candidate,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodePath {
    Member(usize),
    Index(u64),
    Padding(usize),
    Unknown(usize),
}
/// A lightweight, checkpoint-local subobject description, never an SE-1 ID.
/// Type/name strings and metadata borrow the canonical Stage 1 trace.
#[derive(Clone, Debug)]
pub struct LayoutNode<'a> {
    pub path: Vec<NodePath>,
    pub kind: LayoutKind,
    pub original_type_id: Option<&'a str>,
    pub canonical_type_id: Option<&'a str>,
    pub display_name: Option<&'a str>,
    pub member_name: Option<&'a str>,
    pub offset: u64,
    pub size: Option<u64>,
    /// No alignment value is serialized in V2.2's TypeEntry.
    pub alignment: Option<u64>,
    /// Absolute bit positions from the owning object's beginning. Only
    /// DW_AT_data_bit_offset is interpreted. Legacy bit offsets stay opaque.
    pub bit_range: Option<Range<u64>>,
    pub overlaps_storage: bool,
    pub alternative_union_view: bool,
    /// Current dimension within the original DWARF array (row-major order).
    pub dimension: usize,
    pub unavailable_reason: Option<&'static str>,
    type_entry: Option<&'a TypeEntry>,
    ancestors: Vec<&'a str>,
}
impl<'a> LayoutNode<'a> {
    pub fn type_entry(&self) -> Option<&'a TypeEntry> {
        self.type_entry
    }
    pub fn depth(&self) -> usize {
        self.path.len()
    }
}

/// Borrowed hex corresponds only to this node's actually captured bytes.
/// A partial prefix is not zero-filled or claimed to be initialized.
#[derive(Clone, Debug)]
pub struct Capture<'a> {
    pub hex: Option<&'a str>,
    pub byte_range: Option<Range<u64>>,
    pub requested_range: Option<Range<u64>>,
    pub confidence: EvidenceConfidence,
    pub historical: bool,
}

/// Default format=None: shape and raw evidence are available; multibyte
/// numeric interpretation is explicitly unavailable until target verified.
pub struct LayoutEngine<'r, 'v, 'a> {
    registry: &'r ObjectRegistry<'v, 'a>,
    target: Option<TargetFormat>,
}
impl<'r, 'v, 'a> LayoutEngine<'r, 'v, 'a> {
    pub fn new(registry: &'r ObjectRegistry<'v, 'a>) -> Self {
        Self {
            registry,
            target: None,
        }
    }
    pub fn with_target(registry: &'r ObjectRegistry<'v, 'a>, target: TargetFormat) -> Self {
        Self {
            registry,
            target: Some(target),
        }
    }
    /// Select exactly one recorded observation; never reuse final heap-state
    /// memory when a historical checkpoint has no captured block.
    pub fn layout_for_object(
        &self,
        id: &ObjectId,
        checkpoint: usize,
    ) -> Result<MemoryLayout<'r, 'v, 'a>> {
        let mut observations = self.registry.observation_at(id, checkpoint)?;
        let obs = observations.next().ok_or(LayoutError::Absent)?;
        if observations.next().is_some() {
            return Err(LayoutError::Ambiguous);
        }
        self.layout_from_record(obs)
    }
    /// An exact retained observation can be selected even when a checkpoint
    /// has multiple records for one SE-1 object. Foreign refs fail first.
    pub fn layout_for_observation(
        &self,
        obs: &ObservationView<'_, 'v, 'a>,
    ) -> Result<MemoryLayout<'r, 'v, 'a>> {
        let id = &obs.object().id;
        self.registry.object(id)?;
        let location = obs.evidence()?.location();
        let owned = self
            .registry
            .observation_at(id, obs.record.checkpoint)?
            .find(|o| o.record.location == location)
            .ok_or(LayoutError::Absent)?;
        self.layout_from_record(owned)
    }
    fn layout_from_record(
        &self,
        obs: ObservationView<'_, 'v, 'a>,
    ) -> Result<MemoryLayout<'r, 'v, 'a>> {
        let obj = obs.object();
        let id = &obj.id;
        let checkpoint = obs.record.checkpoint;
        let reference = obs.evidence()?;
        let (original_type, extent, runtime_extent, hex, mem, captured, source_valid) =
            match obj.kind {
                ObjectKind::Heap => {
                    let block = obs.heap_block()?.ok_or(LayoutError::Absent)?;
                    let (valid, bytes) = qualified_bytes(
                        block.raw_bytes_hex.as_deref(),
                        block.memory_evidence.as_ref(),
                        None,
                    );
                    (
                        None,
                        Some(block.size_bytes),
                        None,
                        block.raw_bytes_hex.as_deref(),
                        block.memory_evidence.as_ref(),
                        bytes,
                        valid,
                    )
                }
                _ => {
                    let v = obs.variable()?.ok_or(LayoutError::Absent)?;
                    let valid = v.memory.valid_range.is_some();
                    (
                        v.info.type_ref.as_deref(),
                        v.info.runtime_size_bytes.or(v.info.size_bytes).or(v
                            .info
                            .memory_evidence
                            .as_ref()
                            .and_then(|m| m.extent_bytes)),
                        v.info.runtime_size_bytes,
                        v.memory.hex,
                        v.memory.observation,
                        v.memory.valid_range.as_ref().map(|r| r.end).unwrap_or(0),
                        valid,
                    )
                }
            };
        Ok(MemoryLayout {
            registry: self.registry,
            owner: id.clone(),
            checkpoint,
            evidence_location: reference.location(),
            original_type,
            extent,
            runtime_extent,
            hex,
            memory: mem,
            captured,
            source_valid,
            target: self.target,
            historical: obj.kind == ObjectKind::Heap,
        })
    }
}

/// This view holds only evidence positions, borrowed hex and a few numbers.
/// All child nodes/values are produced on demand. No stable subobject IDs.
pub struct MemoryLayout<'r, 'v, 'a> {
    registry: &'r ObjectRegistry<'v, 'a>,
    owner: ObjectId,
    pub checkpoint: usize,
    pub evidence_location: EvidenceLocation,
    pub original_type: Option<&'a str>,
    pub extent: Option<u64>,
    runtime_extent: Option<u64>,
    hex: Option<&'a str>,
    memory: Option<&'a MemoryEvidence>,
    captured: u64,
    source_valid: bool,
    target: Option<TargetFormat>,
    pub historical: bool,
}
impl<'r, 'v, 'a> MemoryLayout<'r, 'v, 'a> {
    pub fn owner(&self) -> &ObjectId {
        &self.owner
    }
    pub fn captured_prefix(&self) -> u64 {
        self.captured
    }
    pub fn raw_hex(&self) -> Option<&'a str> {
        self.hex
    }
    pub fn memory_evidence(&self) -> Option<&'a MemoryEvidence> {
        self.memory
    }
    fn lookup(&self, id: &str) -> Option<&'a TypeEntry> {
        self.registry.view.store().type_entry(id)
    }
    fn resolve(
        &self,
        id: Option<&'a str>,
    ) -> (Option<&'a TypeEntry>, Option<&'a str>, Option<&'static str>) {
        let mut id = id;
        let mut seen = [None; MAX_LAYOUT_DEPTH];
        for depth in 0..MAX_LAYOUT_DEPTH {
            let Some(key) = id else {
                return (None, None, Some("missing type reference"));
            };
            if seen[..depth].contains(&Some(key)) {
                return (None, None, Some("recursive typedef/qualifier metadata"));
            }
            seen[depth] = Some(key);
            let Some(ty) = self.lookup(key) else {
                return (None, None, Some("missing type definition"));
            };
            if matches!(
                ty.kind.as_str(),
                "typedef" | "const" | "volatile" | "restrict" | "atomic"
            ) {
                id = ty.referenced_type.as_deref();
            } else {
                return (Some(ty), Some(ty.id.as_str()), None);
            }
        }
        (None, None, Some("type resolution depth limit"))
    }
    fn node(
        &self,
        id: Option<&'a str>,
        path: Vec<NodePath>,
        offset: u64,
        size: Option<u64>,
        dimension: usize,
        ancestors: Vec<&'a str>,
    ) -> LayoutNode<'a> {
        let (ty, canonical, reason) = self.resolve(id);
        let kind = match ty.map(|t| t.kind.as_str()) {
            Some("base") => LayoutKind::Scalar,
            Some("pointer") => LayoutKind::Pointer,
            Some("array") => LayoutKind::Array,
            Some("struct") => LayoutKind::Struct,
            Some("union") => LayoutKind::Union,
            Some("enum") => LayoutKind::Enum,
            _ => LayoutKind::Opaque,
        };
        LayoutNode {
            path,
            kind,
            original_type_id: id,
            canonical_type_id: canonical,
            display_name: ty.and_then(|t| t.name.as_deref()),
            member_name: None,
            offset,
            size: size.or_else(|| ty.and_then(|t| t.byte_size)),
            alignment: None,
            bit_range: None,
            overlaps_storage: false,
            alternative_union_view: false,
            dimension,
            unavailable_reason: reason.or_else(|| {
                ty.and_then(|t| t.declaration_only.then_some("incomplete declaration"))
            }),
            type_entry: ty,
            ancestors,
        }
    }
    pub fn root(&self) -> LayoutNode<'a> {
        self.node(
            self.original_type,
            Vec::new(),
            0,
            self.extent,
            0,
            Vec::new(),
        )
    }
    /// What was captured *for this particular node*, with no zero-fill.
    pub fn capture(&self, node: &LayoutNode<'_>) -> Capture<'a> {
        let end = node.size.and_then(|size| node.offset.checked_add(size));
        let requested = end.map(|e| node.offset..e);
        let available = end
            .map(|e| e.min(self.captured))
            .filter(|e| *e > node.offset);
        let hex = available.and_then(|e| {
            if !self.source_valid {
                return None;
            }
            let start = usize::try_from(node.offset.checked_mul(2)?).ok()?;
            let finish = usize::try_from(e.checked_mul(2)?).ok()?;
            self.hex?.get(start..finish)
        });
        Capture {
            hex,
            byte_range: available.filter(|_| hex.is_some()).map(|e| node.offset..e),
            requested_range: requested,
            confidence: if hex.is_some() {
                if end.is_some_and(|e| e <= self.captured) {
                    EvidenceConfidence::Definite
                } else {
                    EvidenceConfidence::Candidate
                }
            } else {
                EvidenceConfidence::Unavailable
            },
            historical: self.historical,
        }
    }
    pub fn children_count(&self, node: &LayoutNode<'a>) -> Result<u64> {
        if node.path.len() >= MAX_LAYOUT_DEPTH {
            return Err(LayoutError::DepthLimit);
        }
        match node.kind {
            LayoutKind::Array => self.array_count(node),
            LayoutKind::Struct | LayoutKind::Union => {
                Ok(self.composite_children(node)?.len() as u64)
            }
            _ => Ok(0),
        }
    }
    /// Explicit page bound; a caller must request later pages. No truncation.
    pub fn children_page(
        &self,
        node: &LayoutNode<'a>,
        range: Range<u64>,
    ) -> Result<Vec<LayoutNode<'a>>> {
        if range.end < range.start || range.end - range.start > MAX_CHILD_PAGE {
            return Err(LayoutError::PageLimit);
        }
        let count = self.children_count(node)?;
        if range.end > count {
            return Err(LayoutError::Index);
        }
        if node.kind == LayoutKind::Array {
            range.map(|i| self.array_element(node, i)).collect()
        } else {
            Ok(self
                .composite_children(node)?
                .into_iter()
                .skip(range.start as usize)
                .take((range.end - range.start) as usize)
                .collect())
        }
    }
    pub fn member(&self, node: &LayoutNode<'a>, name: &str) -> Result<Option<LayoutNode<'a>>> {
        if !matches!(node.kind, LayoutKind::Struct | LayoutKind::Union) {
            return Ok(None);
        }
        Ok(self
            .composite_children(node)?
            .into_iter()
            .find(|child| child.member_name == Some(name)))
    }
    fn array_shape(&self, node: &LayoutNode<'a>) -> Result<(u64, u64)> {
        let ty = node.type_entry.ok_or(LayoutError::UnknownExtent)?;
        let element = self
            .resolve(ty.referenced_type.as_deref())
            .0
            .ok_or(LayoutError::UnknownExtent)?;
        let element_size = element.byte_size.ok_or(LayoutError::UnknownExtent)?;
        if element_size == 0 {
            return Err(LayoutError::UnknownExtent);
        }
        let dims = &ty.dimensions;
        if dims.len() > MAX_LAYOUT_DEPTH {
            return Err(LayoutError::DepthLimit);
        }
        if node.dimension >= dims.len() {
            return Err(LayoutError::UnknownExtent);
        }
        let unknown = dims.iter().filter(|d| d.count.is_none()).count();
        let mut counts = [None; MAX_LAYOUT_DEPTH];
        for (slot, dim) in counts.iter_mut().zip(dims) {
            *slot = dim.count;
        }
        let counts = &mut counts[..dims.len()];
        // Only the GDB-observed runtime sizeof of the *root array*, with a
        // single unknown dimension and exact divisibility, justifies a VLA.
        if unknown == 1 && self.resolve(self.original_type).1 == Some(ty.id.as_str()) {
            if let Some(extent) = self.runtime_extent {
                let divisor = counts
                    .iter()
                    .flatten()
                    .try_fold(element_size, |a, &b| a.checked_mul(b))
                    .ok_or(LayoutError::Overflow)?;
                if divisor != 0 && extent % divisor == 0 {
                    for count in counts.iter_mut() {
                        if count.is_none() {
                            *count = Some(extent / divisor);
                        }
                    }
                }
            }
        }
        let count = counts[node.dimension].ok_or(LayoutError::UnknownExtent)?;
        let stride = counts
            .iter()
            .skip(node.dimension + 1)
            .try_fold(element_size, |a, d| {
                a.checked_mul(d.ok_or(LayoutError::UnknownExtent)?)
                    .ok_or(LayoutError::Overflow)
            })?;
        let total = count.checked_mul(stride).ok_or(LayoutError::Overflow)?;
        if node.size.is_some_and(|observed| observed != total) {
            return Err(LayoutError::Inconsistent(
                "observed array extent conflicts with recorded dimensions",
            ));
        }
        Ok((count, stride))
    }
    fn array_count(&self, node: &LayoutNode<'a>) -> Result<u64> {
        self.array_shape(node).map(|p| p.0)
    }
    /// O(1) in the index, O(dimensions + bounded type-chain) in metadata.
    pub fn array_element(&self, node: &LayoutNode<'a>, index: u64) -> Result<LayoutNode<'a>> {
        if node.kind != LayoutKind::Array {
            return Err(LayoutError::UnknownExtent);
        }
        if node.path.len() >= MAX_LAYOUT_DEPTH {
            return Err(LayoutError::DepthLimit);
        }
        let (count, stride) = self.array_shape(node)?;
        if index >= count {
            return Err(LayoutError::Index);
        }
        let offset = node
            .offset
            .checked_add(index.checked_mul(stride).ok_or(LayoutError::Overflow)?)
            .ok_or(LayoutError::Overflow)?;
        if let Some(end) = node.size.and_then(|s| node.offset.checked_add(s)) {
            if offset.checked_add(stride).ok_or(LayoutError::Overflow)? > end {
                return Err(LayoutError::Inconsistent(
                    "element outside observed array extent",
                ));
            }
        }
        let mut path = node.path.clone();
        path.push(NodePath::Index(index));
        let ty = node.type_entry.ok_or(LayoutError::UnknownExtent)?;
        let next = if node.dimension + 1 < ty.dimensions.len() {
            let mut child = node.clone();
            child.path = path;
            child.offset = offset;
            child.size = Some(stride);
            child.dimension += 1;
            child
        } else {
            self.node(
                ty.referenced_type.as_deref(),
                path,
                offset,
                Some(stride),
                0,
                node.ancestors.clone(),
            )
        };
        Ok(next)
    }
    fn composite_children(&self, node: &LayoutNode<'a>) -> Result<Vec<LayoutNode<'a>>> {
        if node.path.len() >= MAX_LAYOUT_DEPTH {
            return Err(LayoutError::DepthLimit);
        }
        let Some(ty) = node.type_entry else {
            return Ok(Vec::new());
        };
        if node.unavailable_reason.is_some() {
            return Ok(Vec::new());
        }
        if ty
            .byte_size
            .zip(node.size)
            .is_some_and(|(static_size, observed)| static_size != observed)
        {
            return Err(LayoutError::Inconsistent(
                "observed composite extent conflicts with DWARF size",
            ));
        }
        if node.ancestors.contains(&ty.id.as_str()) {
            return Err(LayoutError::DepthLimit);
        }
        if ty.members.len() > MAX_COMPOSITE_MEMBERS {
            return Err(LayoutError::PageLimit);
        }
        let union = node.kind == LayoutKind::Union;
        let mut ancestors = node.ancestors.clone();
        ancestors.push(&ty.id);
        let mut children = Vec::with_capacity(ty.members.len() + 2);
        let mut spans = Vec::with_capacity(ty.members.len());
        // DWARF bitfields can share a wider storage unit. Unclaimed bytes
        // around them are not demonstrably compiler *padding*.
        let bitfields = ty.members.iter().any(|m| m.bit_size.is_some());
        let mut unknown = bitfields;
        for (i, m) in ty.members.iter().enumerate() {
            let mut path = node.path.clone();
            path.push(NodePath::Member(i));
            let rel = m
                .byte_offset
                .or_else(|| m.data_bit_offset.map(|bits| bits / 8));
            let Some(rel) = rel else {
                unknown = true;
                children.push(self.unknown_child(
                    path,
                    node.offset,
                    m.name.as_deref(),
                    "member offset unavailable",
                ));
                continue;
            };
            let offset = node.offset.checked_add(rel).ok_or(LayoutError::Overflow)?;
            let mut child = self.node(
                m.type_ref.as_deref(),
                path,
                offset,
                m.byte_size,
                0,
                ancestors.clone(),
            );
            child.member_name = m.name.as_deref();
            child.alternative_union_view = union;
            let (size, bit_range, reason) = self.member_extent(node, m, &child)?;
            child.size = size;
            child.bit_range = bit_range;
            child.unavailable_reason = child.unavailable_reason.or(reason);
            if let Some(size) = size {
                let end = rel.checked_add(size).ok_or(LayoutError::Overflow)?;
                if node.size.is_some_and(|max| end > max) {
                    return Err(LayoutError::Inconsistent("member exceeds containing type"));
                }
                let absolute_end = node.offset.checked_add(end).ok_or(LayoutError::Overflow)?;
                child.overlaps_storage = union;
                for prior in &mut children {
                    if let Some(prior_size) = prior.size {
                        let prior_end = prior
                            .offset
                            .checked_add(prior_size)
                            .ok_or(LayoutError::Overflow)?;
                        if prior.offset < absolute_end && offset < prior_end {
                            child.overlaps_storage = true;
                            prior.overlaps_storage = true;
                        }
                    }
                }
                spans.push((rel, end));
            } else {
                unknown = true;
            }
            children.push(child);
        }
        if !union {
            // Establish padding only when every member's span is known.
            spans.sort_unstable();
            let mut cursor = 0;
            let mut extras = Vec::new();
            for &(start, end) in &spans {
                if start > cursor {
                    extras.push((cursor, start));
                }
                cursor = cursor.max(end);
            }
            if let Some(extent) = node.size {
                if cursor < extent {
                    extras.push((cursor, extent));
                }
            }
            for (i, (start, end)) in extras.into_iter().enumerate() {
                let mut path = node.path.clone();
                path.push(if unknown {
                    NodePath::Unknown(i)
                } else {
                    NodePath::Padding(i)
                });
                children.push(LayoutNode {
                    path,
                    kind: if unknown {
                        LayoutKind::Unknown
                    } else {
                        LayoutKind::Padding
                    },
                    original_type_id: None,
                    canonical_type_id: None,
                    display_name: None,
                    member_name: None,
                    offset: node
                        .offset
                        .checked_add(start)
                        .ok_or(LayoutError::Overflow)?,
                    size: Some(end - start),
                    alignment: None,
                    bit_range: None,
                    overlaps_storage: false,
                    alternative_union_view: false,
                    dimension: 0,
                    unavailable_reason: if bitfields {
                        Some("unused bitfield storage, padding unproven")
                    } else {
                        unknown.then_some("unknown member storage prevents padding claim")
                    },
                    type_entry: None,
                    ancestors: Vec::new(),
                });
            }
        }
        Ok(children)
    }
    /// Member metadata not covering these bit positions. This is NOT a proof
    /// of initialized bytes, compiler padding or a union's active member.
    /// Bounded by at most 1,024 member spans plus the containing extent.
    pub fn unclaimed_bits(&self, node: &LayoutNode<'a>) -> Result<Vec<Range<u64>>> {
        if node.kind != LayoutKind::Struct {
            return Err(LayoutError::UnknownExtent);
        }
        let start = node.offset.checked_mul(8).ok_or(LayoutError::Overflow)?;
        let end = start
            .checked_add(
                node.size
                    .ok_or(LayoutError::UnknownExtent)?
                    .checked_mul(8)
                    .ok_or(LayoutError::Overflow)?,
            )
            .ok_or(LayoutError::Overflow)?;
        let ty = node.type_entry.ok_or(LayoutError::UnknownExtent)?;
        if ty.members.len() > MAX_COMPOSITE_MEMBERS {
            return Err(LayoutError::PageLimit);
        }
        let children = self.composite_children(node)?;
        let mut covered = Vec::with_capacity(ty.members.len());
        for child in children
            .into_iter()
            .filter(|c| matches!(c.path.last(), Some(NodePath::Member(_))))
        {
            let span = if let Some(bits) = child.bit_range {
                bits
            } else {
                let stop = child
                    .offset
                    .checked_add(child.size.ok_or(LayoutError::UnknownExtent)?)
                    .and_then(|n| n.checked_mul(8))
                    .ok_or(LayoutError::Overflow)?;
                child.offset.checked_mul(8).ok_or(LayoutError::Overflow)?..stop
            };
            covered.push(span);
        }
        covered.sort_by_key(|s| s.start);
        let mut gaps = Vec::new();
        let mut cursor = start;
        for span in covered {
            if span.start > cursor {
                gaps.push(cursor..span.start);
            }
            cursor = cursor.max(span.end);
        }
        if cursor < end {
            gaps.push(cursor..end);
        }
        Ok(gaps)
    }
    fn unknown_child(
        &self,
        path: Vec<NodePath>,
        offset: u64,
        name: Option<&'a str>,
        why: &'static str,
    ) -> LayoutNode<'a> {
        LayoutNode {
            path,
            kind: LayoutKind::Unknown,
            original_type_id: None,
            canonical_type_id: None,
            display_name: None,
            member_name: name,
            offset,
            size: None,
            alignment: None,
            bit_range: None,
            overlaps_storage: false,
            alternative_union_view: false,
            dimension: 0,
            unavailable_reason: Some(why),
            type_entry: None,
            ancestors: Vec::new(),
        }
    }
    fn member_extent(
        &self,
        parent: &LayoutNode<'a>,
        m: &Member,
        child: &LayoutNode<'a>,
    ) -> Result<MemberExtent> {
        let Some(width) = m.bit_size else {
            return Ok((child.size, None, None));
        };
        let Some(bit_offset) = m.data_bit_offset else {
            return Ok((
                child.size,
                None,
                Some("legacy or missing bit offset convention"),
            ));
        };
        let end = bit_offset.checked_add(width).ok_or(LayoutError::Overflow)?;
        let byte_start = bit_offset / 8;
        let byte_end = end.checked_add(7).ok_or(LayoutError::Overflow)? / 8;
        let start = parent
            .offset
            .checked_mul(8)
            .and_then(|v| v.checked_add(bit_offset))
            .ok_or(LayoutError::Overflow)?;
        let stop = start.checked_add(width).ok_or(LayoutError::Overflow)?;
        if child.offset
            != parent
                .offset
                .checked_add(byte_start)
                .ok_or(LayoutError::Overflow)?
        {
            return Ok((
                child.size,
                None,
                Some("bit offset conflicts with byte offset"),
            ));
        }
        Ok((Some(byte_end - byte_start), Some(start..stop), None))
    }
    /// Decode at most 16 bytes from the borrowed hex string, never a full
    /// copied snapshot. Values are qualified *representations*, not C lvalues.
    pub fn decode(&self, node: &LayoutNode<'a>) -> DecodeResult<'a> {
        let Some(ty) = node.type_entry else {
            return DecodeResult::unavailable(DecodeStatus::MissingType);
        };
        if node.unavailable_reason.is_some() {
            return DecodeResult::unavailable(
                if node.bit_range.is_none()
                    && node
                        .path
                        .last()
                        .is_some_and(|p| matches!(p, NodePath::Member(_)))
                {
                    DecodeStatus::AmbiguousBitOffset
                } else {
                    DecodeStatus::MissingType
                },
            );
        }
        if !matches!(
            node.kind,
            LayoutKind::Scalar | LayoutKind::Enum | LayoutKind::Pointer
        ) {
            return DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation);
        }
        let Some(target) = self.target else {
            return DecodeResult::unavailable(DecodeStatus::NoTargetByteOrder);
        };
        let Some(reg) = self.registry.view.store().type_registry() else {
            return DecodeResult::unavailable(DecodeStatus::MissingType);
        };
        let Some(unit) = reg.units.iter().find(|u| u.id == ty.unit_id) else {
            return DecodeResult::unavailable(DecodeStatus::MissingType);
        };
        if unit.address_size != target.address_bytes {
            return DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation);
        }
        let Some(size) = node.size else {
            return DecodeResult::unavailable(DecodeStatus::OutOfBounds);
        };
        if size == 0 || size > 16 {
            return DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation);
        }
        let Some(end) = node.offset.checked_add(size) else {
            return DecodeResult::unavailable(DecodeStatus::OutOfBounds);
        };
        if end > self.captured {
            return DecodeResult::unavailable(if node.offset < self.captured {
                DecodeStatus::PartialBytes
            } else {
                DecodeStatus::MissingBytes
            });
        }
        if !self.source_valid {
            return DecodeResult::unavailable(DecodeStatus::MissingBytes);
        }
        let Some(hex) = self.hex.and_then(|h| {
            usize::try_from(node.offset.checked_mul(2)?)
                .ok()
                .zip(usize::try_from(end.checked_mul(2)?).ok())
                .and_then(|(a, b)| h.get(a..b))
        }) else {
            return DecodeResult::unavailable(DecodeStatus::OutOfBounds);
        };
        let mut bytes = [0u8; 16];
        for (i, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let nib = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            };
            let Some(b) = nib(pair[0]).zip(nib(pair[1])).map(|(a, b)| a * 16 + b) else {
                return DecodeResult::unavailable(DecodeStatus::InvalidRepresentation);
            };
            bytes[i] = b;
        }
        if node.bit_range.is_some() && target.byte_order != ByteOrder::Little {
            return DecodeResult::unavailable(DecodeStatus::AmbiguousBitOffset);
        }
        let width = match usize::try_from(size).ok().and_then(|s| s.checked_mul(8)) {
            Some(w) => w,
            None => return DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation),
        };
        let mut number = 0u128;
        for (i, &byte) in bytes.iter().enumerate().take(size as usize) {
            let shift = if target.byte_order == ByteOrder::Little {
                i * 8
            } else {
                (size as usize - 1 - i) * 8
            };
            number |= (byte as u128) << shift;
        }
        let mut bits = width;
        if let Some(span) = &node.bit_range {
            let shift = (span.start % 8) as usize;
            let Ok(w) = usize::try_from(span.end - span.start) else {
                return DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation);
            };
            if w == 0 || w > 128 || w + shift > width {
                return DecodeResult::unavailable(DecodeStatus::AmbiguousBitOffset);
            }
            number >>= shift;
            if w < 128 {
                number &= (1u128 << w) - 1;
            }
            bits = w;
        }
        let signed = |n: u128, w: usize| -> i128 {
            if w == 128 {
                n as i128
            } else {
                let sign = 1u128 << (w - 1);
                if n & sign == 0 {
                    n as i128
                } else {
                    (n | (!0u128 << w)) as i128
                }
            }
        };
        let enc = ty.encoding;
        match node.kind {
            LayoutKind::Pointer if target.flat_pointer && size == target.address_bytes as u64 => {
                DecodeResult::pattern(TypedValue::Address(number))
            }
            LayoutKind::Pointer => {
                DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation)
            }
            LayoutKind::Enum => {
                let encoding = self
                    .resolve(ty.referenced_type.as_deref())
                    .0
                    .and_then(|t| t.encoding);
                let signed_enum =
                    matches!(encoding, Some(5 | 6)) || ty.enumerators.iter().any(|e| e.value < 0);
                let high_bit = number & (1u128 << (bits - 1)) != 0;
                if !signed_enum
                    && high_bit
                    && !matches!(encoding, Some(7 | 8))
                    && !ty
                        .enumerators
                        .iter()
                        .any(|e| e.value >= 0 && e.value as u128 == number)
                {
                    return DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation);
                }
                if !signed_enum && number > i128::MAX as u128 {
                    return DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation);
                }
                let value = if signed_enum {
                    signed(number, bits)
                } else {
                    number as i128
                };
                let mut candidates = ty.enumerators.iter().filter(|e| e.value == value);
                let first = candidates.next();
                let symbol = if candidates.next().is_none() {
                    first.map(|e| e.name.as_str())
                } else {
                    None
                };
                DecodeResult::pattern(TypedValue::Enum {
                    number: value,
                    symbol,
                })
            }
            LayoutKind::Scalar => match enc {
                Some(5 | 6) => DecodeResult::pattern(TypedValue::Signed(signed(number, bits))),
                Some(7) => DecodeResult::pattern(TypedValue::Unsigned(number)),
                Some(8) => {
                    if size == 1 && node.bit_range.is_none() {
                        DecodeResult::pattern(TypedValue::Character(number as u8))
                    } else {
                        DecodeResult::pattern(TypedValue::Unsigned(number))
                    }
                }
                Some(2) if number <= 1 => DecodeResult::pattern(TypedValue::Boolean(number != 0)),
                Some(2) => DecodeResult::unavailable(DecodeStatus::InvalidRepresentation),
                Some(4) if target.ieee_binary32_64 && node.bit_range.is_none() && size == 4 => {
                    let bits = number as u32;
                    // A signaling NaN may trap when consumed; do not present
                    // it as a safely interpreted C floating-point value.
                    if bits & 0x7f80_0000 == 0x7f80_0000
                        && bits & 0x007f_ffff != 0
                        && bits & 0x0040_0000 == 0
                    {
                        DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation)
                    } else {
                        DecodeResult::pattern(TypedValue::Float32(f32::from_bits(bits)))
                    }
                }
                Some(4) if target.ieee_binary32_64 && node.bit_range.is_none() && size == 8 => {
                    let bits = number as u64;
                    if bits & 0x7ff0_0000_0000_0000 == 0x7ff0_0000_0000_0000
                        && bits & 0x000f_ffff_ffff_ffff != 0
                        && bits & 0x0008_0000_0000_0000 == 0
                    {
                        DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation)
                    } else {
                        DecodeResult::pattern(TypedValue::Float64(f64::from_bits(bits)))
                    }
                }
                _ => DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation),
            },
            _ => DecodeResult::unavailable(DecodeStatus::UnsupportedRepresentation),
        }
    }
}

fn qualified_bytes(
    hex: Option<&str>,
    observation: Option<&MemoryEvidence>,
    status: Option<&DataValidity>,
) -> (bool, u64) {
    let Some(m) = observation else {
        return (false, 0);
    };
    if !matches!(m.status.as_str(), "full" | "partial" | "truncated")
        || m.captured_bytes > m.requested_bytes
        || status.is_some_and(|s| !matches!(s, DataValidity::Valid | DataValidity::Truncated))
    {
        return (false, 0);
    }
    let Some(hex) = hex else {
        return (false, 0);
    };
    let valid = m
        .captured_bytes
        .checked_mul(2)
        .is_some_and(|n| n == hex.len() as u64)
        && hex.bytes().all(|c| c.is_ascii_hexdigit());
    (valid, if valid { m.captured_bytes } else { 0 })
}

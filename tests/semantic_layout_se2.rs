use c_trace_engine::dwarf::{Dimension, Member, TypeEntry, TypeRegistry, UnitInfo};
use c_trace_engine::models::{
    DataValidity, DebugSessionTrace, ExecutionSnapshot, HeapBlockInfo, MemoryEvidence,
    SnapshotObservation, VariableInfo,
};
use c_trace_engine::semantic::{
    ByteOrder, DecodeStatus, LayoutEngine, LayoutError, LayoutKind, ObjectKind, ObjectRegistry,
    TargetFormat, TypedValue,
};
use c_trace_engine::trace_parser::{EvidenceConfidence as C, TraceStore};

fn ty(id: &str, kind: &str, bytes: Option<u64>, encoding: Option<u64>) -> TypeEntry {
    TypeEntry {
        id: id.into(),
        unit_id: "u".into(),
        die_offset: 1,
        kind: kind.into(),
        name: Some(id.into()),
        byte_size: bytes,
        encoding,
        referenced_type: None,
        members: vec![],
        dimensions: vec![],
        enumerators: vec![],
        declaration_only: false,
        runtime_dependent: false,
    }
}
fn member(name: &str, type_ref: &str, offset: Option<u64>) -> Member {
    Member {
        name: Some(name.into()),
        type_ref: Some(type_ref.into()),
        byte_offset: offset,
        implicit_union_zero: false,
        bit_size: None,
        data_bit_offset: None,
        legacy_bit_offset: None,
        byte_size: None,
    }
}
fn reg(types: Vec<TypeEntry>) -> TypeRegistry {
    TypeRegistry {
        units: vec![UnitInfo {
            id: "u".into(),
            name: Some("synthetic.c".into()),
            compilation_dir: None,
            dwarf_version: 5,
            address_size: 8,
        }],
        types,
        ..Default::default()
    }
}
fn variable(
    name: &str,
    type_id: Option<&str>,
    size: u64,
    hex: Option<&str>,
    captured: u64,
) -> VariableInfo {
    VariableInfo {
        name: name.into(),
        scope: Some("global".into()),
        identity: Some(format!("symbol:{name}")),
        type_ref: type_id.map(str::to_owned),
        size_bytes: Some(size),
        read_status: DataValidity::Valid,
        bytes_status: if captured == size {
            DataValidity::Valid
        } else {
            DataValidity::Truncated
        },
        raw_bytes_hex: hex.map(str::to_owned),
        memory_evidence: Some(MemoryEvidence {
            extent_bytes: Some(size),
            requested_bytes: size,
            captured_bytes: captured,
            status: if captured == size { "full" } else { "partial" }.into(),
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn trace(types: Vec<TypeEntry>, globals: Vec<VariableInfo>) -> DebugSessionTrace {
    DebugSessionTrace {
        type_registry: Some(reg(types)),
        snapshots: vec![ExecutionSnapshot {
            step_index: 0,
            file: "synthetic.c".into(),
            line: 1,
            func: "main".into(),
            observation: SnapshotObservation {
                stop_id: 1,
                pid: Some(42),
                complete: true,
                globals,
                ..Default::default()
            },
            ..Default::default()
        }],
        ..Default::default()
    }
}
fn target(order: ByteOrder) -> TargetFormat {
    TargetFormat {
        byte_order: order,
        address_bytes: 8,
        ieee_binary32_64: true,
        flat_pointer: true,
    }
}
fn registry(t: DebugSessionTrace) -> TraceStore {
    let store = TraceStore::from_trace(t).unwrap();
    assert!(
        store.validated_semantic().report().is_structurally_sound(),
        "{:?}",
        store.validated_semantic().report()
    );
    store
}
fn object<'a, 'v>(
    r: &'a ObjectRegistry<'v, '_>,
    name: &str,
) -> &'a c_trace_engine::semantic::RuntimeObject<'v> {
    r.objects().iter().find(|o| o.name == Some(name)).unwrap()
}

#[test]
fn integers_boolean_character_enum_float_pointer_and_target_order() {
    let mut signed = ty("s24", "base", Some(3), Some(5));
    signed.name = Some("signed three-byte".into());
    let unsigned = ty("u24", "base", Some(3), Some(7));
    let char_ty = ty("char", "base", Some(1), Some(8));
    let boolean = ty("bool", "base", Some(1), Some(2));
    let float = ty("float", "base", Some(4), Some(4));
    let double = ty("double", "base", Some(8), Some(4));
    let longdouble = ty("longdouble", "base", Some(16), Some(4));
    let ptr = ty("ptr", "pointer", Some(8), None);
    let mut enumeration = ty("enum", "enum", Some(1), None);
    enumeration.enumerators = vec![
        c_trace_engine::dwarf::Enumerator {
            name: "Minus".into(),
            value: -2,
        },
        c_trace_engine::dwarf::Enumerator {
            name: "Also".into(),
            value: -2,
        },
    ];
    let types = vec![
        signed,
        unsigned,
        char_ty,
        boolean,
        float,
        double,
        longdouble,
        ptr,
        enumeration,
    ];
    let names = [
        ("a", "s24", 3, "feffff"),
        ("u", "u24", 3, "010203"),
        ("c", "char", 1, "41"),
        ("b", "bool", 1, "01"),
        ("badbool", "bool", 1, "02"),
        ("e", "enum", 1, "fe"),
        ("f", "float", 4, "0000a03f"),
        ("d", "double", 8, "00000000000004c0"),
        (
            "longdouble",
            "longdouble",
            16,
            "00000000000000000000000000000000",
        ),
        ("pointer", "ptr", 8, "8877665544332211"),
    ];
    let globals = names
        .iter()
        .map(|(n, t, s, h)| variable(n, Some(t), *s, Some(h), *s))
        .collect();
    let store = registry(trace(types, globals));
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let absent = LayoutEngine::new(&r);
    let layout = absent.layout_for_object(&object(&r, "a").id, 0).unwrap();
    assert_eq!(
        layout.decode(&layout.root()).status,
        DecodeStatus::NoTargetByteOrder
    );
    let e = LayoutEngine::with_target(&r, target(ByteOrder::Little));
    let decode = |n: &str| {
        let l = e.layout_for_object(&object(&r, n).id, 0).unwrap();
        l.decode(&l.root())
    };
    assert_eq!(decode("a").value, Some(TypedValue::Signed(-2)));
    assert_eq!(decode("u").value, Some(TypedValue::Unsigned(0x030201)));
    assert_eq!(decode("c").value, Some(TypedValue::Character(b'A')));
    assert_eq!(decode("b").value, Some(TypedValue::Boolean(true)));
    assert_eq!(
        decode("badbool").status,
        DecodeStatus::InvalidRepresentation
    );
    assert_eq!(
        decode("e").value,
        Some(TypedValue::Enum {
            number: -2,
            symbol: None
        }),
        "duplicate enum spellings must stay ambiguous"
    );
    assert_eq!(decode("f").value, Some(TypedValue::Float32(1.25)));
    assert_eq!(decode("d").value, Some(TypedValue::Float64(-2.5)));
    assert_eq!(
        decode("longdouble").status,
        DecodeStatus::UnsupportedRepresentation
    );
    assert_eq!(
        decode("pointer").value,
        Some(TypedValue::Address(0x1122334455667788))
    );
    assert_eq!(
        decode("a").confidence,
        C::Candidate,
        "not an initialization oracle"
    );
    let be = LayoutEngine::with_target(&r, target(ByteOrder::Big));
    let l = be.layout_for_object(&object(&r, "u").id, 0).unwrap();
    assert_eq!(
        l.decode(&l.root()).value,
        Some(TypedValue::Unsigned(0x010203))
    );
}

#[test]
fn structs_padding_packed_nested_unions_bitfields_and_recursive_pointers() {
    let ch = ty("ch", "base", Some(1), Some(8));
    let int = ty("int", "base", Some(4), Some(5));
    let mut plain = ty("plain", "struct", Some(12), None);
    plain.members = vec![
        member("flag", "ch", Some(0)),
        member("count", "int", Some(4)),
        member("tail", "ch", Some(8)),
    ];
    let mut packed = ty("packed", "struct", Some(5), None);
    packed.members = vec![
        member("flag", "ch", Some(0)),
        member("count", "int", Some(1)),
    ];
    let mut overlay = ty("overlay", "union", Some(4), None);
    overlay.members = vec![
        member("word", "int", Some(0)),
        member("byte", "ch", Some(0)),
    ];
    let mut node = ty("node", "struct", Some(16), None);
    let mut next = ty("next", "pointer", Some(8), None);
    next.referenced_type = Some("node".into());
    node.members = vec![
        member("value", "int", Some(0)),
        member("next", "next", Some(8)),
    ];
    let mut outer = ty("outer", "struct", Some(28), None);
    outer.members = vec![
        member("first", "plain", Some(0)),
        member("last", "node", Some(12)),
    ];
    let mut bits = ty("bits", "struct", Some(4), None);
    bits.members = vec![
        member("a", "int", None),
        member("b", "int", None),
        member("c", "int", None),
    ];
    for (m, (start, width)) in bits.members.iter_mut().zip([(0, 3), (3, 5), (8, 9)]) {
        m.bit_size = Some(width);
        m.data_bit_offset = Some(start);
    }
    let mut legacy = ty("legacy", "struct", Some(4), None);
    legacy.members = vec![member("old", "int", Some(0))];
    legacy.members[0].bit_size = Some(3);
    legacy.members[0].legacy_bit_offset = Some(5);
    let types = vec![
        ch, int, plain, packed, overlay, node, next, outer, bits, legacy,
    ];
    let globals = vec![
        variable(
            "plain",
            Some("plain"),
            12,
            Some("0100000004030201fe000000"),
            12,
        ),
        variable("packed", Some("packed"), 5, Some("0240302010"), 5),
        variable("overlay", Some("overlay"), 4, Some("44332211"), 4),
        variable("outer", Some("outer"), 28, None, 0),
        variable("bits", Some("bits"), 4, Some("ed010000"), 4),
        variable("legacy", Some("legacy"), 4, Some("01000000"), 4),
    ];
    let store = registry(trace(types, globals));
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let engine = LayoutEngine::with_target(&r, target(ByteOrder::Little));
    let l = engine
        .layout_for_object(&object(&r, "plain").id, 0)
        .unwrap();
    let root = l.root();
    assert_eq!(
        (root.kind, root.size, root.alignment),
        (LayoutKind::Struct, Some(12), None)
    );
    let count = l.member(&root, "count").unwrap().unwrap();
    assert_eq!((count.offset, count.size), (4, Some(4)));
    assert_eq!(l.capture(&count).hex, Some("04030201"));
    assert_eq!(l.decode(&count).value, Some(TypedValue::Signed(0x01020304)));
    let pads: Vec<_> = l
        .children_page(&root, 0..l.children_count(&root).unwrap())
        .unwrap()
        .into_iter()
        .filter(|n| n.kind == LayoutKind::Padding)
        .map(|n| (n.offset, n.size.unwrap()))
        .collect();
    assert_eq!(pads, vec![(1, 3), (9, 3)]);
    let packed_l = engine
        .layout_for_object(&object(&r, "packed").id, 0)
        .unwrap();
    let packed_root = packed_l.root();
    assert_eq!(
        packed_l
            .member(&packed_root, "count")
            .unwrap()
            .unwrap()
            .offset,
        1
    );
    assert_eq!(
        packed_l.children_count(&packed_root).unwrap(),
        2,
        "no natural-alignment padding invented"
    );
    let u = engine
        .layout_for_object(&object(&r, "overlay").id, 0)
        .unwrap();
    let root = u.root();
    let word = u.member(&root, "word").unwrap().unwrap();
    let byte = u.member(&root, "byte").unwrap().unwrap();
    assert!(
        word.overlaps_storage
            && byte.overlaps_storage
            && word.alternative_union_view
            && byte.alternative_union_view
    );
    assert_eq!(u.decode(&word).value, Some(TypedValue::Signed(0x11223344)));
    assert_eq!(u.decode(&byte).value, Some(TypedValue::Character(0x44)));
    let outer = engine
        .layout_for_object(&object(&r, "outer").id, 0)
        .unwrap();
    let node = outer.member(&outer.root(), "last").unwrap().unwrap();
    let ptr = outer.member(&node, "next").unwrap().unwrap();
    assert_eq!(ptr.kind, LayoutKind::Pointer);
    assert_eq!(outer.children_count(&ptr).unwrap(), 0);
    assert_eq!(ptr.offset, 20);
    let bits = engine.layout_for_object(&object(&r, "bits").id, 0).unwrap();
    let root = bits.root();
    let a = bits.member(&root, "a").unwrap().unwrap();
    let b = bits.member(&root, "b").unwrap().unwrap();
    assert_eq!(a.bit_range, Some(0..3));
    assert_eq!(b.bit_range, Some(3..8));
    assert!(a.overlaps_storage && b.overlaps_storage);
    assert_eq!(
        bits.decode(&a).value,
        Some(TypedValue::Signed(-3)),
        "signed 3-bit 0b101"
    );
    assert_eq!(
        bits.decode(&b).value,
        Some(TypedValue::Signed(-3)),
        "signed 5-bit 0b11101"
    );
    let c = bits.member(&root, "c").unwrap().unwrap();
    assert_eq!(c.bit_range, Some(8..17));
    assert_eq!(bits.unclaimed_bits(&root).unwrap(), vec![17..32]);
    assert!(bits
        .children_page(&root, 0..bits.children_count(&root).unwrap())
        .unwrap()
        .iter()
        .all(|n| n.kind != LayoutKind::Padding));
    assert_eq!(
        bits.decode(&c).value,
        Some(TypedValue::Signed(1)),
        "bits 0x01ed => upper 9 bits 1"
    );
    let legacy = engine
        .layout_for_object(&object(&r, "legacy").id, 0)
        .unwrap();
    let old = legacy.member(&legacy.root(), "old").unwrap().unwrap();
    assert_eq!(old.bit_range, None);
    assert_eq!(legacy.decode(&old).status, DecodeStatus::AmbiguousBitOffset);
}

#[test]
fn pages_multidimensional_vla_unknown_flexible_overflow_cycles_and_partial_capture() {
    let int = ty("int", "base", Some(4), Some(5));
    let mut arr = ty("matrix", "array", Some(24), None);
    arr.referenced_type = Some("int".into());
    arr.dimensions = vec![
        Dimension {
            lower_bound: Some(0),
            upper_bound: Some(1),
            count: Some(2),
            status: "static".into(),
        },
        Dimension {
            lower_bound: Some(0),
            upper_bound: Some(2),
            count: Some(3),
            status: "static".into(),
        },
    ];
    let mut huge = ty("huge", "array", Some(4_000_000_000), None);
    huge.referenced_type = Some("int".into());
    huge.dimensions = vec![Dimension {
        lower_bound: Some(0),
        upper_bound: None,
        count: Some(1_000_000_000),
        status: "static".into(),
    }];
    let mut vla = ty("vla", "array", None, None);
    vla.referenced_type = Some("int".into());
    vla.dimensions = vec![Dimension {
        lower_bound: Some(0),
        upper_bound: None,
        count: None,
        status: "runtime_dependent".into(),
    }];
    let mut flexible = ty("flex", "struct", Some(4), None);
    flexible.members = vec![
        member("prefix", "int", Some(0)),
        member("tail", "vla", Some(4)),
    ];
    let mut cycle = ty("cycle", "typedef", None, None);
    cycle.referenced_type = Some("cycle".into());
    let mut overflowing = ty("overflow", "array", None, None);
    overflowing.referenced_type = Some("int".into());
    overflowing.dimensions = vec![Dimension {
        lower_bound: Some(0),
        upper_bound: None,
        count: Some(u64::MAX),
        status: "static".into(),
    }];
    let mut globals = vec![
        variable(
            "matrix",
            Some("matrix"),
            24,
            Some("010000000200000003000000040000000500000006000000"),
            24,
        ),
        variable("huge", Some("huge"), 4_000_000_000, None, 0),
        variable("vla", Some("vla"), 12, Some("140000001500000016000000"), 12),
        variable("flex", Some("flex"), 4, Some("03000000"), 4),
        variable("cycle", Some("cycle"), 4, None, 0),
        variable("missing", Some("does-not-exist"), 4, None, 0),
        variable("overflow", Some("overflow"), u64::MAX, None, 0),
        variable("partial", Some("int"), 4, Some("0102"), 2),
    ];
    globals[2].runtime_size_bytes = Some(12);
    let store = registry(trace(
        vec![int, arr, huge, vla, flexible, cycle, overflowing],
        globals,
    ));
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let e = LayoutEngine::with_target(&r, target(ByteOrder::Little));
    let l = e.layout_for_object(&object(&r, "matrix").id, 0).unwrap();
    let root = l.root();
    let row = l.array_element(&root, 1).unwrap();
    let element = l.array_element(&row, 2).unwrap();
    assert_eq!((row.offset, element.offset), (12, 20));
    assert_eq!(element.path.len(), 2);
    assert_eq!(l.decode(&element).value, Some(TypedValue::Signed(6)));
    assert_eq!(l.children_page(&row, 0..3).unwrap().len(), 3);
    let v = e.layout_for_object(&object(&r, "vla").id, 0).unwrap();
    assert_eq!(v.children_count(&v.root()).unwrap(), 3);
    assert_eq!(v.array_element(&v.root(), 2).unwrap().offset, 8);
    let flex = e.layout_for_object(&object(&r, "flex").id, 0).unwrap();
    let tail = flex.member(&flex.root(), "tail").unwrap().unwrap();
    assert!(matches!(
        flex.children_count(&tail),
        Err(LayoutError::UnknownExtent)
    ));
    let huge = e.layout_for_object(&object(&r, "huge").id, 0).unwrap();
    assert_eq!(huge.children_count(&huge.root()).unwrap(), 1_000_000_000);
    assert_eq!(huge.children_page(&huge.root(), 0..64).unwrap().len(), 64);
    assert!(matches!(
        huge.children_page(&huge.root(), 0..65),
        Err(LayoutError::PageLimit)
    ));
    assert_eq!(
        huge.array_element(&huge.root(), 999_999_999)
            .unwrap()
            .offset,
        3_999_999_996
    );
    let overflow = e.layout_for_object(&object(&r, "overflow").id, 0).unwrap();
    assert!(matches!(
        overflow.array_element(&overflow.root(), u64::MAX - 1),
        Err(LayoutError::Overflow)
    ));
    let missing = e.layout_for_object(&object(&r, "missing").id, 0).unwrap();
    assert_eq!(missing.root().original_type_id, Some("does-not-exist"));
    assert_eq!(missing.root().kind, LayoutKind::Opaque);
    let cyc = e.layout_for_object(&object(&r, "cycle").id, 0).unwrap();
    assert_eq!(cyc.root().kind, LayoutKind::Opaque);
    assert!(cyc.root().unavailable_reason.is_some());
    let partial = e.layout_for_object(&object(&r, "partial").id, 0).unwrap();
    assert_eq!(partial.captured_prefix(), 2);
    assert_eq!(partial.capture(&partial.root()).byte_range, Some(0..2));
    assert_eq!(
        partial.decode(&partial.root()).status,
        DecodeStatus::PartialBytes
    );
    assert_eq!(partial.memory_evidence().unwrap().status, "partial");
    let foreign = registry(trace(vec![], vec![]));
    let foreign_view = foreign.validated_semantic();
    let foreign_r = ObjectRegistry::build(&foreign_view).unwrap();
    let foreign_e = LayoutEngine::new(&foreign_r);
    assert!(matches!(
        foreign_e.layout_for_object(&object(&r, "matrix").id, 0),
        Err(LayoutError::Registry(_))
    ));
}

#[test]
fn historical_untyped_heap_uses_exact_checkpoint_bytes_not_final_state() {
    let mut t = trace(vec![], vec![]);
    t.snapshots[0].heap_blocks.push(HeapBlockInfo {
        allocation_id: 7,
        address: "0x1000".into(),
        size_bytes: 4,
        raw_bytes_hex: Some("abcd".into()),
        memory_evidence: Some(MemoryEvidence {
            requested_bytes: 4,
            captured_bytes: 2,
            status: "partial".into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    t.heap_state.push(HeapBlockInfo {
        allocation_id: 7,
        address: "0x1000".into(),
        size_bytes: 4,
        raw_bytes_hex: Some("ffffffff".into()),
        memory_evidence: Some(MemoryEvidence {
            requested_bytes: 4,
            captured_bytes: 4,
            status: "full".into(),
            ..Default::default()
        }),
        is_freed: true,
        ..Default::default()
    });
    let store = registry(t);
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let id = &r
        .objects()
        .iter()
        .find(|o| o.kind == ObjectKind::Heap)
        .unwrap()
        .id;
    let l = LayoutEngine::new(&r).layout_for_object(id, 0).unwrap();
    assert!(l.historical);
    assert_eq!(l.root().kind, LayoutKind::Opaque);
    assert_eq!(l.raw_hex(), Some("abcd"));
    assert_eq!(l.captured_prefix(), 2);
    assert_eq!(l.capture(&l.root()).hex, Some("abcd"));
    assert_eq!(l.decode(&l.root()).status, DecodeStatus::MissingType);
    assert!(std::ptr::eq(
        l.raw_hex().unwrap(),
        store.trace().snapshots[0].heap_blocks[0]
            .raw_bytes_hex
            .as_deref()
            .unwrap()
    ));
}

#[test]
fn exact_observation_foreign_trace_absence_unreadable_and_target_mismatch() {
    let mut t = trace(
        vec![ty("int", "base", Some(4), Some(5))],
        vec![variable("live", Some("int"), 4, Some("03000000"), 4)],
    );
    t.snapshots[0]
        .observation
        .globals
        .push(variable("unreadable", Some("int"), 4, None, 0));
    t.snapshots[0]
        .observation
        .globals
        .last_mut()
        .unwrap()
        .memory_evidence
        .as_mut()
        .unwrap()
        .status = "unreadable".into();
    let copy = t.clone();
    t.snapshots.push(ExecutionSnapshot {
        step_index: 1,
        file: "synthetic.c".into(),
        line: 2,
        func: "main".into(),
        observation: SnapshotObservation {
            stop_id: 2,
            pid: Some(42),
            complete: false,
            ..Default::default()
        },
        ..Default::default()
    });
    let store = registry(t);
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let e = LayoutEngine::with_target(&r, target(ByteOrder::Little));
    let id = &object(&r, "live").id;
    let obs = r.observation_at(id, 0).unwrap().next().unwrap();
    let l = e.layout_for_observation(&obs).unwrap();
    assert_eq!(l.decode(&l.root()).value, Some(TypedValue::Signed(3)));
    assert!(matches!(
        e.layout_for_object(id, 1),
        Err(LayoutError::Absent)
    ));
    let unreadable = e
        .layout_for_object(&object(&r, "unreadable").id, 0)
        .unwrap();
    assert_eq!(unreadable.captured_prefix(), 0);
    assert_eq!(unreadable.memory_evidence().unwrap().status, "unreadable");
    assert_eq!(unreadable.capture(&unreadable.root()).hex, None);
    assert_eq!(
        unreadable.decode(&unreadable.root()).status,
        DecodeStatus::MissingBytes
    );
    let mismatched = LayoutEngine::with_target(
        &r,
        TargetFormat {
            address_bytes: 4,
            ..target(ByteOrder::Little)
        },
    );
    let l = mismatched.layout_for_object(id, 0).unwrap();
    assert_eq!(
        l.decode(&l.root()).status,
        DecodeStatus::UnsupportedRepresentation
    );
    let foreign = registry(copy);
    let foreign_view = foreign.validated_semantic();
    let foreign_r = ObjectRegistry::build(&foreign_view).unwrap();
    let foreign_obs = foreign_r
        .observation_at(&object(&foreign_r, "live").id, 0)
        .unwrap()
        .next()
        .unwrap();
    assert!(matches!(
        e.layout_for_observation(&foreign_obs),
        Err(LayoutError::Registry(_))
    ));
}

#[test]
fn hostile_member_metadata_and_big_endian_bitfield_remain_bounded() {
    let int = ty("int", "base", Some(4), Some(5));
    let mut bad = ty("bad", "struct", Some(4), None);
    bad.members = vec![member("wrong", "int", Some(u64::MAX))];
    let mut uncertain = ty("uncertain", "struct", Some(8), None);
    uncertain.members = vec![
        member("unknown", "int", None),
        member("known", "int", Some(4)),
    ];
    let mut bits = ty("bits", "struct", Some(4), None);
    bits.members = vec![member("field", "int", None)];
    bits.members[0].bit_size = Some(3);
    bits.members[0].data_bit_offset = Some(0);
    let mut narrow = ty("narrow", "struct", Some(4), None);
    narrow.members = vec![member("only", "int", Some(0))];
    let mut fixed = ty("fixed", "array", Some(8), None);
    fixed.referenced_type = Some("int".into());
    fixed.dimensions = vec![Dimension {
        lower_bound: Some(0),
        upper_bound: Some(1),
        count: Some(2),
        status: "static".into(),
    }];
    let t = trace(
        vec![int, bad, uncertain, bits, narrow, fixed],
        vec![
            variable("bad", Some("bad"), 4, None, 0),
            variable("uncertain", Some("uncertain"), 8, None, 0),
            variable("bits", Some("bits"), 4, Some("05000000"), 4),
            variable("narrow", Some("narrow"), 8, None, 0),
            variable("fixed", Some("fixed"), 12, None, 0),
        ],
    );
    let store = registry(t);
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let e = LayoutEngine::with_target(&r, target(ByteOrder::Big));
    let bad = e.layout_for_object(&object(&r, "bad").id, 0).unwrap();
    assert!(matches!(
        bad.children_count(&bad.root()),
        Err(LayoutError::Overflow)
    ));
    let uncertain = e.layout_for_object(&object(&r, "uncertain").id, 0).unwrap();
    assert!(uncertain
        .children_page(&uncertain.root(), 0..3)
        .unwrap()
        .iter()
        .any(|n| n.kind == LayoutKind::Unknown));
    assert!(!uncertain
        .children_page(&uncertain.root(), 0..3)
        .unwrap()
        .iter()
        .any(|n| n.kind == LayoutKind::Padding));
    let bits = e.layout_for_object(&object(&r, "bits").id, 0).unwrap();
    let field = bits.member(&bits.root(), "field").unwrap().unwrap();
    assert_eq!(bits.decode(&field).status, DecodeStatus::AmbiguousBitOffset);
    let narrow = e.layout_for_object(&object(&r, "narrow").id, 0).unwrap();
    assert!(matches!(
        narrow.children_count(&narrow.root()),
        Err(LayoutError::Inconsistent(_))
    ));
    let fixed = e.layout_for_object(&object(&r, "fixed").id, 0).unwrap();
    assert!(matches!(
        fixed.children_count(&fixed.root()),
        Err(LayoutError::Inconsistent(_))
    ));
}

#[test]
fn qualifiers_keep_original_type_id_and_unsafe_float_patterns_stay_unavailable() {
    let mut base = ty("base", "base", Some(4), Some(5));
    base.name = Some("int".into());
    let mut alias = ty("named-alias", "typedef", None, None);
    alias.referenced_type = Some("base".into());
    let mut immutable = ty("qualified-const", "const", None, None);
    immutable.referenced_type = Some("named-alias".into());
    let mut volatile = ty("qualified-volatile", "volatile", None, None);
    volatile.referenced_type = Some("qualified-const".into());
    let float = ty("float", "base", Some(4), Some(4));
    let t = trace(
        vec![base, alias, immutable, volatile, float],
        vec![
            variable(
                "qualified",
                Some("qualified-volatile"),
                4,
                Some("03000000"),
                4,
            ),
            variable("snan", Some("float"), 4, Some("0100807f"), 4),
        ],
    );
    let store = registry(t);
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let e = LayoutEngine::with_target(&r, target(ByteOrder::Little));
    let l = e.layout_for_object(&object(&r, "qualified").id, 0).unwrap();
    let root = l.root();
    assert_eq!(root.original_type_id, Some("qualified-volatile"));
    assert_eq!(root.canonical_type_id, Some("base"));
    assert_eq!(root.display_name, Some("int"));
    assert_eq!(l.decode(&root).value, Some(TypedValue::Signed(3)));
    let l = e.layout_for_object(&object(&r, "snan").id, 0).unwrap();
    assert_eq!(
        l.decode(&l.root()).status,
        DecodeStatus::UnsupportedRepresentation
    );
}

#[test]
fn v21_missing_dwarf_and_same_address_heap_reuse_remain_opaque() {
    let legacy = TraceStore::open("examples/v21_normal.json", Default::default()).unwrap();
    let view = legacy.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let engine = LayoutEngine::new(&r);
    for o in r.objects().iter().take(4) {
        if let Some(obs) = r.observations(&o.id).unwrap().next() {
            let layout = engine
                .layout_for_object(&o.id, obs.record.checkpoint)
                .unwrap();
            assert!(
                layout.root().unavailable_reason.is_some() || layout.root().type_entry().is_some()
            );
        }
    }
    let mut t = trace(vec![], vec![]);
    t.snapshots[0].heap_blocks.push(HeapBlockInfo {
        allocation_id: 7,
        address: "0x1000".into(),
        size_bytes: 2,
        raw_bytes_hex: Some("aabb".into()),
        memory_evidence: Some(MemoryEvidence {
            requested_bytes: 2,
            captured_bytes: 2,
            status: "full".into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    t.snapshots.push(ExecutionSnapshot {
        step_index: 1,
        file: "synthetic.c".into(),
        line: 2,
        func: "main".into(),
        observation: SnapshotObservation {
            stop_id: 2,
            pid: Some(42),
            complete: true,
            ..Default::default()
        },
        heap_blocks: vec![HeapBlockInfo {
            allocation_id: 8,
            address: "0x1000".into(),
            size_bytes: 2,
            raw_bytes_hex: Some("ccdd".into()),
            memory_evidence: Some(MemoryEvidence {
                requested_bytes: 2,
                captured_bytes: 2,
                status: "full".into(),
                ..Default::default()
            }),
            ..Default::default()
        }],
        ..Default::default()
    });
    let store = registry(t);
    let view = store.validated_semantic();
    let r = ObjectRegistry::build(&view).unwrap();
    let engine = LayoutEngine::new(&r);
    let first = engine
        .layout_for_object(&r.heap_by_id(7).unwrap().id, 0)
        .unwrap();
    let second = engine
        .layout_for_object(&r.heap_by_id(8).unwrap().id, 1)
        .unwrap();
    assert_eq!(
        (first.raw_hex(), second.raw_hex()),
        (Some("aabb"), Some("ccdd"))
    );
    assert!(matches!(
        engine.layout_for_object(&r.heap_by_id(7).unwrap().id, 1),
        Err(LayoutError::Absent)
    ));
}

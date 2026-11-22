//! Compiler, native C sizeof/offsetof oracle, and fresh Engine→Parser→SE-1→SE-2.
use c_trace_engine::semantic::{
    ByteOrder, LayoutEngine, LayoutKind, ObjectRegistry, TargetFormat, TypedValue,
};
use c_trace_engine::trace_parser::TraceStore;
use c_trace_engine::{trace::trace_program, TraceConfig};
use std::process::Command;

fn native_info(output: &str, key: &str) -> Vec<u64> {
    output
        .split_whitespace()
        .find_map(|w| w.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("missing {key}: {output}"))
        .split(',')
        .map(|n| n.parse().unwrap())
        .collect()
}

#[tokio::test]
async fn gcc_derived_offsets_bytes_values_and_bitfields_match_native_c() {
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("SE-2 live fixture skipped: {tool} unavailable");
            return;
        }
    }
    let source = format!(
        "{}/fixtures/c/semantic_se2_layout.c",
        env!("CARGO_MANIFEST_DIR")
    );
    let bin = std::env::temp_dir().join(format!("cte-se2-oracle-{}", std::process::id()));
    assert!(Command::new("gcc")
        .args(["-O0", "-g", "-gdwarf-5", "-o"])
        .arg(&bin)
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let target = TargetFormat::inspect_elf(&bin).unwrap();
    let output = Command::new(&bin).output().unwrap();
    assert!(output.status.success());
    let printed = String::from_utf8(output.stdout).unwrap();
    let plain = native_info(&printed, "plain");
    let packed = native_info(&printed, "packed");
    let bits = native_info(&printed, "bits");
    let node = native_info(&printed, "node");
    let nested = native_info(&printed, "nested");
    let matrix = native_info(&printed, "matrix");
    let rows = native_info(&printed, "rows");
    let mut config = TraceConfig::default();
    config.build.source_file = source.into();
    config.limits.max_steps = 240;
    config.limits.timeout_seconds = 70;
    config.sanitizer_trigger = c_trace_engine::config::SanitizerTrigger::Never;
    let store = TraceStore::from_trace(trace_program(config).await.unwrap()).unwrap();
    let view = store.validated_semantic();
    assert!(view.report().is_structurally_sound(), "{:?}", view.report());
    let r = ObjectRegistry::build(&view).unwrap();
    let e = LayoutEngine::with_target(&r, target);
    let observed = |name: &str| {
        let (index, _) = store
            .trace()
            .snapshots
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| {
                s.variables.iter().any(|v| {
                    v.name == name
                        && v.type_ref.is_some()
                        && v.memory_evidence
                            .as_ref()
                            .is_some_and(|m| m.status == "full" && m.captured_bytes > 0)
                })
            })
            .unwrap_or_else(|| panic!("no full typed {name}"));
        let object = r
            .objects_at_checkpoint(index)
            .unwrap()
            .find(|o| o.name == Some(name))
            .unwrap();
        (object.id.clone(), index)
    };
    let (id, at) = observed("sample");
    let l = e.layout_for_object(&id, at).unwrap();
    let root = l.root();
    assert_eq!(root.kind, LayoutKind::Struct);
    assert_eq!(root.size, Some(plain[0]));
    assert_eq!(plain[0], plain[3]);
    let count = l.member(&root, "count").unwrap().unwrap();
    let tail = l.member(&root, "tail").unwrap().unwrap();
    assert_eq!((count.offset, tail.offset), (plain[1], plain[2]));
    assert_eq!(l.decode(&count).value, Some(TypedValue::Signed(0x01020304)));
    assert_eq!(l.decode(&tail).value, Some(TypedValue::Signed(-2)));
    assert_eq!(
        l.capture(&count).byte_range,
        Some(count.offset..count.offset + 4)
    );
    let (id, at) = observed("packed");
    let l = e.layout_for_object(&id, at).unwrap();
    let count = l.member(&l.root(), "count").unwrap().unwrap();
    assert_eq!(l.root().size, Some(packed[0]));
    assert_eq!(packed[0], packed[2]);
    assert_eq!(count.offset, packed[1]);
    assert_eq!(l.decode(&count).value, Some(TypedValue::Signed(0x10203040)));
    let (id, at) = observed("overlay");
    let l = e.layout_for_object(&id, at).unwrap();
    let u = l.root();
    let word = l.member(&u, "word").unwrap().unwrap();
    let bytes = l.member(&u, "bytes").unwrap().unwrap();
    assert!(word.alternative_union_view && bytes.alternative_union_view && word.overlaps_storage);
    assert_eq!(
        l.decode(&word).value,
        Some(TypedValue::Unsigned(0x11223344))
    );
    let b = l.array_element(&bytes, 0).unwrap();
    assert_eq!(
        l.decode(&b).value,
        Some(TypedValue::Character(
            if target.byte_order == ByteOrder::Little {
                0x44
            } else {
                0x11
            }
        ))
    );
    let (id, at) = observed("bits");
    let l = e.layout_for_object(&id, at).unwrap();
    let root = l.root();
    assert_eq!(root.size, Some(bits[0]));
    for (name, value) in [
        ("a", TypedValue::Unsigned(5)),
        ("b", TypedValue::Signed(-3)),
        ("c", TypedValue::Unsigned(257)),
    ] {
        let field = l.member(&root, name).unwrap().unwrap();
        assert!(
            field.bit_range.is_some(),
            "GCC DW_AT_data_bit_offset for {name}"
        );
        assert_eq!(
            l.decode(&field).value,
            Some(value),
            "{name}: {:?}",
            field.bit_range
        );
    }
    let (id, at) = observed("node");
    let l = e.layout_for_object(&id, at).unwrap();
    let next = l.member(&l.root(), "next").unwrap().unwrap();
    assert_eq!((l.root().size, next.offset), (Some(node[0]), node[1]));
    assert_eq!(next.kind, LayoutKind::Pointer);
    assert_eq!(l.children_count(&next).unwrap(), 0);
    let addr = store.trace().snapshots[at]
        .variables
        .iter()
        .find(|v| v.name == "node")
        .unwrap()
        .address
        .as_ref()
        .unwrap();
    let numerical_address = u128::from_str_radix(addr.trim_start_matches("0x"), 16).unwrap();
    assert_eq!(
        l.decode(&next).value,
        Some(TypedValue::Address(numerical_address)),
        "numeric bytes only, NOT pointer provenance"
    );
    let (id, at) = observed("nested");
    let nested_layout = e.layout_for_object(&id, at).unwrap();
    let nested_root = nested_layout.root();
    let inner = nested_layout.member(&nested_root, "node").unwrap().unwrap();
    let part = nested_layout.member(&nested_root, "part").unwrap().unwrap();
    assert_eq!(
        (nested_root.size, inner.offset),
        (Some(nested[0]), nested[1])
    );
    let count = nested_layout.member(&part, "count").unwrap().unwrap();
    assert_eq!(count.offset, plain[1]);
    assert_eq!(
        nested_layout.decode(&count).value,
        Some(TypedValue::Signed(27))
    );
    let value = nested_layout.member(&inner, "value").unwrap().unwrap();
    assert_eq!(
        nested_layout.decode(&value).value,
        Some(TypedValue::Signed(17))
    );
    let (id, at) = observed("matrix");
    let l = e.layout_for_object(&id, at).unwrap();
    let row = l.array_element(&l.root(), 1).unwrap();
    let elem = l.array_element(&row, 2).unwrap();
    assert_eq!(l.root().size, Some(matrix[0]));
    assert_eq!(elem.offset, 20);
    assert_eq!(l.decode(&elem).value, Some(TypedValue::Signed(6)));
    let (id, at) = observed("rows");
    let l = e.layout_for_object(&id, at).unwrap();
    assert_eq!(l.root().size, Some(rows[0]));
    let second = l.array_element(&l.root(), 1).unwrap();
    let count = l.member(&second, "count").unwrap().unwrap();
    assert_eq!(count.offset, plain[0] + plain[1]);
    assert_eq!(l.decode(&count).value, Some(TypedValue::Signed(9)));
    for (name, expected) in [
        ("flag", TypedValue::Boolean(true)),
        (
            "shade",
            TypedValue::Enum {
                number: -2,
                symbol: Some("DARK"),
            },
        ),
        ("f", TypedValue::Float32(1.25)),
        ("d", TypedValue::Float64(-2.5)),
    ] {
        let (id, at) = observed(name);
        let l = e.layout_for_object(&id, at).unwrap();
        assert_eq!(l.decode(&l.root()).value, Some(expected), "{name}");
    }
    let (id, at) = observed("letters");
    let l = e.layout_for_object(&id, at).unwrap();
    assert_eq!(l.root().kind, LayoutKind::Array);
    assert_eq!(l.children_count(&l.root()).unwrap(), 4);
    assert_eq!(
        l.decode(&l.array_element(&l.root(), 0).unwrap()).value,
        Some(TypedValue::Signed(65))
    );
    let (id, at) = observed("pointers");
    let l = e.layout_for_object(&id, at).unwrap();
    let pointer = l.array_element(&l.root(), 1).unwrap();
    assert_eq!(pointer.kind, LayoutKind::Pointer);
    assert_eq!(l.children_count(&pointer).unwrap(), 0);
    assert!(matches!(l.decode(&pointer).value,Some(TypedValue::Address(n)) if n!=0));
    if let Some((index, v)) = store
        .trace()
        .snapshots
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, s)| {
            s.variables
                .iter()
                .find(|v| {
                    v.name == "dynamic" && v.type_ref.is_some() && v.runtime_size_bytes.is_some()
                })
                .map(|v| (i, v))
        })
    {
        let obj = r
            .objects_at_checkpoint(index)
            .unwrap()
            .find(|o| o.name == Some("dynamic"))
            .unwrap();
        let l = e.layout_for_object(&obj.id, index).unwrap();
        assert_eq!(l.extent, v.runtime_size_bytes);
        if let Ok(count) = l.children_count(&l.root()) {
            assert_eq!(
                count, 3,
                "runtime GDB sizeof is the sole VLA extent authority"
            );
            assert_eq!(
                l.decode(&l.array_element(&l.root(), 2).unwrap()).value,
                Some(TypedValue::Signed(22))
            );
        }
    }
    std::fs::remove_file(bin).unwrap();
}

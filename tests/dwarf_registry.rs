//! Phase 3: real GCC-produced DWARF v5, not hand-crafted DIE JSON.
use c_trace_engine::models::{DebugSessionTrace, ExecutionSnapshot, VariableInfo};
use c_trace_engine::{dwarf, trace::trace_program, TraceConfig};
use std::{collections::HashSet, path::PathBuf, process::Command};

fn compile(sources: &[&str], debug: bool) -> PathBuf {
    compile_with(
        sources,
        if debug {
            &["-O0", "-gdwarf-5"]
        } else {
            &["-O0", "-g0"]
        },
    )
}

fn compile_with(sources: &[&str], flags: &[&str]) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "c-trace-p3-{}-{}",
        std::process::id(),
        sources[0].replace(['.', '/'], "_")
    ));
    let mut cc = Command::new("gcc");
    cc.args(flags).arg("-o").arg(&path);
    for source in sources {
        cc.arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/c")
                .join(source),
        );
    }
    assert!(cc.status().unwrap().success());
    path
}

#[test]
fn layouts_dimensions_enums_and_runtime_bounds_are_real_gcc_facts() {
    let path = compile(&["dwarf_types.c"], true);
    let (registry, metrics) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(registry.provider, "gcc_dwarf");
    assert_eq!(registry.units.len(), 1);
    assert!(metrics.extraction_us > 0);
    let types = &registry.types;
    for kind in [
        "base", "pointer", "typedef", "const", "volatile", "restrict",
    ] {
        assert!(types.iter().any(|t| t.kind == kind), "missing {kind}");
    }
    assert!(types
        .iter()
        .any(|t| t.encoding == Some(2) && t.byte_size == Some(1)));
    let node = types
        .iter()
        .find(|t| t.name.as_deref() == Some("Node"))
        .unwrap();
    assert_eq!(node.kind, "struct");
    let next = node
        .members
        .iter()
        .find(|m| m.name.as_deref() == Some("next"))
        .unwrap();
    if cfg!(target_arch = "x86_64") {
        assert_eq!(node.byte_size, Some(24));
        assert_eq!(next.byte_offset, Some(8));
    }
    assert_eq!(
        next.type_ref
            .as_deref()
            .and_then(|id| types.iter().find(|t| t.id == id))
            .map(|t| t.kind.as_str()),
        Some("pointer")
    );
    let next_pointer = types
        .iter()
        .find(|t| Some(t.id.as_str()) == next.type_ref.as_deref())
        .unwrap();
    assert_eq!(
        next_pointer.referenced_type.as_deref(),
        Some(node.id.as_str())
    );
    assert_eq!(
        node.members
            .iter()
            .find(|m| m.name.as_deref() == Some("bits"))
            .unwrap()
            .bit_size,
        Some(3)
    );
    let more = node
        .members
        .iter()
        .find(|m| m.name.as_deref() == Some("more"))
        .unwrap();
    assert_eq!(more.bit_size, Some(5));
    if cfg!(target_arch = "x86_64") {
        assert_eq!(more.data_bit_offset, Some(131));
    }
    let color = types
        .iter()
        .find(|t| t.name.as_deref() == Some("Color"))
        .unwrap();
    assert_eq!(color.byte_size, Some(4));
    assert_eq!(
        color
            .enumerators
            .iter()
            .map(|e| (e.name.as_str(), e.value))
            .collect::<Vec<_>>(),
        [("RED", -2), ("GREEN", 4), ("BLUE", 4)]
    );
    let union = types
        .iter()
        .find(|t| t.name.as_deref() == Some("Overlay"))
        .unwrap();
    assert_eq!(union.byte_size, Some(4));
    assert!(union
        .members
        .iter()
        .all(|m| m.byte_offset == Some(0) && m.implicit_union_zero));
    let grid = types
        .iter()
        .find(|t| {
            t.kind == "array"
                && t.dimensions.iter().map(|d| d.count).collect::<Vec<_>>() == [Some(2), Some(3)]
        })
        .unwrap();
    assert_eq!(grid.dimensions.len(), 2);
    assert!(types.iter().any(|t| t.kind == "array"
        && t.dimensions
            .iter()
            .any(|d| d.status == "runtime_dependent" && d.count.is_none())));
    assert!(types.iter().any(|t| t.kind == "array"
        && t.dimensions
            .iter()
            .any(|d| d.status == "unavailable" && d.count.is_none())));
    let var = registry
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("global_array"))
        .unwrap();
    assert_eq!(var.line, Some(19));
    assert_eq!(var.column, Some(12));
    assert!(var
        .file
        .as_ref()
        .unwrap()
        .ends_with("fixtures/c/dwarf_types.c"));
}

#[test]
fn shadowed_declarations_keep_distinct_lexical_parents() {
    let path = compile(&["shadowed.c"], true);
    let (reg, _) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let vars: Vec<_> = reg
        .declarations
        .iter()
        .filter(|d| d.name.as_deref() == Some("value"))
        .collect();
    let scope_ids: HashSet<_> = vars.iter().map(|d| d.scope_id.as_str()).collect();
    assert!(
        scope_ids.len() >= 2,
        "shadowed declarations must not be merged: {vars:?}"
    );
    assert!(vars.iter().all(|d| d.type_ref.is_some()));
}

#[test]
fn distinct_translation_units_and_missing_debug_info() {
    let path = compile(&["two_statics_main.c", "two_statics_other.c"], true);
    let (reg, _) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(reg.units.len(), 2);
    let names: HashSet<_> = reg
        .declarations
        .iter()
        .filter(|d| d.name.as_deref() == Some("shared"))
        .map(|d| (&d.unit_id, &d.file, &d.id))
        .collect();
    assert_eq!(names.len(), 2);
    let nodes: Vec<_> = reg
        .types
        .iter()
        .filter(|t| t.name.as_deref() == Some("Node") && t.kind == "struct")
        .collect();
    assert_eq!(nodes.len(), 2);
    assert_ne!(nodes[0].id, nodes[1].id);
    if cfg!(target_arch = "x86_64") {
        assert_ne!(nodes[0].byte_size, nodes[1].byte_size);
    }
    let floating_node = nodes
        .iter()
        .find(|n| {
            n.members[0].name.as_deref() == Some("value")
                && n.members[0]
                    .type_ref
                    .as_deref()
                    .and_then(|id| reg.types.iter().find(|t| t.id == id))
                    .is_some_and(|t| t.encoding == Some(4))
        })
        .unwrap();
    let member_type = floating_node.members[0].type_ref.as_deref().unwrap();
    assert_eq!(
        reg.types
            .iter()
            .find(|t| t.id == member_type)
            .unwrap()
            .encoding,
        Some(4)
    );
    let no_debug = compile(&["two_statics_main.c", "two_statics_other.c"], false);
    assert!(dwarf::extract(&no_debug).is_err());
    std::fs::remove_file(no_debug).unwrap();
}

#[test]
fn arrays_of_pointers_and_structures_are_not_confused_with_pointer_objects() {
    let path = compile(&["dwarf_arrays.c"], true);
    let (reg, _) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let types: std::collections::HashMap<_, _> =
        reg.types.iter().map(|t| (t.id.as_str(), t)).collect();
    let declared = |name: &str| {
        reg.declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .and_then(|d| d.type_ref.as_deref())
            .and_then(|id| types.get(id).copied())
            .unwrap()
    };
    for (name, element_kind, dimensions) in [
        ("pointers", "pointer", vec![Some(3)]),
        ("objects", "struct", vec![Some(2)]),
        ("matrix", "base", vec![Some(2), Some(3)]),
    ] {
        let array = declared(name);
        assert_eq!(array.kind, "array");
        assert_eq!(
            array.dimensions.iter().map(|d| d.count).collect::<Vec<_>>(),
            dimensions
        );
        assert_eq!(
            types
                .get(array.referenced_type.as_deref().unwrap())
                .unwrap()
                .kind,
            element_kind
        );
    }
    let vla = declared("dynamic");
    assert!(vla
        .dimensions
        .iter()
        .any(|d| d.status == "runtime_dependent" && d.count.is_none()));
}

#[test]
fn invalid_input_and_bound_are_failures_not_empty_success() {
    let path = std::env::temp_dir().join(format!("c-trace-p3-invalid-{}", std::process::id()));
    std::fs::write(&path, b"not an executable").unwrap();
    assert!(dwarf::extract(&path).is_err());
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(dwarf::extract(&path).unwrap_err().contains("64 MiB"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cross_cu_abstract_origin_is_resolved_without_guessing() {
    let path = compile_with(
        &["phase7_lto_a.c", "phase7_lto_b.c", "phase7_lto_main.c"],
        &["-O2", "-gdwarf-5", "-flto"],
    );
    let (reg, _) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        reg.units.len() >= 3,
        "LTO fixture should retain per-source units: {:?}",
        reg.units
    );
    let cross: Vec<_> = reg
        .declarations
        .iter()
        .filter(|declaration| {
            declaration
                .resolved_origin
                .as_deref()
                .is_some_and(|origin| origin.contains("cross_cu"))
        })
        .collect();
    assert!(
        !cross.is_empty(),
        "expected a real cross-CU abstract-origin link: {:?}",
        reg.declarations
    );
    for declaration in &cross {
        // A resolved cross-CU DIE must have completed concrete attributes; the
        // engine never infers them from the referencing DIE alone.
        assert!(declaration.name.is_some(), "{declaration:?}");
        assert!(declaration.file.is_some(), "{declaration:?}");
        assert!(declaration.line.is_some(), "{declaration:?}");
    }
    assert!(reg
        .declarations
        .iter()
        .any(|declaration| declaration.kind.contains("subprogram")));
}

#[test]
fn ambiguous_or_missing_source_evidence_never_gets_a_type_guess() {
    let mut registry = dwarf::TypeRegistry::default();
    for i in 0..2 {
        registry.types.push(dwarf::TypeEntry {
            id: format!("debug:u{i}:o2"),
            unit_id: format!("debug:u{i}"),
            die_offset: 2,
            kind: "base".into(),
            name: Some("int".into()),
            byte_size: Some(4),
            encoding: None,
            referenced_type: None,
            members: Vec::new(),
            dimensions: Vec::new(),
            enumerators: Vec::new(),
            declaration_only: false,
            runtime_dependent: false,
        });
        registry.declarations.push(dwarf::Declaration {
            id: format!("debug:u{i}:o1"),
            unit_id: format!("debug:u{i}"),
            kind: "variable".into(),
            name: Some("same".into()),
            file: Some("/tmp/same.c".into()),
            line: Some(8),
            column: Some(3),
            function: Some("main".into()),
            scope_id: format!("debug:u{i}:o0"),
            type_ref: Some(format!("debug:u{i}:o2")),
            external: None,
            declaration_only: false,
            resolved_origin: None,
        });
    }
    let vars = vec![
        VariableInfo {
            name: "same".into(),
            declaration_file: Some("/tmp/same.c".into()),
            declaration_line: Some(8),
            value: "7".into(),
            ..Default::default()
        },
        VariableInfo {
            name: "same".into(),
            declaration_line: Some(8),
            value: "8".into(),
            ..Default::default()
        },
    ];
    let mut trace = DebugSessionTrace {
        type_registry: Some(registry),
        snapshots: vec![ExecutionSnapshot {
            func: "main".into(),
            variables: vars,
            ..Default::default()
        }],
        ..Default::default()
    };
    dwarf::associate(&mut trace);
    assert!(trace.snapshots[0]
        .variables
        .iter()
        .all(|v| v.type_ref.is_none() && v.declaration_ref.is_none()));
    assert_eq!(trace.snapshots[0].variables[0].value, "7");
}

#[test]
fn qualified_symbol_requires_full_file_identity() {
    let mut registry = dwarf::TypeRegistry::default();
    registry.types.push(dwarf::TypeEntry {
        id: "type:wrong".into(),
        unit_id: "unit:b".into(),
        die_offset: 2,
        kind: "base".into(),
        name: Some("int".into()),
        byte_size: Some(4),
        encoding: None,
        referenced_type: None,
        members: vec![],
        dimensions: vec![],
        enumerators: vec![],
        declaration_only: false,
        runtime_dependent: false,
    });
    registry.declarations.push(dwarf::Declaration {
        id: "decl:wrong".into(),
        unit_id: "unit:b".into(),
        kind: "variable".into(),
        name: Some("value".into()),
        file: Some("/b/common.c".into()),
        line: Some(4),
        column: None,
        function: None,
        scope_id: "unit:b".into(),
        type_ref: Some("type:wrong".into()),
        external: Some(false),
        declaration_only: false,
        resolved_origin: None,
    });
    let lookup = dwarf::ObservationTypes::new(&registry);
    for qualifier in ["/a/common.c", "/a/./common.c", "../a/common.c", "common.c"] {
        let mut var = VariableInfo {
            name: format!("'{qualifier}'::value"),
            address: Some("0x1000".into()),
            ..Default::default()
        };
        lookup.enrich(&mut var, "main");
        assert!(var.type_ref.is_none() && var.declaration_ref.is_none());
        assert!(var.size_bytes.is_none());
    }
    let mut correct = VariableInfo {
        name: "'/b/./common.c'::value".into(),
        address: Some("0x1000".into()),
        ..Default::default()
    };
    lookup.enrich(&mut correct, "main");
    assert_eq!(correct.declaration_ref.as_deref(), Some("decl:wrong"));
    assert_eq!(correct.size_bytes, Some(4));

    registry.declarations[0].file = Some("common.c".into());
    let mut relative = VariableInfo {
        name: "'common.c'::value".into(),
        ..Default::default()
    };
    dwarf::ObservationTypes::new(&registry).enrich(&mut relative, "main");
    assert!(relative.type_ref.is_none() && relative.declaration_ref.is_none());
}

#[test]
fn gcc_same_basename_statics_require_their_own_translation_unit() {
    let path = compile(
        &[
            "phase8r1_a/common.c",
            "phase8r1_b/common.c",
            "phase8r1_main.c",
        ],
        true,
    );
    let (registry, _) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let declarations: Vec<_> = registry
        .declarations
        .iter()
        .filter(|d| d.name.as_deref() == Some("value"))
        .collect();
    assert_eq!(declarations.len(), 2, "{declarations:?}");
    let lookup = dwarf::ObservationTypes::new(&registry);
    for declaration in &declarations {
        let file = declaration.file.as_ref().unwrap();
        assert!(file.starts_with('/'), "{file}");
        let mut observed = VariableInfo {
            name: format!("'{file}'::value"),
            address: Some("0x1000".into()),
            ..Default::default()
        };
        lookup.enrich(&mut observed, "main");
        assert_eq!(
            observed.declaration_ref.as_deref(),
            Some(declaration.id.as_str())
        );
        let mut incomplete = VariableInfo {
            name: "'common.c'::value".into(),
            ..Default::default()
        };
        lookup.enrich(&mut incomplete, "main");
        assert!(incomplete.declaration_ref.is_none());
    }
}

#[test]
fn ambiguous_location_does_not_fall_back_to_a_symbol() {
    let mut registry = dwarf::TypeRegistry::default();
    registry.types.push(dwarf::TypeEntry {
        id: "type:int".into(),
        unit_id: "unit".into(),
        die_offset: 2,
        kind: "base".into(),
        name: Some("int".into()),
        byte_size: Some(4),
        encoding: None,
        referenced_type: None,
        members: vec![],
        dimensions: vec![],
        enumerators: vec![],
        declaration_only: false,
        runtime_dependent: false,
    });
    for (id, name) in [
        ("ambiguous:1", "scope::value"),
        ("ambiguous:2", "scope::value"),
        ("fallback", "value"),
    ] {
        registry.declarations.push(dwarf::Declaration {
            id: id.into(),
            unit_id: "unit".into(),
            kind: "variable".into(),
            name: Some(name.into()),
            file: Some("/a/common.c".into()),
            line: Some(8),
            column: None,
            function: (id == "fallback").then(|| "scope".into()),
            scope_id: "unit".into(),
            type_ref: Some("type:int".into()),
            external: Some(false),
            declaration_only: false,
            resolved_origin: None,
        });
    }
    let mut var = VariableInfo {
        name: "scope::value".into(),
        declaration_file: Some("/a/common.c".into()),
        declaration_line: Some(8),
        ..Default::default()
    };
    dwarf::ObservationTypes::new(&registry).enrich(&mut var, "scope");
    assert!(var.type_ref.is_none() && var.declaration_ref.is_none());
}

#[tokio::test]
async fn live_trace_correlates_only_when_declaration_is_unique() {
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            return;
        }
    }
    let mut cfg = TraceConfig::default();
    cfg.build.source_file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/c/dwarf_types.c");
    cfg.limits.timeout_seconds = 45;
    let trace = trace_program(cfg).await.unwrap();
    assert!(trace.compiled_successfully);
    let registry = trace.type_registry.as_ref().unwrap();
    let types: HashSet<_> = registry.types.iter().map(|t| t.id.as_str()).collect();
    let decls: HashSet<_> = registry
        .declarations
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    let linked: Vec<_> = trace
        .snapshots
        .iter()
        .flat_map(|s| &s.variables)
        .filter(|v| v.type_ref.is_some())
        .collect();
    assert!(!linked.is_empty());
    assert!(linked
        .iter()
        .all(|v| types.contains(v.type_ref.as_deref().unwrap())
            && decls.contains(v.declaration_ref.as_deref().unwrap())));
    assert!(linked.iter().any(|v| v.name == "c" && v.value == "3"));
    c_trace_engine::processor::validate(&trace).unwrap();
    let store = c_trace_engine::trace_parser::TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
    assert!(store.raw().trace().type_registry.is_some());
    assert!(store.semantic_input().raw().trace().type_registry.is_some());
}

#[tokio::test]
async fn missing_debug_sections_do_not_suppress_the_existing_trace() {
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            return;
        }
    }
    let mut cfg = TraceConfig::default();
    cfg.build.source_file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/c/same_line.c");
    cfg.build.compiler_flags.push("-g0".into());
    cfg.limits.timeout_seconds = 30;
    let trace = trace_program(cfg).await.unwrap();
    assert!(trace.compiled_successfully);
    assert!(trace.type_registry.is_none());
    assert!(trace
        .analysis_notes
        .iter()
        .any(|s| s.contains("DWARF metadata unavailable")));
    assert!(trace.recording.profiling.dwarf_extraction_time_us.is_none());
    c_trace_engine::processor::validate(&trace).unwrap();
}

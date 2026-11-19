//! V2.2 Phase 7 evidence fusion: cross-provider links, bounded ambiguity and
//! runtime-fidelity preservation.
use std::path::PathBuf;

use c_trace_engine::clang_ast::{
    self, DwarfMemberRef, HeapCallAmbiguity, HeapCallCandidate, SourceDeclaration,
    SourceExpression, SourceMetadataRegistry, SourceSpan,
};
use c_trace_engine::config::{SanitizerTrigger, TraceMode};
use c_trace_engine::dwarf;
use c_trace_engine::evidence;
use c_trace_engine::models::{
    DebugSessionTrace, ExecutionSnapshot, HeapEvent, HeapEventAttribution, HeapEventType,
    VariableInfo,
};
use c_trace_engine::trace::trace_program;
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};
use c_trace_engine::TraceConfig;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/c")
        .join(name)
}

async fn tools_available() -> bool {
    for tool in ["clang", "gcc", "gdb", "addr2line"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return false;
        }
    }
    true
}

fn compile(sources: &[&str], flags: &[&str]) -> PathBuf {
    let stem = sources[0].replace('.', "_");
    let path = std::env::temp_dir().join(format!("c-trace-p7-{}-{stem}", std::process::id()));
    let mut command = std::process::Command::new("gcc");
    command.args(flags).arg("-o").arg(&path);
    for source in sources {
        command.arg(fixture(source));
    }
    assert!(command.status().unwrap().success());
    path
}

fn source_span(file: &str, line: u32) -> SourceSpan {
    SourceSpan {
        file: file.into(),
        start_offset: u64::from(line) * 100,
        end_offset: u64::from(line) * 100 + 10,
        start_line: line,
        start_column: 5,
        end_line: line,
        end_column: 15,
        provenance: "direct".into(),
        spelling_file: None,
        spelling_offset: None,
    }
}

fn declaration(id: &str, kind: &str, name: &str, file: &str, line: u32) -> SourceDeclaration {
    SourceDeclaration {
        id: id.into(),
        unit_id: "tu0".into(),
        kind: kind.into(),
        name: name.into(),
        type_spelling: Some("int".into()),
        span: source_span(file, line),
        name_line: Some(line),
        name_column: Some(5),
        function: Some("main".into()),
        initializer: None,
        dwarf_declaration: None,
        correlation: "unresolved".into(),
        dwarf_type_ref: None,
        dwarf_member: None,
        parent_type_spelling: None,
    }
}

fn expression(id: &str, kind: &str, file: &str, line: u32) -> SourceExpression {
    SourceExpression {
        id: id.into(),
        unit_id: "tu0".into(),
        kind: kind.into(),
        span: source_span(file, line),
        type_spelling: None,
        operator: None,
        access_syntax: None,
        callee: None,
        call_kind: None,
        operand_ids: Vec::new(),
        referenced_declaration: None,
        function: Some("main".into()),
        sizeof_operand: None,
        sizeof_operand_kind: None,
        referenced_dwarf_declaration: None,
        dwarf_type_ref: None,
        dwarf_type_evidence: None,
        member_ref: None,
        member_name: None,
        base_type_spelling: None,
    }
}

fn heap_event(index: usize, kind: HeapEventType, file: &str, line: u32) -> HeapEvent {
    HeapEvent {
        raw_caller_address: Some("0x1234".into()),
        sequence: index as u64 + 1,
        pid: 1,
        allocation_id: index as u64 + 1,
        event_type: kind,
        address: Some(format!("0x{:x}", 0x5000 + index)),
        size_bytes: Some(4),
        calloc_count: None,
        calloc_element_size: None,
        source_file: Some(file.into()),
        source_line: Some(line),
        old_allocation_id: None,
        heap_usage_after: 4,
        heap_usage_complete: Some(true),
        detail: None,
        attribution: Some(HeapEventAttribution {
            call_site_resolution: "resolved_return_address_minus_one".into(),
            call_site_function: Some("main".into()),
            checkpoint_interval: None,
            history_reliable: true,
        }),
    }
}

/// A. Missing providers leave fusion absent or partial, never fabricated.
#[test]
fn a_missing_providers_keep_fusion_optional() {
    let mut empty = DebugSessionTrace::default();
    evidence::fuse(&mut empty);
    assert!(empty.evidence_fusion.is_none());

    let mut syntax_only = DebugSessionTrace {
        source_metadata: Some(SourceMetadataRegistry::default()),
        ..Default::default()
    };
    evidence::fuse(&mut syntax_only);
    let summary = syntax_only.evidence_fusion.as_ref().unwrap();
    assert!(!summary.dwarf_available);
    // A default/empty registry has no usable translation unit; presence
    // alone must not advertise Clang syntax to consumers.
    assert!(!summary.ast_available);
    assert_eq!(summary.declaration_links, 0);
    assert_eq!(summary.rejected_links, 0);

    let mut partial = DebugSessionTrace {
        source_metadata: Some(SourceMetadataRegistry {
            translation_units: vec![
                clang_ast::AstUnit {
                    status: "unavailable".into(),
                    ..Default::default()
                },
                clang_ast::AstUnit {
                    status: "partial".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    evidence::fuse(&mut partial);
    assert!(
        partial.evidence_fusion.unwrap().ast_available,
        "one usable partial TU is different from all failed TUs"
    );
}

/// B/C. The real DWARF layer resolves a cross-CU abstract origin and retains
/// subprogram declarations without turning them into runtime variables.
#[test]
fn b_c_cross_cu_origin_and_subprogram_declarations_are_real() {
    let path = compile(
        &["phase7_lto_a.c", "phase7_lto_b.c", "phase7_lto_main.c"],
        &["-O2", "-gdwarf-5", "-flto"],
    );
    let (registry, _) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let cross: Vec<_> = registry
        .declarations
        .iter()
        .filter(|declaration| {
            declaration
                .resolved_origin
                .as_deref()
                .is_some_and(|origin| origin.contains("cross_cu"))
        })
        .collect();
    assert!(!cross.is_empty(), "{:?}", registry.declarations);
    assert!(cross.iter().all(|declaration| {
        declaration.name.is_some()
            && declaration.file.is_some()
            && declaration.line.is_some()
            && declaration.column.is_some()
    }));
    assert!(registry
        .declarations
        .iter()
        .any(|declaration| declaration.kind == "subprogram"));

    let mut trace = DebugSessionTrace {
        type_registry: Some(registry),
        ..Default::default()
    };
    let subprogram_declaration = trace
        .type_registry
        .as_ref()
        .unwrap()
        .declarations
        .iter()
        .find(|declaration| declaration.kind == "subprogram" && declaration.name.is_some())
        .unwrap()
        .clone();
    let subprogram = subprogram_declaration.id.clone();
    trace.snapshots.push(ExecutionSnapshot {
        variables: vec![VariableInfo {
            name: subprogram_declaration.name.clone().unwrap(),
            declaration_file: subprogram_declaration.file.clone(),
            declaration_line: subprogram_declaration.line,
            activation_id: Some(9),
            ..Default::default()
        }],
        ..Default::default()
    });
    dwarf::associate(&mut trace);
    assert!(
        trace.snapshots[0].variables[0].declaration_ref.is_none(),
        "subprogram declarations must never back runtime variable association"
    );
    assert_ne!(
        trace.snapshots[0].variables[0].declaration_ref.as_deref(),
        Some(subprogram.as_str())
    );
    assert_eq!(trace.snapshots[0].variables[0].activation_id, Some(9));
}

/// D/E/F/O. Real Clang/DWARF correlation keeps exact locations, uses the
/// missing-column fallback only when needed, and leaves fields to the member
/// resolver instead of coincident source locations. The focused fallback
/// tests live in `src/clang_ast.rs`; this integration case only asserts that
/// real compiler flags are accepted without a semantic reinterpretation.
#[test]
fn d_e_f_o_declaration_flags_are_conservative() {
    use c_trace_engine::clang_ast::clang_flags;
    use c_trace_engine::config::BuildConfig;

    let build = BuildConfig {
        source_file: fixture("ast_source.c"),
        ..Default::default()
    };
    assert!(clang_flags(&build).is_ok());
    let unsafe_build = BuildConfig {
        compiler_flags: vec!["-unknown-semantic-flag".into()],
        ..Default::default()
    };
    assert!(clang_flags(&unsafe_build).is_err());
}

/// G/H/I/J. A real GCC binary + Clang AST produces declaration, type,
/// expression and member links; every reference points into the serialized
/// registries and the one-time audit rejects nothing.
#[tokio::test]
async fn g_h_i_j_real_fusion_links_are_bounded_and_valid() {
    if c_trace_engine::compiler::probe_tool("clang", "--version")
        .await
        .is_err()
    {
        eprintln!("skipped: Clang unavailable");
        return;
    }
    let path = compile(&["ast_source.c"], &["-O0", "-gdwarf-5"]);
    let (dwarf_registry, _) = dwarf::extract(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let build = c_trace_engine::config::BuildConfig {
        source_file: fixture("ast_source.c"),
        ..Default::default()
    };
    let (mut registry, _) = clang_ast::analyze(&build, "clang", Some(&dwarf_registry)).await;
    let mut trace = DebugSessionTrace {
        type_registry: Some(dwarf_registry),
        source_metadata: Some(std::mem::take(&mut registry)),
        ..Default::default()
    };
    evidence::fuse(&mut trace);
    let summary = trace.evidence_fusion.as_ref().unwrap();
    assert!(summary.dwarf_available && summary.ast_available);
    assert!(summary.declaration_links > 0, "{summary:?}");
    assert!(summary.declaration_type_links > 0, "{summary:?}");
    assert!(summary.expression_dwarf_links > 0, "{summary:?}");
    assert!(summary.expression_type_links > 0, "{summary:?}");
    assert!(summary.member_links >= 2, "{summary:?}");
    assert_eq!(summary.rejected_links, 0, "{summary:?}");

    let registry = trace.source_metadata.as_ref().unwrap();
    let types = &trace.type_registry.as_ref().unwrap().types;
    for declaration in &registry.declarations {
        if let Some(id) = declaration.dwarf_type_ref.as_deref() {
            assert!(types.iter().any(|entry| entry.id == id));
        }
        if let Some(member) = declaration.dwarf_member.as_ref() {
            let parent = types
                .iter()
                .find(|entry| entry.id == member.type_ref)
                .unwrap();
            assert_eq!(
                parent.members[member.member_index].name.as_deref(),
                Some(member.name.as_str())
            );
        }
    }
    let fields: Vec<_> = registry
        .declarations
        .iter()
        .filter(|declaration| declaration.kind == "FieldDecl")
        .collect();
    assert!(fields.iter().any(|field| field.name == "x"));
    assert!(fields.iter().any(|field| field.name == "y"));
    assert!(fields
        .iter()
        .filter(|field| field.dwarf_member.is_some())
        .all(|field| field.correlation == "unique_parent_type_and_member_name"));

    // JSON roundtrip keeps the additive links and schema 2.
    let bytes = serde_json::to_vec(&trace).unwrap();
    let restored: DebugSessionTrace = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.schema_version, 2);
    assert_eq!(
        restored
            .source_metadata
            .as_ref()
            .unwrap()
            .declarations
            .len(),
        registry.declarations.len()
    );
    assert_eq!(
        restored.evidence_fusion.as_ref().unwrap().member_links,
        summary.member_links
    );
    let store = TraceStore::from_trace(restored).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

/// K/L. Indexed heap correlation yields one candidate for a unique direct
/// call, explicit ambiguity for multiple calls, and explicit ambiguity for a
/// macro-only line. Output stays bounded.
#[test]
fn k_l_heap_candidates_and_ambiguities_are_explicit_and_bounded() {
    let file = "/tmp/phase7-heap.c";
    let mut registry = SourceMetadataRegistry::default();
    let mut direct_malloc = expression("tu0:e0", "CallExpr", file, 10);
    direct_malloc.call_kind = Some("malloc".into());
    let mut macro_free = expression("tu0:e1", "CallExpr", file, 12);
    macro_free.call_kind = Some("free".into());
    macro_free.span.provenance = "macro_expansion".into();
    let mut direct_free = expression("tu0:e4", "CallExpr", file, 12);
    direct_free.call_kind = Some("free".into());
    let mut macro_only = expression("tu0:e5", "CallExpr", file, 16);
    macro_only.call_kind = Some("free".into());
    macro_only.span.provenance = "macro_expansion".into();
    let mut free_a = expression("tu0:e2", "CallExpr", file, 14);
    free_a.call_kind = Some("free".into());
    let mut free_b = expression("tu0:e3", "CallExpr", file, 14);
    free_b.call_kind = Some("free".into());
    let mut calls = vec![
        direct_malloc,
        macro_free,
        direct_free,
        macro_only,
        free_a,
        free_b,
    ];
    for index in 0..12 {
        let mut extra = expression(&format!("tu0:x{index}"), "CallExpr", file, 20);
        extra.call_kind = Some("malloc".into());
        calls.push(extra);
    }
    registry.expressions = calls;
    let events = vec![
        heap_event(0, HeapEventType::Allocated, file, 10),
        heap_event(1, HeapEventType::Freed, file, 12),
        heap_event(2, HeapEventType::Freed, file, 14),
        heap_event(3, HeapEventType::Allocated, file, 20),
        // A different resolved line must not inherit the indexed calls.
        heap_event(4, HeapEventType::Allocated, file, 99),
        heap_event(5, HeapEventType::Freed, file, 16),
    ];
    clang_ast::correlate_heap_calls(&mut registry, &events, std::path::Path::new("/tmp"));

    assert_eq!(registry.heap_call_candidates.len(), 1);
    let candidate = &registry.heap_call_candidates[0];
    assert_eq!(candidate.event_index, 0);
    assert_eq!(candidate.expression_id, "tu0:e0");
    assert_eq!(
        candidate.evidence,
        "unique_direct_call_on_resolved_line_not_exact_execution"
    );
    let ambiguity = |index: usize| {
        registry
            .heap_call_ambiguities
            .iter()
            .find(|item| item.event_index == index)
            .unwrap()
    };
    assert_eq!(
        ambiguity(1).evidence,
        "multiple_compatible_calls_on_resolved_line"
    );
    assert!(ambiguity(1).expression_ids.contains(&"tu0:e1".into()));
    assert!(ambiguity(1).expression_ids.contains(&"tu0:e4".into()));
    assert_eq!(
        ambiguity(2).evidence,
        "multiple_compatible_calls_on_resolved_line"
    );
    assert_eq!(
        ambiguity(5).evidence,
        "only_macro_expanded_calls_on_resolved_line"
    );
    let many = ambiguity(3);
    assert_eq!(many.evidence, "multiple_compatible_calls_on_resolved_line");
    assert_eq!(
        many.expression_ids.len(),
        8,
        "ambiguity output must stay bounded"
    );
    assert!(registry
        .heap_call_ambiguities
        .iter()
        .all(|item| item.event_index != 4));

    let mut trace = DebugSessionTrace {
        source_metadata: Some(registry),
        heap_events: events,
        ..Default::default()
    };
    evidence::fuse(&mut trace);
    // A second pass is idempotent: no duplicate records, same summary counts.
    let first = trace.evidence_fusion.clone().unwrap();
    evidence::fuse(&mut trace);
    let second = trace.evidence_fusion.as_ref().unwrap();
    assert_eq!(first.heap_call_candidates, second.heap_call_candidates);
    assert_eq!(first.heap_call_ambiguities, second.heap_call_ambiguities);
    assert_eq!(
        trace
            .source_metadata
            .as_ref()
            .unwrap()
            .heap_call_candidates
            .len(),
        second.heap_call_candidates
    );
    assert!(TraceStore::from_trace(trace)
        .unwrap()
        .validate_deep()
        .is_structurally_sound());
}

/// M/N. Runtime observations link to an AST declaration only through one
/// unique exact DWARF identity; collision, mismatch and activation identity
/// are handled explicitly.
#[test]
fn m_n_runtime_source_bridge_requires_unique_exact_dwarf_identity() {
    let file = "/tmp/phase7-bridge.c";
    let dwarf_id = "debug:u0:o10";
    let dwarf = dwarf::TypeRegistry {
        provider: "gcc_dwarf".into(),
        declarations: vec![dwarf_declaration(dwarf_id, file)],
        ..Default::default()
    };
    let mut source = declaration("tu0:d0", "VarDecl", "value", file, 4);
    source.dwarf_declaration = Some(dwarf_id.into());
    source.correlation = "unique_location".into();
    let mut duplicate = declaration("tu0:d1", "VarDecl", "value", file, 4);
    duplicate.dwarf_declaration = Some(dwarf_id.into());
    duplicate.correlation = "unique_location".into();
    let mut trace = DebugSessionTrace {
        type_registry: Some(dwarf),
        source_metadata: Some(SourceMetadataRegistry {
            declarations: vec![source, duplicate],
            ..Default::default()
        }),
        ..Default::default()
    };
    trace.snapshots.push(ExecutionSnapshot {
        variables: vec![
            VariableInfo {
                name: "value".into(),
                declaration_ref: Some(dwarf_id.into()),
                activation_id: Some(7),
                ..Default::default()
            },
            VariableInfo {
                name: "other".into(),
                declaration_ref: Some("debug:u0:o11".into()),
                activation_id: Some(8),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    evidence::fuse(&mut trace);
    let variables = &trace.snapshots[0].variables;
    assert!(
        variables[0].source_declaration_ref.is_none(),
        "two AST declarations claiming one DWARF DIE must stay ambiguous"
    );
    assert!(variables[1].source_declaration_ref.is_none());
    assert_eq!(variables[0].activation_id, Some(7));
    assert_eq!(variables[1].activation_id, Some(8));

    // Removing the duplicate creates exactly one bridge.
    trace.source_metadata.as_mut().unwrap().declarations.pop();
    evidence::fuse(&mut trace);
    assert_eq!(
        trace.snapshots[0].variables[0]
            .source_declaration_ref
            .as_deref(),
        Some("tu0:d0")
    );
    assert_eq!(trace.snapshots[0].variables[0].activation_id, Some(7));
    assert!(trace.snapshots[0].variables[1]
        .source_declaration_ref
        .is_none());
}

fn dwarf_declaration(id: &str, file: &str) -> c_trace_engine::dwarf::Declaration {
    c_trace_engine::dwarf::Declaration {
        id: id.into(),
        unit_id: "debug:u0".into(),
        kind: "variable".into(),
        name: Some("value".into()),
        file: Some(file.into()),
        line: Some(4),
        column: Some(5),
        function: Some("main".into()),
        scope_id: "debug:u0:o1".into(),
        type_ref: None,
        external: None,
        declaration_only: false,
        resolved_origin: None,
    }
}

/// P/Q. Legacy traces load unchanged, and hand-edited Phase 7 references are
/// reported as structural corruption instead of accepted silently.
#[test]
fn p_q_legacy_and_invalid_references() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/v21_dual.json");
    let legacy = TraceStore::open(path, TraceLoadOptions::default()).unwrap();
    assert!(legacy.trace().evidence_fusion.is_none());
    assert!(legacy.raw().source_metadata().is_none());
    assert!(legacy.validate_deep().is_structurally_sound());
    assert!(matches!(
        legacy.source_declaration("tu0:d0"),
        Err(c_trace_engine::trace_parser::QueryError::Unavailable { .. })
    ));

    let file = "/tmp/phase7-invalid.c";
    let mut registry = SourceMetadataRegistry::default();
    let mut decl = declaration("tu0:d0", "VarDecl", "value", file, 4);
    decl.dwarf_declaration = Some("debug:u0:o10".into());
    let mut expr = expression("tu0:e0", "DeclRefExpr", file, 4);
    expr.referenced_declaration = Some("tu0:d0".into());
    expr.dwarf_type_ref = Some("debug:u0:o99".into());
    let mut member_expr = expression("tu0:e1", "MemberExpr", file, 5);
    member_expr.member_ref = Some(DwarfMemberRef {
        type_ref: "debug:u0:o50".into(),
        member_index: 7,
        name: "missing".into(),
        byte_offset: Some(4),
    });
    registry.expressions = vec![expr, member_expr];
    registry.declarations = vec![decl];
    registry.heap_call_candidates = vec![HeapCallCandidate {
        event_index: 9,
        expression_id: "tu0:e404".into(),
        evidence: "should_not_exist".into(),
    }];
    registry.heap_call_ambiguities = vec![HeapCallAmbiguity {
        event_index: 9,
        evidence: "multiple_compatible_calls_on_resolved_line".into(),
        expression_ids: vec!["tu0:e404".into()],
    }];
    let mut trace = DebugSessionTrace {
        source_metadata: Some(registry),
        ..Default::default()
    };
    trace.snapshots.push(ExecutionSnapshot {
        variables: vec![VariableInfo {
            name: "value".into(),
            declaration_ref: Some("debug:u0:o10".into()),
            source_declaration_ref: Some("tu0:d404".into()),
            ..Default::default()
        }],
        ..Default::default()
    });
    let report = TraceStore::from_trace(trace).unwrap().validate_deep();
    let codes: Vec<_> = report
        .findings()
        .iter()
        .filter(|finding| {
            finding.severity
                == c_trace_engine::trace_parser::ValidationSeverity::StructuralCorruption
        })
        .map(|finding| finding.code)
        .collect();
    for expected in [
        "AST_DECLARATION_REFERENCE_INVALID",
        "AST_EXPRESSION_REFERENCE_INVALID",
        "AST_HEAP_CANDIDATE_INVALID",
        "AST_HEAP_AMBIGUITY_INVALID",
        "VARIABLE_SOURCE_DECLARATION_INVALID",
    ] {
        assert!(codes.contains(&expected), "{expected} missing: {codes:?}");
    }
}

/// Valid IDs are not sufficient: each recorded cross-provider relationship
/// must agree with its source unit, declaration, member parent and call site.
#[test]
fn phase8r3_existing_targets_with_contradictory_relationships() {
    let file = "/tmp/phase8r3-links.c";
    let mut source = SourceMetadataRegistry::default();
    source.translation_units.push(clang_ast::AstUnit {
        id: "tu0".into(),
        file: file.into(),
        status: "complete".into(),
        ..Default::default()
    });
    source.translation_units.push(clang_ast::AstUnit {
        id: "tu1".into(),
        file: "/tmp/unrelated.c".into(),
        status: "complete".into(),
        ..Default::default()
    });
    let mut decl = declaration("d0", "VarDecl", "value", file, 4);
    decl.dwarf_declaration = Some("dw0".into());
    let mut other = declaration("d1", "VarDecl", "other", file, 5);
    other.dwarf_declaration = Some("dw1".into());
    let mut field = declaration("f0", "FieldDecl", "member", file, 6);
    field.parent_type_spelling = Some("struct First".into());
    field.dwarf_member = Some(DwarfMemberRef {
        type_ref: "type0".into(),
        member_index: 0,
        name: "member".into(),
        byte_offset: Some(0),
    });
    source.declarations = vec![decl, other, field];
    let mut expr = expression("e0", "DeclRefExpr", file, 4);
    expr.referenced_declaration = Some("d0".into());
    expr.referenced_dwarf_declaration = Some("dw0".into());
    let mut call = expression("call", "CallExpr", file, 8);
    call.call_kind = Some("malloc".into());
    source.expressions = vec![expr, call];
    source.heap_call_candidates.push(HeapCallCandidate {
        event_index: 0,
        expression_id: "call".into(),
        evidence: "unique_direct_call_on_resolved_line_not_exact_execution".into(),
    });
    let dwarf = dwarf::TypeRegistry {
        units: vec![dwarf::UnitInfo {
            id: "debug:u0".into(),
            name: Some(file.into()),
            compilation_dir: None,
            dwarf_version: 5,
            address_size: 8,
        }],
        declarations: vec![
            dwarf_declaration("dw0", file),
            dwarf::Declaration {
                name: Some("other".into()),
                line: Some(5),
                ..dwarf_declaration("dw1", file)
            },
        ],
        types: ["First", "Second"]
            .iter()
            .enumerate()
            .map(|(i, name)| {
                serde_json::from_value::<dwarf::TypeEntry>(serde_json::json!({
                    "id":format!("type{i}"), "unit_id":"debug:u0", "die_offset":i,
                    "kind":"struct", "name":name, "byte_size":4, "encoding":null,
                    "referenced_type":null, "members":[{"name":"member", "type_ref":null,
                    "byte_offset":0,"implicit_union_zero":false,"bit_size":null,
                    "data_bit_offset":null,"legacy_bit_offset":null,"byte_size":4}],
                    "dimensions":[],"enumerators":[],"declaration_only":false,
                    "runtime_dependent":false
                }))
                .unwrap()
            })
            .collect(),
        ..Default::default()
    };
    let trace = DebugSessionTrace {
        source_metadata: Some(source),
        type_registry: Some(dwarf),
        heap_events: vec![heap_event(0, HeapEventType::Allocated, file, 8)],
        ..Default::default()
    };
    let violations = |t: DebugSessionTrace| -> Vec<&'static str> {
        TraceStore::from_trace(t)
            .unwrap()
            .validate_deep()
            .findings()
            .iter()
            .filter(|f| {
                f.severity == c_trace_engine::trace_parser::ValidationSeverity::StructuralCorruption
            })
            .map(|f| f.code)
            .collect()
    };
    assert!(violations(trace.clone()).is_empty(), "valid relationships");
    let mut bad = trace.clone();
    bad.source_metadata.as_mut().unwrap().expressions[0].referenced_dwarf_declaration =
        Some("dw1".into());
    assert!(violations(bad).contains(&"AST_EXPRESSION_RELATION_INVALID"));
    let mut bad = trace.clone();
    bad.source_metadata.as_mut().unwrap().expressions[0].unit_id = "tu1".into();
    assert!(violations(bad).contains(&"AST_EXPRESSION_RELATION_INVALID"));
    let mut bad = trace.clone();
    bad.source_metadata.as_mut().unwrap().declarations[0].dwarf_declaration = Some("dw1".into());
    assert!(violations(bad).contains(&"AST_DECLARATION_RELATION_INVALID"));
    let mut bad = trace.clone();
    bad.type_registry
        .as_mut()
        .unwrap()
        .units
        .push(dwarf::UnitInfo {
            id: "debug:u1".into(),
            name: Some("/tmp/unrelated.c".into()),
            compilation_dir: None,
            dwarf_version: 5,
            address_size: 8,
        });
    bad.type_registry.as_mut().unwrap().declarations[0].unit_id = "debug:u1".into();
    assert!(violations(bad).contains(&"AST_DECLARATION_RELATION_INVALID"));
    let mut bad = trace.clone();
    bad.source_metadata.as_mut().unwrap().declarations[2]
        .dwarf_member
        .as_mut()
        .unwrap()
        .type_ref = "type1".into();
    assert!(violations(bad).contains(&"AST_DECLARATION_RELATION_INVALID"));
    let mut bad = trace.clone();
    bad.source_metadata.as_mut().unwrap().heap_call_candidates[0].expression_id = "e0".into();
    assert!(violations(bad).contains(&"AST_HEAP_CANDIDATE_RELATION_INVALID"));
    let mut bad = trace.clone();
    bad.source_metadata.as_mut().unwrap().expressions[1].call_kind = Some("free".into());
    assert!(violations(bad).contains(&"AST_HEAP_CANDIDATE_RELATION_INVALID"));
    let mut bad = trace.clone();
    bad.heap_events[0].source_line = Some(100);
    assert!(violations(bad).contains(&"AST_HEAP_CANDIDATE_RELATION_INVALID"));
    let mut partial = trace;
    partial.heap_events[0].source_file = None;
    assert!(!violations(partial).contains(&"AST_HEAP_CANDIDATE_RELATION_INVALID"));
}

/// R/S. AST on/off preserves all runtime evidence while adding only the
/// configured static links, and AST-off stays complete for its scope.
#[tokio::test]
async fn r_s_ast_on_off_preserves_runtime_and_evidence_fidelity() {
    if !tools_available().await {
        return;
    }
    let mut config = TraceConfig::default();
    config.build.source_file = fixture("ast_source.c");
    config.tracing.mode = TraceMode::Source;
    config.tracing.static_disassembly = false;
    config.sanitizer_trigger = SanitizerTrigger::Never;
    config.limits.timeout_seconds = 60;
    config.limits.max_steps = 2000;
    let plain = trace_program(config.clone()).await.unwrap();
    config.clang_ast = true;
    let enriched = trace_program(config).await.unwrap();

    assert!(plain.source_metadata.is_none());
    assert!(plain.evidence_fusion.as_ref().is_some());
    assert!(!plain.evidence_fusion.as_ref().unwrap().ast_available);
    let summary = enriched.evidence_fusion.as_ref().unwrap();
    assert!(summary.ast_available && summary.dwarf_available);
    assert!(summary.declaration_links > 0, "{summary:?}");
    assert!(summary.runtime_source_links > 0, "{summary:?}");
    assert_eq!(summary.rejected_links, 0, "{summary:?}");
    assert!(
        !enriched
            .source_metadata
            .as_ref()
            .unwrap()
            .heap_call_candidates
            .is_empty(),
        "{summary:?}"
    );
    assert!(
        !enriched
            .source_metadata
            .as_ref()
            .unwrap()
            .heap_call_ambiguities
            .is_empty(),
        "{summary:?}"
    );

    let event_shape = |trace: &DebugSessionTrace| {
        trace
            .heap_events
            .iter()
            .map(|event| {
                (
                    event.sequence,
                    format!("{:?}", event.event_type),
                    event.allocation_id,
                    event.old_allocation_id,
                    event.size_bytes,
                    event.source_file.clone(),
                    event.source_line,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(event_shape(&plain), event_shape(&enriched));
    assert_eq!(
        serde_json::to_value(&plain.execution).unwrap(),
        serde_json::to_value(&enriched.execution).unwrap()
    );
    assert_eq!(plain.snapshots.len(), enriched.snapshots.len());
    assert_eq!(
        plain.recording.allocation_lifetimes.len(),
        enriched.recording.allocation_lifetimes.len()
    );

    // Compare every stable runtime observation after removing only ASLR
    // addresses, pids, and measured durations; additive Phase 7 fields are
    // excluded explicitly.
    let left = runtime_view(&plain);
    let right = runtime_view(&enriched);
    let mut differences = Vec::new();
    diff_paths(&left, &right, "", &mut differences);
    assert!(
        differences.is_empty(),
        "AST must not alter runtime evidence: {differences:?}"
    );

    let linked_variable = enriched
        .snapshots
        .iter()
        .enumerate()
        .find_map(|(snapshot, item)| {
            item.variables
                .iter()
                .enumerate()
                .find_map(|(variable, info)| {
                    info.source_declaration_ref
                        .as_ref()
                        .map(|source| (snapshot, variable, source.clone()))
                })
        })
        .expect("a real AST-on trace must link at least one runtime variable");
    let store = TraceStore::from_trace(enriched).unwrap();
    let (snapshot, variable, source_id) = linked_variable;
    let declaration = store
        .semantic_input()
        .checkpoint_context(snapshot)
        .unwrap()
        .variable_source_declaration(variable)
        .unwrap()
        .unwrap();
    assert_eq!(declaration.id, source_id);
    assert!(store.raw().source_declaration(&source_id).is_ok());
    let expression_id = store
        .raw()
        .source_metadata()
        .unwrap()
        .expressions
        .first()
        .unwrap()
        .id
        .clone();
    assert!(store.raw().source_expression(&expression_id).is_ok());
    assert!(store.validate_deep().is_structurally_sound());
}

#[cfg(test)]
fn diff_paths(
    left: &serde_json::Value,
    right: &serde_json::Value,
    path: &str,
    out: &mut Vec<String>,
) {
    use serde_json::Value;
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().chain(right.keys()) {
                let next = format!("{path}.{key}");
                match (left.get(key), right.get(key)) {
                    (Some(a), Some(b)) => diff_paths(a, b, &next, out),
                    _ => out.push(next),
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                out.push(format!("{path}.len({}!={})", left.len(), right.len()));
            }
            for (index, (a, b)) in left.iter().zip(right).enumerate() {
                diff_paths(a, b, &format!("{path}[{index}]"), out);
            }
        }
        _ if left != right => out.push(path.to_owned()),
        _ => {}
    }
}

fn variable_view(variable: &VariableInfo) -> serde_json::Value {
    serde_json::json!({
        "name": variable.name,
        "value": variable.value,
        "c_type": variable.c_type,
        "declaration_file": variable.declaration_file,
        "declaration_line": variable.declaration_line,
        "declaration_ref": variable.declaration_ref,
        "type_ref": variable.type_ref,
        "activation_id": variable.activation_id,
        "identity": variable.identity,
        "is_argument": variable.is_argument,
        "lexical_scope_id": variable.lexical_scope_id,
        "scope": variable.scope,
        "scope_status": variable.scope_status,
        "initialization_status": variable.initialization_status,
        "read_status": variable.read_status,
        "read_error": variable.read_error,
        "raw_bytes_hex": variable.raw_bytes_hex,
        "bytes_status": variable.bytes_status,
        "size_bytes": variable.size_bytes,
        "runtime_size_bytes": variable.runtime_size_bytes,
        "size_source": variable.size_source,
        "memory_evidence": variable.memory_evidence,
        "target": variable.target.as_ref().map(|target| serde_json::json!({
            "kind": target.kind,
            "allocation_id": target.allocation_id,
            "address_match_allocation_id": target.address_match_allocation_id,
            "offset_bytes": target.offset_bytes,
            "active": target.active,
            "within_extent": target.within_extent,
            "variable_identity": target.variable_identity,
            "provenance_status": target.provenance_status,
        })),
    })
}

fn block_view(block: &c_trace_engine::models::HeapBlockInfo) -> serde_json::Value {
    serde_json::json!({
        "observed_at_stop": block.observed_at_stop,
        "allocation_id": block.allocation_id,
        "size_bytes": block.size_bytes,
        "allocation_type": block.allocation_type,
        "is_freed": block.is_freed,
        "allocation_file": block.allocation_file,
        "allocation_line": block.allocation_line,
        "raw_bytes_hex": block.raw_bytes_hex,
        "memory_read_error": block.memory_read_error,
        "realloc_from_id": block.realloc_from_id,
        "memory_evidence": block.memory_evidence,
        "lifetime_status": block.lifetime_status,
    })
}

fn runtime_view(trace: &DebugSessionTrace) -> serde_json::Value {
    let snapshots: Vec<_> = trace
        .snapshots
        .iter()
        .map(|snapshot| {
            serde_json::json!({
                "step_index": snapshot.step_index,
                "file": snapshot.file,
                "line": snapshot.line,
                "func": snapshot.func,
                "stdout_so_far": snapshot.stdout_so_far,
                "stopped_reason": snapshot.stopped_reason,
                "memory_truncated": snapshot.memory_truncated,
                "observation": {
                    "stop_id": snapshot.observation.stop_id,
                    "instruction_position": snapshot.observation.instruction_position,
                    "complete": snapshot.observation.complete,
                    "origin": snapshot.observation.origin,
                    "heap_event_count": snapshot.observation.heap_event_count,
                    "errors": snapshot.observation.errors,
                },
                "call_stack": snapshot.call_stack.iter().map(|frame| serde_json::json!({
                    "level": frame.level,
                    "func": frame.func,
                    "file": frame.file,
                    "line": frame.line,
                })).collect::<Vec<_>>(),
                "variables": snapshot.variables.iter().map(variable_view).collect::<Vec<_>>(),
                "globals": snapshot.observation.globals.iter().map(variable_view).collect::<Vec<_>>(),
                "caller_frames": snapshot.caller_frames.iter().map(|frame| serde_json::json!({
                    "activation_id": frame.activation_id,
                    "frame": {
                        "level": frame.frame.level,
                        "func": frame.frame.func,
                        "file": frame.frame.file,
                        "line": frame.frame.line,
                    },
                    "variables": frame.variables.iter().map(variable_view).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "heap_blocks": snapshot.heap_blocks.iter().map(block_view).collect::<Vec<_>>(),
            })
        })
        .collect();
    let events: Vec<_> = trace
        .heap_events
        .iter()
        .map(|event| {
            serde_json::json!({
                "sequence": event.sequence,
                "allocation_id": event.allocation_id,
                "event_type": event.event_type,
                "size_bytes": event.size_bytes,
                "calloc_count": event.calloc_count,
                "calloc_element_size": event.calloc_element_size,
                "source_file": event.source_file,
                "source_line": event.source_line,
                "old_allocation_id": event.old_allocation_id,
                "heap_usage_after": event.heap_usage_after,
                "heap_usage_complete": event.heap_usage_complete,
                "detail": event.detail,
                "attribution": event.attribution.as_ref().map(|attribution| serde_json::json!({
                    "call_site_resolution": attribution.call_site_resolution,
                    "call_site_function": attribution.call_site_function,
                    "checkpoint_interval": attribution.checkpoint_interval,
                    "history_reliable": attribution.history_reliable,
                })),
            })
        })
        .collect();
    let lifetimes: Vec<_> = trace
        .recording
        .allocation_lifetimes
        .iter()
        .map(|lifetime| {
            serde_json::json!({
                "allocation_id": lifetime.allocation_id,
                "allocation_sequence": lifetime.allocation_sequence,
                "free_sequence": lifetime.free_sequence,
                "resolution": lifetime.resolution,
                "lifetime_status": lifetime.lifetime_status,
            })
        })
        .collect();
    let mut heap_history = serde_json::to_value(&trace.recording.heap_history).unwrap();
    if let (Some(history), Some(object)) = (
        trace.recording.heap_history.as_ref(),
        heap_history.as_object_mut(),
    ) {
        object.remove("followed_inferior_pid");
        object.remove("observed_event_pids");
        object.insert(
            "observed_pid_count".into(),
            history.observed_event_pids.len().into(),
        );
        object.insert(
            "followed_pid_observed".into(),
            history
                .followed_inferior_pid
                .is_some_and(|pid| history.observed_event_pids.contains(&pid))
                .into(),
        );
    }
    serde_json::json!({
        "execution": trace.execution,
        "compiled_successfully": trace.compiled_successfully,
        "compile_errors": trace.compile_errors,
        "execution_status": trace.execution_status,
        "schema_version": trace.schema_version,
        "source_checkpoints": trace.recording.source_checkpoints,
        "coverage": trace.recording.coverage,
        "instruction_count": trace.recording.instruction_count,
        "heap_history": heap_history,
        "trace_gaps": trace.recording.trace_gaps.iter().map(|gap| serde_json::json!({
            "origin": gap.origin,
            "reason": gap.reason,
            "instruction_position": gap.instruction_position,
        })).collect::<Vec<_>>(),
        "snapshots": snapshots,
        "heap_events": events,
        "heap_state": trace.heap_state.iter().map(block_view).collect::<Vec<_>>(),
        "allocation_lifetimes": lifetimes,
        "analysis_notes": trace.analysis_notes,
    })
}

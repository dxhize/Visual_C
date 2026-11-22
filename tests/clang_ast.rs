use std::path::PathBuf;

use c_trace_engine::clang_ast;
use c_trace_engine::config::{SanitizerTrigger, TraceMode};
use c_trace_engine::trace::trace_program;
use c_trace_engine::trace_parser::TraceStore;
use c_trace_engine::TraceConfig;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/c")
        .join(name)
}

#[tokio::test]
async fn gcc_optimization_preprocessing_matches_clang_ast_branch() {
    for tool in ["gcc", "clang"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let build = c_trace_engine::config::BuildConfig {
        source_file: fixture("phase8r1_opt.c"),
        compiler_flags: vec!["-O2".into()],
        ..Default::default()
    };
    let gcc = tokio::process::Command::new("gcc")
        .args(["-O0", "-O2", "-E", "-P", "-x", "c"])
        .arg(&build.source_file)
        .output()
        .await
        .unwrap();
    assert!(
        gcc.status.success(),
        "{}",
        String::from_utf8_lossy(&gcc.stderr)
    );
    let gcc_source = String::from_utf8(gcc.stdout).unwrap();
    assert!(
        gcc_source.contains("int optimized_branch = 1;"),
        "{gcc_source}"
    );

    let clang = tokio::process::Command::new("clang")
        .args(["-O0", "-E", "-P", "-x", "c"])
        .args(clang_ast::clang_flags(&build).unwrap())
        .arg(&build.source_file)
        .output()
        .await
        .unwrap();
    assert!(
        clang.status.success(),
        "{}",
        String::from_utf8_lossy(&clang.stderr)
    );
    let clang_source = String::from_utf8(clang.stdout).unwrap();
    assert!(
        clang_source.contains("int optimized_branch = 1;"),
        "GCC selected optimized branch, but Clang selected:\n{clang_source}"
    );
    let (registry, _) = clang_ast::analyze(&build, "clang", None).await;
    assert_eq!(
        registry.translation_units[0].status, "complete",
        "{:?}",
        registry.translation_units
    );
    assert!(registry
        .declarations
        .iter()
        .any(|decl| decl.name == "optimized_branch"));
    assert!(!registry
        .declarations
        .iter()
        .any(|decl| decl.name == "unoptimized_branch"));
}

#[tokio::test]
async fn optimization_and_preprocessor_flags_are_forwarded_or_ast_is_unavailable() {
    for level in ["-O0", "-O1", "-O2", "-O3", "-Og", "-Os", "-Ofast"] {
        let build = c_trace_engine::config::BuildConfig {
            compiler_flags: vec![level.into()],
            ..Default::default()
        };
        assert_eq!(clang_ast::clang_flags(&build).unwrap(), [level]);
    }
    let build = c_trace_engine::config::BuildConfig {
        source_file: fixture("phase8r1_opt.c"),
        defines: vec!["FROM_BUILD=1".into()],
        include_dirs: vec![fixture(".")],
        compiler_flags: vec![
            "-std=gnu11".into(),
            "-x".into(),
            "c".into(),
            "-D".into(),
            "FROM_FLAGS=1".into(),
            "-U".into(),
            "FROM_BUILD".into(),
            "-isystem".into(),
            fixture(".").display().to_string(),
            "-m64".into(),
        ],
        ..Default::default()
    };
    let flags = clang_ast::clang_flags(&build).unwrap();
    assert!(flags.starts_with(&[
        "-DFROM_BUILD=1".into(),
        format!("-I{}", fixture(".").display())
    ]));
    assert!(flags.ends_with(&build.compiler_flags));

    let incompatible = c_trace_engine::config::BuildConfig {
        source_file: fixture("phase8r1_opt.c"),
        compiler_flags: vec!["-O4".into()],
        ..Default::default()
    };
    assert!(clang_ast::clang_flags(&incompatible).is_err());
    let (registry, _) = clang_ast::analyze(&incompatible, "clang", None).await;
    assert_eq!(registry.translation_units[0].status, "unavailable");
    assert!(registry.translation_units[0]
        .reason
        .as_deref()
        .unwrap()
        .contains("-O4"));

    let unsupported_target = c_trace_engine::config::BuildConfig {
        compiler_flags: vec!["--target=x86_64-linux-gnu".into()],
        ..Default::default()
    };
    assert!(clang_ast::clang_flags(&unsupported_target).is_err());
}

#[tokio::test]
async fn gcc_and_clang_apply_defines_undefines_includes_and_language_mode() {
    for tool in ["gcc", "clang"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let build = c_trace_engine::config::BuildConfig {
        source_file: fixture("phase8r1_opt.c"),
        defines: vec!["PHASE8R1_FROM_DEFINE=7".into(), "PHASE8R1_REMOVED=1".into()],
        include_dirs: vec![fixture(".")],
        compiler_flags: vec![
            "-std=gnu11".into(),
            "-U".into(),
            "PHASE8R1_REMOVED".into(),
            "-include".into(),
            "phase7_lto_shared.h".into(),
        ],
        ..Default::default()
    };
    for (tool, flags) in [
        ("gcc", clang_ast::clang_flags(&build).unwrap()),
        ("clang", clang_ast::clang_flags(&build).unwrap()),
    ] {
        let output = tokio::process::Command::new(tool)
            .args(["-O0", "-E", "-P", "-x", "c"])
            .args(flags)
            .arg(&build.source_file)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "extern int phase7_shared_value;",
            "int defined_branch = 7;",
            "int unoptimized_branch = 1;",
        ] {
            assert!(
                source.contains(expected),
                "{tool} did not preprocess {expected}: {source}"
            );
        }
    }
}

#[tokio::test]
async fn mixed_direct_and_macro_free_calls_are_not_claimed_unique() {
    for tool in ["clang", "gcc", "gdb", "addr2line"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let source = fixture("phase8b_macro.c");
    // GCC preprocessing is an independent oracle for the two same-line
    // frees; neither the AST nor a single return PC can prove which ran.
    let preprocessed = std::process::Command::new("gcc")
        .args(["-E", "-P"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(preprocessed.status.success());
    assert!(String::from_utf8_lossy(&preprocessed.stdout).contains("free(first); free(second);"));
    let mut c = TraceConfig::default();
    c.build.source_file = source;
    c.clang_ast = true;
    c.tracing.mode = TraceMode::Source;
    c.sanitizer_trigger = SanitizerTrigger::Never;
    c.limits.timeout_seconds = 35;
    let trace = trace_program(c).await.unwrap();
    let registry = trace.source_metadata.as_ref().unwrap();
    let first_free = trace
        .heap_events
        .iter()
        .position(|e| {
            e.event_type == c_trace_engine::models::HeapEventType::Freed && e.source_line == Some(7)
        })
        .expect("first free resolves to line 7");
    let ambiguous = registry
        .heap_call_ambiguities
        .iter()
        .find(|entry| entry.event_index == first_free)
        .expect("not uniquely attributable");
    assert_eq!(
        ambiguous.evidence,
        "multiple_compatible_calls_on_resolved_line"
    );
    assert_eq!(ambiguous.expression_ids.len(), 2);
    assert!(registry
        .heap_call_candidates
        .iter()
        .all(|c| c.event_index != first_free));
    let expressions: Vec<_> = registry
        .expressions
        .iter()
        .filter(|e| ambiguous.expression_ids.contains(&e.id))
        .collect();
    assert!(expressions.iter().any(|e| e.span.provenance == "direct"));
    assert!(expressions
        .iter()
        .any(|e| e.span.provenance == "macro_expansion"));
}

#[tokio::test]
async fn real_clang_extracts_source_and_macro_evidence_without_header_flood() {
    if c_trace_engine::compiler::probe_tool("clang", "--version")
        .await
        .is_err()
    {
        eprintln!("skipped: Clang unavailable");
        return;
    }
    let mut build = c_trace_engine::config::BuildConfig {
        source_file: fixture("ast_source.c"),
        additional_sources: vec![fixture("ast_second.c")],
        ..Default::default()
    };
    build.compiler_flags.push("-std=gnu11".into());
    let (registry, _) = clang_ast::analyze(&build, "clang", None).await;
    assert!(
        registry
            .translation_units
            .iter()
            .all(|unit| unit.status == "complete"),
        "{:?}",
        registry.translation_units
    );
    assert_eq!(registry.translation_units.len(), 2);
    assert!(registry.declarations.len() < 200);
    assert!(registry.expressions.len() < 500);
    let (again, _) = clang_ast::analyze(&build, "clang", None).await;
    let identities = |items: &[clang_ast::SourceExpression]| {
        items
            .iter()
            .map(|item| (item.id.clone(), item.kind.clone(), item.span.start_offset))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        identities(&registry.expressions),
        identities(&again.expressions),
        "trace-local identities must not depend on Clang pointer-like IDs"
    );
    let calls: Vec<_> = registry
        .expressions
        .iter()
        .filter_map(|item| item.call_kind.as_deref())
        .collect();
    for name in ["malloc", "calloc", "realloc", "free"] {
        assert!(calls.contains(&name), "missing {name}: {calls:?}");
    }
    for kind in [
        "ArraySubscriptExpr",
        "MemberExpr",
        "BinaryOperator",
        "UnaryOperator",
        "UnaryExprOrTypeTraitExpr",
    ] {
        assert!(
            registry.expressions.iter().any(|item| item.kind == kind),
            "{kind}"
        );
    }
    assert!(registry
        .expressions
        .iter()
        .any(|item| item.span.provenance == "macro_expansion"));
    assert!(registry
        .expressions
        .iter()
        .any(|item| item.operator.as_deref() == Some("sizeof")));
    assert!(registry
        .expressions
        .iter()
        .any(|item| item.kind == "MemberExpr" && item.access_syntax.as_deref() == Some("dot")));
    let mallocs: Vec<_> = registry
        .expressions
        .iter()
        .filter(|item| item.call_kind.as_deref() == Some("malloc"))
        .collect();
    assert!(mallocs.len() >= 3);
    assert_ne!(mallocs[1].id, mallocs[2].id);
    assert!(registry
        .declarations
        .iter()
        .any(|item| item.initializer.is_some()));
    assert!(
        registry
            .declarations
            .iter()
            .filter(|item| item.name == "local_counter")
            .count()
            >= 3
    );
    let mut registry = registry;
    let event_at = |line: u32, event_type| c_trace_engine::models::HeapEvent {
        raw_caller_address: Some("0x401001".into()),
        sequence: line.into(),
        pid: 1,
        allocation_id: 1,
        event_type,
        address: Some("0x1000".into()),
        size_bytes: Some(8),
        calloc_count: None,
        calloc_element_size: None,
        source_file: Some(fixture("ast_source.c").display().to_string()),
        source_line: Some(line),
        old_allocation_id: None,
        heap_usage_after: 8,
        heap_usage_complete: Some(true),
        detail: None,
        attribution: Some(c_trace_engine::models::HeapEventAttribution {
            call_site_resolution: "resolved_return_address_minus_one".into(),
            ..Default::default()
        }),
    };
    let events = [
        event_at(11, c_trace_engine::models::HeapEventType::Allocated),
        event_at(12, c_trace_engine::models::HeapEventType::Reallocated),
        event_at(18, c_trace_engine::models::HeapEventType::Freed),
    ];
    clang_ast::correlate_heap_calls(
        &mut registry,
        &events,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    );
    assert_eq!(registry.heap_call_candidates.len(), 1);
    assert_eq!(registry.heap_call_candidates[0].event_index, 1);
}

#[tokio::test]
async fn optional_ast_does_not_change_runtime_or_break_trace_parser() {
    for tool in ["clang", "gcc", "gdb", "addr2line"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let mut config = TraceConfig::default();
    config.build.source_file = fixture("heap_provenance_realloc.c");
    config.tracing.mode = TraceMode::Source;
    config.tracing.static_disassembly = false;
    config.sanitizer_trigger = SanitizerTrigger::Never;
    config.limits.timeout_seconds = 60;
    config.limits.max_steps = 2000;
    let plain = trace_program(config.clone()).await.unwrap();
    config.clang_ast = true;
    let enriched = trace_program(config).await.unwrap();
    let registry = enriched.source_metadata.as_ref().unwrap();
    assert!(
        registry
            .translation_units
            .iter()
            .any(|unit| unit.status == "complete"),
        "{:?}",
        registry.translation_units
    );
    let event_shapes = |trace: &c_trace_engine::models::DebugSessionTrace| {
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
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(event_shapes(&plain), event_shapes(&enriched));
    assert_eq!(plain.execution.outcome, enriched.execution.outcome);
    assert_eq!(
        plain.recording.source_checkpoints,
        enriched.recording.source_checkpoints
    );
    let store = TraceStore::from_trace(enriched).unwrap();
    assert!(store.raw().source_metadata().is_some());
    assert!(store.semantic_input().source_metadata().is_some());
    assert!(
        store.validate_deep().is_structurally_sound(),
        "{:#?}",
        store.validate_deep()
    );
}

#[tokio::test]
async fn missing_clang_records_failure_without_claiming_complete_ast() {
    let build = c_trace_engine::config::BuildConfig {
        source_file: fixture("ast_source.c"),
        ..Default::default()
    };
    let (registry, _) = clang_ast::analyze(&build, "/nonexistent/clang", None).await;
    assert_eq!(registry.translation_units[0].status, "unavailable");
    assert!(registry.translation_units[0].reason.is_some());
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            return;
        }
    }
    let mut config = TraceConfig::default();
    config.build.source_file = fixture("ast_source.c");
    config.clang_ast = true;
    config.clang_path = Some("/nonexistent/clang".into());
    config.tracing.mode = TraceMode::Source;
    config.sanitizer_trigger = SanitizerTrigger::Never;
    config.limits.timeout_seconds = 45;
    let trace = trace_program(config).await.unwrap();
    assert!(trace.compiled_successfully);
    assert!(!trace.snapshots.is_empty());
    assert_eq!(
        trace.source_metadata.as_ref().unwrap().translation_units[0].status,
        "unavailable"
    );
    assert!(
        !trace.evidence_fusion.as_ref().unwrap().ast_available,
        "all-failed Clang extraction cannot advertise AST evidence"
    );
}

#[tokio::test]
async fn malformed_ast_and_output_cap_degrade_without_panicking() {
    let build = c_trace_engine::config::BuildConfig {
        source_file: fixture("ast_source.c"),
        ..Default::default()
    };
    let (malformed, _) = clang_ast::analyze(&build, "/bin/echo", None).await;
    assert_eq!(malformed.translation_units[0].status, "unavailable");
    assert!(malformed.translation_units[0]
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("AST JSON"));

    let dir = std::env::temp_dir().join(format!("ast-cap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let executable = dir.join("large-clang");
    std::fs::write(&executable, "#!/bin/sh\nhead -c 9437184 /dev/zero\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&executable, permissions).unwrap();
    let (oversized, _) = clang_ast::analyze(&build, executable.to_str().unwrap(), None).await;
    assert_eq!(oversized.translation_units[0].status, "unavailable");
    assert!(oversized.translation_units[0]
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("output cap"));
    std::fs::remove_file(&executable).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[tokio::test]
async fn unsupported_gnu_expression_reports_partial_extraction() {
    if c_trace_engine::compiler::probe_tool("clang", "--version")
        .await
        .is_err()
    {
        return;
    }
    let build = c_trace_engine::config::BuildConfig {
        source_file: fixture("ast_gnu.c"),
        ..Default::default()
    };
    let (registry, _) = clang_ast::analyze(&build, "clang", None).await;
    assert_eq!(registry.translation_units[0].status, "partial");
    assert!(registry.translation_units[0].unsupported_nodes > 0);
    assert!(!registry.expressions.is_empty());
}

#[tokio::test]
async fn gcc_only_flag_does_not_suppress_runtime_trace() {
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let mut config = TraceConfig::default();
    config.build.source_file = fixture("ast_source.c");
    config
        .build
        .compiler_flags
        .push("-fno-tree-scev-cprop".into());
    config.clang_ast = true;
    config.sanitizer_trigger = SanitizerTrigger::Never;
    config.tracing.mode = TraceMode::Source;
    config.limits.timeout_seconds = 45;
    let trace = trace_program(config).await.unwrap();
    assert!(trace.compiled_successfully);
    assert!(!trace.snapshots.is_empty());
    assert_eq!(
        trace.source_metadata.as_ref().unwrap().translation_units[0].status,
        "unavailable"
    );
}

#[tokio::test]
async fn accepted_compiler_specific_flags_retain_ast_without_cross_provider_identity() {
    for tool in ["gcc", "gdb", "clang"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let mut config = TraceConfig::default();
    config.build.source_file = fixture("ast_source.c");
    config.build.compiler_flags.push("-fshort-enums".into());
    config.clang_ast = true;
    config.tracing.mode = TraceMode::Source;
    config.sanitizer_trigger = SanitizerTrigger::Never;
    config.limits.timeout_seconds = 60;
    let trace = trace_program(config).await.unwrap();
    let registry = trace.source_metadata.as_ref().unwrap();
    assert!(
        !registry.declarations.is_empty(),
        "AST source evidence is retained"
    );
    assert!(registry
        .translation_units
        .iter()
        .all(|unit| unit.status == "complete"));
    assert!(
        registry
            .declarations
            .iter()
            .all(|decl| decl.dwarf_declaration.is_none()),
        "Clang accepting a GCC -f flag does not prove equivalent compiled declarations"
    );
    assert!(registry.heap_call_candidates.is_empty());
    assert!(trace
        .snapshots
        .iter()
        .flat_map(|snapshot| snapshot.variables.iter())
        .all(|var| var.source_declaration_ref.is_none()));
}

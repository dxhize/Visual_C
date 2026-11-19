//! Optional, bounded Clang JSON AST source evidence. GCC/DWARF/GDB remain
//! authoritative for compiled layout and runtime behavior.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;

use crate::config::BuildConfig;
use crate::dwarf::TypeRegistry;

const MAX_TUS: usize = 16;
const MAX_JSON: usize = 8 * 1024 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 96;
const MAX_DECLS: usize = 2048;
const MAX_EXPRS: usize = 4096;
const MAX_TEXT: usize = 256;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceMetadataRegistry {
    pub provider: String,
    pub provider_version: Option<String>,
    /// Clang syntax remains useful when GCC/Clang option semantics cannot be
    /// shown equivalent, but it must not be linked to compiled/runtime facts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_provider_correlation_unavailable: Option<String>,
    pub translation_units: Vec<AstUnit>,
    pub declarations: Vec<SourceDeclaration>,
    pub expressions: Vec<SourceExpression>,
    /// Source-line candidates, never proof of the runtime expression executed.
    pub heap_call_candidates: Vec<HeapCallCandidate>,
    /// Explicit source-line ambiguities. Never an event-to-expression proof.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heap_call_ambiguities: Vec<HeapCallAmbiguity>,
    pub declaration_correlation_time_us: u64,
    pub heap_correlation_time_us: u64,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapCallCandidate {
    pub event_index: usize,
    pub expression_id: String,
    pub evidence: String,
}

/// A resolved heap-event line with more than one compatible direct call, or
/// only macro-expanded calls. Recorded instead of silently dropping it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapCallAmbiguity {
    pub event_index: usize,
    /// multiple_compatible_calls_on_resolved_line or
    /// only_macro_expanded_calls_on_resolved_line.
    pub evidence: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expression_ids: Vec<String>,
}

/// One unique DWARF member selected by a parent type identity and member
/// name. The index is into the serialized parent member list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DwarfMemberRef {
    pub type_ref: String,
    pub member_index: usize,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AstUnit {
    pub id: String,
    pub file: String,
    pub status: String,
    pub ast_bytes: usize,
    pub nodes_visited: usize,
    pub unsupported_nodes: usize,
    pub parse_time_us: u64,
    pub registry_time_us: u64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpan {
    pub file: String,
    pub start_offset: u64,
    pub end_offset: u64,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub provenance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spelling_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spelling_offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDeclaration {
    pub id: String,
    pub unit_id: String,
    pub kind: String,
    pub name: String,
    pub type_spelling: Option<String>,
    pub span: SourceSpan,
    pub name_line: Option<u32>,
    pub name_column: Option<u32>,
    pub function: Option<String>,
    pub initializer: Option<String>,
    pub dwarf_declaration: Option<String>,
    pub correlation: String,
    /// Optional DWARF type of the correlated declaration (or member type for
    /// a FieldDecl), only when it resolves in the same registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dwarf_type_ref: Option<String>,
    /// Unique DWARF member for a FieldDecl; never inferred from its name alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dwarf_member: Option<DwarfMemberRef>,
    /// Clang spelling of the containing struct/union for a FieldDecl.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_type_spelling: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceExpression {
    pub id: String,
    pub unit_id: String,
    pub kind: String,
    pub span: SourceSpan,
    pub type_spelling: Option<String>,
    pub operator: Option<String>,
    pub access_syntax: Option<String>,
    pub callee: Option<String>,
    /// Standard allocator only if a direct declaration in a system header was seen.
    pub call_kind: Option<String>,
    pub operand_ids: Vec<String>,
    pub referenced_declaration: Option<String>,
    pub function: Option<String>,
    pub sizeof_operand: Option<String>,
    pub sizeof_operand_kind: Option<String>,
    /// DWARF declaration reached through `referenced_declaration`, when that
    /// source declaration has a unique DWARF declaration link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referenced_dwarf_declaration: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dwarf_type_ref: Option<String>,
    /// How `dwarf_type_ref` was selected; never runtime execution evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dwarf_type_evidence: Option<String>,
    /// Unique DWARF member for a MemberExpr.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_ref: Option<DwarfMemberRef>,
    /// Clang member name spelling, independent of DWARF resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_name: Option<String>,
    /// Clang base-expression type spelling (for example `struct Node *`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_type_spelling: Option<String>,
}

fn bounded(value: &str) -> Option<String> {
    (value.len() <= MAX_TEXT).then(|| value.to_owned())
}

fn canonical(path: &Path, cwd: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_owned()
    } else {
        cwd.join(path)
    };
    joined.canonicalize().unwrap_or(joined)
}

/// A compiler option with possible preprocessing effects must not be dropped.
/// Reject unknown flags rather than analyzing a different source program.
pub fn clang_flags(build: &BuildConfig) -> std::result::Result<Vec<String>, String> {
    let mut result = Vec::new();
    for define in &build.defines {
        result.push(format!("-D{define}"));
    }
    for include in &build.include_dirs {
        result.push(format!("-I{}", include.display()));
    }
    let mut iter = build.compiler_flags.iter();
    while let Some(flag) = iter.next() {
        if [
            "-std", "-x", "-isystem", "-iquote", "-include", "-imacros", "-D", "-U", "-I",
        ]
        .contains(&flag.as_str())
        {
            let value = iter
                .next()
                .ok_or_else(|| format!("compiler flag {flag} requires an argument"))?;
            result.extend([flag.clone(), value.clone()]);
        } else if flag.starts_with("-std=")
            || flag == "-ansi"
            || flag == "-pthread"
            || flag.starts_with("-I")
            || flag.starts_with("-D")
            || flag.starts_with("-U")
            || flag.starts_with("-isystem")
            || flag.starts_with("-iquote")
            || flag.starts_with("-include")
            || flag.starts_with("-imacros")
            || flag.starts_with("-f")
            || flag.starts_with("-m")
        {
            // Clang validates these options; rejection makes this TU unavailable.
            result.push(flag.clone());
        } else if ["-O0", "-O1", "-O2", "-O3", "-Og", "-Os", "-Ofast"].contains(&flag.as_str()) {
            // Optimization controls built-in preprocessor macros (e.g. __OPTIMIZE__).
            result.push(flag.clone());
        } else if ["-g", "-g3"].contains(&flag.as_str()) || flag.starts_with("-W") {
            // Do not pass warning-as-error policies to a supplementary parser.
        } else {
            return Err(format!(
                "unclassified GCC compiler flag {flag}: AST skipped"
            ));
        }
    }
    Ok(result)
}

/// One pass per user TU, with a shared work bound and no generated wrapper input.
pub async fn analyze(
    build: &BuildConfig,
    clang: &str,
    dwarf: Option<&TypeRegistry>,
) -> (SourceMetadataRegistry, u64) {
    let started = Instant::now();
    let budget_end = started + Duration::from_secs(8);
    let mut registry = SourceMetadataRegistry {
        provider: "clang-ast-json".into(),
        ..Default::default()
    };
    registry.provider_version = crate::compiler::probe_tool(clang, "--version")
        .await
        .ok()
        .and_then(|version| bounded(&version));
    let cwd = build
        .working_directory
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let sources = build.user_sources();
    let flags = clang_flags(build);
    if let Some(flag) = build.compiler_flags.iter().find(|flag| {
        flag.starts_with("-f") || flag.starts_with("-m") || flag.as_str() == "-pthread"
    }) {
        let reason = format!("GCC/Clang semantics for {flag} not verified");
        registry.cross_provider_correlation_unavailable = Some(reason.clone());
        registry.limitations.push(reason);
    }
    if sources.len() > MAX_TUS {
        registry.limitations.push(format!(
            "only {MAX_TUS} of {} translation units analyzed",
            sources.len()
        ));
    }
    for (position, source) in sources.into_iter().take(MAX_TUS).enumerate() {
        let path = canonical(&source, &cwd);
        let unit_id = format!("tu{position}");
        let mut unit = AstUnit {
            id: unit_id.clone(),
            file: path.display().to_string(),
            status: "unavailable".into(),
            ..Default::default()
        };
        let before_records = registry.declarations.len() + registry.expressions.len();
        if Instant::now() >= budget_end
            || crate::supervisor::remaining().is_zero()
            || crate::supervisor::cancelled()
        {
            unit.reason = Some("AST total budget/deadline exhausted".into());
            registry.translation_units.push(unit);
            registry
                .limitations
                .push("remaining translation units not analyzed".into());
            break;
        }
        let result: std::result::Result<(), String> = async {
            let flags = flags.as_ref().map_err(Clone::clone)?;
            let text = std::fs::read(&path).map_err(|e| e.to_string())?;
            if text.len() > 1024 * 1024 {
                return Err("source exceeds 1 MiB source-location budget".into());
            }
            let mut cmd = Command::new(clang);
            cmd.args([
                "-Xclang",
                "-ast-dump=json",
                "-fsyntax-only",
                "-x",
                "c",
                "-O0",
            ])
            .args(flags)
            .arg(&path)
            .current_dir(&cwd);
            // Tool-only budget; never apply this address-space cap to GDB or
            // to the GCC-produced inferior. The existing helper deliberately
            // treats unsupported rlimits as best effort.
            let ast_limits = crate::resource_limits::ExecutionLimits {
                max_memory_mb: 512,
                timeout_seconds: 3,
                ..Default::default()
            };
            crate::resource_limits::apply_pre_exec_limits(&mut cmd, &ast_limits);
            let output = crate::supervisor::output(
                &mut cmd,
                crate::supervisor::stage_deadline(3).min(budget_end),
                MAX_JSON,
            )
            .await
            .map_err(|e| format!("Clang invocation: {e}"))?;
            if output.timed_out || output.cancelled || output.output_truncated {
                return Err("Clang timed out, cancelled, or exceeded 8 MiB output cap".into());
            }
            if !output.status.is_some_and(|status| status.success()) {
                return Err(format!(
                    "Clang rejected source: {}",
                    String::from_utf8_lossy(&output.stderr)
                        .chars()
                        .take(MAX_TEXT)
                        .collect::<String>()
                ));
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("argument unused during compilation")
                || stderr.contains("not supported")
            {
                return Err(format!(
                    "Clang ignored a configured flag: {}",
                    stderr.chars().take(MAX_TEXT).collect::<String>()
                ));
            }
            unit.ast_bytes = output.stdout.len();
            let parse_started = Instant::now();
            let document: Value = match serde_json::from_slice(&output.stdout) {
                Ok(document) => document,
                Err(error) => {
                    unit.parse_time_us = parse_started.elapsed().as_micros() as u64;
                    return Err(format!("malformed/unsupported AST JSON: {error}"));
                }
            };
            unit.parse_time_us = parse_started.elapsed().as_micros() as u64;
            let extraction_started = Instant::now();
            let extracted = extract(
                &document,
                &path,
                &text,
                &unit_id,
                &mut unit,
                &mut registry,
                budget_end,
            );
            unit.registry_time_us = extraction_started.elapsed().as_micros() as u64;
            extracted?;
            if unit.unsupported_nodes != 0 {
                unit.status = "partial".into();
                unit.reason = Some(format!(
                    "{} unsupported source-expression nodes; supported children retained",
                    unit.unsupported_nodes
                ));
            } else {
                unit.status = "complete".into();
            }
            Ok(())
        }
        .await;
        if let Err(reason) = result {
            unit.reason =
                bounded(&reason).or_else(|| Some(reason.chars().take(MAX_TEXT).collect()));
            unit.status =
                if registry.declarations.len() + registry.expressions.len() > before_records {
                    "partial"
                } else {
                    "unavailable"
                }
                .into();
        }
        registry.translation_units.push(unit);
    }
    let correlation_started = Instant::now();
    let compatible = registry.cross_provider_correlation_unavailable.is_none();
    correlate_declarations(&mut registry, dwarf.filter(|_| compatible), &cwd);
    registry.declaration_correlation_time_us = correlation_started.elapsed().as_micros() as u64;
    (registry, started.elapsed().as_micros() as u64)
}

fn spot<'a>(location: &'a Value, key: &str) -> &'a Value {
    location.get(key).unwrap_or(location)
}

struct LineIndex {
    source_len: usize,
    newlines: Vec<usize>,
}

impl LineIndex {
    fn new(source: &[u8]) -> Self {
        Self {
            source_len: source.len(),
            newlines: source
                .iter()
                .enumerate()
                .filter_map(|(index, &byte)| (byte == b'\n').then_some(index))
                .collect(),
        }
    }

    fn line_column(&self, offset: usize) -> Option<(u32, u32)> {
        if offset >= self.source_len {
            return None;
        }
        let previous = self.newlines.partition_point(|&index| index < offset);
        let column = if previous == 0 {
            offset + 1
        } else {
            offset - self.newlines[previous - 1]
        };
        Some((
            u32::try_from(previous + 1).ok()?,
            u32::try_from(column).ok()?,
        ))
    }
}

fn span(node: &Value, path: &Path, lines: &LineIndex) -> Option<SourceSpan> {
    let range = node.get("range")?;
    let raw_start = range.get("begin")?;
    let raw_end = range.get("end")?;
    let start = spot(raw_start, "expansionLoc");
    let end = spot(raw_end, "expansionLoc");
    let offset = start.get("offset")?.as_u64()?;
    let end_offset = end
        .get("offset")?
        .as_u64()?
        .checked_add(end.get("tokLen").and_then(Value::as_u64).unwrap_or(1))?;
    let start_pos = usize::try_from(offset).ok()?;
    let end_pos = usize::try_from(end_offset).ok()?;
    if start_pos >= lines.source_len || end_pos > lines.source_len || end_pos <= start_pos {
        return None;
    }
    let file = start.get("file").and_then(Value::as_str);
    let declaration_location = node.get("loc").unwrap_or(&Value::Null);
    if start.get("includedFrom").is_some()
        || end.get("includedFrom").is_some()
        || declaration_location.get("includedFrom").is_some()
        || declaration_location
            .get("file")
            .and_then(Value::as_str)
            .is_some_and(|name| Path::new(name).canonicalize().ok().as_deref() != Some(path))
        || file.is_some_and(|name| Path::new(name).canonicalize().ok().as_deref() != Some(path))
        || end
            .get("file")
            .and_then(Value::as_str)
            .is_some_and(|name| Path::new(name).canonicalize().ok().as_deref() != Some(path))
    {
        return None;
    }
    let (start_line, start_column) = lines.line_column(start_pos)?;
    let (end_line, end_column) = lines.line_column(end_pos - 1)?;
    let spelling = raw_start.get("spellingLoc");
    Some(SourceSpan {
        file: path.display().to_string(),
        start_offset: offset,
        end_offset,
        start_line,
        start_column,
        end_line,
        end_column,
        provenance: if spelling.is_some() {
            "macro_expansion".into()
        } else {
            "direct".into()
        },
        spelling_file: spelling
            .and_then(|loc| loc.get("file"))
            .and_then(Value::as_str)
            .and_then(bounded),
        spelling_offset: spelling
            .and_then(|loc| loc.get("offset"))
            .and_then(Value::as_u64),
    })
}

fn inner(node: &Value) -> &[Value] {
    node.get("inner")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn kind(node: &Value) -> &str {
    node.get("kind").and_then(Value::as_str).unwrap_or("")
}

fn raw_id(node: &Value) -> Option<&str> {
    node.get("id").and_then(Value::as_str)
}

fn direct_callee(node: &Value) -> Option<(&str, &str)> {
    let mut child = inner(node).first()?;
    for _ in 0..8 {
        if kind(child) == "DeclRefExpr" {
            let reference = child.get("referencedDecl")?;
            return Some((
                reference.get("name")?.as_str()?,
                reference.get("id")?.as_str()?,
            ));
        }
        child = inner(child).first()?;
    }
    None
}

fn selected_expression(node: &Value) -> bool {
    matches!(
        kind(node),
        "CallExpr"
            | "UnaryExprOrTypeTraitExpr"
            | "BinaryOperator"
            | "CompoundAssignOperator"
            | "UnaryOperator"
            | "ArraySubscriptExpr"
            | "MemberExpr"
            | "InitListExpr"
            | "CStyleCastExpr"
            | "ConditionalOperator"
            | "DeclRefExpr"
            | "IntegerLiteral"
            | "FloatingLiteral"
            | "StringLiteral"
            | "CharacterLiteral"
    )
}

fn visible_child_ids(node: &Value) -> Vec<String> {
    let mut result = Vec::new();
    let mut pending: Vec<&Value> = inner(node).iter().rev().collect();
    let mut budget = 0;
    while let Some(child) = pending.pop() {
        budget += 1;
        if budget > 64 {
            break;
        }
        if selected_expression(child) {
            if let Some(id) = raw_id(child) {
                result.push(id.into());
            }
        } else {
            pending.extend(inner(child).iter().rev());
        }
    }
    result
}

fn extract(
    root: &Value,
    path: &Path,
    source: &[u8],
    unit_id: &str,
    unit: &mut AstUnit,
    registry: &mut SourceMetadataRegistry,
    budget_end: Instant,
) -> std::result::Result<(), String> {
    let lines = LineIndex::new(source);
    let mut header_functions = HashSet::new();
    for node in inner(root) {
        if kind(node) == "FunctionDecl" && span(node, path, &lines).is_none() {
            if let Some(id) = raw_id(node) {
                header_functions.insert(id);
            }
        }
    }
    let mut declaration_ids: HashMap<String, String> = HashMap::new();
    let mut expression_ids: HashMap<String, String> = HashMap::new();
    let mut deferred: Vec<(usize, Vec<String>, Option<String>)> = Vec::new();
    let mut initializers: Vec<(usize, String)> = Vec::new();
    let mut stack: Vec<(&Value, usize, Option<String>, Option<String>)> = inner(root)
        .iter()
        .rev()
        .map(|node| (node, 1, None, None))
        .collect();
    while let Some((node, depth, function, record)) = stack.pop() {
        unit.nodes_visited += 1;
        if unit.nodes_visited > MAX_NODES || depth > MAX_DEPTH {
            return Err("AST node/depth budget exceeded".into());
        }
        if unit.nodes_visited.is_multiple_of(1024)
            && (Instant::now() >= budget_end
                || crate::supervisor::remaining().is_zero()
                || crate::supervisor::cancelled())
        {
            return Err("AST traversal deadline or cancellation".into());
        }
        let Some(location) = span(node, path, &lines) else {
            continue; // never traverse header declarations into user records
        };
        let node_kind = kind(node);
        if (node_kind.ends_with("Expr")
            || node_kind.ends_with("Operator")
            || node_kind.ends_with("Literal"))
            && !selected_expression(node)
            && !matches!(
                node_kind,
                "ImplicitCastExpr" | "ParenExpr" | "ConstantExpr" | "OpaqueValueExpr"
            )
        {
            unit.unsupported_nodes += 1;
        }
        let function = if node_kind == "FunctionDecl" {
            node.get("name")
                .and_then(Value::as_str)
                .and_then(bounded)
                .or(function)
        } else {
            function
        };
        // Only the nearest named record supplies the FieldDecl parent spelling.
        let record = if node_kind == "RecordDecl" {
            node.get("name")
                .and_then(Value::as_str)
                .and_then(bounded)
                .map(|name| {
                    let tag = node
                        .get("tagUsed")
                        .and_then(Value::as_str)
                        .unwrap_or("struct");
                    format!("{tag} {name}")
                })
        } else {
            record
        };
        if matches!(
            node_kind,
            "VarDecl" | "ParmVarDecl" | "FunctionDecl" | "FieldDecl"
        ) {
            let name = node.get("name").and_then(Value::as_str).unwrap_or("");
            if let Some(name) = bounded(name) {
                if registry.declarations.len() >= MAX_DECLS {
                    return Err("AST declaration retention budget exceeded".into());
                }
                let id = format!("{unit_id}:d{}", registry.declarations.len());
                if let Some(raw) = raw_id(node) {
                    declaration_ids.insert(raw.into(), id.clone());
                }
                let idx = registry.declarations.len();
                registry.declarations.push(SourceDeclaration {
                    id,
                    unit_id: unit_id.into(),
                    kind: node_kind.into(),
                    name,
                    type_spelling: node
                        .pointer("/type/qualType")
                        .and_then(Value::as_str)
                        .and_then(bounded),
                    span: location.clone(),
                    name_line: node
                        .get("loc")
                        .map(|loc| spot(loc, "expansionLoc"))
                        .and_then(|loc| loc.get("offset"))
                        .and_then(Value::as_u64)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .and_then(|offset| lines.line_column(offset).map(|(line, _)| line)),
                    name_column: node
                        .get("loc")
                        .map(|loc| spot(loc, "expansionLoc"))
                        .and_then(|loc| loc.get("offset"))
                        .and_then(Value::as_u64)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .and_then(|offset| lines.line_column(offset).map(|(_, column)| column)),
                    function: function.clone(),
                    initializer: None,
                    dwarf_declaration: None,
                    correlation: "unresolved".into(),
                    dwarf_type_ref: None,
                    dwarf_member: None,
                    parent_type_spelling: if node_kind == "FieldDecl" {
                        record.clone()
                    } else {
                        None
                    },
                });
                if node_kind == "VarDecl" && !inner(node).is_empty() {
                    if let Some(raw) = inner(node).last().and_then(|child| {
                        if selected_expression(child) {
                            raw_id(child).map(str::to_owned)
                        } else {
                            visible_child_ids(child).into_iter().next()
                        }
                    }) {
                        initializers.push((idx, raw));
                    }
                }
            }
        }
        if selected_expression(node) {
            if registry.expressions.len() >= MAX_EXPRS {
                return Err("AST expression retention budget exceeded".into());
            }
            let id = format!("{unit_id}:e{}", registry.expressions.len());
            if let Some(raw) = raw_id(node) {
                expression_ids.insert(raw.into(), id.clone());
            }
            let (callee, callee_id) = if node_kind == "CallExpr" {
                direct_callee(node)
                    .map(|(name, id)| (bounded(name), Some(id)))
                    .unwrap_or((None, None))
            } else {
                (None, None)
            };
            let call_kind = if let (Some(name), Some(raw)) = (&callee, callee_id) {
                if ["malloc", "calloc", "realloc", "free"].contains(&name.as_str())
                    && header_functions.contains(raw)
                {
                    Some(name.clone())
                } else {
                    Some("direct_other".into())
                }
            } else if node_kind == "CallExpr" {
                Some("indirect_or_unknown".into())
            } else {
                None
            };
            let idx = registry.expressions.len();
            let reference = node
                .pointer("/referencedDecl/id")
                .or_else(|| node.get("referencedMemberDecl"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let children = visible_child_ids(node);
            deferred.push((idx, children, reference));
            registry.expressions.push(SourceExpression {
                id,
                unit_id: unit_id.into(),
                kind: node_kind.into(),
                span: location,
                type_spelling: node
                    .pointer("/type/qualType")
                    .and_then(Value::as_str)
                    .and_then(bounded),
                operator: node
                    .get("opcode")
                    .or_else(|| {
                        node.get("name")
                            .filter(|_| node_kind == "UnaryExprOrTypeTraitExpr")
                    })
                    .and_then(Value::as_str)
                    .and_then(bounded),
                access_syntax: (node_kind == "MemberExpr").then(|| {
                    if node.get("isArrow").and_then(Value::as_bool) == Some(true) {
                        "arrow"
                    } else {
                        "dot"
                    }
                    .into()
                }),
                callee,
                call_kind,
                operand_ids: Vec::new(),
                referenced_declaration: None,
                function: function.clone(),
                sizeof_operand: node
                    .pointer("/argType/qualType")
                    .and_then(Value::as_str)
                    .and_then(bounded),
                sizeof_operand_kind: (node_kind == "UnaryExprOrTypeTraitExpr").then(|| {
                    if node.get("argType").is_some() {
                        "type"
                    } else {
                        "expression"
                    }
                    .into()
                }),
                referenced_dwarf_declaration: None,
                dwarf_type_ref: None,
                dwarf_type_evidence: None,
                member_ref: None,
                member_name: (node_kind == "MemberExpr")
                    .then(|| node.get("name").and_then(Value::as_str).and_then(bounded))
                    .flatten(),
                base_type_spelling: (node_kind == "MemberExpr")
                    .then(|| {
                        inner(node)
                            .first()
                            .and_then(|base| base.pointer("/type/qualType"))
                            .and_then(Value::as_str)
                            .and_then(bounded)
                    })
                    .flatten(),
            });
        }
        for child in inner(node).iter().rev() {
            stack.push((child, depth + 1, function.clone(), record.clone()));
        }
    }
    for (idx, children, reference) in deferred {
        let expr = &mut registry.expressions[idx];
        expr.operand_ids = children
            .iter()
            .filter_map(|id| expression_ids.get(id).cloned())
            .collect();
        expr.referenced_declaration = reference
            .as_ref()
            .and_then(|id| declaration_ids.get(id).cloned());
    }
    for (idx, raw) in initializers {
        registry.declarations[idx].initializer = expression_ids.get(&raw).cloned();
    }
    Ok(())
}

fn prefer_declaration_die(
    candidates: Vec<&crate::dwarf::Declaration>,
) -> Vec<&crate::dwarf::Declaration> {
    let has_declaration = candidates
        .iter()
        .any(|decl| decl.kind.contains("subprogram") && decl.declaration_only);
    if has_declaration {
        candidates
            .into_iter()
            .filter(|decl| decl.kind.contains("subprogram") && decl.declaration_only)
            .collect()
    } else {
        candidates
    }
}

fn correlate_declarations(
    registry: &mut SourceMetadataRegistry,
    dwarf: Option<&TypeRegistry>,
    cwd: &Path,
) {
    let Some(dwarf) = dwarf else { return };
    type DeclarationKey = (String, u32, u32, String, Option<String>);
    type LineKey = (String, u32, String, Option<String>);
    let types: HashSet<&str> = dwarf.types.iter().map(|item| item.id.as_str()).collect();
    let mut by_location: HashMap<DeclarationKey, Vec<&crate::dwarf::Declaration>> = HashMap::new();
    let mut by_line: HashMap<LineKey, Vec<&crate::dwarf::Declaration>> = HashMap::new();
    for decl in &dwarf.declarations {
        let (Some(file), Some(line), Some(name)) = (&decl.file, decl.line, &decl.name) else {
            continue;
        };
        let function = if decl.kind.contains("subprogram") {
            None
        } else {
            decl.function.clone()
        };
        let file = canonical(Path::new(file), cwd).display().to_string();
        by_line
            .entry((file.clone(), line, name.clone(), function.clone()))
            .or_default()
            .push(decl);
        if let Some(column) = decl.column {
            by_location
                .entry((file, line, column, name.clone(), function))
                .or_default()
                .push(decl);
        }
    }
    let compatible = |source: &SourceDeclaration, decl: &crate::dwarf::Declaration| {
        if source.kind == "FunctionDecl" {
            decl.kind.contains("subprogram")
        } else if matches!(source.kind.as_str(), "VarDecl" | "ParmVarDecl") {
            matches!(decl.kind.as_str(), "variable" | "parameter")
        } else {
            // FieldDecl members are linked by parent type + member name in the
            // fusion pass, never by a coincident source location.
            false
        }
    };
    for source in &mut registry.declarations {
        source.dwarf_declaration = None;
        source.dwarf_type_ref = None;
        let function = if source.kind == "FunctionDecl" {
            None
        } else {
            source.function.clone()
        };
        let exact_key = source
            .name_line
            .zip(source.name_column)
            .map(|(line, column)| {
                (
                    source.span.file.clone(),
                    line,
                    column,
                    source.name.clone(),
                    function.clone(),
                )
            });
        let mut candidates: Vec<_> = exact_key
            .as_ref()
            .and_then(|key| by_location.get(key))
            .into_iter()
            .flat_map(|matches| matches.iter().copied())
            .filter(|decl| compatible(source, decl))
            .collect();
        let mut correlation = "unique_location";
        if candidates.is_empty() {
            // Only a genuinely missing column permits the line-only fallback.
            let line_key = source.name_line.map(|line| {
                (
                    source.span.file.clone(),
                    line,
                    source.name.clone(),
                    function.clone(),
                )
            });
            if let Some(key) = line_key {
                candidates = by_line
                    .get(&key)
                    .into_iter()
                    .flat_map(|matches| matches.iter().copied())
                    .filter(|decl| {
                        compatible(source, decl)
                            && (decl.column.is_none() || source.name_column.is_none())
                    })
                    .collect();
                if !candidates.is_empty() {
                    correlation = "unique_line_with_missing_column";
                }
            }
        }
        if source.kind == "FunctionDecl" {
            candidates = prefer_declaration_die(candidates);
        }
        match candidates.as_slice() {
            [only] => {
                source.dwarf_declaration = Some(only.id.clone());
                source.dwarf_type_ref = only
                    .type_ref
                    .as_deref()
                    .filter(|id| types.contains(id))
                    .map(str::to_owned);
                source.correlation = correlation.into();
            }
            [] => source.correlation = "unresolved".into(),
            _ => source.correlation = "ambiguous".into(),
        }
    }
}

/// No size/next-checkpoint matching: a unique call on the resolved source
/// line is only a candidate, not an exact event-to-expression association.
/// Calls are indexed by canonical source line once; multi-call and macro-only
/// lines remain explicit ambiguity records instead of disappearing.
pub fn correlate_heap_calls(
    registry: &mut SourceMetadataRegistry,
    events: &[crate::models::HeapEvent],
    cwd: &Path,
) {
    let started = Instant::now();
    registry.heap_call_candidates.clear();
    registry.heap_call_ambiguities.clear();
    type LineKey = (PathBuf, u32);
    let mut direct: HashMap<LineKey, Vec<usize>> = HashMap::new();
    let mut expanded: HashMap<LineKey, Vec<usize>> = HashMap::new();
    for (index, expr) in registry.expressions.iter().enumerate() {
        if expr.call_kind.is_none() {
            continue;
        }
        let file = canonical(Path::new(&expr.span.file), cwd);
        let bucket = if expr.span.provenance == "direct" {
            &mut direct
        } else {
            &mut expanded
        };
        // A call spanning an implausible number of lines is not attributed by
        // line identity; retain it only at its start line within a small bound.
        let last = expr
            .span
            .end_line
            .min(expr.span.start_line.saturating_add(64));
        for line in expr.span.start_line..=last {
            bucket.entry((file.clone(), line)).or_default().push(index);
        }
    }
    let compatible = |expr: &SourceExpression, event: &crate::models::HeapEvent| {
        matches!(
            (expr.call_kind.as_deref(), &event.event_type),
            (
                Some("malloc" | "calloc"),
                crate::models::HeapEventType::Allocated
            ) | (Some("realloc"), crate::models::HeapEventType::Reallocated)
                | (Some("free"), crate::models::HeapEventType::Freed)
        )
    };
    for (index, event) in events.iter().enumerate() {
        if index.is_multiple_of(64)
            && (started.elapsed() >= Duration::from_millis(250) || crate::supervisor::cancelled())
        {
            registry
                .limitations
                .push("heap-call candidate correlation stopped at 250 ms/cancellation".into());
            break;
        }
        let (Some(file), Some(line)) = (&event.source_file, event.source_line) else {
            continue;
        };
        if event.attribution.as_ref().is_none_or(|attribution| {
            attribution.call_site_resolution != "resolved_return_address_minus_one"
        }) {
            continue;
        }
        let key = (canonical(Path::new(file), cwd), line);
        let direct_ids: Vec<String> = direct
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|&position| {
                let expr = &registry.expressions[position];
                compatible(expr, event).then(|| expr.id.clone())
            })
            .collect();
        let expanded_ids: Vec<String> = expanded
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|&position| {
                let expr = &registry.expressions[position];
                compatible(expr, event).then(|| expr.id.clone())
            })
            .collect();
        if direct_ids.len() == 1 && expanded_ids.is_empty() {
            registry.heap_call_candidates.push(HeapCallCandidate {
                event_index: index,
                expression_id: direct_ids[0].clone(),
                evidence: "unique_direct_call_on_resolved_line_not_exact_execution".into(),
            });
            continue;
        }
        if direct_ids.is_empty() && expanded_ids.is_empty() {
            continue;
        }
        let evidence = if direct_ids.is_empty() {
            "only_macro_expanded_calls_on_resolved_line"
        } else {
            "multiple_compatible_calls_on_resolved_line"
        };
        let ids = direct_ids.into_iter().chain(expanded_ids);
        registry.heap_call_ambiguities.push(HeapCallAmbiguity {
            event_index: index,
            evidence: evidence.into(),
            // Output stays bounded; the record already states that the line
            // held more than one compatible call.
            expression_ids: ids.take(8).collect(),
        });
    }
    registry.heap_correlation_time_us = started.elapsed().as_micros() as u64;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_flags_are_not_silently_omitted() {
        let build = BuildConfig {
            compiler_flags: vec!["-unknown-semantic-flag".into()],
            ..Default::default()
        };
        assert!(clang_flags(&build).is_err());
    }
    #[test]
    fn malformed_json_and_unknown_nodes_are_safe() {
        assert!(serde_json::from_slice::<Value>(b"{").is_err());
        let mut registry = SourceMetadataRegistry::default();
        let mut unit = AstUnit::default();
        let path = Path::new("/tmp/example.c");
        let root = serde_json::json!({"inner":[{"kind":"UnknownFutureNode","range":{"begin":{"offset":0},"end":{"offset":0,"tokLen":1}}}]});
        extract(
            &root,
            path,
            b"x\n",
            "tu0",
            &mut unit,
            &mut registry,
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert!(registry.expressions.is_empty());
    }

    #[test]
    fn dwarf_link_requires_one_exact_file_line_and_column_match() {
        let file = "/tmp/ast-declaration-correlation.c";
        let source = SourceDeclaration {
            id: "tu0:d0".into(),
            unit_id: "tu0".into(),
            kind: "VarDecl".into(),
            name: "shadow".into(),
            type_spelling: Some("int".into()),
            span: SourceSpan {
                file: file.into(),
                start_offset: 1,
                end_offset: 7,
                start_line: 3,
                start_column: 7,
                end_line: 3,
                end_column: 12,
                provenance: "direct".into(),
                spelling_file: None,
                spelling_offset: None,
            },
            name_line: Some(3),
            name_column: Some(7),
            function: Some("f".into()),
            initializer: None,
            dwarf_declaration: None,
            correlation: "unresolved".into(),
            dwarf_type_ref: None,
            dwarf_member: None,
            parent_type_spelling: None,
        };
        let declaration = crate::dwarf::Declaration {
            id: "gcc:1".into(),
            unit_id: "cu0".into(),
            kind: "variable".into(),
            name: Some("shadow".into()),
            file: Some(file.into()),
            line: Some(3),
            column: Some(7),
            function: Some("f".into()),
            scope_id: "scope1".into(),
            type_ref: None,
            external: None,
            declaration_only: false,
            resolved_origin: None,
        };
        let mut registry = SourceMetadataRegistry {
            declarations: vec![source],
            ..Default::default()
        };
        let mut dwarf = TypeRegistry {
            declarations: vec![declaration.clone()],
            ..Default::default()
        };
        correlate_declarations(&mut registry, Some(&dwarf), Path::new("/tmp"));
        assert_eq!(
            registry.declarations[0].dwarf_declaration.as_deref(),
            Some("gcc:1")
        );
        dwarf.declarations.push(crate::dwarf::Declaration {
            id: "gcc:2".into(),
            ..declaration
        });
        correlate_declarations(&mut registry, Some(&dwarf), Path::new("/tmp"));
        assert_eq!(registry.declarations[0].correlation, "ambiguous");
        assert!(registry.declarations[0].dwarf_declaration.is_none());
    }

    fn source(kind: &str, line: u32, column: Option<u32>) -> SourceDeclaration {
        SourceDeclaration {
            id: "tu0:d0".into(),
            unit_id: "tu0".into(),
            kind: kind.into(),
            name: "shadow".into(),
            type_spelling: Some("int".into()),
            span: SourceSpan {
                file: "/tmp/phase7-correlation.c".into(),
                start_offset: 1,
                end_offset: 7,
                start_line: line,
                start_column: column.unwrap_or(1),
                end_line: line,
                end_column: column.unwrap_or(1) + 5,
                provenance: "direct".into(),
                spelling_file: None,
                spelling_offset: None,
            },
            name_line: Some(line),
            name_column: column,
            function: Some("f".into()),
            initializer: None,
            dwarf_declaration: None,
            correlation: "unresolved".into(),
            dwarf_type_ref: None,
            dwarf_member: None,
            parent_type_spelling: None,
        }
    }

    fn dwarf(
        id: &str,
        kind: &str,
        line: u32,
        column: Option<u32>,
        declaration_only: bool,
    ) -> crate::dwarf::Declaration {
        crate::dwarf::Declaration {
            id: id.into(),
            unit_id: "cu0".into(),
            kind: kind.into(),
            name: Some("shadow".into()),
            file: Some("/tmp/phase7-correlation.c".into()),
            line: Some(line),
            column,
            function: (kind != "subprogram").then(|| "f".into()),
            scope_id: "scope1".into(),
            type_ref: None,
            external: None,
            declaration_only,
            resolved_origin: None,
        }
    }

    #[test]
    fn missing_column_fallback_links_only_when_one_side_lacks_a_column() {
        let mut registry = SourceMetadataRegistry {
            declarations: vec![source("VarDecl", 3, None)],
            ..Default::default()
        };
        let dwarf = TypeRegistry {
            declarations: vec![dwarf("gcc:1", "variable", 3, Some(7), false)],
            ..Default::default()
        };
        correlate_declarations(&mut registry, Some(&dwarf), Path::new("/tmp"));
        assert_eq!(
            registry.declarations[0].correlation,
            "unique_line_with_missing_column"
        );
        assert_eq!(
            registry.declarations[0].dwarf_declaration.as_deref(),
            Some("gcc:1")
        );

        // With both columns present, a different column must not fall back to
        // the shared line.
        let mut both = SourceMetadataRegistry {
            declarations: vec![source("VarDecl", 3, Some(9))],
            ..Default::default()
        };
        correlate_declarations(&mut both, Some(&dwarf), Path::new("/tmp"));
        assert_eq!(both.declarations[0].correlation, "unresolved");
        assert!(both.declarations[0].dwarf_declaration.is_none());
    }

    #[test]
    fn field_decl_is_never_linked_by_coincident_location() {
        let mut registry = SourceMetadataRegistry {
            declarations: vec![source("FieldDecl", 3, Some(7))],
            ..Default::default()
        };
        let dwarf = TypeRegistry {
            declarations: vec![dwarf("gcc:1", "variable", 3, Some(7), false)],
            ..Default::default()
        };
        correlate_declarations(&mut registry, Some(&dwarf), Path::new("/tmp"));
        assert!(registry.declarations[0].dwarf_declaration.is_none());
        assert_eq!(registry.declarations[0].correlation, "unresolved");
    }

    #[test]
    fn function_correlation_prefers_a_declaration_die() {
        let mut registry = SourceMetadataRegistry {
            declarations: vec![source("FunctionDecl", 5, Some(5))],
            ..Default::default()
        };
        let dwarf = TypeRegistry {
            declarations: vec![
                dwarf("gcc:decl", "subprogram", 5, Some(5), true),
                dwarf("gcc:def", "subprogram", 5, Some(5), false),
            ],
            ..Default::default()
        };
        correlate_declarations(&mut registry, Some(&dwarf), Path::new("/tmp"));
        assert_eq!(
            registry.declarations[0].dwarf_declaration.as_deref(),
            Some("gcc:decl")
        );
        assert_eq!(registry.declarations[0].correlation, "unique_location");
    }
}

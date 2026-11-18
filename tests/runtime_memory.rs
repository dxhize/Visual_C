//! Phase 4 runtime evidence from real GCC/GDB sessions, not synthetic snapshots.
use c_trace_engine::config::{SanitizerTrigger, TraceMode};
use c_trace_engine::models::{
    DataValidity, DebugSessionTrace, ExecutionSnapshot, ExecutionStatus, VariableInfo,
};
use c_trace_engine::trace_parser::TraceStore;
use c_trace_engine::{processor, trace::trace_program, TraceConfig};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

static RECORDING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

async fn tools() -> Option<tokio::sync::SemaphorePermit<'static>> {
    let permit = RECORDING.acquire().await.ok()?;
    for tool in ["gcc", "gdb"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("SKIPPED Phase 4 runtime test: required tool {tool} unavailable");
            return None;
        }
    }
    Some(permit)
}

fn config(fixture: &str) -> TraceConfig {
    let mut c = TraceConfig::default();
    c.build.source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/c")
        .join(fixture);
    c.tracing.mode = TraceMode::Source;
    c.limits.timeout_seconds = 60;
    c.limits.max_steps = 20_000;
    c.limits.max_caller_frames = 8;
    c.limits.max_caller_variables = 64;
    c.sanitizer_trigger = SanitizerTrigger::Never;
    // These tests isolate debugger evidence; native diagnostic passes are tested in v2.rs.
    c.valgrind_path = Some("/nonexistent/phase4-no-valgrind".into());
    c
}

async fn run(c: TraceConfig) -> DebugSessionTrace {
    let trace = trace_program(c).await.unwrap();
    assert!(trace.compiled_successfully, "{:?}", trace.compile_errors);
    processor::validate(&trace).unwrap();
    trace
}

fn variables(s: &ExecutionSnapshot) -> impl Iterator<Item = &VariableInfo> {
    s.variables
        .iter()
        .chain(s.caller_frames.iter().flat_map(|f| &f.variables))
        .chain(&s.observation.globals)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn int_bytes(values: &[i32]) -> String {
    hex(&values
        .iter()
        .flat_map(|n| n.to_ne_bytes())
        .collect::<Vec<_>>())
}

fn assert_evidence(v: &VariableInfo) {
    let evidence = v.memory_evidence.as_ref().expect("runtime byte evidence");
    let captured = v.raw_bytes_hex.as_ref().map_or(0, |h| {
        assert_eq!(h.len() % 2, 0);
        assert!(h.bytes().all(|b| b.is_ascii_hexdigit()));
        h.len() / 2
    }) as u64;
    assert_eq!(evidence.captured_bytes, captured, "{}", v.name);
    assert!(
        captured <= evidence.requested_bytes,
        "{}: {evidence:?}",
        v.name
    );
    if let Some(extent) = evidence.extent_bytes {
        assert!(captured <= extent, "{}: {evidence:?}", v.name);
    }
    if evidence.status == "full" {
        assert_eq!(evidence.extent_bytes, Some(captured));
        assert_eq!(v.bytes_status, DataValidity::Valid);
    }
}

#[tokio::test]
async fn recursive_callers_keep_activation_identity_and_cross_frame_pointer_evidence() {
    let Some(_permit) = tools().await else { return };
    let trace = run(config("runtime_memory.c")).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let mut by_address: HashMap<&str, HashSet<u64>> = HashMap::new();
    let mut cross_frame = 0;
    let mut simultaneous_recursion = false;
    let mut shadowed = false;
    for s in &trace.snapshots {
        let recursive: Vec<_> = s
            .caller_frames
            .iter()
            .filter(|f| f.frame.func == "recurse")
            .collect();
        if recursive.len() >= 3 {
            let ids: HashSet<_> = recursive.iter().map(|f| f.activation_id).collect();
            assert_eq!(ids.len(), recursive.len());
            simultaneous_recursion = true;
        }
        for f in &s.caller_frames {
            assert!(f.frame.level > 0);
            assert!(s
                .call_stack
                .iter()
                .any(|actual| actual.level == f.frame.level && actual.func == f.frame.func));
            for v in &f.variables {
                assert_eq!(v.activation_id, Some(f.activation_id));
                assert_evidence(v);
                if v.name == "local" {
                    if let (Some(address), Some(identity)) =
                        (v.address.as_deref(), v.identity.as_deref())
                    {
                        assert!(identity.starts_with(&format!("activation:{}:", f.activation_id)));
                        by_address
                            .entry(address)
                            .or_default()
                            .insert(f.activation_id);
                    }
                }
            }
            let shadows: Vec<_> = f.variables.iter().filter(|v| v.name == "shadow").collect();
            if shadows.len() == 2 {
                shadowed = true;
                if shadows.iter().all(|v| v.address.is_some()) {
                    assert_ne!(shadows[0].address, shadows[1].address);
                    assert_ne!(shadows[0].identity, shadows[1].identity);
                    assert_ne!(shadows[0].lexical_scope_id, shadows[1].lexical_scope_id);
                    for shadow in shadows {
                        if let Some(bytes) = shadow.raw_bytes_hex.as_deref() {
                            let value = shadow
                                .value
                                .parse::<i32>()
                                .expect("initialized caller shadow");
                            assert_eq!(bytes, int_bytes(&[value]), "shadow value/address pairing");
                        }
                    }
                } else {
                    assert!(shadows.iter().all(|v| v.read_error.is_some()));
                }
            }
        }
        if s.func == "observe" {
            let p = s.variables.iter().find(|v| v.name == "incoming").unwrap();
            if let Some(owner) = s
                .caller_frames
                .iter()
                .flat_map(|f| &f.variables)
                .find(|v| v.address.is_some() && v.address == p.points_to)
            {
                let target = p.target.as_ref().expect("cross-frame pointer classified");
                assert_eq!(target.kind, "stack");
                assert_eq!(target.base_address, owner.address);
                assert_eq!(target.variable_identity, owner.identity);
                assert!(target.variable_identity.is_some());
                cross_frame += 1;
            }
        }
    }
    assert!(
        simultaneous_recursion,
        "three live recursive callers required"
    );
    assert!(
        shadowed,
        "caller shadow declarations must remain distinct or explicitly unresolved"
    );
    assert!(
        cross_frame > 0,
        "pointer must resolve against the same stop's caller storage"
    );
    assert!(
        by_address.values().any(|ids| ids.len() >= 2),
        "repeated calls must not reuse activation ids: {by_address:?}"
    );
    let encoded = serde_json::to_vec(&trace).unwrap();
    let decoded: DebugSessionTrace = serde_json::from_slice(&encoded).unwrap();
    let store = TraceStore::from_trace(decoded).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
    let raw = store.raw();
    let semantic = store.semantic_input();
    assert!(std::ptr::eq(raw.trace(), semantic.raw().trace()));
    assert!(raw
        .trace()
        .snapshots
        .iter()
        .any(|s| !s.caller_frames.is_empty()));
    let (index, snapshot) = raw
        .trace()
        .snapshots
        .iter()
        .enumerate()
        .find(|(_, s)| !s.caller_frames.is_empty())
        .unwrap();
    let context = semantic.checkpoint_context(index).unwrap();
    assert!(std::ptr::eq(
        context.variables(),
        snapshot.variables.as_slice()
    ));
    assert!(std::ptr::eq(
        context.caller_frames(),
        snapshot.caller_frames.as_slice()
    ));
}

#[tokio::test]
async fn aggregate_bytes_vlas_and_mutable_static_storage_refresh_at_each_stop() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory_aggregates.c");
    c.limits.max_aggregate_bytes = 16_384;
    let trace = run(c).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    let probes: Vec<_> = trace
        .snapshots
        .iter()
        .filter(|s| s.func == "probe")
        .collect();
    assert!(!probes.is_empty());
    let mut vla_sizes = HashSet::new();
    let mut vla_activations = HashSet::new();
    let mut global_bytes = HashSet::new();
    let mut static_bytes = HashSet::new();
    for s in probes {
        let caller = s
            .caller_frames
            .iter()
            .find(|f| f.frame.func == "capture")
            .expect("aggregate caller");
        let get = |name: &str| caller.variables.iter().find(|v| v.name == name).unwrap();
        for (name, expected) in [
            ("pairs", int_bytes(&[11, 12, 13, 14])),
            ("grid", int_bytes(&[1, 2, 3, 4, 5, 6])),
            ("bundle", int_bytes(&[21, 22, 23, 24, 25, 26])),
            (
                "overlays",
                hex(&[0x11223344u32.to_ne_bytes(), 0x55667788u32.to_ne_bytes()].concat()),
            ),
        ] {
            let v = get(name);
            assert_eq!(
                v.raw_bytes_hex.as_deref(),
                Some(expected.as_str()),
                "{name}"
            );
            assert_eq!(
                v.points_to, None,
                "aggregate containing pointers is not a pointer object"
            );
            assert_evidence(v);
            assert_eq!(v.memory_evidence.as_ref().unwrap().status, "full");
        }
        let pointers = get("pointers");
        assert_eq!(pointers.points_to, None);
        assert_eq!(
            pointers.raw_bytes_hex.as_ref().unwrap().len() as u64 / 2,
            pointers.runtime_size_bytes.unwrap()
        );
        assert_evidence(pointers);
        let dynamic = get("dynamic");
        let n = get("n").value.parse::<u64>().unwrap();
        assert_eq!(dynamic.runtime_size_bytes, Some(n * 4));
        assert_eq!(
            dynamic.memory_evidence.as_ref().unwrap().extent_bytes,
            Some(n * 4)
        );
        let bytes = dynamic.raw_bytes_hex.as_deref().unwrap();
        assert!(bytes.starts_with(&int_bytes(&[n as i32])));
        assert!(bytes.ends_with(&int_bytes(&[n as i32 + 1])));
        vla_sizes.insert(dynamic.runtime_size_bytes.unwrap());
        vla_activations.insert(caller.activation_id);
        let large = get("large");
        assert_eq!(large.runtime_size_bytes, Some(20_000));
        assert_evidence(large);
        let evidence = large.memory_evidence.as_ref().unwrap();
        assert_eq!(evidence.status, "truncated");
        assert_eq!(evidence.requested_bytes, 16_384);
        assert_eq!(evidence.captured_bytes, 16_384);
        assert!(large.raw_bytes_hex.as_deref().unwrap().starts_with("5a"));
        global_bytes.extend(
            s.observation
                .globals
                .iter()
                .filter(|v| v.name.ends_with("::mutable_global"))
                .filter_map(|v| v.raw_bytes_hex.clone()),
        );
        static_bytes.insert(get("mutable_static").raw_bytes_hex.clone().unwrap());
    }
    assert_eq!(vla_sizes, HashSet::from([12, 20]));
    assert_eq!(vla_activations.len(), 2);
    assert!(
        global_bytes.contains(&int_bytes(&[4, 2])) && global_bytes.contains(&int_bytes(&[9, 2])),
        "{global_bytes:?}"
    );
    assert!(
        static_bytes.contains(&int_bytes(&[3, 7])) && static_bytes.contains(&int_bytes(&[3, 12])),
        "{static_bytes:?}"
    );
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

#[tokio::test]
async fn global_pointer_arrays_retain_bytes_without_array_decay_pointer_targets() {
    let Some(_permit) = tools().await else { return };
    let trace = run(config("runtime_memory_globals.c")).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let mut observed = 0;
    let mut scalar_targets = HashSet::new();
    for s in &trace.snapshots {
        let globals = &s.observation.globals;
        let get = |name: &str| {
            globals
                .iter()
                .find(|v| v.name.ends_with(&format!("::{name}")))
                .unwrap()
        };
        let slots = get("slots");
        let address_bytes = |v: &VariableInfo| {
            usize::from_str_radix(v.address.as_deref().unwrap().trim_start_matches("0x"), 16)
                .unwrap()
                .to_ne_bytes()
        };
        let expected = hex(&[address_bytes(get("left")), address_bytes(get("right"))].concat());
        assert_eq!(slots.raw_bytes_hex.as_deref(), Some(expected.as_str()));
        assert_eq!(
            slots.runtime_size_bytes,
            Some((2 * std::mem::size_of::<usize>()) as u64)
        );
        assert_evidence(slots);
        assert_eq!(slots.memory_evidence.as_ref().unwrap().status, "full");
        assert_eq!(
            slots.points_to, None,
            "an array's decayed address is not a stored pointer value"
        );
        assert!(
            slots.target.is_none(),
            "global pointer arrays must not fabricate self-targets"
        );
        let scalar = get("scalar");
        let pointee = globals
            .iter()
            .find(|v| v.address.is_some() && v.address == scalar.points_to)
            .expect("scalar pointer must still resolve to its recorded global/static object");
        let target = scalar.target.as_ref().expect("scalar pointer target");
        assert_eq!(target.base_address, pointee.address);
        assert_eq!(target.variable_identity, pointee.identity);
        assert!(target.variable_identity.is_some());
        assert_eq!(target.offset_bytes, Some(0));
        scalar_targets.insert(pointee.name.as_str());
        observed += 1;
    }
    assert!(observed > 0);
    for name in ["left", "persistent", "right"] {
        assert!(
            scalar_targets
                .iter()
                .any(|target| target.ends_with(&format!("::{name}"))),
            "missing scalar pointer target {name}: {scalar_targets:?}"
        );
    }
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

#[tokio::test]
async fn globally_observed_function_static_keeps_identity_before_during_and_after_owner() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory_globals.c");
    // Automatic discovery learns function statics when their scope is first
    // inspected. An explicitly selected symbol is observable even before entry.
    c.tracing.globals.push("owner::persistent".into());
    let trace = run(c).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    let first_owner = trace
        .snapshots
        .iter()
        .position(|s| s.func == "owner")
        .unwrap();
    let last_owner = trace
        .snapshots
        .iter()
        .rposition(|s| s.func == "owner")
        .unwrap();
    let mut identities = HashSet::new();
    let mut addresses = HashSet::new();
    let mut phases = [false; 3];
    for (index, s) in trace.snapshots.iter().enumerate() {
        let persistent = s
            .observation
            .globals
            .iter()
            .find(|v| v.name.ends_with("::persistent"))
            .expect("function static discovered globally even when its owner is absent");
        identities.insert(
            persistent
                .identity
                .as_deref()
                .expect("canonical static identity"),
        );
        addresses.insert(
            persistent
                .address
                .as_deref()
                .expect("static storage address"),
        );
        assert_evidence(persistent);
        assert_eq!(persistent.memory_evidence.as_ref().unwrap().status, "full");
        let phase = if index < first_owner {
            0
        } else if index <= last_owner {
            1
        } else {
            2
        };
        phases[phase] = true;
        if phase == 0 {
            assert_eq!(
                persistent.raw_bytes_hex.as_deref(),
                Some(int_bytes(&[41]).as_str())
            );
        } else if phase == 2 {
            assert_eq!(
                persistent.raw_bytes_hex.as_deref(),
                Some(int_bytes(&[42]).as_str())
            );
        }
        if s.func == "owner" {
            let local = s.variables.iter().find(|v| v.name == "persistent").unwrap();
            assert_eq!(local.address, persistent.address);
            assert_eq!(
                local.identity, persistent.identity,
                "frame and global observations must share canonical static identity"
            );
        }
    }
    assert_eq!(
        phases, [true; 3],
        "must observe static before, inside and after its owner activation"
    );
    assert_eq!(addresses.len(), 1);
    assert_eq!(
        identities.len(),
        1,
        "live static identity must not depend on owner frame presence: {identities:?}"
    );
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

#[tokio::test]
async fn caller_frame_variable_and_extra_read_caps_are_bounded_and_zero_reads_is_unlimited() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory.c");
    c.limits.max_caller_frames = 1;
    c.limits.max_caller_variables = 2;
    let bounded = run(c).await;
    assert_eq!(bounded.execution_status, ExecutionStatus::Completed);
    assert!(bounded
        .snapshots
        .iter()
        .any(|s| !s.caller_frames.is_empty()));
    for s in &bounded.snapshots {
        assert!(s.caller_frames.len() <= 1);
        assert!(
            s.caller_frames
                .iter()
                .map(|f| f.variables.len())
                .sum::<usize>()
                <= 2
        );
    }
    assert!(bounded
        .snapshots
        .iter()
        .flat_map(|s| &s.caller_frames)
        .flat_map(|f| &f.errors)
        .any(|e| e.contains("max_caller_variables")));
    let mut c = config("runtime_memory.c");
    c.limits.max_extra_memory_reads = 1;
    let one = run(c).await;
    assert_eq!(one.execution_status, ExecutionStatus::Completed);
    assert!(one
        .snapshots
        .iter()
        .flat_map(|s| &s.caller_frames)
        .flat_map(|f| &f.variables)
        .any(|v| v
            .memory_evidence
            .as_ref()
            .is_some_and(|e| e.status == "unattempted")));
    for s in &one.snapshots {
        assert!(
            s.caller_frames
                .iter()
                .flat_map(|f| &f.variables)
                .filter(|v| v.raw_bytes_hex.is_some())
                .count()
                <= 1
        );
    }
    let mut c = config("runtime_memory.c");
    c.limits.max_extra_memory_reads = 0;
    let unlimited = run(c).await;
    assert_eq!(unlimited.execution_status, ExecutionStatus::Completed);
    assert!(
        unlimited.snapshots.iter().any(|s| s
            .caller_frames
            .iter()
            .flat_map(|f| &f.variables)
            .filter(|v| v.raw_bytes_hex.is_some())
            .count()
            > 1),
        "zero must remove the additional-read command cap, not disable capture"
    );
}

#[tokio::test]
async fn snapshot_memory_budget_is_shared_by_frame_zero_callers_and_globals() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory_aggregates.c");
    c.limits.max_snapshot_memory_bytes = 64;
    let trace = run(c).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    let mut truncated = false;
    for s in &trace.snapshots {
        let used: usize = variables(s)
            .filter_map(|v| v.raw_bytes_hex.as_ref())
            .map(|h| h.len() / 2)
            .sum::<usize>()
            + s.heap_blocks
                .iter()
                .filter_map(|b| b.raw_bytes_hex.as_ref())
                .map(|h| h.len() / 2)
                .sum::<usize>();
        assert!(used <= 64, "{}:{} captured {used} bytes", s.func, s.line);
        if s.func == "probe" {
            truncated |= s.memory_truncated;
            assert!(variables(s).any(|v| v
                .memory_evidence
                .as_ref()
                .is_some_and(|e| e.status == "unattempted" || e.status == "truncated")));
        }
    }
    assert!(truncated);
}

#[tokio::test]
async fn global_capture_reports_shared_budget_exhaustion_after_full_local_capture() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory_global_budget.c");
    c.limits.max_caller_frames = 0;
    c.limits.max_variables_per_snapshot = 1;
    c.limits.max_snapshot_memory_bytes = 8;
    let trace = run(c).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let return_line = include_str!("../fixtures/c/runtime_memory_global_budget.c")
        .lines()
        .position(|line| line.contains("return only"))
        .unwrap() as u32
        + 1;
    let initialized: Vec<_> = trace
        .snapshots
        .iter()
        .filter(|s| s.func == "main" && s.line == return_line)
        .collect();
    assert!(
        !initialized.is_empty(),
        "checkpoint after the local initializer and mutation"
    );
    for s in initialized {
        assert_eq!(s.variables.len(), 1);
        let local = &s.variables[0];
        assert_eq!(local.name, "only");
        assert_eq!(
            local.raw_bytes_hex.as_deref(),
            Some(int_bytes(&[38]).as_str())
        );
        assert_evidence(local);
        assert_eq!(local.memory_evidence.as_ref().unwrap().status, "full");
        let global = s
            .observation
            .globals
            .iter()
            .find(|v| v.name.ends_with("::global_pair"))
            .unwrap();
        assert_eq!(
            global.raw_bytes_hex.as_deref(),
            Some(int_bytes(&[11]).as_str())
        );
        assert_evidence(global);
        let evidence = global.memory_evidence.as_ref().unwrap();
        assert_eq!(evidence.extent_bytes, Some(8));
        assert_eq!(evidence.requested_bytes, 4);
        assert_eq!(evidence.captured_bytes, 4);
        assert_eq!(evidence.status, "truncated");
        assert!(
            s.memory_truncated,
            "global-only budget exhaustion must mark the checkpoint"
        );
        assert!(!s.observation.complete);
        assert_eq!(
            variables(s)
                .filter_map(|v| v.raw_bytes_hex.as_ref())
                .map(|h| h.len() / 2)
                .sum::<usize>(),
            8
        );
    }
    assert!(
        trace
            .resource_usage
            .limits_triggered
            .iter()
            .any(|limit| limit == "max_snapshot_memory_bytes"),
        "global capture must report the same named shared budget as local/heap capture"
    );
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

#[tokio::test]
async fn exact_fit_local_capture_is_full_and_complete_without_false_budget_truncation() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory_exact_fit.c");
    c.tracing.discover_globals = false;
    c.limits.max_caller_frames = 0;
    c.limits.max_variables_per_snapshot = 1;
    c.limits.max_snapshot_memory_bytes = 4;
    let trace = run(c).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let return_line = include_str!("../fixtures/c/runtime_memory_exact_fit.c")
        .lines()
        .position(|line| line.contains("return only"))
        .unwrap() as u32
        + 1;
    let initialized: Vec<_> = trace
        .snapshots
        .iter()
        .filter(|s| s.func == "main" && s.line == return_line)
        .collect();
    assert!(
        !initialized.is_empty(),
        "checkpoint after the local initializer and mutation"
    );
    for s in initialized {
        assert!(s.observation.globals.is_empty());
        assert!(s.caller_frames.is_empty());
        assert!(s.heap_blocks.is_empty());
        assert_eq!(s.variables.len(), 1);
        let local = &s.variables[0];
        assert_eq!(local.name, "only");
        assert_eq!(
            local.raw_bytes_hex.as_deref(),
            Some(int_bytes(&[38]).as_str())
        );
        assert_evidence(local);
        let evidence = local.memory_evidence.as_ref().unwrap();
        assert_eq!(evidence.extent_bytes, Some(4));
        assert_eq!(evidence.requested_bytes, 4);
        assert_eq!(evidence.captured_bytes, 4);
        assert_eq!(evidence.status, "full");
        assert!(
            !s.memory_truncated,
            "zero remaining bytes is not truncation when no object was omitted"
        );
        assert!(
            s.observation.complete,
            "exact-fit capture should be complete: {:?}",
            s.observation.errors
        );
    }
    assert!(!trace
        .resource_usage
        .limits_triggered
        .iter()
        .any(|limit| limit == "max_snapshot_memory_bytes"));
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

#[tokio::test]
async fn heap_block_and_byte_caps_preserve_live_observations_without_stale_refreshes() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory_heap.c");
    c.limits.max_heap_blocks_per_snapshot = 2;
    c.limits.max_memory_read_bytes = 16;
    c.limits.max_snapshot_memory_bytes = 64;
    let trace = run(c).await;
    assert_eq!(trace.execution_status, ExecutionStatus::Completed);
    assert_eq!(trace.resource_usage.inferior_exit_code, Some(0));
    let mut first_values = HashSet::new();
    for s in &trace.snapshots {
        assert!(s.heap_blocks.len() <= 2);
        let retained_bytes = variables(s)
            .filter_map(|v| v.raw_bytes_hex.as_ref())
            .map(|h| h.len() / 2)
            .sum::<usize>()
            + s.heap_blocks
                .iter()
                .filter_map(|b| b.raw_bytes_hex.as_ref())
                .map(|h| h.len() / 2)
                .sum::<usize>();
        assert!(
            retained_bytes <= 64,
            "heap and variable reads must share one snapshot budget"
        );
        for block in &s.heap_blocks {
            assert!(!block.is_freed);
            if let Some(bytes) = &block.raw_bytes_hex {
                assert!(bytes.len() / 2 <= 16);
                assert_eq!(block.observed_at_stop, Some(s.observation.stop_id));
                let evidence = block.memory_evidence.as_ref().expect("heap byte evidence");
                assert_eq!(evidence.extent_bytes, Some(block.size_bytes));
                assert_eq!(evidence.captured_bytes, bytes.len() as u64 / 2);
                assert_eq!(evidence.status, "truncated");
                if block.size_bytes == 24 {
                    first_values.insert(bytes[..2].to_owned());
                }
            }
        }
    }
    assert!(
        first_values.contains("0b") && first_values.contains("2c"),
        "{first_values:?}"
    );
    assert!(trace
        .snapshots
        .iter()
        .any(|s| s.heap_blocks.len() == 2 && s.memory_truncated));
    assert!(trace.heap_state.iter().all(|b| b.is_freed));
    let store = TraceStore::from_trace(trace).unwrap();
    assert!(store.validate_deep().is_structurally_sound());
}

// Compare control-flow facts only. Process ids and absolute instruction/stack
// addresses are run-specific; code addresses are normalized by one image base.
fn sequence(trace: &DebugSessionTrace) -> serde_json::Value {
    let address = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap();
    let base = trace
        .recording
        .instruction_events
        .iter()
        .map(|e| address(&e.address))
        .min()
        .unwrap();
    let last = trace
        .recording
        .instruction_events
        .iter()
        .map(|e| address(&e.address))
        .max()
        .unwrap();
    let normalized = |a: &str| {
        let pc = address(a);
        if (base..=last).contains(&pc) {
            serde_json::json!(pc - base)
        } else {
            serde_json::json!("outside_user_image")
        }
    };
    let events: Vec<_> = trace
        .recording
        .instruction_events
        .iter()
        .map(|e| {
            serde_json::json!({
                "event": e.event_id, "before": e.stop_id_before, "after": e.stop_id_after,
                "pc": normalized(&e.address),
                "next": e.next_address.as_deref().map(normalized),
                "source": e.source, "checkpoint": e.checkpoint_id,
                "outcome": e.outcome, "reason": e.stopped_reason, "signal": e.signal,
            })
        })
        .collect();
    let snapshots: Vec<_> = trace.snapshots.iter().map(|s| serde_json::json!({
        "step": s.step_index, "stop": s.observation.stop_id,
        "position": s.observation.instruction_position,
        "file": s.file, "line": s.line, "function": s.func,
        "reason": s.stopped_reason, "signal": s.signal,
        "stack": s.call_stack.iter().map(|f| (&f.func, f.line, f.level)).collect::<Vec<_>>(),
    })).collect();
    serde_json::json!({"events": events, "snapshots": snapshots, "checkpoints": trace.recording.source_checkpoints})
}

#[tokio::test]
async fn enabling_caller_capture_preserves_normalized_instruction_stop_and_source_sequences() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory.c");
    c.tracing.mode = TraceMode::Dual;
    c.build.compiler_flags.push("-fPIE".into());
    c.build.linker_flags.push("-pie".into());
    c.limits.max_caller_frames = 0;
    let disabled = run(c.clone()).await;
    c.limits.max_caller_frames = 8;
    let enabled = run(c).await;
    assert_eq!(disabled.execution_status, ExecutionStatus::Completed);
    assert_eq!(enabled.execution_status, ExecutionStatus::Completed);
    assert!(disabled
        .snapshots
        .iter()
        .all(|s| s.caller_frames.is_empty()));
    assert!(enabled
        .snapshots
        .iter()
        .any(|s| !s.caller_frames.is_empty()));
    assert!(!enabled.recording.instruction_events.is_empty());
    assert_eq!(sequence(&disabled), sequence(&enabled));
    assert_eq!(
        disabled.snapshots.last().unwrap().stdout_so_far,
        enabled.snapshots.last().unwrap().stdout_so_far
    );
}

#[tokio::test]
async fn signal_and_step_limit_preserve_partial_runtime_evidence() {
    let Some(_permit) = tools().await else { return };
    let mut c = config("runtime_memory_partial.c");
    c.build.defines.push("RUNTIME_MEMORY_FAULT".into());
    let fault = run(c).await;
    assert_eq!(fault.execution_status, ExecutionStatus::Crashed);
    assert!(fault
        .snapshots
        .iter()
        .any(|s| s.signal.as_deref() == Some("SIGSEGV")));
    let mut c = config("runtime_memory_partial.c");
    c.limits.max_steps = 16;
    let limited = run(c).await;
    assert_eq!(limited.execution_status, ExecutionStatus::StepLimitReached);
    for trace in [fault, limited] {
        assert!(trace.snapshots.iter().any(|s| s.func == "hold"
            && !s.caller_frames.is_empty()
            && variables(s).any(|v| v.raw_bytes_hex.is_some())));
        assert!(trace.recording.storage.finalized);
        let store = TraceStore::from_trace(trace).unwrap();
        assert!(store.validate_deep().is_structurally_sound());
    }
}

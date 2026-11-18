//! Same-run instruction recording. Processing and pointer enrichment live elsewhere.
use crate::config::{TraceMode, TracingConfig};
use crate::error::{EngineError, Result};
use crate::gdb_mi::{GdbSession, GlobalStaticMeta, StoppedInfo};
use crate::mi_parser::{as_const, MiListItem, MiValue};
use crate::models::*;
use crate::resource_limits::ExecutionLimits;
use crate::storage::Journal;
use std::collections::HashMap;
use std::time::Instant;

pub fn set_validity(var: &mut VariableInfo) {
    var.read_status = if var.value.contains("<optimized out>") {
        DataValidity::OptimizedOut
    } else if var.value.is_empty() || var.value.contains("<unavailable>") {
        DataValidity::Unavailable
    } else {
        DataValidity::Valid
    };
    var.bytes_status = if let Some(hex) = &var.raw_bytes_hex {
        if var.size_bytes.is_some_and(|s| hex.len() as u64 / 2 < s) {
            DataValidity::Truncated
        } else {
            DataValidity::Valid
        }
    } else if var
        .read_error
        .as_deref()
        .is_some_and(|e| e.contains("memory read failed") || e.contains("Cannot access memory"))
    {
        DataValidity::Unreadable
    } else {
        DataValidity::NotCaptured
    };
}

pub(crate) struct MemoryBudget {
    pub remaining_bytes: usize,
    pub max_object_bytes: usize,
    pub max_read_bytes: usize,
    /// None is unlimited. A cached read does not consume a command.
    pub remaining_reads: Option<usize>,
}

pub(crate) async fn capture_variable_memory(
    session: &mut GdbSession,
    var: &mut VariableInfo,
    reads: &mut crate::gdb_mi::CheckpointReads,
    budget: &mut MemoryBudget,
) {
    var.initialization_status = Some("unknown".into());
    if var.size_source.as_deref() != Some("dwarf") {
        var.runtime_size_bytes = var.size_bytes;
        var.size_source = var.size_bytes.map(|_| "gdb".into());
    }
    let mut evidence = MemoryEvidence {
        extent_bytes: var.size_bytes,
        ..Default::default()
    };
    match (var.address.as_deref(), var.size_bytes) {
        (None, _) => {
            evidence.status = "unavailable".into();
            evidence.reason = Some("debugger did not resolve object storage".into());
        }
        (Some(_), None) => {
            evidence.status = "unknown".into();
            evidence.reason = Some("object extent unavailable".into());
        }
        (Some(addr), Some(extent)) => {
            let allowed = extent
                .min(budget.max_object_bytes as u64)
                .min(budget.remaining_bytes as u64);
            let valid_range = crate::memory::address(addr)
                .and_then(|base| base.checked_add(extent))
                .is_some();
            if !valid_range {
                evidence.status = "unknown".into();
                evidence.reason = Some("invalid or overflowing memory range".into());
            } else if extent == 0 {
                evidence.status = "full".into();
                var.raw_bytes_hex = Some(String::new());
            } else if allowed == 0 || budget.max_read_bytes == 0 {
                evidence.status = "unattempted".into();
                evidence.reason = Some("object or checkpoint memory budget exhausted".into());
                evidence.limit = Some(
                    if budget.remaining_bytes == 0 {
                        "max_snapshot_memory_bytes"
                    } else if budget.max_object_bytes != budget.max_read_bytes {
                        "max_aggregate_bytes"
                    } else {
                        "max_memory_read_bytes"
                    }
                    .into(),
                );
            } else {
                let mut hex = String::new();
                let mut failure = None;
                let mut read_cap = false;
                while (hex.len() / 2) < allowed as usize {
                    let offset = (hex.len() / 2) as u64;
                    let chunk = (allowed - offset).min(budget.max_read_bytes as u64);
                    let chunk_address =
                        format!("0x{:x}", crate::memory::address(addr).unwrap() + offset);
                    let cached = reads.lookup(&chunk_address, chunk);
                    if cached.is_none() && budget.remaining_reads == Some(0) {
                        read_cap = true;
                        break;
                    }
                    evidence.requested_bytes += chunk;
                    let (part, error) = if let Some(hex) = cached {
                        session.saved_memory_reads += 1;
                        (hex, None)
                    } else {
                        if let Some(left) = &mut budget.remaining_reads {
                            *left -= 1;
                        }
                        let result = session
                            .read_memory_bytes_partial(&chunk_address, chunk)
                            .await;
                        (result.hex, result.error)
                    };
                    let short = (part.len() / 2) as u64 != chunk;
                    hex.push_str(&part);
                    if let Some(error) = error {
                        failure = Some(error);
                        break;
                    }
                    if short {
                        failure = Some("short memory response".into());
                        break;
                    }
                }
                evidence.captured_bytes = (hex.len() / 2) as u64;
                budget.remaining_bytes = budget
                    .remaining_bytes
                    .saturating_sub(evidence.captured_bytes as usize);
                if !hex.is_empty() {
                    reads.insert(addr, evidence.captured_bytes, &hex);
                    var.raw_bytes_hex = Some(hex);
                }
                evidence.status = if evidence.captured_bytes == extent {
                    "full"
                } else if read_cap && evidence.captured_bytes == 0 {
                    "unattempted"
                } else if failure.is_some() && evidence.captured_bytes == 0 {
                    "unreadable"
                } else if failure.is_some() {
                    "partial"
                } else {
                    "truncated"
                }
                .into();
                if let Some(error) = failure {
                    evidence.reason = Some(error.clone());
                    var.read_error = Some(format!("memory read failed: {error}"));
                } else if read_cap {
                    evidence.reason = Some("max_extra_memory_reads exhausted".into());
                    evidence.limit = Some("max_extra_memory_reads".into());
                } else if evidence.status == "truncated" {
                    evidence.limit = Some(
                        if allowed < extent.min(budget.max_object_bytes as u64) {
                            "max_snapshot_memory_bytes"
                        } else if budget.max_object_bytes != budget.max_read_bytes {
                            "max_aggregate_bytes"
                        } else {
                            "max_memory_read_bytes"
                        }
                        .into(),
                    );
                    evidence.reason =
                        Some(if allowed < extent.min(budget.max_object_bytes as u64) {
                            "checkpoint memory budget".into()
                        } else {
                            "per-object memory limit".into()
                        });
                }
            }
        }
    }
    set_validity(var);
    if evidence.status == "unreadable" {
        var.bytes_status = DataValidity::Unreadable;
    }
    var.memory_evidence = Some(evidence);
}

fn field<'a>(fields: &'a [crate::mi_parser::MiResult], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|r| r.variable == key)
        .and_then(|r| as_const(&r.value))
}

pub struct Recorder {
    pub data: RecordingData,
    config: TracingConfig,
    limits: ExecutionLimits,
    cache: HashMap<String, u64>,
    journal: Journal,
    resident: usize,
    module_epoch: u64,
    journaled_allocations: usize,
}

impl Recorder {
    pub fn new(config: &TracingConfig, limits: &ExecutionLimits) -> Result<Self> {
        let mut data = RecordingData { coverage: Coverage {
            scope: "user_code".into(),
            instruction_capture_enabled: config.mode == TraceMode::Dual,
            complete_for_scope: config.mode == TraceMode::Dual,
            stop_model: "gdb_all_stop".into(),
            truncation_reasons: Vec::new(),
            limitations: vec![
                "Capture begins at main; startup and excluded-code intervals are gaps.".into(),
                "Memory is exact only at its recorded stopped observation, never inferred from registers.".into(),
                "Variables are observed at checkpoints; caller frames are configuration-bounded. Lexical scope and readable bytes do not establish initialization; initialization remains unknown.".into(),
                "All-stop pauses inferior threads, not external processes sharing memory.".into(),
                "Disassembly assumes unchanged user code; self-modifying/JIT code is unsupported.".into(),
            ],
        }, ..Default::default() };
        data.storage.journal = config.journal.as_ref().map(|p| p.display().to_string());
        // Baseline profiling metrics are always collected (F-06).
        data.profiling.enabled = true;
        let mut journal = Journal::new(config.journal.as_deref(), limits.max_trace_bytes)?;
        journal.append("configuration", config)?;
        Ok(Self {
            data,
            config: config.clone(),
            limits: limits.clone(),
            cache: HashMap::new(),
            journal,
            resident: 0,
            module_epoch: 0,
            journaled_allocations: 0,
        })
    }

    pub fn truncate(&mut self, reason: &str) {
        self.data.coverage.complete_for_scope = false;
        if !self
            .data
            .coverage
            .truncation_reasons
            .iter()
            .any(|r| r == reason)
        {
            self.data.coverage.truncation_reasons.push(reason.into());
        }
    }

    fn reserve(&mut self, size: usize) -> Result<()> {
        if self.resident.saturating_add(size) > self.limits.max_trace_memory_bytes {
            return Err(EngineError::LimitReached("max_trace_memory_bytes"));
        }
        self.resident += size;
        Ok(())
    }

    pub fn checkpoint(&mut self, snapshot: &ExecutionSnapshot) -> Result<()> {
        let size = serde_json::to_vec(snapshot)?.len();
        self.reserve(size)?;
        self.journal.append("checkpoint", snapshot)?;
        self.data.source_checkpoints.push(snapshot.step_index);
        Ok(())
    }

    pub fn allocations(&mut self, events: &[HeapEvent], stop_id: u64) -> Result<()> {
        #[derive(serde::Serialize)]
        struct Observed<'a> {
            observed_at_stop: u64,
            event: &'a HeapEvent,
        }
        for event in events.iter().skip(self.journaled_allocations) {
            self.reserve(serde_json::to_vec(event)?.len())?;
            self.journal.append(
                "allocation",
                &Observed {
                    observed_at_stop: stop_id,
                    event,
                },
            )?;
            self.journaled_allocations += 1;
        }
        Ok(())
    }

    pub fn gap(
        &mut self,
        start: Option<String>,
        end: Option<String>,
        origin: &str,
        reason: &str,
    ) -> Result<()> {
        let gap = TraceGap {
            instruction_position: self.data.instruction_count,
            start_address: start,
            end_address: end,
            origin: origin.into(),
            reason: reason.into(),
        };
        self.reserve(serde_json::to_vec(&gap)?.len())?;
        self.journal.append("gap", &gap)?;
        self.data.trace_gaps.push(gap);
        Ok(())
    }

    pub async fn disassemble(
        &mut self,
        session: &mut GdbSession,
        stop: &StoppedInfo,
    ) -> Result<Option<u64>> {
        if !self.config.static_disassembly {
            return Ok(None);
        }
        if self.module_epoch != session.module_epoch() {
            self.cache.clear();
            self.module_epoch = session.module_epoch();
        }
        let Some(addr) = stop.frame.as_ref().and_then(|f| f.address.as_ref()) else {
            return Ok(None);
        };
        if let Some(id) = self.cache.get(addr) {
            self.data.profiling.disassembly_cache_hits += 1;
            return Ok(Some(*id));
        }
        let start = Instant::now();
        // Bounded windows avoid asking GDB for an arbitrarily huge function.
        // Start at an observed instruction boundary; cache every native result.
        let end = crate::memory::address(addr)
            .and_then(|pc| pc.checked_add(128))
            .ok_or_else(|| EngineError::Gdb("invalid observed PC".into()))?;
        let response = session
            .execute(&format!("-data-disassemble -s {addr} -e 0x{end:x} -- 2"))
            .await?;
        self.data.profiling.disassembly_queries += 1;
        self.data.profiling.disassembly_time_us += start.elapsed().as_micros() as u64;
        if let Some(message) = response.error_message() {
            return Err(EngineError::Gdb(message));
        }
        let module_identity = format!(
            "pid={}:module_epoch={}:{}",
            session.inferior_pid.unwrap_or(0),
            self.module_epoch,
            module_at(session.inferior_pid, addr).unwrap_or_else(|| "unknown_module".into())
        );
        if let Some(MiValue::List(items)) = response.record.result("asm_insns") {
            for item in items {
                let MiListItem::Value(MiValue::Tuple(fields)) = item else {
                    continue;
                };
                let Some(address) = field(fields, "address") else {
                    continue;
                };
                if self.cache.contains_key(address) {
                    continue;
                }
                let instruction_id = self.data.disassembly_table.len() as u64;
                let instruction = DisassembledInstruction {
                    instruction_id,
                    module_identity: module_identity.clone(),
                    address: address.into(),
                    bytes: field(fields, "opcodes").map(str::to_owned),
                    text: field(fields, "inst").unwrap_or("").into(),
                    function: field(fields, "func-name").map(str::to_owned),
                };
                self.reserve(serde_json::to_vec(&instruction)?.len())?;
                self.journal.append("disassembly", &instruction)?;
                self.cache.insert(address.into(), instruction_id);
                self.data.disassembly_table.push(instruction);
            }
        }
        Ok(self.cache.get(addr).copied())
    }

    pub async fn selected_memory(&self, session: &mut GdbSession) -> Vec<MemoryObservation> {
        let mut budget = self.limits.max_snapshot_memory_bytes;
        let mut observations = Vec::new();
        for region in &self.config.memory_regions {
            let size = region
                .size
                .min(self.limits.max_memory_read_bytes)
                .min(budget);
            let limit_error = if size < region.size.min(self.limits.max_memory_read_bytes) {
                "memory observation budget exhausted"
            } else {
                "memory observation per-read limit exhausted"
            };
            let address = session
                .evaluate(&format!("(unsigned long)({})", region.address))
                .await
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(|n| format!("0x{n:x}"));
            let (raw_bytes_hex, status, error) = if address.is_none() {
                (
                    None,
                    DataValidity::Unavailable,
                    Some("address expression unavailable at this stop".into()),
                )
            } else if size == 0 && region.size > 0 {
                (None, DataValidity::Truncated, Some(limit_error.into()))
            } else {
                let read = session
                    .read_memory_bytes_partial(address.as_deref().unwrap(), size as u64)
                    .await;
                let captured = read.hex.len() / 2;
                budget = budget.saturating_sub(captured);
                let status = if captured == 0 && read.error.is_some() {
                    DataValidity::Unreadable
                } else if read.error.is_some() || captured < size || size < region.size {
                    DataValidity::Truncated
                } else {
                    DataValidity::Valid
                };
                let error = read.error.or_else(|| {
                    if captured < size {
                        Some(format!("short memory response: {captured} of {size} bytes"))
                    } else if size < region.size {
                        Some(limit_error.into())
                    } else {
                        None
                    }
                });
                (
                    if captured > 0 || size == 0 {
                        Some(read.hex)
                    } else {
                        None
                    },
                    status,
                    error,
                )
            };
            observations.push(MemoryObservation {
                expression: region.address.clone(),
                address,
                requested_bytes: region.size,
                raw_bytes_hex,
                status,
                error,
            });
        }
        observations
    }

    pub fn event(&mut self, event: InstructionEvent) -> Result<()> {
        let start = Instant::now();
        let size = serde_json::to_vec(&event)?.len();
        if self.config.retain_instruction_events {
            self.reserve(size)?;
        }
        self.journal.append("instruction", &event)?;
        self.data.instruction_count += 1;
        if self.config.retain_instruction_events {
            self.data.instruction_events.push(event);
        }
        self.data.profiling.journal_time_us += start.elapsed().as_micros() as u64;
        Ok(())
    }

    pub fn finish(mut self) -> RecordingData {
        self.data.storage.resident_event_bytes = self.resident;
        self.data.storage.bytes_written = self.journal.bytes;
        self.data.storage.finalized = self.journal.finalize().is_ok();
        if !self.data.storage.finalized {
            self.truncate("journal_finalization_failed");
        }
        self.data
    }
}

fn module_at(pid: Option<u32>, address: &str) -> Option<String> {
    let address = crate::memory::address(address)?;
    let maps = std::fs::read_to_string(format!("/proc/{}/maps", pid?)).ok()?;
    for line in maps.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let (start, end) = fields.first()?.split_once('-')?;
        if address >= u64::from_str_radix(start, 16).ok()?
            && address < u64::from_str_radix(end, 16).ok()?
        {
            return Some(format!(
                "{}:range={}:file_offset={}",
                fields.get(5).copied().unwrap_or("anonymous"),
                fields[0],
                fields[2]
            ));
        }
    }
    None
}

pub fn event_outcome(before: &StoppedInfo, after: &StoppedInfo) -> &'static str {
    if after.is_signal() {
        // A synchronous fault is attributable only when the stopped PC is the attempted PC.
        if matches!(
            after.signal_name.as_deref(),
            Some("SIGSEGV" | "SIGBUS" | "SIGILL" | "SIGFPE")
        ) && before.frame.as_ref().and_then(|f| f.address.as_ref())
            == after.frame.as_ref().and_then(|f| f.address.as_ref())
        {
            "faulted"
        } else {
            "interrupted"
        }
    } else if after.reason.as_deref() == Some("end-stepping-range") {
        "completed"
    } else {
        "unknown"
    }
}

pub(crate) async fn capture_global(
    session: &mut GdbSession,
    name: &str,
    limits: &ExecutionLimits,
    reads: &mut crate::gdb_mi::CheckpointReads,
    budget: &mut MemoryBudget,
    types: Option<&crate::dwarf::ObservationTypes>,
    func: &str,
) -> VariableInfo {
    // Explicit function-qualified symbols can be observed before the owner
    // runs. Prove static storage once; qualification alone is not sufficient.
    if name.contains("::") && !name.starts_with('\'') && !session.symbol_scopes.contains_key(name) {
        if let Ok(response) = session
            .execute(&format!(
                "-interpreter-exec console {}",
                crate::gdb_mi::quote_for_mi(&format!("info address {name}"))
            ))
            .await
        {
            if response
                .streams
                .iter()
                .any(|(_, text)| text.contains("static storage at address"))
            {
                session.symbol_scopes.insert(name.into(), "static".into());
            }
        }
    }
    let mut var = VariableInfo {
        name: name.into(),
        scope: Some(
            session
                .symbol_scopes
                .get(name)
                .cloned()
                .unwrap_or_else(|| "global".into()),
        ),
        identity: Some(format!("symbol:{name}")),
        ..Default::default()
    };
    match session.evaluate(name).await {
        Ok(value) => var.value = value,
        Err(e) => {
            var.read_error = Some(e.to_string());
        }
    }
    let meta_key = format!("global:{}:{name}", session.module_epoch());
    // Optimization D: only file-qualified names (the `'file.c'::name` form
    // produced by symbol discovery) have proven frame-independent static
    // storage; their type/address/size are invariant within a module epoch.
    // Function-local statics (`func::name`) stay on the query path.
    let cacheable = name.starts_with('\'') && name.contains("'::");
    let cached = if cacheable {
        session.global_meta_cache.get(&meta_key).cloned()
    } else {
        None
    };
    if let Some(meta) = cached {
        session.global_meta_cache_hits += 1;
        if let Some(t) = &meta.c_type {
            var.c_type = t.clone();
        }
        var.address = meta.address;
        var.size_bytes = meta.size_bytes;
    } else {
        if let Some(c_type) = session.type_cache.get(&meta_key) {
            var.c_type = c_type.clone();
        } else {
            if let Ok(response) = session
                .execute(&format!(
                    "-interpreter-exec console {}",
                    crate::gdb_mi::quote_for_mi(&format!("whatis {name}"))
                ))
                .await
            {
                for (_, text) in response.streams {
                    if let Some(t) = text.trim().strip_prefix("type = ") {
                        var.c_type = t.into();
                    }
                }
            }
            if !var.c_type.is_empty() {
                session
                    .type_cache
                    .insert(meta_key.clone(), var.c_type.clone());
            }
        }
        var.address = session
            .evaluate(&format!("&({name})"))
            .await
            .ok()
            .and_then(|s| crate::gdb_mi::extract_hex_address(&s));
        var.size_bytes = session
            .evaluate(&format!("sizeof({name})"))
            .await
            .ok()
            .and_then(|s| s.parse().ok());
        // Cache only when both stable properties were observed: a transient
        // evaluation failure must be retried on the next capture, never
        // remembered as "unavailable".
        if cacheable && !var.c_type.is_empty() && var.address.is_some() {
            if let Some(size) = var.size_bytes {
                session.global_meta_cache.insert(
                    meta_key,
                    GlobalStaticMeta {
                        c_type: if var.c_type.is_empty() {
                            None
                        } else {
                            Some(var.c_type.clone())
                        },
                        address: var.address.clone(),
                        size_bytes: Some(size),
                    },
                );
            }
        }
    }
    let aggregate = types.is_some_and(|types| types.enrich(&mut var, func));
    let pointer = types
        .and_then(|types| types.is_pointer(&var))
        .unwrap_or_else(|| crate::gdb_mi::is_scalar_pointer_type(&var.c_type));
    if !aggregate && pointer {
        var.points_to = session
            .evaluate(&format!("(unsigned long)({name})"))
            .await
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| format!("0x{v:x}"));
    }
    budget.max_object_bytes = if aggregate {
        limits.max_aggregate_bytes
    } else {
        limits.max_memory_read_bytes
    };
    capture_variable_memory(session, &mut var, reads, budget).await;
    var
}

#[cfg(test)]
mod coverage_regressions {
    use super::*;

    /// Scope, bytes and initialization are independent evidence axes.
    #[test]
    fn limitations_match_current_recording_behavior() {
        let config = TracingConfig::default();
        let limits = ExecutionLimits::default();
        let recorder = Recorder::new(&config, &limits).unwrap();
        let limitations = &recorder.data.coverage.limitations;
        let joined = limitations.join("\n");
        assert!(
            !limitations
                .iter()
                .any(|l| l.contains("Lexical DWARF DIE IDs")),
            "stale limitation string still present: {joined}"
        );
        assert!(
            limitations
                .iter()
                .any(|l| l.contains("caller frames are configuration-bounded")),
            "scope-evidence statement missing: {joined}"
        );
        assert!(
            limitations
                .iter()
                .all(|l| !l.contains("innermost frame only")),
            "stale frame-0-only statement: {joined}"
        );
        assert!(
            limitations
                .iter()
                .any(|l| l.contains("initialization remains unknown")),
            "initialization limitation missing: {joined}"
        );
    }
}

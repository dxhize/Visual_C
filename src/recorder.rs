//! Same-run instruction recording. Processing and pointer enrichment live elsewhere.
use crate::config::{TraceMode, TracingConfig};
use crate::error::{EngineError, Result};
use crate::gdb_mi::{GdbSession, StoppedInfo};
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
                "Lexical DWARF DIE IDs and non-top-frame locals are not collected.".into(),
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
            let address = session
                .evaluate(&format!("(unsigned long)({})", region.address))
                .await
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(|n| format!("0x{n:x}"));
            let result = if size == 0 {
                Err("memory observation budget exhausted".into())
            } else if let Some(address) = &address {
                session.read_memory_bytes(address, size as u64).await
            } else {
                Err("address expression unavailable at this stop".into())
            };
            budget = budget.saturating_sub(size);
            let (raw_bytes_hex, status, error) = match result {
                Ok(hex) => (
                    Some(hex),
                    if size < region.size {
                        DataValidity::Truncated
                    } else {
                        DataValidity::Valid
                    },
                    None,
                ),
                Err(e) => (None, DataValidity::Unreadable, Some(e)),
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

pub async fn capture_global(
    session: &mut GdbSession,
    name: &str,
    limits: &ExecutionLimits,
) -> VariableInfo {
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
            set_validity(&mut var);
            return var;
        }
    }
    let type_key = format!("global:{}:{name}", session.module_epoch());
    if let Some(c_type) = session.type_cache.get(&type_key) {
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
            session.type_cache.insert(type_key, var.c_type.clone());
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
    if var.c_type.contains('*') {
        var.points_to = session
            .evaluate(&format!("(unsigned long)({name})"))
            .await
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|v| format!("0x{v:x}"));
    }
    if let (Some(addr), Some(size)) = (&var.address, var.size_bytes) {
        match session
            .read_memory_bytes(addr, size.min(limits.max_memory_read_bytes as u64))
            .await
        {
            Ok(hex) => var.raw_bytes_hex = Some(hex),
            Err(e) => var.read_error = Some(e),
        }
    }
    set_validity(&mut var);
    var
}

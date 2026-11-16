//! Shared serializable data structures for the complete debug session trace.
//!
//! These types define the wire format consumed by the future visual IDE. All
//! unavailable information is `Option<T>`; nothing is ever fabricated: an
//! absent address, value or memory capture means "could not be observed".

use serde::{Deserialize, Serialize};

use crate::config::BuildConfig;

/// Lifecycle and completeness of one independently executed pipeline stage.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    NotStarted,
    Running,
    Completed,
    Failed,
    TimedOut,
    Skipped,
    Cancelled,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StageResult {
    pub status: Option<StageStatus>,
    pub start_offset_ms: Option<u64>,
    pub end_offset_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub output_complete: Option<bool>,
    pub partial_results_available: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StageResults {
    pub compiler: StageResult,
    pub debugger: StageResult,
    pub valgrind: StageResult,
    pub sanitizer: StageResult,
    pub allocation_tracker: StageResult,
}

// ---------------------------------------------------------------------------
// Compiler diagnostics
// ---------------------------------------------------------------------------

/// One compiler/linker diagnostic. `kind` is gcc's severity (`error`,
/// `warning`, `note`) or `linker-error` for linker-stage failures.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CompilerDiagnostic {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub kind: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Execution snapshots
// ---------------------------------------------------------------------------

/// One stop of the inferior: source position, stack, variables and the heap
/// as reconstructed at that instant.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ExecutionSnapshot {
    #[serde(default)]
    pub observation: SnapshotObservation,
    /// 0-based index of this captured stop.
    pub step_index: usize,
    pub file: String,
    pub line: u32,
    pub func: String,
    pub call_stack: Vec<StackFrameInfo>,
    pub variables: Vec<VariableInfo>,
    /// Heap blocks alive at this stop (contents read through GDB).
    pub heap_blocks: Vec<HeapBlockInfo>,
    /// Program output captured so far (may be head-truncated).
    pub stdout_so_far: String,
    /// GDB's stop reason (`breakpoint-hit`, `end-stepping-range`,
    /// `signal-received`, ...).
    pub stopped_reason: Option<String>,
    /// Signal name when the stop was a signal.
    pub signal: Option<String>,
    /// True when the per-snapshot memory budget ran out mid-capture.
    pub memory_truncated: bool,
    /// True when stdout capture was truncated for this snapshot.
    pub stdout_truncated: bool,
}

/// One frame of the call stack.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StackFrameInfo {
    pub level: u32,
    pub func: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    /// Absolute path when GDB could resolve one (`fullname`).
    pub full_file: Option<String>,
    /// Instruction address of the frame.
    pub address: Option<String>,
}

/// One variable visible at a stop.
///
/// For pointer variables: `address` is the pointer's own address;
/// `points_to` is the address stored *inside* it. The two are deliberately
/// distinct and are never conflated.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct VariableInfo {
    #[serde(default)]
    pub activation_id: Option<u64>,
    #[serde(default)]
    pub lexical_scope_id: Option<String>,
    #[serde(default)]
    pub declaration_file: Option<String>,
    #[serde(default)]
    pub declaration_line: Option<u32>,
    #[serde(default)]
    pub identity: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub target: Option<PointerTarget>,
    #[serde(default)]
    pub read_status: DataValidity,
    #[serde(default)]
    pub bytes_status: DataValidity,
    pub name: String,
    /// C type as reported by the debugger ("" when unavailable).
    pub c_type: String,
    /// Debugger-printed value ("" when unavailable, e.g. optimized out).
    pub value: String,
    /// Address of the variable itself (`0x...`), not of its contents.
    pub address: Option<String>,
    pub size_bytes: Option<u64>,
    /// Stored target address for pointer-typed variables.
    pub points_to: Option<String>,
    /// Raw memory bytes of the variable itself, hex-encoded.
    pub raw_bytes_hex: Option<String>,
    /// True when this variable is a function argument.
    pub is_argument: Option<bool>,
    /// Why some information could not be collected, when relevant.
    pub read_error: Option<String>,
}

/// One heap block. `allocation_id` is stable for the block's whole lifetime;
/// reused addresses always get a fresh id.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HeapBlockInfo {
    /// Stop at which raw bytes were observed alive; final-state bytes may be historical.
    #[serde(default)]
    pub observed_at_stop: Option<u64>,
    pub allocation_id: u64,
    /// Actual address in the traced process (`0x...`).
    pub address: String,
    pub size_bytes: u64,
    /// Allocating function: `malloc`, `calloc` or `realloc`.
    pub allocation_type: String,
    pub is_freed: bool,
    pub allocation_file: Option<String>,
    pub allocation_line: Option<u32>,
    /// Raw bytes read through GDB while the block was alive, hex-encoded.
    /// `None` means "not captured" (limit, unreadable, or freed).
    pub raw_bytes_hex: Option<String>,
    /// Why `raw_bytes_hex` is `None`, when known.
    pub memory_read_error: Option<String>,
    /// For blocks created by a moving `realloc`: the allocation they replaced.
    pub realloc_from_id: Option<u64>,
}

// ---------------------------------------------------------------------------
// Heap events
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HeapEventType {
    Allocated,
    Reallocated,
    Freed,
    AllocationFailed,
}

/// One chronological heap event as recorded by the instrumentation runtime.
///
/// `allocation_id` 0 means "no tracked allocation" (untracked address, e.g.
/// memory allocated inside shared libraries through unwrapped mechanisms).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HeapEvent {
    #[serde(default)]
    pub raw_caller_address: Option<String>,
    /// Runtime sequence number (per process).
    pub sequence: u64,
    /// OS pid of the process that produced the event.
    pub pid: u32,
    pub allocation_id: u64,
    pub event_type: HeapEventType,
    pub address: Option<String>,
    pub size_bytes: Option<u64>,
    /// Resolved caller source location (via addr2line) when available.
    pub source_file: Option<String>,
    pub source_line: Option<u32>,
    /// Previous allocation for moving/growing reallocs.
    pub old_allocation_id: Option<u64>,
    /// Total tracked live bytes after this event (per pid).
    pub heap_usage_after: u64,
    /// Extra context: `free(null)`, `double_free`, `untracked_address`,
    /// `realloc_zero_glibc_freed`, ...
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------
// Crash & sanitizer reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CrashReport {
    #[serde(default)]
    pub instruction_address: Option<String>,
    pub signal_name: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub func: Option<String>,
    /// Summary of the sanitizer run when it reproduced the crash.
    pub asan_summary: Option<String>,
    /// Full stack at the crash.
    pub backtrace: Vec<StackFrameInfo>,
    pub detail: Option<String>,
}

/// Result of the separate AddressSanitizer / UBSan pass.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SanitizerReport {
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub output_complete: bool,
    /// `AddressSanitizer` or `UndefinedBehaviorSanitizer`.
    pub sanitizer: Option<String>,
    pub error_category: Option<String>,
    pub message: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// Raw stack trace lines from the sanitizer output.
    pub stack: Vec<String>,
    /// `reproduced`, `completed_normally`, `inconclusive` or `skipped`.
    pub reproduction: String,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

// ---------------------------------------------------------------------------
// Valgrind report
// ---------------------------------------------------------------------------

/// One Valgrind memcheck error with its stacks.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ValgrindError {
    #[serde(default)]
    pub observed_leaked_bytes: Option<u64>,
    #[serde(default)]
    pub observed_leaked_blocks: Option<u64>,
    pub unique: Option<String>,
    pub kind: String,
    pub what: String,
    pub auxwhat: Option<String>,
    pub stack: Vec<ValgrindFrame>,
    /// Additional stacks (alloc/free sites for UAF, sysparams, ...).
    pub aux_stacks: Vec<Vec<ValgrindFrame>>,
    /// For leak kinds.
    pub leaked_bytes: u64,
    pub leaked_blocks: u64,
    /// Allocation ids whose (file, line) match this error's stack frames.
    pub related_allocation_ids: Vec<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ValgrindFrame {
    pub ip: Option<String>,
    pub obj: Option<String>,
    pub func: Option<String>,
    pub dir: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

/// Independent Valgrind memcheck analysis.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ValgrindReport {
    #[serde(default)]
    pub observed_summary: Option<ObservedHeapSummary>,
    #[serde(default)]
    pub output_complete: bool,
    #[serde(default)]
    pub summary_available: bool,
    #[serde(default)]
    pub parse_error: Option<String>,
    pub total_allocs: u64,
    pub total_frees: u64,
    pub bytes_allocated: u64,
    pub definitely_lost_bytes: u64,
    pub indirectly_lost_bytes: u64,
    pub possibly_lost_bytes: u64,
    pub still_reachable_bytes: u64,
    /// Human-readable error strings (kind + message + primary site).
    pub errors: Vec<String>,
    /// Structured errors with stacks and leak classification.
    pub detailed_errors: Vec<ValgrindError>,
    pub blocks_in_use_at_exit: u64,
    pub bytes_in_use_at_exit: u64,
    pub all_heap_blocks_freed: Option<bool>,
    pub exit_code: Option<i32>,
    /// True when Valgrind was killed by the timeout (XML may be incomplete).
    pub timed_out: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ObservedHeapSummary {
    pub total_allocs: Option<u64>,
    pub total_frees: Option<u64>,
    pub bytes_allocated: Option<u64>,
    pub bytes_in_use_at_exit: Option<u64>,
    pub blocks_in_use_at_exit: Option<u64>,
}

// ---------------------------------------------------------------------------
// Top-level trace
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Interrupted,
    /// Program ran to completion.
    Completed,
    /// Program died from a fatal signal.
    Crashed,
    /// Wall-clock timeout hit; partial trace preserved.
    Timeout,
    /// Step/snapshot limit hit; partial trace preserved.
    StepLimitReached,
    /// Captured-output limit (`max_output_bytes`) hit; partial trace
    /// preserved. Distinct from `StepLimitReached` (V2.1, fixes F-04: the
    /// output-limit termination was previously mislabeled as a step limit).
    OutputLimitReached,
    /// Kernel-enforced resource limit killed the run.
    ResourceLimitReached,
    /// GDB itself failed (missing, inoperable, protocol error).
    DebuggerFailure,
    /// Execution failed for another reason.
    ExecutionFailure,
    /// Compilation failed; nothing was executed.
    CompileFailed,
}

/// Measured resource consumption for the session.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ResourceUsage {
    pub wall_time_ms: u64,
    pub gdb_commands_issued: u64,
    pub snapshots_captured: usize,
    pub allocation_events_ingested: usize,
    pub program_output_bytes: u64,
    pub program_output_truncated: bool,
    /// Names of limits that were actually hit.
    pub limits_triggered: Vec<String>,
    pub inferior_exit_code: Option<i32>,
}

/// Metadata that makes a trace reproducible from the future IDE.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionMeta {
    #[serde(default)]
    pub sanitizer_variant: Option<String>,
    pub session_dir: String,
    pub debug_binary: String,
    pub sanitized_binary: Option<String>,
    pub gcc_version: Option<String>,
    pub gdb_version: Option<String>,
    pub valgrind_version: Option<String>,
    pub addr2line_version: Option<String>,
    /// True when cgroup v2 enforcement was active for the run.
    pub cgroup_enforced: bool,
    pub build: BuildConfig,
    pub program_args: Vec<String>,
    pub working_directory: Option<String>,
}

/// The complete structured result of one tracing session.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DebugSessionTrace {
    #[serde(default)]
    pub execution_limits: crate::resource_limits::ExecutionLimits,
    #[serde(default)]
    pub execution: ExecutionOutcome,
    #[serde(default, flatten)]
    pub recording: RecordingData,
    #[serde(default)]
    pub tracing_configuration: crate::config::TracingConfig,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub compiled_successfully: bool,
    pub compile_errors: Vec<CompilerDiagnostic>,
    pub snapshots: Vec<ExecutionSnapshot>,
    /// Chronological allocation event history (ingest order).
    pub heap_events: Vec<HeapEvent>,
    /// Every reconstructed block with its final state (freed or live).
    pub heap_state: Vec<HeapBlockInfo>,
    pub crash_report: Option<CrashReport>,
    pub valgrind_report: Option<ValgrindReport>,
    pub execution_status: ExecutionStatus,
    pub resource_usage: ResourceUsage,
    /// Result of the separate sanitizer pass (crash reproduction attempt).
    pub sanitizer_report: Option<SanitizerReport>,
    pub session_meta: Option<SessionMeta>,
    /// Operational notes: missing tools, parse failures, truncations.
    pub analysis_notes: Vec<String>,
    #[serde(default)]
    pub stage_results: StageResults,
}

fn default_schema_version() -> u32 {
    2
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataValidity {
    Valid,
    Unavailable,
    Unreadable,
    OptimizedOut,
    Truncated,
    #[default]
    NotCaptured,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PointerTarget {
    pub kind: String,
    pub allocation_id: Option<u64>,
    pub base_address: Option<String>,
    pub offset_bytes: Option<u64>,
    pub active: Option<bool>,
    pub within_extent: Option<bool>,
    pub variable_identity: Option<String>,
}
impl Default for PointerTarget {
    fn default() -> Self {
        Self {
            kind: "unknown".into(),
            allocation_id: None,
            base_address: None,
            offset_bytes: None,
            active: None,
            within_extent: None,
            variable_identity: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct SnapshotObservation {
    pub stop_id: u64,
    /// Number of attempted instruction events preceding this stop (0 initially).
    pub instruction_position: u64,
    pub instruction_address: Option<String>,
    pub thread_id: Option<String>,
    pub pid: Option<u32>,
    pub origin: String,
    pub globals: Vec<VariableInfo>,
    pub complete: bool,
    pub errors: Vec<String>,
    pub heap_event_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SourceContext {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub function: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DisassembledInstruction {
    pub instruction_id: u64,
    pub module_identity: String,
    pub address: String,
    pub bytes: Option<String>,
    pub text: String,
    pub function: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegisterValue {
    pub number: u32,
    pub name: String,
    pub value: Option<String>,
    pub availability: DataValidity,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegisterObservation {
    pub full_checkpoint: bool,
    pub architecture: String,
    pub thread_id: Option<String>,
    pub values: Vec<RegisterValue>,
    pub complete: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemoryObservation {
    pub expression: String,
    pub address: Option<String>,
    pub requested_bytes: usize,
    pub raw_bytes_hex: Option<String>,
    pub status: DataValidity,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstructionEvent {
    #[serde(default)]
    pub stop_id_before: u64,
    #[serde(default)]
    pub stop_id_after: Option<u64>,
    /// Address BEFORE attempting the instruction. Never the next PC mislabeled as executed.
    pub event_id: u64,
    pub instruction_id: Option<u64>,
    pub address: String,
    pub next_address: Option<String>,
    pub thread_id: Option<String>,
    pub pid: Option<u32>,
    pub source: SourceContext,
    pub checkpoint_id: Option<usize>,
    /// completed, faulted, interrupted, or unknown (no proof of completion).
    pub outcome: String,
    pub stopped_reason: Option<String>,
    pub signal: Option<String>,
    /// DEPRECATED since V2.1: per-instruction CPU register snapshots were
    /// removed (~90% of debugger command time; no GUI consumer). Always
    /// `None` in V2.1 output; retained solely to deserialize V2 traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registers: Option<RegisterObservation>,
    pub memory: Vec<MemoryObservation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TraceGap {
    pub instruction_position: u64,
    pub start_address: Option<String>,
    pub end_address: Option<String>,
    pub origin: String,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct RecordingData {
    pub internal_diagnostics: Vec<String>,
    pub disassembly_table: Vec<DisassembledInstruction>,
    pub instruction_events: Vec<InstructionEvent>,
    /// References into legacy snapshots; no duplicate large memory objects.
    pub source_checkpoints: Vec<usize>,
    /// DEPRECATED since V2.1: initial register snapshots removed with the
    /// per-instruction capture. Always `None` in V2.1 output; retained
    /// solely to deserialize V2 traces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_registers: Option<RegisterObservation>,
    pub trace_gaps: Vec<TraceGap>,
    pub instruction_count: u64,
    pub coverage: Coverage,
    pub storage: StorageInfo,
    pub profiling: Profiling,
    pub allocation_lifetimes: Vec<AllocationLifetime>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AllocationLifetime {
    pub allocation_id: u64,
    pub pid: u32,
    pub allocation_sequence: u64,
    pub free_sequence: Option<u64>,
    pub raw_caller_address: String,
    pub module: Option<String>,
    pub resolution: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Coverage {
    pub scope: String,
    pub instruction_capture_enabled: bool,
    pub complete_for_scope: bool,
    pub truncation_reasons: Vec<String>,
    pub stop_model: String,
    pub limitations: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct StorageInfo {
    pub journal: Option<String>,
    pub finalized: bool,
    pub bytes_written: u64,
    pub resident_event_bytes: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Profiling {
    #[serde(default)]
    pub engine_peak_rss_kib: Option<u64>,
    #[serde(default)]
    pub source_resolution_time_us: u64,
    #[serde(default)]
    pub command_time_us: u64,
    #[serde(default)]
    pub mi_parse_time_us: u64,
    #[serde(default)]
    pub memory_read_time_us: u64,
    pub enabled: bool,
    pub step_time_us: u64,
    pub disassembly_time_us: u64,
    /// Time spent in register queries. Always 0 since V2.1 (per-instruction
    /// register capture removed); kept so historical V2 metrics stay
    /// comparable and the field is never reused for unrelated debugger work.
    pub register_time_us: u64,
    pub capture_time_us: u64,
    pub debugger_time_us: u64,
    pub disassembly_queries: u64,
    pub disassembly_cache_hits: u64,
    pub journal_time_us: u64,
}

impl Default for DebugSessionTrace {
    fn default() -> Self {
        Self {
            execution_limits: Default::default(),
            recording: RecordingData::default(),
            execution: ExecutionOutcome::default(),
            tracing_configuration: crate::config::TracingConfig::default(),
            schema_version: 2,
            compiled_successfully: false,
            compile_errors: Vec::new(),
            snapshots: Vec::new(),
            heap_events: Vec::new(),
            heap_state: Vec::new(),
            crash_report: None,
            valgrind_report: None,
            execution_status: ExecutionStatus::ExecutionFailure,
            resource_usage: ResourceUsage::default(),
            sanitizer_report: None,
            session_meta: None,
            analysis_notes: Vec::new(),
            stage_results: StageResults::default(),
        }
    }
}

/// Canonical V2 outcome. The legacy execution_status field remains an export alias.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ExecutionOutcome {
    pub outcome: ProgramOutcome,
    pub exit_code: Option<i32>,
    pub signal: Option<String>,
    pub instruction_address: Option<String>,
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgramOutcome {
    #[default]
    NotStarted,
    CompileFailed,
    Completed,
    Crashed,
    TimedOut,
    StepLimitReached,
    MemoryLimitReached,
    OutputLimitReached,
    Interrupted,
    DebuggerFailed,
    InternalFailure,
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn legacy_trace_deserializes_as_schema_v2_with_unstarted_stages() {
        let legacy = r#"{
            "compiled_successfully": true,
            "compile_errors": [],
            "snapshots": [],
            "heap_events": [],
            "heap_state": [],
            "crash_report": null,
            "valgrind_report": null,
            "execution_status": "completed",
            "resource_usage": {
                "wall_time_ms": 0,
                "gdb_commands_issued": 0,
                "snapshots_captured": 0,
                "allocation_events_ingested": 0,
                "program_output_bytes": 0,
                "program_output_truncated": false,
                "limits_triggered": [],
                "inferior_exit_code": null
            },
            "sanitizer_report": null,
            "session_meta": null,
            "analysis_notes": []
        }"#;
        let trace: DebugSessionTrace = serde_json::from_str(legacy).unwrap();
        assert_eq!(trace.schema_version, 2);
        assert_eq!(trace.stage_results.debugger.status, None);
    }
}

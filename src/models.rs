//! Shared serializable data structures for the complete debug session trace.
//!
//! These types define the wire format consumed by the future visual IDE. All
//! unavailable information is `Option<T>`; nothing is ever fabricated: an
//! absent address, value or memory capture means "could not be observed".

use serde::{Deserialize, Serialize};

use crate::config::BuildConfig;

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

// ---------------------------------------------------------------------------
// Top-level trace
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Program ran to completion.
    Completed,
    /// Program died from a fatal signal.
    Crashed,
    /// Wall-clock timeout hit; partial trace preserved.
    Timeout,
    /// Step/snapshot limit hit; partial trace preserved.
    StepLimitReached,
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
}

impl Default for DebugSessionTrace {
    fn default() -> Self {
        Self {
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
        }
    }
}

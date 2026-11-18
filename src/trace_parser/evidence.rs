//! Trace-local, typed references to original Stage 1 observations.
//!
//! An evidence reference contains a category and canonical position plus a
//! weak trace identity. It never owns a trace or copies an observation. Even
//! after the original store is dropped, a weak identity prevents address
//! reuse from making a reference accidentally valid in a different store.

use std::sync::{Arc, Weak};

use crate::models::{
    AllocationLifetime, CompilerDiagnostic, CrashReport, DebugSessionTrace,
    DisassembledInstruction, ExecutionSnapshot, FrameVariables, HeapBlockInfo, HeapEvent,
    InstructionEvent, MemoryObservation, SanitizerReport, StackFrameInfo, TraceGap, ValgrindError,
    ValgrindReport, VariableInfo,
};

use super::{QueryError, QueryResult, TraceStore};

/// Position within a specific canonical collection; neither an allocation
/// identity, a debugger stop counter, nor an instruction address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvidenceLocation {
    Snapshot(usize),
    InstructionEvent(usize),
    Disassembly(usize),
    HeapEvent(usize),
    HeapState(usize),
    AllocationLifetime(usize),
    TraceGap(usize),
    SnapshotFrame {
        snapshot: usize,
        frame: usize,
    },
    SnapshotVariable {
        snapshot: usize,
        variable: usize,
    },
    /// `frame` is a caller_frames position, not a GDB level or activation ID.
    SnapshotCallerFrame {
        snapshot: usize,
        frame: usize,
    },
    SnapshotCallerVariable {
        snapshot: usize,
        frame: usize,
        variable: usize,
    },
    SnapshotGlobal {
        snapshot: usize,
        variable: usize,
    },
    SnapshotHeapBlock {
        snapshot: usize,
        block: usize,
    },
    EventMemory {
        event: usize,
        observation: usize,
    },
    CompilerDiagnostic(usize),
    ValgrindError(usize),
    InternalDiagnostic(usize),
    CrashReport,
    ValgrindReport,
    SanitizerReport,
}

/// Trace-scoped, small, cloneable reference. Created by
/// [`TraceStore::evidence_ref`] only after the position has resolved; never
/// serialize or reuse it as a cross-session identity.
#[derive(Clone, Debug)]
pub struct EvidenceRef {
    trace: Weak<DebugSessionTrace>,
    location: EvidenceLocation,
}

impl EvidenceRef {
    /// The original typed position. Safe to inspect; it is not sufficient
    /// to resolve evidence without the matching store.
    pub fn location(&self) -> EvidenceLocation {
        self.location
    }
}

/// Borrowed canonical evidence; callers must keep the store alive. Contents
/// such as raw bytes, addresses and availability statuses are unchanged.
#[derive(Debug)]
pub enum Evidence<'a> {
    Snapshot(&'a ExecutionSnapshot),
    InstructionEvent(&'a InstructionEvent),
    Disassembly(&'a DisassembledInstruction),
    HeapEvent(&'a HeapEvent),
    HeapState(&'a HeapBlockInfo),
    AllocationLifetime(&'a AllocationLifetime),
    TraceGap(&'a TraceGap),
    SnapshotFrame(&'a StackFrameInfo),
    SnapshotVariable(&'a VariableInfo),
    SnapshotCallerFrame(&'a FrameVariables),
    SnapshotCallerVariable(&'a VariableInfo),
    SnapshotGlobal(&'a VariableInfo),
    SnapshotHeapBlock(&'a HeapBlockInfo),
    EventMemory(&'a MemoryObservation),
    CompilerDiagnostic(&'a CompilerDiagnostic),
    ValgrindError(&'a ValgrindError),
    InternalDiagnostic(&'a str),
    CrashReport(&'a CrashReport),
    ValgrindReport(&'a ValgrindReport),
    SanitizerReport(&'a SanitizerReport),
}

fn position<'a, T>(items: &'a [T], index: usize, name: &'static str) -> QueryResult<&'a T> {
    items.get(index).ok_or(QueryError::InvalidReference {
        field: "evidence position",
        value: index as u64,
        collection: name,
        len: items.len(),
    })
}

fn absent(what: &'static str) -> QueryError {
    QueryError::Unavailable { what }
}

impl TraceStore {
    /// Validate a typed position, then bind it to this loaded trace.
    ///
    /// O(1) direct vector access, with no owned data or allocation. Optional
    /// reports return `Unavailable`, invalid positions `InvalidReference`.
    pub fn evidence_ref(&self, location: EvidenceLocation) -> QueryResult<EvidenceRef> {
        self.resolve_location(location)?;
        Ok(EvidenceRef {
            trace: Arc::downgrade(&self.trace),
            location,
        })
    }

    /// Resolve to the original borrowed record. `ForeignTrace` rejects even
    /// equal-looking positions from a separately loaded copy of the JSON.
    /// Clones of the *same* store resolve each other's references.
    pub fn resolve_evidence(&self, reference: &EvidenceRef) -> QueryResult<Evidence<'_>> {
        if !Weak::ptr_eq(&reference.trace, &Arc::downgrade(&self.trace)) {
            return Err(QueryError::ForeignTrace);
        }
        self.resolve_location(reference.location)
    }

    fn resolve_location(&self, location: EvidenceLocation) -> QueryResult<Evidence<'_>> {
        let trace = self.trace();
        let recording = &trace.recording;
        Ok(match location {
            EvidenceLocation::Snapshot(i) => {
                Evidence::Snapshot(position(&trace.snapshots, i, "snapshots")?)
            }
            EvidenceLocation::InstructionEvent(i) => Evidence::InstructionEvent(position(
                &recording.instruction_events,
                i,
                "instruction_events",
            )?),
            EvidenceLocation::Disassembly(i) => Evidence::Disassembly(position(
                &recording.disassembly_table,
                i,
                "disassembly_table",
            )?),
            EvidenceLocation::HeapEvent(i) => {
                Evidence::HeapEvent(position(&trace.heap_events, i, "heap_events")?)
            }
            EvidenceLocation::HeapState(i) => {
                Evidence::HeapState(position(&trace.heap_state, i, "heap_state")?)
            }
            EvidenceLocation::AllocationLifetime(i) => Evidence::AllocationLifetime(position(
                &recording.allocation_lifetimes,
                i,
                "allocation_lifetimes",
            )?),
            EvidenceLocation::TraceGap(i) => {
                Evidence::TraceGap(position(&recording.trace_gaps, i, "trace_gaps")?)
            }
            EvidenceLocation::SnapshotFrame { snapshot, frame } => {
                let s = position(&trace.snapshots, snapshot, "snapshots")?;
                Evidence::SnapshotFrame(position(&s.call_stack, frame, "snapshot.call_stack")?)
            }
            EvidenceLocation::SnapshotVariable { snapshot, variable } => {
                let s = position(&trace.snapshots, snapshot, "snapshots")?;
                Evidence::SnapshotVariable(position(&s.variables, variable, "snapshot.variables")?)
            }
            EvidenceLocation::SnapshotGlobal { snapshot, variable } => {
                let s = position(&trace.snapshots, snapshot, "snapshots")?;
                Evidence::SnapshotGlobal(position(
                    &s.observation.globals,
                    variable,
                    "snapshot.observation.globals",
                )?)
            }
            EvidenceLocation::SnapshotCallerFrame { snapshot, frame } => {
                let s = position(&trace.snapshots, snapshot, "snapshots")?;
                Evidence::SnapshotCallerFrame(position(
                    &s.caller_frames,
                    frame,
                    "snapshot.caller_frames",
                )?)
            }
            EvidenceLocation::SnapshotCallerVariable {
                snapshot,
                frame,
                variable,
            } => {
                let s = position(&trace.snapshots, snapshot, "snapshots")?;
                let caller = position(&s.caller_frames, frame, "snapshot.caller_frames")?;
                Evidence::SnapshotCallerVariable(position(
                    &caller.variables,
                    variable,
                    "caller_frame.variables",
                )?)
            }
            EvidenceLocation::SnapshotHeapBlock { snapshot, block } => {
                let s = position(&trace.snapshots, snapshot, "snapshots")?;
                Evidence::SnapshotHeapBlock(position(
                    &s.heap_blocks,
                    block,
                    "snapshot.heap_blocks",
                )?)
            }
            EvidenceLocation::EventMemory { event, observation } => {
                let e = position(&recording.instruction_events, event, "instruction_events")?;
                Evidence::EventMemory(position(&e.memory, observation, "event.memory")?)
            }
            EvidenceLocation::CompilerDiagnostic(i) => {
                Evidence::CompilerDiagnostic(position(&trace.compile_errors, i, "compile_errors")?)
            }
            EvidenceLocation::ValgrindError(i) => {
                let report = trace
                    .valgrind_report
                    .as_ref()
                    .ok_or_else(|| absent("a Valgrind report"))?;
                Evidence::ValgrindError(position(
                    &report.detailed_errors,
                    i,
                    "valgrind_report.detailed_errors",
                )?)
            }
            EvidenceLocation::InternalDiagnostic(i) => Evidence::InternalDiagnostic(position(
                &recording.internal_diagnostics,
                i,
                "internal_diagnostics",
            )?),
            EvidenceLocation::CrashReport => Evidence::CrashReport(
                trace
                    .crash_report
                    .as_ref()
                    .ok_or_else(|| absent("a crash report"))?,
            ),
            EvidenceLocation::ValgrindReport => Evidence::ValgrindReport(
                trace
                    .valgrind_report
                    .as_ref()
                    .ok_or_else(|| absent("a Valgrind report"))?,
            ),
            EvidenceLocation::SanitizerReport => Evidence::SanitizerReport(
                trace
                    .sanitizer_report
                    .as_ref()
                    .ok_or_else(|| absent("a sanitizer report"))?,
            ),
        })
    }
}

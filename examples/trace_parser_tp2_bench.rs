//! Reproduce the TP-2 release comparison without a benchmark crate.
//! Usage: cargo run --release --example trace_parser_tp2_bench -- TRACE.json
//! This is a single-run microbenchmark, not a statistical confidence interval.

use c_trace_engine::models::DebugSessionTrace;
use c_trace_engine::trace_parser::TraceStore;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;
use std::{env, fs};

static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
struct Meter;

#[global_allocator]
static ALLOCATOR: Meter = Meter;

// SAFETY: Each operation delegates to System with the caller's original
// pointer/layout. The counter tracks only successful allocations; it never
// allocates or alters the allocator's return values.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, new_size) };
        if !result.is_null() {
            LIVE_BYTES.fetch_add(
                new_size as isize - layout.size() as isize,
                Ordering::Relaxed,
            );
        }
        result
    }
}

fn rss_kib() -> usize {
    fs::read_to_string("/proc/self/status")
        .expect("Linux /proc/self/status")
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .expect("VmRSS")
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn measure(label: &str, reps: usize, mut query: impl FnMut(usize) -> usize) {
    let start = Instant::now();
    let mut checksum = 0usize;
    for i in 0..reps {
        checksum = checksum.wrapping_add(black_box(query(black_box(i))));
    }
    black_box(checksum);
    let elapsed = start.elapsed();
    println!(
        "{label:32} {reps:8} {:10.1} ns/query total={elapsed:?} checksum={checksum}",
        elapsed.as_nanos() as f64 / reps as f64
    );
}

fn main() {
    let path = env::args()
        .nth(1)
        .expect("usage: trace_parser_tp2_bench TRACE.json");
    let bytes = fs::read(&path).expect("trace bytes");
    let started = Instant::now();
    let trace: DebugSessionTrace = serde_json::from_slice(&bytes).expect("complete V2.1 JSON");
    let parse = started.elapsed();
    let before_rss = rss_kib();
    let before_live = LIVE_BYTES.load(Ordering::Relaxed);
    let started = Instant::now();
    let store = TraceStore::from_trace(trace).expect("supported schema");
    let build = started.elapsed();
    let after_rss = rss_kib();
    let after_live = LIVE_BYTES.load(Ordering::Relaxed);
    let trace = store.trace();
    let events = &trace.recording.instruction_events;
    let disassembly = &trace.recording.disassembly_table;
    println!(
        "FILE {path} bytes={} events={} disasm={} snapshots={} heap={} \
         parse={parse:?} build={build:?} rss_before_index={before_rss} KiB \
         rss_after_index={after_rss} KiB rss_delta={} KiB index_estimate={} bytes \
         live_alloc_delta={} bytes",
        bytes.len(),
        events.len(),
        disassembly.len(),
        trace.snapshots.len(),
        trace.heap_events.len(),
        after_rss as isize - before_rss as isize,
        store.indexes().estimated_bytes(),
        after_live - before_live,
    );
    let reps = if events.len() > 10_000 { 500 } else { 4_000 };
    if !events.is_empty() {
        measure("direct event.get (O(1))", 100_000, |i| {
            store.instruction_event(i % events.len()).unwrap().event_id as usize
        });
        if let Some(first) = events.iter().find_map(|event| event.instruction_id) {
            let last = disassembly.len() as u64 - 1;
            for (label, id) in [("hit", first), ("last-entry", last)] {
                measure(&format!("instruction scan {label}"), reps, |_| {
                    events
                        .iter()
                        .filter(|event| event.instruction_id == Some(id))
                        .count()
                });
                measure(&format!("instruction index {label}"), reps * 10, |_| {
                    store.instruction_execution_positions(id).unwrap().len()
                });
            }
        }
        if let Some((file, line)) = events
            .iter()
            .find_map(|event| event.source.file.as_deref().zip(event.source.line))
        {
            for (label, query_line) in [("hit", line), ("miss", u32::MAX)] {
                measure(&format!("source scan {label}"), reps, |_| {
                    events
                        .iter()
                        .filter(|event| {
                            event.source.file.as_deref() == Some(file)
                                && event.source.line == Some(query_line)
                        })
                        .count()
                });
                measure(&format!("source index {label}"), reps * 10, |_| {
                    store
                        .executed_event_positions_for_source(file, query_line)
                        .len()
                });
            }
        }
    }
    if let Some(first) = trace.snapshots.first() {
        for (label, stop) in [("hit", first.observation.stop_id), ("miss", u64::MAX)] {
            measure(&format!("stop snapshot scan {label}"), reps * 10, |_| {
                trace
                    .snapshots
                    .iter()
                    .position(|snapshot| snapshot.observation.stop_id == stop)
                    .unwrap_or(0)
            });
            measure(&format!("stop snapshot binary {label}"), reps * 10, |_| {
                store
                    .snapshot_at_stop(stop)
                    .unwrap()
                    .map(|snapshot| snapshot.step_index)
                    .unwrap_or(0)
            });
        }
    }
    if let Some(first) = trace.heap_events.first() {
        for (label, id) in [("hit", first.allocation_id), ("miss", u64::MAX)] {
            measure(&format!("allocation scan {label}"), reps * 10, |_| {
                trace
                    .heap_events
                    .iter()
                    .filter(|event| event.allocation_id == id)
                    .count()
            });
            measure(&format!("allocation index {label}"), reps * 10, |_| {
                store.heap_event_positions_for_allocation(id).len()
            });
        }
    }
}

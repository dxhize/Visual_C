//! Reproducible TP-3 release microbenchmarks over complete V2.1 JSON.
//! Run: cargo run --release --example trace_parser_tp3_bench -- trace.json [...]
//! Seven rounds: median and p95 of each measured operation, not speedups.
use c_trace_engine::models::DebugSessionTrace;
use c_trace_engine::trace_parser::{Evidence, EvidenceLocation, TraceStore};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::time::Instant;
use std::{env, fs};

static LIVE: AtomicIsize = AtomicIsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
struct Meter;
#[global_allocator]
static ALLOCATOR: Meter = Meter;

// SAFETY: All allocation operations use System and retain the passed layout;
// atomics count successful allocations only, without allocating themselves.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(p, layout) };
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(p, layout, size) };
        if !next.is_null() {
            LIVE.fetch_add(size as isize - layout.size() as isize, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        next
    }
}

fn bench(label: &str, reps: usize, mut query: impl FnMut(usize) -> usize) {
    let before = ALLOCS.load(Ordering::Relaxed);
    let mut samples = [0.0; 7];
    let mut check = 0usize;
    for sample in &mut samples {
        let start = Instant::now();
        for i in 0..reps {
            check = check.wrapping_add(black_box(query(black_box(i))));
        }
        *sample = start.elapsed().as_nanos() as f64 / reps as f64;
    }
    black_box(check);
    // Sorting seven scalar measurements never allocates.
    samples.sort_by(f64::total_cmp);
    println!(
        "{label:34} median={:10.1} ns p95={:10.1} ns allocs={} reps/round={reps}",
        samples[3],
        samples[6],
        ALLOCS.load(Ordering::Relaxed) - before,
    );
}

fn rss_kib() -> usize {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn main() {
    let paths: Vec<String> = env::args().skip(1).collect();
    assert!(
        !paths.is_empty(),
        "provide one or more complete JSON traces"
    );
    for path in paths {
        let bytes = fs::read(&path).unwrap();
        let start = Instant::now();
        let trace: DebugSessionTrace = serde_json::from_slice(&bytes).unwrap();
        let parse = start.elapsed();
        let rss_before = rss_kib();
        let start = Instant::now();
        let store = TraceStore::from_trace(trace).unwrap();
        let index_build = start.elapsed();
        let rss_after = rss_kib();
        let persistent = LIVE.load(Ordering::Relaxed);
        let init_allocs = ALLOCS.load(Ordering::Relaxed);
        let first = Instant::now();
        let raw = store.raw();
        let raw_cold = first.elapsed();
        let first = Instant::now();
        let semantic = store.semantic_input();
        let semantic_cold = first.elapsed();
        let first_context = if store.trace().snapshots.is_empty() {
            None
        } else {
            let start = Instant::now();
            let context = semantic.checkpoint_context(0).unwrap();
            black_box(context.snapshot_index());
            Some(start.elapsed())
        };
        let after_views = LIVE.load(Ordering::Relaxed);
        println!(
            "\nFILE {path} bytes={} events={} snapshots={} heap={} parse={parse:?} \
             tp2_index_build={index_build:?} rss_before_index={rss_before} KiB \
             rss_after_index={rss_after} KiB tp3_live_delta={} bytes tp3_allocs={} \
             first_raw={raw_cold:?} first_semantic={semantic_cold:?} \
             first_context={first_context:?}",
            bytes.len(),
            store.retained_event_count(),
            store.trace().snapshots.len(),
            store.heap_events().len(),
            after_views - persistent,
            ALLOCS.load(Ordering::Relaxed) - init_allocs
        );
        black_box(raw);
        black_box(semantic);
        bench("raw() creation", 100_000, |_| {
            black_box(store.raw()).trace().schema_version as usize
        });
        bench("semantic_input() creation", 100_000, |_| {
            black_box(store.semantic_input())
                .store()
                .trace()
                .schema_version as usize
        });
        let snapshot_len = store.trace().snapshots.len();
        if snapshot_len != 0 {
            bench("checkpoint_context varied", 20_000, |i| {
                let context = store
                    .semantic_input()
                    .checkpoint_context(i % snapshot_len)
                    .unwrap();
                black_box(context.snapshot().step_index)
            });
            let source_count = store.source_checkpoints().len();
            if source_count != 0 {
                bench("traverse source checkpoints", 500, |_| {
                    store
                        .semantic_input()
                        .source_checkpoints()
                        .map(|result| result.unwrap().snapshot_index())
                        .fold(0usize, usize::wrapping_add)
                });
                bench("traverse + checkpoint events", 200, |_| {
                    store
                        .semantic_input()
                        .source_checkpoints()
                        .map(|context| context.unwrap().events_at_checkpoint().unwrap().len())
                        .fold(0usize, usize::wrapping_add)
                });
            }
            let refs: Vec<_> = (0..snapshot_len.min(16))
                .map(|i| store.evidence_ref(EvidenceLocation::Snapshot(i)).unwrap())
                .collect();
            bench("resolve evidence varied", 20_000, |i| {
                match store.resolve_evidence(&refs[i % refs.len()]).unwrap() {
                    Evidence::Snapshot(s) => black_box(s.step_index),
                    _ => unreachable!(),
                }
            });
        }
        let events = store.retained_event_count();
        if events != 0 {
            bench("bounded instruction_range(<=16)", 20_000, |i| {
                let start = i % events;
                store
                    .raw()
                    .instruction_range(start..(start + 16).min(events))
                    .unwrap()
                    .len()
            });
            if let Some((file, line)) = store
                .trace()
                .recording
                .instruction_events
                .iter()
                .find_map(|event| event.source.file.as_deref().zip(event.source.line))
            {
                bench("source -> static instructions", 20_000, |i| {
                    store
                        .raw()
                        .instructions_for_source(file, if i % 2 == 0 { line } else { u32::MAX })
                        .unwrap()
                        .len()
                });
            }
        }
        if snapshot_len != 0 {
            let thread_reps = 5_000;
            bench("parallel 4 readers (per batch)", 7, |_| {
                let start = Instant::now();
                std::thread::scope(|scope| {
                    for _ in 0..4 {
                        scope.spawn(|| {
                            for i in 0..thread_reps {
                                black_box(
                                    store
                                        .semantic_input()
                                        .checkpoint_context(i % snapshot_len)
                                        .unwrap()
                                        .snapshot_index(),
                                );
                            }
                        });
                    }
                });
                black_box(start.elapsed().as_nanos() as usize)
            });
        }
        println!(
            "persistent_tp3_after_queries={} bytes (compared with {} bytes before view creation; \
             benchmark's own output buffers may grow)",
            LIVE.load(Ordering::Relaxed) - persistent,
            persistent
        );
    }
}

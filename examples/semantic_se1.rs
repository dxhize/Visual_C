//! Read-only SE-1 usage/benchmark, one input per process for RSS attribution.
//! cargo run --release --example semantic_se1 -- examples/v21_dual.json
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use c_trace_engine::semantic::{ObjectKind, ObjectRegistry};
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};

struct Count;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicI64 = AtomicI64::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let resized = unsafe { System.realloc(ptr, layout, new_size) };
        if !resized.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_add(new_size as i64 - layout.size() as i64, Ordering::Relaxed);
        }
        resized
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as i64, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static COUNTER: Count = Count;

fn memory_kib(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|line| line.starts_with(key))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}
fn median(mut samples: Vec<u64>) -> u64 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("provide a trace JSON path")?;
    let started = Instant::now();
    let store = TraceStore::open(&path, TraceLoadOptions::default())?;
    let parsed_us = started.elapsed().as_micros();
    let started = Instant::now();
    let view = store.validated_semantic();
    let validation_us = started.elapsed().as_micros();
    let before = memory_kib("VmRSS:");
    let live_before = LIVE.load(Ordering::Relaxed);
    let started = Instant::now();
    let registry = ObjectRegistry::build(&view)?;
    let build_us = started.elapsed().as_micros();
    let live_delta_bytes = LIVE.load(Ordering::Relaxed) - live_before;
    let after = memory_kib("VmRSS:");
    let heap = registry
        .objects()
        .iter()
        .find(|o| o.kind == ObjectKind::Heap);
    let first = registry.objects().first();
    let mut checkpoint_samples = Vec::new();
    let mut observation_samples = Vec::new();
    let mut heap_samples = Vec::new();
    let before_allocs = ALLOCS.load(Ordering::Relaxed);
    for _ in 0..500 {
        if !store.trace().snapshots.is_empty() {
            let now = Instant::now();
            let count = registry.objects_at_checkpoint(0)?.count();
            std::hint::black_box(count);
            checkpoint_samples.push(now.elapsed().as_nanos() as u64);
        }
        if let Some(first) = first.filter(|_| !store.trace().snapshots.is_empty()) {
            let now = Instant::now();
            let count = registry.observation_at(&first.id, 0)?.count();
            std::hint::black_box(count);
            observation_samples.push(now.elapsed().as_nanos() as u64);
        }
        if let Some(heap) = heap {
            let now = Instant::now();
            std::hint::black_box(registry.heap_by_id(heap.allocation_id.unwrap()));
            heap_samples.push(now.elapsed().as_nanos() as u64);
        }
    }
    // Instrumentation Vec pushes above allocate; isolate an allocation-free
    // warmed set of queries without timing/allocation counter interference.
    let mut check = 0usize;
    let query_before = ALLOCS.load(Ordering::Relaxed);
    for _ in 0..1000 {
        if !store.trace().snapshots.is_empty() {
            check += registry.objects_at_checkpoint(0)?.count();
        }
        if let Some(first) = first.filter(|_| !store.trace().snapshots.is_empty()) {
            check += registry.observation_at(&first.id, 0)?.count();
        }
        if let Some(heap) = heap {
            check += registry.heap_by_id(heap.allocation_id.unwrap()).is_some() as usize;
        }
    }
    std::hint::black_box(check);
    let query_allocs = ALLOCS.load(Ordering::Relaxed) - query_before;
    println!("file={path} parse+index_us={parsed_us} validate_us={validation_us} build_us={build_us} objects={} observations={} registry_est_bytes={} live_delta_bytes={live_delta_bytes} rss_before_kib={before} rss_after_kib={after} peak_kib={} checkpoint_median_ns={} observation_median_ns={} heap_median_ns={} query_allocs={} benchmark_setup_allocs={}",
        registry.objects().len(), registry.observation_count(), registry.estimated_bytes(),
        memory_kib("VmHWM:"),
        if checkpoint_samples.is_empty(){0}else{median(checkpoint_samples)},
        if observation_samples.is_empty(){0}else{median(observation_samples)},
        if heap_samples.is_empty(){0}else{median(heap_samples)},
        query_allocs,query_before-before_allocs);
    if let Some(heap) = heap {
        let lifetime = registry.heap_lifetime(&heap.id)?.unwrap();
        println!(
            "first_heap_id={} pid={:?} identity={:?} final_status={:?}",
            heap.allocation_id.unwrap(),
            heap.pid,
            lifetime.identity_confidence,
            lifetime.status
        );
    }
    Ok(())
}

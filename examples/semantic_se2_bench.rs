//! Read-only usage and reproducible release benchmark, one trace per process.
//! cargo run --release --example semantic_se2_bench -- trace.json [independently-verified-target.elf]
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use c_trace_engine::semantic::{LayoutEngine, LayoutKind, ObjectRegistry, TargetFormat};
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};

struct Counter;
static COUNT: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicI64 = AtomicI64::new(0);
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            COUNT.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_add(l.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            COUNT.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_add(l.size() as i64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            COUNT.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_add(n as i64 - l.size() as i64, Ordering::Relaxed);
        }
        q
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as i64, Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOC: Counter = Counter;
fn rss(key: &str) -> u64 {
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
/// 512 warmed repetitions; exclude the preallocated timing vector itself.
fn timing(mut f: impl FnMut()) -> (u64, u64, i64) {
    let mut times = Vec::with_capacity(512);
    let allocations = COUNT.load(Ordering::Relaxed);
    let live = LIVE.load(Ordering::Relaxed);
    for _ in 0..512 {
        let s = Instant::now();
        f();
        times.push(s.elapsed().as_nanos() as u64);
    }
    let count = COUNT.load(Ordering::Relaxed) - allocations;
    let net = LIVE.load(Ordering::Relaxed) - live;
    times.sort_unstable();
    (times[256], count, net)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let file = args.next().ok_or("provide trace JSON")?;
    let target = args
        .next()
        .map(|path| TargetFormat::inspect_elf(std::path::Path::new(&path)))
        .transpose()?;
    let store = TraceStore::open(&file, TraceLoadOptions::default())?;
    let view = store.validated_semantic();
    let rss_before = rss("VmRSS:");
    let live_before = LIVE.load(Ordering::Relaxed);
    let start = Instant::now();
    let registry = ObjectRegistry::build(&view)?;
    let se1_build_us = start.elapsed().as_micros();
    let se1_delta = LIVE.load(Ordering::Relaxed) - live_before;
    let rss_se1 = rss("VmRSS:");
    let live_before = LIVE.load(Ordering::Relaxed);
    let start = Instant::now();
    let engine = if let Some(target) = target {
        LayoutEngine::with_target(&registry, target)
    } else {
        LayoutEngine::new(&registry)
    };
    let se2_new_ns = start.elapsed().as_nanos();
    let se2_delta = LIVE.load(Ordering::Relaxed) - live_before;
    let rss_se2 = rss("VmRSS:");
    let mut scalar = None;
    let mut compound = None;
    let mut array = None;
    let mut opaque = None;
    let mut nested = None;
    let mut recursive = None;
    for obj in registry.objects() {
        let Some(obs) = registry.observations(&obj.id)?.next() else {
            continue;
        };
        let Ok(layout) = engine.layout_for_object(&obj.id, obs.record.checkpoint) else {
            continue;
        };
        if obj.name == Some("nested") && nested.is_none() {
            nested = Some((obj.id.clone(), obs.record.checkpoint));
        }
        if obj.name == Some("node") && recursive.is_none() {
            recursive = Some((obj.id.clone(), obs.record.checkpoint));
        }
        match layout.root().kind {
            LayoutKind::Scalar
                if scalar.is_none()
                    && (target.is_none() || layout.decode(&layout.root()).value.is_some()) =>
            {
                scalar = Some((obj.id.clone(), obs.record.checkpoint))
            }
            LayoutKind::Struct | LayoutKind::Union if compound.is_none() => {
                compound = Some((obj.id.clone(), obs.record.checkpoint))
            }
            LayoutKind::Array if array.is_none() || obj.name == Some("large") => {
                array = Some((obj.id.clone(), obs.record.checkpoint))
            }
            LayoutKind::Opaque if opaque.is_none() => {
                opaque = Some((obj.id.clone(), obs.record.checkpoint))
            }
            _ => {}
        }
    }
    let mut samples = Vec::new();
    if let Some((id, checkpoint)) = scalar
        .as_ref()
        .or(compound.as_ref())
        .or(array.as_ref())
        .or(opaque.as_ref())
    {
        let root = engine.layout_for_object(id, *checkpoint)?.root();
        let (ns, allocs, net) = timing(|| {
            std::hint::black_box(registry.observation_at(id, *checkpoint).unwrap().count());
        });
        samples.push(("se1_observation_ns", ns, allocs, net));
        let (ns, allocs, net) = timing(|| {
            std::hint::black_box(engine.layout_for_object(id, *checkpoint).unwrap().root());
        });
        samples.push(("root_ns", ns, allocs, net));
        if matches!(
            root.kind,
            LayoutKind::Scalar | LayoutKind::Enum | LayoutKind::Pointer
        ) {
            let layout = engine.layout_for_object(id, *checkpoint)?;
            let (ns, allocs, net) = timing(|| {
                std::hint::black_box(layout.decode(&root));
            });
            samples.push(("decode_ns", ns, allocs, net));
        }
    }
    if let Some((id, checkpoint)) = &compound {
        let l = engine.layout_for_object(id, *checkpoint)?;
        let root = l.root();
        if let Some(name) = root
            .type_entry()
            .and_then(|ty| ty.members.first())
            .and_then(|m| m.name.as_deref())
        {
            let (ns, allocs, net) = timing(|| {
                std::hint::black_box(l.member(&root, name).unwrap());
            });
            samples.push(("member_ns", ns, allocs, net));
        }
    }
    if let Some((id, checkpoint)) = &nested {
        let l = engine.layout_for_object(id, *checkpoint)?;
        let root = l.root();
        let (ns, allocs, net) = timing(|| {
            let child = l.member(&root, "node").unwrap().unwrap();
            std::hint::black_box(l.member(&child, "value").unwrap());
        });
        samples.push(("nested_member_ns", ns, allocs, net));
    }
    if let Some((id, checkpoint)) = &recursive {
        let l = engine.layout_for_object(id, *checkpoint)?;
        let root = l.root();
        let (ns, allocs, net) = timing(|| {
            let pointer = l.member(&root, "next").unwrap().unwrap();
            std::hint::black_box(l.children_count(&pointer).unwrap());
        });
        samples.push(("recursive_pointer_ns", ns, allocs, net));
    }
    if let Some((id, checkpoint)) = &array {
        let l = engine.layout_for_object(id, *checkpoint)?;
        let root = l.root();
        if let Ok(count) = l.children_count(&root) {
            if count > 0 {
                let (ns, allocs, net) = timing(|| {
                    std::hint::black_box(l.array_element(&root, count - 1).unwrap());
                });
                samples.push(("element_ns", ns, allocs, net));
                let end = count.min(32);
                let (ns, allocs, net) = timing(|| {
                    std::hint::black_box(l.children_page(&root, 0..end).unwrap());
                });
                samples.push(("page_max32_ns", ns, allocs, net));
            }
        }
    }
    println!("file={file} snapshots={} objects={} observations={} se1_build_us={se1_build_us} se1_live_bytes={se1_delta} se2_new_ns={se2_new_ns} se2_live_bytes={se2_delta} rss_before_kib={rss_before} rss_se1_kib={rss_se1} rss_se2_kib={rss_se2} peak_kib={} target_present={}",
        store.trace().snapshots.len(),registry.objects().len(),registry.observation_count(),rss("VmHWM:"),target.is_some());
    for (label, ns, allocs, net) in samples {
        println!("{label}={ns} {label}_allocs={allocs} {label}_net_bytes={net}");
    }
    Ok(())
}

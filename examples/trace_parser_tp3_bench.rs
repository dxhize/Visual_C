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
        // TP-5 metadata probes: borrowed canonical slices, no new index.
        bench("heap process scope typed", 20_000, |_| {
            black_box(store.heap_process_scope());
            store.heap_history().is_some() as usize
        });
        bench("type registry borrowed", 20_000, |_| {
            store
                .type_registry()
                .map_or(0, |registry| black_box(registry.types.len()))
        });
        bench("evidence fusion borrowed", 20_000, |_| {
            store.evidence_fusion().is_some() as usize
        });
        if let Some(id) = store
            .trace()
            .source_metadata
            .as_ref()
            .and_then(|registry| registry.expressions.first())
            .map(|expression| expression.id.as_str())
        {
            bench("source expression by ID", 20_000, |_| {
                black_box(store.source_expression(id).unwrap()).id.len()
            });
        }
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
        // TP-6 opt-in validation happens ONCE. Query loops use only borrowed
        // canonical evidence; measured allocations must remain zero.
        let before_live = LIVE.load(Ordering::Relaxed);
        let before_allocs = ALLOCS.load(Ordering::Relaxed);
        let start = Instant::now();
        let validated = store.validated_semantic();
        println!(
            "tp6_context={:?} findings={} allocs={} live_delta={} bytes rss_after={} KiB",
            start.elapsed(),
            validated.report().findings().len(),
            ALLOCS.load(Ordering::Relaxed) - before_allocs,
            LIVE.load(Ordering::Relaxed) - before_live,
            rss_kib(),
        );
        bench("tp6 process coverage", 20_000, |_| {
            black_box(validated.process_coverage()).history.is_some() as usize
        });
        if snapshot_len != 0 {
            bench("tp6 checkpoint varied", 20_000, |i| {
                black_box(validated.checkpoint(i % snapshot_len).unwrap())
                    .context
                    .snapshot_index()
            });
        }
        if let Some((snapshot, variable)) = store
            .trace()
            .snapshots
            .iter()
            .enumerate()
            .find_map(|(index, s)| (!s.variables.is_empty()).then_some((index, 0)))
        {
            let reference = store
                .evidence_ref(EvidenceLocation::SnapshotVariable { snapshot, variable })
                .unwrap();
            bench("tp6 variable join", 20_000, |_| {
                black_box(validated.variable(&reference).unwrap())
                    .info
                    .name
                    .len()
            });
        }
        if let Some(lifetime) = store.allocation_lifetimes().first() {
            bench("tp6 heap identity", 20_000, |_| {
                black_box(validated.heap(lifetime.allocation_id))
                    .events()
                    .count()
            });
        }
        if let Some(id) = store
            .trace()
            .source_metadata
            .as_ref()
            .and_then(|r| r.expressions.first())
            .map(|e| e.id.as_str())
        {
            bench("tp6 source expression", 20_000, |_| {
                black_box(validated.source_expression(id))
                    .heap_call_candidates()
                    .count()
            });
        }
        if snapshot_len != 0 {
            let live_before = LIVE.load(Ordering::Relaxed);
            let alloc_before = ALLOCS.load(Ordering::Relaxed);
            let summary = validated.checkpoint_summary(0).unwrap();
            println!(
                "tp7_summary_live_delta={} bytes allocs={} rss_after={} KiB",
                LIVE.load(Ordering::Relaxed) - live_before,
                ALLOCS.load(Ordering::Relaxed) - alloc_before,
                rss_kib()
            );
            black_box(&summary);
            bench("tp7 summary varied", 20_000, |i| {
                black_box(validated.checkpoint_summary(i % snapshot_len).unwrap()).snapshot_index()
            });
            let first_with_locals = store
                .trace()
                .snapshots
                .iter()
                .position(|s| !s.variables.is_empty());
            if let Some(index) = first_with_locals {
                let summary = validated.checkpoint_summary(index).unwrap();
                let page_len = summary.checkpoint.context.variables().len().min(16);
                bench("tp7 local page <=16", 5_000, |_| {
                    summary
                        .variables_page(
                            c_trace_engine::trace_parser::VariableScope::Local,
                            0..page_len,
                        )
                        .unwrap()
                        .map(|item| item.unwrap().info.name.len())
                        .sum()
                });
            }
            let page_len = summary
                .checkpoint
                .context
                .events_at_checkpoint()
                .unwrap()
                .len()
                .min(16);
            bench("tp7 checkpoint event page", 5_000, |_| {
                summary
                    .instruction_events_page(0..page_len)
                    .unwrap()
                    .map(|item| item.unwrap().position)
                    .fold(0usize, usize::wrapping_add)
            });
            if let Some((index, expression_count)) = store
                .trace()
                .snapshots
                .iter()
                .enumerate()
                .find_map(|(index, snapshot)| {
                    let file = snapshot
                        .call_stack
                        .first()
                        .and_then(|f| f.full_file.as_deref())
                        .unwrap_or(&snapshot.file);
                    let count = store
                        .trace()
                        .source_metadata
                        .as_ref()?
                        .expressions
                        .iter()
                        .filter(|expr| {
                            expr.span.file == file
                                && expr.span.start_line <= snapshot.line
                                && snapshot.line <= expr.span.end_line
                        })
                        .count();
                    (count != 0).then_some((index, count))
                })
            {
                let source = validated.checkpoint_summary(index).unwrap();
                bench("tp7 static source page", 2_000, |_| {
                    source
                        .source_expressions_page(0..expression_count.min(16))
                        .unwrap()
                        .count()
                });
            }
        }
    }
    // Bounded large-result oracle, independent of the real 1000-snapshot
    // source trace (which has very few variables per checkpoint).
    let mut trace = DebugSessionTrace::default();
    trace
        .snapshots
        .push(c_trace_engine::models::ExecutionSnapshot {
            observation: c_trace_engine::models::SnapshotObservation {
                stop_id: 1,
                ..Default::default()
            },
            variables: (0..1024)
                .map(|i| c_trace_engine::models::VariableInfo {
                    name: format!("v{i}"),
                    read_status: c_trace_engine::models::DataValidity::Valid,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
    let store = TraceStore::from_trace(trace).unwrap();
    let validated = store.validated_semantic();
    let summary = validated.checkpoint_summary(0).unwrap();
    println!(
        "TP7_LARGE_PAGE vars=1024 max_page={} rss={} KiB",
        c_trace_engine::trace_parser::MAX_CONSUMER_PAGE,
        rss_kib()
    );
    bench("tp7 128-variable page", 300, |i| {
        let start = (i % 8) * 128;
        summary
            .variables_page(
                c_trace_engine::trace_parser::VariableScope::Local,
                start..start + 128,
            )
            .unwrap()
            .map(|item| item.unwrap().info.name.len())
            .sum()
    });
    bench("tp7 eight bounded pages", 40, |_| {
        (0..8)
            .map(|page| {
                summary
                    .variables_page(
                        c_trace_engine::trace_parser::VariableScope::Local,
                        page * 128..(page + 1) * 128,
                    )
                    .unwrap()
                    .map(|item| item.unwrap().info.name.len())
                    .sum::<usize>()
            })
            .sum()
    });
    bench("tp7 oversized page rejected", 5_000, |_| {
        usize::from(
            summary
                .variables_page(c_trace_engine::trace_parser::VariableScope::Local, 0..129)
                .is_err(),
        )
    });
}

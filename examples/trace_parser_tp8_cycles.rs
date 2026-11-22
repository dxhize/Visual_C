//! Independent repeated-load / multi-reader cleanup probe; RSS is process RSS,
//! not the trace's retained allocation size. One process per input.
use c_trace_engine::trace_parser::{TraceLoadOptions, TraceStore};
use std::hint::black_box;
use std::time::Instant;
fn rss() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u64>()
        .unwrap()
        * 4
}
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let mut peak = 0;
    let start = rss();
    for i in 0..12 {
        let t = Instant::now();
        {
            let store = TraceStore::open(&path, TraceLoadOptions::default()).unwrap();
            let validated = store.validated_semantic();
            assert!(validated.report().is_structurally_sound());
            let clones: Vec<_> = (0..4).map(|_| store.clone()).collect();
            assert!(clones
                .iter()
                .all(|s| std::ptr::eq(s.trace(), store.trace())));
            std::thread::scope(|scope| {
                for reader in &clones {
                    scope.spawn(move || {
                        let view = reader.validated_semantic();
                        for _ in 0..20 {
                            black_box(view.execution().retained);
                            if !reader.trace().snapshots.is_empty() {
                                black_box(view.checkpoint_summary(0).unwrap().stop_id());
                            }
                        }
                    });
                }
            });
            peak = peak.max(rss());
        }
        println!("cycle={i} load_read_us={} after_drop_rss_kib={} peak_rss_kib={peak} baseline_kib={start}",t.elapsed().as_micros(),rss());
    }
}

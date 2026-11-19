//! Measure opt-in deep reference validation over one canonical trace.
//! Usage: cargo run --release --example phase8r3_validation_bench -- TRACE.json
use c_trace_engine::trace_parser::TraceStore;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("TRACE.json path");
    let start = Instant::now();
    let store = TraceStore::open(path, Default::default()).unwrap();
    let load = start.elapsed();
    let start = Instant::now();
    let iterations = 100;
    let mut findings = 0;
    for _ in 0..iterations {
        findings += std::hint::black_box(store.validate_deep()).findings().len();
    }
    println!(
        "load={load:?} validate_100={:?} findings_total={findings}",
        start.elapsed()
    );
    for finding in store.validate_deep().findings() {
        println!(
            "{:?} {} ({}): {}",
            finding.severity, finding.code, finding.occurrences, finding.detail
        );
    }
}

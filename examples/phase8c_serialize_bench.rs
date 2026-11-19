//! Measure streaming final JSON serialization without allocating a JSON String.
//! Usage: cargo run --release --example phase8c_serialize_bench -- TRACE.json
use c_trace_engine::models::DebugSessionTrace;
use std::io::{self, Write};
use std::time::Instant;

#[derive(Default)]
struct Counter(u64);
impl Write for Counter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0 += data.len() as u64;
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
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
    let file = std::env::args().nth(1).expect("TRACE.json required");
    let trace: DebugSessionTrace = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    for run in 0..3 {
        let mut output = Counter::default();
        let start = Instant::now();
        serde_json::to_writer(&mut output, &trace).unwrap();
        println!(
            "run={run} seconds={:.6} bytes={} rss_kib={}",
            start.elapsed().as_secs_f64(),
            output.0,
            rss_kib()
        );
    }
}

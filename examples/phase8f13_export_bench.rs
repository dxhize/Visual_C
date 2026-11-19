//! Bounded final JSON export benchmark; reads an existing trace, no debugger.
//! cargo run --release --example phase8f13_export_bench -- TRACE.json
use c_trace_engine::{json_export, DebugSessionTrace};
use std::io::{self, Write};
use std::time::Instant;
struct Counter(usize);
impl Write for Counter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.0 += data.len();
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn peak_rss_kib() -> String {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .unwrap()
        .trim()
        .to_owned()
}
fn main() {
    let file = std::env::args().nth(1).expect("TRACE.json required");
    let trace: DebugSessionTrace = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let mut oracle = Counter(0);
    serde_json::to_writer(&mut oracle, &trace).unwrap();
    let measured = oracle.0 + 1;
    for limit in [measured - 1, measured, measured * 2] {
        let mut output = Counter(0);
        let start = Instant::now();
        let result = json_export::export_json(&trace, false, limit, None, &mut output);
        println!("limit={limit} success={} elapsed_ms={:.3} output_bytes={} independent_bytes={measured} peak_rss={}",
            result.is_ok(), start.elapsed().as_secs_f64() * 1000.0, output.0, peak_rss_kib());
    }
}

//! CLI entry point.
//!
//! Exit codes:
//! * `0` — a trace was produced (execution completed, crashed, timed out,
//!   hit a step limit, ...).
//! * `2` — compilation failed; a partial trace with the compiler diagnostics
//!   is still emitted (on stdout, as JSON).
//! * `1` — operational error before/around tracing (bad arguments, missing
//!   source file, missing required tool): message on stderr, no trace.

use std::io::Write;
use std::process::ExitCode;

use clap::Parser;

use c_trace_engine::cli::Cli;
use c_trace_engine::trace;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = match cli.to_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    match trace::trace_program(config.clone()).await {
        Ok(trace) => {
            let json = if config.pretty {
                serde_json::to_string_pretty(&trace)
            } else {
                serde_json::to_string(&trace)
            };
            match json {
                Ok(json) => {
                    if let Some(out) = &config.output_file {
                        if let Err(e) = std::fs::write(out, format!("{json}\n")) {
                            eprintln!("error: cannot write output file {}: {e}", out.display());
                            return ExitCode::from(1);
                        }
                    }
                    // Trace to stdout. When a file was requested too, the
                    // stdout stream is the same JSON document.
                    let mut stdout = std::io::stdout().lock();
                    let _ = writeln!(stdout, "{json}");
                    let _ = stdout.flush();

                    if !trace.compiled_successfully {
                        return ExitCode::from(2);
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: cannot serialize trace: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

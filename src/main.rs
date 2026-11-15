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
            let write_json = |writer: &mut dyn Write| -> std::io::Result<()> {
                if config.pretty {
                    serde_json::to_writer_pretty(&mut *writer, &trace)?;
                } else {
                    serde_json::to_writer(&mut *writer, &trace)?;
                }
                writeln!(writer)?;
                writer.flush()
            };
            let output = (|| -> std::io::Result<()> {
                if let Some(out) = &config.output_file {
                    let file = std::fs::File::create(out)?;
                    write_json(&mut std::io::BufWriter::new(file))?;
                }
                let mut stdout = std::io::stdout().lock();
                write_json(&mut stdout)
            })();
            match output {
                Ok(()) => {
                    if trace.execution_status
                        == c_trace_engine::models::ExecutionStatus::CompileFailed
                    {
                        ExitCode::from(2)
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(e) => {
                    eprintln!("error: cannot write trace: {e}");
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

//! CLI entry point.
//!
//! Exit codes:
//! * `0` — a trace was produced (execution completed, crashed, timed out,
//!   hit a step limit, ...).
//! * `2` — compilation failed; a partial trace with the compiler diagnostics
//!   is still emitted (on stdout, as JSON).
//! * `1` — operational error before/around tracing (bad arguments, missing
//!   source file, missing required tool): message on stderr, no trace.

use std::process::ExitCode;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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
            let mut sigint =
                match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()) {
                    Ok(signal) => signal,
                    Err(e) => {
                        eprintln!("error: cannot monitor JSON export cancellation: {e}");
                        return ExitCode::from(1);
                    }
                };
            let cancelled = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&cancelled);
            let watcher = tokio::spawn(async move {
                sigint.recv().await;
                flag.store(true, Ordering::SeqCst);
            });
            let output = c_trace_engine::json_export::export_json_cancellable(
                &trace,
                config.pretty,
                config.limits.max_json_export_bytes,
                config.output_file.as_deref(),
                &mut std::io::stdout().lock(),
                Some(&cancelled),
            );
            watcher.abort();
            match output {
                Ok(_) => {
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

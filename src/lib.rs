//! # c-trace-engine
//!
//! Standalone Rust backend that compiles a configurable C project with GCC,
//! executes it under GDB (Machine Interface 2), records execution snapshots
//! and heap allocation events, performs an independent Valgrind analysis, and
//! emits one structured, serializable [`models::DebugSessionTrace`].
//!
//! Module map (see README for the full architecture):
//!
//! | Module                | Responsibility                                        |
//! |-----------------------|-------------------------------------------------------|
//! | `models`              | Serializable trace data model                          |
//! | `config`              | `BuildConfig`, `TraceConfig`, JSON config files        |
//! | `resource_limits`     | `ExecutionLimits` + rlimit/cgroup enforcement          |
//! | `compiler`            | GCC invocation + JSON/SARIF diagnostic parsing         |
//! | `mi_parser`           | GDB MI2 tokenizer + recursive-descent parser (`nom`)   |
//! | `gdb_mi`              | Async GDB session: pty, tokens, stepping, snapshots    |
//! | `allocation_tracker`  | C instrumentation runtime + event ingestion            |
//! | `valgrind`            | Independent Valgrind run + XML parsing                 |
//! | `sanitizer`           | Separate ASan/UBSan diagnostic pass                    |
//! | `trace`               | Orchestrates the complete pipeline                     |
//! | `cli`                 | Argument parsing and config merging                    |

pub mod allocation_tracker;
pub mod cli;
pub mod compiler;
pub mod config;
pub mod error;
pub mod gdb_mi;
pub mod memory;
pub mod mi_parser;
pub mod models;
pub mod processor;
pub mod recorder;
pub mod resource_limits;
pub mod sanitizer;
pub mod storage;
pub mod supervisor;
pub mod trace;
pub mod valgrind;

pub use config::{BuildConfig, TraceConfig};
pub use error::{EngineError, Result};
pub use models::DebugSessionTrace;
pub use resource_limits::ExecutionLimits;

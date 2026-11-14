//! Structured error types for the whole engine.
//!
//! Every fallible operation returns `Result<T, EngineError>`; failures are
//! never silently ignored. Errors that map to user-visible states (missing
//! tools, compiler failures, timeouts) are preserved so the orchestrator can
//! surface them in the serialized trace.

use thiserror::Error;

/// Top-level error type for the tracing engine.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("required tool `{tool}` is not available: {reason}")]
    MissingTool { tool: String, reason: String },

    #[error("compilation failed")]
    CompilationFailed,

    #[error("gdb session error: {0}")]
    Gdb(String),

    #[error("gdb command `{command}` timed out after {timeout_secs}s")]
    GdbCommandTimeout {
        command: String,
        timeout_secs: u64,
    },

    #[error("gdb command `{command}` failed: {message}")]
    GdbCommandError {
        command: String,
        message: String,
    },

    #[error("MI parse error: {0}")]
    MiParse(#[from] crate::mi_parser::MiParseError),

    #[error("allocation event parse error (line {line}): {message}")]
    AllocationEventParse { line: usize, message: String },

    #[error("valgrind error: {0}")]
    Valgrind(String),

    #[error("resource limit enforcement error: {0}")]
    ResourceLimit(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

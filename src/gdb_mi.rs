//! GDB process management and execution tracing over MI2.
//!
//! The session owns one `gdb --interpreter=mi2` child placed in its own
//! process group with inherited rlimits, one background reader task that
//! parses every MI line and routes it (command responses by token, async
//! records through a channel), and one pty reader task for the inferior's
//! own stdout/stderr.
//!
//! Program output never mixes with the MI protocol: the inferior is attached
//! to a dedicated pseudo-terminal (`-inferior-tty-set`), so its output flows
//! to a private buffer while MI traffic flows through pipes. Stdout and
//! stderr are merged on that pty (they cannot be separated on a tty); the
//! sanitizer pass captures them separately instead.
//!
//! Communication model: strictly one command in flight. Every command
//! carries a unique numeric token and is answered by the matching result
//! record; stream records observed in the meantime are returned with the
//! response (needed for console-command output like `whatis`). Async
//! `*running` / `*stopped` / notification records are collected on a
//! separate channel independent of command responses.

use std::collections::{HashMap, HashSet};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};

use crate::error::{EngineError, Result};
use crate::mi_parser::{
    as_const, as_tuple, parse_line, MiListItem, MiRecord, MiValue, StreamKind, Token,
};
use crate::models::{StackFrameInfo, VariableInfo};
use crate::resource_limits::kill_process_group;

const COMMAND_TIMEOUT_SECS: u64 = 20;
const MAX_CONSOLE_LOG_BYTES: usize = 256 * 1024;
const MAX_FRAMES_PER_SNAPSHOT: usize = 200;

// ---------------------------------------------------------------------------
// Pseudo-terminal for the inferior's own output
// ---------------------------------------------------------------------------

pub struct PtyPair {
    master: OwnedFd,
    pub slave_path: String,
}

/// Create a pty pair; the slave is handed to GDB via `-inferior-tty-set`,
/// the master is read by the engine to capture program output.
pub fn create_pty() -> Result<PtyPair> {
    unsafe {
        let master_fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        if master_fd < 0 {
            return Err(EngineError::Io(std::io::Error::last_os_error()));
        }
        if libc::grantpt(master_fd) != 0 || libc::unlockpt(master_fd) != 0 {
            let e = std::io::Error::last_os_error();
            libc::close(master_fd);
            return Err(e.into());
        }
        let mut buf = [0u8; 128];
        let rc = libc::ptsname_r(master_fd, buf.as_mut_ptr() as *mut libc::c_char, buf.len());
        if rc != 0 {
            let e = std::io::Error::last_os_error();
            libc::close(master_fd);
            return Err(e.into());
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        let slave_path = String::from_utf8_lossy(&buf[..len]).into_owned();
        // Non-blocking master: AsyncFd readiness handling relies on it.
        let flags = libc::fcntl(master_fd, libc::F_GETFL);
        let _ = libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        Ok(PtyPair {
            master: OwnedFd::from_raw_fd(master_fd),
            slave_path,
        })
    }
}

/// Inferior output buffer (pty bytes), bounded by `max_output_bytes`.
#[derive(Default)]
pub struct ProgramBuffer {
    pub data: Vec<u8>,
    pub total_bytes: u64,
    pub truncated: bool,
}

impl ProgramBuffer {
    fn push(&mut self, bytes: &[u8], cap: usize) {
        self.total_bytes += bytes.len() as u64;
        if self.data.len() < cap {
            let remaining = cap - self.data.len();
            let take = bytes.len().min(remaining);
            self.data.extend_from_slice(&bytes[..take]);
        }
        if self.data.len() >= cap {
            self.truncated = true;
        }
    }

    /// Captured output as a lossy UTF-8 string (head-truncated by the cap).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).into_owned()
    }
}

// ---------------------------------------------------------------------------
// Session plumbing
// ---------------------------------------------------------------------------

type StreamRecord = (StreamKind, String);
type CommandOutcome = std::result::Result<(MiRecord, Vec<StreamRecord>), String>;

struct Shared {
    pending: Mutex<HashMap<u64, oneshot::Sender<CommandOutcome>>>,
    current_streams: Mutex<Vec<StreamRecord>>,
    async_tx: mpsc::UnboundedSender<MiRecord>,
    console: Mutex<String>,
    program: Mutex<ProgramBuffer>,
    eof: AtomicBool,
    output_cap: usize,
    /// When true (C_TRACE_DEBUG=1), every raw MI line is mirrored into the
    /// console buffer for post-mortem diagnostics.
    debug_echo: bool,
}

/// Raw facts of a `*stopped` record.
#[derive(Debug, Clone, Default)]
pub struct StoppedInfo {
    pub reason: Option<String>,
    pub signal_name: Option<String>,
    pub signal_meaning: Option<String>,
    pub frame: Option<StackFrameInfo>,
    pub thread_id: Option<String>,
    pub exit_code: Option<String>,
    pub bkptno: Option<String>,
    /// Full original record (for details we did not promote to fields).
    pub raw: Option<MiRecord>,
}

impl StoppedInfo {
    pub fn is_exit(&self) -> bool {
        matches!(self.reason.as_deref(), Some("exited-normally") | Some("exited"))
    }

    pub fn is_signal(&self) -> bool {
        self.reason.as_deref() == Some("signal-received")
    }
}

/// Options for launching a GDB session.
pub struct GdbLaunchOptions {
    pub binary: PathBuf,
    pub working_directory: Option<PathBuf>,
    pub program_args: Vec<String>,
    pub limits: crate::resource_limits::ExecutionLimits,
    /// Extra environment variables for GDB; the inferior inherits GDB's
    /// environment, so this is how the allocation event file is advertised.
    pub env: Vec<(String, String)>,
}

pub struct MiCommandResponse {
    pub record: MiRecord,
    pub streams: Vec<StreamRecord>,
}

impl MiCommandResponse {
    /// Value of `^done,value="..."` (expression evaluation).
    pub fn value(&self) -> Option<&str> {
        as_const(self.record.result("value")?)
    }

    /// Whether the response is a `^error` and its message.
    pub fn error_message(&self) -> Option<String> {
        if self.record.class() == Some("error") {
            Some(
                as_const(self.record.result("msg")?)
                    .unwrap_or("unknown error")
                    .to_string(),
            )
        } else {
            None
        }
    }
}

pub struct GdbSession {
    child: Child,
    stdin: ChildStdin,
    shared: Arc<Shared>,
    async_rx: mpsc::UnboundedReceiver<MiRecord>,
    backlog: Vec<MiRecord>,
    next_token: u64,
    reader_task: tokio::task::JoinHandle<()>,
    pty_task: Option<tokio::task::JoinHandle<()>>,
    pub inferior_pid: Option<u32>,
    pub commands_issued: Arc<AtomicU64>,
}

impl GdbSession {
    /// Launch `gdb --nx --quiet --interpreter=mi2 <binary>` and apply session
    /// configuration (pagination off, debuginfod off, inferior tty, working
    /// directory, program arguments).
    pub async fn launch(opts: &GdbLaunchOptions) -> Result<GdbSession> {
        let pty = create_pty()?;
        let mut cmd = tokio::process::Command::new("gdb");
        cmd.args(["--nx", "--quiet", "--interpreter=mi2"])
            .arg(&opts.binary)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        if let Some(dir) = &opts.working_directory {
            cmd.current_dir(dir);
        }
        for (key, value) in &opts.env {
            cmd.env(key, value);
        }
        cmd.process_group(0);
        // No pre_exec rlimits here: GDB needs unrestricted address space for
        // symbol loading and its own bookkeeping. The inferior's budget is
        // enforced by the engine's wall/step deadlines and by the cgroup when
        // a delegated subtree is available.

        let mut child = cmd.spawn().map_err(|e| EngineError::MissingTool {
            tool: "gdb".into(),
            reason: format!("could not spawn gdb: {e}"),
        })?;
        let stdin = child.stdin.take().expect("gdb stdin");
        let stdout: ChildStdout = child.stdout.take().expect("gdb stdout");

        // ONE channel between the reader task and the session: the tx goes
        // into `shared` (used by `route_record`), the rx is held here.
        let (async_tx, async_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            current_streams: Mutex::new(Vec::new()),
            async_tx,
            console: Mutex::new(String::new()),
            program: Mutex::new(ProgramBuffer::default()),
            eof: AtomicBool::new(false),
            output_cap: opts.limits.max_output_bytes,
            debug_echo: std::env::var_os("C_TRACE_DEBUG").is_some(),
        });

        let reader_task = tokio::spawn(reader_task(stdout, shared.clone()));
        let slave_path = pty.slave_path.clone();
        // The pty task owns the master fd exclusively; it is aborted (and the
        // fd closed by dropping its OwnedFd) in shutdown/kill/Drop.
        let pty_task = tokio::spawn(pty_reader_task(pty.master, shared.clone()));

        let mut session = Self {
            child,
            stdin,
            shared,
            async_rx,
            backlog: Vec::new(),
            next_token: 1,
            reader_task,
            pty_task: Some(pty_task),
            inferior_pid: None,
            commands_issued: Arc::new(AtomicU64::new(0)),
        };

        // --- session configuration ---
        session.execute("-gdb-set pagination off").await?;
        session.execute("-gdb-set confirm off").await?;
        session.execute("-gdb-set debuginfod enabled off").await?;
        session.execute("-gdb-set mi-async off").await?;
        session
            .execute(&format!("-inferior-tty-set {}", slave_path))
            .await?;
        if let Some(dir) = &opts.working_directory {
            session
                .execute(&format!(
                    "-environment-cd {}",
                    quote_for_mi(&dir.display().to_string())
                ))
                .await?;
        }
        if !opts.program_args.is_empty() {
            let args: Vec<String> = opts.program_args.iter().map(|a| quote_for_mi(a)).collect();
            session.execute(&format!("-exec-arguments {}", args.join(" "))).await?;
        }
        Ok(session)
    }

    /// Send one MI command and await its token-matched result.
    pub async fn execute(&mut self, command: &str) -> Result<MiCommandResponse> {
        let command = command.trim();
        if self.shared.eof.load(Ordering::SeqCst) {
            return Err(EngineError::Gdb("gdb session already closed".into()));
        }
        let token = self.next_token;
        self.next_token += 1;
        self.commands_issued.fetch_add(1, Ordering::SeqCst);

        // Streams accumulated by earlier commands (or by async chatter) must
        // not leak into this command's response.
        self.shared.current_streams.lock().unwrap().clear();

        let (tx, rx) = oneshot::channel();
        self.shared.pending.lock().unwrap().insert(token, tx);

        let line = format!("{}{}\n", token, command);
        if let Err(e) = self.stdin.write_all(line.as_bytes()).await {
            self.shared.eof.store(true, Ordering::SeqCst);
            return Err(EngineError::Gdb(format!("writing to gdb failed: {e}")));
        }
        if let Err(e) = self.stdin.flush().await {
            return Err(EngineError::Gdb(format!("flushing to gdb failed: {e}")));
        }

        match tokio::time::timeout(Duration::from_secs(COMMAND_TIMEOUT_SECS), rx).await {
            Ok(Ok(Ok((record, streams)))) => Ok(MiCommandResponse { record, streams }),
            Ok(Ok(Err(e))) => Err(EngineError::Gdb(e)),
            Ok(Err(_)) => {
                self.shared.pending.lock().unwrap().remove(&token);
                Err(EngineError::Gdb("gdb dropped the response channel".into()))
            }
            Err(_) => {
                self.shared.pending.lock().unwrap().remove(&token);
                Err(EngineError::GdbCommandTimeout {
                    command: command.to_string(),
                    timeout_secs: COMMAND_TIMEOUT_SECS,
                })
            }
        }
    }

    /// Await the next `*stopped` record, buffering unrelated async records.
    pub async fn wait_stopped(&mut self, timeout: Duration) -> Result<StoppedInfo> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(EngineError::Gdb("timed out waiting for *stopped".into()));
            }
            match tokio::time::timeout(remaining, self.async_rx.recv()).await {
                Ok(Some(record)) => match &record {
                    MiRecord::Exec { class, .. } if class == "stopped" => {
                        return Ok(stopped_info_from(&record));
                    }
                    MiRecord::Notify { class, results } => {
                        self.note_notify(class, results);
                    }
                    _ => self.backlog.push(record),
                },
                Ok(None) => {
                    return Err(EngineError::Gdb("gdb exited before *stopped".into()))
                }
                Err(_) => {
                    return Err(EngineError::Gdb(format!(
                        "timed out after {:.1}s waiting for *stopped",
                        timeout.as_secs_f32()
                    )))
                }
            }
        }
    }

    /// Buffer notifications for later inspection; extracts the inferior pid
    /// from `=thread-group-started`.
    fn note_notify(&mut self, class: &str, results: &[crate::mi_parser::MiResult]) {
        if class == "thread-group-started" {
            for r in results {
                if r.variable == "pid" {
                    if let Some(v) = as_const(&r.value) {
                        self.inferior_pid = v.parse().ok();
                    }
                }
            }
        }
        self.backlog.push(MiRecord::Notify {
            class: class.to_string(),
            results: results.to_vec(),
        });
    }

    /// Insert a breakpoint; returns the breakpoint number.
    pub async fn insert_breakpoint(&mut self, location: &str) -> Result<u32> {
        let resp = self
            .execute(&format!("-break-insert {}", quote_for_mi(location)))
            .await?;
        if let Some(msg) = resp.error_message() {
            return Err(EngineError::GdbCommandError {
                command: format!("-break-insert {location}"),
                message: msg,
            });
        }
        let bkpt = resp.record.result("bkpt");
        if let Some(v) = bkpt {
            if let Some(t) = as_tuple(v) {
                for r in t {
                    if r.variable == "number" {
                        if let Some(n) = as_const(&r.value) {
                            return Ok(n.parse().unwrap_or(0));
                        }
                    }
                }
            }
        }
        Ok(0)
    }

    /// `-exec-run` + wait for the first stop.
    pub async fn run(&mut self, timeout: Duration) -> Result<StoppedInfo> {
        let resp = self.execute("-exec-run").await?;
        if let Some(msg) = resp.error_message() {
            return Err(EngineError::GdbCommandError {
                command: "-exec-run".into(),
                message: msg,
            });
        }
        self.wait_stopped(timeout).await
    }

    /// Step into (`-exec-step`) and wait for the stop.
    pub async fn exec_step(&mut self, timeout: Duration) -> Result<StoppedInfo> {
        let resp = self.execute("-exec-step").await?;
        if let Some(msg) = resp.error_message() {
            return Err(EngineError::GdbCommandError {
                command: "-exec-step".into(),
                message: msg,
            });
        }
        self.wait_stopped(timeout).await
    }

    /// Run until the current frame returns (`-exec-finish`).
    pub async fn exec_finish(&mut self, timeout: Duration) -> Result<StoppedInfo> {
        let resp = self.execute("-exec-finish").await?;
        if let Some(msg) = resp.error_message() {
            return Err(EngineError::GdbCommandError {
                command: "-exec-finish".into(),
                message: msg,
            });
        }
        self.wait_stopped(timeout).await
    }

    /// Continue (`-exec-continue`) and wait for the stop.
    pub async fn exec_continue(&mut self, timeout: Duration) -> Result<StoppedInfo> {
        let resp = self.execute("-exec-continue").await?;
        if let Some(msg) = resp.error_message() {
            return Err(EngineError::GdbCommandError {
                command: "-exec-continue".into(),
                message: msg,
            });
        }
        self.wait_stopped(timeout).await
    }

    /// Evaluate an expression via `-data-evaluate-expression`. The expression
    /// is always passed as one quoted MI argument.
    pub async fn evaluate(&mut self, expression: &str) -> Result<String> {
        let resp = self
            .execute(&format!(
                "-data-evaluate-expression {}",
                quote_for_mi(expression)
            ))
            .await?;
        if let Some(msg) = resp.error_message() {
            return Err(EngineError::GdbCommandError {
                command: format!("-data-evaluate-expression {expression}"),
                message: msg,
            });
        }
        Ok(resp.value().unwrap_or_default().to_string())
    }

    /// Read raw memory via `-data-read-memory-bytes`; returns hex-encoded
    /// bytes. Returns Err with GDB's message when unreadable.
    pub async fn read_memory_bytes(&mut self, address: &str, size: u64) -> std::result::Result<String, String> {
        if size == 0 {
            return Ok(String::new());
        }
        let resp = self
            .execute(&format!(
                "-data-read-memory-bytes {} {}",
                quote_for_mi(address),
                size
            ))
            .await
            .map_err(|e| e.to_string())?;
        if let Some(msg) = resp.error_message() {
            return Err(msg);
        }
        if let Some(MiValue::List(items)) = resp.record.result("memory") {
            if let Some(MiListItem::Value(MiValue::Tuple(entry))) = items.first() {
                for r in entry {
                    if r.variable == "contents" {
                        if let Some(s) = as_const(&r.value) {
                            return Ok(s.to_string());
                        }
                    }
                }
            }
        }
        Err("memory response missing contents".into())
    }

    /// Current inferior output captured so far (pty), head-truncated to the
    /// configured cap.
    pub fn program_output(&self) -> (String, u64, bool) {
        let guard = self.shared.program.lock().unwrap();
        (guard.text(), guard.total_bytes, guard.truncated)
    }

    /// GDB's own console/log chatter (diagnostics for humans).
    pub fn console_log(&self) -> String {
        self.shared.console.lock().unwrap().clone()
    }

    /// Backlog of async records (notifications etc.) collected so far.
    pub fn take_backlog(&mut self) -> Vec<MiRecord> {
        std::mem::take(&mut self.backlog)
    }

    /// Graceful exit; falls back to killing the process group.
    pub async fn shutdown(mut self) {
        let _ = self.execute("-gdb-exit").await;
        let wait = tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await;
        if matches!(wait, Ok(Err(_)) | Err(_)) {
            if let Some(pid) = self.child.id() {
                kill_process_group(pid);
                let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
            }
        }
        self.reader_task.abort();
        if let Some(t) = self.pty_task.take() {
            t.abort();
        }
    }

    /// Hard kill of the whole process group (timeout / resource limit path).
    /// Also kills the inferior's own process group: GDB puts the inferior in
    /// a separate group (job control), so killing GDB's group alone would
    /// leave the inferior running as an orphan.
    pub async fn kill(&mut self) {
        if let Some(pid) = self.child.id() {
            kill_process_group(pid);
        }
        if let Some(inferior) = self.inferior_pid {
            if self.child.id() != Some(inferior) {
                kill_process_group(inferior);
            }
        }
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        self.reader_task.abort();
        if let Some(t) = self.pty_task.take() {
            t.abort();
        }
    }
}

impl Drop for GdbSession {
    fn drop(&mut self) {
        // Best effort: reader tasks are tokio tasks; abort them and ensure
        // neither GDB nor the inferior can outlive the session.
        self.reader_task.abort();
        if let Some(t) = self.pty_task.take() {
            t.abort();
        }
        if let Some(pid) = self.child.id() {
            kill_process_group(pid);
        }
        if let Some(inferior) = self.inferior_pid {
            if self.child.id() != Some(inferior) {
                kill_process_group(inferior);
            }
        }
    }
}

fn stopped_info_from(record: &MiRecord) -> StoppedInfo {
    let mut info = StoppedInfo {
        reason: None,
        signal_name: None,
        signal_meaning: None,
        frame: None,
        thread_id: None,
        exit_code: None,
        bkptno: None,
        raw: Some(record.clone()),
    };
    for r in record.results() {
        match r.variable.as_str() {
            "reason" => info.reason = as_const(&r.value).map(|s| s.to_string()),
            "signal-name" => info.signal_name = as_const(&r.value).map(|s| s.to_string()),
            "signal-meaning" => info.signal_meaning = as_const(&r.value).map(|s| s.to_string()),
            "thread-id" => info.thread_id = as_const(&r.value).map(|s| s.to_string()),
            "exit-code" => info.exit_code = as_const(&r.value).map(|s| s.to_string()),
            "bkptno" => info.bkptno = as_const(&r.value).map(|s| s.to_string()),
            "frame" => info.frame = Some(frame_from_tuple(as_tuple(&r.value).unwrap_or(&[]))),
            _ => {}
        }
    }
    info
}

/// Build a `StackFrameInfo` from a `frame={...}` result tuple.
pub fn frame_from_tuple(fields: &[crate::mi_parser::MiResult]) -> StackFrameInfo {
    let mut f = StackFrameInfo::default();
    for r in fields {
        let v = as_const(&r.value).map(|s| s.to_string());
        match r.variable.as_str() {
            "level" => f.level = v.and_then(|s| s.parse().ok()).unwrap_or(0),
            "func" => f.func = v.unwrap_or_default(),
            "file" => f.file = v.filter(|s| !s.is_empty() && s != "??"),
            "fullname" => f.full_file = v.filter(|s| !s.is_empty() && s != "??"),
            "line" => f.line = v.and_then(|s| s.parse().ok()),
            "addr" => f.address = v,
            _ => {}
        }
    }
    f
}

/// Parse `-stack-list-frames` output into frames.
pub fn parse_frames(record: &MiRecord) -> Vec<StackFrameInfo> {
    let mut frames = Vec::new();
    if let Some(MiValue::List(items)) = record.result("stack") {
        for item in items {
            match item {
                MiListItem::Result(res) => {
                    if res.variable == "frame" {
                        frames.push(frame_from_tuple(as_tuple(&res.value).unwrap_or(&[])));
                    }
                }
                MiListItem::Value(MiValue::Tuple(t)) => {
                    frames.push(frame_from_tuple(t));
                }
                _ => {}
            }
        }
    }
    frames.truncate(MAX_FRAMES_PER_SNAPSHOT);
    frames
}

/// One entry of `-stack-list-variables --all-values`.
#[derive(Debug, Clone)]
pub struct RawVariable {
    pub name: String,
    pub value: String,
    pub is_argument: bool,
}

/// Parse `-stack-list-variables --all-values` output.
pub fn parse_variables(record: &MiRecord) -> Vec<RawVariable> {
    let mut out = Vec::new();
    if let Some(MiValue::List(items)) = record.result("variables") {
        for item in items {
            if let MiListItem::Value(MiValue::Tuple(t)) = item {
                let mut name = String::new();
                let mut value = String::new();
                let mut is_arg = false;
                for r in t {
                    match r.variable.as_str() {
                        "name" => name = as_const(&r.value).unwrap_or_default().to_string(),
                        "value" => value = as_const(&r.value).unwrap_or_default().to_string(),
                        "arg" => is_arg = as_const(&r.value) == Some("1"),
                        _ => {}
                    }
                }
                if !name.is_empty() {
                    out.push(RawVariable {
                        name,
                        value,
                        is_argument: is_arg,
                    });
                }
            }
        }
    }
    out
}

/// Extract the last `0x...` hex token from a GDB-printed value
/// (handles both `0x5555...` and `(int *) 0x7ffd...`).
pub fn extract_hex_address(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'0' && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
            let start = i;
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
                j += 1;
            }
            if j > i + 2 {
                best = Some((start, j));
            }
            i = j;
        } else {
            i += 1;
        }
    }
    best.map(|(s, e)| value[s..e].to_ascii_lowercase())
}

// ---------------------------------------------------------------------------
// Snapshot capture (GDB-side state; heap composition happens in trace.rs)
// ---------------------------------------------------------------------------

/// What counts as "user code" when deciding where to snapshot and how to
/// step: canonical source paths of the configured project, or any file under
/// the project directories.
#[derive(Clone, Debug)]
pub struct UserCodeClassifier {
    /// Canonical user source file paths.
    pub files: HashSet<String>,
    /// Basenames of user sources (fallback when fullname is unavailable).
    pub basenames: HashSet<String>,
    /// Project directories (working dir + source parents + include dirs).
    pub dirs: Vec<PathBuf>,
}

impl UserCodeClassifier {
    pub fn from_build(build: &crate::config::BuildConfig) -> Self {
        let mut files = HashSet::new();
        let mut basenames = HashSet::new();
        let mut dirs = Vec::new();
        let mut push_source = |p: &Path| {
            if let (Ok(canon), true) = (std::fs::canonicalize(p), p.exists()) {
                files.insert(canon.display().to_string());
            } else {
                files.insert(p.display().to_string());
            }
            if let Some(b) = p.file_name().and_then(|b| b.to_str()) {
                basenames.insert(b.to_string());
            }
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    dirs.push(parent.to_path_buf());
                }
            }
        };
        for s in build.user_sources() {
            push_source(&s);
        }
        if let Some(wd) = &build.working_directory {
            if wd.is_dir() {
                dirs.push(wd.clone());
            }
        }
        for d in &build.include_dirs {
            dirs.push(d.clone());
        }
        Self {
            files,
            basenames,
            dirs,
        }
    }

    pub fn is_user_file(&self, frame: &StackFrameInfo) -> bool {
        let candidates: [Option<&str>; 2] = [
            frame.full_file.as_deref(),
            frame.file.as_deref(),
        ];
        for cand in candidates.iter().flatten() {
            let path = Path::new(cand);
            if let Ok(canon) = std::fs::canonicalize(path) {
                if self.files.contains(canon.display().to_string().as_str()) {
                    return true;
                }
            }
            if self.files.contains(*cand) {
                return true;
            }
            let lowered = cand.to_string();
            if self
                .dirs
                .iter()
                .any(|d| lowered.starts_with(&format!("{}/", d.display())))
            {
                return true;
            }
            if path.is_absolute() {
                if let Some(base) = path.file_name().and_then(|b| b.to_str()) {
                    if self.basenames.contains(base) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Options controlling per-stop data collection.
#[derive(Clone, Debug)]
pub struct CaptureOptions {
    pub classifier: UserCodeClassifier,
    pub max_variables: usize,
    pub max_memory_read: usize,
    /// Per-snapshot memory budget for raw byte reads (variables only; heap
    /// reads are budgeted by the orchestrator on top of this).
    pub memory_budget: usize,
}

/// GDB-side state captured at one stop.
#[derive(Debug, Default)]
pub struct CapturedState {
    pub frames: Vec<StackFrameInfo>,
    pub current: Option<StackFrameInfo>,
    pub is_user_code: bool,
    pub variables: Vec<VariableInfo>,
    /// Bytes spent on variable raw-byte reads in this capture.
    pub memory_used: usize,
}

/// Capture frames + variables (+ per-variable raw bytes) at a stop.
pub async fn capture_state(
    session: &mut GdbSession,
    opts: &CaptureOptions,
) -> Result<CapturedState> {
    let mut state = CapturedState::default();

    let frames_resp = session.execute("-stack-list-frames").await?;
    state.frames = parse_frames(&frames_resp.record);
    state.current = state.frames.first().cloned();
    state.is_user_code = state
        .current
        .as_ref()
        .map(|f| opts.classifier.is_user_file(f))
        .unwrap_or(false);

    if !state.is_user_code {
        return Ok(state);
    }

    let vars_resp = session.execute("-stack-list-variables --all-values").await?;
    let raw_vars = parse_variables(&vars_resp.record);
    let budget = &mut state.memory_used;

    for raw in raw_vars.iter().take(opts.max_variables) {
        let mut var = VariableInfo {
            name: raw.name.clone(),
            value: raw.value.clone(),
            is_argument: Some(raw.is_argument),
            ..Default::default()
        };

        // Type via console `whatis` (one command, no varobj bookkeeping).
        match session
            .execute(&format!(
                "-interpreter-exec console \"whatis {}\"",
                raw.name
            ))
            .await
        {
            Ok(resp) => {
                var.c_type = extract_type_from_streams(&resp.streams);
            }
            Err(e) => var.read_error = Some(e.to_string()),
        }

        // Address of the variable itself.
        if let Ok(addr_val) = session.evaluate(&format!("&{}", raw.name)).await {
            var.address = extract_hex_address(&addr_val);
        }

        // Size (runtime-evaluated, works for VLAs too).
        if let Ok(size_val) = session.evaluate(&format!("sizeof({})", raw.name)).await {
            var.size_bytes = size_val.trim().parse::<u64>().ok();
        }

        // For pointer variables: the address stored INSIDE the pointer.
        // The pointer's own address is `var.address`; never conflated.
        if var.c_type.contains('*') {
            if let Ok(target) = session
                .evaluate(&format!("(unsigned long)({})", raw.name))
                .await
            {
                if let Ok(n) = target.trim().parse::<u64>() {
                    var.points_to = Some(format!("0x{n:x}"));
                }
            }
        }

        // Raw bytes of the variable itself, within budget.
        let mut err = var.read_error.take();
        if let (Some(addr), Some(size)) = (&var.address, var.size_bytes) {
            let read_size = size.min(opts.max_memory_read as u64);
            if *budget + read_size as usize <= opts.memory_budget {
                match session.read_memory_bytes(addr, read_size).await {
                    Ok(hex) => {
                        *budget += read_size as usize;
                        var.raw_bytes_hex = Some(hex);
                    }
                    Err(e) => err = Some(format!("memory read failed: {e}")),
                }
            } else {
                err = Some("snapshot memory budget exhausted".into());
            }
        }
        var.read_error = err;
        state.variables.push(var);
    }

    Ok(state)
}

/// Pull `type = ...` out of console-command stream output (`whatis`).
fn extract_type_from_streams(streams: &[StreamRecord]) -> String {
    for (kind, text) in streams.iter().rev() {
        if matches!(kind, StreamKind::Console | StreamKind::Log) {
            for line in text.lines() {
                if let Some(rest) = line.trim().strip_prefix("type = ") {
                    return rest.trim().to_string();
                }
            }
        }
    }
    String::new()
}

/// Quote a value as a single MI c-string argument.
fn quote_for_mi(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Background tasks
// ---------------------------------------------------------------------------

async fn reader_task(stdout: ChildStdout, shared: Arc<Shared>) {
    let mut reader = BufReader::new(stdout);
    let mut line_buf = Vec::new();
    loop {
        line_buf.clear();
        match reader.read_until(b'\n', &mut line_buf).await {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&line_buf);
                handle_line(&line, &shared);
            }
            Err(_) => break,
        }
    }
    // EOF: fail every pending command.
    shared.eof.store(true, Ordering::SeqCst);
    let mut pending = shared.pending.lock().unwrap();
    for (_, tx) in pending.drain() {
        let _ = tx.send(Err("gdb exited".into()));
    }
}

fn handle_line(line: &str, shared: &Shared) {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() || trimmed.starts_with("(gdb)") {
        return;
    }
    if shared.debug_echo {
        let mut console = shared.console.lock().unwrap();
        if console.len() < MAX_CONSOLE_LOG_BYTES {
            console.push_str("[raw] ");
            console.push_str(trimmed);
            console.push('\n');
        }
    }
    match parse_line(trimmed) {
        Ok(Some(record)) => route_record(record, shared),
        Ok(None) => {}
        Err(e) => {
            let mut console = shared.console.lock().unwrap();
            if console.len() < MAX_CONSOLE_LOG_BYTES {
                console.push_str("[unparsed] ");
                console.push_str(trimmed);
                console.push_str(" (");
                console.push_str(&e.to_string());
                console.push_str(")\n");
            }
        }
    }
}

fn route_record(record: MiRecord, shared: &Shared) {
    match &record {
        MiRecord::Stream { kind, text } => {
            match kind {
                StreamKind::Console | StreamKind::Log => {
                    let mut console = shared.console.lock().unwrap();
                    if console.len() < MAX_CONSOLE_LOG_BYTES {
                        console.push_str(text);
                    }
                }
                StreamKind::Target => {
                    // Target stream (`@`) only appears when the inferior is
                    // NOT attached to a pty; treat as program output.
                    let mut program = shared.program.lock().unwrap();
                    program.push(text.as_bytes(), shared.output_cap);
                }
            }
            // Collect for the in-flight command (console commands such as
            // `whatis` deliver their result through stream records).
            shared.current_streams.lock().unwrap().push((*kind, text.clone()));
        }
        MiRecord::Result { token, .. } | MiRecord::Exec { token, .. } => {
            if let Some(tok) = token {
                let sender = shared.pending.lock().unwrap().remove(tok);
                if let Some(tx) = sender {
                    let streams = {
                        let mut guard = shared.current_streams.lock().unwrap();
                        std::mem::take(&mut *guard)
                    };
                    let _ = tx.send(Ok((record, streams)));
                    return;
                }
            }
            // Async record (*running / *stopped / unexpected result).
            let _ = shared.async_tx.send(record);
        }
        MiRecord::Notify { .. } => {
            let _ = shared.async_tx.send(record);
        }
    }
}

async fn pty_reader_task(master: OwnedFd, shared: Arc<Shared>) {
    use tokio::io::unix::AsyncFd;
    let fd = match unsafe { AsyncFd::register(master) } {
        Ok(fd) => fd,
        Err(_) => return,
    };
    loop {
        let mut guard = match fd.readable().await {
            Ok(g) => g,
            Err(_) => break,
        };
        let mut buf = [0u8; 4096];
        let result = guard.try_io(|inner| {
            let fd = inner.get_ref().as_raw_fd();
            let n = unsafe {
                libc::read(
                    fd,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if n < 0 {
                // EAGAIN => try again later; EIO => slave closed.
                return Err(std::io::Error::last_os_error());
            }
            std::io::Result::Ok(n as usize)
        });
        match result {
            Ok(Ok(0)) => break,          // closed
            Ok(Ok(n)) => {
                let mut program = shared.program.lock().unwrap();
                program.push(&buf[..n], shared.output_cap);
            }
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            // EIO on Linux: all slave sides closed (inferior exited).
            Ok(Err(_)) => break,
            Err(_) => continue,
        }
    }
}

// keep Token import referenced for documentation-level typing
#[allow(dead_code)]
fn _token_type_witness(_: std::option::Option<Token>) {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mi_parser::parse_line;

    fn parse(s: &str) -> MiRecord {
        parse_line(s).unwrap().unwrap()
    }

    #[test]
    fn frames_parse_from_real_output() {
        let rec = parse("10^done,stack=[frame={level=\"0\",addr=\"0x00000000004004af\",func=\"main\",file=\"sample.c\",fullname=\"/tmp/sample.c\",line=\"12\",arch=\"i386:x86-64\"},frame={level=\"1\",addr=\"0x00007ffff7c29d90\",func=\"__libc_start_call_main\",from=\"/lib64/libc.so.6\"}]");
        let frames = parse_frames(&rec);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].func, "main");
        assert_eq!(frames[0].line, Some(12));
        assert_eq!(frames[0].full_file.as_deref(), Some("/tmp/sample.c"));
        assert_eq!(frames[1].func, "__libc_start_call_main");
        assert!(frames[1].file.is_none(), "from= is not file=");
    }

    #[test]
    fn variables_parse_from_real_output() {
        let rec = parse("11^done,variables=[{name=\"argc\",arg=\"1\",value=\"1\"},{name=\"argv\",arg=\"1\",value=\"0x7fffffffdbe8\"},{name=\"arr\",value=\"0x0\"},{name=\"p\",value=\"{x = 0, y = 0}\"}]");
        let vars = parse_variables(&rec);
        assert_eq!(vars.len(), 4);
        assert_eq!(vars[0].name, "argc");
        assert!(vars[0].is_argument);
        assert!(!vars[2].is_argument);
        assert_eq!(vars[2].value, "0x0");
        assert_eq!(vars[3].value, "{x = 0, y = 0}");
    }

    #[test]
    fn stopped_info_from_real_record() {
        let rec = parse("*stopped,reason=\"breakpoint-hit\",disp=\"keep\",bkptno=\"1\",frame={addr=\"0x00000000004004af\",func=\"main\",args=[{name=\"argc\",value=\"1\"}],file=\"sample.c\",fullname=\"/tmp/sample.c\",line=\"12\",arch=\"i386:x86-64\"},thread-id=\"1\",stopped-threads=\"all\",core=\"0\"");
        let info = stopped_info_from(&rec);
        assert_eq!(info.reason.as_deref(), Some("breakpoint-hit"));
        assert_eq!(info.bkptno.as_deref(), Some("1"));
        assert_eq!(info.frame.as_ref().unwrap().func, "main");
        assert!(!info.is_exit());
        assert!(!info.is_signal());
    }

    #[test]
    fn stopped_exit_and_signal() {
        let exit = stopped_info_from(&parse("*stopped,reason=\"exited-normally\""));
        assert!(exit.is_exit());
        assert!(exit.frame.is_none());

        let signal = stopped_info_from(&parse("*stopped,reason=\"signal-received\",signal-name=\"SIGSEGV\",signal-meaning=\"Segmentation fault\",frame={func=\"main\",file=\"crash.c\",fullname=\"/tmp/crash.c\",line=\"5\"},thread-id=\"1\",stopped-threads=\"all\""));
        assert!(signal.is_signal());
        assert_eq!(signal.signal_name.as_deref(), Some("SIGSEGV"));
        assert_eq!(signal.frame.as_ref().unwrap().line, Some(5));
    }

    #[test]
    fn hex_extraction() {
        assert_eq!(extract_hex_address("0x7fffffffdaa8").as_deref(), Some("0x7fffffffdaa8"));
        assert_eq!(extract_hex_address("(int *) 0x7ffd1234").as_deref(), Some("0x7ffd1234"));
        assert_eq!(extract_hex_address("no address here"), None);
        assert_eq!(extract_hex_address("0x0").as_deref(), Some("0x0"));
        // last wins (type prefix then address)
        assert_eq!(
            extract_hex_address("(int *) 0x1 0x2").as_deref(),
            Some("0x2")
        );
    }

    #[test]
    fn whatis_type_extraction() {
        let streams = vec![
            (StreamKind::Console, "type = int *\n".to_string()),
        ];
        assert_eq!(extract_type_from_streams(&streams), "int *");
        let streams = vec![(StreamKind::Log, "type = struct Point\n".to_string())];
        assert_eq!(extract_type_from_streams(&streams), "struct Point");
        assert_eq!(extract_type_from_streams(&[]), "");
    }

    #[test]
    fn mi_quoting() {
        assert_eq!(quote_for_mi("plain"), "\"plain\"");
        assert_eq!(quote_for_mi("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_for_mi("back\\slash"), "\"back\\\\slash\"");
    }

    #[test]
    fn user_code_classification() {
        let dir = std::env::temp_dir().join(format!("cte-cls-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("sample.c");
        std::fs::write(&src, "int main(void){return 0;}\n").unwrap();
        let build = crate::config::BuildConfig {
            source_file: src.clone(),
            ..Default::default()
        };
        let cls = UserCodeClassifier::from_build(&build);
        let user_frame = StackFrameInfo {
            level: 0,
            func: "main".into(),
            file: Some("sample.c".into()),
            line: Some(1),
            full_file: Some(src.display().to_string()),
            address: None,
        };
        assert!(cls.is_user_file(&user_frame));
        let libc_frame = StackFrameInfo {
            level: 1,
            func: "__libc_start_main".into(),
            file: None,
            line: None,
            full_file: None,
            address: Some("0x7f".into()),
        };
        assert!(!cls.is_user_file(&libc_frame));
        // file under project dir counts as user code
        let header_frame = StackFrameInfo {
            level: 0,
            func: "helper".into(),
            file: Some("helper.h".into()),
            line: Some(2),
            full_file: Some(dir.join("helper.h").display().to_string()),
            address: None,
        };
        assert!(cls.is_user_file(&header_frame));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn program_buffer_cap() {
        let mut buf = ProgramBuffer::default();
        buf.push(&[b'a'; 100], 50);
        assert_eq!(buf.data.len(), 50);
        assert!(buf.truncated);
        assert_eq!(buf.total_bytes, 100);
    }
}

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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
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
    output_limit: tokio::sync::Notify,
    inferior_pid: AtomicU32,
    multi_threaded: AtomicBool,
    threads_seen: AtomicU32,
    parse_time_us: AtomicU64,
    module_epoch: AtomicU64,
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
    command_categories: CommandCategories,
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
        matches!(
            self.reason.as_deref(),
            Some("exited-normally") | Some("exited")
        )
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
    /// V2.1 (F-05): explicit gdb executable path; when unset, `gdb` is
    /// resolved via PATH (execvp semantics: non-executable PATH matches are
    /// skipped, so a shadowing non-executable file cannot satisfy discovery).
    pub gdb_path: Option<String>,
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

/// Proven-stable global metadata cached per session (Optimization D).
/// Only declaration type, static-storage address and size qualify; runtime
/// values, pointer targets and memory contents are never cached here.
#[derive(Clone, Debug, Default)]
pub(crate) struct GlobalStaticMeta {
    pub c_type: Option<String>,
    pub address: Option<String>,
    pub size_bytes: Option<u64>,
}

/// Checkpoint-scoped raw-byte read reuse map (Optimization F). The inferior
/// is stopped for the entire capture, so re-serving an overlapping request
/// from a buffer read at this very stop is exact, not speculative. The map
/// lives for one checkpoint only; cross-checkpoint reuse is deliberately not
/// implemented (writes can occur through aliases, calls, or instructions the
/// trace does not retain). Heap reads are managed by the orchestrator and
/// never enter this map.
#[derive(Default)]
pub struct CheckpointReads {
    reads: HashMap<u64, (u64, String)>,
}

impl CheckpointReads {
    /// Bytes for `[addr, addr + size)` when a buffer read at this same stop
    /// covers the range; `None` otherwise (caller performs a live read).
    pub fn lookup(&self, addr: &str, size: u64) -> Option<String> {
        let start = crate::memory::address(addr)?;
        let end = start.checked_add(size)?;
        self.reads.iter().find_map(|(&base, (cached_size, hex))| {
            let cached_end = base.checked_add(*cached_size)?;
            if start < base || end > cached_end {
                return None;
            }
            let offset = usize::try_from(start - base).ok()?.checked_mul(2)?;
            let len = usize::try_from(size).ok()?.checked_mul(2)?;
            Some(hex.get(offset..offset.checked_add(len)?)?.to_string())
        })
    }

    /// Record only validated, actually observed bytes. Keep the longest read
    /// at a given address; a partial read must not mask a later complete one.
    pub fn insert(&mut self, addr: &str, size: u64, hex: &str) {
        let Some(start) = crate::memory::address(addr) else {
            return;
        };
        if !hex.len().is_multiple_of(2) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return;
        }
        let actual = size.min((hex.len() / 2) as u64);
        if actual == 0 || start.checked_add(actual).is_none() {
            return;
        }
        let entry = self.reads.entry(start).or_default();
        if entry.0 < actual {
            *entry = (actual, hex[..actual as usize * 2].to_string());
        }
    }
}

/// A contiguous prefix of the requested memory, with a diagnostic when GDB
/// could not provide the entire range. Bytes after a hole are never rebased.
pub(crate) struct MemoryRead {
    pub hex: String,
    pub error: Option<String>,
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
    deadline: Instant,
    max_commands: u64,
    poisoned: bool,
    activations: HashMap<String, Vec<(String, u64)>>,
    selected_thread: Option<String>,
    next_activation: u64,
    pub(crate) type_cache: HashMap<String, String>,
    pub command_time_us: u64,
    pub memory_read_time_us: u64,
    pub symbol_scopes: HashMap<String, String>,
    scope_support: Option<bool>,
    /// The probe function has been defined in this GDB process; only its
    /// arguments need transmitting on subsequent calls. Never shared across
    /// sessions or executables.
    scope_probe_installed: bool,
    /// Proven-stable global metadata per `global:{epoch}:{name}` (Opt. D):
    /// declaration type, static-storage address, size. Only file-qualified
    /// names (`'file.c'::name`) are cached; function-local statics
    /// (`func::name`) are re-evaluated because their frame-independence is
    /// unproven. Never holds runtime values, pointer targets or bytes.
    pub(crate) global_meta_cache: HashMap<String, GlobalStaticMeta>,
    // V2.2 optimization counters (session-scoped; consumed by Profiling):
    /// Lexical-probe calls whose static data came from the per-chain memo.
    pub(crate) probe_cache_hits: u64,
    /// Per-variable address/size/pointer evaluations replaced by probe data.
    pub(crate) saved_variable_meta_queries: u64,
    /// Memory reads served from same-checkpoint byte reuse.
    pub(crate) saved_memory_reads: u64,
    /// Stops whose call stack was derived without a frame listing.
    pub(crate) derived_frame_stops: u64,
    /// Global address/size evaluations served from the session metadata cache.
    pub(crate) global_meta_cache_hits: u64,
}

/// GDB command counts by category, classified at issue time (no extra
/// debugger traffic). Categories are stable identifiers used in the
/// `gdb_command_categories` profiling map.
#[derive(Default)]
pub struct CommandCategories {
    stack_frames: AtomicU64,
    stack_variables: AtomicU64,
    lexical_probe: AtomicU64,
    caller_probe: AtomicU64,
    type_queries: AtomicU64,
    data_evaluate: AtomicU64,
    memory_reads: AtomicU64,
    breakpoints: AtomicU64,
    symbol_info: AtomicU64,
    exec_steps: AtomicU64,
    other: AtomicU64,
}

impl CommandCategories {
    fn record(&self, command: &str) {
        let category = if command.starts_with("-stack-list-frames") {
            &self.stack_frames
        } else if command.starts_with("-stack-list-variables") {
            &self.stack_variables
        } else if command.starts_with("-interpreter-exec console \"python")
            && command.contains("C_TRACE_P2([], [], ")
        {
            &self.caller_probe
        } else if command.starts_with("-interpreter-exec console \"python") {
            &self.lexical_probe
        } else if command.contains("\"whatis ") {
            &self.type_queries
        } else if command.starts_with("-data-evaluate-expression") {
            &self.data_evaluate
        } else if command.starts_with("-data-read-memory") {
            &self.memory_reads
        } else if command.starts_with("-break-") {
            &self.breakpoints
        } else if command.starts_with("-symbol-info-") {
            &self.symbol_info
        } else if command.starts_with("-exec-") {
            &self.exec_steps
        } else {
            &self.other
        };
        category.fetch_add(1, Ordering::Relaxed);
    }

    /// Category → count map for the profiling record.
    pub fn to_map(&self) -> std::collections::BTreeMap<String, u64> {
        let pairs = [
            ("stack_frames", &self.stack_frames),
            ("stack_variables", &self.stack_variables),
            ("lexical_probe", &self.lexical_probe),
            ("caller_probe", &self.caller_probe),
            ("type_queries", &self.type_queries),
            ("data_evaluate", &self.data_evaluate),
            ("memory_reads", &self.memory_reads),
            ("breakpoints", &self.breakpoints),
            ("symbol_info", &self.symbol_info),
            ("exec_steps", &self.exec_steps),
            ("other", &self.other),
        ];
        pairs
            .into_iter()
            .map(|(name, counter)| (name.to_string(), counter.load(Ordering::Relaxed)))
            .collect()
    }
}

impl GdbSession {
    /// Launch `gdb --nx --quiet --interpreter=mi2 <binary>` and apply session
    /// configuration (pagination off, debuginfod off, inferior tty, working
    /// directory, program arguments).
    pub async fn launch(opts: &GdbLaunchOptions) -> Result<GdbSession> {
        if crate::supervisor::cancelled() {
            return Err(EngineError::Cancelled);
        }
        if crate::supervisor::remaining().is_zero() {
            return Err(crate::supervisor::timeout_error("GDB launch"));
        }
        let pty = create_pty()?;
        crate::supervisor::enable_subreaper();
        let mut cmd = tokio::process::Command::new(opts.gdb_path.as_deref().unwrap_or("gdb"));
        cmd.args(["--nx", "--quiet", "--interpreter=mi2"])
            .env("DEBUGINFOD_URLS", "")
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
        cmd.kill_on_drop(true);
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
            output_limit: tokio::sync::Notify::new(),
            inferior_pid: AtomicU32::new(0),
            multi_threaded: AtomicBool::new(false),
            threads_seen: AtomicU32::new(0),
            parse_time_us: AtomicU64::new(0),
            module_epoch: AtomicU64::new(0),
            pending: Mutex::new(HashMap::new()),
            current_streams: Mutex::new(Vec::new()),
            async_tx,
            console: Mutex::new(String::new()),
            program: Mutex::new(ProgramBuffer::default()),
            eof: AtomicBool::new(false),
            output_cap: opts.limits.max_output_bytes,
            debug_echo: std::env::var_os("C_TRACE_DEBUG").is_some(),
            command_categories: CommandCategories::default(),
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
            deadline: crate::supervisor::stage_deadline(opts.limits.debugger_timeout_seconds),
            max_commands: opts.limits.max_gdb_commands,
            poisoned: false,
            activations: HashMap::new(),
            selected_thread: None,
            next_activation: 1,
            type_cache: HashMap::new(),
            command_time_us: 0,
            memory_read_time_us: 0,
            symbol_scopes: HashMap::new(),
            global_meta_cache: HashMap::new(),
            scope_support: None,
            scope_probe_installed: false,
            probe_cache_hits: 0,
            saved_variable_meta_queries: 0,
            saved_memory_reads: 0,
            derived_frame_stops: 0,
            global_meta_cache_hits: 0,
        };

        // --- session configuration ---
        let configuration = async {
            session.checked("-gdb-set pagination off").await?;
            session.checked("-gdb-set confirm off").await?;
            session.checked("-gdb-set debuginfod enabled off").await?;
            session.checked("-gdb-set mi-async on").await?;
            session.checked("-gdb-set non-stop off").await?;
            session.checked("-gdb-set style enabled off").await?;
            session.checked("-gdb-set may-call-functions off").await?;
            session.checked("-gdb-set auto-load off").await?;
            session
                .checked(&format!("-inferior-tty-set {}", slave_path))
                .await?;
            // GDB already inherited the requested cwd via Command::current_dir.
            // Applying the same relative directory a second time changes its meaning.
            if !opts.program_args.is_empty() {
                let args: Vec<String> = opts.program_args.iter().map(|a| quote_for_mi(a)).collect();
                session
                    .checked(&format!("-exec-arguments {}", args.join(" ")))
                    .await?;
            }
            Ok::<(), EngineError>(())
        }
        .await;
        if let Err(error) = configuration {
            session.kill().await;
            return Err(error);
        }
        Ok(session)
    }

    async fn checked(&mut self, command: &str) -> Result<MiCommandResponse> {
        let response = self.execute(command).await?;
        if let Some(message) = response.error_message() {
            return Err(EngineError::GdbCommandError {
                command: command.into(),
                message,
            });
        }
        Ok(response)
    }

    pub fn observed_inferior_pid(&self) -> Option<u32> {
        self.inferior_pid.or_else(|| {
            let pid = self.shared.inferior_pid.load(Ordering::SeqCst);
            (pid != 0).then_some(pid)
        })
    }

    /// Send one MI command and await its token-matched result.
    pub async fn execute(&mut self, command: &str) -> Result<MiCommandResponse> {
        if crate::supervisor::cancelled() {
            return Err(EngineError::Cancelled);
        }
        let command_started = Instant::now();
        if self.poisoned {
            return Err(EngineError::Gdb(
                "session desynchronized after an earlier command timeout".into(),
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(crate::supervisor::timeout_error("GDB command"));
        }
        if self.commands_issued.load(Ordering::SeqCst) >= self.max_commands {
            return Err(EngineError::LimitReached("max_gdb_commands"));
        }
        let command = command.trim();
        if self.shared.eof.load(Ordering::SeqCst) {
            return Err(EngineError::Gdb("gdb session already closed".into()));
        }
        let token = self.next_token;
        self.next_token += 1;
        self.commands_issued.fetch_add(1, Ordering::SeqCst);
        self.shared.command_categories.record(command);

        // Streams accumulated by earlier commands (or by async chatter) must
        // not leak into this command's response.
        self.shared.current_streams.lock().unwrap().clear();

        let (tx, rx) = oneshot::channel();
        self.shared.pending.lock().unwrap().insert(token, tx);

        let line = format!("{}{}\n", token, command);
        let write = tokio::select! {
            result = tokio::time::timeout_at(self.deadline.into(), self.stdin.write_all(line.as_bytes())) => result,
            _ = crate::supervisor::cancellation() => return Err(EngineError::Cancelled),
        };
        if let Err(e) = write.unwrap_or_else(|_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "GDB stdin deadline",
            ))
        }) {
            self.shared.eof.store(true, Ordering::SeqCst);
            return Err(EngineError::Gdb(format!("writing to gdb failed: {e}")));
        }
        if let Err(e) = self.stdin.flush().await {
            return Err(EngineError::Gdb(format!("flushing to gdb failed: {e}")));
        }

        let end = self
            .deadline
            .min(Instant::now() + Duration::from_secs(COMMAND_TIMEOUT_SECS));
        let received = tokio::select! {
            result = tokio::time::timeout_at(end.into(), rx) => result,
            _ = crate::supervisor::cancellation() => {
                self.shared.pending.lock().unwrap().remove(&token);
                return Err(EngineError::Cancelled);
            }
        };
        let outcome = match received {
            Ok(Ok(Ok((record, streams)))) => Ok(MiCommandResponse { record, streams }),
            Ok(Ok(Err(e))) => Err(EngineError::Gdb(e)),
            Ok(Err(_)) => {
                self.shared.pending.lock().unwrap().remove(&token);
                Err(EngineError::Gdb("gdb dropped the response channel".into()))
            }
            Err(_) => {
                // A late stream response cannot safely be associated with a later command.
                self.poisoned = true;
                self.shared.pending.lock().unwrap().remove(&token);
                Err(EngineError::GdbCommandTimeout {
                    command: command.to_string(),
                    timeout_secs: COMMAND_TIMEOUT_SECS,
                })
            }
        };
        let elapsed = command_started.elapsed();
        self.command_time_us += elapsed.as_micros() as u64;
        if std::env::var_os("C_TRACE_DEBUG").is_some() {
            eprintln!(
                "[mi] {:>6.1}ms {:>2} {}",
                elapsed.as_secs_f64() * 1000.0,
                self.commands_issued.load(Ordering::SeqCst),
                command
            );
        }
        outcome
    }

    /// Await the next `*stopped` record, buffering unrelated async records.
    pub async fn wait_stopped(&mut self, timeout: Duration) -> Result<StoppedInfo> {
        let deadline = self.deadline.min(Instant::now() + timeout);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(EngineError::GdbCommandTimeout {
                    command: "waiting for *stopped".into(),
                    timeout_secs: timeout.as_secs().max(1),
                });
            }
            let receive = async {
                tokio::select! {
                    biased;
                    record = self.async_rx.recv() => Ok(record),
                    _ = crate::supervisor::cancellation() => Err(EngineError::Cancelled),
                    _ = self.shared.output_limit.notified() => Err(EngineError::LimitReached("max_output_bytes")),
                }
            };
            match tokio::time::timeout(remaining, receive).await {
                Ok(Ok(Some(record))) => match &record {
                    MiRecord::Exec { class, .. } if class == "stopped" => {
                        let stop = stopped_info_from(&record);
                        self.selected_thread = stop.thread_id.clone();
                        return Ok(stop);
                    }
                    MiRecord::Notify { class, results } => {
                        if class == "gdb-eof" {
                            return Err(EngineError::Gdb("gdb terminated before stop".into()));
                        }
                        self.note_notify(class, results);
                    }
                    _ => {
                        if self.backlog.len() < 4096 {
                            self.backlog.push(record);
                        }
                    }
                },
                Ok(Ok(None)) => return Err(EngineError::Gdb("gdb exited before *stopped".into())),
                Ok(Err(error)) => return Err(error),
                Err(_) => {
                    return Err(EngineError::GdbCommandTimeout {
                        command: "waiting for *stopped".into(),
                        timeout_secs: timeout.as_secs().max(1),
                    })
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

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Uses GDB's DWARF objects without executing any code in the inferior.
    ///
    /// V2.2 (Optimization A):
    /// - Every discovered symbol carries fresh address/size/pointer-target
    ///   properties (per-stop, never cached), so the normal capture path can
    ///   skip its per-property MI evaluations.
    ///
    /// Static declaration memoization was tested but increased capture time;
    /// block traversal remains fresh here (see Phase 2 performance report).
    async fn lexical_symbols(
        &mut self,
        names: &[&str],
        ambiguous: &[&str],
    ) -> (HashMap<String, Vec<LexicalSymbol>>, bool) {
        if self.scope_support == Some(false) {
            return (HashMap::new(), false);
        }
        match self
            .inspect_lexical_symbols(names, ambiguous, None, usize::MAX)
            .await
        {
            Ok(probe) => (probe.symbols, probe.cached),
            Err(_) => {
                self.scope_support = Some(false);
                (HashMap::new(), false)
            }
        }
    }

    /// Inspect an explicit frame without selecting it. Symbol.value(frame)
    /// resolves shadowed declarations against the right recursive activation.
    async fn inspect_lexical_symbols(
        &mut self,
        names: &[&str],
        ambiguous: &[&str],
        level: Option<u32>,
        limit: usize,
    ) -> std::result::Result<ProbeOutput, String> {
        let python = r#"import gdb, json
frame = gdb.selected_frame() if level < 0 else gdb.newest_frame()
for i in range(max(level, 0)):
    frame = frame.older()
if frame is None:
    raise RuntimeError("caller frame unavailable")
out = {}
omitted = 0
captured = 0
b = frame.block()
while b is not None and not b.is_global and not b.is_static:
    for s in b:
        if (level >= 0 or s.name in allowed) and (s.is_variable or s.is_argument):
            if captured >= limit:
                omitted += 1
                continue
            captured += 1
            d = {"c_type": str(s.type), "scope_id": "block:%x-%x:%d" % (b.start, b.end, s.line), "file": s.symtab.fullname() if s.symtab else None, "line": s.line, "storage": "static" if s.addr_class == gdb.SYMBOL_LOC_STATIC else "automatic", "is_argument": s.is_argument}
            out.setdefault(s.name, []).append(d)
            if level >= 0 or s.name in ambiguous:
                try:
                    value = s.value(frame)
                    d["value"] = str(value)
                    d["address"] = "0x%x" % int(value.address) if value.address else None
                    d["size"] = value.type.sizeof
                    d["points_to"] = "0x%x" % int(value) if value.type.strip_typedefs().code == gdb.TYPE_CODE_PTR else None
                except Exception as e:
                    d["read_error"] = str(e)
            else:
                try:
                    value = s.value(frame)
                except Exception:
                    pass
                else:
                    try:
                        d["address"] = "0x%x" % int(value.address) if value.address else None
                    except Exception:
                        pass
                    try:
                        d["size"] = value.type.sizeof
                    except Exception:
                        pass
                    try:
                        if value.type.strip_typedefs().code == gdb.TYPE_CODE_PTR:
                            d["points_to"] = "0x%x" % int(value)
                    except Exception:
                        pass
    b = b.superblock
print("C_TRACE_SCOPE:" + json.dumps({"cached": False, "symbols": out, "omitted": omitted}))"#;
        let invocation = format!(
            "C_TRACE_P2({}, {}, {}, {})",
            serde_json::to_string(names).expect("symbol names"),
            serde_json::to_string(ambiguous).expect("symbol names"),
            level.map_or(-1, i64::from),
            limit,
        );
        let code = if self.scope_probe_installed {
            format!("python {invocation}")
        } else {
            // GDB-Python state persists for this debugger process. Define
            // static probe code once, then send only the requested names per
            // checkpoint. No runtime observation is cached by the function.
            let definition = format!(
                "def C_TRACE_P2(names, duplicates, level, limit):\n    allowed = set(names)\n    ambiguous = set(duplicates)\n{}\n{invocation}",
                python
                    .lines()
                    .map(|line| format!("    {line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            format!(
                "python exec({})",
                serde_json::to_string(&definition).expect("constant script")
            )
        };
        let response = self
            .execute(&format!(
                "-interpreter-exec console {}",
                quote_for_mi(&code)
            ))
            .await
            .map_err(|e| e.to_string())?;
        if let Some(error) = response.error_message() {
            return Err(error);
        }
        for (_, text) in response.streams {
            for line in text.lines() {
                if let Some(json) = line.strip_prefix("C_TRACE_SCOPE:") {
                    if let Ok(probe) = serde_json::from_str::<ProbeOutput>(json) {
                        self.scope_support = Some(true);
                        self.scope_probe_installed = true;
                        return Ok(probe);
                    }
                }
            }
        }
        Err("lexical probe response unavailable".into())
    }
    pub fn multi_threaded(&self) -> bool {
        self.shared.multi_threaded.load(Ordering::SeqCst)
    }
    pub fn parse_time_us(&self) -> u64 {
        self.shared.parse_time_us.load(Ordering::Relaxed)
    }
    pub fn module_epoch(&self) -> u64 {
        self.shared.module_epoch.load(Ordering::SeqCst)
    }

    /// GDB command counts by category for the profiling record.
    pub fn command_categories(&self) -> std::collections::BTreeMap<String, u64> {
        self.shared.command_categories.to_map()
    }

    /// Optimization C: activation bookkeeping for a stop whose full frame
    /// list is not needed. The call path is taken as the stored activation
    /// path plus the stopped frame, applying exactly the same
    /// truncate-to-common-prefix / append-with-monotonic-id semantics as a
    /// full capture. Ids appended here are transient: they are discarded at
    /// the next fully listed stop and never reach a snapshot.
    pub(crate) fn derive_activation_bookkeeping(&mut self, stopped_func: &str) -> u64 {
        let thread = self
            .selected_thread
            .clone()
            .unwrap_or_else(|| "unknown".into());
        let mut path: Vec<String> = self
            .activations
            .entry(thread.clone())
            .or_default()
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        path.push(stopped_func.to_string());
        let activations = self.activations.get_mut(&thread).expect("entry above");
        let common = path
            .iter()
            .zip(activations.iter())
            .take_while(|(a, (b, _))| *a == b)
            .count();
        activations.truncate(common);
        for name in path.iter().skip(common) {
            let id = self.next_activation;
            self.next_activation += 1;
            activations.push((name.clone(), id));
        }
        activations.last().map(|(_, id)| *id).unwrap_or(0)
    }

    pub async fn exec_instruction(&mut self, timeout: Duration) -> Result<StoppedInfo> {
        let resp = self.execute("-exec-step-instruction").await?;
        if let Some(message) = resp.error_message() {
            return Err(EngineError::GdbCommandError {
                command: "-exec-step-instruction".into(),
                message,
            });
        }
        self.wait_stopped(timeout).await
    }

    pub async fn interrupt(&mut self) {
        // Independent short deadline; do not extend the session to await an interrupt.
        if let Some(pid) = self.inferior_pid {
            unsafe {
                libc::kill(pid as i32, libc::SIGINT);
            }
        }
        let _ = tokio::time::timeout(Duration::from_millis(100), async {
            while let Some(record) = self.async_rx.recv().await {
                if record.class() == Some("stopped") {
                    break;
                }
            }
        })
        .await;
    }

    pub async fn discover_globals(
        &mut self,
        classifier: &UserCodeClassifier,
        cap: usize,
    ) -> Vec<String> {
        self.user_symbols(classifier, cap, "variables").await
    }

    pub async fn install_user_entries(&mut self, classifier: &UserCodeClassifier) -> Vec<String> {
        let names = self.user_symbols(classifier, 128, "functions").await;
        let mut errors = Vec::new();
        if names.len() == 128 {
            errors.push("callback entry discovery capped at 128 functions".into());
        }
        for name in names {
            match self
                .execute(&format!(
                    "-break-insert {}",
                    quote_for_mi(&format!("*{name}"))
                ))
                .await
            {
                Ok(response) => {
                    if let Some(error) = response.error_message() {
                        errors.push(error);
                    }
                }
                Err(error) => {
                    errors.push(error.to_string());
                    break;
                }
            }
        }
        errors
    }

    async fn user_symbols(
        &mut self,
        classifier: &UserCodeClassifier,
        cap: usize,
        kind: &str,
    ) -> Vec<String> {
        let Ok(response) = self
            .execute(&format!("-symbol-info-{kind} --max-results 256"))
            .await
        else {
            return Vec::new();
        };
        let mut names = Vec::new();
        if let Some(symbols) = response.record.result("symbols").and_then(as_tuple) {
            for entry in symbols {
                if entry.variable != "debug" {
                    continue;
                }
                if let MiValue::List(files) = &entry.value {
                    for file in files {
                        let MiListItem::Value(MiValue::Tuple(fields)) = file else {
                            continue;
                        };
                        let get = |key: &str| {
                            fields
                                .iter()
                                .find(|r| r.variable == key)
                                .and_then(|r| as_const(&r.value))
                        };
                        let frame = StackFrameInfo {
                            file: get("filename").map(str::to_owned),
                            full_file: get("fullname").map(str::to_owned),
                            ..Default::default()
                        };
                        if !classifier.is_user_file(&frame) {
                            continue;
                        }
                        if let Some(MiValue::List(vars)) = fields
                            .iter()
                            .find(|r| r.variable == "symbols")
                            .map(|r| &r.value)
                        {
                            for var in vars {
                                if let MiListItem::Value(MiValue::Tuple(v)) = var {
                                    if let Some(name) = v
                                        .iter()
                                        .find(|r| r.variable == "name")
                                        .and_then(|r| as_const(&r.value))
                                    {
                                        if names.len() < cap {
                                            // File qualification disambiguates file statics.
                                            let qualified = format!(
                                                "'{}'::{name}",
                                                frame.file.as_deref().unwrap_or("")
                                            );
                                            if kind == "variables" {
                                                let declaration = v
                                                    .iter()
                                                    .find(|r| r.variable == "description")
                                                    .and_then(|r| as_const(&r.value))
                                                    .unwrap_or("");
                                                self.symbol_scopes.insert(
                                                    qualified.clone(),
                                                    if declaration.starts_with("static ") {
                                                        "static"
                                                    } else {
                                                        "global"
                                                    }
                                                    .into(),
                                                );
                                            }
                                            names.push(qualified);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        names
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

    /// Read raw memory via `-data-read-memory-bytes`; returns a hex-encoded
    /// contiguous prefix, or Err when no bytes are readable. Evidence-aware
    /// callers use `read_memory_bytes_partial` to retain the partial diagnostic.
    pub async fn read_memory_bytes(
        &mut self,
        address: &str,
        size: u64,
    ) -> std::result::Result<String, String> {
        let read = self.read_memory_bytes_partial(address, size).await;
        if !read.hex.is_empty() || read.error.is_none() {
            Ok(read.hex)
        } else {
            Err(read.error.unwrap_or_default())
        }
    }

    pub(crate) async fn read_memory_bytes_partial(
        &mut self,
        address: &str,
        size: u64,
    ) -> MemoryRead {
        let failure = |error: String| MemoryRead {
            hex: String::new(),
            error: Some(error),
        };
        let Some(start) = crate::memory::address(address) else {
            return failure("invalid memory address".into());
        };
        if start.checked_add(size).is_none()
            || usize::try_from(size)
                .ok()
                .and_then(|s| s.checked_mul(2))
                .is_none()
        {
            return failure("memory range overflow".into());
        }
        if size == 0 {
            return MemoryRead {
                hex: String::new(),
                error: None,
            };
        }
        let started = Instant::now();
        let resp = self
            .execute(&format!(
                "-data-read-memory-bytes {} {}",
                quote_for_mi(address),
                size
            ))
            .await;
        self.memory_read_time_us += started.elapsed().as_micros() as u64;
        let resp = match resp {
            Ok(resp) => resp,
            Err(error) => return failure(error.to_string()),
        };
        if let Some(msg) = resp.error_message() {
            return failure(msg);
        }
        parse_memory_read(&resp.record, start, size)
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
        let cleanup_end = Instant::now() + Duration::from_secs(2);
        let mut descendants = self
            .child
            .id()
            .map(crate::supervisor::descendants)
            .unwrap_or_default();
        self.interrupt().await;
        for pid in &descendants {
            kill_process_group(*pid);
        }
        if let Some(pid) = self.child.id() {
            kill_process_group(pid);
        }
        let pid = self.shared.inferior_pid.load(Ordering::SeqCst);
        let inferior = (pid != 0).then_some(pid);
        if let Some(inferior) = inferior {
            if !descendants.contains(&inferior) {
                descendants.push(inferior);
            }
            if self.child.id() != Some(inferior) {
                kill_process_group(inferior);
            }
        }
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout_at(cleanup_end.into(), self.child.wait()).await;
        crate::supervisor::reap_descendants(descendants, cleanup_end).await;
        self.inferior_pid = None;
        self.shared.inferior_pid.store(0, Ordering::SeqCst);
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
        let pid = self.shared.inferior_pid.load(Ordering::SeqCst);
        let inferior = (pid != 0).then_some(pid);
        if let Some(inferior) = inferior {
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
    parse_frames_bounded(record).0
}

/// The extra returned frame is only a sentinel, never retained as evidence.
fn parse_frames_bounded(record: &MiRecord) -> (Vec<StackFrameInfo>, bool) {
    let mut frames = Vec::new();
    if let Some(MiValue::List(items)) = record.result("stack") {
        for item in items {
            let fields = match item {
                MiListItem::Result(res) if res.variable == "frame" => as_tuple(&res.value),
                MiListItem::Value(MiValue::Tuple(t)) => Some(t.as_slice()),
                _ => None,
            };
            if let Some(fields) = fields {
                if frames.len() == MAX_FRAMES_PER_SNAPSHOT {
                    return (frames, true);
                }
                frames.push(frame_from_tuple(fields));
            }
        }
    }
    (frames, false)
}

/// One entry of `-stack-list-variables --all-values`.
#[derive(Debug, Clone)]
pub struct RawVariable {
    pub name: String,
    pub value: String,
    pub is_argument: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
/// Wrapper of the V2.2 probe output: memo-hit flag + per-name symbols.
struct ProbeOutput {
    #[serde(default)]
    cached: bool,
    #[serde(default)]
    symbols: HashMap<String, Vec<LexicalSymbol>>,
    #[serde(default)]
    omitted: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct LexicalSymbol {
    c_type: String,
    scope_id: String,
    file: Option<String>,
    line: Option<u32>,
    storage: String,
    #[serde(default)]
    is_argument: bool,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    points_to: Option<String>,
    #[serde(default)]
    read_error: Option<String>,
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

fn parse_memory_read(record: &MiRecord, start: u64, size: u64) -> MemoryRead {
    let mut out = MemoryRead {
        hex: String::new(),
        error: None,
    };
    let Some(end) = start.checked_add(size) else {
        out.error = Some("memory range overflow".into());
        return out;
    };
    if usize::try_from(size)
        .ok()
        .and_then(|n| n.checked_mul(2))
        .is_none()
    {
        out.error = Some("memory size overflow".into());
        return out;
    }
    let Some(MiValue::List(items)) = record.result("memory") else {
        out.error = Some("memory response missing contents".into());
        return out;
    };
    let mut segments = Vec::new();
    for item in items {
        let MiListItem::Value(MiValue::Tuple(fields)) = item else {
            out.error = Some("malformed memory response entry".into());
            continue;
        };
        let field = |key: &str| {
            fields
                .iter()
                .find(|r| r.variable == key)
                .and_then(|r| as_const(&r.value))
        };
        let begin = field("begin").and_then(crate::memory::address);
        let offset = field("offset").and_then(crate::memory::address);
        let segment_end = field("end").and_then(crate::memory::address);
        let contents = field("contents");
        let (Some(begin), Some(segment_end), Some(contents)) = (begin, segment_end, contents)
        else {
            out.error = Some("memory response missing range or contents".into());
            continue;
        };
        let len = (contents.len() / 2) as u64;
        if !contents.len().is_multiple_of(2)
            || !contents.bytes().all(|b| b.is_ascii_hexdigit())
            || begin < start
            || segment_end > end
            || begin > segment_end
            || begin.checked_add(len).is_none_or(|n| n > segment_end)
            || field("offset").is_some() && offset.and_then(|n| start.checked_add(n)) != Some(begin)
        {
            out.error = Some("invalid memory response range or hex contents".into());
            continue;
        }
        if begin + len != segment_end {
            out.error = Some(format!(
                "partial memory response segment: {len} of {} bytes",
                segment_end - begin
            ));
        }
        segments.push((begin, contents));
    }
    segments.sort_by_key(|(begin, _)| *begin);
    let mut cursor = start;
    for (begin, contents) in segments {
        if begin != cursor {
            out.error = Some("noncontiguous or overlapping memory response".into());
            break;
        }
        out.hex.push_str(contents);
        cursor += (contents.len() / 2) as u64;
    }
    if cursor != end && out.error.is_none() {
        out.error = Some(format!(
            "partial memory response: {} of {size} bytes",
            cursor - start
        ));
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

/// Conservative fallback for GDB type displays when no native pointer value
/// was observed. An embedded '*' can describe an array or a struct member.
pub(crate) fn is_scalar_pointer_type(c_type: &str) -> bool {
    let mut display = c_type.trim_end();
    loop {
        let unqualified = ["const", "volatile", "restrict"]
            .iter()
            .find_map(|qualifier| {
                let prefix = display.strip_suffix(*qualifier)?;
                prefix
                    .chars()
                    .last()
                    .filter(|c| c.is_whitespace() || *c == '*')?;
                Some(prefix.trim_end())
            });
        match unqualified {
            Some(prefix) => display = prefix,
            None => return display.ends_with('*'),
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshot capture (GDB-side state; heap composition happens in trace.rs)
// ---------------------------------------------------------------------------

/// What counts as "user code" when deciding where to snapshot and how to
/// step: canonical source paths of the configured project, or any file under
/// the project directories.
#[derive(Clone, Debug)]
pub struct UserCodeClassifier {
    pub instrumentation_file: Option<String>,
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
            let source = if s.is_relative() {
                build
                    .working_directory
                    .as_ref()
                    .map(|d| d.join(&s))
                    .unwrap_or(s)
            } else {
                s
            };
            push_source(&source);
        }
        if let Some(wd) = &build.working_directory {
            if wd.is_dir() {
                dirs.push(wd.clone());
            }
        }
        for d in &build.include_dirs {
            dirs.push(if d.is_relative() {
                build
                    .working_directory
                    .as_ref()
                    .map(|wd| wd.join(d))
                    .unwrap_or(d.clone())
            } else {
                d.clone()
            });
        }
        dirs = dirs
            .into_iter()
            .map(|d| std::fs::canonicalize(&d).unwrap_or(d))
            .collect();
        Self {
            instrumentation_file: None,
            files,
            basenames,
            dirs,
        }
    }

    pub fn is_user_file(&self, frame: &StackFrameInfo) -> bool {
        if self.origin(frame) == "instrumentation_runtime" {
            return false;
        }
        self.matches_user(frame)
    }

    pub fn origin(&self, frame: &StackFrameInfo) -> &'static str {
        if [
            "__wrap_malloc",
            "__wrap_calloc",
            "__wrap_realloc",
            "__wrap_free",
        ]
        .contains(&frame.func.as_str())
            || [frame.full_file.as_ref(), frame.file.as_ref()]
                .into_iter()
                .flatten()
                .any(|file| {
                    self.instrumentation_file.as_ref().is_some_and(|runtime| {
                        file == runtime
                            || (file.ends_with("/alloc_tracker.c")
                                && std::fs::canonicalize(file)
                                    .ok()
                                    .is_some_and(|p| p.to_string_lossy() == runtime.as_str()))
                    })
                })
        {
            "instrumentation_runtime"
        } else if self.matches_user(frame) {
            "user_code"
        } else if frame.file.is_some()
            || frame.func.starts_with("__libc")
            || frame.func.contains("@plt")
        {
            "system_library"
        } else {
            "unknown"
        }
    }

    fn matches_user(&self, frame: &StackFrameInfo) -> bool {
        let candidates: [Option<&str>; 2] = [frame.full_file.as_deref(), frame.file.as_deref()];
        for cand in candidates.iter().flatten() {
            let path = Path::new(cand);
            if self.files.contains(*cand) {
                return true;
            }
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
            if frame.full_file.is_none() && path.is_absolute() {
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
    pub max_aggregate_bytes: usize,
    pub type_metadata: Option<Arc<crate::dwarf::ObservationTypes>>,
    /// Per-snapshot memory budget for raw byte reads (variables only; heap
    /// reads are budgeted by the orchestrator on top of this).
    pub memory_budget: usize,
    pub max_caller_frames: usize,
    pub max_caller_variables: usize,
    pub max_extra_memory_reads: usize,
    /// Optimization C: when set, the stop is a transient non-user stop whose
    /// captured state is discarded by the orchestrator (only activation
    /// bookkeeping consumes it). Derive the call path from the stored
    /// activation path plus this stop frame instead of listing 200 frames.
    /// Bookkeeping self-heals at the next fully listed stop.
    pub stopped_frame: Option<StackFrameInfo>,
    pub derive_chain: bool,
}

fn prepare_variable_memory(
    var: &mut VariableInfo,
    function: &str,
    opts: &CaptureOptions,
    budget: &mut crate::recorder::MemoryBudget,
) {
    // Preserve the fresh GDB extent separately from any declaration fallback.
    var.runtime_size_bytes = var.size_bytes;
    let aggregate = opts
        .type_metadata
        .as_ref()
        .is_some_and(|types| types.enrich(var, function));
    budget.max_object_bytes = if aggregate {
        opts.max_aggregate_bytes
    } else {
        opts.max_memory_read
    };
}

/// GDB-side state captured at one stop.
#[derive(Debug, Default)]
pub struct CapturedState {
    pub errors: Vec<String>,
    pub frames: Vec<StackFrameInfo>,
    pub current: Option<StackFrameInfo>,
    pub is_user_code: bool,
    pub variables: Vec<VariableInfo>,
    pub caller_frames: Vec<crate::models::FrameVariables>,
    /// Bytes spent on variable raw-byte reads in this capture.
    pub memory_used: usize,
}

/// Caller inspection never mutates GDB's selected frame or thread. A failed
/// caller probe remains local evidence and cannot discard frame-zero capture.
async fn capture_caller(
    session: &mut GdbSession,
    frame: &StackFrameInfo,
    activation: u64,
    opts: &CaptureOptions,
    remaining: usize,
    reads: &mut CheckpointReads,
    budget: &mut crate::recorder::MemoryBudget,
) -> crate::models::FrameVariables {
    let mut out = crate::models::FrameVariables {
        frame: frame.clone(),
        activation_id: activation,
        ..Default::default()
    };
    let probe = match session
        .inspect_lexical_symbols(&[], &[], Some(frame.level), remaining)
        .await
    {
        Ok(probe) => probe,
        Err(error) => {
            out.errors
                .push(format!("caller lexical capture failed: {error}"));
            return out;
        }
    };
    if probe.omitted > 0 {
        out.errors
            .push(format!("max_caller_variables: {} omitted", probe.omitted));
    }
    // Sort names for deterministic output; each name's declarations retain
    // innermost-to-outermost lexical order, exactly as in frame zero.
    let mut symbols: Vec<_> = probe.symbols.into_iter().collect();
    symbols.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (name, declarations) in symbols {
        for symbol in declarations
            .into_iter()
            .take(remaining.saturating_sub(out.variables.len()))
        {
            let mut var = VariableInfo {
                name: name.clone(),
                value: symbol.value.clone().unwrap_or_default(),
                is_argument: Some(symbol.is_argument),
                activation_id: Some(activation),
                ..Default::default()
            };
            var.c_type = symbol.c_type.clone();
            var.address = symbol.address.clone();
            var.size_bytes = symbol.size;
            var.runtime_size_bytes = symbol.size;
            var.points_to = symbol.points_to.clone();
            var.lexical_scope_id = Some(symbol.scope_id.clone());
            var.declaration_file = symbol.file.clone();
            var.declaration_line = symbol.line;
            var.scope = Some(
                if symbol.storage == "static" {
                    "static"
                } else {
                    session
                        .inferior_pid
                        .zip(var.address.as_deref().and_then(crate::memory::address))
                        .and_then(|(pid, addr)| crate::memory::storage_region(pid, addr))
                        .unwrap_or("local")
                }
                .into(),
            );
            var.identity = Some(if symbol.storage == "static" {
                format!("static:{}:{}", symbol.scope_id, name)
            } else {
                format!("activation:{activation}:{}:{}", symbol.scope_id, name)
            });
            var.scope_status = Some(lexical_scope_status(
                var.lexical_scope_id.as_deref(),
                frame.address.as_deref(),
            ));
            var.read_error = symbol.read_error.clone();
            prepare_variable_memory(&mut var, &frame.func, opts, budget);
            crate::recorder::capture_variable_memory(session, &mut var, reads, budget).await;
            out.variables.push(var);
        }
    }
    out
}

/// Capture frames + variables (+ per-variable raw bytes) at a stop.
/// `reads` is the checkpoint-scoped reuse map: variable reads are recorded
/// into it so a later `capture_global` at the same stop can re-serve
/// overlapping byte ranges without a second live read.
pub async fn capture_state(
    session: &mut GdbSession,
    opts: &CaptureOptions,
    reads: &mut CheckpointReads,
) -> Result<CapturedState> {
    let mut state = CapturedState::default();

    // Optimization C: a transient non-user stop whose captured state is
    // discarded only needs activation bookkeeping. Derive the call path from
    // the stored activation path plus the stopped frame instead of issuing a
    // full frame listing. If the stop frame is missing or unexpectedly user
    // code, fall back to the full listing.
    if opts.derive_chain {
        if let Some(stopped) = &opts.stopped_frame {
            if !opts.classifier.is_user_file(stopped) {
                let _activation = session.derive_activation_bookkeeping(&stopped.func);
                state.current = Some(stopped.clone());
                state.is_user_code = false;
                session.derived_frame_stops += 1;
                return Ok(state);
            }
        }
    }

    let frames_resp = session.execute("-stack-list-frames 0 200").await?;
    if let Some(error) = frames_resp.error_message() {
        return Err(EngineError::Gdb(error));
    }
    let (frames, chain_truncated) = parse_frames_bounded(&frames_resp.record);
    state.frames = frames;
    state.current = state.frames.first().cloned();
    state.is_user_code = state
        .current
        .as_ref()
        .map(|f| opts.classifier.is_user_file(f))
        .unwrap_or(false);

    // Activation paths run oldest to youngest; repeated function names at
    // different depths still receive distinct IDs for recursive invocations.
    let path: Vec<String> = state.frames.iter().rev().map(|f| f.func.clone()).collect();
    let activations = session
        .activations
        .entry(
            session
                .selected_thread
                .clone()
                .unwrap_or_else(|| "unknown".into()),
        )
        .or_default();
    if chain_truncated {
        // The oldest retained recursive name is not a stable root. Keeping
        // this prefix would silently reuse IDs as deeper calls slide the cap.
        activations.clear();
        state.errors.push(format!(
            "max_frames_per_snapshot: call chain exceeds {MAX_FRAMES_PER_SNAPSHOT} frames; activation continuity lost; automatic variable identities and caller observations unavailable"
        ));
    } else {
        let common = path
            .iter()
            .zip(activations.iter())
            .take_while(|(a, (b, _))| *a == b)
            .count();
        activations.truncate(common);
        for name in path.iter().skip(common) {
            let id = session.next_activation;
            session.next_activation += 1;
            activations.push((name.clone(), id));
        }
    }
    let activation = activations.last().map(|(_, id)| *id).unwrap_or(0);
    let activation_ids: Vec<u64> = activations.iter().map(|(_, id)| *id).collect();
    if !state.is_user_code {
        return Ok(state);
    }
    let vars_resp = session
        .execute("-stack-list-variables --all-values")
        .await?;
    let raw_vars = parse_variables(&vars_resp.record);
    if let Some(error) = vars_resp.error_message() {
        return Err(EngineError::Gdb(error));
    }
    // Name multiplicity over the FULL variable list (uncapped): a capped
    // duplicate must still be recognized as ambiguous. Computed once (O(n))
    // instead of two O(n²) scans per capture.
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for v in &raw_vars {
        *name_counts.entry(v.name.as_str()).or_insert(0) += 1;
    }
    let names: Vec<_> = raw_vars
        .iter()
        .take(opts.max_variables)
        .map(|v| v.name.as_str())
        .collect();
    let ambiguous: Vec<_> = names
        .iter()
        .copied()
        .filter(|name| name_counts.get(*name).copied().unwrap_or(0) > 1)
        .collect();
    let (lexical, probe_cached) = if names.is_empty() {
        (HashMap::new(), false)
    } else {
        session.lexical_symbols(&names, &ambiguous).await
    };
    if probe_cached {
        // Optimization B: static declaration data served from the GDB-side
        // per-chain memo (dynamic properties still recomputed every stop).
        session.probe_cache_hits += 1;
    }
    if !raw_vars.is_empty() && lexical.is_empty() {
        state
            .errors
            .push("lexical metadata unavailable; storage-based fallback identities used".into());
    }
    if raw_vars.len() > opts.max_variables {
        state.errors.push(format!(
            "max_variables_per_snapshot: {} omitted",
            raw_vars.len() - opts.max_variables
        ));
    }
    let mut budget = crate::recorder::MemoryBudget {
        remaining_bytes: opts.memory_budget,
        max_object_bytes: opts.max_memory_read,
        max_read_bytes: opts.max_memory_read,
        remaining_reads: None,
    };
    let function = state
        .current
        .as_ref()
        .map_or("", |frame| frame.func.as_str());
    let mut captured_ambiguous = HashSet::new();

    for (index, raw) in raw_vars.iter().take(opts.max_variables).enumerate() {
        if state.variables.len() >= opts.max_variables {
            break;
        }
        let mut var = VariableInfo {
            name: raw.name.clone(),
            value: raw.value.clone(),
            is_argument: Some(raw.is_argument),
            ..Default::default()
        };
        if name_counts.get(raw.name.as_str()).copied().unwrap_or(0) > 1 {
            if let Some(symbols) = lexical.get(&raw.name).filter(|symbols| symbols.len() > 1) {
                if !captured_ambiguous.insert(raw.name.clone()) {
                    continue;
                }
                for symbol in symbols
                    .iter()
                    .take(opts.max_variables.saturating_sub(state.variables.len()))
                {
                    let mut var = VariableInfo {
                        name: raw.name.clone(),
                        c_type: symbol.c_type.clone(),
                        value: symbol.value.clone().unwrap_or_default(),
                        address: symbol.address.clone(),
                        size_bytes: symbol.size,
                        points_to: symbol.points_to.clone(),
                        activation_id: Some(activation),
                        lexical_scope_id: Some(symbol.scope_id.clone()),
                        declaration_file: symbol.file.clone(),
                        declaration_line: symbol.line,
                        scope: Some(
                            if symbol.storage == "static" {
                                "static"
                            } else {
                                "local"
                            }
                            .into(),
                        ),
                        identity: Some(format!(
                            "activation:{activation}:{}:{}",
                            symbol.scope_id, raw.name
                        )),
                        read_error: symbol.read_error.clone(),
                        ..Default::default()
                    };
                    var.scope_status = Some(lexical_scope_status(
                        var.lexical_scope_id.as_deref(),
                        state.current.as_ref().and_then(|f| f.address.as_deref()),
                    ));
                    if symbol.storage == "static" {
                        var.identity = Some(format!("static:{}:{}", symbol.scope_id, raw.name));
                    } else {
                        var.scope = Some(
                            session
                                .inferior_pid
                                .zip(var.address.as_deref().and_then(crate::memory::address))
                                .and_then(|(pid, addr)| crate::memory::storage_region(pid, addr))
                                .unwrap_or("local")
                                .into(),
                        );
                    }
                    prepare_variable_memory(&mut var, function, opts, &mut budget);
                    crate::recorder::capture_variable_memory(session, &mut var, reads, &mut budget)
                        .await;
                    state.variables.push(var);
                }
                continue;
            }
            var.identity = Some(format!(
                "unresolved:{activation}:{:?}:{index}",
                state.current
            ));
            var.activation_id = Some(activation);
            var.scope = Some("ambiguous_lexical_scope".into());
            var.scope_status = Some(lexical_scope_status(None, None));
            var.read_error =
                Some("MI name collision: storage/type association not captured".into());
            prepare_variable_memory(&mut var, function, opts, &mut budget);
            crate::recorder::capture_variable_memory(session, &mut var, reads, &mut budget).await;
            state.variables.push(var);
            continue;
        }

        // Type via console `whatis` (one command, no varobj bookkeeping).
        let type_key = format!("{:?}:{}:{}", state.current, raw.name, activation);
        if let Some(symbol) = lexical.get(&raw.name).and_then(|symbols| symbols.first()) {
            var.c_type = symbol.c_type.clone();
            var.lexical_scope_id = Some(symbol.scope_id.clone());
            var.declaration_file = symbol.file.clone();
            var.declaration_line = symbol.line;
            // Optimization A: the probe resolved storage properties fresh at
            // this stop; only query MI for properties it could not provide
            // (VLA/register/optimized-out cases fall back to the original
            // evaluations).
            if let Some(addr) = &symbol.address {
                var.address = Some(addr.clone());
                session.saved_variable_meta_queries += 1;
            } else if let Ok(addr_val) = session.evaluate(&format!("&{}", raw.name)).await {
                var.address = extract_hex_address(&addr_val);
            }
            if let Some(size) = symbol.size {
                var.size_bytes = Some(size);
                session.saved_variable_meta_queries += 1;
            } else if let Ok(size_val) = session.evaluate(&format!("sizeof({})", raw.name)).await {
                var.size_bytes = size_val.trim().parse::<u64>().ok();
            }
        } else if let Some(c_type) = session.type_cache.get(&type_key) {
            var.c_type = c_type.clone();
        } else {
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
            if !var.c_type.is_empty() {
                session.type_cache.insert(type_key, var.c_type.clone());
            }
        }

        // When the probe was unavailable, retain the original MI fallback.
        // A present property was observed fresh at this very stop above.
        if lexical.get(&raw.name).and_then(|v| v.first()).is_none() {
            if let Ok(addr_val) = session.evaluate(&format!("&{}", raw.name)).await {
                var.address = extract_hex_address(&addr_val);
            }
            if let Ok(size_val) = session.evaluate(&format!("sizeof({})", raw.name)).await {
                var.size_bytes = size_val.trim().parse::<u64>().ok();
            }
        }

        // For pointer variables: the address stored INSIDE the pointer.
        // The pointer's own address is `var.address`; never conflated.
        // Prefer the probe's fresh target (same value, no MI round trip).
        let probe_target = lexical
            .get(&raw.name)
            .and_then(|symbols| symbols.first())
            .and_then(|symbol| symbol.points_to.clone());
        if let Some(target) = probe_target {
            var.points_to = Some(target);
            session.saved_variable_meta_queries += 1;
        } else if is_scalar_pointer_type(&var.c_type) {
            if let Ok(target) = session
                .evaluate(&format!("(unsigned long)({})", raw.name))
                .await
            {
                if let Ok(n) = target.trim().parse::<u64>() {
                    var.points_to = Some(format!("0x{n:x}"));
                }
            }
        }

        let scope = session
            .inferior_pid
            .zip(var.address.as_deref().and_then(crate::memory::address))
            .and_then(|(pid, addr)| crate::memory::storage_region(pid, addr))
            .unwrap_or("unknown");
        var.scope = Some(scope.into());
        var.identity = Some(if scope == "static" {
            format!(
                "static:{:?}:{}:{}",
                state.current.as_ref().map(|f| (&f.full_file, &f.func)),
                var.name,
                var.address.as_deref().unwrap_or("unaddressable")
            )
        } else {
            format!(
                "activation:{activation}:{}:{}",
                var.name,
                var.address.as_deref().unwrap_or("unaddressable")
            )
        });
        var.activation_id = Some(activation);
        if let Some(symbol) = lexical.get(&raw.name).and_then(|symbols| symbols.first()) {
            // Native lexical identity is independent of an object's current address.
            var.scope = Some(
                if symbol.storage == "static" {
                    "static"
                } else if scope == "stack" {
                    "stack"
                } else {
                    "local"
                }
                .into(),
            );
            var.identity = Some(if symbol.storage == "static" {
                format!("static:{}:{}", symbol.scope_id, var.name)
            } else {
                format!("activation:{activation}:{}:{}", symbol.scope_id, var.name)
            });
        }
        var.scope_status = Some(lexical_scope_status(
            var.lexical_scope_id.as_deref(),
            state.current.as_ref().and_then(|f| f.address.as_deref()),
        ));
        prepare_variable_memory(&mut var, function, opts, &mut budget);
        crate::recorder::capture_variable_memory(session, &mut var, reads, &mut budget).await;
        state.variables.push(var);
    }

    if chain_truncated {
        for var in &mut state.variables {
            var.activation_id = None;
            if var.scope.as_deref() != Some("static") {
                var.identity = None;
            }
        }
    }
    if !chain_truncated && opts.max_caller_frames > 0 && opts.max_caller_variables > 0 {
        let caller_count = state
            .frames
            .iter()
            .skip(1)
            .filter(|frame| opts.classifier.is_user_file(frame))
            .count();
        if caller_count > opts.max_caller_frames {
            state.errors.push(format!(
                "max_caller_frames: {} omitted",
                caller_count - opts.max_caller_frames
            ));
        }
        let mut remaining = opts.max_caller_variables;
        budget.remaining_reads =
            (opts.max_extra_memory_reads != 0).then_some(opts.max_extra_memory_reads);
        for (index, frame) in state
            .frames
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, frame)| opts.classifier.is_user_file(frame))
            .take(opts.max_caller_frames)
        {
            if remaining == 0 {
                state
                    .errors
                    .push("max_caller_variables: further callers omitted".into());
                break;
            }
            // Activation bookkeeping was constructed oldest to youngest.
            let depth = state.frames.len() - index - 1;
            let Some(&id) = activation_ids.get(depth) else {
                continue;
            };
            let evidence =
                capture_caller(session, frame, id, opts, remaining, reads, &mut budget).await;
            remaining = remaining.saturating_sub(evidence.variables.len());
            state.caller_frames.push(evidence);
        }
    }
    state.memory_used = opts.memory_budget.saturating_sub(budget.remaining_bytes);
    Ok(state)
}

/// Lexical-scope activity for one captured variable, derived only from
/// evidence already recorded at the stop: the variable's defining lexical
/// block address range (`block:<start>-<end>:<decl_line>`, emitted by the
/// GDB-Python lexical probe) and the captured frame-0 program counter.
///
/// The result claims lexical scope activity only — never initialization,
/// storage establishment, or value meaningfulness (see [`crate::models::ScopeStatus`]).
/// Block ranges are half-open `[start, end)`, matching GDB's block model.
pub(crate) fn lexical_scope_status(
    lexical_scope_id: Option<&str>,
    frame_address: Option<&str>,
) -> crate::models::ScopeStatus {
    use crate::models::ScopeStatus;
    let Some(id) = lexical_scope_id.and_then(|id| id.strip_prefix("block:")) else {
        return ScopeStatus::Unknown;
    };
    let Some((range, _decl_line)) = id.split_once(':') else {
        return ScopeStatus::Unknown;
    };
    let Some((start, end)) = range.split_once('-') else {
        return ScopeStatus::Unknown;
    };
    let (Ok(start), Ok(end)) = (u64::from_str_radix(start, 16), u64::from_str_radix(end, 16))
    else {
        return ScopeStatus::Unknown;
    };
    let Some(pc) = frame_address.and_then(crate::memory::address) else {
        return ScopeStatus::Unknown;
    };
    if start <= pc && pc < end {
        ScopeStatus::Active
    } else {
        ScopeStatus::Inactive
    }
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
pub fn quote_for_mi(value: &str) -> String {
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
        match (&mut reader)
            .take(16 * 1024 * 1024)
            .read_until(b'\n', &mut line_buf)
            .await
        {
            Ok(0) => break,
            Ok(_) => {
                if line_buf.last() != Some(&b'\n') {
                    shared
                        .console
                        .lock()
                        .unwrap()
                        .push_str("[protocol failure] MI record exceeds 16 MiB or is truncated\n");
                    break;
                }
                let line = String::from_utf8_lossy(&line_buf);
                handle_line(&line, &shared);
            }
            Err(_) => break,
        }
    }
    // EOF: fail every pending command.
    shared.eof.store(true, Ordering::SeqCst);
    let _ = shared.async_tx.send(MiRecord::Notify {
        class: "gdb-eof".into(),
        results: Vec::new(),
    });
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
    let started = Instant::now();
    let parsed = parse_line(trimmed);
    shared
        .parse_time_us
        .fetch_add(started.elapsed().as_micros() as u64, Ordering::Relaxed);
    match parsed {
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
                    let previously_truncated = program.truncated;
                    program.push(text.as_bytes(), shared.output_cap);
                    if !previously_truncated && program.truncated {
                        shared.output_limit.notify_one();
                    }
                }
            }
            // Collect for the in-flight command (console commands such as
            // `whatis` deliver their result through stream records).
            shared
                .current_streams
                .lock()
                .unwrap()
                .push((*kind, text.clone()));
        }
        MiRecord::Result { token, .. } => {
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
        MiRecord::Notify { class, results } => {
            if class == "library-loaded" || class == "library-unloaded" {
                shared.module_epoch.fetch_add(1, Ordering::SeqCst);
            }
            if class == "thread-created" && shared.threads_seen.fetch_add(1, Ordering::SeqCst) > 0 {
                shared.multi_threaded.store(true, Ordering::SeqCst);
            }
            if class == "thread-group-started" {
                if let Some(pid) = results
                    .iter()
                    .find(|r| r.variable == "pid")
                    .and_then(|r| as_const(&r.value))
                    .and_then(|s| s.parse().ok())
                {
                    shared.inferior_pid.store(pid, Ordering::SeqCst);
                }
            }
            if class == "thread-group-exited" {
                shared.inferior_pid.store(0, Ordering::SeqCst);
            }
            let _ = shared.async_tx.send(record);
        }
        MiRecord::Exec { .. } => {
            // Async tokens are not command acknowledgements, even if equal.
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
            let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n < 0 {
                // EAGAIN => try again later; EIO => slave closed.
                return Err(std::io::Error::last_os_error());
            }
            std::io::Result::Ok(n as usize)
        });
        match result {
            Ok(Ok(0)) => break, // closed
            Ok(Ok(n)) => {
                let mut program = shared.program.lock().unwrap();
                let previously_truncated = program.truncated;
                program.push(&buf[..n], shared.output_cap);
                if !previously_truncated && program.truncated {
                    shared.output_limit.notify_one();
                }
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

#[cfg(test)]
mod routing_regressions {
    use super::*;

    async fn mock_session(replies: Vec<(&str, Vec<String>)>) -> GdbSession {
        let script = r#"import json, re, sys
replies = iter(json.loads(sys.argv[1]))
for line in sys.stdin:
    token, command = re.match(r'(\d+)(.*)', line.strip()).groups()
    print('~' + json.dumps('MOCK_COMMAND:' + command + '\n'), flush=True)
    if command == '-gdb-exit':
        print(token + '^exit', flush=True)
        break
    expected, records = next(replies, ('NO MORE COMMANDS', []))
    if expected not in command:
        print(token + '^error,msg="unexpected command"', flush=True)
        continue
    for record in records:
        print(token + record if record.startswith('^') else record, flush=True)
"#;
        let mut child = tokio::process::Command::new("python3")
            .args([
                "-u",
                "-c",
                script,
                &serde_json::to_string(&replies).unwrap(),
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (shared, async_rx) = shared();
        let shared = Arc::new(shared);
        let reader_task = tokio::spawn(reader_task(stdout, shared.clone()));
        GdbSession {
            child,
            stdin,
            shared,
            async_rx,
            backlog: Vec::new(),
            next_token: 1,
            reader_task,
            pty_task: None,
            inferior_pid: None,
            commands_issued: Arc::new(AtomicU64::new(0)),
            deadline: Instant::now() + Duration::from_secs(10),
            max_commands: 100,
            poisoned: false,
            activations: HashMap::new(),
            selected_thread: Some("1".into()),
            next_activation: 1,
            type_cache: HashMap::new(),
            command_time_us: 0,
            memory_read_time_us: 0,
            symbol_scopes: HashMap::new(),
            scope_support: None,
            scope_probe_installed: false,
            global_meta_cache: HashMap::new(),
            probe_cache_hits: 0,
            saved_variable_meta_queries: 0,
            saved_memory_reads: 0,
            derived_frame_stops: 0,
            global_meta_cache_hits: 0,
        }
    }

    fn probe_reply(symbols: serde_json::Value, omitted: usize) -> Vec<String> {
        let text = format!(
            "C_TRACE_SCOPE:{}\n",
            serde_json::json!({"symbols": symbols, "omitted": omitted})
        );
        vec![
            format!("~{}", serde_json::to_string(&text).unwrap()),
            "^done".into(),
        ]
    }

    fn capture_options() -> CaptureOptions {
        CaptureOptions {
            classifier: UserCodeClassifier::from_build(&crate::config::BuildConfig {
                source_file: "/project/user.c".into(),
                ..Default::default()
            }),
            max_variables: 8,
            max_memory_read: 16,
            max_aggregate_bytes: 16,
            type_metadata: None,
            memory_budget: 64,
            max_caller_frames: 2,
            max_caller_variables: 8,
            max_extra_memory_reads: 0,
            stopped_frame: None,
            derive_chain: false,
        }
    }

    #[tokio::test]
    async fn truncated_recursive_chain_loses_identity_without_losing_frame_zero_bytes() {
        let mut replies = Vec::new();
        for count in [200, 201, 201, 200] {
            let frames = (0..count).map(|level| format!(
                r#"frame={{level="{level}",addr="0x110",func="recurse",fullname="/project/user.c"}}"#
            )).collect::<Vec<_>>().join(",");
            replies.push((
                "-stack-list-frames 0 200",
                vec![format!("^done,stack=[{frames}]")],
            ));
            replies.push((
                "-stack-list-variables",
                vec![r#"^done,variables=[{name="x",value="7"},{name="s",value="9"}]"#.into()],
            ));
            replies.push(("python", probe_reply(serde_json::json!({
                "x": [{"c_type":"int", "scope_id":"block:100-200:2", "file":"/project/user.c", "line":2, "storage":"automatic", "address":"0x1000", "size":4}],
                "s": [{"c_type":"int", "scope_id":"block:100-200:3", "file":"/project/user.c", "line":3, "storage":"static", "address":"0x2000", "size":4}]
            }), 0)));
            replies.push((
                "-data-read-memory-bytes",
                vec![r#"^done,memory=[{begin="0x1000",end="0x1004",contents="07000000"}]"#.into()],
            ));
            replies.push((
                "-data-read-memory-bytes",
                vec![r#"^done,memory=[{begin="0x2000",end="0x2004",contents="09000000"}]"#.into()],
            ));
        }
        let mut session = mock_session(replies).await;
        let mut options = capture_options();
        options.max_caller_frames = 0;
        let complete = capture_state(&mut session, &options, &mut CheckpointReads::default())
            .await
            .unwrap();
        assert_eq!(complete.frames.len(), 200);
        assert!(complete.errors.is_empty());
        let old_activation = complete.variables[0].activation_id.unwrap();
        let next_activation = session.next_activation;
        for _ in 0..2 {
            options.max_caller_frames = 3;
            let truncated = capture_state(&mut session, &options, &mut CheckpointReads::default())
                .await
                .unwrap();
            assert_eq!(truncated.frames.len(), 200);
            assert!(truncated
                .errors
                .iter()
                .any(|e| e.contains("activation continuity lost")));
            assert!(truncated.caller_frames.is_empty());
            assert!(truncated
                .variables
                .iter()
                .all(|v| v.activation_id.is_none()));
            let automatic = &truncated.variables[0];
            assert!(automatic.identity.is_none());
            assert_eq!(automatic.value, "7");
            assert_eq!(automatic.address.as_deref(), Some("0x1000"));
            assert_eq!(automatic.raw_bytes_hex.as_deref(), Some("07000000"));
            assert_eq!(automatic.memory_evidence.as_ref().unwrap().status, "full");
            assert_eq!(
                truncated.variables[1].identity.as_deref(),
                Some("static:block:100-200:3:s")
            );
            assert!(session.activations["1"].is_empty());
            assert_eq!(session.next_activation, next_activation);
        }
        options.max_caller_frames = 0;
        let recovered = capture_state(&mut session, &options, &mut CheckpointReads::default())
            .await
            .unwrap();
        assert_eq!(recovered.frames.len(), 200);
        assert!(recovered.errors.is_empty());
        assert!(recovered.variables[0].activation_id.unwrap() > old_activation);
        assert_eq!(session.command_categories()["stack_frames"], 4);
        assert_eq!(
            session
                .console_log()
                .matches("MOCK_COMMAND:-stack-list-frames 0 200")
                .count(),
            4
        );
        session.shutdown().await;
    }

    #[test]
    fn scalar_pointer_type_display_rejects_aggregate_and_function_pointer_syntax() {
        for c_type in [
            "int *",
            "const int *",
            "int **",
            "int * const volatile restrict",
            "int *const",
            "struct item *",
            "struct { int *member; } *",
        ] {
            assert!(is_scalar_pointer_type(c_type), "{c_type}");
        }
        for c_type in [
            "int",
            "int *[2]",
            "int * [2] const",
            "struct { int *member; }",
            "union { int *member; int value; }",
            "int (*)(int)",
            "int (*)[2]",
            "pointer_typedef const",
            "int *notconst",
            "",
        ] {
            assert!(!is_scalar_pointer_type(c_type), "{c_type}");
        }
    }

    #[tokio::test]
    async fn frame_zero_pointer_array_has_no_fabricated_pointer_fallback() {
        let symbols = serde_json::json!({"slots": [{
            "c_type":"int *[2]", "scope_id":"block:100-200:2", "file":"/project/user.c",
            "line":2, "storage":"automatic", "address":"0x1000", "size":16
        }]});
        let mut session = mock_session(vec![
            ("-stack-list-frames", vec![r#"^done,stack=[frame={level="0",addr="0x110",func="main",fullname="/project/user.c"}]"#.into()]),
            ("-stack-list-variables", vec![r#"^done,variables=[{name="slots",value="{0x2000, 0x3000}"}]"#.into()]),
            ("python", probe_reply(symbols, 0)),
            ("-data-read-memory-bytes", vec![r#"^done,memory=[{begin="0x1000",end="0x1010",contents="00200000000000000030000000000000"}]"#.into()]),
        ]).await;
        let state = capture_state(
            &mut session,
            &capture_options(),
            &mut CheckpointReads::default(),
        )
        .await
        .unwrap();
        assert_eq!(state.variables.len(), 1);
        assert!(state.variables[0].points_to.is_none());
        assert_eq!(
            state.variables[0].raw_bytes_hex.as_deref(),
            Some("00200000000000000030000000000000")
        );
        assert!(!session.console_log().contains("(unsigned long)(slots)"));
        assert_eq!(session.command_categories()["data_evaluate"], 0);
        session.shutdown().await;
    }

    #[tokio::test]
    async fn selected_memory_retains_partial_diagnostic_and_accounts_actual_bytes() {
        use crate::config::{MemoryRegion, TracingConfig};
        use crate::models::DataValidity;
        let mut session = mock_session(vec![
            (
                "-data-evaluate-expression",
                vec![r#"^done,value="4096""#.into()],
            ),
            (
                r#"-data-read-memory-bytes "0x1000" 16"#,
                vec![
                    r#"^done,memory=[{begin="0x1000",end="0x1008",contents="0102030405060708"}]"#
                        .into(),
                ],
            ),
            (
                "-data-evaluate-expression",
                vec![r#"^done,value="8192""#.into()],
            ),
            (
                r#"-data-read-memory-bytes "0x2000" 8"#,
                vec![
                    r#"^done,memory=[{begin="0x2000",end="0x2008",contents="090a0b0c0d0e0f10"}]"#
                        .into(),
                ],
            ),
            (
                "-data-evaluate-expression",
                vec![r#"^error,msg="No symbol missing""#.into()],
            ),
            (
                "-data-evaluate-expression",
                vec![r#"^done,value="12288""#.into()],
            ),
        ])
        .await;
        let config = TracingConfig {
            memory_regions: [
                ("first", 16),
                ("second", 8),
                ("missing", 4),
                ("exhausted", 4),
            ]
            .into_iter()
            .map(|(address, size)| MemoryRegion {
                address: address.into(),
                size,
            })
            .collect(),
            ..Default::default()
        };
        let limits = crate::resource_limits::ExecutionLimits {
            max_snapshot_memory_bytes: 16,
            max_memory_read_bytes: 16,
            ..Default::default()
        };
        let recorder = crate::recorder::Recorder::new(&config, &limits).unwrap();
        let observations = recorder.selected_memory(&mut session).await;
        assert_eq!(observations.len(), 4);
        assert_eq!(observations[0].requested_bytes, 16);
        assert_eq!(
            observations[0].raw_bytes_hex.as_deref(),
            Some("0102030405060708")
        );
        assert_eq!(observations[0].status, DataValidity::Truncated);
        assert!(observations[0].error.as_ref().unwrap().contains("8 of 16"));
        assert_eq!(
            observations[1].raw_bytes_hex.as_deref(),
            Some("090a0b0c0d0e0f10")
        );
        assert_eq!(observations[1].status, DataValidity::Valid);
        assert_eq!(observations[2].status, DataValidity::Unavailable);
        assert!(observations[2].address.is_none());
        assert_eq!(observations[3].status, DataValidity::Truncated);
        assert!(observations[3].error.as_ref().unwrap().contains("budget"));
        assert_eq!(session.command_categories()["memory_reads"], 2);
        session.shutdown().await;
    }

    #[tokio::test]
    async fn caller_probe_failure_preserves_frame_zero_and_selected_context() {
        let symbols = serde_json::json!({"x": [{"c_type":"int", "scope_id":"block:100-200:2", "file":"/project/user.c", "line":2, "storage":"automatic", "address":"0x1000", "size":4}]});
        let mut session = mock_session(vec![
            ("-stack-list-frames", vec![r#"^done,stack=[frame={level="0",addr="0x110",func="recurse",fullname="/project/user.c",line="2"},frame={level="3",addr="0x120",func="recurse",fullname="/project/user.c",line="3"}]"#.into()]),
            ("-stack-list-variables", vec![r#"^done,variables=[{name="x",value="7"}]"#.into()]),
            ("python", probe_reply(symbols, 0)),
            ("-data-read-memory-bytes", vec![r#"^done,memory=[{begin="0x1000",offset="0x0",end="0x1004",contents="07000000"}]"#.into()]),
            ("C_TRACE_P2([], [], 3, 8)", vec![r#"^error,msg="frame unavailable""#.into()]),
            ("-stack-info-frame", vec![r#"^done,frame={level="0",func="recurse"}"#.into()]),
        ]).await;
        let mut reads = CheckpointReads::default();
        let state = capture_state(&mut session, &capture_options(), &mut reads)
            .await
            .unwrap();
        assert_eq!(state.variables.len(), 1);
        assert_eq!(
            state.variables[0].raw_bytes_hex.as_deref(),
            Some("07000000")
        );
        assert_eq!(state.caller_frames.len(), 1);
        assert!(state.caller_frames[0].errors[0].contains("frame unavailable"));
        assert_ne!(
            state.variables[0].activation_id,
            Some(state.caller_frames[0].activation_id)
        );
        assert!(!session.poisoned);
        assert_eq!(session.selected_thread.as_deref(), Some("1"));
        assert!(!session.console_log().contains("-stack-select-frame"));
        assert!(!session.console_log().contains("-thread-select"));
        assert!(!session.console_log().contains(".select("));
        assert!(session
            .execute("-stack-info-frame")
            .await
            .unwrap()
            .error_message()
            .is_none());
        session.shutdown().await;
    }

    #[tokio::test]
    async fn caller_shadows_have_lexical_identities_and_fresh_extents_without_selection() {
        let declarations = serde_json::json!({"x": [
            {"c_type":"int[3]", "scope_id":"block:100-180:5", "file":"/project/user.c", "line":5, "storage":"automatic", "value":"{1, 2, 3}", "address":"0x1000", "size":12},
            {"c_type":"int", "scope_id":"block:100-200:2", "file":"/project/user.c", "line":2, "storage":"static", "value":"7", "address":"0x2000", "size":4}
        ]});
        let mut session = mock_session(vec![
            ("python", probe_reply(declarations, 3)),
            ("-data-read-memory-bytes", vec![r#"^done,memory=[{begin="0x1000",end="0x100c",contents="010000000200000003000000"}]"#.into()]),
            ("-data-read-memory-bytes", vec![r#"^done,memory=[{begin="0x2000",end="0x2004",contents="07000000"}]"#.into()]),
        ]).await;
        let mut budget = crate::recorder::MemoryBudget {
            remaining_bytes: 64,
            max_object_bytes: 16,
            max_read_bytes: 16,
            remaining_reads: None,
        };
        let frame = StackFrameInfo {
            level: 3,
            func: "recurse".into(),
            address: Some("0x110".into()),
            ..Default::default()
        };
        let out = capture_caller(
            &mut session,
            &frame,
            42,
            &capture_options(),
            2,
            &mut CheckpointReads::default(),
            &mut budget,
        )
        .await;
        assert_eq!(out.variables.len(), 2);
        assert!(out.errors[0].contains("3 omitted"));
        assert_eq!(
            out.variables[0].identity.as_deref(),
            Some("activation:42:block:100-180:5:x")
        );
        assert_eq!(
            out.variables[1].identity.as_deref(),
            Some("static:block:100-200:2:x")
        );
        assert_eq!(out.variables[0].runtime_size_bytes, Some(12));
        assert_eq!(
            out.variables[0].memory_evidence.as_ref().unwrap().status,
            "full"
        );
        assert_eq!(budget.remaining_bytes, 48);
        assert!(out.variables.iter().all(|v| v.activation_id == Some(42)));
        assert_eq!(session.command_categories()["memory_reads"], 2);
        assert!(!session.console_log().contains("-stack-select-frame"));
        assert!(!session.console_log().contains(".select("));
        session.shutdown().await;
    }

    #[tokio::test]
    async fn variable_memory_unavailable_unknown_overflow_and_budget_fail_closed() {
        let mut session = mock_session(Vec::new()).await;
        let mut reads = CheckpointReads::default();
        for (address, extent, remaining, status) in [
            (None, Some(4), 16, "unavailable"),
            (Some("0x1000"), None, 16, "unknown"),
            (Some("0xfffffffffffffffe"), Some(4), 16, "unknown"),
            (Some("0x1000"), Some(4), 0, "unattempted"),
        ] {
            let mut var = VariableInfo {
                address: address.map(str::to_owned),
                size_bytes: extent,
                value: "7".into(),
                ..Default::default()
            };
            let mut budget = crate::recorder::MemoryBudget {
                remaining_bytes: remaining,
                max_object_bytes: 2,
                max_read_bytes: 4,
                remaining_reads: None,
            };
            crate::recorder::capture_variable_memory(
                &mut session,
                &mut var,
                &mut reads,
                &mut budget,
            )
            .await;
            let evidence = var.memory_evidence.as_ref().unwrap();
            assert_eq!(evidence.status, status);
            assert_eq!(evidence.requested_bytes, 0);
            assert_eq!(evidence.captured_bytes, 0);
            assert_eq!(budget.remaining_bytes, remaining);
            assert!(var.raw_bytes_hex.is_none());
        }
        let mut cached = VariableInfo {
            address: Some("0x1001".into()),
            size_bytes: Some(2),
            value: "7".into(),
            ..Default::default()
        };
        reads.insert("0x1000", 4, "01020304");
        let mut budget = crate::recorder::MemoryBudget {
            remaining_bytes: 2,
            max_object_bytes: 2,
            max_read_bytes: 4,
            remaining_reads: Some(0),
        };
        crate::recorder::capture_variable_memory(
            &mut session,
            &mut cached,
            &mut reads,
            &mut budget,
        )
        .await;
        assert_eq!(cached.raw_bytes_hex.as_deref(), Some("0203"));
        assert_eq!(cached.memory_evidence.as_ref().unwrap().status, "full");
        assert_eq!(budget.remaining_bytes, 0);
        assert_eq!(budget.remaining_reads, Some(0));
        assert_eq!(session.command_categories()["memory_reads"], 0);
        session.shutdown().await;
    }

    #[tokio::test]
    async fn variable_memory_partial_response_preserves_bytes_and_unreadable_reports_error() {
        let mut session = mock_session(vec![
            (
                "-data-read-memory-bytes",
                vec![
                    r#"^done,memory=[{begin="0x1000",offset="0x0",end="0x1004",contents="0102"}]"#
                        .into(),
                ],
            ),
            (
                "-data-read-memory-bytes",
                vec![r#"^error,msg="Cannot access memory""#.into()],
            ),
        ])
        .await;
        let mut budget = crate::recorder::MemoryBudget {
            remaining_bytes: 16,
            max_object_bytes: 4,
            max_read_bytes: 4,
            remaining_reads: Some(2),
        };
        let mut reads = CheckpointReads::default();
        for (address, expected_status, expected_hex, captured) in [
            ("0x1000", "partial", Some("0102"), 2),
            ("0x2000", "unreadable", None, 0),
        ] {
            let mut var = VariableInfo {
                address: Some(address.into()),
                size_bytes: Some(4),
                value: "7".into(),
                ..Default::default()
            };
            crate::recorder::capture_variable_memory(
                &mut session,
                &mut var,
                &mut reads,
                &mut budget,
            )
            .await;
            let evidence = var.memory_evidence.as_ref().unwrap();
            assert_eq!(evidence.status, expected_status);
            assert_eq!(evidence.requested_bytes, 4);
            assert_eq!(evidence.captured_bytes, captured);
            assert!(evidence.reason.is_some());
            assert_eq!(var.raw_bytes_hex.as_deref(), expected_hex);
            assert!(var
                .read_error
                .as_ref()
                .unwrap()
                .contains("memory read failed"));
        }
        assert_eq!(budget.remaining_bytes, 14);
        assert_eq!(budget.remaining_reads, Some(0));
        assert_eq!(reads.lookup("0x1000", 2).as_deref(), Some("0102"));
        assert!(reads.lookup("0x1000", 4).is_none());
        session.shutdown().await;
    }

    #[tokio::test]
    async fn aggregate_reads_obey_single_read_cap_and_keep_prefix_on_quota_or_failure() {
        for mode in ["full", "quota", "failure"] {
            let mut replies = vec![(
                r#"-data-read-memory-bytes "0x1000" 4"#,
                vec![r#"^done,memory=[{begin="0x1000",end="0x1004",contents="01020304"}]"#.into()],
            )];
            if mode != "quota" {
                replies.push((
                    r#"-data-read-memory-bytes "0x1004" 4"#,
                    vec![if mode == "failure" {
                        r#"^error,msg="Cannot access memory""#.into()
                    } else {
                        r#"^done,memory=[{begin="0x1004",end="0x1008",contents="05060708"}]"#.into()
                    }],
                ));
            }
            if mode == "full" {
                replies.push((
                    r#"-data-read-memory-bytes "0x1008" 4"#,
                    vec![
                        r#"^done,memory=[{begin="0x1008",end="0x100c",contents="090a0b0c"}]"#
                            .into(),
                    ],
                ));
            }
            let mut session = mock_session(replies).await;
            let mut var = VariableInfo {
                address: Some("0x1000".into()),
                size_bytes: Some(12),
                value: "{...}".into(),
                ..Default::default()
            };
            let mut budget = crate::recorder::MemoryBudget {
                remaining_bytes: 16,
                max_object_bytes: 12,
                max_read_bytes: 4,
                remaining_reads: if mode == "quota" { Some(1) } else { None },
            };
            let mut reads = CheckpointReads::default();
            crate::recorder::capture_variable_memory(
                &mut session,
                &mut var,
                &mut reads,
                &mut budget,
            )
            .await;
            let evidence = var.memory_evidence.as_ref().unwrap();
            assert_eq!(
                evidence.status,
                match mode {
                    "full" => "full",
                    "quota" => "truncated",
                    _ => "partial",
                }
            );
            assert_eq!(
                evidence.requested_bytes,
                match mode {
                    "full" => 12,
                    "quota" => 4,
                    _ => 8,
                }
            );
            let expected_hex = if mode == "full" {
                "0102030405060708090a0b0c"
            } else {
                "01020304"
            };
            assert_eq!(var.raw_bytes_hex.as_deref(), Some(expected_hex));
            assert_eq!(evidence.captured_bytes, (expected_hex.len() / 2) as u64);
            assert_eq!(budget.remaining_bytes, 16 - expected_hex.len() / 2);
            assert_eq!(
                reads.lookup("0x1000", evidence.captured_bytes).as_deref(),
                Some(expected_hex)
            );
            assert_eq!(
                session.command_categories()["memory_reads"],
                match mode {
                    "full" => 3,
                    "quota" => 1,
                    _ => 2,
                }
            );
            session.shutdown().await;
        }
    }

    #[tokio::test]
    async fn caller_memory_quota_counts_commands_not_cache_hits_or_retained_failures() {
        let mut session = mock_session(vec![(
            "-data-read-memory-bytes",
            vec![r#"^error,msg="Cannot access memory""#.into()],
        )])
        .await;
        let mut reads = CheckpointReads::default();
        reads.insert("0x1000", 8, "0102030405060708");
        let mut budget = crate::recorder::MemoryBudget {
            remaining_bytes: 16,
            max_object_bytes: 4,
            max_read_bytes: 4,
            remaining_reads: Some(1),
        };
        let variable = |address: &str| VariableInfo {
            address: Some(address.into()),
            size_bytes: Some(4),
            value: "1".into(),
            ..Default::default()
        };
        let mut cached = variable("0x1002");
        crate::recorder::capture_variable_memory(
            &mut session,
            &mut cached,
            &mut reads,
            &mut budget,
        )
        .await;
        assert_eq!(cached.raw_bytes_hex.as_deref(), Some("03040506"));
        assert_eq!(budget.remaining_reads, Some(1));
        assert_eq!(budget.remaining_bytes, 12);
        let mut failed = variable("0x2000");
        crate::recorder::capture_variable_memory(
            &mut session,
            &mut failed,
            &mut reads,
            &mut budget,
        )
        .await;
        assert_eq!(
            failed.memory_evidence.as_ref().unwrap().status,
            "unreadable"
        );
        assert_eq!(budget.remaining_reads, Some(0));
        assert_eq!(budget.remaining_bytes, 12);
        let mut blocked = variable("0x3000");
        crate::recorder::capture_variable_memory(
            &mut session,
            &mut blocked,
            &mut reads,
            &mut budget,
        )
        .await;
        assert_eq!(
            blocked.memory_evidence.as_ref().unwrap().status,
            "unattempted"
        );
        assert_eq!(session.command_categories()["memory_reads"], 1);
        session.shutdown().await;
    }

    #[tokio::test]
    async fn caller_zero_read_quota_is_unlimited_and_sparse_levels_keep_activations() {
        for quota in [0, 1] {
            let symbol = |address: &str| {
                serde_json::json!({"x": [{
                    "c_type":"int", "scope_id":"block:100-200:2", "file":"/project/user.c",
                    "line":2, "storage":"automatic", "value":"7", "address":address, "size":4
                }]})
            };
            let mut replies = vec![
                ("-stack-list-frames", vec![r#"^done,stack=[frame={level="0",addr="0x110",func="recurse",fullname="/project/user.c"},frame={level="1",func="library"},frame={level="3",addr="0x120",func="recurse",fullname="/project/user.c"},frame={level="5",addr="0x130",func="main",fullname="/project/user.c"}]"#.into()]),
                ("-stack-list-variables", vec!["^done,variables=[]".into()]),
                ("python", probe_reply(symbol("0x1000"), 0)),
                ("-data-read-memory-bytes", vec![r#"^done,memory=[{begin="0x1000",end="0x1004",contents="07000000"}]"#.into()]),
                ("C_TRACE_P2([], [], 5, 7)", probe_reply(symbol("0x2000"), 0)),
            ];
            if quota == 0 {
                replies.push((
                    "-data-read-memory-bytes",
                    vec![
                        r#"^done,memory=[{begin="0x2000",end="0x2004",contents="07000000"}]"#
                            .into(),
                    ],
                ));
            }
            let mut session = mock_session(replies).await;
            let mut options = capture_options();
            options.max_extra_memory_reads = quota;
            let state = capture_state(&mut session, &options, &mut CheckpointReads::default())
                .await
                .unwrap();
            assert_eq!(state.caller_frames.len(), 2);
            assert_eq!(state.caller_frames[0].frame.level, 3);
            assert_eq!(state.caller_frames[0].activation_id, 2);
            assert_eq!(state.caller_frames[1].frame.level, 5);
            assert_eq!(state.caller_frames[1].activation_id, 1);
            assert_eq!(state.memory_used, if quota == 0 { 8 } else { 4 });
            let status = &state.caller_frames[1].variables[0]
                .memory_evidence
                .as_ref()
                .unwrap()
                .status;
            assert_eq!(status, if quota == 0 { "full" } else { "unattempted" });
            assert_eq!(
                session.command_categories()["memory_reads"],
                if quota == 0 { 2 } else { 1 }
            );
            session.shutdown().await;
        }
    }

    fn shared() -> (Shared, mpsc::UnboundedReceiver<MiRecord>) {
        let (async_tx, rx) = mpsc::unbounded_channel();
        (
            Shared {
                output_limit: tokio::sync::Notify::new(),
                inferior_pid: AtomicU32::new(0),
                multi_threaded: AtomicBool::new(false),
                threads_seen: AtomicU32::new(0),
                parse_time_us: AtomicU64::new(0),
                module_epoch: AtomicU64::new(0),
                pending: Mutex::new(HashMap::new()),
                current_streams: Mutex::new(Vec::new()),
                async_tx,
                console: Mutex::new(String::new()),
                program: Mutex::new(ProgramBuffer::default()),
                eof: AtomicBool::new(false),
                output_cap: 1024,
                debug_echo: false,
                command_categories: CommandCategories::default(),
            },
            rx,
        )
    }

    #[test]
    fn command_categories_classify_without_changing_commands() {
        let categories = CommandCategories::default();
        for command in [
            "-stack-list-frames 0 200",
            "-stack-list-variables --all-values",
            "-interpreter-exec console \"python exec('x')\"",
            "-interpreter-exec console \"python C_TRACE_P2([], [], 3, 8)\"",
            "-interpreter-exec console \"whatis x\"",
            "-data-evaluate-expression \"sizeof(x)\"",
            "-data-read-memory-bytes 0x100 4",
            "-exec-step",
        ] {
            categories.record(command);
        }
        let counts = categories.to_map();
        for name in [
            "stack_frames",
            "stack_variables",
            "lexical_probe",
            "caller_probe",
            "type_queries",
            "data_evaluate",
            "memory_reads",
            "exec_steps",
        ] {
            assert_eq!(counts[name], 1, "{name}");
        }
        assert_eq!(counts.values().sum::<u64>(), 8);
    }

    #[test]
    fn checkpoint_reads_reuse_contained_subsets_only() {
        let mut reads = CheckpointReads::default();
        assert!(reads.lookup("0x1000", 4).is_none());
        reads.insert("0x1000", 4, "01020304");
        assert_eq!(reads.lookup("0x1000", 4).as_deref(), Some("01020304"));
        assert_eq!(reads.lookup("0x1000", 2).as_deref(), Some("0102"));
        assert_eq!(reads.lookup("0x1001", 2).as_deref(), Some("0203"));
        assert!(reads.lookup("0xfff", 2).is_none());
        assert!(reads.lookup("0x1003", 2).is_none());
        assert!(reads.lookup("0x1000", 5).is_none());
        assert!(CheckpointReads::default().lookup("0x1000", 4).is_none());
    }

    #[test]
    fn checkpoint_cache_keeps_actual_bytes_and_longest_same_start() {
        let mut reads = CheckpointReads::default();
        reads.insert("0x1000", 8, "0102");
        assert_eq!(reads.lookup("0x1000", 2).as_deref(), Some("0102"));
        assert!(reads.lookup("0x1000", 3).is_none());
        reads.insert("0x1000", 4, "01020304");
        reads.insert("0x1000", 1, "01");
        assert_eq!(reads.lookup("0x1001", 3).as_deref(), Some("020304"));
    }

    #[test]
    fn checkpoint_cache_rejects_invalid_hex_and_overflow() {
        let mut reads = CheckpointReads::default();
        for hex in ["0", "0z", "\u{00e9}\u{00e9}"] {
            reads.insert("0x1000", 2, hex);
        }
        reads.insert("0xffffffffffffffff", 1, "01");
        assert!(reads.lookup("0x1000", 1).is_none());
        assert!(reads.lookup("0xffffffffffffffff", 1).is_none());
        assert!(reads.lookup("0x1", u64::MAX).is_none());
        assert!(reads.lookup("0x0", u64::MAX / 2 + 1).is_none());
    }

    fn memory_response(body: &str, start: u64, size: u64) -> MemoryRead {
        parse_memory_read(&parse_line(body).unwrap().unwrap(), start, size)
    }

    #[test]
    fn memory_parser_combines_contiguous_segments_in_address_order() {
        let read = memory_response(
            r#"1^done,memory=[{begin="0x1002",offset="0x2",end="0x1004",contents="0304"},{begin="0x1000",offset="0x0",end="0x1002",contents="0102"}]"#,
            0x1000,
            4,
        );
        assert_eq!(read.hex, "01020304");
        assert!(read.error.is_none());
    }

    #[test]
    fn memory_parser_preserves_prefix_but_never_bridges_holes_or_overlaps() {
        for tail in [
            r#"{begin="0x1003",offset="0x3",end="0x1004",contents="04"}"#,
            r#"{begin="0x1001",offset="0x1",end="0x1003",contents="0203"}"#,
        ] {
            let body = format!(
                r#"1^done,memory=[{{begin="0x1000",offset="0x0",end="0x1002",contents="0102"}},{tail}]"#
            );
            let read = memory_response(&body, 0x1000, 4);
            assert_eq!(read.hex, "0102");
            assert!(read.error.unwrap().contains("noncontiguous"));
        }
        let read = memory_response(
            r#"1^done,memory=[{begin="0x1002",offset="0x2",end="0x1004",contents="0304"}]"#,
            0x1000,
            4,
        );
        assert!(read.hex.is_empty());
        assert!(read.error.is_some());
    }

    #[test]
    fn memory_parser_marks_short_prefix_partial() {
        let read = memory_response(
            r#"1^done,memory=[{begin="0x1000",offset="0x0",end="0x1004",contents="0102"}]"#,
            0x1000,
            4,
        );
        assert_eq!(read.hex, "0102");
        assert!(read.error.unwrap().contains("2 of 4"));
    }

    #[test]
    fn memory_parser_rejects_bad_range_hex_offset_and_overflow() {
        for entry in [
            r#"{begin="0x1000",end="0x1002",contents="010"}"#,
            r#"{begin="0x1000",end="0x1002",contents="01zz"}"#,
            r#"{begin="0x1000",offset="0x1",end="0x1002",contents="0102"}"#,
            r#"{begin="0x1000",end="0x1001",contents="0102"}"#,
            r#"{begin="0x1000",end="0x1005",contents="0102"}"#,
            r#"{begin="0x1000",contents="0102"}"#,
        ] {
            let read = memory_response(&format!("1^done,memory=[{entry}]"), 0x1000, 4);
            assert!(read.hex.is_empty(), "{entry}");
            assert!(read.error.is_some(), "{entry}");
        }
        let read = memory_response("1^done,memory=[]", u64::MAX, 1);
        assert!(read.error.unwrap().contains("overflow"));
    }

    #[test]
    fn module_notifications_advance_global_metadata_epoch() {
        let (shared, _rx) = shared();
        assert_eq!(shared.module_epoch.load(Ordering::SeqCst), 0);
        let key_before = format!(
            "global:{}:'x.c'::a",
            shared.module_epoch.load(Ordering::SeqCst)
        );
        for (class, expected) in [
            ("library-loaded", 1),
            ("thread-group-started", 1),
            ("library-unloaded", 2),
        ] {
            route_record(
                MiRecord::Notify {
                    class: class.into(),
                    results: Vec::new(),
                },
                &shared,
            );
            assert_eq!(shared.module_epoch.load(Ordering::SeqCst), expected);
        }
        let key_after = format!(
            "global:{}:'x.c'::a",
            shared.module_epoch.load(Ordering::SeqCst)
        );
        assert_ne!(key_before, key_after);
    }

    #[tokio::test]
    async fn async_token_cannot_acknowledge_command() {
        let (shared, mut async_rx) = shared();
        let (tx, mut response) = oneshot::channel();
        shared.pending.lock().unwrap().insert(7, tx);
        handle_line("7*stopped,reason=\"end-stepping-range\"", &shared);
        assert!(response.try_recv().is_err());
        assert_eq!(async_rx.recv().await.unwrap().class(), Some("stopped"));
        handle_line("7^running", &shared);
        assert_eq!(response.await.unwrap().unwrap().0.class(), Some("running"));
    }

    #[tokio::test]
    async fn late_and_unexpected_results_are_preserved_not_reassigned() {
        let (shared, mut async_rx) = shared();
        let (tx, response) = oneshot::channel();
        shared.pending.lock().unwrap().insert(2, tx);
        handle_line("1^done,value=\"old\"", &shared);
        assert_eq!(async_rx.recv().await.unwrap().class(), Some("done"));
        handle_line("2^done,value=\"new\"", &shared);
        assert_eq!(
            as_const(response.await.unwrap().unwrap().0.result("value").unwrap()),
            Some("new")
        );
    }

    #[test]
    fn partial_or_malformed_response_never_becomes_zero() {
        let (shared, _) = shared();
        handle_line("9^done,value=\"unterminated", &shared);
        assert!(shared.console.lock().unwrap().contains("[unparsed]"));
        assert_eq!(extract_hex_address("<unavailable>"), None);
    }

    #[tokio::test]
    async fn debugger_death_fails_wait_promptly() {
        if tokio::process::Command::new("gdb")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            eprintln!("SKIPPED: gdb missing");
            return;
        }
        let options = GdbLaunchOptions {
            binary: "/bin/true".into(),
            working_directory: None,
            program_args: Vec::new(),
            limits: Default::default(),
            env: Vec::new(),
            gdb_path: None,
        };
        let mut session = GdbSession::launch(&options).await.unwrap();
        let start = Instant::now();
        kill_process_group(session.child.id().unwrap());
        assert!(session.wait_stopped(Duration::from_secs(2)).await.is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        session.kill().await;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mi_parser::parse_line;

    #[test]
    fn runtime_origin_uses_owned_source_not_user_name_prefixes() {
        let build = crate::config::BuildConfig {
            source_file: "/project/alloc_tracker.c".into(),
            ..Default::default()
        };
        let mut classifier = UserCodeClassifier::from_build(&build);
        classifier.instrumentation_file = Some("/project/session/alloc_tracker.c".into());
        let user = StackFrameInfo {
            func: "cte_user_helper".into(),
            full_file: Some("/project/alloc_tracker.c".into()),
            ..Default::default()
        };
        assert_eq!(classifier.origin(&user), "user_code");
        let runtime = StackFrameInfo {
            func: "cte_write_all".into(),
            full_file: Some("/project/session/alloc_tracker.c".into()),
            ..Default::default()
        };
        assert_eq!(classifier.origin(&runtime), "instrumentation_runtime");
        assert!(!classifier.is_user_file(&runtime));
    }

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
        assert_eq!(
            extract_hex_address("0x7fffffffdaa8").as_deref(),
            Some("0x7fffffffdaa8")
        );
        assert_eq!(
            extract_hex_address("(int *) 0x7ffd1234").as_deref(),
            Some("0x7ffd1234")
        );
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
        let streams = vec![(StreamKind::Console, "type = int *\n".to_string())];
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

#[cfg(test)]
mod scope_status_regressions {
    use super::lexical_scope_status;
    use crate::models::ScopeStatus;

    // The probe emits `block:<start:x>-<end:x>:<decl_line>`; frame addresses
    // are `0x`-prefixed. Block ranges are half-open `[start, end)`.
    #[test]
    fn active_only_inside_the_defining_block_range() {
        let scope = Some("block:4005d7-400823:49");
        assert_eq!(
            lexical_scope_status(scope, Some("0x00000000004005d7")),
            ScopeStatus::Active
        );
        assert_eq!(
            lexical_scope_status(scope, Some("0x0000000000400822")),
            ScopeStatus::Active
        );
        assert_eq!(
            lexical_scope_status(scope, Some("0x0000000000400823")),
            ScopeStatus::Inactive
        );
        assert_eq!(
            lexical_scope_status(scope, Some("0x00000000004005d6")),
            ScopeStatus::Inactive
        );
    }

    #[test]
    fn missing_or_unparsable_evidence_is_unknown_never_guessed() {
        assert_eq!(
            lexical_scope_status(None, Some("0x1")),
            ScopeStatus::Unknown
        );
        assert_eq!(
            lexical_scope_status(Some("block:4005d7-400823:49"), None),
            ScopeStatus::Unknown
        );
        // Storage-based fallback ids and other formats carry no block range.
        assert_eq!(
            lexical_scope_status(Some("activation:1"), Some("0x1")),
            ScopeStatus::Unknown
        );
        assert_eq!(
            lexical_scope_status(Some("block:zz-99:1"), Some("0x1")),
            ScopeStatus::Unknown
        );
        assert_eq!(
            lexical_scope_status(Some("block:4005d7:1"), Some("0x1")),
            ScopeStatus::Unknown
        );
        assert_eq!(
            lexical_scope_status(Some(""), Some("0x1")),
            ScopeStatus::Unknown
        );
        // Frame addresses must parse; GDB error text must not.
        assert_eq!(
            lexical_scope_status(Some("block:10-20:1"), Some("<unavailable>")),
            ScopeStatus::Unknown
        );
    }
}

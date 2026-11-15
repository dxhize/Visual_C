//! Monotonic session budgets and cancellation-safe process ownership.
//! Cleanup may take up to two seconds after a deadline. This is not sandboxing.
use std::future::Future;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

use crate::error::{EngineError, Result};
use crate::models::{StageResult, StageStatus};

/// Linux adopts orphaned grandchildren here so they can be reaped, rather than
/// relying on a container PID 1 to reap them. Never use waitpid(-1): other
/// concurrently owned Tokio children must retain their own wait status.
pub fn enable_subreaper() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) == 0 })
}

pub fn descendants(pid: u32) -> Vec<u32> {
    let mut found = Vec::new();
    let mut pending = vec![pid];
    while let Some(parent) = pending.pop() {
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{parent}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            let Ok(text) = std::fs::read_to_string(task.path().join("children")) else {
                continue;
            };
            for child in text
                .split_whitespace()
                .filter_map(|p| p.parse::<u32>().ok())
            {
                if !found.contains(&child) && child != pid {
                    found.push(child);
                    pending.push(child);
                }
            }
        }
        if found.len() >= 4096 {
            break;
        } // bounded defensive process-tree scan
    }
    found
}

pub async fn reap_descendants(mut pids: Vec<u32>, end: Instant) {
    loop {
        pids.retain(|pid| unsafe {
            let result = libc::waitpid(*pid as i32, std::ptr::null_mut(), libc::WNOHANG);
            result == 0
        });
        if pids.is_empty() || Instant::now() >= end {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

tokio::task_local! { static DEADLINE: Instant; }
tokio::task_local! { static CANCELLATION: std::sync::Arc<Cancellation>; }

struct Cancellation {
    requested: std::sync::atomic::AtomicBool,
    notify: tokio::sync::Notify,
}

struct SignalTask(tokio::task::JoinHandle<()>);
impl Drop for SignalTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub fn cancelled() -> bool {
    CANCELLATION
        .try_with(|token| token.requested.load(std::sync::atomic::Ordering::SeqCst))
        .unwrap_or(false)
}

pub fn request_cancel() {
    let _ = CANCELLATION.try_with(|token| {
        token
            .requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        token.notify.notify_waiters();
    });
}

pub async fn cancellation() {
    let token = CANCELLATION.try_with(std::sync::Arc::clone).ok();
    if let Some(token) = token {
        let notified = token.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if token.requested.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        notified.await;
    } else {
        std::future::pending::<()>().await;
    }
}

pub async fn scope<T>(seconds: u64, future: impl Future<Output = T>) -> T {
    let token = std::sync::Arc::new(Cancellation {
        requested: false.into(),
        notify: tokio::sync::Notify::new(),
    });
    let signal_token = token.clone();
    let mut interrupt =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).ok();
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
    let _signals = SignalTask(tokio::spawn(async move {
        tokio::select! {
            _ = async { if let Some(signal) = &mut interrupt { signal.recv().await; } else { std::future::pending::<()>().await; } } => {},
            _ = async { if let Some(signal) = &mut terminate { signal.recv().await; } else { std::future::pending::<()>().await; } } => {},
        }
        signal_token
            .requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        signal_token.notify.notify_waiters();
    }));
    CANCELLATION
        .scope(
            token,
            DEADLINE.scope(Instant::now() + Duration::from_secs(seconds), future),
        )
        .await
}

pub async fn scope_until<T>(end: Instant, future: impl Future<Output = T>) -> T {
    DEADLINE.scope(end.min(deadline()), future).await
}

pub fn deadline() -> Instant {
    DEADLINE
        .try_with(|d| *d)
        .unwrap_or_else(|_| Instant::now() + Duration::from_secs(300))
}

pub fn remaining() -> Duration {
    deadline().saturating_duration_since(Instant::now())
}

pub fn stage_deadline(seconds: u64) -> Instant {
    Instant::now()
        .checked_add(Duration::from_secs(seconds))
        .map(|end| end.min(deadline()))
        .unwrap_or_else(deadline)
}

pub fn timeout_error(operation: &str) -> EngineError {
    EngineError::Deadline {
        operation: operation.into(),
    }
}

pub struct OwnedChild {
    pub child: Child,
    group: Option<u32>,
}

impl OwnedChild {
    pub fn spawn(cmd: &mut Command) -> std::io::Result<Self> {
        enable_subreaper();
        cmd.process_group(0).kill_on_drop(true);
        let child = cmd.spawn()?;
        let group = child.id();
        Ok(Self { child, group })
    }

    pub async fn terminate(&mut self) {
        let end = Instant::now() + Duration::from_secs(2);
        let descendants = self.group.map(descendants).unwrap_or_default();
        for pid in &descendants {
            crate::resource_limits::kill_process_group(*pid);
        }
        if let Some(pid) = self.group.take() {
            crate::resource_limits::kill_process_group(pid);
        }
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout_at(end.into(), self.child.wait()).await;
        reap_descendants(descendants, end).await;
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(pid) = self.group {
            crate::resource_limits::kill_process_group(pid);
        }
        // Tokio's child reaper handles the cancellation path.
        let _ = self.child.start_kill();
    }
}

pub struct ProcessOutput {
    pub cancelled: bool,
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub output_truncated: bool,
}

async fn drain(
    mut pipe: impl tokio::io::AsyncRead + Unpin,
    cap: usize,
    bytes: &mut Vec<u8>,
    truncated: &mut bool,
) {
    let mut chunk = [0; 8192];
    while let Ok(n) = pipe.read(&mut chunk).await {
        if n == 0 {
            break;
        }
        let keep = n.min(cap.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&chunk[..keep]);
        *truncated |= keep < n;
    }
}

/// Drain both pipes concurrently, including while waiting for the process.
/// No detached reader tasks can survive cancellation or hold pipes indefinitely.
pub async fn output(cmd: &mut Command, end: Instant, cap: usize) -> Result<ProcessOutput> {
    if cancelled() {
        return Err(EngineError::Cancelled);
    }
    if Instant::now() >= end {
        return Err(timeout_error("process launch"));
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut owned = OwnedChild::spawn(cmd)?;
    let out = owned.child.stdout.take().expect("stdout");
    let err = owned.child.stderr.take().expect("stderr");
    let mut out_bytes = Vec::new();
    let mut err_bytes = Vec::new();
    let mut out_truncated = false;
    let mut err_truncated = false;
    let mut stdout = Box::pin(drain(out, cap, &mut out_bytes, &mut out_truncated));
    let mut stderr = Box::pin(drain(err, cap, &mut err_bytes, &mut err_truncated));
    let mut out_done = false;
    let mut err_done = false;
    let mut status = None;
    let mut adopted = Vec::new();
    let mut timed_out = false;
    let mut was_cancelled = false;
    loop {
        if status.is_some() && out_done && err_done {
            break;
        }
        tokio::select! {
            _ = cancellation() => {
                was_cancelled = true;
                owned.terminate().await;
                break;
            }
            _ = &mut stdout, if !out_done => out_done = true,
            _ = &mut stderr, if !err_done => err_done = true,
            result = owned.child.wait(), if status.is_none() => {
                status = Some(result?);
                // Descendants must not outlive their stage or hold its pipes open.
                if let Some(pid) = owned.group.take() {
                    adopted.extend(descendants(std::process::id()).into_iter().filter(|child|
                        unsafe { libc::getpgid(*child as i32) == pid as i32 }));
                    crate::resource_limits::kill_process_group(pid);
                }
            }
            _ = tokio::time::sleep_until(end.into()) => {
                timed_out = true;
                owned.terminate().await;
                break;
            }
        }
    }
    drop(stdout);
    drop(stderr);
    reap_descendants(adopted, Instant::now() + Duration::from_secs(2)).await;
    Ok(ProcessOutput {
        cancelled: was_cancelled,
        status,
        stdout: out_bytes,
        stderr: err_bytes,
        timed_out,
        output_truncated: out_truncated || err_truncated,
    })
}

pub fn stage(
    started: Instant,
    begin: Instant,
    status: StageStatus,
    exit: Option<i32>,
    reason: Option<String>,
    complete: bool,
    partial: bool,
) -> StageResult {
    StageResult {
        status: Some(status),
        start_offset_ms: Some(begin.duration_since(started).as_millis() as u64),
        end_offset_ms: Some(started.elapsed().as_millis() as u64),
        duration_ms: Some(begin.elapsed().as_millis() as u64),
        exit_code: exit,
        reason,
        output_complete: Some(complete),
        partial_results_available: Some(partial),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_wakes_stalled_process_wait_and_retains_partial_output() {
        let start = Instant::now();
        let out = scope(30, async {
            let mut command = Command::new("sh");
            command.args(["-c", "echo observed; sleep 20"]);
            let (out, ()) = tokio::join!(
                output(&mut command, Instant::now() + Duration::from_secs(20), 1024),
                async {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    request_cancel();
                }
            );
            out.unwrap()
        })
        .await;
        assert!(out.cancelled);
        assert!(!out.timed_out);
        assert!(String::from_utf8_lossy(&out.stdout).contains("observed"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[tokio::test]
    async fn stalled_process_and_inherited_pipes_are_bounded() {
        let start = Instant::now();
        let out = output(
            Command::new("sh").args(["-c", "sleep 20 & wait"]),
            start + Duration::from_millis(100),
            1024,
        )
        .await
        .unwrap();
        assert!(out.timed_out);
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn timeout_retains_output_already_read() {
        let out = output(
            Command::new("sh").args(["-c", "echo observed; sleep 20"]),
            Instant::now() + Duration::from_millis(150),
            1024,
        )
        .await
        .unwrap();
        assert!(out.timed_out);
        assert!(String::from_utf8_lossy(&out.stdout).contains("observed"));
    }

    #[tokio::test]
    async fn successful_parent_cannot_leave_pipe_holding_grandchild_or_zombie() {
        let out = output(
            Command::new("sh").args(["-c", "sleep 20 & echo $!"]),
            Instant::now() + Duration::from_secs(2),
            1024,
        )
        .await
        .unwrap();
        assert!(!out.timed_out);
        let pid: u32 = String::from_utf8(out.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}

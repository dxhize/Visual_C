//! Execution resource limits and their Linux enforcement.
//!
//! Two enforcement layers are provided, both applied to the *inferior* side of
//! the process tree rather than to the engine itself:
//!
//! 1. Diagnostic children receive CPU/file/open-file rlimits. GDB is not
//!    restricted by the inferior's address-space budget; Linux prlimit applies
//!    that budget to the stopped inferior after the initial main breakpoint.
//! 2. **cgroup v2** (best effort): when the kernel exposes a writable cgroup
//!    subtree (root, or a delegated user slice), a dedicated group is created
//!    with `memory.max` and `pids.max`; the stopped inferior is attached
//!    to it so its future children are accounted and can be
//!    killed atomically via `cgroup.kill`.
//!
//! # Security note
//!
//! Resource limits are **not** a security sandbox. rlimits apply only to the
//! traced process tree, but the inferior still runs with the engine user's
//! privileges and full filesystem access. Namespaces, filesystem restriction
//! and syscall filtering belong to a later phase.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Result};

/// Hard cap for the inferior's file writes, preventing disk-filling accidents.
/// Not user-configurable in this phase; documented as a fixed policy.
pub const MAX_INFERIOR_FILE_SIZE_BYTES: u64 = 64 * 1024 * 1024;

/// Hard cap for simultaneously open file descriptors in the inferior.
pub const MAX_INFERIOR_OPEN_FILES: u64 = 256;

/// NOTE on process-count limits: Linux `RLIMIT_NPROC` is enforced **per
/// real user id**, not per process tree — lowering it here would make every
/// `fork`/`vfork` fail for the whole desktop session, including GDB's own
/// spawning of the inferior. Child-process budgets are therefore enforced
/// only through cgroup v2 `pids.max` when a delegated subtree is available
/// (see `CgroupGuard`), and documented as best-effort.
/// Configurable resource limits for a tracing session.
///
/// Defaults follow the engine's initial policy table (see README):
/// 1,000 snapshots, 10 s wall timeout, 256 MB address space, 1 MB captured
/// output, 10,000 tracked allocations, 1 MB memory captured per snapshot,
/// 4 KB maximum individual memory read, 16 child processes.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct ExecutionLimits {
    /// Maximum number of single-line execution steps issued to GDB.
    pub max_steps: usize,
    /// Wall-clock timeout for the traced execution, in seconds.
    pub timeout_seconds: u64,
    /// Address-space limit for the inferior process tree, in MiB.
    pub max_memory_mb: usize,
    /// Maximum bytes of inferior stdout+stderr captured (merged via pty).
    pub max_output_bytes: usize,
    /// Maximum number of allocation records reconstructed from events.
    pub max_heap_allocations: usize,
    /// Maximum number of execution snapshots captured.
    pub max_snapshot_count: usize,
    /// Maximum raw memory bytes captured per snapshot (heap + variables).
    pub max_snapshot_memory_bytes: usize,
    /// Maximum bytes for a single memory read.
    pub max_memory_read_bytes: usize,
    /// Maximum number of child processes the inferior may spawn.
    pub max_process_count: usize,
    /// Maximum variables introspected per snapshot (address/size/bytes).
    pub max_variables_per_snapshot: usize,
    pub max_instruction_events: usize,
    pub max_source_checkpoints: usize,
    pub max_trace_bytes: usize,
    pub max_trace_memory_bytes: usize,
    pub max_register_snapshot_bytes: usize,
    pub max_gdb_commands: u64,
    pub max_globals: usize,
    pub compiler_timeout_seconds: u64,
    pub debugger_timeout_seconds: u64,
    pub analysis_timeout_seconds: u64,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_steps: 1_000,
            timeout_seconds: 10,
            max_memory_mb: 256,
            max_output_bytes: 1_024 * 1024,
            max_heap_allocations: 10_000,
            max_snapshot_count: 1_000,
            max_snapshot_memory_bytes: 1024 * 1024,
            max_memory_read_bytes: 4 * 1024,
            max_process_count: 16,
            max_variables_per_snapshot: 24,
            max_instruction_events: 100_000,
            max_source_checkpoints: 1_000,
            max_trace_bytes: 128 * 1024 * 1024,
            max_trace_memory_bytes: 32 * 1024 * 1024,
            max_register_snapshot_bytes: 64 * 1024,
            max_gdb_commands: 1_000_000,
            max_globals: 32,
            compiler_timeout_seconds: 30,
            debugger_timeout_seconds: 30,
            analysis_timeout_seconds: 10,
        }
    }
}

impl ExecutionLimits {
    /// Sanity-check configured limits; rejects values that cannot work.
    pub fn validate(&self) -> Result<()> {
        if self.timeout_seconds > 31_536_000 {
            return Err(EngineError::Config(
                "timeout_seconds must not exceed one year".into(),
            ));
        }
        if self.max_memory_mb as u64 > u64::MAX / (1024 * 1024) {
            return Err(EngineError::Config(
                "max_memory_mb overflows byte accounting".into(),
            ));
        }
        if self.max_gdb_commands == 0
            || self.max_trace_bytes == 0
            || self.max_trace_memory_bytes == 0
        {
            return Err(EngineError::Config(
                "debugger command and trace storage budgets must be positive".into(),
            ));
        }
        if self.max_steps == 0 {
            return Err(EngineError::Config("max_steps must be > 0".into()));
        }
        if self.timeout_seconds == 0 {
            return Err(EngineError::Config("timeout_seconds must be > 0".into()));
        }
        if self.max_memory_mb < 16 {
            // Below this the traced program cannot even link its runtime
            // image (libc + program + stacks); the engine refuses the
            // configuration rather than producing a broken run.
            return Err(EngineError::Config(
                "max_memory_mb must be >= 16 for the traced program".into(),
            ));
        }
        if self.max_snapshot_count == 0 {
            return Err(EngineError::Config("max_snapshot_count must be > 0".into()));
        }
        if self.max_memory_read_bytes == 0 {
            return Err(EngineError::Config(
                "max_memory_read_bytes must be > 0".into(),
            ));
        }
        Ok(())
    }
}

/// Apply the full inferior resource budget to a spawned child via a
/// `pre_exec` hook; limits are inherited by every descendant. This is the
/// budget a *traced program* should run under. Tools with large virtual
/// address-space needs (GDB, Valgrind, ASan-instrumented binaries) must use
/// [`apply_diagnostic_limits`] instead: `RLIMIT_AS` breaks ASan's shadow
/// memory reservation and starves GDB's symbol loading.
pub fn apply_pre_exec_limits(cmd: &mut tokio::process::Command, limits: &ExecutionLimits) {
    let address_space = limits.max_memory_mb as u64 * 1024 * 1024;
    // CPU seconds: at least 1, derived from the wall timeout. The kernel sends
    // SIGXCPU (then SIGKILL) which the engine maps to a resource-limit result.
    let cpu_seconds = limits.timeout_seconds.max(1);
    let fsize = MAX_INFERIOR_FILE_SIZE_BYTES;
    let nofile = MAX_INFERIOR_OPEN_FILES;
    unsafe {
        cmd.pre_exec(move || {
            set_rlimit(libc::RLIMIT_AS, address_space);
            set_rlimit(libc::RLIMIT_CPU, cpu_seconds);
            set_rlimit(libc::RLIMIT_FSIZE, fsize);
            set_rlimit(libc::RLIMIT_NOFILE, nofile);
            set_rlimit(libc::RLIMIT_CORE, 0);
            // RLIMIT_NPROC is deliberately NOT set: on Linux it applies to
            // the whole real user, not the inferior tree (see module docs).
            Ok(())
        });
    }
}

/// Apply the tool-safe subset of the budget to a spawned child: CPU, max
/// file size and open files — but no address-space cap, because GDB,
/// Valgrind and sanitizer runtimes reserve many terabytes of *virtual*
/// address space for their own bookkeeping and abort immediately under a
/// small `RLIMIT_AS` (errno 12 on `ReserveShadowMemoryRange`).
///
/// Runaway child processes are still bounded: by the CPU limit
/// (`SIGXCPU` → resource-limit result) and by the engine's own wall
/// deadlines, which kill the whole process group.
pub fn apply_diagnostic_limits(cmd: &mut tokio::process::Command, limits: &ExecutionLimits) {
    let cpu_seconds = limits.timeout_seconds.max(1);
    let fsize = MAX_INFERIOR_FILE_SIZE_BYTES;
    let nofile = MAX_INFERIOR_OPEN_FILES;
    unsafe {
        cmd.pre_exec(move || {
            set_rlimit(libc::RLIMIT_CPU, cpu_seconds);
            set_rlimit(libc::RLIMIT_FSIZE, fsize);
            set_rlimit(libc::RLIMIT_NOFILE, nofile);
            set_rlimit(libc::RLIMIT_CORE, 0);
            Ok(())
        });
    }
}

/// Apply only to the stopped inferior, never to GDB or ASan. Startup before
/// main is outside this limit unless an external cgroup already contains it.
pub fn limit_stopped_inferior(pid: u32, limits: &ExecutionLimits) -> std::io::Result<()> {
    for (resource, value) in [
        (
            libc::RLIMIT_AS,
            (limits.max_memory_mb as u64).saturating_mul(1024 * 1024),
        ),
        (libc::RLIMIT_CPU, limits.timeout_seconds.max(1)),
        (libc::RLIMIT_FSIZE, MAX_INFERIOR_FILE_SIZE_BYTES),
        (libc::RLIMIT_CORE, 0),
    ] {
        let limit = libc::rlimit {
            rlim_cur: value,
            rlim_max: if resource == libc::RLIMIT_CPU {
                value.saturating_add(1)
            } else {
                value
            },
        };
        if unsafe { libc::prlimit(pid as i32, resource, &limit, std::ptr::null_mut()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

unsafe fn set_rlimit(resource: u32, value: u64) {
    let lim = libc::rlimit {
        rlim_cur: value as libc::rlim_t,
        rlim_max: if resource == libc::RLIMIT_CPU {
            value.saturating_add(1)
        } else {
            value
        } as libc::rlim_t,
    };
    // Failures are ignored deliberately: a restricted environment may refuse
    // individual limits; the remaining layers (wall timeout, counters) still
    // bound the run.
    let _ = libc::setrlimit(resource, &lim);
}

/// Put the child into its own process group so that the whole inferior tree
/// (GDB, inferior, grandchildren) can be terminated with one signal.
pub fn set_process_group(cmd: &mut tokio::process::Command) {
    cmd.process_group(0);
}

/// Kill a process group (and, defensively, the leader) with SIGKILL.
pub fn kill_process_group(child_pid: u32) {
    let pid = child_pid as i32;
    unsafe {
        // Ignore ESRCH: the group may already be gone.
        let _ = libc::kill(-pid, libc::SIGKILL);
        let _ = libc::kill(pid, libc::SIGKILL);
    }
}

/// Best-effort cgroup v2 enforcement. Returns `None` when cgroup v2 is not
/// available or the engine is not permitted to create a subgroup.
pub struct CgroupGuard {
    path: PathBuf,
}

impl CgroupGuard {
    /// Try to create a dedicated cgroup for this session.
    pub fn create(limits: &ExecutionLimits) -> Option<CgroupGuard> {
        let root = Path::new("/sys/fs/cgroup");
        let current = fs::read_to_string("/proc/self/cgroup")
            .ok()
            .and_then(|text| {
                text.lines().find_map(|line| {
                    line.strip_prefix("0::")
                        .map(|p| root.join(p.trim_start_matches('/')))
                })
            });
        let base = current
            .as_deref()
            .filter(|p| p.join("cgroup.controllers").exists())
            .unwrap_or(root);
        if !base.join("cgroup.controllers").exists() {
            return None; // not cgroup v2 unified hierarchy
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        let path = base.join(format!("c-trace-{}-{stamp}", std::process::id()));
        if fs::create_dir(&path).is_err() {
            return None; // no delegation / not permitted
        }
        if fs::write(
            path.join("memory.max"),
            (limits.max_memory_mb as u64 * 1024 * 1024).to_string(),
        )
        .is_err()
        {
            let _ = fs::remove_dir(&path);
            return None;
        }
        if fs::write(
            path.join("pids.max"),
            limits.max_process_count.max(1).to_string(),
        )
        .is_err()
        {
            let _ = fs::remove_dir(&path);
            return None;
        }
        Some(CgroupGuard { path })
    }

    /// Attach a pid (and therefore all of its descendants) to the group.
    pub fn attach(&self, pid: u32) -> bool {
        fs::write(self.path.join("cgroup.procs"), pid.to_string()).is_ok()
    }

    /// Kill every process in the group atomically (kernel >= 5.14).
    pub fn kill_all(&self) {
        let _ = fs::write(self.path.join("cgroup.kill"), "1");
    }
}

impl Drop for CgroupGuard {
    fn drop(&mut self) {
        // Best effort cleanup; the directory may still contain processes if
        // kill_all was never called.
        let _ = fs::remove_dir(&self.path);
    }
}

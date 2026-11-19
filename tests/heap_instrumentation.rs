//! Independent raw transport oracles for the C allocation wrapper.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use c_trace_engine::allocation_tracker::{verify_event_status, EventFileReader, HeapTimeline};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct CaseDir(PathBuf);

impl CaseDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cte-phase8r2-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for CaseDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn compile_with_flags(fixture: &str, dir: &Path, flags: &[&str]) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary = dir.join("fixture");
    let output = Command::new("gcc")
        .args(["-O0", "-pthread", "-DCTE_TESTING"])
        .args(flags)
        .arg(root.join("fixtures/c").join(fixture))
        .arg(root.join("c_runtime/alloc_tracker.c"))
        .args([
            "-Wl,--wrap=malloc",
            "-Wl,--wrap=calloc",
            "-Wl,--wrap=realloc",
            "-Wl,--wrap=free",
            "-o",
        ])
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "gcc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    binary
}

fn compile(fixture: &str, dir: &Path) -> PathBuf {
    compile_with_flags(fixture, dir, &[])
}

fn run(binary: &Path, dir: &Path, mode: &str) -> Output {
    let mut command = Command::new(binary);
    command
        .arg(mode)
        .env("C_TRACE_EVENT_FILE", dir.join("events"))
        .env("C_TRACE_EVENT_STATUS_FILE", dir.join("status"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success(), "{mode}: {output:?}");
            return output;
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            unsafe {
                // Kill the fixture's process group, including a child stuck
                // after fork, then reap the direct child below.
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!("{mode}: fixture timed out (post-fork allocator deadlock)");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn oracle(dir: &Path, counts: &[usize], failures: &[usize]) {
    let events = std::fs::read_to_string(dir.join("events")).unwrap();
    let status = std::fs::read_to_string(dir.join("status")).unwrap();
    let mut by_pid: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
    for line in events.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        assert!(
            parts.len() == 10 || parts.len() == 12,
            "truncated event: {line:?}"
        );
        let pid = parts[0].parse().unwrap();
        let seq = parts[1].parse().unwrap();
        let id = parts[3].parse().unwrap();
        by_pid.entry(pid).or_default().push((seq, id));
    }
    let mut acks = BTreeMap::new();
    for line in status.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        assert_eq!(fields.len(), 3, "invalid ACK: {line:?}");
        let pid: u32 = fields[0].parse().unwrap();
        let seq: usize = fields[1].parse().unwrap();
        let failure: usize = fields[2].parse().unwrap();
        assert!(
            acks.insert(pid, (seq, failure)).is_none(),
            "duplicate ACK for {pid}"
        );
    }
    assert_eq!(
        acks.len(),
        counts.len(),
        "status={status:?}, events={events:?}"
    );
    assert_eq!(
        by_pid.keys().collect::<BTreeSet<_>>(),
        acks.keys().collect()
    );
    let mut actual_counts = Vec::new();
    let mut actual_failures = Vec::new();
    for (pid, (last, failed)) in acks {
        let rows = &by_pid[&pid];
        for (index, &(seq, _)) in rows.iter().enumerate() {
            assert_eq!(seq, index + 1, "noncontiguous seq for {pid}: {rows:?}");
        }
        assert!(rows.iter().any(|&(seq, id)| seq == 1 && id == 1));
        assert_eq!(last, rows.len(), "missing event for {pid}");
        actual_counts.push(rows.len());
        actual_failures.push(failed);
    }
    actual_counts.sort();
    actual_failures.sort();
    assert_eq!(actual_counts, counts);
    assert_eq!(actual_failures, failures);
}

#[test]
fn normal_event_log_has_terminal_ack() {
    let dir = CaseDir::new();
    let binary = compile("phase8r2_basic.c", &dir.0);
    run(&binary, &dir.0, "normal");
    oracle(&dir.0, &[2], &[0]);
}

#[test]
fn independent_raw_oracle_distinguishes_silent_and_unacknowledged_forks() {
    for (mode, pid_counts, ack_counts) in [
        ("parent", vec![4], 1),
        ("observed", vec![2, 4], 2),
        ("repeated", vec![2, 2, 2, 4], 4),
        ("silent", vec![4], 1),
        ("incomplete", vec![2, 4], 1),
    ] {
        let dir = CaseDir::new();
        let binary = compile("phase8dr_fork.c", &dir.0);
        run(&binary, &dir.0, mode);
        let rows = std::fs::read_to_string(dir.0.join("events")).unwrap();
        let acks = std::fs::read_to_string(dir.0.join("status")).unwrap();
        let mut per_pid = BTreeMap::<u32, Vec<usize>>::new();
        for line in rows.lines() {
            let parts: Vec<_> = line.split_whitespace().collect();
            per_pid
                .entry(parts[0].parse().unwrap())
                .or_default()
                .push(parts[1].parse().unwrap());
        }
        let mut sizes: Vec<_> = per_pid.values().map(Vec::len).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, pid_counts, "{mode}");
        assert_eq!(acks.lines().count(), ack_counts, "{mode}");
        for sequences in per_pid.values() {
            assert_eq!(
                sequences,
                &(1..=sequences.len()).collect::<Vec<_>>(),
                "{mode}"
            );
        }
        if mode == "incomplete" {
            let child = *per_pid.iter().find(|(_, seq)| seq.len() == 2).unwrap().0;
            assert!(!acks
                .lines()
                .any(|line| line.starts_with(&format!("{child} "))));
        }
    }
}

#[test]
fn repeated_forks_restart_child_sequence_and_ids() {
    let dir = CaseDir::new();
    let binary = compile("phase8r2_basic.c", &dir.0);
    run(&binary, &dir.0, "forks");
    oracle(&dir.0, &[2, 2, 2, 4], &[0, 0, 0, 0]);
}

#[test]
fn multithread_allocations_are_serialized() {
    let dir = CaseDir::new();
    let binary = compile("phase8r2_basic.c", &dir.0);
    run(&binary, &dir.0, "threads");
    oracle(&dir.0, &[160], &[0]);
}

#[test]
fn fork_while_worker_holds_tracker_lock_cannot_deadlock_child() {
    let dir = CaseDir::new();
    let binary = compile("phase8r2_stress.c", &dir.0);
    run(&binary, &dir.0, "lock_fork");
    oracle(&dir.0, &[2, 4], &[0, 0]);
}

#[test]
fn audited_pre_repair_fork_behavior_is_reproduced_by_test_oracle() {
    let dir = CaseDir::new();
    let binary = compile_with_flags(
        "phase8r2_stress.c",
        &dir.0,
        &["-DCTE_TEST_DISABLE_FORK_RESET"],
    );
    let started = Instant::now();
    let output = Command::new(binary)
        .arg("lock_fork")
        .env("C_TRACE_EVENT_FILE", dir.0.join("events"))
        .env("C_TRACE_EVENT_STATUS_FILE", dir.0.join("status"))
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "pre-repair oracle unexpectedly passed"
    );
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "fixture watchdog did not bound the reproduced child deadlock"
    );
}

#[test]
fn partial_and_eintr_writes_recover_without_changing_errno() {
    for mode in ["partial", "eintr"] {
        let dir = CaseDir::new();
        let binary = compile("phase8r2_stress.c", &dir.0);
        run(&binary, &dir.0, mode);
        oracle(&dir.0, &[2], &[0]);
    }
}

#[test]
fn terminal_write_errors_are_reported_by_independent_ack() {
    for mode in ["error", "zero", "eintr_exhausted", "partial_error"] {
        let dir = CaseDir::new();
        let binary = compile("phase8r2_stress.c", &dir.0);
        run(&binary, &dir.0, mode);
        let status = std::fs::read_to_string(dir.0.join("status")).unwrap();
        let ack: Vec<_> = status.split_whitespace().collect();
        assert_eq!(ack.len(), 3, "{mode}: {status:?}");
        assert_eq!(ack[1..], ["2", "1"], "{mode}: {status:?}");
        let events = std::fs::read_to_string(dir.0.join("events")).unwrap();
        let lines: Vec<_> = events.lines().collect();
        assert_eq!(lines.len(), if mode == "partial_error" { 2 } else { 1 });
        assert!(lines[0].contains(" malloc "));
        if mode == "partial_error" {
            assert!(
                !events.ends_with('\n'),
                "partial line must not masquerade as a full event"
            );
            assert!(lines[1].starts_with(ack[0]));
        }

        // Feed the actual C-written files through the production Rust reader
        // and timeline, not a synthetic event string. A terminal failure must
        // invalidate coverage even if the last complete event has no seq gap.
        let mut reader = EventFileReader::new(dir.0.join("events"));
        let mut timeline = HeapTimeline::new(100);
        for event in reader.read_new() {
            timeline.ingest(&event);
        }
        if mode != "partial_error" {
            // The audited sequence-only check accepted this prefix as
            // complete: the missing terminal event has no successor to
            // expose a sequence gap.
            assert!(timeline.history_complete(), "{mode}: baseline oracle");
        }
        let pid: u32 = ack[0].parse().unwrap();
        let errors = verify_event_status(&dir.0.join("status"), &timeline, Some(pid));
        assert!(!errors.is_empty(), "{mode}: missing failure detection");
        assert!(errors.iter().any(|error| error.contains("writer failure")));
        timeline.mark_transport_incomplete();
        assert!(!timeline.history_complete());
        assert_eq!(timeline.events().len(), 1, "{mode}: verified prefix lost");
    }
}

#[test]
fn status_ack_failure_and_abnormal_exit_leave_no_valid_ack() {
    for mode in ["ack_failure", "abnormal"] {
        let dir = CaseDir::new();
        let binary = compile("phase8r2_stress.c", &dir.0);
        run(&binary, &dir.0, mode);
        let status = std::fs::read_to_string(dir.0.join("status")).unwrap();
        assert!(status.is_empty(), "{mode}: unexpected ACK {status:?}");
    }
}

#[test]
fn status_open_failure_does_not_write_ack_to_event_log() {
    let dir = CaseDir::new();
    let binary = compile("phase8r2_basic.c", &dir.0);
    let output = Command::new(binary)
        .arg("normal")
        .env("C_TRACE_EVENT_FILE", dir.0.join("events"))
        .env("C_TRACE_EVENT_STATUS_FILE", &dir.0)
        .output()
        .unwrap();
    assert!(output.status.success());
    let events = std::fs::read_to_string(dir.0.join("events")).unwrap();
    assert_eq!(events.lines().count(), 2);
    assert!(events
        .lines()
        .all(|line| line.split_whitespace().count() == 10));
}

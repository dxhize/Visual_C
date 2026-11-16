#!/usr/bin/env python3
"""Repeatable local benchmark. Build first: cargo build; run this from the repo root.
GNU time peak RSS includes the largest waited-for child (often GDB), not just Rust.
"""
import json
import hashlib
import pathlib
import platform
import statistics
import subprocess
import tempfile

CASES = [
    ("hello_source", "fixtures/c/hello.c", ["--trace-mode", "source"]),
    ("hello_dual", "fixtures/c/hello.c", []),
    ("loop_dual", "fixtures/c/finite_loop.c", []),
    # V2.1: register capture was removed from the engine; the former
    # "loop_no_registers" case (which measured `--no-registers`) is no longer
    # meaningful. The journal-only and tight-loop cases remain.
    ("loop_journal_only", "fixtures/c/finite_loop.c", ["--journal-only"]),
    ("tight_loop_journal_only", "fixtures/c/spin_same_line.c",
     ["--journal-only", "--max-instruction-events", "2000", "--analysis-timeout", "1"]),
]

results = []
binary_sha256 = hashlib.sha256(pathlib.Path("target/debug/c-trace-engine").read_bytes()).hexdigest()
tools = {}
with tempfile.TemporaryDirectory(prefix="cte-benchmark-") as directory:
    for name, fixture, flags in CASES:
        samples = []
        for repetition in range(3):
            trace = pathlib.Path(directory) / f"{name}-{repetition}.json"
            timing = trace.with_suffix(".time")
            journal = trace.with_suffix(".jsonl")
            args = ["target/debug/c-trace-engine", fixture, "--timeout", "30",
                    "--max-steps", "10000", "--output", str(trace), *flags]
            if "--journal-only" in flags:
                args.extend(["--journal", str(journal)])
            subprocess.run(["/usr/bin/time", "-f", "%e %M", "-o", str(timing), *args],
                           stdout=subprocess.DEVNULL, check=True)
            wall, peak = timing.read_text().split()
            data = json.loads(trace.read_text())
            tools = {k: data["session_meta"].get(k) for k in
                     ("gcc_version", "gdb_version", "valgrind_version", "addr2line_version")}
            profile = data["profiling"]
            debugger_seconds = profile["debugger_time_us"] / 1e6
            samples.append({
                "wall_seconds": float(wall),
                "peak_waited_process_rss_kib": int(peak),
                "engine_peak_rss_kib": profile["engine_peak_rss_kib"],
                "json_bytes": trace.stat().st_size,
                "journal_bytes": journal.stat().st_size if journal.exists() else None,
                "instruction_attempts": data["instruction_count"],
                "events_per_debugger_second": round(data["instruction_count"] / debugger_seconds, 2),
                "outcome": data["execution_status"],
                "profiling": profile,
            })
        results.append({"case": name, "fixture": fixture, "flags": flags, "samples": samples,
                        "median_wall_seconds": statistics.median(s["wall_seconds"] for s in samples)})

print(json.dumps({"method": "debug build; three serial samples; all pipeline stages included",
                  "platform": platform.platform(), "binary_sha256": binary_sha256,
                  "tools": tools, "results": results}, indent=2))

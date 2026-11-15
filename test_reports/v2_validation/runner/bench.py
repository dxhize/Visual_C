#!/usr/bin/env python3
"""Suite F benchmarks: 6 workloads x {native, source, dual} x 3 samples.

- Native timing: externally measured with /usr/bin/time (wall + max RSS).
- Engine timing: external wall + engine-reported resource_usage/profiling.
- Medians over 3 samples. Bounded: every engine call gets the 150s watchdog.
"""
import json, os, statistics, subprocess, time

ROOT = "/home/xe/Documents/visual_c"
BIN = ROOT + "/target/debug/c-trace-engine"
FIX = ROOT + "/test_reports/v2_validation/fixtures"
EV = ROOT + "/test_reports/v2_validation/evidence/benchmarks"
os.makedirs(EV, exist_ok=True)

WORKLOADS = ["bench_arith", "bench_loop", "bench_nested", "bench_recurse",
             "bench_heap", "bench_calls"]
SAMPLES = 3

def timed_run(cmd, timeout=150):
    cmd = ["/usr/bin/time", "-f", "TIME_RSS %e %M"] + cmd
    t0 = time.perf_counter()
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, timeout=timeout)
    wall = time.perf_counter() - t0
    rss = None
    for line in reversed(p.stderr.decode(errors="replace").strip().splitlines()):
        if line.startswith("TIME_RSS"):
            rss = float(line.split()[-1])
            break
    return wall, rss, p

results = {}
for w in WORKLOADS:
    src = f"{FIX}/{w}.c"
    exe = f"{FIX}/{w}"
    subprocess.run(["gcc", "-g", "-O0", "-o", exe, src], check=True, capture_output=True)
    runs = {"native": [], "source": [], "dual": []}
    for mode in ("native", "source", "dual"):
        for i in range(SAMPLES):
            cmd = [exe] if mode == "native" else \
                [BIN, src, "--trace-mode", mode, "--timeout", "120"]
            wall, rss, p = timed_run(cmd)
            rec = {"wall_s": round(wall, 3), "max_rss_kib": rss, "exit": p.returncode}
            if mode != "native":
                try:
                    tr = json.loads(p.stdout)
                    ru = tr.get("resource_usage") or {}
                    prof = tr.get("profiling") or {}
                    st = tr.get("storage") or {}
                    rec.update({
                        "status": tr.get("execution_status"),
                        "instr_events": tr.get("instruction_count"),
                        "snapshots": len(tr.get("snapshots") or []),
                        "gdb_commands": ru.get("gdb_commands_issued"),
                        "trace_bytes": len(p.stdout),
                        "engine_wall_ms": ru.get("wall_time_ms"),
                        "register_time_us": prof.get("register_time_us"),
                        "disassembly_time_us": prof.get("disassembly_time_us"),
                        "disassembly_queries": prof.get("disassembly_queries"),
                        "disassembly_cache_hits": prof.get("disassembly_cache_hits"),
                        "command_time_us": prof.get("command_time_us"),
                        "step_time_us": prof.get("step_time_us"),
                        "capture_time_us": prof.get("capture_time_us"),
                        "memory_read_time_us": prof.get("memory_read_time_us"),
                        "source_resolution_time_us": prof.get("source_resolution_time_us"),
                        "mi_parse_time_us": prof.get("mi_parse_time_us"),
                        "engine_peak_rss_kib": prof.get("engine_peak_rss_kib"),
                        "storage_bytes": st.get("bytes_written"),
                    })
                    open(f"{EV}/{w}_{mode}_{i}.json", "wb").write(p.stdout)
                except Exception as ex:
                    rec["parse_error"] = str(ex)
            runs[mode].append(rec)
    results[w] = runs
    med = {m: statistics.median([r["wall_s"] for r in runs[m]]) for m in runs}
    ev_n = runs["dual"][1].get("instr_events") if runs["dual"] else None
    tb = runs["dual"][1].get("trace_bytes") if runs["dual"] else None
    print(f"{w:13s} native={med['native']:.2f}s source={med['source']:.2f}s "
          f"dual={med['dual']:.2f}s src_ovh={med['source']/max(med['native'],1e-9):6.1f}x "
          f"dual_ovh={med['dual']/max(med['native'],1e-9):6.1f}x events={ev_n} trace={tb}B")

json.dump(results, open(f"{EV}/raw_runs.json", "w"), indent=1)
print("wrote", f"{EV}/raw_runs.json")

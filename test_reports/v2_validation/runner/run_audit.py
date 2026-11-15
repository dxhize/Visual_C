#!/usr/bin/env python3
"""Audit runner: executes the engine against audit fixtures (suites A-D),
captures evidence per test, runs invariant + semantic validators,
verifies process cleanup, and writes results JSON.

Engine source is NOT modified; this is a black-box driver.
"""
import json, os, subprocess, sys, time, shutil, glob
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from validate_trace import validate_trace, semantic_checks

ROOT = "/home/xe/Documents/visual_c"
BIN = os.path.join(ROOT, "target/debug/c-trace-engine")
FIX = os.path.join(ROOT, "test_reports/v2_validation/fixtures")
EV = os.path.join(ROOT, "test_reports/v2_validation/evidence")
RESULTS = os.path.join(ROOT, "test_reports/v2_validation/test_results.json")

os.makedirs(EV, exist_ok=True)
ENGINE_RC = {0: "ok", 2: "completed-with-findings", 1: "engine-internal-error"}


def snapshot_processes():
    """List (pid,args) of gdb/inferior processes currently alive."""
    out = []
    try:
        ps = subprocess.run(["ps", "-eo", "pid,args"], capture_output=True, text=True).stdout
        for line in ps.splitlines()[1:]:
            if ("program_debug" in line or "gdb --nx" in line or "valgrind --tool" in line) and "audit.py" not in line and "ps -eo" not in line:
                pid, _, args = line.strip().partition(" ")
                out.append((pid, args[:120]))
    except Exception:
        pass
    return out


def run_engine(test_id, args, fixture, expect_status, dual, timeout=150, journal=None):
    ev_dir = os.path.join(EV, test_id)
    shutil.rmtree(ev_dir, ignore_errors=True)
    os.makedirs(ev_dir)
    if journal and os.path.exists(journal):
        os.remove(journal)
    cmd = [BIN] + args
    if journal:
        cmd += ["--journal", journal]
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=ROOT, capture_output=True, timeout=timeout)
        wall = time.time() - t0
        rc, outb, errb = p.returncode, p.stdout, p.stderr
        timed_out = False
    except subprocess.TimeoutExpired:
        wall = time.time() - t0
        rc, outb, errb, timed_out = -9, b"", b"WATCHDOG TIMEOUT", True
    open(os.path.join(ev_dir, "engine_stderr.log"), "wb").write(errb)
    open(os.path.join(ev_dir, "command.txt"), "w").write(" ".join(cmd) + "\n")
    trace = None
    parse_err = ""
    try:
        trace = json.loads(outb)
        open(os.path.join(ev_dir, "trace.json"), "wb").write(outb)
    except Exception as ex:
        parse_err = str(ex)
        open(os.path.join(ev_dir, "raw_stdout.txt"), "wb").write(outb)
    # cleanup verification
    time.sleep(0.4)
    survivors = snapshot_processes()
    open(os.path.join(ev_dir, "survivors.json"), "w").write(json.dumps(survivors, indent=1))
    checks = []
    if trace is None:
        checks.append(("trace_json_parsed", "FAIL", parse_err[:200] or "no JSON on stdout"))
        if timed_out:
            checks.append(("watchdog", "FAIL", f"engine exceeded {timeout}s watchdog"))
    else:
        checks.append(("trace_json_parsed", "PASS", ""))
        jctx = {"dual": dual, "journal_path": journal, "expect_status": expect_status,
                "fixture": fixture}
        checks += [(f"E:{c['invariant']}", c["status"], c["detail"])
                   for c in validate_trace(trace, jctx)]
        checks += semantic_checks(trace, test_id, fixture)
        # process cleanup independent of engine claims
        checks.append(("cleanup_no_survivors", "PASS" if not survivors else "FAIL",
                       json.dumps(survivors) if survivors else "no gdb/inferior/valgrind processes remain"))
        if journal:
            checks.append(("E20_journal", "PASS" if os.path.isfile(journal) else "FAIL", journal))
    status = "FAIL" if any(c[1] == "FAIL" for c in checks) else \
        ("BLOCKED" if any(c[1] == "BLOCKED" for c in checks) else "PASS")
    if timed_out:
        status = "FAIL"
    result = {
        "id": test_id, "fixture": fixture, "command": " ".join(cmd),
        "expected": expect_status, "engine_exit": rc,
        "wall_seconds": round(wall, 2),
        "status": status, "checks": checks,
        "summary": {
            "execution_status": (trace or {}).get("execution_status"),
            "outcome": ((trace or {}).get("execution") or {}).get("outcome"),
            "snapshots": len((trace or {}).get("snapshots") or []),
            "instruction_count": (trace or {}).get("instruction_count", 0),
            "gdb_commands": ((trace or {}).get("resource_usage") or {}).get("gdb_commands_issued"),
            "wall_ms": ((trace or {}).get("resource_usage") or {}).get("wall_time_ms"),
            "limits": ((trace or {}).get("resource_usage") or {}).get("limits_triggered"),
            "trace_bytes": os.path.getsize(os.path.join(ev_dir, "trace.json")) if os.path.isfile(os.path.join(ev_dir, "trace.json")) else 0,
        },
        "evidence_dir": os.path.relpath(ev_dir, ROOT),
    }
    return result


def fixtures(*names):
    return [os.path.join(FIX, n) for n in names]


TESTS = [
    # ---- Suite A ----
    ("A1", fixtures("audit_a1_vars.c"), [], "completed", False),
    ("A1D", fixtures("audit_a1_vars.c"), ["--trace-mode", "dual"], "completed", True),
    ("A2", fixtures("audit_a2_calls.c"), ["--trace-mode", "dual"], "completed", True),
    ("A3", fixtures("audit_a3_recurse.c"), ["--trace-mode", "dual"], "completed", True),
    ("A4", fixtures("audit_a4_globals.c"), ["--trace-mode", "dual"], "completed", True),
    ("A5", fixtures("audit_a5_main.c"),
     ["--add-source"] + fixtures("audit_a5_helper.c"), "completed", True),
    # ---- Suite B ----
    ("B1", [os.path.join(ROOT, "fixtures/c/null_write.c")], ["--timeout", "20"], "crashed", True),
    ("B2", fixtures("audit_b2_heap.c"), ["--trace-mode", "dual"], "completed", True),
    ("B3", fixtures("audit_b3_realloc.c"), ["--trace-mode", "dual"], "completed", True),
    ("B4", fixtures("audit_b4_alias.c"), ["--trace-mode", "dual"], "completed", True),
    ("B5", [os.path.join(ROOT, "fixtures/c/use_after_free.c")], ["--timeout", "20"], "completed", True),
    ("B6", fixtures("audit_b6_double_free.c"), ["--timeout", "20"], "crashed", True),
    ("B7", fixtures("audit_b7_oob.c"), ["--timeout", "20"], "crashed", True),
    ("B8", fixtures("audit_b2_heap.c"), ["--trace-mode", "source"], "completed", False),
    ("B9", fixtures("audit_b9_bytes.c"), ["--trace-mode", "dual"], "completed", True),
    # ---- Suite C ----
    ("C1", fixtures("audit_c1_arith.c"), [], "completed", True),
    ("C2", fixtures("audit_c2_loop.c"), [], "completed", True),
    ("C3", fixtures("audit_a2_calls.c"), [], "completed", True),  # dual again, calls
    ("C4", fixtures("audit_c4_regs.c"), [], "completed", True),
    ("C5", [os.path.join(ROOT, "fixtures/c/null_write.c")], ["--timeout", "20"], "crashed", True),  # same as B1 dual, checked again
    ("C6", fixtures("audit_c6_oneline.c"), [], "completed", True),
    ("C7", fixtures("audit_c7_libcalls.c"), [], "completed", True),
    # ---- Suite D ----
    ("D1", [os.path.join(ROOT, "fixtures/c/spin_same_line.c")], ["--timeout", "10", "--max-steps", "1000000"], "timeout", True),
    ("D2", fixtures("audit_c2_loop.c"), ["--max-instruction-events", "25"], "step_limit_reached", True),
    ("D3", fixtures("audit_a4_globals.c"), ["--max-source-checkpoints", "3", "--trace-mode", "dual"], "step_limit_reached", True),
    ("D4", fixtures("audit_d4_output.c"), ["--trace-mode", "source", "--max-output-bytes", "16384", "--timeout", "60"], "step_limit_reached", False),
    ("J1", fixtures("audit_c2_loop.c"), ["--journal-only"], "completed", True),  # journal e2e
]

JOURNAL_PATH = os.path.join(EV, "J1", "session.jsonl")

def main():
    results = []
    for tid, srcs, extra, expect, dual in TESTS:
        args = srcs + extra
        journal = JOURNAL_PATH if tid == "J1" else None
        res = run_engine(tid, args, os.path.basename(srcs[0]), expect, dual, journal=journal)
        results.append(res)
        fails = [c for c in res["checks"] if c[1] == "FAIL"]
        print(f"[{res['status']}] {tid}: status={res['summary']['execution_status']} "
              f"snaps={res['summary']['snapshots']} events={res['summary']['instruction_count']} "
              f"wall={res['wall_seconds']}s fails={len(fails)}")
        for c in fails:
            print(f"    FAIL {c[0]}: {c[2][:140]}")
    # ---- journal run needs a real journal file (J1 used --journal-only without path? fixed below)
    # ---- Suite D6 stage failures ----
    # D6A: no valgrind reachable — shadow it with a non-executable file
    shadow = os.path.join(EV, "shadowbin")
    os.makedirs(shadow, exist_ok=True)
    for tool in ("gcc", "gdb", "addr2line", "as", "ld"):
        src = shutil.which(tool)
        if src:
            dst = os.path.join(shadow, tool)
            if os.path.lexists(dst):
                os.remove(dst)
            os.symlink(src, dst)
    open(os.path.join(shadow, "valgrind"), "w").write("# shadowed out for D6A\n")
    os.chmod(os.path.join(shadow, "valgrind"), 0o644)  # not executable -> probe fails -> "missing"
    # D6A: no valgrind on PATH
    env = dict(os.environ, PATH=shadow + ":" + "/usr/bin:/bin:/usr/sbin")
    ev_dir = os.path.join(EV, "D6A")
    shutil.rmtree(ev_dir, ignore_errors=True); os.makedirs(ev_dir)
    cmd = [BIN] + fixtures("audit_a1_vars.c")
    t0 = time.time()
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, timeout=120, env=env)
    open(os.path.join(ev_dir, "engine_stderr.log"), "wb").write(p.stderr)
    open(os.path.join(ev_dir, "command.txt"), "w").write("PATH w/o valgrind: " + " ".join(cmd) + "\n")
    try:
        tr = json.loads(p.stdout)
        open(os.path.join(ev_dir, "trace.json"), "wb").write(p.stdout)
        r = run_checks("D6A", tr, fixture="audit_a1_vars.c", dual=False, expect="completed", wall=time.time()-t0, rc=p.returncode)
    except Exception as ex:
        r = {"id": "D6A", "status": "FAIL", "checks": [("trace_json_parsed", "FAIL", str(ex))], "summary": {}, "wall_seconds": time.time()-t0}
    results.append(r); print(f"[{r['status']}] D6A valgrind-missing fails={sum(1 for c in r['checks'] if c[1]=='FAIL')}")
    for c in r["checks"]:
        if c[1] == "FAIL": print(f"    FAIL {c[0]}: {c[2][:140]}")

    # D6B: fake valgrind emitting garbage xml
    fake_vg = os.path.join(shadow, "valgrind")
    open(fake_vg, "w").write("#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in --xml-file=*) out=\"${a#--xml-file=}\";; esac; done\n"
                             "printf 'this is not xml at all <<<<>>>>' > \"${out:-/dev/null}\"\nexit 0\n")
    os.chmod(fake_vg, 0o755)
    ev_dir = os.path.join(EV, "D6B")
    shutil.rmtree(ev_dir, ignore_errors=True); os.makedirs(ev_dir)
    t0 = time.time()
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, timeout=120, env=env)
    open(os.path.join(ev_dir, "engine_stderr.log"), "wb").write(p.stderr)
    open(os.path.join(ev_dir, "command.txt"), "w").write("fake valgrind (garbage xml): " + " ".join(cmd) + "\n")
    open(os.path.join(ev_dir, "fake_valgrind.sh"), "w").write(open(fake_vg).read())
    try:
        tr = json.loads(p.stdout)
        open(os.path.join(ev_dir, "trace.json"), "wb").write(p.stdout)
        r = run_checks("D6B", tr, fixture="audit_a1_vars.c", dual=False, expect="completed", wall=time.time()-t0, rc=p.returncode)
    except Exception as ex:
        r = {"id": "D6B", "status": "FAIL", "checks": [("trace_json_parsed", "FAIL", str(ex))], "summary": {}, "wall_seconds": time.time()-t0}
    results.append(r); print(f"[{r['status']}] D6B valgrind-garbage fails={sum(1 for c in r['checks'] if c[1]=='FAIL')}")
    for c in r["checks"]:
        if c[1] == "FAIL": print(f"    FAIL {c[0]}: {c[2][:140]}")

    # D6C: gdb that dies mid-session (overwrite any existing shadow entry first)
    fake_gdb = os.path.join(shadow, "gdb")
    real_gdb = shutil.which("gdb")
    if os.path.lexists(fake_gdb):
        os.remove(fake_gdb)
    open(fake_gdb, "w").write(
        "#!/bin/sh\n"
        f'if [ "$1" = "--version" ]; then exec {real_gdb} "$@"; fi\n'
        f"{real_gdb} \"$@\" & G=$!\n"
        "sleep 0.7\nkill -9 $G 2>/dev/null\nwait $G 2>/dev/null\nexit 137\n")
    os.chmod(fake_gdb, 0o755)
    ev_dir = os.path.join(EV, "D6C")
    shutil.rmtree(ev_dir, ignore_errors=True); os.makedirs(ev_dir)
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=ROOT, capture_output=True, timeout=90, env=env)
        rc, so, se, hang = p.returncode, p.stdout, p.stderr, False
    except subprocess.TimeoutExpired:
        rc, so, se, hang, _ = -9, b"", b"", True, None
    wall = time.time() - t0
    open(os.path.join(ev_dir, "engine_stderr.log"), "wb").write(se)
    open(os.path.join(ev_dir, "command.txt"), "w").write("gdb-kills-itself-after-0.7s: " + " ".join(cmd) + "\n")
    open(os.path.join(ev_dir, "fake_gdb.sh"), "w").write(open(fake_gdb).read())
    survivors = snapshot_processes()
    open(os.path.join(ev_dir, "survivors.json"), "w").write(json.dumps(survivors, indent=1))
    try:
        tr = json.loads(so)
        open(os.path.join(ev_dir, "trace.json"), "wb").write(so)
        r = run_checks("D6C", tr, fixture="audit_a1_vars.c", dual=False, expect="debugger_failed", wall=wall, rc=rc)
        r["checks"].append(("cleanup_no_survivors", "PASS" if not survivors else "FAIL", json.dumps(survivors)))
        if hang:
            r["checks"].append(("no_hang", "FAIL", "engine hung past 90s watchdog"))
        r["status"] = "FAIL" if any(c[1] == "FAIL" for c in r["checks"]) else "PASS"
    except Exception as ex:
        r = {"id": "D6C", "status": "FAIL", "checks": [("trace_json_parsed", "FAIL", str(ex) + (hang and " (also hung)" or ""))], "summary": {}, "wall_seconds": wall}
    results.append(r); print(f"[{r['status']}] D6C gdb-death wall={wall:.1f}s fails={sum(1 for c in r['checks'] if c[1]=='FAIL')}")
    for c in r["checks"]:
        if c[1] == "FAIL": print(f"    FAIL {c[0]}: {c[2][:140]}")

    json.dump({"test_suite": "c-trace-engine-v2-validation", "tests": results},
              open(RESULTS, "w"), indent=1)
    print("wrote", RESULTS)


def run_checks(tid, tr, fixture, dual, expect, wall, rc):
    jctx = {"dual": dual, "journal_path": None, "expect_status": expect, "fixture": fixture}
    checks = [("trace_json_parsed", "PASS", "")]
    checks += [(f"E:{c['invariant']}", c["status"], c["detail"]) for c in validate_trace(tr, jctx)]
    checks += semantic_checks(tr, tid, fixture)
    survivors = snapshot_processes()
    checks.append(("cleanup_no_survivors", "PASS" if not survivors else "FAIL", json.dumps(survivors)))
    status = "FAIL" if any(c[1] == "FAIL" for c in checks) else \
        ("BLOCKED" if any(c[1] == "BLOCKED" for c in checks) else "PASS")
    ev_dir = os.path.join(EV, tid)
    return {"id": tid, "fixture": fixture, "expected": expect, "engine_exit": rc,
            "wall_seconds": round(wall, 2), "status": status, "checks": checks,
            "summary": {"execution_status": tr.get("execution_status"),
                        "outcome": (tr.get("execution") or {}).get("outcome"),
                        "snapshots": len(tr.get("snapshots") or []),
                        "limits": (tr.get("resource_usage") or {}).get("limits_triggered"),
                        "wall_ms": (tr.get("resource_usage") or {}).get("wall_time_ms")},
            "evidence_dir": os.path.relpath(ev_dir, ROOT)}


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Serial Phase 4/5 heap benchmark. Never builds the engine or changes fixtures.

Run `baseline` against the independently preserved Phase 4 executable first.
`final` is deliberately gated until the coordinator clears a release rebuild.
One sample per workload is descriptive, not a statistical performance claim.
"""

import argparse
import collections
import datetime
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import signal
import subprocess
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_DIRECTORY = pathlib.Path("/tmp/kilo/phase5")
SOURCES = {
    "simple_malloc_free": """#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int *p = malloc(sizeof *p);
    if (!p) return 2;
    *p = 17;
    printf("simple=%d\\n", *p);
    free(p);
    return 0;
}
""",
    "loop_reuse": """#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int sum = 0;
    for (int i = 0; i < 8; ++i) {
        int *p = malloc(sizeof *p);
        if (!p) return 2;
        *p = i + 1;
        sum += *p;
        free(p);
    }
    printf("reuse=%d\\n", sum);
    return sum == 36 ? 0 : 3;
}
""",
    "realloc_heavy": """#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int *p = malloc(sizeof *p);
    if (!p) return 2;
    p[0] = 9;
    for (int i = 2; i <= 7; ++i) {
        int *next = realloc(p, (size_t)i * sizeof *p);
        if (!next) { free(p); return 2; }
        p = next;
        p[i - 1] = i;
    }
    printf("realloc=%d,%d\\n", p[0], p[6]);
    free(p);
    return 0;
}
""",
    "recursive_heap": """#include <stdio.h>
#include <stdlib.h>
static int descend(int depth) {
    int *p = malloc(sizeof *p);
    if (!p) return -1000;
    *p = depth;
    int result = depth ? descend(depth - 1) : 0;
    result += *p;
    free(p);
    return result;
}
int main(void) {
    int result = descend(5);
    printf("recursive=%d\\n", result);
    return result == 15 ? 0 : 3;
}
""",
    "pointer_array": """#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int *items[4] = {0};
    for (int i = 0; i < 4; ++i) {
        items[i] = malloc(sizeof *items[i]);
        if (!items[i]) return 2;
        *items[i] = 10 + i;
    }
    int result = *items[0] + *items[3];
    printf("array=%d\\n", result);
    for (int i = 3; i >= 0; --i) free(items[i]);
    return result == 23 ? 0 : 3;
}
""",
    "alias_reassign": """#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int *first = malloc(sizeof *first);
    int *second = malloc(sizeof *second);
    if (!first || !second) { free(first); free(second); return 2; }
    *first = 11;
    *second = 22;
    int *alias = first;
    int before = *alias;
    alias = second;
    int after = *alias;
    printf("alias=%d,%d\\n", before, after);
    free(first);
    free(second);
    return before == 11 && after == 22 ? 0 : 3;
}
""",
    "nested_calls": """#include <stdio.h>
#include <stdlib.h>
static int leaf(int *p) {
    int *q = malloc(sizeof *q);
    if (!q) return -100;
    *q = *p + 4;
    int result = *q;
    free(q);
    return result;
}
static int middle(int n) {
    int *local = malloc(sizeof *local);
    if (!local) return -100;
    *local = n;
    int result = leaf(local);
    free(local);
    return result;
}
int main(void) {
    int result = middle(8);
    printf("nested=%d\\n", result);
    return result == 12 ? 0 : 3;
}
""",
    "snapshotheavy": """#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int *kept[24] = {0};
    int total = 0;
    for (int i = 0; i < 24; ++i) {
        kept[i] = malloc(sizeof *kept[i]);
        if (!kept[i]) return 2;
        *kept[i] = i;
        total += *kept[i];
    }
    for (int i = 0; i < 24; ++i) free(kept[i]);
    printf("snapshotheavy=%d\\n", total);
    return total == 276 ? 0 : 3;
}
""",
    "many_events": """#include <stdio.h>
#include <stdlib.h>
int main(void) {
    int sum = 0;
    for (int i = 0; i < 40; ++i) {
        int *p = calloc(1, sizeof *p);
        if (!p) return 2;
        *p = i;
        sum += *p;
        free(p);
    }
    printf("events=%d\\n", sum);
    return sum == 780 ? 0 : 3;
}
""",
    "zero_fail": """#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
int main(void) {
    void *zero = malloc(0);
    void *empty = calloc(0, 4);
    int *live = malloc(sizeof *live);
    if (!live) { free(zero); free(empty); return 2; }
    *live = 37;
    volatile size_t impossible = SIZE_MAX;
    void *failed = realloc(live, impossible);
    if (failed) { free(failed); free(zero); free(empty); return 3; }
    printf("zero_fail=%d\\n", *live);
    free(live);
    free(zero);
    free(empty);
    free(NULL);
    return 0;
}
""",
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def provenance(binary):
    stat = binary.stat()
    return {"path": str(binary), "sha256": sha(binary), "bytes": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
            "mtime_utc": datetime.datetime.fromtimestamp(
                stat.st_mtime, datetime.timezone.utc).isoformat(),
            "git_head": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                       text=True, capture_output=True, check=True).stdout.strip(),
            "platform": platform.platform()}


def prepare(directory):
    manifest = directory / "manifest.json"
    if manifest.exists():
        cases = json.loads(manifest.read_text())
        for case in cases:
            for name in ("source", "config"):
                if sha(pathlib.Path(case[name])) != case[name + "_sha256"]:
                    raise RuntimeError("Input changed: " + case[name])
        return cases
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "sources").mkdir()
    (directory / "configs").mkdir()
    cases = []
    for name, code in SOURCES.items():
        source = directory / "sources" / (name + ".c")
        source.write_text(code)
        config = directory / "configs" / (name + ".json")
        save(config, {"source": str(source), "pretty": False,
                      "sanitizer_trigger": "never",
                      "tracing": {"mode": "dual", "discover_globals": True},
                      "limits": {"timeout_seconds": 100, "debugger_timeout_seconds": 100,
                                 "compiler_timeout_seconds": 30, "analysis_timeout_seconds": 10,
                                 "max_steps": 12000, "max_snapshot_count": 800,
                                 "max_source_checkpoints": 800,
                                 "max_instruction_events": 30000,
                                 "max_memory_read_bytes": 4096,
                                 "max_snapshot_memory_bytes": 1048576}})
        cases.append({"name": name, "source": str(source), "source_sha256": sha(source),
                      "config": str(config), "config_sha256": sha(config)})
    save(manifest, cases)
    return cases


def code_addresses(data):
    """Build PIE-safe module offsets only for ranges actually backed by GDB."""
    ranges = []
    pattern = re.compile(r"^(.*):range=([0-9a-fA-F]+)-([0-9a-fA-F]+):file_offset=([0-9a-fA-F]+)")
    for item in data.get("disassembly_table", []):
        module = item.get("module_identity") or ""
        match = pattern.match(module)
        if match:
            start, end, offset = (int(match.group(i), 16) for i in (2, 3, 4))
            entry = (start, end, start - offset, pathlib.Path(match.group(1)).name)
            if entry not in ranges:
                ranges.append(entry)
    return ranges


def projection(data):
    ranges = code_addresses(data)
    unresolved = collections.Counter()

    def code(value):
        if not value:
            return value
        try:
            addr = int(value, 16)
        except (TypeError, ValueError):
            return value
        for start, end, base, module in ranges:
            if start <= addr < end:
                return module + "+" + hex(addr - base)
        unresolved["code"] += 1
        return None

    def path(value):
        return pathlib.Path(value).name if value else value

    def pointer(value, snapshot):
        if value is None:
            return None
        try:
            addr = int(value, 16)
        except (TypeError, ValueError):
            return value
        if addr == 0:
            return "null"
        blocks = [("heap:" + str(b["allocation_id"]), b) for b in snapshot.get("heap_blocks", [])]
        groups = [("frame0", snapshot.get("variables", [])),
                  ("global", snapshot.get("observation", {}).get("globals", []))]
        groups.extend(("caller:" + str(index), frame.get("variables", []))
                      for index, frame in enumerate(snapshot.get("caller_frames", []), 1))
        objects = blocks + [("object:" + group + ":" + str(v.get("scope")) + ":"
                             + str(v.get("name")) + ":" + str(v.get("declaration_line")), v)
                            for group, variables in groups for v in variables]
        matches = set()
        for identity, item in objects:
            try:
                start = int(item["address"], 16)
                extent = item.get("size_bytes") or 0
                if start <= addr < start + max(extent, 1):
                    matches.add(identity + "+" + str(addr - start))
            except (KeyError, TypeError, ValueError):
                continue
        if len(matches) == 1:
            return matches.pop()
        unresolved["pointer"] += 1
        return None

    result = {"execution": {k: data.get("execution", {}).get(k)
                             for k in ("outcome", "exit_code", "signal")},
              "source_checkpoints": data.get("source_checkpoints", []),
              "instructions": [], "stops": [], "values": [], "heap_events": [],
              "gaps": [], "output": None, "unresolved": None}
    for item in data.get("instruction_events", []):
        source = item.get("source") or {}
        result["instructions"].append({
            "id": item.get("event_id"), "before": item.get("stop_id_before"),
            "after": item.get("stop_id_after"), "pc": code(item.get("address")),
            "next": code(item.get("next_address")), "outcome": item.get("outcome"),
            "checkpoint": item.get("checkpoint_id"), "reason": item.get("stopped_reason"),
            "signal": item.get("signal"), "source": {"file": path(source.get("file")),
                                                   "line": source.get("line"),
                                                   "func": source.get("func")}})
    for snap in data.get("snapshots", []):
        obs = snap.get("observation") or {}
        result["stops"].append({
            "step": snap.get("step_index"), "stop": obs.get("stop_id"),
            "instruction_position": obs.get("instruction_position"),
            "pc": code(obs.get("instruction_address")), "file": path(snap.get("file")),
            "line": snap.get("line"), "func": snap.get("func"),
            "reason": snap.get("stopped_reason"), "signal": snap.get("signal"),
            "stdout": snap.get("stdout_so_far")})
        values = []
        for group, variables in (("frame0", snap.get("variables", [])),
                                 ("global", obs.get("globals", []))):
            for var in variables:
                value = var.get("value")
                if isinstance(value, str):
                    value = re.sub(r"0x[0-9a-fA-F]+",
                                   lambda match: pointer(match.group(), snap) or "<unresolved-pointer>",
                                   value)
                values.append({"group": group, "name": var.get("name"),
                               "declaration": path(var.get("declaration_file")),
                               "line": var.get("declaration_line"), "value": value,
                               "points_to": pointer(var.get("points_to"), snap),
                               "read_status": var.get("read_status"),
                               "scope_status": var.get("scope_status")})
        result["values"].append(values)
    for event in data.get("heap_events", []):
        result["heap_events"].append({k: event.get(k) for k in
                                      ("sequence", "event_type", "allocation_id", "size_bytes",
                                       "old_allocation_id", "heap_usage_after", "detail")})
    for gap in data.get("trace_gaps", []):
        result["gaps"].append({k: gap.get(k) for k in ("instruction_position", "origin", "reason")})
    result["output"] = {"program_output_bytes": data.get("resource_usage", {}).get("program_output_bytes"),
                        "last_stdout": result["stops"][-1]["stdout"] if result["stops"] else None}
    result["unresolved"] = dict(unresolved)
    return result


def metrics(data):
    profile = data.get("profiling") or {}
    events = data.get("heap_events", [])
    lifetimes = data.get("allocation_lifetimes", [])
    categories = profile.get("gdb_command_categories")
    correlation = collections.Counter()
    for item in lifetimes + events:
        for key in ("correlation", "association", "resolution", "correlation_status"):
            if item.get(key) is not None:
                correlation[key + ":" + str(item[key])] += 1
    return {"execution_status": data.get("execution_status"),
            "execution": data.get("execution"), "stage_results": data.get("stage_results"),
            "instruction_count": data.get("instruction_count"),
            "instruction_events": len(data.get("instruction_events", [])),
            "checkpoints": len(data.get("snapshots", [])), "heap_events": len(events),
            "heap_event_types": dict(collections.Counter(e.get("event_type") for e in events)),
            "allocation_lifetimes": len(lifetimes),
            "resolved_vs_ambiguous": dict(correlation) if correlation else None,
            "event_processing_profile": {key: value for key, value in profile.items()
                                         if key in (
                                             "heap_event_ingest_time_us",
                                             "heap_snapshot_reconstruction_time_us",
                                             "heap_interval_correlation_time_us",
                                             "pointer_enrichment_time_us",
                                         )} or None,
            "correlation_fields_present": sorted({key for item in lifetimes + events
                                                   for key in item if "correlat" in key or "resol" in key
                                                   or "ambigu" in key or "associat" in key}),
            "gdb_command_categories": categories,
            "gdb_commands": data.get("resource_usage", {}).get("gdb_commands_issued"),
            "source_resolution_seconds": profile.get("source_resolution_time_us", 0) / 1e6,
            "capture_seconds": profile.get("capture_time_us", 0) / 1e6,
            "debugger_seconds": profile.get("debugger_time_us", 0) / 1e6,
            "engine_peak_rss_kib": profile.get("engine_peak_rss_kib"),
            "profiling": profile, "resource_usage": data.get("resource_usage"),
            "tools": {key: (data.get("session_meta") or {}).get(key) for key in
                      ("gcc_version", "gdb_version", "valgrind_version", "addr2line_version")}}


def run(binary, directory, label, cases):
    destination = directory / label
    if destination.exists():
        raise RuntimeError("Refusing to overwrite suite: " + str(destination))
    destination.mkdir()
    original = provenance(binary)
    copied = destination / "c-trace-engine"
    shutil.copy2(binary, copied)
    if sha(copied) != original["sha256"]:
        raise RuntimeError("Executable changed while copying")
    (destination / "cli-help.txt").write_text(subprocess.run(
        [str(copied), "--help"], text=True, capture_output=True, check=True).stdout)
    summary = {"suite": label, "binary": provenance(copied), "original_binary": original,
               "method": "one serial release execution per exact source/config; complete pipeline",
               "samples": []}
    save(destination / "summary.json", summary)
    for case in cases:
        name = case["name"]
        trace = destination / (name + ".trace.json")
        journal = destination / (name + ".journal.jsonl")
        stderr = destination / (name + ".stderr")
        timing = destination / (name + ".time")
        command = ["/usr/bin/time", "-f", "%e %M", "-o", str(timing), str(copied),
                    "--config", case["config"], "--journal", str(journal),
                    "--output", str(trace), "--keep-temp"]
        sample = {"case": name, "source_sha256": case["source_sha256"],
                  "config_sha256": case["config_sha256"], "command": command,
                  "trace": str(trace), "journal": str(journal), "stderr": str(stderr),
                  "timing": str(timing)}
        start = time.monotonic()
        with stderr.open("w") as err:
            process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.DEVNULL,
                                       stderr=err, start_new_session=True)
            try:
                sample["returncode"] = process.wait(timeout=180)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                sample["error"] = "harness timeout after 180 seconds"
        sample["wall_seconds"] = time.monotonic() - start
        sample["stderr_bytes"] = stderr.stat().st_size
        sample["trace_bytes"] = trace.stat().st_size if trace.exists() else None
        sample["journal_bytes"] = journal.stat().st_size if journal.exists() else None
        if timing.exists():
            fields = timing.read_text().strip().splitlines()[-1].split()
            if len(fields) == 2:
                try:
                    sample["gnu_time_wall_seconds"] = float(fields[0])
                    sample["peak_waited_process_rss_kib"] = int(fields[1])
                except ValueError:
                    sample["time_parse_error"] = timing.read_text()
        if trace.exists():
            try:
                processing_start = time.monotonic()
                data = json.loads(trace.read_text())
                sample.update(metrics(data))
                session_dir = (data.get("session_meta") or {}).get("session_dir")
                if session_dir:
                    event_log = pathlib.Path(session_dir) / "alloc_events.txt"
                    sample["event_log_path"] = str(event_log)
                    sample["event_log_bytes"] = (
                        event_log.stat().st_size if event_log.is_file() else None
                    )
                semantic = destination / (name + ".semantic.json")
                save(semantic, projection(data))
                sample["semantic"] = str(semantic)
                sample["processing_seconds"] = time.monotonic() - processing_start
            except (ValueError, TypeError, KeyError) as exc:
                sample["processing_error"] = str(exc)
        else:
            sample["error"] = sample.get("error", "no trace emitted")
        summary["samples"].append(sample)
        save(destination / "summary.json", summary)
        print(label, name, "rc", sample.get("returncode"), "seconds",
              round(sample["wall_seconds"], 3), "events", sample.get("heap_events"), flush=True)
    return summary


def differences(before, after, limit=8):
    examples = []

    def visit(left, right, path):
        if left == right or len(examples) >= limit:
            return
        if isinstance(left, dict) and isinstance(right, dict):
            for key in sorted(left.keys() | right.keys()):
                visit(left.get(key), right.get(key), path + "/" + key)
        elif isinstance(left, list) and isinstance(right, list):
            if len(left) != len(right):
                examples.append({"path": path + "/length", "baseline": len(left), "final": len(right)})
            for index, (a, b) in enumerate(zip(left, right)):
                visit(a, b, path + "/" + str(index))
        else:
            examples.append({"path": path, "baseline": left, "final": right})

    visit(before, after, "")
    return examples


def compare(directory):
    first = json.loads((directory / "baseline" / "summary.json").read_text())
    second = json.loads((directory / "final" / "summary.json").read_text())
    if len(first["samples"]) != len(second["samples"]) or len(first["samples"]) != len(SOURCES):
        raise RuntimeError("Incomplete paired workloads; refusing partial comparison")
    results = []
    for old, new in zip(first["samples"], second["samples"]):
        if old["case"] != new["case"]:
            raise RuntimeError("Workload order changed")
        a = json.loads(pathlib.Path(old["semantic"]).read_text())
        b = json.loads(pathlib.Path(new["semantic"]).read_text())
        comparisons = {}
        for key in a:
            if key == "unresolved":
                continue
            known_equal = a[key] == b[key]
            affected = ((key in ("instructions", "stops") and
                         (a["unresolved"].get("code", 0) or b["unresolved"].get("code", 0))) or
                        (key == "values" and
                         (a["unresolved"].get("pointer", 0) or b["unresolved"].get("pointer", 0))))
            comparisons[key] = {"equal": None if known_equal and affected else known_equal,
                                "known_fields_equal": known_equal,
                                "status": ("indeterminate_unresolved_addresses" if known_equal and affected
                                           else "match" if known_equal else "difference"),
                                "examples": differences(a[key], b[key])}
        # The wrapper's code changed between builds. Absolute/relative PCs
        # cannot establish instruction equivalence across changed binaries;
        # compare retained event order, source and outcomes independently.
        control_flow = {
            "instructions_without_code_addresses": [
                [{key: value for key, value in item.items() if key not in ("pc", "next")}
                 for item in trace["instructions"]]
                for trace in (a, b)
            ],
            "stops_without_code_addresses": [
                [{key: value for key, value in item.items() if key != "pc"}
                 for item in trace["stops"]]
                for trace in (a, b)
            ],
            "heap_events_without_lifetime_ids": [
                [{key: item.get(key) for key in
                  ("sequence", "event_type", "size_bytes", "heap_usage_after")}
                 for item in trace["heap_events"]]
                for trace in (a, b)
            ],
        }
        controlled = {
            name: {"equal": before == after, "examples": differences(before, after)}
            for name, (before, after) in control_flow.items()
        }
        measurements = {}
        for key in ("wall_seconds", "peak_waited_process_rss_kib", "engine_peak_rss_kib",
                    "trace_bytes", "journal_bytes", "event_log_bytes", "stderr_bytes", "processing_seconds",
                    "capture_seconds", "debugger_seconds", "source_resolution_seconds",
                    "gdb_commands", "heap_events", "instruction_events", "checkpoints"):
            x, y = old.get(key), new.get(key)
            measurements[key] = {"baseline": x, "final": y,
                                 "delta": y - x if x is not None and y is not None else None}
        results.append({"case": old["case"], "identical_source_and_config":
                        old["source_sha256"] == new["source_sha256"] and
                        old["config_sha256"] == new["config_sha256"],
                         "semantics": comparisons, "controlled_semantics": controlled,
                        "allocation_resolution": {"baseline": old.get("resolved_vs_ambiguous"),
                                                  "final": new.get("resolved_vs_ambiguous")},
                        "event_processing_profile": {"baseline": old.get("event_processing_profile"),
                                                     "final": new.get("event_processing_profile")},
                        "baseline_unresolved": a["unresolved"], "final_unresolved": b["unresolved"],
                        "note": "Unknown addresses are indeterminate, never asserted equal; corrected attribution/identity excluded. Inspect raw traces for differences."})
    report = {"baseline_sha256": first["binary"]["sha256"],
              "final_sha256": second["binary"]["sha256"],
              "limitations": ["Single run per workload; no significance or causal speed claim.",
                              "GNU time RSS includes largest waited-for child, often GDB.",
                              "Unresolved addresses and uninitialized values are not proven equal.",
                              "Heap source attribution and identity fields intentionally excluded; allocation IDs retained."],
              "cases": results}
    save(directory / "comparison.json", report)
    return report


def analyze(directory):
    """Refresh projections from captured traces without starting an engine."""
    for label in ("baseline", "final"):
        summary_path = directory / label / "summary.json"
        if not summary_path.exists():
            continue
        summary = json.loads(summary_path.read_text())
        for sample in summary["samples"]:
            if not pathlib.Path(sample["trace"]).exists():
                continue
            data = json.loads(pathlib.Path(sample["trace"]).read_text())
            sample["event_processing_profile"] = metrics(data)["event_processing_profile"]
            semantic = pathlib.Path(sample["semantic"])
            save(semantic, projection(data))
        save(summary_path, summary)


def self_test():
    original = {"disassembly_table": [{"address": "0x401020",
                 "module_identity": "/tmp/a/program_debug:range=401000-402000:file_offset=00000000"}],
                "instruction_events": [{"event_id": 0, "stop_id_before": 1,
                                        "address": "0x401020", "next_address": "0x401024",
                                        "source": {"file": "/tmp/a/work.c", "line": 8}}],
                "snapshots": [{"step_index": 0, "variables": [
                    {"name": "arr", "address": "0x7000", "size_bytes": 16,
                     "value": "{0x7008, 0x7000}"},
                    {"name": "p", "value": "0x7008", "points_to": "0x7008"}],
                    "observation": {"stop_id": 1, "instruction_address": "0x401020"}}]}
    shifted = json.loads(json.dumps(original))
    shifted["disassembly_table"][0].update({
        "address": "0x601020",
        "module_identity": "/tmp/b/program_debug:range=601000-602000:file_offset=00000000"})
    shifted["instruction_events"][0].update({"address": "0x601020", "next_address": "0x601024"})
    shifted["snapshots"][0]["observation"]["instruction_address"] = "0x601020"
    shifted["snapshots"][0]["variables"][0].update({"address": "0x8000", "value": "{0x8008, 0x8000}"})
    shifted["snapshots"][0]["variables"][1].update({"value": "0x8008", "points_to": "0x8008"})
    assert projection(original) == projection(shifted), "PIE/heap/stack relocation changed semantics"
    shifted["snapshots"][0]["variables"][1]["points_to"] = "0x8009"
    assert projection(original)["values"] != projection(shifted)["values"], "interior pointer offset lost"
    shifted["instruction_events"][0]["address"] = "0x601021"
    assert projection(original)["instructions"] != projection(shifted)["instructions"], "code offset lost"
    shifted["snapshots"][0]["variables"][1]["points_to"] = "0x9000"
    assert projection(shifted)["unresolved"]["pointer"] > 0, "unknown address asserted resolved"
    print("projection self-test passed: relocated addresses, interior/code differences, unknowns")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("baseline", "final", "compare", "analyze", "self-test"))
    parser.add_argument("--directory", type=pathlib.Path, default=DEFAULT_DIRECTORY)
    parser.add_argument("--binary", type=pathlib.Path)
    parser.add_argument("--final-authorized", action="store_true",
                        help="Coordinator cleared serial final run against rebuilt release executable")
    args = parser.parse_args()
    directory = args.directory.resolve()
    if args.stage == "self-test":
        self_test()
        return
    if args.stage == "compare":
        compare(directory)
        return
    if args.stage == "analyze":
        analyze(directory)
        return
    if args.stage == "final" and (not args.final_authorized or
                                   not (directory / "baseline" / "summary.json").exists()):
        parser.error("final requires captured baseline and coordinator clearance (--final-authorized)")
    cases = prepare(directory)
    binary = (args.binary or (directory / "c-trace-engine.phase4" if args.stage == "baseline"
                              else ROOT / "target/release/c-trace-engine")).resolve()
    if args.stage == "baseline" and binary == (ROOT / "target/release/c-trace-engine").resolve():
        parser.error("baseline must use independently preserved Phase 4 binary, not mutable release target")
    if args.stage == "final" and provenance(binary)["sha256"] == json.loads(
            (directory / "baseline" / "summary.json").read_text())["binary"]["sha256"]:
        parser.error("final binary matches baseline; rebuild release before final")
    run(binary, directory, args.stage, cases)
    if args.stage == "final":
        compare(directory)


if __name__ == "__main__":
    main()

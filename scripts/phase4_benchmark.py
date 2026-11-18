#!/usr/bin/env python3
"""Serial Phase 3/4 benchmark; never builds or runs tests.

Run baseline before replacing the release executable, then final only once the
coordinator has finished serial tests and rebuilt release. Outputs are durable:
configs, generated sources, executable copy, stderr, time, traces and summaries.
No third-party Python packages are required. One sample is not significance.
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
import statistics
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXPECTED_BASELINE_MTIME = "2026-11-17T15:05:49"
EXPANDED = {"max_caller_frames": 3, "max_caller_variables": 24,
            "max_extra_memory_reads": 32, "max_aggregate_bytes": 16384,
            "max_snapshot_memory_bytes": 1048576}
GENERATED = {
    "large_aggregate": """struct Block { int items[2048]; };
int main(void) {
    struct Block block = {{0}};
    block.items[0] = 3;
    block.items[2047] = 5;
    return block.items[0] + block.items[2047] == 8 ? 0 : 1;
}
""",
    "arithmetic": """int main(void) {
    int a = 7;
    int b = 9;
    int result = a * b + 5;
    return result == 68 ? 0 : 1;
}
""",
    "nested_calls": """static int leaf(int *p) {
    int value = *p + 2;
    return value;
}
static int middle(int x) {
    int local = x + 3;
    int result = leaf(&local);
    return result;
}
int main(void) {
    int result = middle(7);
    return result == 12 ? 0 : 1;
}
""",
    "deep_recursion": """static int recurse(int depth) {
    if (depth == 0) return 0;
    int result = recurse(depth - 1);
    return result + depth;
}
int main(void) {
    int result = recurse(32);
    return result == 528 ? 0 : 1;
}
""",
    "snapshotheavy": """int main(void) {
    volatile int total = 0;
    for (int i = 0; i < 499; ++i) {
        total += i;
    }
    return total == 124251 ? 0 : 1;
}
""",
}
FIXTURES = {"arrays_structs": "aggregate.c", "vla": "vla_metadata.c",
            "globals_statics": "globals.c", "pointers": "pointer_identity.c",
            "heap": "pointer_demo.c", "finite_loop": "finite_loop.c"}
ORDER = ["arithmetic", "nested_calls", "deep_recursion", "arrays_structs", "large_aggregate",
         "vla", "globals_statics", "pointers", "heap", "finite_loop",
         "snapshotheavy"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, data):
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def provenance(binary):
    stat = binary.stat()
    return {"path": str(binary), "sha256": digest(binary), "size": stat.st_size,
            "mtime_ns": stat.st_mtime_ns,
            "mtime_utc": datetime.datetime.fromtimestamp(
                stat.st_mtime, datetime.timezone.utc).isoformat(),
            "git_head": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                                       capture_output=True, text=True).stdout.strip(),
            "git_status": subprocess.run(["git", "status", "--short"], cwd=ROOT,
                                         capture_output=True, text=True).stdout,
            "platform": platform.platform()}


def prepare(directory):
    manifest_path = directory / "manifest.json"
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
        for case in manifest["cases"]:
            for field in ("source", "config"):
                if digest(pathlib.Path(case[field])) != case[field + "_sha256"]:
                    raise RuntimeError("Input changed: " + case[field])
        return manifest
    directory.mkdir(parents=True, exist_ok=True)
    sources = directory / "sources"
    configs = directory / "configs"
    sources.mkdir()
    configs.mkdir()
    cases = []
    for name in ORDER:
        if name in GENERATED:
            source = sources / (name + ".c")
            source.write_text(GENERATED[name])
        else:
            source = ROOT / "fixtures" / "c" / FIXTURES[name]
        # These keys are accepted by the actual Phase 3 executable. Do not add
        # new Phase 4 keys to the compatibility configuration.
        config = {"source": str(source), "pretty": False,
                  "sanitizer_trigger": "never",
                  "limits": {"timeout_seconds": 120, "max_steps": 20000,
                             "max_snapshot_count": 1100,
                             "max_source_checkpoints": 1100,
                             "max_instruction_events": 30000,
                             "max_snapshot_memory_bytes": 1048576,
                             "max_memory_read_bytes": 4096,
                             "max_variables_per_snapshot": 24,
                             "compiler_timeout_seconds": 30,
                             "debugger_timeout_seconds": 120,
                             "analysis_timeout_seconds": 10},
                  "tracing": {"mode": "dual", "discover_globals": True}}
        path = configs / (name + ".json")
        save(path, config)
        cases.append({"name": name, "source": str(source),
                      "source_sha256": digest(source), "config": str(path),
                      "config_sha256": digest(path),
                      "source_origin": "generated" if name in GENERATED else "fixture"})
    manifest = {"cases": cases, "expanded_limits": EXPANDED,
                "method": "serial release CLI; full pipeline; dual mode; no warmup",
                "limitations": ["One sample does not establish statistical significance.",
                                "GNU time RSS includes largest waited-for child, not only engine.",
                                "Uninitialized values and aggregate padding may vary.",
                                "No raw-byte equality claim without initialization evidence."]}
    save(manifest_path, manifest)
    return manifest


def variable_groups(snapshot):
    yield snapshot.get("variables", [])
    yield snapshot.get("observation", {}).get("globals", [])
    for frame in snapshot.get("caller_frames", []):
        yield frame.get("variables", [])


def metrics(data):
    profile = data.get("profiling", {})
    categories = profile.get("gdb_command_categories")
    snapshots = data.get("snapshots", [])
    variables = [v for s in snapshots for group in variable_groups(s) for v in group]
    type_entries = {entry["id"]: entry for entry in (data.get("type_registry") or {}).get("types", [])}

    def dynamic_type(variable):
        reference = variable.get("type_ref")
        seen = set()
        while reference in type_entries and reference not in seen:
            seen.add(reference)
            entry = type_entries[reference]
            if entry.get("runtime_dependent"):
                return True
            if entry.get("kind") not in ("typedef", "const", "volatile", "restrict", "atomic"):
                return False
            reference = entry.get("referenced_type")
        return False
    evidence = [v["memory_evidence"] for v in variables if v.get("memory_evidence")]
    category_total = sum(categories.values()) if categories is not None else None
    total = data.get("resource_usage", {}).get("gdb_commands_issued", category_total)
    captured = sum(len(v.get("raw_bytes_hex") or "") // 2 for v in variables)
    heap_evidence = [b["memory_evidence"] for s in snapshots for b in s.get("heap_blocks", [])
                     if b.get("memory_evidence")]
    targets = collections.Counter((v.get("target") or {}).get("kind", "absent")
                                  for v in variables if v.get("points_to") is not None)
    return {
        "total_commands": total,
        "categorized_commands": category_total,
        "commands_per_checkpoint": total / len(snapshots) if total is not None and snapshots else None,
        "frame_queries": categories.get("stack_frames", 0) + categories.get("caller_probe", 0) if categories is not None else None,
        "caller_queries": categories.get("caller_probe", 0) if categories is not None else None,
        "memory_reads": categories.get("memory_reads") if categories is not None else None,
        "checkpoints": len(snapshots),
        "stack_frame_observations": sum(len(s.get("call_stack", [])) for s in snapshots),
        "caller_frame_observations": sum(len(s.get("caller_frames", [])) for s in snapshots),
        "frame0_variable_observations": sum(len(s.get("variables", [])) for s in snapshots),
        "global_variable_observations": sum(len(s.get("observation", {}).get("globals", [])) for s in snapshots),
        "variable_observations": len(variables),
        "requested_bytes": sum(e.get("requested_bytes", 0) for e in evidence + heap_evidence) if evidence or heap_evidence else None,
        "requested_variable_bytes": sum(e.get("requested_bytes", 0) for e in evidence) if evidence else None,
        "requested_heap_bytes": sum(e.get("requested_bytes", 0) for e in heap_evidence) if heap_evidence else None,
        "requested_bytes_note": "explicit memory evidence only; absent in Phase3",
        "observed_object_extent_bytes": sum(v.get("size_bytes") or 0 for v in variables),
        "captured_variable_bytes": captured,
        "captured_heap_bytes": sum(len(b.get("raw_bytes_hex") or "") // 2
                                   for s in snapshots for b in s.get("heap_blocks", [])),
        "memory_evidence_observations": len(evidence),
        "truncated_observations": sum(e.get("status") == "truncated" for e in evidence + heap_evidence) if evidence or heap_evidence else None,
        "memory_evidence_statuses": dict(collections.Counter(e.get("status") for e in evidence + heap_evidence)),
        "memory_truncated_snapshots": sum(bool(s.get("memory_truncated")) for s in snapshots),
        "dynamic_extents": sorted({(v.get("name", ""), v["runtime_size_bytes"])
                                   for v in variables if v.get("runtime_size_bytes") is not None
                                    and (dynamic_type(v) or "variable length" in v.get("c_type", ""))}),
        "dynamic_extents_legacy_size": sorted({(v.get("name", ""), v["size_bytes"])
                                               for v in variables if v.get("size_bytes") is not None
                                                and (dynamic_type(v) or "variable length" in v.get("c_type", ""))}),
        "array_extents": sorted({(v.get("name", ""), v.get("c_type", ""), v["size_bytes"])
                                 for v in variables if v.get("size_bytes") is not None
                                 and "[" in v.get("c_type", "")}),
        "resolved_pointer_observations": sum(
            1 for v in variables if v.get("points_to") is not None
            and ((v.get("target") or {}).get("variable_identity") is not None
                 or (v.get("target") or {}).get("allocation_id") is not None)),
        "pointer_target_kinds": dict(targets),
        "capture_seconds": profile.get("capture_time_us", 0) / 1e6,
        "debugger_seconds": profile.get("debugger_time_us", 0) / 1e6,
        "engine_peak_rss_kib": profile.get("engine_peak_rss_kib"),
        "instruction_attempts": data.get("instruction_count"),
        "execution_status": data.get("execution_status"),
        "execution": data.get("execution"), "stage_results": data.get("stage_results"),
        "resource_usage": data.get("resource_usage"),
        "profiling": profile,
        "provenance_evidence": {
            "type_registry_present": data.get("type_registry") is not None,
            "caller_frames_field_present": any("caller_frames" in s for s in snapshots),
            "caller_limit_field_present": "max_caller_frames" in data.get("execution_limits", {}),
            "aggregate_limit_field_present": "max_aggregate_bytes" in data.get("execution_limits", {}),
            "memory_evidence_field_present": bool(evidence)},
        "tools": {k: (data.get("session_meta") or {}).get(k)
                  for k in ("gcc_version", "gdb_version", "valgrind_version", "addr2line_version")},
    }


def hex_address(value):
    try:
        return int(value, 16) if isinstance(value, str) and value.startswith("0x") else None
    except ValueError:
        return None


def source_path(value):
    if value is None:
        return None
    return str(value).replace(str(ROOT) + "/", "repo/")


class Normalizer:
    """Normalize proven object ranges and per-module GDB instruction offsets.

    Unknown addresses are deliberately retained, not wildcarded. There is no
    basis to claim equality of unresolved pointers. Raw bytes are not compared:
    aggregate padding, uninitialized bytes and stored pointers are not stable.
    """

    def __init__(self, data):
        modules = collections.defaultdict(list)
        for instruction in data.get("disassembly_table", []):
            address = hex_address(instruction.get("address"))
            if address is not None:
                module = instruction.get("module_identity", "unknown")
                # Session-unique temporary directories do not identify code.
                match = re.match(r"(.*):range=([0-9a-fA-F]+)-([0-9a-fA-F]+):file_offset=([0-9a-fA-F]+)", module)
                if match:
                    name = pathlib.Path(match[1]).name
                    start, end, offset = (int(match[index], 16) for index in (2, 3, 4))
                    modules[(name, start - offset, start, end)].append(address)
                else:
                    modules[(pathlib.Path(module).name, None, None, None)].append(address)
        self.modules = [(name, base if base is not None else min(addresses),
                         start if start is not None else min(addresses),
                         end if end is not None else max(addresses) + 32)
                        for (name, base, start, end), addresses in sorted(modules.items())]
        self.unknown_addresses = set()

    def code(self, value):
        address = hex_address(value)
        if address is None:
            return value
        for module, base, start, end in self.modules:
            if start <= address < end:
                return "code:" + module + "+" + hex(address - base)
        self.unknown_addresses.add(value)
        return value

    @staticmethod
    def variable_key(variable, frame):
        return [frame, variable.get("scope"), variable.get("name"),
                source_path(variable.get("declaration_file")), variable.get("declaration_line"),
                variable.get("is_argument")]

    def ranges(self, snapshot):
        ranges = []
        groups = [(0, snapshot.get("variables", []))]
        groups.append(("global", snapshot.get("observation", {}).get("globals", [])))
        groups.extend((c.get("frame", {}).get("level"), c.get("variables", []))
                      for c in snapshot.get("caller_frames", []))
        for frame, variables in groups:
            for variable in variables:
                address = hex_address(variable.get("address"))
                if address is not None:
                    key = json.dumps(self.variable_key(variable, frame), separators=(",", ":"))
                    ranges.append((address, variable.get("size_bytes") or 0, "object:" + key))
        for block in snapshot.get("heap_blocks", []):
            address = hex_address(block.get("address"))
            if address is not None:
                ranges.append((address, block.get("size_bytes", 0),
                               "heap:" + str(block.get("allocation_id"))))
        return ranges

    def object_address(self, value, ranges):
        address = hex_address(value)
        if address is None:
            return value
        if address == 0:
            return "null"
        matches = sorted({name + "+" + str(address - start)
                          for start, size, name in ranges
                          if start <= address < start + max(size, 1)})
        if matches:
            return "|".join(matches)
        return self.code(value)

    def value(self, value, ranges):
        return re.sub(r"0x[0-9a-fA-F]+", lambda match: self.object_address(match[0], ranges),
                      value or "")


def semantic_projection(data):
    normalizer = Normalizer(data)
    snapshots = data.get("snapshots", [])
    projection = {"execution": {key: data.get("execution", {}).get(key)
                                 for key in ("outcome", "exit_code", "signal")},
                  "instructions": [], "checkpoints": [], "frame0_values": [], "global_values": [],
                  "pointer_resolution": [], "heap_events": [], "gaps": [],
                  "limits": data.get("resource_usage", {}).get("limits_triggered", []),
                  "output_bytes": data.get("resource_usage", {}).get("program_output_bytes")}
    for gap in data.get("trace_gaps", []):
        projection["gaps"].append({key: normalizer.code(value) if key in ("start_address", "end_address") else value
                                   for key, value in gap.items()})
    for event in data.get("instruction_events", []):
        projection["instructions"].append({
            "event": event.get("event_id"), "before": event.get("stop_id_before"),
            "after": event.get("stop_id_after"), "address": normalizer.code(event.get("address")),
            "next": normalizer.code(event.get("next_address")),
            "outcome": event.get("outcome"), "reason": event.get("stopped_reason"),
            "signal": event.get("signal"), "checkpoint": event.get("checkpoint_id"),
            "source": {key: source_path(value) if key == "file" else value
                       for key, value in event.get("source", {}).items()}})
    for snapshot in snapshots:
        ranges = normalizer.ranges(snapshot)
        projection["checkpoints"].append({
            "step": snapshot.get("step_index"), "file": source_path(snapshot.get("file")),
            "line": snapshot.get("line"), "func": snapshot.get("func"),
            "reason": snapshot.get("stopped_reason"), "signal": snapshot.get("signal"),
            "stdout": snapshot.get("stdout_so_far"),
            "stop": snapshot.get("observation", {}).get("stop_id"),
            "instruction_position": snapshot.get("observation", {}).get("instruction_position"),
            "instruction_address": normalizer.code(snapshot.get("observation", {}).get("instruction_address")),
            "activation_ids": sorted({v["activation_id"] for v in snapshot.get("variables", [])
                                      if v.get("activation_id") is not None}),
            "stack": [{key: source_path(frame.get(key)) if key == "file" else
                       normalizer.code(frame.get(key)) if key == "address" else frame.get(key)
                       for key in ("level", "func", "file", "line", "address")}
                      for frame in snapshot.get("call_stack", [])]})
        values = []
        targets = []
        globals_values = []
        for level, variable in [(0, v) for v in snapshot.get("variables", [])] + [
                ("global", v) for v in snapshot.get("observation", {}).get("globals", [])]:
            key = normalizer.variable_key(variable, level)
            # Only frame0 object ranges are used in the comparison: enabling
            # caller evidence must not change how an old observation is named.
            current_ranges = [(a, b, c) for a, b, c in ranges
                              if c.startswith("heap:") or c.startswith('object:[0,')
                              or c.startswith('object:["global",')]
            destination = values if level == 0 else globals_values
            destination.append({"key": key, "type": variable.get("c_type"),
                           "value": normalizer.value(variable.get("value"), current_ranges),
                           "points_to": normalizer.object_address(variable.get("points_to"), current_ranges),
                           "extent": variable.get("size_bytes"),
                           "scope_status": variable.get("scope_status"),
                           "read_status": variable.get("read_status")})
            if variable.get("points_to") is not None:
                target = variable.get("target") or {}
                targets.append({"key": key, "kind": target.get("kind"),
                                "offset": target.get("offset_bytes"),
                                "base": normalizer.object_address(target.get("base_address"), current_ranges)})
        projection["frame0_values"].append(values)
        projection["global_values"].append(globals_values)
        projection["pointer_resolution"].append(targets)
    for event in data.get("heap_events", []):
        # Allocation IDs are stable within each execution; absolute allocator
        # addresses and process IDs are intentionally not semantic identity.
        projection["heap_events"].append({key: source_path(event.get(key)) if key == "source_file"
                                         else event.get(key) for key in
                                         ("sequence", "event_type", "allocation_id", "size_bytes",
                                          "source_file", "source_line", "old_allocation_id",
                                          "heap_usage_after", "detail")})
    projection["normalization"] = {
        "code_bases": normalizer.modules,
        "unresolved_absolute_addresses": sorted(normalizer.unknown_addresses),
        "byte_comparison": "not asserted: initialization/padding/pointer bytes may vary"}
    return projection


def refresh(directory):
    """Reprocess captured artifacts without executing the engine again."""
    for label in ("baseline", "compatibility", "expanded"):
        path = directory / label / "summary.json"
        if not path.exists():
            continue
        summary = json.loads(path.read_text())
        for sample in summary["samples"]:
            trace = pathlib.Path(sample["trace"])
            if not trace.exists():
                continue
            try:
                data = json.loads(trace.read_text())
                sample.update(metrics(data))
                projection = trace.with_suffix(".semantic.json")
                save(projection, semantic_projection(data))
                sample["semantic_projection"] = str(projection)
            except (ValueError, TypeError, KeyError) as error:
                sample["analysis_error"] = str(error)
        save(path, summary)


def probe_baseline(directory):
    """Actual CLI parsing probes cannot launch GDB: source is deliberately absent."""
    binary = directory / "baseline/c-trace-engine"
    results = []
    for key, value in EXPANDED.items():
        if key == "max_snapshot_memory_bytes":
            continue
        config = directory / "baseline" / ("probe-" + key + ".json")
        save(config, {"source": str(directory / "deliberately-absent.c"), "limits": {key: value}})
        result = subprocess.run([str(binary), "--config", str(config)], cwd=ROOT,
                                capture_output=True, text=True, timeout=10)
        results.append({"field": key, "returncode": result.returncode,
                        "stderr": result.stderr, "stdout": result.stdout,
                        "unknown_field_rejected": "unknown field `" + key + "`" in result.stderr})
    save(directory / "baseline/cli-probes.json", results)


def self_test(directory):
    """Pure-data checks; no compiler/debugger, no additional source files."""
    data = {"disassembly_table": [{"module_identity": "/tmp/session/program_debug:range=00400000-00402000:file_offset=00000000",
                                    "address": "0x400100"}],
            "instruction_events": [{"event_id": 0, "address": "0x400100", "next_address": "0x400104"}],
            "snapshots": [{"step_index": 0, "variables": [
                {"name": "x", "c_type": "int", "value": "7", "address": "0x7000", "size_bytes": 4},
                {"name": "p", "c_type": "int *", "value": "0x7000", "address": "0x7008",
                 "size_bytes": 8, "points_to": "0x7000"}]}]}
    shifted = json.loads(json.dumps(data))
    shifted["disassembly_table"][0].update({
        "module_identity": "/tmp/other/program_debug:range=00600000-00602000:file_offset=00000000",
        "address": "0x600100"})
    shifted["instruction_events"][0].update({"address": "0x600100", "next_address": "0x600104"})
    shifted["snapshots"][0]["variables"][0]["address"] = "0x8000"
    shifted["snapshots"][0]["variables"][1].update({"address": "0x8008", "points_to": "0x8000", "value": "0x8000"})
    before, after = semantic_projection(data), semantic_projection(shifted)
    for key in before:
        if key != "normalization":
            assert before[key] == after[key], "ASLR normalization: " + key
    shifted["snapshots"][0]["variables"][1].update({"points_to": "0x8001", "value": "0x8001"})
    assert before["frame0_values"] != semantic_projection(shifted)["frame0_values"], "interior pointer must differ"
    shifted["instruction_events"][0]["address"] = "0x600101"
    assert before["instructions"] != semantic_projection(shifted)["instructions"], "instruction offset must differ"
    normalizer = Normalizer(data)
    assert normalizer.object_address("0xdeadbeef", []) == "0xdeadbeef", "unknown pointer not hidden"
    assert differences([1, 2], [1, 3]), "difference examples"
    for label in ("baseline", "compatibility", "expanded"):
        path = directory / label / "summary.json"
        if not path.exists():
            continue
        for sample in json.loads(path.read_text())["samples"]:
            if "semantic_projection" in sample:
                value = json.loads(pathlib.Path(sample["semantic_projection"]).read_text())
                assert not differences(value, value), "self comparison"
    save(directory / "harness-self-test.json", {"status": "passed", "checks": [
        "object ASLR relocation", "instruction/module base relocation", "changed interior pointer detected",
        "changed instruction offset detected", "unknown addresses retained", "saved artifacts self-compare"]})
    print("Harness self-tests passed; no debugger execution", flush=True)


def differences(left, right, limit=12):
    """Bound retained examples, not the equality comparison itself."""
    found = []

    def visit(a, b, path):
        if a == b or len(found) >= limit:
            return
        if isinstance(a, dict) and isinstance(b, dict):
            for key in sorted(a.keys() | b.keys()):
                visit(a.get(key), b.get(key), path + "/" + key)
        elif isinstance(a, list) and isinstance(b, list):
            if len(a) != len(b):
                found.append({"path": path + "/length", "baseline": len(a), "candidate": len(b)})
            for index, (first, second) in enumerate(zip(a, b)):
                visit(first, second, path + "/" + str(index))
        else:
            found.append({"path": path, "baseline": a, "candidate": b})

    visit(left, right, "")
    return found


def compare(directory):
    refresh(directory)
    baseline_path = directory / "baseline/summary.json"
    has_phase3 = baseline_path.exists()
    baseline = json.loads((baseline_path if has_phase3 else directory / "compatibility/summary.json").read_text())
    report = {"method": "paired by workload/repetition; Phase3 baseline only when the original artifacts exist",
              "phase3_baseline_available": has_phase3,
              "limitations": json.loads((directory / "manifest.json").read_text())["limitations"],
              "comparisons": []}
    if not has_phase3:
        report["limitations"].append("Original Phase3 baseline executable/traces were lost from temporary storage between sessions; no Phase3-to-Phase4 delta is computed.")
    for label in (("compatibility", "expanded") if has_phase3 else ("expanded",)):
        path = directory / label / "summary.json"
        if not path.exists():
            continue
        candidate = json.loads(path.read_text())
        index = {(s["case"], s["repetition"]): s for s in candidate["samples"]}
        for original in baseline["samples"]:
            sample = index.get((original["case"], original["repetition"]))
            if sample is None:
                report["comparisons"].append({"case": original["case"], "suite": label,
                                               "error": "missing candidate sample"})
                continue
            row = {"case": original["case"], "suite": label,
                   "repetition": original["repetition"],
                   "identical_config": original["config_sha256"] == sample["config_sha256"],
                   "baseline_returncode": original.get("returncode"),
                   "candidate_returncode": sample.get("returncode"), "metrics": {}}
            for metric in ("total_commands", "commands_per_checkpoint", "frame_queries", "caller_queries", "memory_reads",
                           "capture_seconds", "debugger_seconds", "wall_seconds", "engine_peak_rss_kib",
                           "peak_waited_process_rss_kib", "trace_bytes", "checkpoints",
                           "stack_frame_observations", "caller_frame_observations", "variable_observations",
                           "requested_bytes", "captured_variable_bytes", "captured_heap_bytes",
                           "truncated_observations", "resolved_pointer_observations"):
                a, b = original.get(metric), sample.get(metric)
                row["metrics"][metric] = {"baseline": a, "candidate": b,
                                           "delta": b - a if a is not None and b is not None else None,
                                           "ratio": b / a if a not in (None, 0) and b is not None else None}
            row["dynamic_extents"] = {"baseline": original.get("array_extents"),
                                      "candidate": sample.get("array_extents")}
            if "semantic_projection" in original and "semantic_projection" in sample:
                first = json.loads(pathlib.Path(original["semantic_projection"]).read_text())
                second = json.loads(pathlib.Path(sample["semantic_projection"]).read_text())
                row["semantic"] = {
                    key: {"equal": first[key] == second[key],
                          "difference_examples": differences(first[key], second[key])}
                    for key in first if key != "normalization"}
                row["control_flow_equal"] = all(row["semantic"][key]["equal"]
                                                 for key in ("execution", "instructions", "checkpoints", "heap_events", "gaps", "output_bytes"))
                row["all_compared_semantics_equal"] = all(value["equal"] for value in row["semantic"].values())
                row["unresolved_absolute_addresses"] = {
                    "baseline": first["normalization"]["unresolved_absolute_addresses"],
                    "candidate": second["normalization"]["unresolved_absolute_addresses"]}
                row["semantic_status"] = ("control_flow_matches_with_observation_differences"
                                          if row["control_flow_equal"] and not row["all_compared_semantics_equal"]
                                          else "all_compared_semantics_match" if row["all_compared_semantics_equal"]
                                          else "control_flow_difference_requires_review")
            else:
                row["error"] = "missing semantic artifact (inspect run failure)"
            report["comparisons"].append(row)
    save(directory / "comparison.json", report)
    write_report(directory, baseline, report)
    return report


def write_report(directory, baseline, report):
    lines = ["# Phase 4 Benchmark", "", "Serial release runs, full pipeline, one run per case by default.",
             "No statistical significance or general performance claim is made.", "",
             "## Baseline Provenance", "",
              "- Comparison baseline: `" + ("Phase3" if report["phase3_baseline_available"] else "Phase4 compatibility") + "`",
              "- SHA-256: `" + baseline["provenance"]["sha256"] + "`",
              "- Executable mtime UTC: `" + baseline["provenance"]["mtime_utc"] + "`",
              "- Retained binary: `" + baseline["executable_copy"] + "`", "",
             "## Measurements", "",
             "| Suite | Case | Commands | Frame Queries | Memory Reads | Capture s | Debugger s | Wall s | Engine RSS KiB | Trace Bytes |",
             "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"]
    for label in ("baseline", "compatibility", "expanded"):
        path = directory / label / "summary.json"
        if not path.exists():
            continue
        summary = json.loads(path.read_text())
        for sample in summary["samples"]:
            cells = [label, sample["case"]]
            cells.extend(str(round(sample[k], 4)) if isinstance(sample.get(k), float)
                         else str(sample.get(k, "unavailable")) for k in
                         ("total_commands", "frame_queries", "memory_reads", "capture_seconds",
                          "debugger_seconds", "wall_seconds", "engine_peak_rss_kib", "trace_bytes"))
            lines.append("| " + " | ".join(cells) + " |")
    lines.extend(["", "## Evidence And Resource Cost", "",
                  "| Suite | Case | Commands/checkpoint | Caller queries | Caller frames | Variables | Requested bytes | Captured object bytes | Truncated objects | Resolved object pointers | Waited-process RSS KiB |",
                  "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"])
    for label in ("baseline", "compatibility", "expanded"):
        path = directory / label / "summary.json"
        if not path.exists():
            continue
        for sample in json.loads(path.read_text())["samples"]:
            values = ("commands_per_checkpoint", "caller_queries", "caller_frame_observations",
                      "variable_observations", "requested_bytes", "captured_variable_bytes",
                      "truncated_observations", "resolved_pointer_observations",
                      "peak_waited_process_rss_kib")
            cells = [label, sample["case"]]
            for key in values:
                value = sample.get(key)
                cells.append(str(round(value, 2)) if isinstance(value, float) else str(value if value is not None else "unavailable"))
            lines.append("| " + " | ".join(cells) + " |")
    lines.extend(["", "## Fidelity", "",
                  "Instruction addresses use per-module GDB disassembly base offsets. Object addresses use observed object/heap ranges; unresolved addresses remain visible.",
                  "Frame-0 values and pointer resolutions are compared separately from control flow. Raw byte equality is not asserted because initialization and padding cannot be established.", ""])
    if not report["comparisons"]:
        lines.append("Final has not been captured; no baseline/final fidelity or performance claim is available.")
    for row in report["comparisons"]:
        lines.append("- " + row["suite"] + "/" + row["case"] + ": control flow="
                     + str(row.get("control_flow_equal", "unavailable")) + ", all compared semantics="
                     + str(row.get("all_compared_semantics_equal", "unavailable"))
                     + ", identical config=" + str(row.get("identical_config", False)))
    lines.extend(["", "## Capture Outcomes", ""])
    for label in ("baseline", "compatibility", "expanded"):
        path = directory / label / "summary.json"
        if not path.exists():
            continue
        samples = json.loads(path.read_text())["samples"]
        for sample in samples:
            failed_stages = {name: stage.get("status") for name, stage in (sample.get("stage_results") or {}).items()
                             if stage.get("status") in ("failed", "timed_out", "cancelled")}
            lines.append("- " + label + "/" + sample["case"] + ": CLI=" + str(sample.get("returncode"))
                         + ", inferior=" + str((sample.get("execution") or {}).get("exit_code"))
                         + ", outcome=" + str(sample.get("execution_status", "unavailable"))
                         + (", failed stages=" + json.dumps(failed_stages) if failed_stages else "")
                         + (", error=" + sample["error"] if "error" in sample else ""))
    lines.extend(["", "## Limitations", ""])
    lines.extend("- " + note for note in report["limitations"])
    lines.extend(["- Phase 3 lacks explicit requested/captured byte evidence: requested bytes and evidence truncation counts are null, not zero.",
                  "- Array extents are observed GDB sizes, not inferred DWARF VLA bounds.",
                   "- Existing aggregate fixture exits 1 and pointer_identity exits 45 in both runs; CLI success is not synonymous with inferior exit zero. Exact outcomes and stage failures remain in summary JSON.",
                  "- Full trace, stderr, exact commands, stages, configuration hashes and per-metric deltas are retained in JSON artifacts.", ""])
    (directory / "report.md").write_text("\n".join(lines))


def run_suite(binary, directory, label, manifest, repetitions, expanded=False):
    output = directory / label
    if output.exists():
        raise RuntimeError("Refusing to overwrite captured suite: " + str(output))
    output.mkdir()
    original = provenance(binary)
    executable = output / "c-trace-engine"
    shutil.copy2(binary, executable)
    help_result = subprocess.run([str(executable), "--help"], capture_output=True, text=True)
    (output / "cli-help.txt").write_text(help_result.stdout + help_result.stderr)
    summary = {"label": label, "provenance": original, "executable_copy": str(executable),
               "repetitions": repetitions, "expanded": expanded, "samples": []}
    if digest(executable) != original["sha256"]:
        raise RuntimeError("Executable changed while copying; refusing mixed provenance")
    save(output / "summary.json", summary)
    for case in manifest["cases"]:
        config = pathlib.Path(case["config"])
        if expanded:
            payload = json.loads(config.read_text())
            payload["limits"].update(EXPANDED)
            config = output / (case["name"] + ".config.json")
            save(config, payload)
        for repetition in range(repetitions):
            name = case["name"] + "-" + str(repetition)
            trace = output / (name + ".json")
            timing = output / (name + ".time")
            stderr = output / (name + ".stderr")
            command = ["/usr/bin/time", "-f", "%e %M", "-o", str(timing),
                       str(executable), "--config", str(config), "--output", str(trace)]
            start = time.monotonic()
            sample = {"case": case["name"], "repetition": repetition,
                      "config": str(config), "config_sha256": digest(config),
                      "trace": str(trace), "stderr": str(stderr), "command": command}
            with stderr.open("w") as errors:
                try:
                    process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.DEVNULL,
                                               stderr=errors, start_new_session=True)
                    try:
                        sample["returncode"] = process.wait(timeout=240)
                    except subprocess.TimeoutExpired:
                        # Kill the benchmark group, not only the time wrapper.
                        os.killpg(process.pid, signal.SIGTERM)
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait()
                        raise
                except subprocess.TimeoutExpired:
                    sample["error"] = "harness timeout after 240 seconds"
            sample["wall_seconds"] = time.monotonic() - start
            if timing.exists():
                fields = timing.read_text().splitlines()[-1].split()
                if len(fields) == 2:
                    try:
                        sample["gnu_time_wall_seconds"] = float(fields[0])
                        sample["peak_waited_process_rss_kib"] = int(fields[1])
                    except ValueError:
                        sample["time_parse_error"] = timing.read_text()
            if trace.exists():
                sample["trace_bytes"] = trace.stat().st_size
                try:
                    sample.update(metrics(json.loads(trace.read_text())))
                except (ValueError, TypeError, KeyError) as error:
                    sample["trace_parse_error"] = str(error)
            else:
                sample["error"] = sample.get("error", "no trace emitted")
            summary["samples"].append(sample)
            save(output / "summary.json", summary)
            print(label, name, "rc=", sample.get("returncode"),
                  "wall=", round(sample["wall_seconds"], 3), flush=True)
    summary["median_wall_seconds"] = {
        case["name"]: statistics.median(s["wall_seconds"] for s in summary["samples"]
                                       if s["case"] == case["name"])
        for case in manifest["cases"]}
    save(output / "summary.json", summary)
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=["baseline", "final", "current", "compare", "analyze", "probe-baseline", "self-test"])
    parser.add_argument("--binary", type=pathlib.Path, default=ROOT / "target/release/c-trace-engine")
    parser.add_argument("--directory", type=pathlib.Path, default=pathlib.Path("/tmp/kilo/phase4"))
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--final-authorized", action="store_true",
                        help="Coordinator confirmed release rebuild and no concurrent tests")
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("repetitions must be positive")
    directory = args.directory.resolve()
    manifest = prepare(directory)
    if args.stage == "baseline":
        actual = provenance(args.binary)
        if not actual["mtime_utc"].startswith(EXPECTED_BASELINE_MTIME):
            parser.error("baseline binary timestamp does not match required pre-Phase4 release")
        run_suite(args.binary.resolve(), directory, "baseline", manifest, args.repetitions)
        refresh(directory)
    elif args.stage == "final":
        if not args.final_authorized:
            parser.error("final requires --final-authorized after coordinator notice")
        if not (directory / "baseline/summary.json").exists():
            parser.error("capture baseline first")
        run_suite(args.binary.resolve(), directory, "compatibility", manifest, args.repetitions)
        # Both final suites use the same pinned executable, even if the
        # workspace release binary is replaced later by another participant.
        run_suite(directory / "compatibility/c-trace-engine", directory, "expanded", manifest, args.repetitions, True)
        compare(directory)
    elif args.stage == "current":
        run_suite(args.binary.resolve(), directory, "compatibility", manifest, args.repetitions)
        run_suite(directory / "compatibility/c-trace-engine", directory, "expanded", manifest, args.repetitions, True)
        compare(directory)
    elif args.stage == "analyze":
        refresh(directory)
    elif args.stage == "probe-baseline":
        probe_baseline(directory)
    elif args.stage == "self-test":
        self_test(directory)
    else:
        compare(directory)


if __name__ == "__main__":
    main()

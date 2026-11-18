#!/usr/bin/env python3
"""Serial release-build AST on/off benchmark; never builds or alters the engine.

Two repetitions per case by default; inputs/configuration are identical within
each pair. Retain the raw traces, stdout, and per-run measurements.
"""
import argparse
import json
import pathlib
import statistics
import subprocess
import time

import phase5_benchmark as semantic

CASES = {
    "arithmetic": ("int main(void) { int a=3; return a-3; }\n", None),
    "pointer": ("int main(void) { int x=2,y=0; int *p=&x; p=&y; return *p; }\n", None),
    "aggregates": ("struct P {int x,y;}; int main(void) {struct P p={1,2};int a[3]={0};a[1]=p.x;return a[1]-1;}\n", None),
    "heap": ("#include <stdlib.h>\nint main(void) {int *p=malloc(sizeof(int));if(!p)return 1;*p=1;free(p);return 0;}\n", None),
    "realloc": ("#include <stdlib.h>\nint main(void) {int *p=malloc(sizeof(int));if(!p)return 1;for(int n=2;n<6;n++){int *q=realloc(p,n*sizeof(int));if(!q){free(p);return 1;}p=q;}free(p);return 0;}\n", None),
    "nested": ("int main(void) {int x=0;{int x=1;{int x=2; x++;} x++;}return x;}\n", None),
    "multiple_tu": ("extern int helper(void); int main(void){return helper()-3;}\n",
                    "static int x; int helper(void){x=3;return x;}\n"),
    "macro": ("#define ADD(X) ((X)+1)\n#define INC(X) ((X)=ADD(X))\nint main(void){int a=0;INC(a);return a-1;}\n", None),
    "gnu": ("int main(void) {int x=({int y=2;y+1;});return x-3;}\n", None),
    "headers": ("#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <stdint.h>\nint main(void){char *p=malloc(16);if(!p)return 1;memset(p,0,16);int n=(int)strlen(p);free(p);return n;}\n", None),
}


def metrics(data):
    registry = data.get("source_metadata") or {}
    units = registry.get("translation_units", [])
    profile = data.get("profiling", {})
    return {
        "ast_status": [item["status"] for item in units],
        "ast_output_bytes": sum(item.get("ast_bytes", 0) for item in units),
        "ast_parse_us": sum(item.get("parse_time_us", 0) for item in units),
        "ast_registry_us": sum(item.get("registry_time_us", 0) for item in units),
        "ast_declaration_correlation_us": registry.get("declaration_correlation_time_us"),
        "ast_heap_correlation_us": registry.get("heap_correlation_time_us"),
        "ast_invocation_and_extraction_us": profile.get("clang_ast_time_us"),
        "nodes_visited": sum(item.get("nodes_visited", 0) for item in units),
        "declarations": len(registry.get("declarations", [])),
        "expressions": len(registry.get("expressions", [])),
        "heap_candidates": len(registry.get("heap_call_candidates", [])),
        "engine_peak_rss_kib": profile.get("engine_peak_rss_kib"),
        "gdb_commands": (data.get("resource_usage") or {}).get("gdb_commands_issued"),
        "snapshots": len(data.get("snapshots", [])),
        "instructions": len(data.get("instruction_events", [])),
        "heap_events": len(data.get("heap_events", [])),
        "execution": data.get("execution", {}).get("outcome"),
        "exit_code": data.get("execution", {}).get("exit_code"),
        "checkpoint_indices": data.get("source_checkpoints", []),
        "event_shape": [
            [e.get(k) for k in ("sequence", "event_type", "allocation_id",
                                "old_allocation_id", "size_bytes", "source_line")]
            for e in data.get("heap_events", [])
        ],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--directory", type=pathlib.Path, default=pathlib.Path("/tmp/opencode/p6-benchmark"))
    parser.add_argument("--repeats", type=int, default=2)
    parser.add_argument("--compare-only", action="store_true",
                        help="recompute comparisons from an existing completed suite")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("repeats must be positive")
    if args.compare_only:
        results = json.loads((args.directory / "summary.json").read_text())
    else:
        if args.directory.exists():
            parser.error("refusing to overwrite existing benchmark directory")
        args.directory.mkdir(parents=True)
        results = []
    for name, (primary, extra) in ([] if args.compare_only else CASES.items()):
        source = args.directory / (name + ".c")
        source.write_text(primary)
        other = args.directory / (name + "_other.c")
        if extra:
            other.write_text(extra)
        for repeat in range(args.repeats):
            for enabled in (False, True):
                tag = f"{name}-{repeat}-{'on' if enabled else 'off'}"
                trace = args.directory / (tag + ".json")
                command = [str(args.binary.resolve()), "--source", str(source),
                           "--output", str(trace), "--no-pretty", "--timeout", "35",
                           "--max-steps", "350", "--sanitizer-trigger", "never"]
                if extra:
                    command += ["--add-source", str(other)]
                if enabled:
                    command.append("--clang-ast")
                begin = time.monotonic()
                run = subprocess.run(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                     timeout=55, check=False)
                elapsed = time.monotonic() - begin
                (args.directory / (tag + ".stderr")).write_bytes(run.stderr)
                if run.returncode != 0 or not trace.exists():
                    raise RuntimeError(f"{tag} failed ({run.returncode}): {run.stderr[:500]!r}")
                data = json.loads(trace.read_text())
                item = {"case": name, "repeat": repeat, "ast_enabled": enabled,
                        "wall_seconds": elapsed, "trace_bytes": trace.stat().st_size,
                        **metrics(data)}
                results.append(item)
                print(tag, "wall", round(elapsed, 2), "ast", item["ast_status"], flush=True)
                (args.directory / "summary.json").write_text(json.dumps(results, indent=2))
    comparisons = []
    for name in CASES:
        pairs = []
        for repeat in range(args.repeats):
            off, on = [next(item for item in results if item["case"] == name
                            and item["repeat"] == repeat and item["ast_enabled"] == state)
                       for state in (False, True)]
            invariant = ("gdb_commands", "snapshots", "instructions", "heap_events",
                         "execution", "exit_code", "checkpoint_indices", "event_shape")
            projections = [
                semantic.projection(json.loads((args.directory / f"{name}-{repeat}-{state}.json").read_text()))
                for state in ("off", "on")
            ]
            projection_keys = [key for key in projections[0] if key != "unresolved"]
            projection_differences = [
                key for key in projection_keys if projections[0][key] != projections[1][key]
            ]
            pairs.append({"runtime_equal": all(off[key] == on[key] for key in invariant),
                          "differences": [key for key in invariant if off[key] != on[key]],
                          "semantic_projection_equal": not projection_differences,
                          "semantic_projection_differences": projection_differences,
                          "unresolved_addresses": [x["unresolved"] for x in projections],
                          "wall_delta_seconds": on["wall_seconds"] - off["wall_seconds"],
                          "trace_delta_bytes": on["trace_bytes"] - off["trace_bytes"],
                          "rss_delta_kib": (on["engine_peak_rss_kib"] or 0)
                                           - (off["engine_peak_rss_kib"] or 0),
                          "ast_time_us": on["ast_invocation_and_extraction_us"]})
        comparisons.append({"case": name, "pairs": pairs,
                            "median_wall_delta_seconds": statistics.median(p["wall_delta_seconds"] for p in pairs),
                            "median_trace_delta_bytes": statistics.median(p["trace_delta_bytes"] for p in pairs),
                            "median_ast_time_us": statistics.median(p["ast_time_us"] for p in pairs)})
    (args.directory / "comparison.json").write_text(json.dumps(comparisons, indent=2))
    if not all(pair["runtime_equal"] and pair["semantic_projection_equal"]
               for c in comparisons for pair in c["pairs"]):
        raise RuntimeError("paired runtime projection differs; inspect comparison.json")


if __name__ == "__main__":
    main()

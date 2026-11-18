#!/usr/bin/env python3
"""Serial Phase 7 release benchmark: AST on/off fusion and evidence fidelity.

The script never builds or alters the engine. Each pair runs the same source,
configuration, limits and repetition with only `--clang-ast` toggled. It keeps
raw traces, stderr and per-run measurements, then checks:

* runtime/semantic projection equality (AST must not change runtime evidence),
* every Phase 7 reference resolves inside the serialized registries,
* summary counters equal independently counted links,
* AST-off traces remain valid and report `ast_available: false`.
"""
import argparse
import json
import pathlib
import statistics
import subprocess
import time

import phase5_benchmark as semantic

CASES = {
    "arithmetic": {
        "primary": "int main(void) { int a=3; return a-3; }\n",
    },
    "pointer": {
        "primary": "int main(void) { int x=2,y=0; int *p=&x; p=&y; return *p; }\n",
    },
    "aggregates": {
        "primary": "struct P {int x,y;}; int main(void) {struct P p={1,2};int a[3]={0};a[1]=p.x;return a[1]-1;}\n",
    },
    "members": {
        "primary": ("#include <stdlib.h>\n"
                    "struct Pair { int left; int right; };\n"
                    "int main(void) { struct Pair p = {1, 2}; int *q = malloc(sizeof *q);\n"
                    "if (!q) return 1; *q = 3; p.left = *q; free(q); return p.left + p.right - 5; }\n"),
    },
    "heap": {
        "primary": ("#include <stdlib.h>\n"
                    "int main(void) {int *p=malloc(sizeof(int));if(!p)return 1;*p=1;free(p);return 0;}\n"),
    },
    "ambiguous_heap": {
        "primary": ("#include <stdlib.h>\n"
                    "int main(void) {int *r=malloc(8), *s=malloc(16); if(!r||!s){free(r);free(s);return 1;}\n"
                    "r[0]=1; s[0]=2; free(r); free(s); return 0;}\n"),
    },
    "realloc": {
        "primary": ("#include <stdlib.h>\n"
                    "int main(void) {int *p=malloc(sizeof(int));if(!p)return 1;for(int n=2;n<6;n++){int *q=realloc(p,n*sizeof(int));if(!q){free(p);return 1;}p=q;}free(p);return 0;}\n"),
    },
    "nested": {
        "primary": "int main(void) {int x=0;{int x=1;{int x=2; x++;} x++;}return x;}\n",
    },
    "multiple_tu": {
        "primary": "extern int helper(void); int main(void){return helper()-3;}\n",
        "extra": ["static int x; int helper(void){x=3;return x;}\n"],
    },
    "macro": {
        "primary": ("#define ADD(X) ((X)+1)\n#define INC(X) ((X)=ADD(X))\n"
                    "int main(void){int a=0;INC(a);return a-1;}\n"),
    },
    "macro_alloc": {
        "primary": ("#include <stdlib.h>\n#define TAKE(N) malloc((N) * sizeof(int))\n"
                    "int main(void){int *p=TAKE(4); if(!p) return 1; p[0]=7; free(p); return p[0]-7;}\n"),
    },
    "gnu": {
        "primary": "int main(void) {int x=({int y=2;y+1;});return x-3;}\n",
    },
    "headers": {
        "primary": ("#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n#include <stdint.h>\n"
                    "int main(void){char *p=malloc(16);if(!p)return 1;memset(p,0,16);int n=(int)strlen(p);free(p);return n;}\n"),
    },
}


def load(path):
    return json.loads(path.read_text())


def metrics(data):
    registry = data.get("source_metadata") or {}
    units = registry.get("translation_units", [])
    profile = data.get("profiling") or {}
    fusion = data.get("evidence_fusion") or {}
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
        "heap_ambiguities": len(registry.get("heap_call_ambiguities", [])),
        "fusion": fusion,
        "fusion_time_us": profile.get("evidence_fusion_time_us"),
        "evidence_validation_time_us": profile.get("evidence_validation_time_us"),
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


def valid_member(member, types):
    parent = types.get(member.get("type_ref"))
    if parent is None or parent.get("kind") not in ("struct", "union"):
        return False
    index = member.get("member_index")
    members = parent.get("members") or []
    if not isinstance(index, int) or not 0 <= index < len(members):
        return False
    selected = members[index]
    return (selected.get("name") == member.get("name")
            and selected.get("byte_offset") == member.get("byte_offset"))


def audit_links(data):
    """Independent Phase 7 reference audit; returns (errors, counted links)."""
    errors = []
    registry = data.get("source_metadata") or {}
    dwarf = data.get("type_registry") or {}
    fusion = data.get("evidence_fusion") or {}
    declarations = {item.get("id"): item for item in registry.get("declarations", [])}
    expressions = {item.get("id"): item for item in registry.get("expressions", [])}
    dwarf_declarations = {item.get("id") for item in dwarf.get("declarations", [])}
    types = {item.get("id"): item for item in dwarf.get("types", [])}
    counted = {
        "declaration_links": 0,
        "declaration_line_fallbacks": 0,
        "declaration_type_links": 0,
        "member_links": 0,
        "expression_dwarf_links": 0,
        "expression_type_links": 0,
        "heap_call_candidates": len(registry.get("heap_call_candidates", [])),
        "heap_call_ambiguities": len(registry.get("heap_call_ambiguities", [])),
        "runtime_source_links": 0,
        "runtime_variables_examined": 0,
    }
    for declaration in registry.get("declarations", []):
        if declaration.get("dwarf_declaration") is not None:
            counted["declaration_links"] += 1
            if declaration["dwarf_declaration"] not in dwarf_declarations:
                errors.append("declaration dwarf_declaration missing")
        if declaration.get("correlation") == "unique_line_with_missing_column":
            counted["declaration_line_fallbacks"] += 1
        if declaration.get("dwarf_type_ref") is not None:
            counted["declaration_type_links"] += 1
            if declaration["dwarf_type_ref"] not in types:
                errors.append("declaration dwarf_type_ref missing")
        if declaration.get("dwarf_member") is not None:
            counted["member_links"] += 1
            if not valid_member(declaration["dwarf_member"], types):
                errors.append("declaration dwarf_member invalid")
    for expression in registry.get("expressions", []):
        for operand in expression.get("operand_ids") or []:
            if operand not in expressions:
                errors.append("expression operand missing")
        referenced = expression.get("referenced_declaration")
        if referenced is not None and referenced not in declarations:
            errors.append("expression referenced_declaration missing")
        if expression.get("referenced_dwarf_declaration") is not None:
            counted["expression_dwarf_links"] += 1
            if expression["referenced_dwarf_declaration"] not in dwarf_declarations:
                errors.append("expression dwarf declaration missing")
        if expression.get("dwarf_type_ref") is not None:
            counted["expression_type_links"] += 1
            if expression["dwarf_type_ref"] not in types:
                errors.append("expression dwarf_type_ref missing")
        if expression.get("member_ref") is not None:
            counted["member_links"] += 1
            if not valid_member(expression["member_ref"], types):
                errors.append("expression member_ref invalid")
    events = data.get("heap_events") or []
    for candidate in registry.get("heap_call_candidates", []):
        if not 0 <= candidate.get("event_index", -1) < len(events):
            errors.append("heap candidate event missing")
        if candidate.get("expression_id") not in expressions:
            errors.append("heap candidate expression missing")
    for ambiguity in registry.get("heap_call_ambiguities", []):
        if not 0 <= ambiguity.get("event_index", -1) < len(events):
            errors.append("heap ambiguity event missing")
        if not ambiguity.get("expression_ids"):
            errors.append("heap ambiguity has no expressions")
        for expression_id in ambiguity.get("expression_ids") or []:
            if expression_id not in expressions:
                errors.append("heap ambiguity expression missing")

    def observe(variables):
        for variable in variables:
            counted["runtime_variables_examined"] += 1
            if variable.get("source_declaration_ref") is None:
                continue
            counted["runtime_source_links"] += 1
            declaration = declarations.get(variable.get("source_declaration_ref"))
            if declaration is None:
                errors.append("runtime source declaration missing")
            elif declaration.get("dwarf_declaration") != variable.get("declaration_ref"):
                errors.append("runtime source bridge is not the exact DWARF identity")

    for snapshot in data.get("snapshots", []):
        observe(snapshot.get("variables") or [])
        observe((snapshot.get("observation") or {}).get("globals") or [])
        for frame in snapshot.get("caller_frames") or []:
            observe(frame.get("variables") or [])

    for key, value in counted.items():
        if fusion.get(key) != value:
            errors.append(f"summary {key}={fusion.get(key)} but counted {value}")
    if fusion.get("rejected_links") != 0:
        errors.append(f"rejected_links={fusion.get('rejected_links')}")
    return errors, counted


def setup_case(directory, name, case):
    source = directory / (name + ".c")
    source.write_text(case["primary"])
    extras = []
    for index, text in enumerate(case.get("extra", [])):
        extra = directory / f"{name}_extra{index}.c"
        extra.write_text(text)
        extras.append(extra)
    return source, extras


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--directory", type=pathlib.Path,
                        default=pathlib.Path("/tmp/opencode/p7-benchmark"))
    parser.add_argument("--repeats", type=int, default=2)
    parser.add_argument("--cases", default=",".join(CASES),
                        help="comma-separated case subset")
    parser.add_argument("--compare-only", action="store_true",
                        help="recompute comparisons from an existing completed suite")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("repeats must be positive")
    selected = [item.strip() for item in args.cases.split(",") if item.strip()]
    unknown = [item for item in selected if item not in CASES]
    if unknown:
        parser.error("unknown cases: " + ", ".join(unknown))
    if args.compare_only:
        results = load(args.directory / "summary.json")
    else:
        if args.directory.exists():
            parser.error("refusing to overwrite existing benchmark directory")
        args.directory.mkdir(parents=True)
        results = []
    for name in selected:
        if args.compare_only:
            continue
        case = CASES[name]
        source, extras = setup_case(args.directory, name, case)
        for repeat in range(args.repeats):
            for enabled in (False, True):
                tag = f"{name}-{repeat}-{'on' if enabled else 'off'}"
                trace = args.directory / (tag + ".json")
                command = [str(args.binary.resolve()), "--source", str(source),
                           "--output", str(trace), "--no-pretty", "--timeout", "35",
                           "--max-steps", "350", "--sanitizer-trigger", "never"]
                for extra in extras:
                    command += ["--add-source", str(extra)]
                if enabled:
                    command.append("--clang-ast")
                begin = time.monotonic()
                run = subprocess.run(command, stdout=subprocess.DEVNULL,
                                     stderr=subprocess.PIPE, timeout=55, check=False)
                elapsed = time.monotonic() - begin
                (args.directory / (tag + ".stderr")).write_bytes(run.stderr)
                if run.returncode != 0 or not trace.exists():
                    raise RuntimeError(f"{tag} failed ({run.returncode}): {run.stderr[:500]!r}")
                data = load(trace)
                errors, counted = audit_links(data)
                if errors:
                    raise RuntimeError(f"{tag} evidence audit failed: {errors[:10]}")
                fusion = data.get("evidence_fusion") or {}
                if enabled and not fusion.get("ast_available"):
                    raise RuntimeError(f"{tag} did not report ast_available")
                if not enabled and (fusion.get("ast_available") or data.get("source_metadata")):
                    raise RuntimeError(f"{tag} unexpectedly contains AST evidence")
                item = {"case": name, "repeat": repeat, "ast_enabled": enabled,
                        "wall_seconds": elapsed, "trace_bytes": trace.stat().st_size,
                        "link_counts": counted, **metrics(data)}
                results.append(item)
                print(tag, "wall", round(elapsed, 2), "ast", item["ast_status"],
                      "links", counted["declaration_links"], flush=True)
                (args.directory / "summary.json").write_text(json.dumps(results, indent=2))

    comparisons = []
    for name in selected:
        pairs = []
        for repeat in range(args.repeats):
            off, on = [next(item for item in results if item["case"] == name
                            and item["repeat"] == repeat and item["ast_enabled"] == state)
                       for state in (False, True)]
            invariant = ("gdb_commands", "snapshots", "instructions", "heap_events",
                         "execution", "exit_code", "checkpoint_indices", "event_shape")
            projections = [
                semantic.projection(load(args.directory / f"{name}-{repeat}-{state}.json"))
                for state in ("off", "on")
            ]
            projection_keys = [key for key in projections[0] if key != "unresolved"]
            projection_differences = [
                key for key in projection_keys if projections[0][key] != projections[1][key]
            ]
            pairs.append({
                "runtime_equal": all(off[key] == on[key] for key in invariant),
                "differences": [key for key in invariant if off[key] != on[key]],
                "semantic_projection_equal": not projection_differences,
                "semantic_projection_differences": projection_differences,
                "unresolved_addresses": [projection["unresolved"] for projection in projections],
                "wall_delta_seconds": on["wall_seconds"] - off["wall_seconds"],
                "trace_delta_bytes": on["trace_bytes"] - off["trace_bytes"],
                "rss_delta_kib": (on["engine_peak_rss_kib"] or 0)
                                 - (off["engine_peak_rss_kib"] or 0),
                "ast_time_us": on["ast_invocation_and_extraction_us"],
                "fusion_time_us": on["fusion_time_us"],
                "evidence_validation_time_us": on["evidence_validation_time_us"],
                "fusion": on["fusion"],
            })
        comparisons.append({
            "case": name,
            "pairs": pairs,
            "median_wall_delta_seconds": statistics.median(
                pair["wall_delta_seconds"] for pair in pairs),
            "median_trace_delta_bytes": statistics.median(
                pair["trace_delta_bytes"] for pair in pairs),
            "median_ast_time_us": statistics.median(
                pair["ast_time_us"] or 0 for pair in pairs),
            "median_fusion_time_us": statistics.median(
                pair["fusion_time_us"] or 0 for pair in pairs),
        })
    (args.directory / "comparison.json").write_text(json.dumps(comparisons, indent=2))
    if not all(pair["runtime_equal"] and pair["semantic_projection_equal"]
               and not pair["differences"] and not pair["semantic_projection_differences"]
               for comparison in comparisons for pair in comparison["pairs"]):
        raise RuntimeError("paired runtime projection differs; inspect comparison.json")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Generate the DOC-1 Stage 1 source inventory; no third-party dependencies.

This is a lexical index, not a Rust/C parser or a behavioral audit. Run from any
working directory: python3 docs/stage1-html/inventory.py [--check].
"""

import argparse
import bisect
import json
import re
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
OUTPUT = Path(__file__).resolve().parent / "assets/data/stage1-inventory.json"
GRAPH = Path(__file__).resolve().parent / "assets/data/verified-relationships.json"

# Deliberately curated, not inferred from filenames or source-syntax semantics.
# Tuple: subsystem, role, responsibility, coarse side-effect category.
ROLES = {
    "Cargo.toml": ("build", "manifest", "Package metadata and external dependency declarations.", "configuration"),
    "c_runtime/alloc_tracker.c": ("engine/heap", "instrumentation runtime", "Wrapped C allocators, allocation bookkeeping, and event/status file emission.", "external_io_and_state"),
    "src/lib.rs": ("crate", "library root", "Declares library modules and re-exports selected entry points.", "declarations"),
    "src/main.rs": ("engine/cli", "binary entry point", "Parses CLI input, runs a trace, and exports its JSON result.", "external_io_and_state"),
    "src/allocation_tracker.rs": ("engine/heap", "heap event ingestion", "Writes the C runtime source, reads allocation events, reconstructs heap history, and resolves caller sites.", "external_io_and_state"),
    "src/clang_ast.rs": ("engine/metadata", "Clang AST extraction", "Collects optional static source declarations/expressions and correlates candidate metadata.", "external_io_and_state"),
    "src/cli.rs": ("engine/cli", "command-line configuration", "Defines CLI arguments and translates them into trace configuration.", "configuration_and_state"),
    "src/compiler.rs": ("engine/build", "C compiler integration", "Invokes C builds and parses compiler diagnostic outputs.", "external_io_and_state"),
    "src/config.rs": ("engine/config", "configuration model", "Defines and loads build, tracing, and execution configuration.", "configuration_and_state"),
    "src/dwarf.rs": ("engine/metadata", "DWARF metadata extraction", "Indexes debug information and associates recorded observations with type/declaration metadata.", "read_and_transform"),
    "src/error.rs": ("engine/core", "error types", "Defines engine error and result types.", "declarations"),
    "src/evidence/mod.rs": ("engine/evidence", "evidence fusion coordinator", "Coordinates bounded links across recorded runtime, AST, and DWARF evidence.", "in_memory_state"),
    "src/evidence/declarations.rs": ("engine/evidence", "declaration links", "Indexes DWARF declarations and links uniquely matched source members.", "in_memory_state"),
    "src/evidence/heap.rs": ("engine/evidence", "heap/source candidates", "Records bounded heap-event to static allocator-call candidates and ambiguities.", "in_memory_state"),
    "src/evidence/source_links.rs": ("engine/evidence", "source/observation links", "Links source expressions and qualified runtime observations to recorded declarations.", "in_memory_state"),
    "src/evidence/validation.rs": ("engine/evidence", "link audit", "Audits constructed cross-source references and clears invalid links.", "in_memory_state"),
    "src/gdb_mi.rs": ("engine/debugger", "GDB/MI session", "Manages debugger subprocess communication, commands, and captured state.", "external_io_and_state"),
    "src/json_export.rs": ("engine/export", "bounded JSON export", "Preflights serialization and writes final JSON with size and cancellation checks.", "external_io_and_state"),
    "src/memory.rs": ("engine/memory", "pointer enrichment", "Reads process maps/load bias and qualifies checkpoint pointer address candidates against observed extents.", "read_and_transform"),
    "src/mi_parser.rs": ("engine/debugger", "MI syntax parser", "Parses GDB machine-interface output into structured protocol records.", "read_and_transform"),
    "src/models.rs": ("engine/schema", "trace schema", "Defines serializable trace records and evidence/status fields.", "declarations_and_transform"),
    "src/processor.rs": ("engine/validation", "optional structural validator", "Checks recording instruction IDs, event references and coverage structure; not invoked by the engine production assembly path.", "read_and_transform"),
    "src/recorder.rs": ("engine/recording", "trace recording", "Records bounded tracing events and session journal data.", "external_io_and_state"),
    "src/resource_limits.rs": ("engine/limits", "resource enforcement", "Defines execution limits and applies operating-system resource controls.", "external_io_and_state"),
    "src/sanitizer.rs": ("engine/diagnostics", "sanitizer runner", "Runs a separate sanitizer diagnostic pass and captures its report.", "external_io_and_state"),
    "src/storage.rs": ("engine/storage", "bounded JSONL journal", "Writes capped append-only record journals and reads recoverable complete lines.", "external_io_and_state"),
    "src/supervisor.rs": ("engine/process", "process supervision", "Supervises execution and termination of child processes.", "external_io_and_state"),
    "src/trace.rs": ("engine/orchestration", "trace coordinator", "Coordinates compilation, GDB capture, heap reconstruction, optional metadata, diagnostics, and export-ready trace assembly.", "external_io_and_state"),
    "src/valgrind.rs": ("engine/diagnostics", "Valgrind diagnostics", "Runs independent Valgrind analysis and parses its XML report.", "external_io_and_state"),
    "src/trace_parser/mod.rs": ("parser/core", "parser root", "Loads a canonical immutable trace, exposes bounded queries, and coordinates parser modules.", "read_and_transform"),
    "src/trace_parser/consumer.rs": ("parser/consumer", "bounded consumer views", "Composes checkpoint summaries and scoped, paged consumer evidence.", "read_only_query"),
    "src/trace_parser/error.rs": ("parser/core", "parser errors", "Defines ingestion and query error types.", "declarations"),
    "src/trace_parser/evidence.rs": ("parser/evidence", "evidence references", "Defines trace-local evidence locations and resolves borrowed references.", "read_only_query"),
    "src/trace_parser/indexes.rs": ("parser/indexes", "position indexes", "Builds position-only indexes and supports indexed trace navigation.", "in_memory_state"),
    "src/trace_parser/journal.rs": ("parser/ingestion", "journal records", "Parses bounded JSONL journal records without reconstructing a full trace.", "read_and_transform"),
    "src/trace_parser/json.rs": ("parser/ingestion", "JSON ingestion", "Detects input format and decodes bounded JSON trace documents.", "read_and_transform"),
    "src/trace_parser/semantic.rs": ("parser/semantic", "validated evidence views", "Exposes qualified borrowed evidence after explicit deep validation.", "read_only_query"),
    "src/trace_parser/validation.rs": ("parser/validation", "deep validation", "Audits imported trace relationships and reports structural/partial findings.", "read_and_transform"),
    "src/trace_parser/views.rs": ("parser/views", "borrowed trace views", "Exposes raw and semantic-input views over the canonical trace.", "read_only_query"),
}

EXTERNAL_INTERFACES = {
    "src/main.rs": ["CLI arguments", "stdout", "filesystem"],
    "src/trace.rs": ["GCC", "GDB/MI2", "addr2line", "Valgrind (optional)", "sanitizer executable (conditional)"],
    "src/compiler.rs": ["GCC subprocess", "compiler diagnostics"],
    "src/gdb_mi.rs": ["GDB/MI2 subprocess", "PTY", "inferior process"],
    "src/mi_parser.rs": ["GDB/MI2 protocol records"],
    "src/allocation_tracker.rs": ["allocation event/status files", "addr2line subprocess"],
    "c_runtime/alloc_tracker.c": ["GNU ld --wrap allocator hooks", "event/status file descriptors", "pthread_atfork"],
    "src/dwarf.rs": ["ELF/DWARF debug artifact"],
    "src/clang_ast.rs": ["Clang JSON AST subprocess (optional)"],
    "src/valgrind.rs": ["Valgrind Memcheck subprocess", "native XML report"],
    "src/sanitizer.rs": ["ASan/UBSan executable", "bounded stderr"],
    "src/resource_limits.rs": ["Linux prlimit", "cgroup v2 (best effort)"],
    "src/memory.rs": ["/proc/PID/maps", "ELF load bias"],
    "src/supervisor.rs": ["OS process groups", "signals", "subprocess stdout/stderr"],
    "src/storage.rs": ["JSONL journal filesystem"],
    "src/json_export.rs": ["JSON output file", "stdout"],
    "src/trace_parser/json.rs": ["serialized schema-v2 JSON"],
    "src/trace_parser/journal.rs": ["typed JSONL record stream"],
}

DOC3_PAGES = {
    "src/trace_parser/mod.rs": "parser-ingestion-deep.html",
    "src/trace_parser/json.rs": "parser-ingestion-deep.html",
    "src/trace_parser/journal.rs": "parser-ingestion-deep.html",
    "src/trace_parser/error.rs": "parser-ingestion-deep.html",
    "src/trace_parser/indexes.rs": "parser-indexes-deep.html",
    "src/trace_parser/evidence.rs": "parser-ownership-deep.html",
    "src/trace_parser/views.rs": "parser-ownership-deep.html",
    "src/trace_parser/validation.rs": "parser-validation-deep.html",
    "src/trace_parser/semantic.rs": "parser-semantic-deep.html",
    "src/trace_parser/consumer.rs": "parser-consumer-deep.html",
}
DOC2_PAGES = {
    "src/main.rs": "engine-entry-deep.html",
    "src/lib.rs": "engine-entry-deep.html",
    "src/cli.rs": "engine-entry-deep.html",
    "src/config.rs": "engine-entry-deep.html",
    "src/error.rs": "engine-entry-deep.html",
    "src/compiler.rs": "engine-compiler-deep.html",
    "src/trace.rs": "engine-gdb-deep.html",
    "src/gdb_mi.rs": "engine-gdb-deep.html",
    "src/mi_parser.rs": "engine-mi-deep.html",
    "src/recorder.rs": "engine-recorder-deep.html",
    "src/memory.rs": "engine-stack-deep.html",
    "src/dwarf.rs": "engine-dwarf-deep.html",
    "src/clang_ast.rs": "engine-ast-deep.html",
    "src/allocation_tracker.rs": "engine-heap-deep.html",
    "c_runtime/alloc_tracker.c": "engine-allocator-deep.html",
    "src/valgrind.rs": "engine-diagnostics-deep.html",
    "src/sanitizer.rs": "engine-diagnostics-deep.html",
    "src/evidence/mod.rs": "engine-fusion-deep.html",
    "src/evidence/declarations.rs": "engine-fusion-deep.html",
    "src/evidence/source_links.rs": "engine-fusion-deep.html",
    "src/evidence/heap.rs": "engine-fusion-deep.html",
    "src/evidence/validation.rs": "engine-fusion-deep.html",
    "src/supervisor.rs": "engine-supervision-deep.html",
    "src/resource_limits.rs": "engine-supervision-deep.html",
    "src/models.rs": "engine-export-deep.html",
    "src/processor.rs": "engine-export-deep.html",
    "src/storage.rs": "engine-export-deep.html",
    "src/json_export.rs": "engine-export-deep.html",
}

DOC_ALL_PAGES = {**DOC2_PAGES, **DOC3_PAGES}

RUST_FN = re.compile(
    r"(?m)^[ \t]*(?:(?:pub(?:\([^)]*\))?|async|unsafe|const|default|extern)\s+)*"
    r"fn\s+(?P<name>[A-Za-z_]\w*)\s*(?:<|\()"
)
RUST_MAJOR = re.compile(
    r"(?m)^[ \t]*(?:(?P<visibility>pub(?:\([^)]*\))?)\s+)?"
    r"(?P<kind>struct|enum|union|trait|type|const|static|mod)\s+"
    r"(?P<name>[A-Za-z_]\w*)\b"
)
IMPL = re.compile(r"(?m)^[ \t]*impl\b")
INLINE_MODULE = re.compile(r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*\{")
C_FN = re.compile(
    r"(?m)^[ \t]*(?P<visibility>static\s+)?"
    r"(?:void|unsigned\s+long|unsigned\s+int|int|char|cte_slot|ssize_t)\s*\*?\s*"
    r"(?P<name>(?:cte_|__wrap_)\w+)\s*\("
)
MODULE_REF = re.compile(r"\b(?:crate|c_trace_engine)::[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*")
RELATIVE_USE = re.compile(r"\buse\s+(?:super|self)::[A-Za-z_]\w*(?:::[A-Za-z_]\w*)*")
TEST_ATTR = re.compile(r"#\[\s*(?:tokio::)?test\s*\]\s*(?:async\s+)?fn\s+([A-Za-z_]\w*)")
RAW_STRING = re.compile(r'(?:br|rb|r)(#{0,16})"')
CHAR_LITERAL = re.compile(r"'(?:\\.|[^'\\\n])'")


def mask_non_code(text):
    """Replace comments and literals with spaces while retaining offsets/newlines."""
    out = list(text)

    def blank(start, end):
        for pos in range(start, end):
            if out[pos] != "\n":
                out[pos] = " "

    i = 0
    while i < len(text):
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = len(text) if end < 0 else end
        elif text.startswith("/*", i):
            depth = 1
            end = i + 2
            while end < len(text) and depth:
                if text.startswith("/*", end):
                    depth += 1
                    end += 2
                elif text.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
        else:
            raw = RAW_STRING.match(text, i)
            if raw:
                marker = '"' + raw.group(1)
                found = text.find(marker, i + len(raw.group()))
                end = len(text) if found < 0 else found + len(marker)
            elif text[i] == '"' or text.startswith('b"', i):
                end = i + (2 if text.startswith('b"', i) else 1)
                while end < len(text):
                    if text[end] == "\\":
                        end += 2
                    elif text[end] == '"':
                        end += 1
                        break
                    else:
                        end += 1
            elif text[i] == "'" and (char_match := CHAR_LITERAL.match(text, i)):
                end = char_match.end()
            else:
                i += 1
                continue
        blank(i, min(end, len(text)))
        i = end
    return "".join(out)


def line_of(newlines, offset):
    return bisect.bisect_left(newlines, offset) + 1


def definition_end(masked, start):
    """First top-level body opener or declaration semicolon after a header."""
    parens = brackets = 0
    for pos in range(start, len(masked)):
        char = masked[pos]
        if char == "(":
            parens += 1
        elif char == ")":
            parens -= 1
        elif char == "[":
            brackets += 1
        elif char == "]":
            brackets -= 1
        elif not parens and not brackets and char in "{;":
            return pos, char
    return None, None


def brace_pairs(masked):
    stack = []
    pairs = {}
    for i, char in enumerate(masked):
        if char == "{":
            stack.append(i)
        elif char == "}" and stack:
            pairs[stack.pop()] = i
    return pairs


def impl_owner(header):
    """Take only a simple self type; never invent an owner for complex impls."""
    header = re.sub(r"^impl\s*", "", header.strip())
    if header.startswith("<"):
        level = 0
        for i, char in enumerate(header):
            level += (char == "<") - (char == ">")
            if level == 0:
                header = header[i + 1:].strip()
                break
    header = header.split(" where ", 1)[0]
    target = header.rsplit(" for ", 1)[-1].strip()
    match = re.match(r"^([A-Za-z_]\w*(?:::[A-Za-z_]\w*)*)(?:\s*<.*>|\s*)$", target)
    return match.group(1) if match else None


def visibility_of(header, c=False):
    if c:
        return "file_private" if header.lstrip().startswith("static ") else "external_linkage"
    match = re.search(r"\bpub(?:\(([^)]+)\))?(?![A-Za-z_0-9])", header)
    return "private" if not match else ("public" if not match.group(1) else "pub(" + match.group(1) + ")")


def symbol(path, module, name, kind, line, signature, visibility, owner=None):
    return {
        "id": f"{path}:{line}:{kind}:{name}",
        "name": name,
        "qualifiedName": (f"{module}::{owner}::{name}" if owner else f"{module}::{name}") if module else name,
        "path": path,
        "line": line,
        "signature": " ".join(signature.split()),
        "kind": kind,
        "visibility": visibility,
        "inspectionStatus": "automatically_indexed_unverified",
        "docPage": DOC_ALL_PAGES.get(path),
        "documentationStatus": "file_chapter_indexed_only" if path in DOC_ALL_PAGES else "indexed_only",
    }


def module_of(path):
    if path == "src/lib.rs":
        return "crate"
    if path == "src/main.rs":
        return "bin::c-trace-engine"
    pieces = Path(path).with_suffix("").parts[1:]
    if pieces[-1] == "mod":
        pieces = pieces[:-1]
    return "crate::" + "::".join(pieces)


def extract_symbols(path, text, module):
    if not path.endswith((".rs", ".c")):
        return [], [], []
    masked = mask_non_code(text)
    newlines = [i for i, char in enumerate(text) if char == "\n"]
    pairs = brace_pairs(masked)
    items = []
    major = []
    functions = []
    if path.endswith(".rs"):
        impls = []
        inline_modules = []
        for match in INLINE_MODULE.finditer(masked):
            opening = masked.find("{", match.start(), match.end())
            if opening in pairs:
                inline_modules.append((opening, pairs[opening], match.group(1)))

        def scoped_module(offset):
            nested = sorted((block for block in inline_modules if block[0] < offset < block[1]),
                            key=lambda block: block[0])
            return module + "".join("::" + block[2] for block in nested)

        for match in IMPL.finditer(masked):
            opening, delim = definition_end(masked, match.end())
            if delim == "{" and opening in pairs:
                impls.append((match.start(), opening, pairs[opening], impl_owner(masked[match.start():opening])))
        for match in RUST_MAJOR.finditer(masked):
            line = line_of(newlines, match.start())
            enclosing = [block for block in impls if block[1] < match.start() < block[2]]
            owner = min(enclosing, key=lambda block: block[2] - block[1])[3] if enclosing else None
            item = symbol(path, scoped_module(match.start()), match.group("name"), match.group("kind"), line,
                          text[match.start():match.end()], visibility_of(match.group()), owner)
            if enclosing and owner is None:
                item["qualifiedName"] = None
            items.append(item)
            major.append(item["id"])
        matches = RUST_FN.finditer(masked)
    else:
        impls = []
        matches = C_FN.finditer(masked)
        for match in re.finditer(r"(?m)^}\s*(cte_slot)\s*;", masked):
            item = symbol(path, module, match.group(1), "typedef struct", line_of(newlines, masked.rfind("typedef struct", 0, match.start())),
                          "typedef struct { ... } " + match.group(1), "file_private")
            items.append(item)
            major.append(item["id"])
        for match in re.finditer(r"(?m)^#define\s+(CTE_TABLE_[A-Za-z_]+)\b[^\n]*", text):
            item = symbol(path, module, match.group(1), "macro constant", line_of(newlines, match.start()),
                          match.group(), "file_private")
            items.append(item)
            major.append(item["id"])
    for match in matches:
        end, delim = definition_end(masked, match.end() - 1)
        if delim != "{" or end not in pairs:
            continue  # Trait declaration or C prototype, not a definition.
        line = line_of(newlines, match.start())
        header = text[match.start():end].strip()
        enclosing = [block for block in impls if block[1] < match.start() < block[2]]
        owner = min(enclosing, key=lambda block: block[2] - block[1])[3] if enclosing else None
        local_module = scoped_module(match.start()) if path.endswith(".rs") else module
        item = symbol(path, local_module, match.group("name"), "function", line, header,
                      visibility_of(match.group(), path.endswith(".c")), owner)
        if enclosing and owner is None:
            item["qualifiedName"] = None
        items.append(item)
        functions.append(item["id"])
        if item["visibility"] in ("public", "external_linkage"):
            major.append(item["id"])
    items.sort(key=lambda item: (item["line"], item["id"]))
    return items, major, functions


def dependencies(path, text, module, modules):
    if not path.endswith(".rs"):
        return []
    masked = mask_non_code(text)
    newlines = [i for i, char in enumerate(text) if char == "\n"]
    found = {}
    for match in MODULE_REF.finditer(masked):
        parts = match.group().split("::")
        reference = "crate::" + "::".join(parts[1:])
        for candidate in sorted(modules, key=len, reverse=True):
            if reference == candidate or reference.startswith(candidate + "::"):
                if candidate != module and candidate != "crate":
                    found.setdefault(candidate, line_of(newlines, match.start()))
                break
    for match in RELATIVE_USE.finditer(masked):
        parts = match.group()[4:].split("::")
        base = module.rsplit("::", 1)[0] if parts[0] == "super" else module
        reference = base + "::" + "::".join(parts[1:])
        for candidate in sorted(modules, key=len, reverse=True):
            if reference == candidate or reference.startswith(candidate + "::"):
                if candidate != module and candidate != "crate":
                    found.setdefault(candidate, line_of(newlines, match.start()))
                break
    return [
        {"targetModule": target, "targetPath": modules[target],
         "referenceKind": "static reference", "line": line}
        for target, line in sorted(found.items())
    ]


def known_tests(path, text, module, integration_tests):
    if path == "Cargo.toml":
        return []
    masked = mask_non_code(text)
    inline = TEST_ATTR.findall(masked)
    results = []
    if inline:
        results.append({"path": path, "testNames": inline, "match": "in-file test attribute (lexical)"})
    for test_path, (contents, code) in integration_tests.items():
        if path == "c_runtime/alloc_tracker.c":
            matched = path in contents
        elif module in (None, "crate", "bin::c-trace-engine"):
            matched = False
        else:
            # Exact module-path prefix only; a parent reference does not match a child.
            prefix = "c_trace_engine::" + module.removeprefix("crate::")
            matched = bool(re.search(r"\b" + re.escape(prefix) + r"\b(?:::|\b)",
                                     code))
        if matched:
            results.append({"path": test_path, "testNames": [],
                            "match": "explicit source module/path reference (not proof of coverage)"})
    return results


def build():
    paths = sorted(
        [p.relative_to(ROOT).as_posix() for p in (ROOT / "src").rglob("*.rs")
         if "semantic" not in p.relative_to(ROOT / "src").parts[:-1]]
        + [p.relative_to(ROOT).as_posix() for p in (ROOT / "c_runtime").glob("*.c")]
        + ["Cargo.toml"]
    )
    missing = set(paths) - ROLES.keys()
    stale = ROLES.keys() - set(paths)
    if missing or stale:
        raise ValueError(f"curated role map must match current scope; missing={sorted(missing)}, stale={sorted(stale)}")
    modules = {module_of(path): path for path in paths if path.endswith(".rs")}
    crates = sorted(tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8")).get("dependencies", {}))
    test_files = {}
    for test in sorted((ROOT / "tests").glob("*.rs")):
        contents = test.read_text(encoding="utf-8")
        test_files[test.relative_to(ROOT).as_posix()] = (contents, mask_non_code(contents))
    files = []
    symbols = []
    for path in paths:
        text = (ROOT / path).read_text(encoding="utf-8")
        subsystem, role, responsibility, effects = ROLES[path]
        module = module_of(path) if path.endswith(".rs") else None
        manifest = tomllib.loads(text) if path == "Cargo.toml" else None
        extracted, major, functions = extract_symbols(path, text, module)
        symbols.extend(extracted)
        files.append({
            "id": path, "path": path,
            "language": "Rust" if path.endswith(".rs") else "C" if path.endswith(".c") else "TOML",
            "module": module, "subsystem": subsystem, "role": role,
            "responsibility": responsibility,
            "sideEffectsStatus": {"category": effects, "basis": "curated file-level estimate; functions not audited"},
            "inspectionStatus": {"indexing": "automatically_indexed", "verification": "unverified",
                                 "todo": ["DOC-2: verify symbol/relationship interpretations",
                                          "DOC-3: review behavior and side effects"]},
            "majorSymbols": major, "functions": functions,
            "knownTests": known_tests(path, text, module, test_files),
            "docPage": DOC_ALL_PAGES.get(path),
            "documentationStatus": "detailed_file_chapter" if path in DOC_ALL_PAGES else "indexed_only",
            "outboundDependencies": dependencies(path, text, module, modules),
            "externalLibraries": [name for name in crates if path.endswith('.rs') and
                                  re.search(r"\b" + re.escape(name.replace('-', '_')) + r"::", mask_non_code(text))],
            "externalInterfaces": EXTERNAL_INTERFACES.get(path, []),
            **({"manifest": {"package": manifest["package"]["name"],
                              "externalDependencies": sorted(manifest.get("dependencies", {}))}}
               if manifest is not None else {}),
        })
    for record in files:
        record["incomingDependencies"] = [other["path"] for other in files
                                          if any(dep["targetPath"] == record["path"]
                                                 for dep in other["outboundDependencies"])]
    by_id = {item["id"] for item in symbols}
    assert len(by_id) == len(symbols), "duplicate symbol IDs"
    assert all(set(record["majorSymbols"] + record["functions"]) <= by_id for record in files)
    if GRAPH.exists():
        graph = json.loads(GRAPH.read_text(encoding="utf-8"))
        by_location = {(item["path"], item["line"]): item for item in symbols}
        for node in graph["nodes"]:
            if node.get("kind") == "external interface":
                continue  # Subprocess boundary, not a source function definition.
            item = by_location.get((node["path"], node["line"]))
            if item is None:
                raise ValueError(f"Graph node has no indexed function definition: {node['id']} {node['source']}")
            item["inspectionStatus"] = "curated_graph_node_source_checked"
            item["docPage"] = node["docPage"]
            item["documentationStatus"] = "explained_and_graph_linked"
            item["purpose"] = node["purpose"]
            item["inputsOutputs"] = node["inputsOutputs"]
            item["stateAndLimitations"] = node["state"]
            item["knownCallers"] = [dict(node=edge["from"], classification=edge["kind"], evidence=edge["evidence"])
                                    for edge in graph["edges"] if edge["to"] == node["id"]]
            item["importantCallees"] = [dict(node=edge["to"], classification=edge["kind"], evidence=edge["evidence"])
                                       for edge in graph["edges"] if edge["from"] == node["id"]]
    return {
        "schemaVersion": 1,
        "scope": "DOC-1 Stage 1 source inventory: src/**/*.rs excluding src/semantic/**; c_runtime/*.c; Cargo.toml",
        "caveats": [
            "Lexically extracted definitions, signatures and line numbers; macros, conditional compilation, generated code and complex syntax are not exhaustively parsed.",
            "Impl-qualified names are emitted only for simple self types; unresolved owners are null. A qualified name does not prove a function is callable or executed.",
            "Dependencies are static source references, never direct call edges or evidence of execution. External crates and implicit relationships are not inventoried.",
            "Known tests are in-file test attributes or explicit test-file module/path matches; no coverage, execution or correctness claim is made.",
            "File-level side effects are coarse curated estimates, not per-function analysis. Only graph-matched functions have reviewed summaries/edges; other symbols remain unverified pending DOC-2/3.",
        ],
        "counts": {"files": len(files), "rustFiles": sum(f["language"] == "Rust" for f in files),
                    "cFiles": sum(f["language"] == "C" for f in files),
                    "manifestFiles": sum(f["language"] == "TOML" for f in files),
                    "symbols": len(symbols), "functions": sum(s["kind"] == "function" for s in symbols),
                    "typeSymbols": sum(s["kind"] in {"struct", "enum", "union", "trait", "type", "typedef struct"} for s in symbols),
                    "testFileMatches": sum(len(f["knownTests"]) for f in files),
                   "moduleDependencies": sum(len(f["outboundDependencies"]) for f in files)},
        "files": files, "symbols": symbols,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify checked-in output is current")
    args = parser.parse_args()
    result = build()
    encoded = json.dumps(result, indent=2, ensure_ascii=True) + "\n"
    if args.check:
        if not OUTPUT.is_file() or OUTPUT.read_text(encoding="utf-8") != encoded:
            parser.error(f"stale or missing generated inventory: {OUTPUT}")
    else:
        OUTPUT.write_text(encoded, encoding="utf-8")
    print(json.dumps(result["counts"], sort_keys=True))


if __name__ == "__main__":
    main()

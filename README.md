# Visual C — C Trace Engine

Visual C records a **bounded execution of a C program** and lets Rust code inspect the saved evidence. The Trace Engine builds a debug executable with GCC, observes selected stops with GDB/MI2, logs allocator calls it can intercept, and writes a schema-v2 JSON trace. The Trace Parser loads that trace once, indexes positions and offers borrowed, confidence-qualified queries. Valgrind and sanitizer findings come from **separate program runs**; they cannot identify an exact allocation from the primary GDB run.

**Start with the [offline Stage 1 documentation](docs/stage1-html/index.html)** for the study-friendly, 67-chapter guide (open the HTML file directly). [Stage 1's final audit](docs/stage1-final-audit.md) approves its *bounded* evidence contract. Stage 2 currently provides an [SE-1 runtime object registry](docs/semantic-se1.md) and [SE-2 observation-scoped memory layouts](docs/semantic-se2.md); it does **not** provide a pointer graph, execution simulator, or proof of C initialization. The offline website documents Stage 1, not Stage 2 internals.

## Requirements

- Linux, Rust/Cargo, GCC and GDB for live tracing. Python 3 is used for the documentation builder and the optional inspection commands below.
- Valgrind, Clang and `addr2line` supply optional diagnostics/source information; if unavailable, check the trace's stage results rather than assuming a clean analysis.
- The resource controls are **not a security sandbox**. Run untrusted C inside separate OS/container/VM isolation.

Work from the repository root. For the examples below, `/tmp/visual-c-hello.json` is a disposable output file; use your own path if you need to retain a recording. Check `cargo run --release -- --help` for every flag and its accepted values.

## Quick start

```sh
cargo build --release
target/release/c-trace-engine fixtures/c/hello.c \
  --trace-mode source --timeout 35 --sanitizer-trigger never \
  --output /tmp/visual-c-hello.json > /dev/null
python3 -m json.tool /tmp/visual-c-hello.json > /tmp/visual-c-hello-pretty.json
head -60 /tmp/visual-c-hello-pretty.json
cargo run --release --example stage1_public_consumer_audit -- /tmp/visual-c-hello.json
```

`--output` writes a JSON file **and** the program still writes the same JSON to stdout; `> /dev/null` hides the duplicate. A successful export has exit code `0` even if the traced C program crashed, timed out or hit a recording limit; inspect `execution_status`, `coverage`, `heap_history` and `stage_results`. Compile rejection can still write a diagnostic JSON trace and exits `2`; configuration, tool or export errors exit `1` and may leave no usable file.

## Usable commands

The commands in this section use files that ship with this repository. Inspection examples use `/tmp/visual-c-hello.json` from Quick start; Stage 2 examples use `/tmp/visual-c-dual.json` from the dual-mode command below. Commands that invoke GCC/GDB may take longer on slower machines. Run `cargo build --release` once before using `target/release/c-trace-engine` directly.

### Build and run recordings

```sh
# Show all options and recognized values.
cargo run --release -- --help

# Lower-cost source checkpoints, saving the trace to a file.
target/release/c-trace-engine fixtures/c/hello.c \
  --trace-mode source --timeout 35 --sanitizer-trigger never \
  --output /tmp/visual-c-hello.json > /dev/null

# Source + attempted-instruction recording of a real heap fixture.
target/release/c-trace-engine fixtures/c/phase8b_defined.c \
  --trace-mode dual --max-steps 350 --timeout 50 \
  --sanitizer-trigger never --no-pretty \
  --output /tmp/visual-c-dual.json > /dev/null

# Supply arguments to the C program AFTER -- (engine options go before it).
# The fixture's 'normal 2' mode performs two allocation/free pairs.
target/release/c-trace-engine fixtures/c/phase8r2_basic.c \
  --compiler-flag=-pthread --linker-flag=-pthread \
  --trace-mode source --timeout 35 --sanitizer-trigger never \
  --output /tmp/visual-c-args.json -- normal 2 > /dev/null

# Compile and link two C translation units.
target/release/c-trace-engine fixtures/c/multi_file/main.c \
  --add-source fixtures/c/multi_file/helper.c --timeout 35 \
  --sanitizer-trigger never --output /tmp/visual-c-multi.json > /dev/null

# Opt in to Clang source metadata (requires Clang); it is NOT execution proof.
target/release/c-trace-engine fixtures/c/hello.c \
  --clang-ast --timeout 35 --sanitizer-trigger never \
  --output /tmp/visual-c-clang.json > /dev/null

# Run the independent sanitizer pass as well (requires sanitizer runtime).
target/release/c-trace-engine fixtures/c/invalid_write.c \
  --timeout 50 --sanitizer-trigger always \
  --output /tmp/visual-c-diagnostics.json > /dev/null
```

The program's arguments can also be passed with repeated `--arg VALUE`. The last example can record a *crash* and still exit `0` if exporting its trace succeeds. Valgrind may run as its own stage when installed, regardless of `--sanitizer-trigger never` (that flag controls the sanitizer pass only). Each run can have different addresses, PIDs and allocation order.

### Configuration, limits and journal

```sh
# A working config-file example. CLI scalar options override its values.
python3 - <<'PY'
import json
from pathlib import Path
Path('/tmp/visual-c-config.json').write_text(json.dumps({
    'source': 'fixtures/c/hello.c',
    'limits': {'timeout_seconds': 35, 'max_steps': 80},
    'output_file': '/tmp/visual-c-config-trace.json'
}))
PY
target/release/c-trace-engine --config /tmp/visual-c-config.json \
  --sanitizer-trigger never > /dev/null

# Bound separate workloads: stop attempts, debugger commands, JSONL bytes,
# final JSON bytes, and memory captured per checkpoint are different limits.
target/release/c-trace-engine fixtures/c/finite_loop.c \
  --trace-mode dual --timeout 35 --max-steps 120 \
  --max-instruction-events 5000 --max-gdb-commands 50000 \
  --max-snapshot-memory-bytes 4096 --max-trace-bytes 1048576 \
  --max-json-export-bytes 1048576 --sanitizer-trigger never \
  --output /tmp/visual-c-bounded.json > /dev/null

# Save incremental JSONL alongside full final JSON. Make a fresh directory:
# the engine deliberately refuses to overwrite an existing journal.
journal_dir=$(mktemp -d /tmp/visual-c-journal-XXXXXX)
target/release/c-trace-engine fixtures/c/hello.c \
  --trace-mode source --timeout 35 --sanitizer-trigger never \
  --journal "$journal_dir/session.jsonl" \
  --output /tmp/visual-c-journal-trace.json > /dev/null
```

A JSONL journal is **not** a complete JSON trace and cannot be opened as a full `TraceStore`. `--journal-only` requires `--journal` and can omit resident instruction arrays. `--max-trace-bytes` limits the journal, while `--max-json-export-bytes` limits each emitted JSON document; **neither bounds the RAM already used to assemble the trace**. See [resource boundaries](docs/stage1-html/engine-limits.html) and [config-file fields](docs/build-config.md).

### Inspect recordings and run consumers

```sh
# Pretty-print the file produced in Quick start.
python3 -m json.tool /tmp/visual-c-hello.json > /tmp/visual-c-hello-pretty.json
head -60 /tmp/visual-c-hello-pretty.json

# Print separately recorded outcome, capture counts and stage statuses.
python3 - <<'PY'
import json
from pathlib import Path
t = json.loads(Path('/tmp/visual-c-hello.json').read_text())
for key in ('schema_version', 'execution_status', 'compiled_successfully',
            'coverage', 'heap_history', 'stage_results'):
    print(f'{key}: {t.get(key)}')
print('snapshots:', len(t.get('snapshots', [])))
print('heap events:', len(t.get('heap_events', [])))
PY

# Stage 1: validation, scoped coverage and bounded public parser queries.
cargo run --release --example stage1_public_consumer_audit -- /tmp/visual-c-hello.json

# Stage 2 SE-1: object IDs, observations and heap-lifetime queries.
cargo run --release --example semantic_se1 -- /tmp/visual-c-dual.json

# Stage 2 SE-2: observed layouts and bounded child queries; with no supplied,
# independently verified target ELF, typed-byte decoding remains unavailable.
cargo run --release --example semantic_se2_bench -- /tmp/visual-c-dual.json

# The six supported, checked-in V2.1 inputs also load (without tracing C).
cargo run --release --example stage1_public_consumer_audit -- examples/v21_dual.json
```

A `TraceStore` load checks **schema compatibility only**. `validated_semantic()` separately checks structural relationships. A pointer's numeric address inside a block is not proven pointer provenance; full captured bytes do not prove initialization. The four older pre-V2.1 `examples/v2_*.json` files are historical comparison artifacts, **not** supported imports. For SE-2 typed decoding, the caller must independently establish that an explicitly supplied ELF describes the recorded target; schema v2 does not serialize target byte order or prove ELF identity.

### Tests, formatting and documentation

```sh
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo test --no-fail-fast -- --test-threads=1

# Focused Stage 2 tests (live tests need GCC and GDB).
cargo test --test semantic_registry_se1 --test semantic_registry_se1_edges -- --test-threads=1
cargo test --test semantic_layout_se2 --test semantic_layout_se2_live -- --test-threads=1

# Regenerate/check the offline Stage 1 HTML from its content/*.md sources.
python3 docs/stage1-html/build.py
python3 docs/stage1-html/check.py

# Optional browser smoke test: requires Node 22 and google-chrome.
node docs/stage1-html/browser-check.mjs

# Optional local server; then visit http://localhost:8000/index.html
python3 -m http.server 8000 -d docs/stage1-html
```

You can also open [`docs/stage1-html/index.html`](docs/stage1-html/index.html) directly with `file://`; no server or network is necessary. To edit or rebuild the documentation, follow the [site maintenance guide](docs/stage1-html/README.md). [`docs/stage1-html/stage1-complete-text.md`](docs/stage1-html/stage1-complete-text.md) collects its 67 chapter texts for review; the editable per-chapter sources are under `docs/stage1-html/content/`.

## Evidence and scope

1. **A checkpoint is a stopped observation**, not a video of every instruction. Instruction gaps, limits, crashes and missing bytes stay explicit. GDB follows one inferior; completion for observed event PIDs does not prove that silent fork descendants were discovered.
2. **Allocator IDs come from intercepted calls in the primary run.** A later allocation can reuse an old address. Independent Valgrind/sanitizer source-site matches are clues, never exact primary-run allocation identities.
3. **Static metadata describes code, not execution.** DWARF describes the GCC-built artifact; optional Clang AST describes source expressions. A source call candidate does not prove the call ran.
4. **Borrowed evidence keeps history intact.** The parser shares one canonical trace, checks trace-scoped references, and caps TP-7 result pages at 128. SE-1 IDs are trace-scoped; SE-2 paths describe *one* observation, with child pages capped at 64. Neither layer overwrites captured bytes after a free or address reuse.

For background, consult the [Stage 1 API and limitations](docs/stage1-html/handoff.html), [SE-1](docs/semantic-se1.md), [SE-2](docs/semantic-se2.md), [V2.1 update](docs/v2.1/summary.md), and [V2 format history](docs/v2.md). License: MIT.

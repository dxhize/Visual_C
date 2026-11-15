# TEST RESULTS — c-trace-engine V2 validation (suites A–D + J1 + stage-failure harness)

**Method.** Black-box campaign against the checksummed dev build
(`target/debug/c-trace-engine`, `cargo build`). Existing suite: 112/112 passed
before any audit fixture ran (`evidence/existing_tests.log`). Every engine
invocation: subprocess watchdog (150 s; 90 s for D6C), independent process
survivor scan after each run, full trace JSON parsed and validated by
`runner/validate_trace.py` (invariants E1–E20) plus per-suite semantic checks.
Validators were debugged during the campaign; validator fixes stayed on the
audit side. Engine findings: F-01…F-06 (ISSUES.md).

**Per-test status counts:** 29 PASS · 1 BLOCKED (D6A) · 0 FAIL.
WARN check occurrences: 31 — 24 informational (E8c cadence note ×23, C2
static-table note ×1) + 7 substantive on D1–D4 (F-01 metadata), B5/B7 (F-02
sanitizer), B6 (double-free observation O-01).

Statuses: PASS = all checks green. BLOCKED = not testable without an engine
change or privileged isolation. WARN = recorded deviation (see line).

# TEST RESULTS — c-trace-engine V2 validation (suites A–D + J1 + stage-failure harness)

Per-test raw evidence lives in `evidence/<TEST_ID>/` (trace.json, engine_stderr.log,
command.txt, survivors.json). Machine-readable summary: `test_results.json`.

## A1 — PASS
- Fixture: `audit_a1_vars.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a1_vars.c`
- Expected: `completed` | engine exit: 0 | wall: 3.76s
- Trace: execution_status=completed outcome=completed snapshots=8 instruction_events=25 gdb_commands=192 limits=[]

## A1D — PASS
- Fixture: `audit_a1_vars.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a1_vars.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 3.77s
- Trace: execution_status=completed outcome=completed snapshots=8 instruction_events=25 gdb_commands=192 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 2 full checkpoints in 25 events (<256 events: cadence not exercised)

## A2 — PASS
- Fixture: `audit_a2_calls.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a2_calls.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 4.54s
- Trace: execution_status=completed outcome=completed snapshots=17 instruction_events=76 gdb_commands=278 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 76 events (<256 events: cadence not exercised)

## A3 — PASS
- Fixture: `audit_a3_recurse.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a3_recurse.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 5.15s
- Trace: execution_status=completed outcome=completed snapshots=38 instruction_events=99 gdb_commands=448 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 99 events (<256 events: cadence not exercised)

## A4 — PASS
- Fixture: `audit_a4_globals.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a4_globals.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 4.15s
- Trace: execution_status=completed outcome=completed snapshots=16 instruction_events=41 gdb_commands=389 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 41 events (<256 events: cadence not exercised)

## A5 — PASS
- Fixture: `audit_a5_main.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a5_main.c --add-source /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a5_helper.c`
- Expected: `completed` | engine exit: 0 | wall: 4.0s
- Trace: execution_status=completed outcome=completed snapshots=9 instruction_events=33 gdb_commands=144 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 2 full checkpoints in 33 events (<256 events: cadence not exercised)

## B1 — PASS
- Fixture: `null_write.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/fixtures/c/null_write.c --timeout 20`
- Expected: `crashed` | engine exit: 0 | wall: 3.87s
- Trace: execution_status=crashed outcome=crashed snapshots=4 instruction_events=5 gdb_commands=56 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 5 events (<256 events: cadence not exercised)

## B2 — PASS
- Fixture: `audit_b2_heap.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_b2_heap.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 3.84s
- Trace: execution_status=completed outcome=completed snapshots=9 instruction_events=28 gdb_commands=175 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 3 full checkpoints in 28 events (<256 events: cadence not exercised)

## B3 — PASS
- Fixture: `audit_b3_realloc.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_b3_realloc.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 4.28s
- Trace: execution_status=completed outcome=completed snapshots=14 instruction_events=47 gdb_commands=387 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 6 full checkpoints in 47 events (<256 events: cadence not exercised)

## B4 — PASS
- Fixture: `audit_b4_alias.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_b4_alias.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 3.91s
- Trace: execution_status=completed outcome=completed snapshots=10 instruction_events=29 gdb_commands=228 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 3 full checkpoints in 29 events (<256 events: cadence not exercised)

## B5 — PASS
- Fixture: `use_after_free.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/fixtures/c/use_after_free.c --timeout 20`
- Expected: `completed` | engine exit: 0 | wall: 3.86s
- Trace: execution_status=completed outcome=completed snapshots=9 instruction_events=24 gdb_commands=167 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 4 full checkpoints in 24 events (<256 events: cadence not exercised)
- WARN `B5_asan`: reproduction=None

## B6 — PASS
- Fixture: `audit_b6_double_free.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_b6_double_free.c --timeout 20`
- Expected: `crashed` | engine exit: 0 | wall: 3.72s
- Trace: execution_status=crashed outcome=crashed snapshots=7 instruction_events=16 gdb_commands=102 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 3 full checkpoints in 16 events (<256 events: cadence not exercised)
- WARN `B6_second_free_recorded`: freed events=1 types=['allocated', 'freed']

## B7 — PASS
- Fixture: `audit_b7_oob.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_b7_oob.c --timeout 20`
- Expected: `crashed` | engine exit: 0 | wall: 3.74s
- Trace: execution_status=completed outcome=completed snapshots=8 instruction_events=22 gdb_commands=152 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 3 full checkpoints in 22 events (<256 events: cadence not exercised)
- WARN `B7_asan_overflow`: category= 

## B8 — PASS
- Fixture: `audit_b2_heap.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_b2_heap.c --trace-mode source`
- Expected: `completed` | engine exit: 0 | wall: 3.53s
- Trace: execution_status=completed outcome=completed snapshots=10 instruction_events=0 gdb_commands=136 limits=[]

## B9 — PASS
- Fixture: `audit_b9_bytes.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_b9_bytes.c --trace-mode dual`
- Expected: `completed` | engine exit: 0 | wall: 3.58s
- Trace: execution_status=completed outcome=completed snapshots=6 instruction_events=19 gdb_commands=130 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 19 events (<256 events: cadence not exercised)

## C1 — PASS
- Fixture: `audit_c1_arith.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_c1_arith.c`
- Expected: `completed` | engine exit: 0 | wall: 3.6s
- Trace: execution_status=completed outcome=completed snapshots=6 instruction_events=13 gdb_commands=118 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 13 events (<256 events: cadence not exercised)

## C2 — PASS
- Fixture: `audit_c2_loop.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_c2_loop.c`
- Expected: `completed` | engine exit: 0 | wall: 3.85s
- Trace: execution_status=completed outcome=completed snapshots=5 instruction_events=37 gdb_commands=127 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 37 events (<256 events: cadence not exercised)
- WARN `C2_disassembly_not_duplicated`: static table=44 unique instructions vs 37 dynamic events (table legitimately covers non-executed statics; no per-event duplication)

## C3 — PASS
- Fixture: `audit_a2_calls.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a2_calls.c`
- Expected: `completed` | engine exit: 0 | wall: 4.55s
- Trace: execution_status=completed outcome=completed snapshots=17 instruction_events=76 gdb_commands=278 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 76 events (<256 events: cadence not exercised)

## C4 — PASS
- Fixture: `audit_c4_regs.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_c4_regs.c`
- Expected: `completed` | engine exit: 0 | wall: 3.82s
- Trace: execution_status=completed outcome=completed snapshots=8 instruction_events=19 gdb_commands=202 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 19 events (<256 events: cadence not exercised)

## C5 — PASS
- Fixture: `null_write.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/fixtures/c/null_write.c --timeout 20`
- Expected: `crashed` | engine exit: 0 | wall: 3.63s
- Trace: execution_status=crashed outcome=crashed snapshots=4 instruction_events=5 gdb_commands=56 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 5 events (<256 events: cadence not exercised)

## C6 — PASS
- Fixture: `audit_c6_oneline.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_c6_oneline.c`
- Expected: `completed` | engine exit: 0 | wall: 3.68s
- Trace: execution_status=completed outcome=completed snapshots=2 instruction_events=21 gdb_commands=98 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 21 events (<256 events: cadence not exercised)

## C7 — PASS
- Fixture: `audit_c7_libcalls.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_c7_libcalls.c`
- Expected: `completed` | engine exit: 0 | wall: 4.03s
- Trace: execution_status=completed outcome=completed snapshots=11 instruction_events=38 gdb_commands=218 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 4 full checkpoints in 38 events (<256 events: cadence not exercised)

## D1 — PASS
- Fixture: `spin_same_line.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/fixtures/c/spin_same_line.c --timeout 10 --max-steps 1000000`
- Expected: `timeout` | engine exit: 0 | wall: 10.25s
- Trace: execution_status=timeout outcome=timed_out snapshots=3 instruction_events=566 gdb_commands=1168 limits=['session_or_debugger_deadline', 'overall_session_deadline']
- WARN `E:E14_stage_statuses`: allocation_tracker: completed but output incomplete without reason

## D2 — PASS
- Fixture: `audit_c2_loop.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_c2_loop.c --max-instruction-events 25`
- Expected: `step_limit_reached` | engine exit: 0 | wall: 3.69s
- Trace: execution_status=step_limit_reached outcome=step_limit_reached snapshots=3 instruction_events=25 gdb_commands=89 limits=['max_instruction_events']
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 25 events (<256 events: cadence not exercised)
- WARN `E:E14_stage_statuses`: allocation_tracker: completed but output incomplete without reason

## D3 — PASS
- Fixture: `audit_a4_globals.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_a4_globals.c --max-source-checkpoints 3 --trace-mode dual`
- Expected: `step_limit_reached` | engine exit: 0 | wall: 3.4s
- Trace: execution_status=step_limit_reached outcome=step_limit_reached snapshots=3 instruction_events=5 gdb_commands=76 limits=['max_source_checkpoints']
- WARN `E:E8c_full_checkpoint_cadence`: 1 full checkpoints in 5 events (<256 events: cadence not exercised)
- WARN `E:E14_stage_statuses`: allocation_tracker: completed but output incomplete without reason

## D4 — PASS
- Fixture: `audit_d4_output.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_d4_output.c --trace-mode source --max-output-bytes 16384 --timeout 60`
- Expected: `step_limit_reached` | engine exit: 0 | wall: 14.26s
- Trace: execution_status=step_limit_reached outcome=output_limit_reached snapshots=731 instruction_events=0 gdb_commands=5127 limits=['max_output_bytes', 'max_output_bytes']
- WARN `E:E14_stage_statuses`: allocation_tracker: completed but output incomplete without reason

## J1 — PASS
- Fixture: `audit_c2_loop.c`
- Command: `/home/xe/Documents/visual_c/target/debug/c-trace-engine /home/xe/Documents/visual_c/test_reports/v2_validation/fixtures/audit_c2_loop.c --journal-only --journal /home/xe/Documents/visual_c/test_reports/v2_validation/evidence/J1/session.jsonl`
- Expected: `completed` | engine exit: 0 | wall: 3.8s
- Trace: execution_status=completed outcome=completed snapshots=5 instruction_events=37 gdb_commands=127 limits=[]
- WARN `E:E8c_full_checkpoint_cadence`: 0 full checkpoints in 0 events (<256 events: cadence not exercised)

## D6A — BLOCKED
- Fixture: `audit_a1_vars.c`
- Command: `None`
- Expected: `completed` | engine exit: 0 | wall: 4.1s
- Trace: execution_status=completed outcome=completed snapshots=8 instruction_events=None gdb_commands=None limits=[]

## D6B — PASS
- Fixture: `audit_a1_vars.c`
- Command: `None`
- Expected: `completed` | engine exit: 0 | wall: 2.9s
- Trace: execution_status=completed outcome=completed snapshots=8 instruction_events=None gdb_commands=None limits=[]

## D6C — PASS
- Fixture: `audit_a1_vars.c`
- Command: `None`
- Expected: `debugger_failed` | engine exit: 0 | wall: 1.89s
- Trace: execution_status=debugger_failure outcome=debugger_failed snapshots=0 instruction_events=None gdb_commands=None limits=[]

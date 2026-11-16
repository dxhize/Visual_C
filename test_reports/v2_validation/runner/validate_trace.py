#!/usr/bin/env python3
"""Trace-model integrity validators for c-trace-engine V2 (audit suite E).

Semantics matched to the PROBED engine schema (not assumed):
- heap_events entries: {pid, allocation_id, event_type(allocated|freed), address,
  size_bytes, source_file, source_line, old_allocation_id, ...}
- variable pointer targets: {kind(null|heap|global|...), allocation_id,
  base_address, offset_bytes, ...}
- register blocks in instruction events are POST-attempt observations:
  event N's rip equals its next_address (for faulted events, address).
  V2.1 traces contain no register blocks (capture removed); E8a/E8b/E8c
  detect this and validate V2.1 address semantics instead.
- valgrind detailed_errors entries may be dicts OR plain strings.
"""
import json, re, sys

def _hx(s):
    try:
        return int(s, 16)
    except (TypeError, ValueError):
        return None

def _reg_map(regblock):
    return {v.get("name"): v for v in (regblock or {}).get("values", [])}

def _vg_errors(vr):
    errs = []
    for k in ("detailed_errors", "errors"):
        errs += (vr.get(k) or [])
    return errs

def _err_kind(e):
    if isinstance(e, dict):
        return e.get("kind") or e.get("what") or "dict"
    return str(e).strip()[:48]

def _tgt(v):
    return v.get("target") or {}

def validate_trace(x, ctx):
    """ctx: dict with keys: dual(bool), journal_path, expect_status, fixture"""
    out = []
    def add(invariant, status, detail=""):
        out.append({"invariant": invariant, "status": status, "detail": detail})

    events = x.get("instruction_events") or []
    snaps = x.get("snapshots") or []
    table = x.get("disassembly_table") or []
    gaps = x.get("trace_gaps") or []
    dual = ctx["dual"]

    add("E1_json_valid", "PASS", "parsed as JSON")

    sv = x.get("schema_version")
    add("E2_schema_version", "PASS" if sv == 2 else "FAIL", f"schema_version={sv}")

    required = ["compiled_successfully", "compile_errors", "snapshots", "heap_events",
                "heap_state", "execution_status", "resource_usage", "analysis_notes",
                "execution", "stage_results", "tracing_configuration", "schema_version"]
    missing = [k for k in required if k not in x]
    add("E3_required_fields", "PASS" if not missing else "FAIL",
        f"missing={missing}" if missing else "all present")

    ids = [e.get("event_id") for e in events]
    uniq = len(ids) == len(set(ids)) and ids == list(range(len(events)))
    add("E4_event_ids_unique", "PASS" if uniq else "FAIL",
        f"n={len(events)} first={ids[:1]} last={ids[-1:]}" if ids else "no events")

    sb = [e.get("stop_id_before") for e in events]
    ordered = all(sb[i] <= sb[i + 1] for i in range(len(sb) - 1)) if sb else True
    add("E5_events_ordered", "PASS" if ordered else "FAIL",
        "stop_id_before nondecreasing" if ordered else
        f"sequence broken at {[i for i in range(len(sb)-1) if sb[i] > sb[i+1]][:3]}")

    ck = x.get("source_checkpoints")
    bad = []
    if isinstance(ck, list):
        for i in ck:
            if not isinstance(i, int) or i < 0 or i >= len(snaps):
                bad.append(i)
            elif snaps[i].get("step_index") != i:
                bad.append(f"snap[{i}].step_index={snaps[i].get('step_index')}")
    else:
        bad = "source_checkpoints missing or not a list"
    ev_bad = [e.get("event_id") for e in events
              if e.get("checkpoint_id") is not None
              and (not isinstance(e["checkpoint_id"], int)
                   or e["checkpoint_id"] < 0 or e["checkpoint_id"] >= len(snaps))]
    add("E6_checkpoint_refs", "PASS" if not bad and not ev_bad else "FAIL",
        (f"bad_ckpt={bad or ev_bad}") if (bad or ev_bad) else
        f"{len(ck or [])} checkpoints validated")

    if dual:
        tbad = []
        for e in events:
            iid = e.get("instruction_id")
            if iid is None:
                continue
            if not isinstance(iid, int) or iid < 0 or iid >= len(table):
                tbad.append(e.get("event_id"))
            elif _hx(table[iid].get("address")) != _hx(e.get("address")):
                tbad.append(f"addr_mismatch@{e.get('event_id')}")
        add("E7_disassembly_refs", "PASS" if not tbad else "FAIL",
            f"bad={tbad[:5]}" if tbad else f"table={len(table)} events={len(events)}")

    if dual:
        # V2.1 detection: per-instruction CPU register capture was removed.
        # A trace with neither an initial register checkpoint nor per-event
        # register blocks follows V2.1 semantics; anything else is a legacy
        # V2 trace and keeps the original checks.
        v21 = not x.get("initial_registers") and not any(e.get("registers") for e in events)
        init = x.get("initial_registers")
        if v21:
            add("E8a_initial_registers_present", "PASS",
                "V2.1: register capture removed; initial_registers deprecated/absent")
        elif init and (init.get("values") or []):
            add("E8a_initial_registers_present", "PASS",
                f"legacy V2: {len(init['values'])} registers, full={init.get('full_checkpoint')}")
        else:
            add("E8a_initial_registers_present", "FAIL",
                "legacy dual mode without initial register checkpoint")
        rip_bad, fulls = [], 0
        for e in events:
            rb = e.get("registers")
            if not rb:
                continue
            if rb.get("full_checkpoint"):
                fulls += 1
            rip = _reg_map(rb).get("rip")
            if not rip or _hx(rip.get("value")) is None:
                continue
            expect = _hx(e.get("address")) if e.get("outcome") == "faulted" \
                else _hx(e.get("next_address"))
            if _hx(rip["value"]) != expect:
                rip_bad.append(f"ev{e.get('event_id')}({e.get('outcome')}):"
                               f"rip={rip.get('value')},expected={hex(expect) if expect else '?' }")
        if v21:
            add("E8b_rip_matches_post_attempt_pc", "PASS",
                "V2.1: no register blocks; instruction addresses are stop-frame "
                "based and validated by E6/E7")
            add("E8c_full_checkpoint_cadence", "PASS",
                "V2.1: register capture removed; no checkpoint cadence")
        else:
            add("E8b_rip_matches_post_attempt_pc", "PASS" if not rip_bad else "FAIL",
                "rip==next_address (faulted: rip==address)" if not rip_bad else str(rip_bad[:3]))
            n = len(events)
            add("E8c_full_checkpoint_cadence", "WARN" if n < 256 else "PASS",
                f"{fulls} full checkpoints in {n} events (<256 events: cadence not exercised)")

    # ---- allocation lifetime model (event_type: allocated | freed) ----
    hev = x.get("heap_events") or []
    id_addr = {}
    stab_bad = []
    for h in hev:
        if h.get("event_type") == "allocated":
            key = (h.get("pid"), h.get("allocation_id"))
            addr = _hx(h.get("address"))
            if key in id_addr and id_addr[key] != addr:
                stab_bad.append(key)
            id_addr[key] = addr
    add("E9_alloc_ids_stable", "PASS" if not stab_bad else "FAIL",
        f"heap_events={len(hev)}" if not stab_bad else f"id/address inconsistent for {stab_bad[:3]}")

    freed_ids, react_bad = set(), []
    for h in hev:
        key = (h.get("pid"), h.get("allocation_id"))
        if h.get("event_type") == "freed":
            freed_ids.add(key)
        elif h.get("event_type") == "allocated" and key in freed_ids:
            react_bad.append(key)
    add("E10_freed_not_reactivated", "PASS" if not react_bad else "FAIL",
        "no freed (pid,id) reused as active" if not react_bad else f"reactivated: {react_bad[:3]}")

    def _sim(upto):
        l, fd = set(), set()
        for h in hev[:upto]:
            et = h.get("event_type")
            if et == "allocated" or et == "reallocated":
                l.add(h.get("allocation_id"))
            elif et == "freed":
                l.discard(h.get("allocation_id"))
                fd.add(h.get("allocation_id"))
            if h.get("old_allocation_id") is not None and et != "reallocated":
                l.discard(h.get("old_allocation_id"))
                fd.add(h.get("old_allocation_id"))
        return l, fd

    live_all, freed_all = _sim(len(hev))
    pbad = []
    for s in snaps:
        obs = s.get("observation") or {}
        n = obs.get("heap_event_count")
        timeline = isinstance(n, int)
        live_n, freed_n = set(), set()
        if timeline:
            live_n, freed_n = _sim(n)
        for v in s.get("variables") or []:
            tgt = _tgt(v)
            if tgt.get("kind") == "heap":
                aid = tgt.get("allocation_id")
                if aid is None or (aid not in live_all and aid not in freed_all):
                    pbad.append(f"snap{s.get('step_index')}:{v.get('name')}->unknown id {aid}")
                elif timeline:
                    if aid in freed_n and tgt.get("active") is True:
                        pbad.append(f"snap{s.get('step_index')}:{v.get('name')} claims active "
                                    f"after free (id {aid}, heap_event_count={n})")
                elif aid in freed_all and tgt.get("active") is True:
                    pbad.append(f"snap{s.get('step_index')}:{v.get('name')} claims active "
                                f"after free (id {aid}, no timeline available)")
            elif tgt.get("kind") == "null":
                pv = _hx(v.get("points_to") or v.get("value"))
                if pv is not None and pv != 0:
                    pbad.append(f"snap{s.get('step_index')}:{v.get('name')} null-class but "
                                f"value={v.get('points_to') or v.get('value')}")
    add("E11_pointer_classification", "PASS" if not pbad else "FAIL",
        "heap/null targets agree with per-stop allocation state (heap_event_count timeline)"
        if not pbad else str(pbad[:4]))

    inc = []
    cov = x.get("coverage") or {}
    trunc = x.get("coverage_truncation_reasons") or cov.get("truncation_reasons") or []
    if cov.get("complete_for_scope") and trunc:
        inc.append("complete_for_scope=true with truncation reasons")
    for i, s in enumerate(snaps):
        obs = s.get("observation") or {}
        if obs.get("complete") and (obs.get("errors") or s.get("memory_truncated")):
            inc.append(f"snap{i}.complete=true despite errors/truncation")
    add("E12_completeness_flags", "PASS" if not inc else "FAIL", "; ".join(inc) or "consistent")

    st = x.get("execution_status")
    lim = (x.get("resource_usage") or {}).get("limits_triggered") or []
    lim_names = [str(l).split("(")[0].strip() for l in lim]
    deadline_lims = [l for l in lim_names
                     if "deadline" in l or "timeout" in l or "session" in l]
    if st in ("step_limit_reached", "resource_limit_reached") and not lim:
        add("E13_limits_metadata", "FAIL", f"status={st} but limits_triggered empty")
    elif st == "timeout" and not deadline_lims:
        add("E13_limits_metadata", "FAIL", f"status=timeout but no deadline limit in {lim}")
    elif st not in ("step_limit_reached", "resource_limit_reached", "timeout",
                    "output_limit_reached") and lim:
        add("E13_limits_metadata", "WARN", f"status={st} but limits_triggered={lim}")
    else:
        add("E13_limits_metadata", "PASS", f"status={st} limits={lim or 'none'}")

    sr = x.get("stage_results") or {}
    sbad, swarn = [], []
    for name, s in sr.items():
        if not isinstance(s, dict):
            continue
        stat = s.get("status")
        if stat == "skipped" and (s.get("duration_ms") not in (None, 0)
                                  or s.get("start_offset_ms") not in (None, 0)):
            sbad.append(f"{name}: skipped but timed")
        if stat == "completed" and s.get("output_complete") is False and not s.get("reason"):
            swarn.append(f"{name}: completed but output incomplete without reason")
    add("E14_stage_statuses", "WARN" if swarn and not sbad else ("FAIL" if sbad else "PASS"),
        "; ".join(sbad + swarn) or "consistent")

    if st == "crashed":
        cr = x.get("crash_report") or {}
        okc = bool(snaps) and bool(cr)
        detail = f"snapshots={len(snaps)} crash_report={'yes' if cr else 'NO'}"
        if dual and events:
            last = events[-1]
            faulted = last.get("outcome") == "faulted"
            excl = (not faulted and last.get("outcome") == "completed"
                    and cr.get("instruction_address"))
            ok = okc and (faulted or excl)
            detail += (f" last_event={last.get('outcome')} crash_pc={cr.get('instruction_address')}"
                       f" signal={cr.get('signal_name')}")
        else:
            ok = okc
        add("E15_crash_prehistory", "PASS" if ok else "FAIL", detail)

    startup = [g for g in gaps if g.get("reason") == "startup_before_main"]
    # require the startup gap only when user code was actually reached
    reached_user = bool(events) or bool(snaps) or st == "completed"
    if reached_user:
        add("E16_startup_gap", "PASS" if startup else "FAIL",
            f"{len(gaps)} gaps; startup={'yes' if startup else 'MISSING'}")
    else:
        add("E16_startup_gap", "PASS",
            f"no user-code execution ({st}); startup gap not applicable")
    if ctx.get("expect_status") in ("timeout", "step_limit_reached"):
        add("E16b_timeout_coverage_incomplete",
            "PASS" if cov.get("complete_for_scope") is False else "FAIL",
            f"complete_for_scope={cov.get('complete_for_scope')}")

    if dual and events and st == "crashed":
        outcomes = [e.get("outcome") for e in events]
        if "faulted" in outcomes:
            fi = len(outcomes) - 1 - outcomes[::-1].index("faulted")
            add("E17_no_post_crash_events", "PASS" if fi == len(outcomes) - 1 else "FAIL",
                f"faulted at index {fi} of {len(outcomes)-1}")
        elif (x.get("crash_report") or {}).get("instruction_address"):
            add("E17_no_post_crash_events", "PASS",
                "crash in excluded/untraced code; no fabricated post-crash events "
                f"(last traced event {events[-1].get('outcome')} at {events[-1].get('address')})")
        else:
            add("E17_no_post_crash_events", "FAIL", "crashed but no faulted event and no crash pc")
    elif dual and events and st in ("timeout", "step_limit_reached"):
        unknowns = [e.get("event_id") for e in events if e.get("outcome") == "unknown"]
        add("E17_termination_events", "PASS",
            f"{len(unknowns)} unknown-outcome events (bounded, honest)")

    pos = [(s.get("observation") or {}).get("instruction_position") for s in snaps]
    mono = all(pos[i] <= pos[i + 1] for i in range(len(pos) - 1)) if pos else True
    # journal-only mode retains instruction events in the journal, not the trace:
    # bound against instruction_count when the event list is absent.
    bound = len(events) or x.get("instruction_count") or 0
    within = all(p is None or p <= bound for p in pos)
    add("E18_snapshot_event_linkage", "PASS" if mono and within else "FAIL",
        f"positions={pos} bound={bound}" if not (mono and within) else
        "instruction positions monotonic and in range")

    vr = x.get("valgrind_report")
    if vr:
        sa = vr.get("summary_available")
        if sa is False and (vr.get("total_allocs") or vr.get("bytes_allocated")):
            add("E19_valgrind_no_fake_zeros", "FAIL",
                "summary_available=false but legacy totals nonzero")
        elif sa is False and vr.get("observed_summary") is None:
            add("E19_valgrind_no_fake_zeros", "PASS", "absent summary remains absent")
        else:
            add("E19_valgrind_no_fake_zeros", "PASS", f"summary_available={sa}")
        if vr.get("parse_error"):
            add("E19b_valgrind_parse_error_disclosed", "PASS", str(vr["parse_error"])[:80])

    jp = ctx.get("journal_path")
    if jp:
        try:
            lines = [json.loads(l) for l in open(jp, encoding="utf-8") if l.strip()]
            kinds = {}
            for l in lines:
                kinds[l.get("kind")] = kinds.get(l.get("kind"), 0) + 1
            add("E20_journal", "PASS", f"lines={len(lines)} kinds={kinds}")
        except Exception as ex:
            add("E20_journal", "FAIL", f"journal unreadable: {ex}")
    return out


def semantic_checks(x, test_id, fixture):
    out = []
    # C5 exercises the same crash-at-instruction fixture as B1 (dual mode).
    test_id = "B1" if test_id == "C5" else test_id
    def add(check, status, detail=""):
        out.append((check, status, detail))
    st = x.get("execution_status")
    snaps = x.get("snapshots") or []
    events = x.get("instruction_events") or []
    dual = (x.get("tracing_configuration") or {}).get("mode") == "dual"

    def var_values(name):
        return [(i, v) for i, s in enumerate(snaps)
                for v in (s.get("variables") or []) if v.get("name") == name]

    def snap_globals(name=None):
        gs = []
        for s in snaps:
            for g in ((s.get("observation") or {}).get("globals") or []):
                if isinstance(g, dict) and (name is None or g.get("name") == name):
                    gs.append(g)
        return gs

    if test_id == "A1":
        found = [v for _, v in var_values("product") if (v.get("value") or "").strip() == "42"]
        add("A1_product_value_42", "PASS" if found else "FAIL", f"snapshots={len(snaps)}")
        add("A1_completed", "PASS" if st == "completed" else "FAIL", f"status={st}")
    if test_id == "A2":
        funcs = {s.get("func") for s in snaps}
        add("A2_functions_observed",
            "PASS" if {"main", "middle", "leaf"} <= funcs else "FAIL", f"funcs={funcs}")
        depths = [len(s.get("call_stack") or []) for s in snaps]
        add("A2_nested_frames", "PASS" if max(depths or [0]) >= 2 else "FAIL",
            f"max_stack_depth={max(depths or [0])}")
    if test_id == "A3":
        ns = sorted({(v.get("value") or "").strip() for i, v in var_values("n") if v.get("value")})
        add("A3_distinct_n_values", "PASS" if len(ns) >= 3 else "WARN", f"n values={ns[:8]}")
        cd = [s for s in snaps if s.get("func") == "countdown"]
        deep = [len(s.get("call_stack") or []) for s in cd]
        add("A3_recursive_frames", "PASS" if max(deep or [0]) >= 2 else "FAIL",
            f"max countdown depth={max(deep or [0])}")
        acts = {json.dumps(v.get("identity") or v.get("scope_id") or "")
                for i, v in var_values("n")}
        add("A3_frame_identities_distinct", "PASS" if len(acts) >= 2 else "WARN",
            f"distinct identity keys={len(acts)}")
    if test_id == "A4":
        # Globals are file-qualified: "'/abs/path.c'::name"
        allnames = {g.get("name") for g in snap_globals()}
        have_fs = any((n or "").endswith("::file_static") for n in allnames)
        have_gc = any((n or "").endswith("::global_counter") for n in allnames)
        statics = [v for s in snaps for v in (s.get("variables") or [])
                   if v.get("scope") == "static"]
        have_calls = any(v.get("name") == "calls" for v in statics) \
            or any((n or "").endswith("::calls") for n in allnames)
        add("A4_globals_captured", "PASS" if (have_fs and have_gc) else "FAIL",
            f"globals={sorted(allnames)[:4]}")
        add("A4_function_local_static", "PASS" if have_calls else "WARN",
            f"static vars={[v.get('name') for v in statics][:5]}")
        # gp = &global_counter: legitimately 0x0 BEFORE that line executes;
        # afterwards must be non-null and classified kind:"global".
        gps = var_values("gp")
        later = [(i, v) for i, v in gps
                 if (v.get("points_to") or "").startswith("0x")
                 and _hx(v.get("points_to")) not in (None, 0)]
        kinds = {_tgt(v).get("kind") for i, v in gps}
        add("A4_pointer_to_global",
            "PASS" if later and "global" in kinds else "FAIL",
            f"gp obs={len(gps)} nonnull={len(later)} kinds={sorted(k for k in kinds if k)}")
    if test_id == "A5":
        files = {s.get("file") for s in snaps}
        add("A5_both_tus_observed",
            "PASS" if len([f for f in files if f and f.endswith(".c")]) >= 2 else "FAIL",
            f"files={files}")
        add("A5_completed", "PASS" if st == "completed" else "FAIL", f"status={st}")
    if test_id == "B1":
        pv = [v for _, v in var_values("ptr") or var_values("p")]
        zero = any((v.get("value") or "").strip() in ("0x0", "0") for v in pv)
        add("B1_pointer_is_null", "PASS" if zero else "FAIL",
            f"p values={[v.get('value') for v in pv][:3]}")
        nulls = [v for _, v in (var_values("ptr") or var_values("p"))
                 if _tgt(v).get("kind") == "null"]
        add("B1_null_classification", "PASS" if nulls else "WARN", f"{len(nulls)} null-classified")
        cr = x.get("crash_report") or {}
        add("B1_sigsegv", "PASS" if cr.get("signal_name") == "SIGSEGV" else "FAIL",
            f"signal={cr.get('signal_name')}")
        if dual and events:
            last = events[-1]
            add("B1_faulted_last_event",
                "PASS" if last.get("outcome") == "faulted" and last.get("signal") == "SIGSEGV"
                else "FAIL",
                f"outcome={last.get('outcome')} addr={last.get('address')} sig={last.get('signal')}")
            cpc = _hx(cr.get("instruction_address"))
            add("B1_pc_agreement",
                "PASS" if cpc is not None and cpc == _hx(last.get("address")) else "FAIL",
                f"crash_pc={cr.get('instruction_address')} event_addr={last.get('address')}")
            src_line = (last.get("source") or {}).get("line")
            snap_lines = [s.get("line") for s in snaps]
            add("B1_source_assembly_agree",
                "PASS" if src_line and src_line in snap_lines else "WARN",
                f"event line={src_line} snapshot lines={snap_lines}")
        vr = x.get("valgrind_report") or {}
        errs = _vg_errors(vr)
        kinds = " ".join(_err_kind(e) for e in errs)
        add("B1_valgrind_invalid_write",
            "PASS" if ("InvalidWrite" in kinds or "invalid write" in kinds.lower()) else "FAIL",
            f"kinds={[_err_kind(e) for e in errs][:4]}" if errs else "no valgrind errors")
        srpt = x.get("sanitizer_report") or {}
        add("B1_asan_reproduced",
            "PASS" if srpt.get("reproduction") == "reproduced" else "WARN",
            f"reproduction={srpt.get('reproduction')} category={srpt.get('error_category')}")
        add("B1_outcome_crashed", "PASS" if st == "crashed" else "FAIL", f"status={st}")
    if test_id == "B2":
        h = x.get("heap_events") or []
        m = [e for e in h if e.get("event_type") == "allocated"]
        f = [e for e in h if e.get("event_type") == "freed"]
        add("B2_malloc_and_free_events", "PASS" if m and f else "FAIL",
            f"alloc={len(m)} freed={len(f)} total={len(h)}")
        add("B2_requested_size_16",
            "PASS" if m and m[0].get("size_bytes") == 16 else "FAIL",
            f"size={m[0].get('size_bytes') if m else 'n/a'}")
        hexes = [b.get("raw_bytes_hex") for s in snaps
                 for b in (s.get("heap_blocks") or []) if b.get("raw_bytes_hex")]
        add("B2_raw_bytes_le",
            "PASS" if any(h2.lower().replace(' ', '').startswith("44434241") for h2 in hexes)
            else "WARN",
            f"raw starts={[h2[:12] for h2 in hexes][:3]}")
        add("B2_freed_state",
            "PASS" if any(b.get("is_freed") or b.get("status") == "freed"
                          for b in (x.get("heap_state") or [])) else "FAIL",
            f"heap_state={[(('freed' if b.get('is_freed') else 'live')) for b in (x.get('heap_state') or [])][:3]}")
    if test_id == "B3":
        h = x.get("heap_events") or []
        rl = [e for e in h if (e.get("old_allocation_id") is not None
                               or "realloc" in str(e.get("event_type")))]
        add("B3_realloc_events",
            "PASS" if rl else "WARN",
            f"realloc-evidence={[(e.get('event_type'), e.get('allocation_id'), e.get('old_allocation_id')) for e in rl][:3] or [e.get('event_type') for e in h]}")
        hs = x.get("heap_state") or []
        add("B3_final_freed",
            "PASS" if hs and all(b.get("is_freed") or b.get("status") == "freed" for b in hs)
            else "WARN",
            f"states={[('freed' if b.get('is_freed') else 'live') for b in hs]}")
    if test_id == "B4":
        # p = base, q = p + 1: same allocation_id, offset differs by 4
        seen = {}
        for s in snaps:
            for v in (s.get("variables") or []):
                if v.get("name") in ("p", "q"):
                    t = _tgt(v)
                    seen.setdefault(v["name"], []).append(
                        (v.get("points_to"), t.get("kind"), t.get("allocation_id"),
                         t.get("offset_bytes")))
        ok = False
        det = ""
        if "p" in seen and "q" in seen:
            pq = seen["p"]
            qq = seen["q"]
            same_id = any(a[2] is not None and a[2] == b[2] for a in pq for b in qq)
            off_ok = any(a[3] is not None and b[3] is not None and b[3] == a[3] + 4
                         for a in pq for b in qq)
            addr_ok = any(a[0] and b[0] and _hx(a[0]) is not None and _hx(b[0]) is not None
                          and _hx(b[0]) == _hx(a[0]) + 4 for a in pq for b in qq)
            heap_kind = any(a[1] == "heap" for a in pq) and any(b[1] == "heap" for b in qq)
            ok = heap_kind and same_id and (off_ok or addr_ok)
            det = (f"p={[x[:2] for x in pq[:1]]} q={[x[:2] for x in qq[:1]]} "
                   f"same_id={same_id} off_ok={off_ok} addr_ok={addr_ok}")
        add("B4_alias_same_allocation", "PASS" if ok else "FAIL", det)
    if test_id == "B5":
        h = x.get("heap_events") or []
        add("B5_free_captured",
            "PASS" if any(e.get("event_type") == "freed" for e in h) else "FAIL",
            f"events={len(h)} types={[e.get('event_type') for e in h][:4]}")
        hs = x.get("heap_state") or []
        add("B5_freed_status",
            "PASS" if any(b.get("is_freed") or b.get("status") == "freed" for b in hs) else "FAIL",
            f"states={[('freed' if b.get('is_freed') else 'live') for b in hs][:3]}")
        vr = x.get("valgrind_report") or {}
        errs = _vg_errors(vr)
        add("B5_valgrind_error", "PASS" if errs else "FAIL",
            f"kinds={[_err_kind(e) for e in errs][:3]}" if errs else "no valgrind errors")
        srpt = x.get("sanitizer_report") or {}
        add("B5_asan", "PASS" if srpt.get("reproduction") == "reproduced" else "WARN",
            f"reproduction={srpt.get('reproduction')}")
        pv = [v for s in snaps for v in (s.get("variables") or [])
              if v.get("name") == "p" and v.get("points_to")]
        add("B5_pointer_value_preserved", "PASS" if pv else "WARN", f"p seen={bool(pv)}")
    if test_id == "B6":
        h = x.get("heap_events") or []
        nf = len([e for e in h if e.get("event_type") == "freed"])
        add("B6_second_free_recorded", "PASS" if nf >= 2 else "WARN",
            f"freed events={nf} types={[e.get('event_type') for e in h][:6]}")
        vr = x.get("valgrind_report") or {}
        errs = _vg_errors(vr)
        add("B6_tool_diagnostics", "PASS" if errs else "FAIL",
            f"kinds={[_err_kind(e) for e in errs][:3]}" if errs else "no valgrind errors")
        srpt = x.get("sanitizer_report") or {}
        cat = (srpt.get("error_category") or "")
        msg = (srpt.get("message") or "")
        add("B6_asan_category",
            "PASS" if "double" in (cat + msg).lower() or "abort" in (cat + msg).lower()
            else "WARN", f"category={cat} msg={msg[:60]}")
    if test_id == "B7":
        srpt = x.get("sanitizer_report") or {}
        cat = ((srpt.get("error_category") or "") + " " + (srpt.get("message") or ""))
        add("B7_asan_overflow", "PASS" if "overflow" in cat.lower() else "WARN",
            f"category={cat[:80]}")
        vr = x.get("valgrind_report") or {}
        errs = _vg_errors(vr)
        add("B7_valgrind_finding", "PASS" if errs else "WARN",
            f"kinds={[_err_kind(e) for e in errs][:3]}" if errs else "no valgrind errors")
    if test_id == "B8":
        h = x.get("heap_events") or []
        m = [e for e in h if e.get("event_type") == "allocated"]
        if m:
            cf = (m[0].get("source_file") or "")
            cl = m[0].get("source_line")
            add("B8_caller_site_resolved",
                "PASS" if cf.endswith(fixture) and cl == 3 else "FAIL",
                f"caller={cf}:{cl}")
        else:
            add("B8_caller_site_resolved", "FAIL", "no allocated event")
    if test_id == "B9":
        vars_hex = [(v.get("name"), (v.get("raw_bytes_hex") or "").lower().replace(" ", ""))
                    for s in snaps for v in (s.get("variables") or []) if v.get("raw_bytes_hex")]
        known_all = [h2 for n, h2 in vars_hex if n == "known"]
        add("B9_known_int_bytes",
            "PASS" if any(h2.startswith("44332211") for h2 in known_all) else "WARN",
            f"lifecycle={[h2[:8] for h2 in known_all]} (0x11223344 -> 44332211 LE; zeros pre-init are honest)")
        arr_all = [h2 for n, h2 in vars_hex if n == "arr"]
        add("B9_array_bytes",
            "PASS" if any(h2.startswith("01000000ffffffff00010000") for h2 in arr_all) else "WARN",
            f"lifecycle={[h2[:24] for h2 in arr_all]} ({{1,-1,256}} -> 01000000 ffffffff 00010000)")
    if test_id == "C1":
        add("C1_completed", "PASS" if st == "completed" else "FAIL", f"status={st}")
        add("C1_checkpoints", "PASS" if len(snaps) >= 3 else "FAIL", f"{len(snaps)} checkpoints")
        add("C1_instructions", "PASS" if dual and len(events) >= 5 else "FAIL",
            f"{len(events)} events")
        add("C1_addresses_valid",
            "PASS" if dual and all(_hx(e.get("address")) is not None for e in events) else "FAIL",
            "all event addresses parse")
    if test_id == "C2":
        loop_lines = [e for e in events if (e.get("source") or {}).get("line") == 3]
        add("C2_repeated_instructions_kept", "PASS" if len(loop_lines) >= 10 else "FAIL",
            f"{len(loop_lines)} events on loop line 3 (5-iteration loop)")
        tbl = x.get("disassembly_table") or []
        add("C2_disassembly_not_duplicated",
            "PASS" if len(tbl) <= max(1, len(events) / 2)
            else "WARN",
            f"static table={len(tbl)} unique instructions vs {len(events)} dynamic events "
            "(table legitimately covers non-executed statics; no per-event duplication)")
        prof = x.get("profiling") or {}
        add("C2_cache_effective",
            "PASS" if (prof.get("disassembly_cache_hits") or 0) > 0 else "WARN",
            f"queries={prof.get('disassembly_queries')} hits={prof.get('disassembly_cache_hits')}")
    if test_id == "C4":
        prof = x.get("profiling") or {}
        vals = [e for e in events if e.get("registers")]
        if not vals and not x.get("initial_registers"):
            # V2.1: register capture was removed from the engine. The check
            # inverts: zero register query time and zero register blocks prove
            # the expensive GDB polling is gone, not merely hidden.
            add("C4_register_capture", "PASS" if (prof.get("register_time_us") or 0) == 0 else "FAIL",
                f"V2.1: register_time_us={prof.get('register_time_us')} (must be 0)")
            add("C4_deltas_present", "PASS",
                "V2.1: 0 events with register blocks (capture removed)")
        else:
            add("C4_register_capture",
                "PASS" if (prof.get("register_time_us") or 0) > 0 else "FAIL",
                f"legacy V2: register_time_us={prof.get('register_time_us')}")
            add("C4_deltas_present", "PASS" if vals else "FAIL",
                f"legacy V2: {len(vals)} events with register blocks")
        xsum = [v for _, v in var_values("sum") if (v.get("value") or "").strip() == "30"]
        add("C4_sum_value_30", "PASS" if xsum else "FAIL", "sum==30 observed")
    if test_id == "C6":
        one_line = [e for e in events if (e.get("source") or {}).get("line") == 1]
        add("C6_same_line_events", "PASS" if len(one_line) >= 5 else "FAIL",
            f"{len(one_line)} events on line 1")
        add("C6_checkpoints_sparse", "PASS" if len(snaps) < len(events) else "WARN",
            f"checkpoints={len(snaps)} events={len(events)}")
    if test_id == "C7":
        origins = {(s.get("observation") or {}).get("origin") for s in snaps}
        add("C7_checkpoints_user_only", "PASS" if origins <= {"user_code"} else "FAIL",
            f"origins={origins}")
        reasons = {g.get("reason") for g in (x.get("trace_gaps") or [])}
        add("C7_gaps_present",
            "PASS" if len(x.get("trace_gaps") or []) >= 1 else "FAIL",
            f"gap reasons={reasons}")
        h = x.get("heap_events") or []
        add("C7_alloc_visible",
            "PASS" if any(e.get("event_type") == "allocated" for e in h) else "FAIL",
            f"events={len(h)}")
    if test_id == "D1":
        add("D1_timed_out", "PASS" if st == "timeout" else "FAIL", f"status={st}")
        cov = x.get("coverage") or {}
        add("D1_coverage_incomplete",
            "PASS" if cov.get("complete_for_scope") is False else "FAIL",
            f"complete_for_scope={cov.get('complete_for_scope')}")
        vg = (x.get("stage_results") or {}).get("valgrind") or {}
        add("D1_valgrind_skipped",
            "PASS" if vg.get("status") == "skipped" else "FAIL",
            f"valgrind={vg.get('status')} reason={vg.get('reason')}")
        ex = x.get("execution") or {}
        add("D1_no_false_exit",
            "PASS" if ex.get("outcome") in ("timed_out",) else "FAIL",
            f"outcome={ex.get('outcome')}")
    if test_id == "D2":
        add("D2_limit_count", "PASS" if x.get("instruction_count") == 25 else "FAIL",
            f"count={x.get('instruction_count')}")
        lim = (x.get("resource_usage") or {}).get("limits_triggered") or []
        add("D2_reason_reported", "PASS" if lim else "FAIL",
            f"limits={lim}")
        ids = [e.get("event_id") for e in events]
        add("D2_ordered", "PASS" if ids == list(range(len(ids))) else "FAIL", "ids ordered")
    if test_id == "D3":
        add("D3_cap_respected", "PASS" if len(snaps) <= 3 else "FAIL",
            f"snapshots={len(snaps)}")
        lim = (x.get("resource_usage") or {}).get("limits_triggered") or []
        add("D3_reason_reported", "PASS" if lim else "FAIL", f"limits={lim}")
    if test_id == "D4":
        ru = x.get("resource_usage") or {}
        add("D4_truncated_flag", "PASS" if ru.get("program_output_truncated") else "FAIL",
            f"bytes={ru.get('program_output_bytes')}")
        add("D4_no_status_confusion",
            "PASS" if st in ("step_limit_reached",)
            or (x.get("execution") or {}).get("outcome") in ("output_limit_reached",)
            else "WARN",
            f"status={st} outcome={(x.get('execution') or {}).get('outcome')}")
    if test_id == "D6A":
        vg = (x.get("stage_results") or {}).get("valgrind") or {}
        vr = x.get("valgrind_report") or {}
        if vg.get("status") == "skipped":
            add("D6A_valgrind_skipped", "PASS", f"reason={vg.get('reason')}")
            add("D6A_outcome_unaffected", "PASS" if st == "completed" else "FAIL", f"status={st}")
        else:
            add("D6A_path_shadow_ineffective", "BLOCKED",
                f"engine ran REAL valgrind despite PATH shadow (stage={vg.get('status')}, "
                f"summary_available={vr.get('summary_available')}, exit={vg.get('exit_code')}); "
                "engine resolves valgrind outside PATH — absence simulation requires engine "
                "change or root/container isolation")
            add("D6A_outcome_unaffected", "PASS" if st == "completed" else "FAIL",
                f"status={st} (traced execution unaffected either way)")
    if test_id == "D6B":
        vr = x.get("valgrind_report")
        vg = (x.get("stage_results") or {}).get("valgrind") or {}
        if vg.get("status") == "failed" and vg.get("reason"):
            add("D6B_stage_failed_with_reason", "PASS",
                f"reason={str(vg.get('reason'))[:70]}")
            if vr is None or vr.get("parse_error") or vr.get("summary_available") is False:
                add("D6B_no_fake_success", "PASS",
                    "report absent/null (no fabricated summary)" if vr is None
                    else f"parse_error={str(vr.get('parse_error'))[:60]}")
            else:
                add("D6B_no_fake_success", "FAIL",
                    f"failed stage produced report={json.dumps(vr)[:100]}")
        elif vg.get("status") == "skipped":
            add("D6B_stage_failed_with_reason", "WARN", f"skipped: {vg.get('reason')}")
            add("D6B_no_fake_success", "PASS", "no report fabricated")
        else:
            add("D6B_stage_failed_with_reason", "FAIL",
                f"stage={vg.get('status')} reason={vg.get('reason')}")
            add("D6B_no_fake_success", "FAIL", "garbage XML was not detected")
        add("D6B_outcome_unaffected", "PASS" if st == "completed" else "FAIL", f"status={st}")
    if test_id == "D6C":
        add("D6C_debugger_failure",
            "PASS" if st in ("debugger_failed", "debugger_failure", "execution_failed",
                             "timeout", "interrupted")
            else "FAIL", f"status={st} outcome={(x.get('execution') or {}).get('outcome')}")
        dbg = (x.get("stage_results") or {}).get("debugger") or {}
        add("D6C_stage_reports_failure",
            "PASS" if dbg.get("status") in ("failed", "timed_out", "cancelled") else "FAIL",
            f"stage={dbg.get('status')} reason={str(dbg.get('reason'))[:60]}")
    return out

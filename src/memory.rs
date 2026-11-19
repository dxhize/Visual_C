//! Evidence-based enrichment. Unknown is preferable to a guessed classification.
use crate::allocation_tracker::HeapTimeline;
use crate::models::*;

pub fn address(text: &str) -> Option<u64> {
    u64::from_str_radix(text.strip_prefix("0x")?, 16).ok()
}

pub fn storage_region(pid: u32, addr: u64) -> Option<&'static str> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).ok()?;
    for line in maps.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let (start, end) = fields.first()?.split_once('-')?;
        let start = u64::from_str_radix(start, 16).ok()?;
        let end = u64::from_str_radix(end, 16).ok()?;
        if addr >= start && addr < end {
            return match fields.get(5).copied() {
                Some("[stack]") => Some("stack"),
                Some(path) if path.starts_with('/') => Some("static"),
                _ => None,
            };
        }
    }
    None
}

fn mapping_kind(pid: u32, addr: u64) -> Option<&'static str> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).ok()?;
    let mut observed = false;
    for line in maps.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        let (start, end) = fields.first()?.split_once('-')?;
        let start = u64::from_str_radix(start, 16).ok()?;
        let end = u64::from_str_radix(end, 16).ok()?;
        observed = true;
        if addr >= start && addr < end {
            return if fields.get(1)?.contains('x') {
                Some("code")
            } else if fields.get(5) == Some(&"[stack]") {
                Some("stack")
            } else {
                None
            };
        }
    }
    observed.then_some("unmapped")
}

/// ELF64 little-endian executable load bias using a real PT_LOAD mapping.
/// Unsupported ELF formats return None, never a guessed relocation.
pub fn executable_bias(binary: &std::path::Path, pid: u32) -> Option<u64> {
    let elf = std::fs::read(binary).ok()?;
    if elf.get(..6)? != b"\x7fELF\x02\x01" {
        return None;
    }
    let u64_at = |offset: usize| -> Option<u64> {
        Some(u64::from_le_bytes(
            elf.get(offset..offset + 8)?.try_into().ok()?,
        ))
    };
    let u16_at = |offset: usize| -> Option<u16> {
        Some(u16::from_le_bytes(
            elf.get(offset..offset + 2)?.try_into().ok()?,
        ))
    };
    if u16_at(16)? == 2 {
        return Some(0);
    } // ET_EXEC
    if u16_at(16)? != 3 {
        return None;
    } // ET_DYN (PIE)
    let phoff = usize::try_from(u64_at(32)?).ok()?;
    let entsize = u16_at(54)? as usize;
    let count = u16_at(56)? as usize;
    let path = std::fs::canonicalize(binary).ok()?.display().to_string();
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).ok()?;
    for line in maps.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 6 || fields[5] != path {
            continue;
        }
        let start = u64::from_str_radix(fields[0].split('-').next()?, 16).ok()?;
        let offset = u64::from_str_radix(fields[2], 16).ok()?;
        for i in 0..count {
            let p = phoff.checked_add(i.checked_mul(entsize)?)?;
            let kind = u32::from_le_bytes(elf.get(p..p + 4)?.try_into().ok()?);
            if kind != 1 {
                continue;
            }
            let file_offset = u64_at(p + 8)?;
            let vaddr = u64_at(p + 16)?;
            if offset == file_offset & !4095 {
                return start.checked_sub(vaddr & !4095);
            }
        }
    }
    None
}

/// Only small metadata is borrowed; values and captured bytes stay in place.
#[derive(Clone, Copy)]
struct Observed<'a> {
    base: u64,
    size: u64,
    scope: &'a str,
    identity: Option<&'a str>,
}

fn observed(var: &VariableInfo) -> Option<Observed<'_>> {
    if var.scope_status == Some(ScopeStatus::Inactive) {
        return None;
    }
    Some(Observed {
        base: var.address.as_deref().and_then(address)?,
        size: var.size_bytes?,
        scope: var.scope.as_deref().unwrap_or("unknown"),
        identity: var
            .identity
            .as_deref()
            .filter(|id| !id.is_empty() && !id.starts_with("unresolved:")),
    })
}

/// Retained for callers enriching two borrowed collections outside a snapshot.
pub fn enrich(
    vars: &mut [VariableInfo],
    globals: &[VariableInfo],
    timeline: &HeapTimeline,
    pid: Option<u32>,
) {
    let known: Vec<_> = vars.iter().chain(globals).filter_map(observed).collect();
    let targets: Vec<_> = vars
        .iter()
        .map(|var| pointer_target(var, &known, timeline, pid))
        .collect();
    for (var, target) in vars.iter_mut().zip(targets) {
        var.target = target;
    }
}

/// Enrich all pointer observations using only objects at this checkpoint.
/// Frame-0 variables remain separate from callers. No variable/value/byte
/// buffers are cloned, and no objects from earlier (possibly dead) activations
/// are retained. Only the resulting small pointer-target records are buffered.
pub fn enrich_snapshot(
    snapshot: &mut ExecutionSnapshot,
    timeline: &HeapTimeline,
    pid: Option<u32>,
) {
    let variables = || {
        snapshot
            .variables
            .iter()
            .chain(
                snapshot
                    .caller_frames
                    .iter()
                    .flat_map(|frame| frame.variables.iter()),
            )
            .chain(snapshot.observation.globals.iter())
    };
    let mut known: Vec<_> = snapshot
        .variables
        .iter()
        .chain(snapshot.observation.globals.iter())
        .filter_map(observed)
        .collect();
    for caller in &snapshot.caller_frames {
        // A retained frame association, not an activation number alone, is
        // evidence that a caller is still on this checkpoint's stack.
        if caller.activation_id == 0
            || caller.frame.level == 0
            || !snapshot
                .call_stack
                .iter()
                .any(|frame| same_frame(frame, &caller.frame))
        {
            continue;
        }
        known.extend(
            caller
                .variables
                .iter()
                .filter(|var| var.activation_id == Some(caller.activation_id))
                .filter_map(observed),
        );
    }
    let targets: Vec<_> = variables()
        .map(|var| pointer_target(var, &known, timeline, pid))
        .collect();
    for (var, target) in snapshot
        .variables
        .iter_mut()
        .chain(
            snapshot
                .caller_frames
                .iter_mut()
                .flat_map(|frame| frame.variables.iter_mut()),
        )
        .chain(snapshot.observation.globals.iter_mut())
        .zip(targets)
    {
        var.target = target;
    }
}

/// Reconcile only the *certainty* of observed heap targets after terminal
/// transport verification. Never resolve a checkpoint pointer through the
/// final timeline: a subsequent free/realloc/reuse cannot change what was
/// observed while the inferior was stopped. Likewise preserve stack/code
/// mapping evidence because the inferior's /proc maps may be gone now.
pub fn refresh_heap_targets(snapshot: &mut ExecutionSnapshot, timeline: &HeapTimeline, pid: u32) {
    let incomplete = !timeline.history_complete() || !timeline.is_pid_complete(pid);
    for var in snapshot
        .variables
        .iter_mut()
        .chain(
            snapshot
                .caller_frames
                .iter_mut()
                .flat_map(|frame| frame.variables.iter_mut()),
        )
        .chain(snapshot.observation.globals.iter_mut())
    {
        let Some(target) = var.target.as_mut().filter(|target| target.kind == "heap") else {
            continue;
        };
        // At capture time a single-PID trace may use local IDs; later child
        // events require widening serialized identities, without performing
        // any new numeric address match against later heap lifetimes.
        if timeline.needs_pid_widening() {
            if let Some(id) = target.address_match_allocation_id.as_mut() {
                if *id <= u32::MAX as u64 {
                    *id = timeline.global_id(pid, *id);
                }
            }
            if let Some(id) = target.allocation_id.as_mut() {
                if *id <= u32::MAX as u64 {
                    *id = timeline.global_id(pid, *id);
                }
            }
        }
        if incomplete {
            target.active = None;
            target.allocation_id = None;
            target.provenance_status = Some("history_incomplete".into());
        }
    }
}

pub(crate) fn same_frame(a: &StackFrameInfo, b: &StackFrameInfo) -> bool {
    a.level == b.level
        && a.func == b.func
        && a.file == b.file
        && a.full_file == b.full_file
        && a.line == b.line
        && a.address == b.address
}

fn pointer_target(
    var: &VariableInfo,
    known: &[Observed<'_>],
    timeline: &HeapTimeline,
    pid: Option<u32>,
) -> Option<PointerTarget> {
    let target = var.points_to.as_deref().and_then(address)?;
    let mut info = PointerTarget::default();
    if target == 0 {
        info.kind = "null".into();
    } else if let Some((b, overlaps)) = pid.and_then(|pid| {
        let candidates = timeline.address_candidates(pid, target);
        let newest = *candidates.last()?;
        Some((newest, candidates.len()))
    }) {
        let complete = timeline.is_pid_complete(b.pid) && timeline.history_complete();
        info.kind = "heap".into();
        info.address_match_allocation_id = Some(timeline.global_id(b.pid, b.allocation_id));
        // A numeric match identifies a region, not a pointer's origin.
        // Once another lifetime occupied that region the identity is
        // ambiguous even when the allocator has recycled precisely the same
        // address and only one block is currently alive.
        info.provenance_status = Some(
            if !complete {
                "history_incomplete"
            } else if overlaps > 1 {
                "ambiguous_reuse"
            } else if b.freed {
                "known_freed_address"
            } else {
                "address_only"
            }
            .into(),
        );
        info.base_address = Some(format!("0x{:x}", b.addr));
        info.offset_bytes = Some(target - b.addr);
        info.active = complete.then_some(!b.freed);
        info.within_extent = Some(target - b.addr < b.size);
    } else {
        let mut matches = known.iter().filter(|obj| {
            target
                .checked_sub(obj.base)
                .is_some_and(|offset| offset < obj.size)
        });
        if let Some(first) = matches.next() {
            // Recursive sightings of a static are aliases only with a
            // definite identical identity AND extent. Equal addresses,
            // names, or stack identities alone never justify deduplication.
            let unique = matches.all(|other| {
                matches!(first.scope, "global" | "static")
                    && other.scope == first.scope
                    && first.identity.is_some()
                    && first.identity == other.identity
                    && first.base == other.base
                    && first.size == other.size
            });
            if !unique {
                return Some(info);
            }
            info.kind = match first.scope {
                "global" => "global",
                "static" => "static",
                "stack" => "stack",
                _ => "unknown",
            }
            .into();
            info.base_address = Some(format!("0x{:x}", first.base));
            info.offset_bytes = Some(target - first.base);
            info.variable_identity = first.identity.map(str::to_owned);
        } else if let Some(kind) = pid.and_then(|pid| mapping_kind(pid, target)) {
            info.kind = kind.into();
        }
    }
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ScopeStatus;

    fn var(
        name: &str,
        addr: Option<&str>,
        size: Option<u64>,
        scope: &str,
        identity: Option<&str>,
    ) -> VariableInfo {
        VariableInfo {
            name: name.into(),
            address: addr.map(|a| a.into()),
            size_bytes: size,
            scope: Some(scope.into()),
            identity: identity.map(|i| i.into()),
            ..Default::default()
        }
    }

    fn target_of(vars: &[VariableInfo], i: usize) -> PointerTarget {
        vars[i].target.clone().unwrap()
    }

    #[test]
    fn dangling_and_reused_addresses_do_not_prove_pointer_origin() {
        let mut t = HeapTimeline::new(10);
        for l in [
            "1 1 malloc 1 1000 8 0 0 401000 ok",
            "1 2 free 1 1000 8 0 0 401000 ok",
        ] {
            t.ingest(&crate::allocation_tracker::parse_event_line(l).unwrap());
        }
        let mut vars = [VariableInfo {
            points_to: Some("0x1004".into()),
            ..Default::default()
        }];
        enrich(&mut vars, &[], &t, Some(1));
        assert_eq!(vars[0].target.as_ref().unwrap().active, Some(false));
        assert_eq!(vars[0].target.as_ref().unwrap().offset_bytes, Some(4));
        assert_eq!(vars[0].target.as_ref().unwrap().allocation_id, None);
        assert_eq!(
            vars[0].target.as_ref().unwrap().address_match_allocation_id,
            Some(1)
        );
        assert_eq!(
            vars[0]
                .target
                .as_ref()
                .unwrap()
                .provenance_status
                .as_deref(),
            Some("known_freed_address")
        );
        t.ingest(
            &crate::allocation_tracker::parse_event_line("1 3 malloc 2 1000 8 0 0 401000 ok")
                .unwrap(),
        );
        enrich(&mut vars, &[], &t, Some(1));
        assert_eq!(vars[0].target.as_ref().unwrap().allocation_id, None);
        assert_eq!(
            vars[0].target.as_ref().unwrap().address_match_allocation_id,
            Some(2)
        );
        assert_eq!(
            vars[0]
                .target
                .as_ref()
                .unwrap()
                .provenance_status
                .as_deref(),
            Some("ambiguous_reuse")
        );
    }

    #[test]
    fn unique_live_numeric_containment_is_not_pointer_provenance() {
        let mut timeline = HeapTimeline::new(10);
        timeline.ingest(
            &crate::allocation_tracker::parse_event_line("1 1 malloc 7 2000 16 0 0 401000 ok")
                .unwrap(),
        );
        let mut vars = [VariableInfo {
            // This numeric value could be reconstructed from an integer; no
            // observed pointer assignment establishes its allocation origin.
            points_to: Some("0x2004".into()),
            ..Default::default()
        }];
        enrich(&mut vars, &[], &timeline, Some(1));
        let target = target_of(&vars, 0);
        assert_eq!(target.kind, "heap");
        assert_eq!(target.address_match_allocation_id, Some(7));
        assert_eq!(target.provenance_status.as_deref(), Some("address_only"));
        assert_eq!(target.base_address.as_deref(), Some("0x2000"));
        assert_eq!(target.offset_bytes, Some(4));
        assert_eq!(target.active, Some(true));
        assert_eq!(target.within_extent, Some(true));
        assert_eq!(target.allocation_id, None);

        timeline.ingest(
            &crate::allocation_tracker::parse_event_line("1 2 malloc 8 3000 8 0 0 401000 ok")
                .unwrap(),
        );
        vars[0].points_to = Some("0x3000".into());
        enrich(&mut vars, &[], &timeline, Some(1));
        let reassigned = target_of(&vars, 0);
        assert_eq!(reassigned.address_match_allocation_id, Some(8));
        assert_eq!(reassigned.base_address.as_deref(), Some("0x3000"));
        assert_eq!(reassigned.offset_bytes, Some(0));
        assert_eq!(
            reassigned.provenance_status.as_deref(),
            Some("address_only")
        );
        assert_eq!(reassigned.allocation_id, None);
    }

    #[test]
    fn realloc_lifetimes_remain_address_candidates_not_pointer_origins() {
        let mut timeline = HeapTimeline::new(10);
        let mut vars = [VariableInfo {
            points_to: Some("0x1004".into()),
            ..Default::default()
        }];
        for (line, candidate, status, active) in [
            (
                "1 1 malloc 1 1000 12 0 0 401000 ok",
                1,
                "address_only",
                Some(true),
            ),
            (
                "1 2 realloc 2 1000 48 1 1000 401010 ok_same",
                2,
                "ambiguous_reuse",
                Some(true),
            ),
            (
                "1 3 realloc 3 2000 96 2 1000 401030 ok_moved",
                2,
                "ambiguous_reuse",
                Some(false),
            ),
        ] {
            timeline.ingest(&crate::allocation_tracker::parse_event_line(line).unwrap());
            enrich(&mut vars, &[], &timeline, Some(1));
            let target = target_of(&vars, 0);
            assert_eq!(target.address_match_allocation_id, Some(candidate));
            assert_eq!(target.provenance_status.as_deref(), Some(status));
            assert_eq!(target.active, active);
            assert_eq!(target.allocation_id, None);
        }
        vars[0].points_to = Some("0x2004".into());
        enrich(&mut vars, &[], &timeline, Some(1));
        let moved = target_of(&vars, 0);
        assert_eq!(moved.address_match_allocation_id, Some(3));
        assert_eq!(moved.provenance_status.as_deref(), Some("address_only"));
        assert_eq!(moved.active, Some(true));
        assert_eq!(moved.allocation_id, None);
    }

    #[test]
    fn unique_stack_variable_target_reuses_the_recorded_identity() {
        // int x at 0x1000; int *p = &x stored inside p (address 0x2000);
        // int a[4] at 0x1100 for the interior-pointer case.
        let mut vars = [
            var(
                "x",
                Some("0x1000"),
                Some(4),
                "stack",
                Some("activation:7:block:100-200:3:x"),
            ),
            var(
                "a",
                Some("0x1100"),
                Some(16),
                "stack",
                Some("activation:7:block:100-200:4:a"),
            ),
            var(
                "p",
                Some("0x2000"),
                Some(8),
                "stack",
                Some("activation:7:block:100-200:5:p"),
            ),
        ];
        vars[2].points_to = Some("0x1000".into());
        enrich(&mut vars, &[], &HeapTimeline::new(10), Some(1));
        let t = target_of(&vars, 2);
        assert_eq!(t.kind, "stack");
        assert_eq!(t.base_address.as_deref(), Some("0x1000"));
        assert_eq!(t.offset_bytes, Some(0));
        assert_eq!(
            t.variable_identity.as_deref(),
            Some("activation:7:block:100-200:3:x")
        );
        // A pointer into the aggregate stays attributed to the container
        // with its offset preserved; no element identity is fabricated.
        vars[2].points_to = Some("0x1108".into());
        enrich(&mut vars, &[], &HeapTimeline::new(10), Some(1));
        let t = target_of(&vars, 2);
        assert_eq!(t.kind, "stack");
        assert_eq!(t.base_address.as_deref(), Some("0x1100"));
        assert_eq!(t.offset_bytes, Some(8));
        assert_eq!(
            t.variable_identity.as_deref(),
            Some("activation:7:block:100-200:4:a")
        );
    }

    #[test]
    fn global_target_reuses_the_global_symbol_identity() {
        let mut vars = [var(
            "gp",
            Some("0x3000"),
            Some(8),
            "stack",
            Some("activation:1:block:100-200:2:gp"),
        )];
        vars[0].points_to = Some("0x4000".into());
        let globals = [var(
            "g_value",
            Some("0x4000"),
            Some(4),
            "global",
            Some("symbol:'x.c'::g_value"),
        )];
        enrich(&mut vars, &globals, &HeapTimeline::new(10), Some(1));
        let t = target_of(&vars, 0);
        assert_eq!(t.kind, "global");
        assert_eq!(
            t.variable_identity.as_deref(),
            Some("symbol:'x.c'::g_value")
        );
        // A global pointer resolves against globals and frame-0 variables.
        let mut globals = [var(
            "gp",
            Some("0x3000"),
            Some(8),
            "global",
            Some("symbol:'x.c'::gp"),
        )];
        globals[0].points_to = Some("0x1000".into());
        let locals = [var(
            "x",
            Some("0x1000"),
            Some(4),
            "stack",
            Some("activation:2:block:100-200:3:x"),
        )];
        enrich(&mut globals, &locals, &HeapTimeline::new(10), Some(1));
        let t = target_of(&globals, 0);
        assert_eq!(t.kind, "stack");
        assert_eq!(
            t.variable_identity.as_deref(),
            Some("activation:2:block:100-200:3:x")
        );
    }

    #[test]
    fn overlapping_candidates_stay_unknown() {
        let mut vars = [
            var(
                "a",
                Some("0x1000"),
                Some(16),
                "stack",
                Some("activation:1:block:100-200:1:a"),
            ),
            var(
                "b",
                Some("0x1000"),
                Some(16),
                "stack",
                Some("activation:1:block:100-200:2:b"),
            ),
            var(
                "p",
                None,
                Some(8),
                "stack",
                Some("activation:1:block:100-200:3:p"),
            ),
        ];
        vars[2].points_to = Some("0x1004".into());
        enrich(&mut vars, &[], &HeapTimeline::new(10), Some(1));
        let t = target_of(&vars, 2);
        assert_eq!(t.kind, "unknown");
        assert_eq!(t.base_address, None);
        assert_eq!(t.offset_bytes, None);
        assert_eq!(t.variable_identity, None);
    }

    #[test]
    fn placeholder_and_missing_identities_are_never_attached() {
        let mut vars = [
            var(
                "u",
                Some("0x1000"),
                Some(4),
                "stack",
                Some("unresolved:1:block:100-200:1:u"),
            ),
            var("p", Some("0x2000"), Some(8), "stack", None),
        ];
        vars[1].points_to = Some("0x1000".into());
        enrich(&mut vars, &[], &HeapTimeline::new(10), Some(1));
        let t = target_of(&vars, 1);
        assert_eq!(t.kind, "stack");
        assert_eq!(t.variable_identity, None);
    }

    #[test]
    fn heap_address_candidate_takes_precedence_over_a_variable() {
        // A tracked heap block overlaps the extent of a recorded variable.
        let mut t = HeapTimeline::new(10);
        t.ingest(
            &crate::allocation_tracker::parse_event_line("1 1 malloc 1 2000 16 0 0 401000 ok")
                .unwrap(),
        );
        let mut vars = [
            var(
                "shadow",
                Some("0x2000"),
                Some(16),
                "stack",
                Some("activation:1:block:100-200:1:shadow"),
            ),
            var(
                "p",
                Some("0x3000"),
                Some(8),
                "stack",
                Some("activation:1:block:100-200:2:p"),
            ),
        ];
        vars[1].points_to = Some("0x2004".into());
        enrich(&mut vars, &[], &t, Some(1));
        let info = target_of(&vars, 1);
        assert_eq!(info.kind, "heap");
        assert_eq!(info.address_match_allocation_id, Some(1));
        assert_eq!(info.allocation_id, None);
        assert_eq!(info.provenance_status.as_deref(), Some("address_only"));
        assert_eq!(info.offset_bytes, Some(4));
        assert_eq!(info.active, Some(true));
        assert_eq!(info.variable_identity, None);
    }

    #[test]
    fn null_unreadable_and_unmatched_targets_stay_unresolved() {
        let mut vars = [
            var(
                "nul",
                Some("0x1000"),
                Some(8),
                "stack",
                Some("activation:1:block:100-200:1:nul"),
            ),
            var(
                "junk",
                Some("0x1008"),
                Some(8),
                "stack",
                Some("activation:1:block:100-200:2:junk"),
            ),
        ];
        vars[0].points_to = Some("0x0".into());
        vars[1].points_to = Some("0xdeadbeef".into());
        enrich(&mut vars, &[], &HeapTimeline::new(10), None);
        let null_target = target_of(&vars, 0);
        assert_eq!(null_target.kind, "null");
        assert_eq!(null_target.variable_identity, None);
        // No /proc region known in tests: kind stays the default, no identity.
        let junk_target = target_of(&vars, 1);
        assert_eq!(junk_target.kind, "unknown");
        assert_eq!(junk_target.variable_identity, None);
    }

    #[test]
    fn objects_without_recorded_extent_are_never_candidates() {
        // size_bytes is None: the object cannot bound a target address.
        let mut vars = [
            var(
                "x",
                Some("0x1000"),
                None,
                "stack",
                Some("activation:1:block:100-200:1:x"),
            ),
            var(
                "p",
                Some("0x2000"),
                Some(8),
                "stack",
                Some("activation:1:block:100-200:2:p"),
            ),
        ];
        vars[1].points_to = Some("0x1000".into());
        enrich(&mut vars, &[], &HeapTimeline::new(10), None);
        let t = target_of(&vars, 1);
        assert_eq!(t.kind, "unknown");
        assert_eq!(t.variable_identity, None);
    }

    #[test]
    fn scope_status_travels_on_the_recorded_variable() {
        let mut v = var(
            "x",
            Some("0x1000"),
            Some(4),
            "stack",
            Some("activation:1:block:100-200:1:x"),
        );
        v.scope_status = Some(ScopeStatus::Active);
        let mut vars = [v];
        vars[0].points_to = Some("0x1000".into());
        enrich(&mut vars, &[], &HeapTimeline::new(10), Some(1));
        assert_eq!(vars[0].scope_status, Some(ScopeStatus::Active));
    }

    fn multi_frame_snapshot() -> ExecutionSnapshot {
        let mut local = var(
            "local",
            Some("0x1000"),
            Some(4),
            "stack",
            Some("activation:2:local"),
        );
        local.activation_id = Some(2);
        local.raw_bytes_hex = Some("01000000".into());
        local.points_to = Some("0x2000".into());
        let mut caller = var(
            "caller",
            Some("0x2000"),
            Some(8),
            "stack",
            Some("activation:1:caller"),
        );
        caller.activation_id = Some(1);
        caller.points_to = Some("0x3004".into());
        let mut global = var(
            "global",
            Some("0x3000"),
            Some(16),
            "global",
            Some("symbol:global"),
        );
        global.points_to = Some("0x1000".into());
        let frame = StackFrameInfo {
            level: 1,
            func: "main".into(),
            ..Default::default()
        };
        ExecutionSnapshot {
            variables: vec![local],
            caller_frames: vec![FrameVariables {
                frame: frame.clone(),
                activation_id: 1,
                variables: vec![caller],
                ..Default::default()
            }],
            call_stack: vec![StackFrameInfo::default(), frame],
            observation: SnapshotObservation {
                globals: vec![global],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn snapshot_enriches_all_frames_and_globals_without_moving_buffers() {
        let mut snapshot = multi_frame_snapshot();
        let vars = snapshot.variables.as_ptr();
        let bytes = snapshot.variables[0]
            .raw_bytes_hex
            .as_ref()
            .unwrap()
            .as_ptr();
        let callers = snapshot.caller_frames[0].variables.as_ptr();
        let globals = snapshot.observation.globals.as_ptr();
        enrich_snapshot(&mut snapshot, &HeapTimeline::new(10), None);
        assert_eq!(snapshot.variables.as_ptr(), vars);
        assert_eq!(
            snapshot.variables[0]
                .raw_bytes_hex
                .as_ref()
                .unwrap()
                .as_ptr(),
            bytes
        );
        assert_eq!(snapshot.caller_frames[0].variables.as_ptr(), callers);
        assert_eq!(snapshot.observation.globals.as_ptr(), globals);
        assert_eq!(snapshot.variables.len(), 1);
        assert_eq!(
            target_of(&snapshot.variables, 0)
                .variable_identity
                .as_deref(),
            Some("activation:1:caller")
        );
        let target = target_of(&snapshot.caller_frames[0].variables, 0);
        assert_eq!(target.variable_identity.as_deref(), Some("symbol:global"));
        assert_eq!(target.offset_bytes, Some(4));
        assert_eq!(
            target_of(&snapshot.observation.globals, 0)
                .variable_identity
                .as_deref(),
            Some("activation:2:local")
        );
    }

    #[test]
    fn dead_inactive_and_unassociated_caller_objects_are_not_candidates() {
        let timeline = HeapTimeline::new(10);
        let mut snapshot = multi_frame_snapshot();
        enrich_snapshot(&mut snapshot, &timeline, None);
        snapshot.caller_frames.clear();
        enrich_snapshot(&mut snapshot, &timeline, None);
        assert_eq!(target_of(&snapshot.variables, 0).kind, "unknown");
        for invalid in 0..3 {
            let mut snapshot = multi_frame_snapshot();
            match invalid {
                0 => {
                    snapshot.caller_frames[0].variables[0].scope_status =
                        Some(ScopeStatus::Inactive)
                }
                1 => snapshot.caller_frames[0].variables[0].activation_id = Some(9),
                _ => snapshot.call_stack.pop().map(|_| ()).unwrap(),
            }
            enrich_snapshot(&mut snapshot, &timeline, None);
            assert_eq!(target_of(&snapshot.variables, 0).kind, "unknown");
        }
    }

    #[test]
    fn recursive_statics_dedup_only_with_proven_identity_and_extent() {
        let mut snapshot = multi_frame_snapshot();
        let mut static_var = var(
            "s",
            Some("0x4000"),
            Some(16),
            "static",
            Some("static:block:s"),
        );
        static_var.activation_id = Some(2);
        snapshot.variables.push(static_var.clone());
        static_var.activation_id = Some(1);
        snapshot.caller_frames[0].variables.push(static_var);
        snapshot.variables[0].points_to = Some("0x4004".into());
        let timeline = HeapTimeline::new(10);
        enrich_snapshot(&mut snapshot, &timeline, None);
        let target = target_of(&snapshot.variables, 0);
        assert_eq!(target.kind, "static");
        assert_eq!(target.variable_identity.as_deref(), Some("static:block:s"));
        for invalid in 0..4 {
            let mut snapshot = snapshot.clone();
            let alias = &mut snapshot.caller_frames[0].variables[1];
            match invalid {
                0 => alias.size_bytes = Some(8),
                1 => alias.identity = Some("static:other:s".into()),
                2 => alias.identity = Some("unresolved:1:s".into()),
                _ => alias.scope = Some("stack".into()),
            }
            enrich_snapshot(&mut snapshot, &timeline, None);
            let target = target_of(&snapshot.variables, 0);
            assert_eq!(target.kind, "unknown");
            assert_eq!(target.variable_identity, None);
            assert_eq!(target.base_address, None);
        }
    }

    #[test]
    fn multi_frame_heap_precedence_and_cross_frame_ambiguity() {
        let mut snapshot = multi_frame_snapshot();
        snapshot.variables.push(var(
            "overlap",
            Some("0x2000"),
            Some(8),
            "stack",
            Some("activation:2:overlap"),
        ));
        let mut timeline = HeapTimeline::new(10);
        enrich_snapshot(&mut snapshot, &timeline, Some(1));
        assert_eq!(target_of(&snapshot.variables, 0).kind, "unknown");
        timeline.ingest(
            &crate::allocation_tracker::parse_event_line("1 1 malloc 1 2000 16 0 0 401000 ok")
                .unwrap(),
        );
        enrich_snapshot(&mut snapshot, &timeline, Some(1));
        let target = target_of(&snapshot.variables, 0);
        assert_eq!(target.kind, "heap");
        assert_eq!(target.address_match_allocation_id, Some(1));
        assert_eq!(target.allocation_id, None);
        assert_eq!(target.variable_identity, None);
        snapshot.caller_frames[0].variables[0].points_to = Some("0x2004".into());
        enrich_snapshot(&mut snapshot, &timeline, Some(1));
        let caller = target_of(&snapshot.caller_frames[0].variables, 0);
        assert_eq!(caller.address_match_allocation_id, Some(1));
        assert_eq!(caller.provenance_status.as_deref(), Some("address_only"));
        assert_eq!(caller.offset_bytes, Some(4));
        assert_eq!(caller.allocation_id, None);
        // Invalidated pointer evidence must not keep an earlier resolution.
        snapshot.variables[0].points_to = None;
        enrich_snapshot(&mut snapshot, &timeline, Some(1));
        assert!(snapshot.variables[0].target.is_none());
    }
}

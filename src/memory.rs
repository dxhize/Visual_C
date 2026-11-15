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

pub fn enrich(
    vars: &mut [VariableInfo],
    globals: &[VariableInfo],
    timeline: &HeapTimeline,
    pid: Option<u32>,
) {
    let known: Vec<_> = vars
        .iter()
        .chain(globals)
        .filter_map(|v| {
            Some((
                address(v.address.as_deref()?)?,
                v.size_bytes?,
                v.identity.clone(),
                v.scope.clone(),
            ))
        })
        .collect();
    for var in vars {
        let Some(target) = var.points_to.as_deref().and_then(address) else {
            continue;
        };
        let mut info = PointerTarget::default();
        if target == 0 {
            info.kind = "null".into();
        } else if let Some(b) = timeline.blocks().iter().rev().find(|b| {
            Some(b.pid) == pid
                && target >= b.addr
                && target
                    .checked_sub(b.addr)
                    .is_some_and(|off| off < b.size || (off == 0 && b.size == 0))
        }) {
            info.kind = "heap".into();
            info.allocation_id = Some(timeline.global_id(b.pid, b.allocation_id));
            info.base_address = Some(format!("0x{:x}", b.addr));
            info.offset_bytes = Some(target - b.addr);
            info.active = Some(!b.freed);
            info.within_extent = Some(target - b.addr < b.size);
        } else if let Some((base, _, identity, scope)) = known
            .iter()
            .find(|(base, size, _, _)| target >= *base && target - base < *size)
        {
            info.kind = match scope.as_deref() {
                Some("global") => "global",
                Some("static") => "static",
                Some("stack") => "stack",
                _ => "unknown",
            }
            .into();
            info.base_address = Some(format!("0x{base:x}"));
            info.offset_bytes = Some(target - base);
            info.variable_identity = identity.clone();
        } else if let Some(kind) = pid.and_then(|pid| mapping_kind(pid, target)) {
            info.kind = kind.into();
        }
        var.target = Some(info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dangling_is_not_null_and_address_reuse_prefers_latest_identity() {
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
        t.ingest(
            &crate::allocation_tracker::parse_event_line("1 3 malloc 2 1000 8 0 0 401000 ok")
                .unwrap(),
        );
        enrich(&mut vars, &[], &t, Some(1));
        assert_eq!(vars[0].target.as_ref().unwrap().allocation_id, Some(2));
    }
}

//! Phase 3 gate: exercise gimli + object on a real GCC DWARF v5 executable.
use gimli::{EndianRcSlice, Reader, RunTimeEndian};
use object::{Object, ObjectSection};
use std::{process::Command, rc::Rc};

#[test]
fn gcc_dwarf_v5_provides_cu_struct_member_array_and_declaration() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/c/dwarf_types.c");
    let output = std::env::temp_dir().join(format!("c-trace-dwarf-probe-{}", std::process::id()));
    let status = Command::new("gcc")
        .args(["-g", "-gdwarf-5", "-O0", "-o"])
        .arg(&output)
        .arg(fixture)
        .status()
        .unwrap();
    assert!(status.success());
    let bytes = std::fs::read(&output).unwrap();
    let object = object::File::parse(&*bytes).unwrap();
    let endian = if object.is_little_endian() {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    };
    let dwarf = gimli::Dwarf::load(|section| {
        let data = object
            .section_by_name(section.name())
            .map(|s| s.uncompressed_data().unwrap().into_owned())
            .unwrap_or_default();
        Ok::<_, gimli::Error>(EndianRcSlice::new(Rc::from(data), endian))
    })
    .unwrap();
    let mut units = dwarf.units();
    let mut saw_cu = false;
    let mut saw_struct = false;
    let mut saw_member = false;
    let mut saw_array = false;
    let mut saw_named_array = false;
    let mut saw_decl = false;
    while let Some(header) = units.next().unwrap() {
        saw_cu = true;
        assert_eq!(header.version(), 5);
        let unit = dwarf.unit(header).unwrap();
        let mut entries = unit.entries();
        let mut node_depth = None;
        while let Some(entry) = entries.next_dfs().unwrap() {
            if node_depth.is_some_and(|d| entry.depth() <= d) {
                node_depth = None;
            }
            let name = entry.attr(gimli::DW_AT_name).and_then(|a| {
                dwarf
                    .attr_string(&unit, a.value())
                    .ok()
                    .and_then(|r| r.to_string_lossy().ok().map(|s| s.into_owned()))
            });
            match entry.tag() {
                gimli::DW_TAG_structure_type if name.as_deref() == Some("Node") => {
                    let size = entry
                        .attr(gimli::DW_AT_byte_size)
                        .and_then(|a| a.udata_value());
                    saw_struct = size.is_some();
                    if object.architecture() == object::Architecture::X86_64 {
                        assert_eq!(size, Some(24)); // GCC/x86-64 ABI, not a portable C constant.
                    }
                    node_depth = Some(entry.depth());
                }
                gimli::DW_TAG_member
                    if node_depth.is_some_and(|d| entry.depth() == d + 1)
                        && name.as_deref() == Some("next") =>
                {
                    let offset = entry
                        .attr(gimli::DW_AT_data_member_location)
                        .and_then(|a| a.udata_value());
                    saw_member = offset.is_some();
                    if object.architecture() == object::Architecture::X86_64 {
                        assert_eq!(offset, Some(8));
                    }
                }
                gimli::DW_TAG_array_type => {
                    let offset = entry.offset();
                    let mut cursor = unit.entries_at_offset(offset).unwrap();
                    let mut subrange = false;
                    let _root = cursor.next_dfs().unwrap();
                    while let Some(child) = cursor.next_dfs().unwrap() {
                        if child.depth() <= 0 {
                            break;
                        }
                        if child.tag() == gimli::DW_TAG_subrange_type {
                            let upper = child
                                .attr(gimli::DW_AT_upper_bound)
                                .and_then(|a| a.udata_value());
                            subrange |= upper == Some(4);
                        }
                    }
                    saw_array |= subrange;
                }
                gimli::DW_TAG_typedef if name.as_deref() == Some("Five") => {
                    if let Some(gimli::AttributeValue::UnitRef(o)) =
                        entry.attr(gimli::DW_AT_type).map(|a| a.value())
                    {
                        saw_named_array = unit
                            .entry(o)
                            .is_ok_and(|t| t.tag() == gimli::DW_TAG_array_type);
                    }
                }
                gimli::DW_TAG_variable if name.as_deref() == Some("global_array") => {
                    saw_decl = entry.attr(gimli::DW_AT_decl_file).is_some()
                        && entry
                            .attr(gimli::DW_AT_decl_line)
                            .and_then(|a| a.udata_value())
                            == Some(19);
                }
                _ => {}
            }
        }
    }
    std::fs::remove_file(output).unwrap();
    assert!(saw_cu && saw_struct && saw_member && saw_array && saw_named_array && saw_decl,
        "gate: cu={saw_cu}, struct={saw_struct}, member={saw_member}, array={saw_array}, named_array={saw_named_array}, declaration={saw_decl}");
}

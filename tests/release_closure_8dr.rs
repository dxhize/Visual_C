//! Independent GCC/native/Valgrind executions: identical numeric mappings
//! cannot identify allocation lifetimes across processes.
use c_trace_engine::valgrind::{correlate_with_allocations, parse_valgrind_xml};
use std::path::PathBuf;
use std::process::Command;

#[tokio::test]
async fn independent_same_address_and_different_address_runs_never_link_heap_ids() {
    for tool in ["gcc", "valgrind"] {
        if c_trace_engine::compiler::probe_tool(tool, "--version")
            .await
            .is_err()
        {
            eprintln!("skipped: {tool} unavailable");
            return;
        }
    }
    let dir = std::env::temp_dir().join(format!("cte-8dr-address-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/c/phase8dr_address.c");
    let binary = dir.join("native");
    let build = Command::new("gcc")
        .args(["-g", "-O0"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = |alt: bool| {
        let mut cmd = Command::new(&binary);
        if alt {
            cmd.arg("alt");
        }
        let result = cmd.output().unwrap();
        assert!(result.status.success());
        String::from_utf8(result.stdout).unwrap()
    };
    let first = run(false);
    let second = run(false);
    let different = run(true);
    assert!(first.contains("mapped=0x500000000000"), "{first}");
    assert!(second.contains("mapped=0x500000000000"), "{second}");
    assert!(different.contains("mapped=0x500000100000"), "{different}");
    assert_ne!(first, second, "separate PIDs, same numeric mapping");
    let xml = dir.join("valgrind.xml");
    let valgrind = Command::new("timeout")
        .arg("15")
        .arg("valgrind")
        .arg("--tool=memcheck")
        .arg("--xml=yes")
        .arg(format!("--xml-file={}", xml.display()))
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        valgrind.status.success(),
        "{}",
        String::from_utf8_lossy(&valgrind.stderr)
    );
    let output = String::from_utf8(valgrind.stdout).unwrap();
    assert!(output.contains("mapped=0x500000000000"), "{output}");
    assert_ne!(output, first, "Valgrind runs in a third PID");
    let mut report = parse_valgrind_xml(&std::fs::read_to_string(xml).unwrap()).unwrap();
    let error = report
        .detailed_errors
        .iter()
        .find(|e| e.kind == "InvalidRead")
        .unwrap();
    let frame = error
        .stack
        .iter()
        .find(|f| f.file.as_deref() == Some("phase8dr_address.c"))
        .unwrap();
    let line = frame.line.unwrap();
    correlate_with_allocations(
        &mut report,
        &[
            (source.display().to_string(), line, 7),
            (
                format!(
                    "/elsewhere/{}",
                    source.file_name().unwrap().to_string_lossy()
                ),
                line,
                99,
            ),
        ],
    );
    let error = report
        .detailed_errors
        .iter()
        .find(|e| e.kind == "InvalidRead")
        .unwrap();
    assert!(error.related_allocation_ids.is_empty());
    assert_eq!(error.source_site_candidates.len(), 1);
    assert_eq!(error.source_site_candidates[0].line, line);
    assert!(error.what.contains("Invalid read"));
    std::fs::remove_dir_all(dir).unwrap();
}

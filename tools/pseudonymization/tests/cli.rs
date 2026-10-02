use std::process::Command;

#[test]
fn existing_report_cannot_be_reused_by_a_new_job() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("report.json");
    std::fs::write(&output, "earlier report").unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mdoc-pseudonymization-qualification"))
        .args([
            "run",
            "missing-adapter",
            "multi-v2.1-q8",
            "missing-models",
            "missing-corpus",
        ])
        .arg(&output)
        .args(["missing-runtime", "test machine"])
        .output()
        .unwrap();
    assert!(!status.status.success());
    assert!(String::from_utf8_lossy(&status.stderr).contains("output already exists"));
    assert_eq!(std::fs::read_to_string(output).unwrap(), "earlier report");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

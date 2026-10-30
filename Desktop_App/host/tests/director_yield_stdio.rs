//! Actual binary publication boundary (including the last JSON redaction pass).
#[test]
fn real_stdio_yield_cap_and_history_resume() {
    for args in [vec![], vec!["--no-discriminator"]] {
        let status = std::process::Command::new("python3")
            .arg(format!(
                "{}/tests/director_yield_stdio.py",
                env!("CARGO_MANIFEST_DIR")
            ))
            .arg(env!("CARGO_BIN_EXE_workbench-host"))
            .args(args)
            .status()
            .expect("launch loopback-only stdio regression");
        assert!(
            status.success(),
            "real stdio yield regression failed: {status}"
        );
    }
}

//! Exercise the actual binary's eight-worker/64-queued-request admission path.
#[test]
fn stdio_stop_bypasses_saturated_workers_and_queue_without_resurrecting_requests() {
    let scratch = tempfile::tempdir().unwrap();
    let result = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/director_stdio_admission.py"
        ))
        .arg("--binary")
        .arg(env!("CARGO_BIN_EXE_workbench-host"))
        .env("TMPDIR", scratch.path())
        .output()
        .expect("python3 is required for the loopback stdio fixture");
    println!("{}", String::from_utf8_lossy(&result.stdout));
    assert!(
        result.status.success(),
        "stdio admission failure: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

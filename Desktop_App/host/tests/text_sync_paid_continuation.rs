//! Connected native paid-stage recovery, using ONLY loopback mock-provider fixtures.
//! The ordinary provider binary is refused by the Python driver before any operation.
#[cfg(feature = "mock-provider")]
#[test]
fn talking_shot_paid_stages_resume_without_duplicate_speech_or_video() {
    use std::{path::Path, process::Command};
    let evidence = tempfile::tempdir().expect("owned test evidence");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/text_sync_paid_continuation.py");
    let mut cases: Vec<(&str, Vec<&str>)> = vec![
        ("native", vec![]),
        ("legacy_saved_wav", vec!["--legacy"]),
        ("pending_collect", vec!["--pending-video", "--collect-twice"]),
    ];
    for case in ["stop-before-confirm", "same-root-reopen", "input-change", "route-change", "newer-card", "stop-after-audio", "wav-change", "duplicate-confirmation"] {
        cases.push((case, vec!["--negative", case]));
    }
    for (name, flags) in cases {
        let output = Command::new("python3")
            .env_remove("PYTHONOPTIMIZE")
            .arg(&script).args(flags)
            .arg("--host").arg(env!("CARGO_BIN_EXE_workbench-host"))
            .arg("--evidence").arg(evidence.path().join(name))
            .output().expect("run isolated native fixture");
        assert!(output.status.success(), "{name}: {:?}\n{}\n{}", output.status.code(), String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        println!("native fixture {name}: PASS");
    }
}

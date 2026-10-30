use serde_json::{json, Value};
use std::{fs, process::Command};
use workbench_host::{
    director_actions::{validate_action, DirectorAction},
    specialists,
};

#[test]
fn validated_inspect_measures_real_source_without_writing_or_provider_calls() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("asymmetric.wav");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "aevalsrc=if(between(t\\,0.7\\,1.8)\\,0.3*sin(2*PI*440*t)\\,0):s=16000:d=2.4",
            "-y",
        ])
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let before = fs::read(&source).unwrap();
    workbench_host::workflow::import::persist_state(
        root.path(),
        &workbench_host::workflow::state::ProjectState::default(),
    )
    .unwrap();
    workbench_host::session::open(root.path().to_str().unwrap()).unwrap();
    let files_before = fs::read_dir(root.path()).unwrap().count();
    let action: DirectorAction = serde_json::from_value(
        json!({"action":"inspect","op":"media_probe","path":"asymmetric.wav"}),
    )
    .unwrap();
    let validated = validate_action(action, root.path().to_str()).unwrap();
    let output = specialists::run_inspect(&validated, root.path().to_str()).unwrap();
    let result: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(result["has_audio"], true);
    assert!(
        (result["candidate_first_activity_seconds"].as_f64().unwrap() - 0.7).abs() < 0.03,
        "{output}"
    );
    assert!(
        (result["candidate_last_activity_seconds"].as_f64().unwrap() - 1.8).abs() < 0.03,
        "{output}"
    );
    assert_eq!(fs::read(&source).unwrap(), before);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), files_before);
    workbench_host::session::close();
}

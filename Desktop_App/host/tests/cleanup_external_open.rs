//! FX01: production dispatcher authorization, not cfg(test) filesystem bypasses.
use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};

#[test]
fn fx01_selected_external_classify_then_read_spends_only_the_content_read() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    let plan = b"retained Plan bytes";
    fs::write(root.join("Preproduction/workflow_state.json"), plan).unwrap();
    fs::write(root.join("Scripts/keep.md"), b"retained script").unwrap();
    let external = dir.path().join("external.md");
    let neighbor = dir.path().join("unselected.md");
    let text = "# External text\r\nScene 1 — Exact UTF-8\r\nAlice: Hello!\r\n";
    fs::write(&external, text).unwrap();
    fs::write(&neighbor, b"unselected bytes").unwrap();
    fs::write(root.join("local.txt"), b"local text").unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    let call = |cmd: &str, args| dispatch(&app, &recorder, cmd, &args);
    call("open_project", json!({"path":root})).unwrap();
    for cmd in ["classify_file", "read_text_file"] {
        assert!(call(cmd, json!({"path":external})).unwrap_err().contains("one-time grant"));
    }
    // This is the exact main-process grant issued after native dialog selection.
    call("grant_path", json!({"path":external,"intent":"read"})).unwrap();
    for cmd in ["classify_file", "read_text_file"] {
        assert!(call(cmd, json!({"path":neighbor})).unwrap_err().contains("one-time grant"));
    }
    assert_eq!(call("classify_file", json!({"path":external})).unwrap(), json!("text"));
    assert_eq!(call("read_text_file", json!({"path":external})).unwrap(), json!(text));
    for cmd in ["read_text_file", "classify_file", "read_media_bytes"] {
        let error = call(cmd, json!({"path":external})).unwrap_err();
        println!("FX01 replay {cmd}: {error}");
        assert!(error.contains("one-time grant"));
    }
    for _ in 0..2 {
        assert_eq!(call("classify_file", json!({"path":root.join("local.txt")})).unwrap(), json!("text"));
        assert_eq!(call("read_text_file", json!({"path":root.join("local.txt")})).unwrap(), json!("local text"));
    }
    assert_eq!(fs::read(&external).unwrap(), text.as_bytes());
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(), plan);
    assert_eq!(fs::read(root.join("Scripts/keep.md")).unwrap(), b"retained script");
    assert_eq!(fs::read_dir(root.join("Scripts")).unwrap().count(), 1);
    call("close_project", json!({})).unwrap();
}

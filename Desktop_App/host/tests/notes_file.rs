use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};

#[test]
fn notes_file_create_exact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();

    let notes = "  First line stays indented.\n\nUnicode: café, 雨, 🎬\nTrailing whitespace stays.  \n";
    let saved = dispatch(
        &app,
        &recorder,
        "save_project_notes",
        &json!({"projectRoot":root,"text":notes}),
    )
    .unwrap();

    assert_eq!(saved, json!("Notes.md"));
    assert_eq!(fs::read(root.join("Notes.md")).unwrap(), notes.as_bytes());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);

    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}

#[test]
fn notes_file_replace() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Notes.md"), b"old notes must disappear\n").unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();

    let latest = "  Latest notes only.\n\nUnicode remains exact: café, 雨, 🎬\n";
    let saved = dispatch(
        &app,
        &recorder,
        "save_project_notes",
        &json!({"projectRoot":root,"text":latest}),
    )
    .unwrap();

    assert_eq!(saved, json!("Notes.md"));
    assert_eq!(fs::read(root.join("Notes.md")).unwrap(), latest.as_bytes());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);

    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}

#[cfg(unix)]
#[test]
fn notes_file_confinement() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let outside = dir.path().join("outside.md");
    let original = b"outside stays unchanged\n";
    fs::write(&outside, original).unwrap();
    symlink(&outside, root.join("Notes.md")).unwrap();

    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();

    let result = dispatch(
        &app,
        &recorder,
        "save_project_notes",
        &json!({"projectRoot":root,"text":"redirected write must fail\n"}),
    );

    let error = result.expect_err("redirected Notes.md must be refused");
    assert!(error.contains("symlink"), "unexpected error: {error}");
    assert_eq!(fs::read(&outside).unwrap(), original);
    assert!(fs::symlink_metadata(root.join("Notes.md"))
        .unwrap()
        .file_type()
        .is_symlink());

    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}

//! Phase 4 §8.1 (Get_Studio_To_Production.md): the review-export open path
//! classifies the finished file before the app can show or open it, and
//! `classify_file` stays host-side. These tests pin that hop against the
//! real classifier: exported media kinds resolve to viewer classes, text
//! sidecars resolve to `text`, unknown bytes resolve to `unsupported`, and
//! a missing path is an error rather than a silent `unsupported`.

use std::fs;
use tempfile::TempDir;
use workbench_host::fs_ops::classify_file;

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> String {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path.to_string_lossy().to_string()
}

#[test]
fn review_export_media_classifies_for_the_open_hop() {
    let dir = TempDir::new().unwrap();
    // A review export is a video file; opening it needs `video`, and the
    // accompanying notes sidecar needs `text`.
    assert_eq!(
        classify_file(write(
            &dir,
            "Scene1_master.mp4",
            b"\x00\x00\x00\x18ftypmp42"
        )),
        Ok("video".into())
    );
    assert_eq!(
        classify_file(write(&dir, "Scene1_master.png", b"\x89PNG\r\n\x1a\n")),
        Ok("image".into())
    );
    assert_eq!(
        classify_file(write(&dir, "scene_notes.md", b"# notes\n")),
        Ok("text".into())
    );
}

#[test]
fn unknown_bytes_are_unsupported_not_text() {
    let dir = TempDir::new().unwrap();
    // Extensionless binary payload: must not be opened in the text viewer.
    let path = write(&dir, "blob.bin", &[0u8, 159, 12, 250, 3, 0]);
    assert_eq!(classify_file(path), Ok("unsupported".into()));
}

#[test]
fn missing_file_is_an_error_not_a_silent_class() {
    let dir = TempDir::new().unwrap();
    let ghost = dir.path().join("Final/Scene1_master.mp4");
    let error = classify_file(ghost.to_string_lossy().to_string())
        .expect_err("a path that is not a file must not classify");
    assert!(
        error.contains("Not a file"),
        "unexpected error text: {error}"
    );
}

#[test]
fn empty_path_is_rejected() {
    assert!(classify_file(String::new()).is_err());
}

use serde_json::json;
use std::{fs, path::Path, sync::Mutex};
use workbench_host::{ctx::HostCtx, dispatch, script, voice::RecorderState};

const TEXT: &str = "# Two friends\r\n\r\n## Scene 1 — Hello\r\nAlice: Hello!\r\n\r\n## Scene 2 — Goodbye\r\nBob: Goodbye!\r\n";
static SESSION: Mutex<()> = Mutex::new(());

fn project(test: impl FnOnce(&Path, &HostCtx, &RecorderState)) {
    let _lock = SESSION.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
    test(&root, &app, &recorder);
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}

fn ingest(app: &HostCtx, recorder: &RecorderState, root: &Path, source: &Path, kind: &str) -> Result<serde_json::Value, String> {
    dispatch(app, recorder, "ingest_script_file", &json!({"projectRoot":root,"sourcePath":source,"sourceKind":kind}))
}

#[test]
fn sc04_external_file_preserves_exact_bytes_and_identical_reimport() {
    project(|root, app, recorder| {
        let source = root.parent().unwrap().join("friends.md");
        fs::write(&source, TEXT).unwrap();
        for _ in 0..2 {
            dispatch(app, recorder, "grant_path", &json!({"path":source,"intent":"read"})).unwrap();
            assert_eq!(ingest(app, recorder, root, &source, "external-import").unwrap(), json!("Scripts/friends.md"));
            assert_eq!(fs::read(root.join("Scripts/friends.md")).unwrap(), TEXT.as_bytes());
            assert_eq!(fs::read(&source).unwrap(), TEXT.as_bytes());
        }
        assert_eq!(fs::read_dir(root.join("Scripts")).unwrap().count(), 1);
        assert!(!root.join("Preproduction").exists());
    });
}

#[test]
fn sc04_root_script_and_all_supported_extensions_preserve_sources() {
    project(|root, app, recorder| {
        for name in ["two-friends-saying-hello.md", "friends.markdown", "friends.txt", "friends.MD"] {
            fs::write(root.join(name), TEXT).unwrap();
            assert_eq!(ingest(app, recorder, root, Path::new(name), "existing-project-file").unwrap(), json!(format!("Scripts/{name}")));
            assert_eq!(fs::read(root.join("Scripts").join(name)).unwrap(), TEXT.as_bytes());
            assert_eq!(fs::read(root.join(name)).unwrap(), TEXT.as_bytes());
        }
        assert!(!root.join("Preproduction").exists());
    });
}

#[test]
fn sc04_already_in_scripts_is_not_copied_or_rewritten() {
    project(|root, app, recorder| {
        for relative in ["Scripts/friends.md", "Scripts/nested/friends.txt"] {
            let source = root.join(relative);
            fs::create_dir_all(source.parent().unwrap()).unwrap();
            fs::write(&source, TEXT).unwrap();
            let before = fs::metadata(&source).unwrap().modified().unwrap();
            assert_eq!(ingest(app, recorder, root, &source, "existing-project-file").unwrap(), json!(relative));
            assert_eq!(fs::metadata(&source).unwrap().modified().unwrap(), before);
            assert_eq!(fs::read(&source).unwrap(), TEXT.as_bytes());
        }
        assert!(!root.join("Scripts/friends.txt").exists());
    });
}

#[test]
fn sc04_collision_and_parse_failure_preserve_original_bytes() {
    project(|root, app, recorder| {
        fs::create_dir(root.join("Scripts")).unwrap();
        fs::write(root.join("Scripts/friends.md"), TEXT).unwrap();
        for text in ["Scene 1\nDifferent contents", "No scenes", ""] {
            fs::write(root.join("friends.md"), text).unwrap();
            assert!(ingest(app, recorder, root, Path::new("friends.md"), "existing-project-file").is_err());
            assert_eq!(fs::read(root.join("Scripts/friends.md")).unwrap(), TEXT.as_bytes());
            assert_eq!(fs::read(root.join("friends.md")).unwrap(), text.as_bytes());
        }
        fs::write(root.join("Scripts/invalid.md"), "No scenes").unwrap();
        assert!(ingest(app, recorder, root, Path::new("Scripts/invalid.md"), "existing-project-file").is_err());
        assert_eq!(fs::read(root.join("Scripts/invalid.md")).unwrap(), b"No scenes");
    });
}

#[test]
fn sc04_unreadable_invalid_utf8_and_unsupported_sources_write_nothing() {
    project(|root, app, recorder| {
        fs::create_dir(root.join("directory.md")).unwrap();
        fs::write(root.join("binary.md"), [0xff, 0xfe]).unwrap();
        fs::write(root.join("script.pdf"), TEXT).unwrap();
        fs::write(root.join("unparseable.txt"), "No scenes").unwrap();
        for source in ["missing.md", "directory.md", "binary.md", "script.pdf", "unparseable.txt"] {
            assert!(ingest(app, recorder, root, Path::new(source), "existing-project-file").is_err());
        }
        assert!(!root.join("Scripts").exists());
        assert_eq!(fs::read(root.join("binary.md")).unwrap(), [0xff, 0xfe]);
    });
}

#[test]
fn sc04_external_reads_require_exact_consumed_once_grants_and_correct_root() {
    project(|root, app, recorder| {
        let source = root.parent().unwrap().join("outside.md");
        fs::write(&source, TEXT).unwrap();
        assert!(ingest(app, recorder, root, &source, "external-import").is_err());
        dispatch(app, recorder, "grant_path", &json!({"path":source,"intent":"read"})).unwrap();
        assert!(ingest(app, recorder, root, &source, "existing-project-file").is_err());
        assert!(ingest(app, recorder, root.parent().unwrap(), &source, "external-import").is_err());
        assert!(ingest(app, recorder, root, &source, "invalid-kind").is_err());
        assert!(!root.join("Scripts").exists());
        assert_eq!(ingest(app, recorder, root, &source, "external-import").unwrap(), json!("Scripts/outside.md"));
        assert!(ingest(app, recorder, root, &source, "external-import").is_err());
        assert_eq!(fs::read(&source).unwrap(), TEXT.as_bytes());
    });
}

#[cfg(unix)]
#[test]
fn sc04_symlink_sources_cannot_register_outside_and_destinations_cannot_redirect() {
    use std::os::unix::fs::symlink;
    project(|root, app, recorder| {
        let source = root.parent().unwrap().join("outside.md");
        fs::write(&source, TEXT).unwrap();
        symlink(&source, root.join("alias.md")).unwrap();
        assert!(ingest(app, recorder, root, Path::new("alias.md"), "existing-project-file").is_err());
        fs::write(root.join("friends.md"), TEXT).unwrap();
        fs::create_dir(root.join("other")).unwrap();
        symlink(root.join("other"), root.join("Scripts")).unwrap();
        assert!(ingest(app, recorder, root, Path::new("friends.md"), "existing-project-file").is_err());
        fs::remove_file(root.join("Scripts")).unwrap();
        fs::create_dir(root.join("Scripts")).unwrap();
        symlink(&source, root.join("Scripts/friends.md")).unwrap();
        assert!(ingest(app, recorder, root, Path::new("friends.md"), "existing-project-file").is_err());
        assert_eq!(fs::read(source).unwrap(), TEXT.as_bytes());
        assert_eq!(fs::read_dir(root.join("other")).unwrap().count(), 0);
    });
}

fn save(app: &HostCtx, recorder: &RecorderState, root: &Path, name: &str, text: &str) -> Result<serde_json::Value, String> {
    dispatch(app, recorder, "save_project_script", &json!({"projectRoot":root,"filename":name,"text":text}))
}

#[test]
fn sc03_host_saves_exact_bytes_and_parses_both_scenes() {
    project(|root, app, recorder| {
        for name in ["friends.md", "friends.markdown", "friends.txt", "friends.MD"] {
            assert_eq!(save(app, recorder, root, name, TEXT).unwrap(), json!(format!("Scripts/{name}")));
            let saved = fs::read_to_string(root.join("Scripts").join(name)).unwrap();
            assert_eq!(saved.as_bytes(), TEXT.as_bytes());
            let scenes = script::extract_scenes(&saved);
            assert_eq!(scenes.iter().map(|scene| scene.id).collect::<Vec<_>>(), vec![1, 2]);
            assert!(scenes[0].source_text.contains("Hello!"));
            assert!(scenes[1].source_text.contains("Goodbye!"));
        }
        assert!(!root.join("Preproduction").exists(), "save must not import");
        assert!(save(app, recorder, root.parent().unwrap(), "wrong.md", TEXT).is_err());
        assert!(!root.parent().unwrap().join("Scripts").exists());
    });
}

#[test]
fn sc03_invalid_text_and_extensions_write_nothing() {
    project(|root, app, recorder| {
        fs::write(root.join("sentinel"), "preserve").unwrap();
        for text in ["", " \r\n\t", "No headings", "## Scene one\nHello", "## Scene 4294967296\nHello"] {
            assert!(save(app, recorder, root, "bad.md", text).unwrap_err().contains(if text.trim().is_empty() {"empty"} else {"Scene N"}));
        }
        for name in ["draft", "draft.pdf", "draft.md.exe"] {
            assert!(save(app, recorder, root, name, TEXT).unwrap_err().contains("filename"));
        }
        assert!(!root.join("Scripts").exists());
        assert_eq!(fs::read_to_string(root.join("sentinel")).unwrap(), "preserve");
    });
}

#[test]
fn sc03_traversal_and_nested_paths_are_rejected() {
    project(|root, app, recorder| {
        for name in ["../escape.md", "../../escape.md", "Scripts/draft.md", "sub/../draft.md", "..\\escape.md", "/tmp/escape.md", "C:\\escape.md", "draft\n.md", "draft\0.md", " draft.md"] {
            assert!(save(app, recorder, root, name, TEXT).unwrap_err().contains("filename"));
        }
        assert!(!root.join("Scripts").exists());
        assert!(!root.parent().unwrap().join("escape.md").exists());
    });
}

#[test]
fn sc03_same_name_collision_preserves_original_bytes() {
    project(|root, app, recorder| {
        save(app, recorder, root, "friends.md", TEXT).unwrap();
        for text in [TEXT, "Scene 1\nReplacement"] {
            assert!(save(app, recorder, root, "friends.md", text).unwrap_err().contains("never replaced"));
            assert_eq!(fs::read(root.join("Scripts/friends.md")).unwrap(), TEXT.as_bytes());
        }
        assert_eq!(fs::read_dir(root.join("Scripts")).unwrap().count(), 1);
    });
}

#[cfg(unix)]
#[test]
fn sc03_symlink_directory_escape_and_internal_redirect_are_rejected() {
    use std::os::unix::fs::symlink;
    project(|root, app, recorder| {
        let outside = root.parent().unwrap().join("outside");
        let internal = root.join("other");
        fs::create_dir(&outside).unwrap();
        fs::create_dir(&internal).unwrap();
        for target in [&outside, &internal, &root.join("missing")] {
            symlink(target, root.join("Scripts")).unwrap();
            assert!(save(app, recorder, root, "friends.md", TEXT).is_err());
            fs::remove_file(root.join("Scripts")).unwrap();
        }
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
        assert_eq!(fs::read_dir(internal).unwrap().count(), 0);
    });
}

#[cfg(unix)]
#[test]
fn sc03_symlink_destination_and_dangling_link_are_rejected() {
    use std::os::unix::fs::symlink;
    project(|root, app, recorder| {
        fs::create_dir(root.join("Scripts")).unwrap();
        let outside = root.parent().unwrap().join("original.md");
        fs::write(&outside, "original").unwrap();
        for target in [&outside, &root.join("missing.md")] {
            symlink(target, root.join("Scripts/friends.md")).unwrap();
            assert!(save(app, recorder, root, "friends.md", TEXT).is_err());
            fs::remove_file(root.join("Scripts/friends.md")).unwrap();
        }
        assert_eq!(fs::read_to_string(outside).unwrap(), "original");
        assert!(!root.join("missing.md").exists());
    });
}

#[test]
fn sc03_write_failure_preserves_existing_files() {
    project(|root, app, recorder| {
        // A file at the directory path fails independently of uid/permission bypasses.
        fs::write(root.join("Scripts"), "existing file").unwrap();
        assert!(save(app, recorder, root, "friends.md", TEXT).unwrap_err().contains("Could not create Scripts"));
        assert_eq!(fs::read_to_string(root.join("Scripts")).unwrap(), "existing file");
        assert_eq!(fs::read_dir(root).unwrap().count(), 1);
    });
}

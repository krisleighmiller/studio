use std::{fs, path::Path};
use serde_json::json;
use workbench_host::workflow::{cmd_import, import::{import_project, persist_state, ImportOptions}, state::ProjectState};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};
static SESSION: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn open(root: &Path) { dispatch(&HostCtx::new(root.join("settings")), &RecorderState::new(), "open_project", &json!({"path":root})).unwrap(); }
fn scan(root: &Path) -> ProjectState { import_project(root, ImportOptions { persist: true }).unwrap().state }
fn edit_import(root: &Path) -> Result<serde_json::Value, String> { cmd_import(&json!({"projectRoot":root,"persist":true,"editedScriptPath":"Scripts/B.md"})) }
#[test]
fn edits_preserve_source_identity_other_scene_and_production_on_repeat_and_reopen() {
    let _session = SESSION.lock().unwrap();
    let dir = tempfile::tempdir().unwrap(); let root = dir.path(); open(root); fs::create_dir(root.join("Scripts")).unwrap();
    fs::write(root.join("Original.md"), "Scene 3 — Original\nMARA: Untouched original.").unwrap();
    fs::write(root.join("Scripts/A.md"), "Scene 1 — A\n- **Dialogue:**\n  - **MARA:** Keep A.").unwrap();
    fs::write(root.join("Scripts/B.md"), "Scene 3 — B\n- **Dialogue:**\n  - **MARA:** Before.").unwrap();
    let mut state = scan(root);
    let b = state.scenes.iter().find(|s| s.source_path.as_deref()==Some("Scripts/B.md") && s.source_scene_number==Some(3)).unwrap().id;
    state.scene_mut(b, "").intent_approved = true;
    workbench_host::workflow::identity::add_alternate_shot(&mut state, b).unwrap();
    persist_state(root,&state).unwrap();
    let a = serde_json::to_value(state.scenes.iter().find(|s| s.source_path.as_deref()==Some("Scripts/A.md")).unwrap()).unwrap();
    let before = state.scene(b).unwrap().clone();
    for line in ["Après — café.", "再见 — tomorrow."] {
        fs::write(root.join("Scripts/B.md"), format!("Scene 3 — B\n- **Dialogue:**\n  - **MARA:** {line}")).unwrap();
        let result = edit_import(root).unwrap(); let updated: ProjectState = serde_json::from_value(result["state"].clone()).unwrap();
        let receipt = &result["editedScript"];
        assert_eq!(receipt["path"], "Scripts/B.md");
        assert_eq!(receipt["stateRevision"], updated.state_revision);
        assert_eq!(receipt["affectedSceneIds"], json!([b]));
        let bytes = fs::read(root.join("Scripts/B.md")).unwrap();
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        assert_eq!(receipt["sourceSha256"], digest);
        assert_eq!(fs::read_to_string(root.join("Scripts/B.md")).unwrap(), format!("Scene 3 — B\n- **Dialogue:**\n  - **MARA:** {line}"));
        let changed = updated.scene(b).unwrap();
        assert!(changed.source_excerpt.as_ref().unwrap().contains(line));
        assert_eq!(changed.dialogue[0].text,line); assert_eq!(changed.source_path,before.source_path); assert_eq!(changed.source_scene_number,Some(3)); assert_eq!(changed.stable_id,before.stable_id);
        assert!(changed.intent_approved);
        assert_eq!(changed.cuts.len(), before.cuts.len());
        assert_eq!(changed.cuts[0].stable_id, before.cuts[0].stable_id);
        assert_eq!(changed.cuts[0].dialogue, before.cuts[0].dialogue);
        assert!(changed.cuts[0].script_dialogue_review.is_some());
        assert_eq!(fs::read_to_string(root.join("Original.md")).unwrap(), "Scene 3 — Original\nMARA: Untouched original.");
        assert_eq!(serde_json::to_value(updated.scenes.iter().find(|s|s.source_path.as_deref()==Some("Scripts/A.md")).unwrap()).unwrap(),a);
        assert_eq!(scan(root).scene(b).unwrap().dialogue[0].text,line);
    }
}
#[test]
fn nested_saved_script_is_not_claimed_as_imported() {
    let _session = SESSION.lock().unwrap();
    let dir = tempfile::tempdir().unwrap(); let root = dir.path(); open(root);
    fs::create_dir_all(root.join("Scripts/Nested")).unwrap();
    fs::write(root.join("Scripts/B.md"), "Scene 1 — B\n- **Action:** Keep B.").unwrap();
    scan(root); let before = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    let text = "Scene 1 — Nested\n- **Action:** Do not claim imported.";
    fs::write(root.join("Scripts/Nested/N.md"), text).unwrap();
    let result = cmd_import(&json!({"projectRoot":root,"persist":true,"editedScriptPath":"Scripts/Nested/N.md"}));
    assert!(result.is_err(), "undiscoverable source must not report success");
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before);
    assert_eq!(fs::read_to_string(root.join("Scripts/Nested/N.md")).unwrap(),text);
}
#[test]
fn trashed_scene_folder_hides_scene_but_keeps_saved_work() {
    use workbench_host::scene_plan::fixtures::{broll_scene_yaml, cut_markdown};
    let _session = SESSION.lock().unwrap();
    let dir = tempfile::tempdir().unwrap(); let root = dir.path(); open(root);
    let folder = root.join("Scene_Prompts/Scene1"); fs::create_dir_all(&folder).unwrap();
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/story.md"), "Scene 1 — Kept in script").unwrap();
    fs::write(folder.join("Cut1.md"), cut_markdown(1, 1, &broll_scene_yaml(1))).unwrap();
    let mut state = scan(root);
    assert_eq!(state.scene(1).unwrap().cuts.len(), 1);
    state.scene_mut(1, "Scene 1").intent_approved = true;
    persist_state(root, &state).unwrap();
    dispatch(&HostCtx::new(root.join("settings")), &RecorderState::new(), "delete_path", &json!({"path":folder})).unwrap();
    let after = scan(root);
    assert!(after.scene(1).unwrap().source_missing);
    assert!(after.scene(1).unwrap().intent_approved);
    assert_eq!(after.scene(1).unwrap().cuts, state.scene(1).unwrap().cuts);
    let trashed = fs::read_dir(root.join(".workbench-trash")).unwrap().next().unwrap().unwrap().path();
    fs::rename(trashed, &folder).unwrap();
    let restored = scan(root);
    assert!(!restored.scene(1).unwrap().source_missing);
    assert!(restored.scene(1).unwrap().intent_approved);
}

#[test]
fn trashed_script_hides_its_scenes_without_erasing_saved_production() {
    let _session = SESSION.lock().unwrap();
    let dir = tempfile::tempdir().unwrap(); let root = dir.path(); open(root); fs::create_dir(root.join("Scripts")).unwrap();
    let source = root.join("Scripts/B.md");
    fs::write(&source, "Scene 1 — B\n- **Action:** Save this work.").unwrap();
    let mut state = scan(root);
    state.scene_mut(1, "B").intent_approved = true;
    persist_state(root, &state).unwrap();
    dispatch(&HostCtx::new(root.join("settings")), &RecorderState::new(), "delete_path", &json!({"path":source})).unwrap();
    let after = scan(root);
    assert!(after.scripts.is_empty());
    assert!(after.scene(1).unwrap().source_missing);
    assert!(after.scene(1).unwrap().intent_approved);
    let report = workbench_host::workflow::project_report(&after);
    assert!(report.scenes.is_empty());
    assert!(report.summary.contains("0 scene(s)"));
    let trashed = fs::read_dir(root.join(".workbench-trash")).unwrap().next().unwrap().unwrap().path();
    fs::rename(trashed, &source).unwrap();
    let restored = scan(root);
    assert!(!restored.scene(1).unwrap().source_missing);
    assert!(restored.scene(1).unwrap().intent_approved);
}

#[test]
fn removed_script_headings_hide_scenes_but_preserve_their_work_for_restore() {
    let _session = SESSION.lock().unwrap();
    let dir = tempfile::tempdir().unwrap(); let root = dir.path(); open(root); fs::create_dir(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/B.md"), "Scene 1 — B\n- **Dialogue:**\n  - **MARA:** Before.\n\nScene 2 — Other\n- **Dialogue:**\n  - **MARA:** Keep metadata.").unwrap();
    let mut state = scan(root);
    let id = state.scenes.iter().find(|scene| scene.source_scene_number == Some(2)).unwrap().id;
    state.scene_mut(id, "Other").intent_approved = true;
    persist_state(root, &state).unwrap();
    fs::write(root.join("Scripts/B.md"), "Scene 1 — B\n- **Dialogue:**\n  - **MARA:** Changed.").unwrap();
    let updated: ProjectState = serde_json::from_value(edit_import(root).unwrap()["state"].clone()).unwrap();
    assert!(!updated.scene(1).unwrap().source_missing);
    assert!(updated.scene(id).unwrap().source_missing);
    assert!(updated.scene(id).unwrap().intent_approved);
    fs::write(root.join("Scripts/B.md"), "Scene 1 — B\n\nScene 2 — Other").unwrap();
    let restored = scan(root);
    assert!(!restored.scene(id).unwrap().source_missing);
    assert!(restored.scene(id).unwrap().intent_approved);
    fs::write(root.join("Scripts/B.md"), "No Scene headings at all.").unwrap();
    let empty: ProjectState = serde_json::from_value(edit_import(root).unwrap()["state"].clone()).unwrap();
    assert!(empty.scenes.iter().all(|scene| scene.source_missing));
    assert!(empty.scene(id).unwrap().intent_approved);
}

use std::{fs, path::Path};
use workbench_host::workflow::{import::{import_project, persist_state, ImportOptions}, state::ProjectState};
fn scan(root: &Path) -> ProjectState { import_project(root, ImportOptions { persist: true }).unwrap().state }
#[test]
fn reordered_heading_edit_keeps_ids_but_renumber_swap_refuses() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    fs::create_dir(root.join("Scripts")).unwrap();
    let source = root.join("Scripts/B.md");
    fs::write(&source, "Scene 1 — Kitchen\n- **Dialogue:**\n  - **MARA:** Kitchen line.\n\nScene 2 — Road\n- **Dialogue:**\n  - **MARA:** Road line.").unwrap();
    let mut before = scan(root);
    before.scene_mut(1, "").intent_approved = true;
    workbench_host::workflow::identity::add_alternate_shot(&mut before, 1).unwrap();
    let old_cut = before.scene(1).unwrap().cuts[0].stable_id.clone();
    persist_state(root, &before).unwrap();
    fs::write(&source, "Scene 2 — Road at dusk\n- **Dialogue:**\n  - **MARA:** Road line.\n\nScene 1 — Kitchen\n- **Dialogue:**\n  - **MARA:** Kitchen line.").unwrap();
    let reordered = scan(root);
    assert_eq!(reordered.scene(1).unwrap().stable_id, before.scene(1).unwrap().stable_id);
    assert_eq!(reordered.scene(1).unwrap().cuts[0].stable_id, old_cut);
    assert_eq!(reordered.scene(2).unwrap().stable_id, before.scene(2).unwrap().stable_id);
    assert!(!reordered.scene(1).unwrap().source_missing);
    let saved_bytes = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    fs::write(&source, "Scene 1 — Road at dusk\n- **Dialogue:**\n  - **MARA:** Road line.\n\nScene 2 — Kitchen\n- **Dialogue:**\n  - **MARA:** Kitchen line.").unwrap();
    let error = import_project(root, ImportOptions { persist: true }).unwrap_err();
    assert!(error.contains("scene identity") && error.contains("Scripts/B.md"), "{error}");
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(), saved_bytes);
}
#[test]
fn removed_paid_scene_number_cannot_be_reused_for_another_heading() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    fs::create_dir(root.join("Scripts")).unwrap();
    let source = root.join("Scripts/B.md");
    fs::write(&source, "Scene 1 — Kitchen\n- **Action:** Kitchen action.\n\nScene 2 — Road\n- **Action:** Road action.").unwrap();
    let mut before = scan(root);
    before.scene_mut(2, "").intent_approved = true;
    workbench_host::workflow::identity::add_alternate_shot(&mut before, 2).unwrap();
    fs::create_dir(root.join("Final")).unwrap();
    fs::write(root.join("Final/scene2-frame.png"), b"retained media").unwrap();
    before.scene_mut(2, "").cuts[0].approved_start_frame = Some("Final/scene2-frame.png".into());
    let saved_cut = before.scene(2).unwrap().cuts[0].clone();
    persist_state(root, &before).unwrap();
    fs::write(&source, "Scene 1 — Kitchen\n- **Action:** Kitchen action.").unwrap();
    let missing = scan(root);
    assert!(missing.scene(2).unwrap().source_missing);
    assert!(missing.scene(2).unwrap().intent_approved);
    assert_eq!(missing.scene(2).unwrap().cuts[0], saved_cut);
    assert_eq!(fs::read(root.join("Final/scene2-frame.png")).unwrap(), b"retained media");
    let saved = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    fs::write(&source, "Scene 1 — Kitchen\n- **Action:** Kitchen action.\n\nScene 2 — Forest\n- **Action:** New action.").unwrap();
    let error = import_project(root, ImportOptions { persist: true }).unwrap_err();
    assert!(error.contains("scene identity") && error.contains("Scripts/B.md"), "{error}");
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(), saved);
    fs::write(&source, "Scene 1 — Kitchen\n- **Action:** Kitchen action.\n\nScene 2 — Road\n- **Action:** Road action.").unwrap();
    let restored = scan(root);
    assert!(!restored.scene(2).unwrap().source_missing);
    assert_eq!(restored.scene(2).unwrap().stable_id, before.scene(2).unwrap().stable_id);
    assert_eq!(restored.scene(2).unwrap().cuts[0], saved_cut);
    assert_eq!(fs::read(root.join("Final/scene2-frame.png")).unwrap(), b"retained media");
}

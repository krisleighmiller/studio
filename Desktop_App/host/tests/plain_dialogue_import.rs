use std::fs;
use serde_json::json;
use workbench_host::{script::DialogueLine, workflow::{import::{import_project, persist_state, ImportOptions}, state::{CutSketch, SketchDialogue}}};

#[test]
fn host_import_recovers_scene_dialogue_without_assigning_cuts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/test.md"), "Scene 1 — Ward\nLocation: Hospital\nAction: Doors open.\nDialogue:\nMara: First line.\n\nAction: The alarm rings.\nDialogue:\nBob: Second line.\n\nScene 2 — Hall\nDialogue:\nMara: Another scene.\n").unwrap();
    let mut old = import_project(root, ImportOptions { persist: true }).unwrap().state;
    // Simulate state persisted by the old parser, which found no plain dialogue.
    old.scene_mut(1, "").dialogue.clear();
    old.scene_mut(2, "").dialogue.clear();
    workbench_host::workflow::identity::add_alternate_shot(&mut old, 1).unwrap();
    let cut = &mut old.scene_mut(1, "").cuts[0];
    cut.dialogue = vec![DialogueLine { speaker: "Mara".into(), text: "Edited production cue".into() }];
    cut.sketch = Some(CutSketch { dialogue: vec![SketchDialogue { speaker: "Mara".into(), text: "Edited sketch cue".into(), heard_as: None }], ..Default::default() });
    // A discovered cut must merge with the saved cut, not replace its edits.
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    fs::write(root.join("Scenes/Scene1/Cut1/test.wav"), b"fixture").unwrap();
    persist_state(root, &old).unwrap();
    let app = workbench_host::ctx::HostCtx::new(root.join("settings"));
    let recorder = workbench_host::voice::RecorderState::new();
    workbench_host::dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
    let result = workbench_host::dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot":root,"persist":true})).unwrap();
    assert_eq!(result["state"]["scenes"][0]["dialogue"].as_array().unwrap().len(), 2);
    let imported = import_project(root, ImportOptions { persist: true }).unwrap().state;
    let scene = imported.scene(1).unwrap();
    assert_eq!(scene.dialogue.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(), vec!["First line.", "Second line."]);
    assert_eq!(scene.cuts[0].dialogue[0].text, "Edited production cue");
    assert_eq!(scene.cuts[0].sketch.as_ref().unwrap().dialogue[0].text, "Edited sketch cue");
    // Empty -> non-empty is first-time authoring, not a revision of approved
    // dialogue, so it no longer opens a review. Recovered dialogue and the
    // authored cut/sketch work are all preserved.
    assert!(scene.cuts[0].script_dialogue_review.is_none());
    let reopened = import_project(root, ImportOptions { persist: true }).unwrap().state;
    assert_eq!(reopened.scene(1).unwrap().dialogue, scene.dialogue);
    assert!(reopened.scene(1).unwrap().cuts[0].script_dialogue_review.is_none());
}

use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};
use workbench_host::workflow::{import::{import_project, persist_state, ImportOptions}, state::{CutSketch, SketchDialogue}};

#[test]
fn revised_scene_three_requires_review_and_refreshes_only_its_note() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir_all(root.join("Scripts")).unwrap();
    let source = root.join("Scripts/script.md");
    fs::write(&source, "## Scene 2 — Elsewhere\n- **Dialogue:**\n  - **BOB:** Leave it.\n## Scene 3 — Kitchen\n- **Dialogue:**\n  - **MARA:** This line is long enough to speak slowly today.\n").unwrap();
    let app = HostCtx::new(temp.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path": root})).unwrap();
    dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot":root,"persist":true})).unwrap();
    for scene in [2, 3] {
        dispatch(&app, &recorder, "workflow_add_alternate_shot", &json!({"projectRoot":root,"sceneId":scene})).unwrap();
        let mut state = import_project(&root, ImportOptions { persist: false }).unwrap().state;
        let cut = &mut state.scene_mut(scene, "").cuts[0];
        cut.dialogue = state_dialogue(scene);
        cut.sketch = Some(CutSketch { dialogue: vec![SketchDialogue { speaker: if scene == 3 {"MARA"} else {"BOB"}.into(), text: cut.dialogue[0].text.clone(), heard_as: None }], ..Default::default() });
        persist_state(&root, &state).unwrap();
        dispatch(&app, &recorder, "workflow_save_production_prompt", &json!({"projectRoot":root,"sceneId":scene,"cut":1,"prompt":format!("Authored prompt {scene}"),"negativePrompt":"Keep faces consistent"})).unwrap();
    }
    let note2 = root.join("Scene_Prompts/Scene2/Cut1.txt");
    let note3 = root.join("Scene_Prompts/Scene3/Cut1.txt");
    let before2 = fs::read(&note2).unwrap();
    let before3 = fs::read(&note3).unwrap();
    fs::write(&source, "## Scene 2 — Elsewhere\n- **Dialogue:**\n  - **BOB:** Leave it.\n## Scene 3 — Kitchen\n- **Dialogue:**\n  - **MARA:** Short.\n").unwrap();
    let changed = import_project(&root, ImportOptions { persist: true }).unwrap().state;
    assert!(changed.scene(3).unwrap().cuts[0].script_dialogue_review.is_some());
    let args = json!({"projectRoot":root,"sceneId":3,"cut":1});
    assert!(dispatch(&app, &recorder, "workflow_preview_production_handoff", &args).is_err());
    assert_eq!(fs::read(&note3).unwrap(), before3);
    let mut state = changed;
    let cut = &mut state.scene_mut(3, "").cuts[0];
    cut.dialogue[0].text = "Short.".into();
    cut.script_dialogue_review = None;
    state.scene_mut(3, "").script_dialogue_review = None;
    persist_state(&root, &state).unwrap();
    let preview = dispatch(&app, &recorder, "workflow_preview_production_handoff", &args).unwrap();
    assert_eq!(preview["oldCues"][0]["text"], "This line is long enough to speak slowly today.");
    assert_eq!(preview["newCues"][0]["text"], "Short.");
    assert_eq!(preview["prompt"], "Authored prompt 3");
    assert_ne!(preview["oldTiming"], preview["newTiming"]);
    assert_eq!(fs::read(&note3).unwrap(), before3);
    // A separate supported save succeeds after the real preview. This is a
    // sequential stale-revision replay, not a fabricated revision literal.
    let saved = dispatch(&app, &recorder, "workflow_save_production_prompt", &json!({"projectRoot":root,"sceneId":3,"cut":1,"prompt":"Revised authored prompt 3","negativePrompt":"Keep faces consistent"})).unwrap();
    assert!(saved["note"].as_str().unwrap().contains("Saved production prompt"));
    let after_save3 = fs::read(&note3).unwrap();
    assert_ne!(after_save3, before3);
    assert_eq!(fs::read(&note2).unwrap(), before2);
    let latest = dispatch(&app, &recorder, "workflow_preview_production_handoff", &args).unwrap();
    assert_ne!(preview["revision"], latest["revision"]);
    let stale = dispatch(&app, &recorder, "workflow_refresh_production_handoff", &json!({"projectRoot":root,"sceneId":3,"cut":1,"revision":preview["revision"]})).unwrap_err();
    assert_eq!(stale, "Production note or planning changed since preview. Review the latest values before refreshing.");
    assert_eq!(fs::read(&note3).unwrap(), after_save3);
    assert_eq!(fs::read(&note2).unwrap(), before2);
    println!("P3 successful intervening save, real stale revision refused exactly; note and sibling unchanged after refusal: {stale}");
    dispatch(&app, &recorder, "workflow_refresh_production_handoff", &json!({"projectRoot":root,"sceneId":3,"cut":1,"revision":latest["revision"]})).unwrap();
    let after3 = fs::read_to_string(&note3).unwrap();
    assert!(after3.contains("text: Short."));
    assert!(after3.contains("Revised authored prompt 3"));
    assert!(after3.contains("Keep faces consistent"));
    assert_eq!(fs::read(&note2).unwrap(), before2);
    assert_ne!(after3.as_bytes(), before3);
}

fn state_dialogue(scene: u32) -> Vec<workbench_host::script::DialogueLine> {
    vec![workbench_host::script::DialogueLine { speaker: if scene == 3 {"MARA"} else {"BOB"}.into(), text: if scene == 3 {"This line is long enough to speak slowly today."} else {"Leave it."}.into() }]
}

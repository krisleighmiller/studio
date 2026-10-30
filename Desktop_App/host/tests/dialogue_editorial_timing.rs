//! Offline regression: the same save button refreshes old inflated handoffs.
use std::{fs, path::Path, process::Command};
use serde_json::{json, Value};
use workbench_host::{assemble, ctx::HostCtx, director_actions::{self, ActionKind, DirectorAction}, scene_plan::{extract::{load_handoff_document, parse_handoff_yaml}, fixtures::{lipsync_scene_yaml, scene_markdown}, handoff::SpeechMode, render::{canonical_yaml, sidecar_document}}, workflow::{self, import, state::{CutSketch, JobRecord, ProjectState}}};

struct SessionGuard;
impl Drop for SessionGuard { fn drop(&mut self) { workbench_host::session::close(); } }

fn saved(root: &Path) -> workbench_host::scene_plan::handoff::SceneHandoff {
    load_handoff_document(&fs::read_to_string(root.join("Scene_Prompts/Scene1/Cut1.txt")).unwrap()).unwrap()
}

#[test]
fn save_button_refreshes_inflated_timing_and_reopen_keeps_jobs_takes() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    workbench_host::session::open(root.to_str().unwrap()).unwrap(); let _guard=SessionGuard;
    let text = "Oooh, I better slow down, so we don't crash and die.";
    let mut h = parse_handoff_yaml(&lipsync_scene_yaml(1, text)).unwrap();
    h.editing.estimated_screen_duration_seconds = 9.0;
    let yaml = canonical_yaml(&h).unwrap();
    fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();
    fs::write(root.join("Scene_Prompts/Scene1/Cut1.txt"), sidecar_document(&h,&yaml)).unwrap();
    fs::write(root.join("prod.md"), scene_markdown(1,&yaml)).unwrap();
    let mut state = ProjectState::new();
    state.jobs.push(JobRecord::open("job_1","render"));
    let cut = state.scene_mut(1,"Scene 1").cut_mut(1);
    cut.speech_mode = Some(SpeechMode::SourceVideoLipsync);
    cut.dialogue = vec![workbench_host::script::DialogueLine {speaker:"Mom".into(),text:text.into()}];
    cut.sketch = Some(CutSketch::default());
    cut.production_note_path = Some("Scene_Prompts/Scene1/Cut1.txt".into());
    import::persist_state(root,&state).unwrap();
    let before = serde_json::to_value(import::load_state(root).unwrap()).unwrap();
    workflow::cmd_save_production_prompt(&json!({"projectRoot":root,"sceneId":1,"cut":1,"prompt":h.prompt,"negativePrompt":"keep negative"})).unwrap();
    let updated = saved(root);
    assert_eq!(updated.editing.estimated_screen_duration_seconds,5.0);
    assert_eq!(updated.speech.cues[0].estimated_duration_seconds,Some(4.0));
    let derived = updated.derived.as_ref().unwrap();
    assert_eq!(derived.minimum_source_duration_seconds,6.0);
    assert_eq!(derived.render_duration_seconds,10.0);
    assert!(fs::read_to_string(root.join("prod.md")).unwrap().contains("estimated_screen_duration_seconds: 5.0"));
    let reopened = serde_json::to_value(import::load_state(root).unwrap()).unwrap();
    assert_eq!(reopened["jobs"],before["jobs"]);
    assert_eq!(reopened["scenes"][0]["cuts"][0]["takes"],before["scenes"][0]["cuts"][0]["takes"]);
    assert_eq!(saved(root),updated);
}

#[test]
#[ignore = "requires isolated copied Mom project; no original project is accepted"]
fn copied_mom_refresh_reopen_and_native_assembly() {
    let root_s=std::env::var("STUDIO_TIMING_DERIVATIVE").expect("isolated derivative path");
    assert!(root_s.starts_with("/runtime/dialogue-timing-repair/"));
    let root=Path::new(&root_s);
    workbench_host::session::open(&root_s).unwrap(); let _guard=SessionGuard;
    let wav=root.join("Scenes/Scene1/Cut1/S01C01.wav");
    let video=root.join("Scenes/Scene1/Cut1/S01C01.mp4");
    let wav_before=fs::read(&wav).unwrap();let video_before=fs::read(&video).unwrap();
    let before=serde_json::to_value(import::load_state(root).unwrap()).unwrap();
    let handoff=saved(root);
    workflow::cmd_save_production_prompt(&json!({"projectRoot":root,"sceneId":1,"cut":1,"prompt":handoff.prompt,"negativePrompt":handoff.negative_prompt})).unwrap();
    let after=serde_json::to_value(import::load_state(root).unwrap()).unwrap();
    assert_eq!(after["jobs"],before["jobs"]);
    assert_eq!(after["scenes"][0]["cuts"][0]["takes"],before["scenes"][0]["cuts"][0]["takes"]);
    assert_eq!(after["scenes"][0]["cuts"][0]["selected_take"],before["scenes"][0]["cuts"][0]["selected_take"]);
    assert_eq!(saved(root).editing.estimated_screen_duration_seconds,5.0);
    // Legacy assembler indexes metadata by scene, not cut. Isolate the selected
    // Mom handoff using the existing canonical master renderer; do not read Sam.
    let h=saved(root);
    fs::write(root.join("mom-assembly.md"),workbench_host::scene_plan::render::master_block(&h,&canonical_yaml(&h).unwrap())).unwrap();
    let app=HostCtx::new(root.join("isolated-settings"));
    for (mode,start,duration) in [("estimated",0.5,5.0),("custom",0.5,6.5)] {
        let inputs=json!({"production_path":"mom-assembly.md","output_path":format!("Final/mom_{mode}.mp4"),"work_dir":format!("Final/work_{mode}"),"trim":mode,"fade_in":false,"fade_out":false,"overwrite":true,"caption_mode":"none","scenes":[{"id":1,"cut":1,"video":"Scenes/Scene1/Cut1/S01C01.mp4","start":start,"duration":duration}]}).as_object().unwrap().clone();
        let notes=assemble::approval_notes(&inputs,Some(&root_s));
        fs::write(root.join(format!("{mode}-plan.md")),&notes).unwrap();
        let action=DirectorAction {action:ActionKind::RunSpecialist,specialist:Some("assemble".into()),inputs,message:None,op:None,path:None};
        let validated=director_actions::validate_action(action,Some(&root_s)).unwrap();
        let result=assemble::run(&app,&validated,Some(&root_s)).unwrap();
        fs::write(root.join(format!("{mode}-result.md")),result).unwrap();
        let output=Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(root.join(format!("Final/mom_{mode}.mp4"))).output().unwrap();
        assert!(output.status.success());
        let probe:Value=serde_json::from_slice(&output.stdout).unwrap();
        let actual:f64=probe["format"]["duration"].as_str().unwrap().parse().unwrap();
        assert!((actual-duration).abs()<0.12,"{mode}: {actual} vs {duration}");
        assert!(probe["streams"].as_array().unwrap().iter().any(|s|s["codec_type"]=="audio"));
        fs::write(root.join(format!("{mode}-probe.json")),output.stdout).unwrap();
    }
    assert_eq!(fs::read(wav).unwrap(),wav_before);
    assert_eq!(fs::read(video).unwrap(),video_before);
}

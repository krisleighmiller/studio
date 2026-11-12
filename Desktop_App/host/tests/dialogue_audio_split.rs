//! Disposable local fixtures only: no credentials, uploads or provider requests.
use serde_json::json;
use std::{fs, process::Command};
use workbench_host::{
    ctx::HostCtx,
    dispatch, pending, session,
    voice::RecorderState,
    workflow::{
        dialogue_audio, import,
        state::{AssetStatus, JobStatus, ProjectState},
    },
};
#[test]
fn speech_prepares_without_visuals_and_settles_exact_cut_draft_without_video() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Characters/A")).unwrap();
    let voice = "Characters/A/voice.wav";
    fs::write(
        root.join(voice),
        b"voice fingerprint fixture; preparation does not upload",
    )
    .unwrap();
    let state=ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Speech fixture","cuts":[{"id":{"source_scene":1,"cut":2},"stable_id":"shot_two","speech_mode":"source_video_lipsync","dialogue":[{"speaker":"A","text":"The complete first line."},{"speaker":"A","text":"And the entire second line."}]},{"id":{"source_scene":1,"cut":3},"stable_id":"shot_three","speech_mode":"source_video_lipsync","dialogue":[{"speaker":"B","text":"A different cut."}]}]}],"assets":[{"id":"voice_a","path":voice,"role":"voice_sample","status":"draft"}]}).to_string()).unwrap();
    import::persist_state(&root, &state).unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let settings = dir.path().join("settings");
    fs::create_dir_all(&settings).unwrap();
    fs::write(settings.join("settings.json"),json!({"apis":[{"id":"local","name":"Local fixture","kind":"atlas-cloud","baseUrl":"http://127.0.0.1:9","enabled":true}],"capabilityDefaults":{"audio.tts_clone":{"apiId":"local","modelId":"fixture-speech-model"}}}).to_string()).unwrap();
    let app = HostCtx::new(settings);
    let rec = RecorderState::new();
    let args = json!({"projectRoot":root,"sceneId":1,"cut":2,"voiceAssetId":"voice_a"});
    let result = dispatch(&app, &rec, "workflow_prepare_dialogue_audio", &args).unwrap();
    let card = result["pendingAction"].as_str().unwrap();
    assert!(card.contains("fixture-speech-model"));
    assert!(card.contains("zero video"));
    let action = pending::current().unwrap();
    assert_eq!(action.specialist.as_deref(), Some("dialogue_audio"));
    assert_eq!(
        action.inputs["dialogue"][1]["text"],
        "And the entire second line."
    );
    assert!(!action.inputs.contains_key("handoff_path"));
    assert!(!action.inputs.contains_key("source_frame"));
    let target = dialogue_audio::validate(&root, &action).unwrap();
    assert!(!target.exists());
    let before = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    let mut changed = ProjectState::from_json(std::str::from_utf8(&before).unwrap()).unwrap();
    changed.scenes[0].cuts[0].dialogue[1].text = "Changed dialogue".into();
    import::persist_state(&root, &changed).unwrap();
    assert!(dialogue_audio::validate(&root, &action).is_err());
    fs::write(root.join("Preproduction/workflow_state.json"), &before).unwrap();
    let mut wrong = action.clone();
    wrong.inputs.insert("cut".into(), json!(3));
    assert!(dialogue_audio::validate(&root, &wrong).is_err());
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=731:duration=1",
            "-c:a",
            "pcm_s16le"
        ])
        .arg(&target)
        .status()
        .unwrap()
        .success());
    let bytes = fs::read(&target).unwrap();
    dialogue_audio::settle(&root, &action, &target).unwrap();
    let saved = ProjectState::from_json(
        &fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap(),
    )
    .unwrap();
    let asset = saved
        .assets
        .iter()
        .find(|a| a.path.ends_with("dialogue_1.wav"))
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Draft);
    assert_eq!(asset.cut_id.unwrap().cut, 2);
    assert!(saved.scenes[0]
        .cuts
        .iter()
        .all(|c| c.selected_performance_asset_id.is_none()));
    assert_eq!(
        saved
            .jobs
            .iter()
            .find(|j| j.kind == "dialogue_audio")
            .unwrap()
            .status,
        JobStatus::Completed
    );
    assert!(!saved
        .jobs
        .iter()
        .any(|j| j.kind == "render" || j.kind.starts_with("atlas_generate_")));
    assert!(dialogue_audio::settle(&root, &action, &target).is_err());
    assert_eq!(fs::read(&target).unwrap(), bytes);
    let again = dispatch(&app, &rec, "workflow_prepare_dialogue_audio", &args).unwrap();
    assert!(again["pendingAction"].is_string());
    assert_ne!(
        pending::current().unwrap().inputs["output_path"],
        action.inputs["output_path"]
    );
    let other = tempfile::tempdir().unwrap();
    session::open(other.path().to_str().unwrap()).unwrap();
    assert!(dispatch(&app, &rec, "workflow_prepare_dialogue_audio", &args).is_err());
    session::close();
    println!("audio split: two complete lines, explicit voice, no visuals/prompts/video model, exact cut Draft settlement, unchanged saved bytes, no automatic selection/video, fresh version, project isolation; zero provider calls");
}

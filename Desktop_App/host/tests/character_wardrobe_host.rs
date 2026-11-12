use serde_json::json;
use std::{fs, process::Command};
use workbench_host::{
    ctx::HostCtx,
    dispatch,
    voice::RecorderState,
    workflow::{import, state::ProjectState},
};
#[test]
fn attach_two_wardrobes_without_scene_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Characters/Hero")).unwrap();
    fs::write(root.join("Characters/Hero/character.yaml"), "name: Hero\n").unwrap();
    let source = dir.path().join("source.png");
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=32x48",
            "-frames:v",
            "1",
            "-threads",
            "1"
        ])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let state = ProjectState::from_json(
        &json!({"version":2,"characters":[{"id":"hero","name":"Hero","dir":"Characters/Hero"}]})
            .to_string(),
    )
    .unwrap();
    import::persist_state(&root, &state).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let rec = RecorderState::new();
    dispatch(&app, &rec, "open_project", &json!({"path":root})).unwrap();
    for _ in 0..2 {
        dispatch(
            &app,
            &rec,
            "grant_path",
            &json!({"path":source,"intent":"read"}),
        )
        .unwrap();
        dispatch(
            &app,
            &rec,
            "workflow_attach_character_wardrobe",
            &json!({"projectRoot":root,"characterId":"hero","sourcePath":source}),
        )
        .unwrap();
    }
    let reopened = dispatch(
        &app,
        &rec,
        "workflow_import",
        &json!({"projectRoot":root,"persist":true}),
    )
    .unwrap();
    let assets = reopened["state"]["assets"].as_array().unwrap();
    let wardrobe: Vec<_> = assets
        .iter()
        .filter(|a| a["role"] == "wardrobe_reference")
        .collect();
    assert_eq!(wardrobe.len(), 2);
    assert_ne!(wardrobe[0]["path"], wardrobe[1]["path"]);
    for a in wardrobe {
        assert_eq!(a["character_id"], "hero");
        assert!(a.get("scene_id").is_none() || a["scene_id"].is_null());
        assert!(reopened["state"]["characters"][0]["asset_ids"]
            .as_array()
            .unwrap()
            .contains(&a["id"]));
    }
    assert!(reopened["state"]["scenes"].as_array().unwrap().is_empty());
    fs::write(app.settings_path(),serde_json::to_vec(&json!({"apis":[{"id":"host-api","name":"Host Provider","kind":"image-provider","baseUrl":"https://provider.invalid","apiKey":"","secretKey":"","enabled":true}],"capabilityDefaults":{"image.text_to_image":{"apiId":"host-api","modelId":"host-image-model"}}})).unwrap()).unwrap();
    let assets = reopened["state"]["assets"].as_array().unwrap();
    let ids: Vec<_> = assets
        .iter()
        .filter(|a| a["role"] == "wardrobe_reference")
        .map(|a| a["id"].as_str().unwrap().to_string())
        .collect();
    let paths: Vec<_> = ids
        .iter()
        .map(|id| {
            assets.iter().find(|a| a["id"] == *id).unwrap()["path"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let generate = |refs: serde_json::Value| {
        dispatch(
            &app,
            &rec,
            "workflow_generate_character_wardrobe",
            &json!({"projectRoot":root,"characterId":"hero","prompt":"red coat","referenceAssetIds":refs}),
        )
    };
    assert!(generate(json!([ids[0], "missing"])).is_err());
    let generated = generate(json!([ids[1], ids[0]])).unwrap();
    assert!(generated["pendingAction"]
        .as_str()
        .unwrap()
        .contains("host-image-model"));
    let pending = workbench_host::pending::current().unwrap();
    assert_eq!(
        pending.inputs["reference_images"],
        json!([paths[1], paths[0]])
    );
    let saved = import::load_state(&root).unwrap();
    let job = saved
        .jobs
        .iter()
        .find(|j| j.kind == "character_wardrobe")
        .unwrap();
    assert_eq!(job.input_asset_ids, vec![ids[1].clone(), ids[0].clone()]);
    let output = job.output_path.clone().unwrap();
    fs::copy(&source, root.join(&output)).unwrap();
    let loaded = import::load_state(&root).unwrap();
    let settled = loaded
        .jobs
        .iter()
        .find(|j| j.kind == "character_wardrobe")
        .unwrap();
    assert_eq!(
        settled.status,
        workbench_host::workflow::state::JobStatus::Completed
    );
    let asset = loaded.asset(&settled.output_asset_ids[0]).unwrap();
    assert_eq!(asset.character_id.as_deref(), Some("hero"));
    assert!(asset.scene_id.is_none());
    assert_eq!(asset.path, output);
    assert_eq!(
        import::load_state(&root)
            .unwrap()
            .assets
            .iter()
            .filter(|a| a.path == output)
            .count(),
        1
    );
    let generated2 = generate(json!([])).unwrap();
    assert!(generated2["pendingAction"].is_string());
    let pending = workbench_host::pending::current().unwrap();
    assert!(!pending.inputs.contains_key("reference_images"));
    let state = import::load_state(&root).unwrap();
    let jobs: Vec<_> = state
        .jobs
        .iter()
        .filter(|j| j.kind == "character_wardrobe")
        .collect();
    assert_eq!(jobs.len(), 2);
    assert_ne!(jobs[0].output_path, jobs[1].output_path);
    let second_output = jobs[1].output_path.clone().unwrap();
    assert!(workbench_host::workflow::character_wardrobe::on_failed(
        root.to_str(), &second_output, "provider-failed: test"
    ).is_some());
    let after_failure = import::load_state(&root).unwrap();
    assert_eq!(after_failure.jobs.iter().find(|j| j.output_path.as_deref() == Some(&second_output)).unwrap().status,
        workbench_host::workflow::state::JobStatus::Failed);
}

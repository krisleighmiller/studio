use serde_json::json;
use std::fs;
use workbench_host::{
    ctx::HostCtx, dispatch, pending, voice::RecorderState,
    workflow::{generate_image, import, state::ProjectState},
};

/// The project session root is process-global; serialize the tests that
/// open and close projects so they cannot race each other.
static SESSION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn setup() -> (tempfile::TempDir, std::path::PathBuf, HostCtx, RecorderState) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(&root).unwrap();
    let state = ProjectState::from_json(
        &json!({
            "version": 2,
            "scenes": [],
            "assets": [{"id": "ref_a", "path": "Preproduction/Reference/ref_a.png", "role": "interior_reference", "status": "draft"}]
        })
        .to_string(),
    )
    .unwrap();
    fs::create_dir_all(root.join("Preproduction/Reference")).unwrap();
    fs::write(root.join("Preproduction/Reference/ref_a.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    import::persist_state(&root, &state).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    fs::create_dir_all(dir.path().join("settings")).unwrap();
    let settings = json!({
        "apis": [{"id": "offline", "name": "Offline", "kind": "atlas-cloud", "baseUrl": "http://127.0.0.1:9", "enabled": true}],
        "capabilityDefaults": {
            "image.text_to_image": {"apiId": "offline", "modelId": "text-model"},
            "image.image_to_image": {"apiId": "offline", "modelId": "chosen-model"}
        }
    })
    .to_string();
    fs::write(app.settings_path(), &settings).unwrap();
    let rec = RecorderState::new();
    dispatch(&app, &rec, "open_project", &json!({"path": root})).unwrap();
    (dir, root, app, rec)
}

#[test]
fn happy_path_prepares_count_jobs_and_carries_prompt_aspect_and_refs() {
    let _guard = SESSION_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, root, app, rec) = setup();
    let result = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({
            "projectRoot": root,
            "prompt": "a neon alley at night",
            "aspectRatio": "9:16",
            "count": 3,
            "imageCapability": "image.image_to_image",
            "referenceAssetIds": ["ref_a"],
            "title": "Alley study"
        }),
    )
    .unwrap();
    assert!(result["pendingAction"].as_str().unwrap().contains("Alley study"));
    assert!(result["pendingAction"].as_str().unwrap().contains("chosen-model"));
    let action = pending::current().unwrap();
    let batch = action.inputs["batch"].as_array().unwrap();
    assert_eq!(batch.len(), 3, "one prepared item per requested image");
    for item in batch {
        assert_eq!(item["prompt"], "a neon alley at night");
        assert_eq!(item["aspect_ratio"], "9:16");
        assert_eq!(item["image_capability"], "image.image_to_image");
        assert_eq!(item["reference_images"], json!(["Preproduction/Reference/ref_a.png"]));
        let output = item["output_path"].as_str().unwrap();
        assert!(output.starts_with("Preproduction/GeneratedImages/alley_study_"));
    }
    let state = import::load_state(&root).unwrap();
    assert_eq!(state.jobs.len(), 3);
    assert!(state.jobs.iter().all(|job| job.kind == "generated_image"));
    assert!(state.jobs.iter().all(|job| job.status == workbench_host::workflow::state::JobStatus::Pending));
    assert!(state.jobs.iter().all(|job| job.input_asset_ids == vec!["ref_a"]));
    // Settlement registers one generated_image asset per still and finishes jobs.
    let rels: Vec<String> = state
        .jobs
        .iter()
        .map(|job| job.output_path.clone().unwrap())
        .collect();
    for rel in &rels {
        let absolute = root.join(rel);
        fs::write(&absolute, b"\x89PNG\r\n\x1a\n").unwrap();
        let note = generate_image::on_still_saved(Some(root.to_str().unwrap()), &absolute.to_string_lossy()).unwrap();
        assert!(note.contains("Registered generated image"));
    }
    let settled = import::load_state(&root).unwrap();
    let generated: Vec<_> = settled
        .assets
        .iter()
        .filter(|asset| asset.role == workbench_host::workflow::state::AssetRole::GeneratedImage)
        .collect();
    assert_eq!(generated.len(), 3);
    assert!(generated.iter().all(|asset| asset.scene_id.is_none() && asset.cut_id.is_none()));
    assert!(settled.jobs.iter().all(|job| job.status == workbench_host::workflow::state::JobStatus::Completed));
    pending::cancel_all();
    dispatch(&app, &rec, "close_project", &json!({})).unwrap();
    drop(dir);
}

#[test]
fn invalid_or_missing_reference_ids_are_rejected_without_pending() {
    let _guard = SESSION_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, root, app, rec) = setup();
    let error = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": "room", "referenceAssetIds": ["nope"]}),
    )
    .unwrap_err();
    assert!(error.contains("nope"), "{error}");
    assert!(pending::current().is_none());
    let error = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": "room", "referenceAssetIds": [42]}),
    )
    .unwrap_err();
    assert!(error.contains("referenceAssetIds"), "{error}");
    assert!(pending::current().is_none());
    let state = import::load_state(&root).unwrap();
    assert!(state.jobs.is_empty());
    dispatch(&app, &rec, "close_project", &json!({})).unwrap();
    drop(dir);
}

#[test]
fn count_bounds_and_aspect_are_enforced() {
    let _guard = SESSION_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, root, app, rec) = setup();
    let error = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": "room", "count": 0}),
    )
    .unwrap_err();
    assert!(error.contains("count must be between 1 and 8"), "{error}");
    let error = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": "room", "count": 9}),
    )
    .unwrap_err();
    assert!(error.contains("count must be between 1 and 8"), "{error}");
    let error = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": "room", "aspectRatio": "21:9"}),
    )
    .unwrap_err();
    assert!(error.contains("aspectRatio"), "{error}");
    assert!(pending::current().is_none());
    // Defaults: aspect 9:16 and count 1 apply when omitted.
    let result = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": "room"}),
    )
    .unwrap();
    let action = pending::current().unwrap();
    let batch = action.inputs["batch"].as_array().unwrap();
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0]["aspect_ratio"], "9:16");
    assert!(batch[0]["output_path"].as_str().unwrap().starts_with("Preproduction/GeneratedImages/image_"));
    assert!(result["pendingAction"].as_str().unwrap().contains("1 still"));
    pending::cancel_all();
    dispatch(&app, &rec, "close_project", &json!({})).unwrap();
    drop(dir);
}

#[test]
fn empty_prompt_with_refs_is_variations_and_empty_alone_is_rejected() {
    let _guard = SESSION_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, root, app, rec) = setup();
    let result = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": "", "referenceAssetIds": ["ref_a"], "count": 2}),
    )
    .unwrap();
    assert!(result["pendingAction"].as_str().unwrap().contains("2 stills"));
    let action = pending::current().unwrap();
    let batch = action.inputs["batch"].as_array().unwrap();
    assert_eq!(batch.len(), 2);
    for item in batch {
        assert!(item["prompt"].as_str().unwrap().contains("variation of the attached reference images"));
    }
    let state = import::load_state(&root).unwrap();
    assert_eq!(state.jobs.len(), 2);
    pending::cancel_all();
    let error = dispatch(
        &app,
        &rec,
        "workflow_generate_image",
        &json!({"projectRoot": root, "prompt": ""}),
    )
    .unwrap_err();
    assert!(error.contains("Describe an image"), "{error}");
    dispatch(&app, &rec, "close_project", &json!({})).unwrap();
    drop(dir);
}

#[test]
fn studio_generation_needs_no_project_session() {
    let _guard = SESSION_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    fs::create_dir_all(dir.path().join("settings")).unwrap();
    let settings = json!({
        "apis": [{"id": "offline", "name": "Offline", "kind": "atlas-cloud", "baseUrl": "http://127.0.0.1:9", "enabled": true}],
        "capabilityDefaults": {
            "image.text_to_image": {"apiId": "offline", "modelId": "text-model"}
        }
    })
    .to_string();
    fs::write(app.settings_path(), &settings).unwrap();
    // No project session anywhere: studio generation must not demand one.
    let _ = dispatch(&app, &RecorderState::new(), "close_project", &json!({}));
    let result = dispatch(
        &app,
        &RecorderState::new(),
        "workflow_generate_image",
        &json!({
            "prompt": "woman walking on the beach at sunset",
            "aspectRatio": "9:16",
            "count": 1
        }),
    );
    match result {
        Ok(value) => {
            let pending_text = value["pendingAction"].as_str().unwrap_or_default();
            assert!(
                !pending_text.is_empty(),
                "expected a pending approval without any project, got {value}"
            );
            // The card must carry the one-use approve token or the renderer
            // fails silently (no dialog, no error).
            assert!(
                pending_text.contains("[[workbench_approve:"),
                "studio pending action must embed the approval token"
            );
            // Execution must accept the library root: approving must not
            // demand an open project session. The offline provider then fails
            // on its own (connection refused), which is fine for this gate.
            let id = pending_text
                .split("[[workbench_approve:")
                .nth(1)
                .and_then(|tail| tail.split("]]").next())
                .expect("token id present");
            let executed = dispatch(&app, &RecorderState::new(), "director_approve", &json!({ "id": id }));
            match executed {
                Ok(_reply) => {}
                Err(error) => {
                    // "open a project" is matched case-insensitively: the host
                    // has two phrasings ("Open a project first." and
                    // "open a project before using a file path") and either
                    // means execution wrongly demanded a project session.
                    assert!(
                        !error.to_lowercase().contains("open a project")
                            && !error.contains("project changed after this approval"),
                        "studio execution must not require a project session: {error}"
                    );
                }
            }
        }
        Err(error) => panic!("studio generate must not require a project: {error}"),
    }
    let root = generate_image::studio_root(&app);
    let state = import::load_state(&root).unwrap();
    assert_eq!(state.jobs.len(), 1);
    assert!(state.jobs.iter().all(|job| job.kind == "generated_image"));
}

#[test]
fn studio_reference_lanes_prepare_and_execute_without_a_project() {
    let _guard = SESSION_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    fs::create_dir_all(dir.path().join("settings")).unwrap();
    let settings = json!({
        "apis": [{"id": "offline", "name": "Offline", "kind": "atlas-cloud", "baseUrl": "http://127.0.0.1:9", "enabled": true}],
        "capabilityDefaults": {
            "image.text_to_image": {"apiId": "offline", "modelId": "text-model"},
            "image.image_to_image": {"apiId": "offline", "modelId": "chosen-model"},
            "image.character_reference": {"apiId": "offline", "modelId": "char-model"},
            "image.edit": {"apiId": "offline", "modelId": "edit-model"}
        }
    })
    .to_string();
    fs::write(app.settings_path(), &settings).unwrap();
    let _ = dispatch(&app, &RecorderState::new(), "close_project", &json!({}));
    // Seed the studio library with one reference image.
    let root = generate_image::studio_root(&app);
    generate_image::ensure_studio_state(&root).unwrap();
    fs::create_dir_all(root.join("Preproduction/Reference")).unwrap();
    fs::write(root.join("Preproduction/Reference/sheet_ref.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    let mut state = import::load_state(&root).unwrap();
    let asset_id = state.push_asset(workbench_host::workflow::state::AssetRecord {
        id: String::new(),
        path: "Preproduction/Reference/sheet_ref.png".into(),
        role: workbench_host::workflow::state::AssetRole::GeneratedImage,
        status: workbench_host::workflow::state::AssetStatus::Draft,
        scene_id: None,
        cut_id: None,
        character_id: None,
        source_job_id: None,
        parent_asset_id: None,
        notes: Some("library reference".into()),
    });
    import::save_project(&root, state).unwrap();
    // All four panel lanes are the same still_image specialist with a
    // capability; the reference-bearing lanes must prepare and execute
    // against the library, not a project session.
    for capability in ["image.text_to_image", "image.image_to_image", "image.character_reference", "image.edit"] {
        let result = dispatch(
            &app,
            &RecorderState::new(),
            "workflow_generate_image",
            &json!({
                "prompt": "a red lantern over the clinic door",
                "aspectRatio": "9:16",
                "count": 1,
                "imageCapability": capability,
                "referenceAssetIds": [asset_id]
            }),
        );
        let value = match result {
            Ok(value) => value,
            Err(error) => panic!("studio {capability} lane must not require a project: {error}"),
        };
        let pending_text = value["pendingAction"].as_str().unwrap_or_default();
        assert!(pending_text.contains("[[workbench_approve:"), "{capability} lane missing approve token");
        let id = pending_text
            .split("[[workbench_approve:")
            .nth(1)
            .and_then(|tail| tail.split("]]").next())
            .unwrap();
        let executed = dispatch(&app, &RecorderState::new(), "director_approve", &json!({ "id": id }));
        match executed {
            Ok(_reply) => {}
            Err(error) => assert!(
                !error.to_lowercase().contains("open a project") && !error.contains("project changed after this approval"),
                "studio {capability} execution must not require a project session: {error}"
            ),
        }
    }
}

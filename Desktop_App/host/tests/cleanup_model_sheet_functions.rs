use serde_json::{json, Value};
use std::{fs, path::Path};
use workbench_host::specialists::prepare_model_sheet_manifest;

#[path = "support/ms03.rs"]
mod ms03;

#[test]
fn ms03_typed_command_parks_approval_without_provider_or_chat() {
    let _lock = ms03::SESSION.lock().unwrap_or_else(|e| e.into_inner());
    fixture(|root| {
        use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import::persist_state, state::ProjectState}};
        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{"id":"alice", "name":"Alice", "dir":"Characters/Alice", "profile_path":"Characters/Alice/character.yaml", "identity_ready":false, "voice_ready":false}])).unwrap();
        persist_state(root, &state).unwrap();
        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        let mut req = request();
        req["characterId"] = json!("alice");
        req = json!({"characterId":"alice", "action":"run_specialist", "specialist":"model_sheet", "inputs":req});
        req["inputs"].as_object_mut().unwrap().remove("characterId");
        let result = dispatch(&app, &recorder, "start_character_model_sheet", &json!({"projectRoot":root,"request":req})).unwrap();
        assert_eq!(result["status"], "pendingApproval");
        assert!(result["pendingAction"].as_str().unwrap().contains("[[workbench_approve:"));
        assert!(!manifest_path(root).exists(), "preparation belongs after typed validation");
        dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
    });
}

#[test]
fn minimal_visual_inputs_reach_native_approval_and_prepare_without_full_identity() {
    let _lock = ms03::SESSION.lock().unwrap_or_else(|e| e.into_inner());
    for reference_only in [false, true] {
        fixture(|root| {
            use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import::persist_state, state::ProjectState}};
            let mut state = ProjectState::default(); state.profile = Some(Default::default());
            state.characters = serde_json::from_value(json!([{"id":"alice", "name":"Alice", "dir":"Characters/Alice", "profile_path":"Characters/Alice/character.yaml", "identity_ready":false, "voice_ready":false, "profile":{}, "reference_asset_ids":if reference_only { vec!["ref"] } else { vec![] }}])).unwrap();
            if reference_only { state.assets = serde_json::from_value(json!([{"id":"ref", "path":"Characters/Alice/ref.png", "role":"identity_reference", "status":"present", "character_id":"alice"}])).unwrap(); }
            persist_state(root, &state).unwrap();
            let app = HostCtx::new(root.join("settings")); let recorder = RecorderState::new();
            dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
            let mut inputs = request();
            inputs["identity_text"] = json!(if reference_only { "" } else { "Silver bob, blue coat" });
            if reference_only { inputs["reference_images"] = json!(["Characters/Alice/ref.png"]); }
            let req = json!({"characterId":"alice", "action":"run_specialist", "specialist":"model_sheet", "inputs":inputs});
            let approval = dispatch(&app, &recorder, "start_character_model_sheet", &json!({"projectRoot":root,"request":req})).unwrap();
            assert_eq!(approval["status"], "pendingApproval");
            dispatch(&app, &recorder, "director_cancel", &json!({})).unwrap();
            assert!(!manifest_path(root).exists());
            let pack = prepare(root, &inputs).unwrap();
            assert_eq!(pack["text_only_gate"], !reference_only);
            assert_eq!(prepare(root, &inputs).unwrap(), pack, "Update accepts same minimal inputs");
            assert!(!Path::new(pack["shots"][0]["path"].as_str().unwrap()).exists());
            dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
        });
    }
}

fn request() -> Value {
    json!({"output_path":"Characters/Alice/model_sheet", "character_files":["Characters/Alice/character.yaml"], "character_slug":"Alice", "identity_text":"Original identity brief", "reference_images":[]})
}
fn fixture(test: impl FnOnce(&Path)) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("Characters/Alice")).unwrap();
    fs::write(dir.path().join("Characters/Alice/character.yaml"), "name: Alice\n").unwrap();
    let image = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i", "color=red:s=16x16", "-frames:v", "1", "-threads", "1", "-y"])
        .arg(dir.path().join("Characters/Alice/ref.png")).output().unwrap();
    assert!(image.status.success(), "{}", String::from_utf8_lossy(&image.stderr));
    test(dir.path());
}
fn prepare(root: &Path, request: &Value) -> Result<Value, String> {
    prepare_model_sheet_manifest(request.as_object().unwrap(), None, root.to_str())
        .map(|manifest| serde_json::to_value(manifest).unwrap())
}

fn manifest_path(root: &Path) -> std::path::PathBuf {
    root.join("Characters/Alice/model_sheet/model_sheet_manifest.yaml")
}
fn disk_manifest(root: &Path) -> Value {
    serde_yaml_ng::from_str(&fs::read_to_string(manifest_path(root)).unwrap()).unwrap()
}
fn save_disk(root: &Path, value: &Value) {
    fs::write(manifest_path(root), serde_yaml_ng::to_string(value).unwrap()).unwrap();
}

#[test]
fn ms02_initial_text_pack_holds_rest_and_is_byte_idempotent() {
    fixture(|root| {
        let req = request();
        let first = prepare(root, &req).unwrap();
        let bytes = fs::read(manifest_path(root)).unwrap();
        assert_eq!(first["shots"].as_array().unwrap().len(), 12);
        assert_eq!(first["shots"][0]["status"], "pending");
        assert!(first["shots"].as_array().unwrap()[1..].iter().all(|s| s["status"] == "held"));
        assert_eq!(first["identity_locked"], false);
        assert_eq!(first["text_only_gate"], true);
        assert_eq!(first["model_id"], "");
        assert!(first["shots"].as_array().unwrap().iter().all(|s| s["prediction_id"] == "" && !Path::new(s["path"].as_str().unwrap()).exists()));
        assert_eq!(prepare(root, &req).unwrap(), first);
        assert_eq!(fs::read(manifest_path(root)).unwrap(), bytes);
    });
}

#[test]
fn ms02_initial_image_conditioned_pack_uses_ordered_current_refs() {
    fixture(|root| {
        fs::copy(root.join("Characters/Alice/ref.png"), root.join("Characters/Alice/second.png")).unwrap();
        let mut req = request();
        req["reference_images"] = json!(["Characters/Alice/second.png", "Characters/Alice/ref.png"]);
        let pack = prepare(root, &req).unwrap();
        assert_eq!(pack["mode"], "image_conditioned");
        assert_eq!(pack["identity_locked"], true);
        assert_eq!(pack["text_only_gate"], false);
        assert_eq!(pack["reference_images"], json!([root.join("Characters/Alice/second.png"), root.join("Characters/Alice/ref.png")]));
        assert!(pack["shots"].as_array().unwrap().iter().all(|s| s["status"] == "pending"));
        assert_eq!(prepare(root, &req).unwrap(), pack);
    });
}

#[test]
fn ms02_changed_brief_preserves_saved_history_but_resets_current_gate() {
    fixture(|root| {
        let mut req = request();
        prepare(root, &req).unwrap();
        let mut old = disk_manifest(root);
        let saved_path = old["shots"][0]["path"].as_str().unwrap().to_owned();
        // This is a pre-existing saved fixture, never a generated result of preparation.
        fs::copy(root.join("Characters/Alice/ref.png"), &saved_path).unwrap();
        old["shots"][0]["status"] = json!("saved");
        old["shots"][0]["prediction_id"] = json!("historical-prediction");
        old["identity_locked"] = json!(true);
        old["mode"] = json!("image_conditioned");
        old["identity_source"]["approval_shot"] = json!("closeup_front");
        old["identity_source"]["approval_shot_path"] = json!(saved_path);
        old["identity_source"]["reference_images"] = json!([saved_path]);
        save_disk(root, &old);
        let saved_bytes = fs::read(&saved_path).unwrap();
        // Continuations preserve the runtime identity lock and exact saved state.
        req["identity_gate"] = json!("continue");
        assert_eq!(prepare(root, &req).unwrap()["shots"][0]["status"], "saved");
        req["identity_text"] = json!("New identity\nwith a \"quote\" and \\ backslash");
        let fresh = prepare(root, &req).unwrap();
        assert_eq!(fresh["preparation_inputs"]["identity_text"], req["identity_text"]);
        assert_eq!(fresh["mode"], "text_to_image");
        assert_eq!(fresh["identity_locked"], false);
        assert_eq!(fresh["approval_shot_path"], "");
        assert_eq!(fresh["shots"][0]["status"], "pending");
        assert_eq!(fresh["shots"][1]["status"], "held");
        assert_ne!(fresh["shots"][0]["path"], saved_path);
        assert_eq!(fresh["history"][0]["shots"][0]["path"], saved_path);
        assert_eq!(fresh["history"][0]["shots"][0]["status"], "saved");
        assert_eq!(fresh["history"][0]["shots"][0]["prediction_id"], "historical-prediction");
        assert_eq!(fs::read(saved_path).unwrap(), saved_bytes);
        assert_eq!(prepare(root, &req).unwrap(), fresh);
        req["identity_text"] = json!("Third brief");
        let third = prepare(root, &req).unwrap();
        assert_eq!(third["history"].as_array().unwrap().len(), 2);
        assert_eq!(third["history"][0], fresh["history"][0]);
    });
}

#[test]
fn ms02_legacy_saved_pack_is_archived_when_refs_are_removed() {
    fixture(|root| {
        let mut req = request();
        req["reference_images"] = json!(["Characters/Alice/ref.png"]);
        prepare(root, &req).unwrap();
        let mut legacy = disk_manifest(root);
        legacy.as_object_mut().unwrap().remove("preparation_inputs");
        legacy.as_object_mut().unwrap().remove("history");
        legacy["shots"][0]["status"] = json!("saved");
        save_disk(root, &legacy);
        req["reference_images"] = json!([]);
        let refreshed = prepare(root, &req).unwrap();
        assert_eq!(refreshed["history"][0]["shots"][0]["status"], "saved");
        assert_eq!(refreshed["history"][0]["reference_images"], legacy["identity_source"]["reference_images"]);
        assert_eq!(refreshed["mode"], "text_to_image");
        assert_eq!(refreshed["shots"][0]["status"], "pending");
        assert!(refreshed["shots"].as_array().unwrap()[1..].iter().all(|s| s["status"] == "held"));
        assert_eq!(prepare(root, &req).unwrap(), refreshed);
    });
}

#[test]
fn ms02_filtered_pack_keeps_first_plate_gate() {
    fixture(|root| {
        let mut req = request();
        req["shot_ids"] = json!(["standing_front", "standing_back"]);
        let pack = prepare(root, &req).unwrap();
        assert_eq!(pack["shots"].as_array().unwrap().len(), 2);
        assert_eq!(pack["shots"][0]["id"], "standing_front");
        assert_eq!(pack["shots"][0]["status"], "pending");
        assert_eq!(pack["shots"][1]["id"], "standing_back");
        assert_eq!(pack["shots"][1]["status"], "held");
    });
}

#[test]
fn ms02_saved_character_canon_changes_invalidate_old_pack() {
    fixture(|root| {
        let req = request();
        prepare(root, &req).unwrap();
        fs::write(root.join("Characters/Alice/character.yaml"), "name: Alice\nappearance: new silver hair\n").unwrap();
        let changed = prepare(root, &req).unwrap();
        assert_eq!(changed["history"].as_array().unwrap().len(), 1);
        assert_eq!(changed["shots"][0]["status"], "pending");
        // The runner's own generated-reference bookkeeping must not stale a continuation.
        fs::write(root.join("Characters/Alice/character.yaml"), "name: Alice\nappearance: new silver hair\n\n## Reference Assets\nCharacter image reference: generated.png\nAdditional image references: other.png\nAsset notes: model sheet pack\n").unwrap();
        assert_eq!(prepare(root, &req).unwrap(), changed);
    });
}

#[test]
fn ms02_invalid_request_leaves_prior_bytes_intact() {
    fixture(|root| {
        let req = request();
        prepare(root, &req).unwrap();
        let bytes = fs::read(manifest_path(root)).unwrap();
        for (key, value) in [
            ("reference_images", json!(["missing.png"])),
            ("reference_images", json!(["../../outside.png"])),
            ("reference_images", json!([3])),
            ("reference_images", json!("not-an-array")),
            ("identity_text", json!("  ")),
            ("identity_text", json!(17)),
            ("character_files", json!(["missing.yaml"])),
            ("shot_ids", json!(["not_a_shot"])),
            ("output_path", json!("../../outside")),
        ] {
            let mut bad = req.clone();
            bad[key] = value;
            assert!(prepare(root, &bad).is_err(), "{key}: {bad}");
            assert_eq!(fs::read(manifest_path(root)).unwrap(), bytes);
        }
    });
}

#[test]
fn ms02_corrupt_manifest_is_not_silently_reinitialized() {
    fixture(|root| {
        prepare(root, &request()).unwrap();
        for corrupt in ["not: [valid YAML", "shots: []\n", "identity_source: {}\nshots: [42]\n"] {
            fs::write(manifest_path(root), corrupt).unwrap();
            assert!(prepare(root, &request()).is_err());
            assert_eq!(fs::read_to_string(manifest_path(root)).unwrap(), corrupt);
        }
    });
}

#[test]
fn ms02_existing_snapshot_is_checked_against_disk() {
    fixture(|root| {
        let req = request();
        let first = prepare_model_sheet_manifest(req.as_object().unwrap(), None, root.to_str()).unwrap();
        let supplied = prepare_model_sheet_manifest(req.as_object().unwrap(), Some(first.clone()), root.to_str()).unwrap();
        assert_eq!(serde_json::to_value(supplied).unwrap(), serde_json::to_value(&first).unwrap());
        let mut disk = disk_manifest(root);
        disk["shots"][0]["status"] = json!("submitted");
        disk["shots"][0]["prediction_id"] = json!("existing-submission");
        save_disk(root, &disk);
        let bytes = fs::read(manifest_path(root)).unwrap();
        assert!(prepare_model_sheet_manifest(req.as_object().unwrap(), Some(first), root.to_str()).is_err());
        assert_eq!(fs::read(manifest_path(root)).unwrap(), bytes);
        assert_eq!(prepare(root, &req).unwrap()["shots"][0]["status"], "submitted");
    });
}

#[cfg(unix)]
#[test]
fn ms02_write_failure_preserves_prior_manifest() {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    let mut denied = false;
    fixture(|root| {
            let mut req = request();
            prepare(root, &req).unwrap();
            let path = manifest_path(root);
            let bytes = fs::read(&path).unwrap();
            let parent = path.parent().unwrap();
            fs::set_permissions(parent, fs::Permissions::from_mode(0o555)).unwrap();
            req["identity_text"] = json!("changed brief");
            let result = prepare(root, &req);
            fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
            if let Err(error) = result {
                assert!(error.contains("Permission denied"), "{error}");
                assert_eq!(fs::read(path).unwrap(), bytes);
                denied = true;
            }
        });
    if denied { return; }
    assert!(std::env::var_os("MS02_WRITE_FAILURE_CHILD").is_none(), "unprivileged child unexpectedly bypassed permissions");
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", "ms02_write_failure_preserves_prior_manifest", "--nocapture"])
        .env("MS02_WRITE_FAILURE_CHILD", "1");
    // Root cannot exercise chmod denial; ordinary unprivileged runs need no setuid.
    if std::process::Command::new("id").arg("-u").output().unwrap().stdout == b"0\n" {
        command.uid(65534).gid(65534);
    }
    let output = command.output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn ms02_changed_refs_refresh_existing_manifest_without_generation() {
    fixture(|root| {
        let mut req = request();
        let first = prepare(root, &req).unwrap();
        assert_eq!(first["mode"], "text_to_image");
        req["reference_images"] = json!(["Characters/Alice/ref.png"]);
        let updated = prepare(root, &req).unwrap();
        assert_eq!(updated["mode"], "image_conditioned");
        assert_eq!(updated["reference_images"], json!([root.join("Characters/Alice/ref.png")]));
        assert!(updated["shots"].as_array().unwrap().iter().all(|s| s["status"] == "pending"));
        assert!(!root.join("Characters/Alice/model_sheet/alice_closeup_front.png").exists());
    });
}

#[test]
fn model_sheet_delete_wrong_owner_refused() {
    fixture(|root| {
        use workbench_host::{
            ctx::HostCtx,
            dispatch,
            voice::RecorderState,
            workflow::{
                import::{load_state, persist_state},
                state::ProjectState,
            },
        };

        let req = request();
        prepare(root, &req).unwrap();
        let mut manifest = disk_manifest(root);
        let photo_path = Path::new(manifest["shots"][3]["path"].as_str().unwrap()).to_path_buf();
        fs::copy(root.join("Characters/Alice/ref.png"), &photo_path).unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("owned-by-alice");
        save_disk(root, &manifest);
        let relative = photo_path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([
            {
                "id":"alice", "name":"Alice", "dir":"Characters/Alice",
                "profile_path":"Characters/Alice/character.yaml",
                "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
                "identity_ready":true, "voice_ready":false, "asset_ids":["alice-photo"],
                "reference_asset_ids":["alice-photo"]
            },
            {
                "id":"bob", "name":"Bob", "dir":"Characters/Bob",
                "profile_path":"Characters/Bob/character.yaml",
                "model_sheet_manifest_path":"Characters/Bob/model_sheet/model_sheet_manifest.yaml",
                "identity_ready":true, "voice_ready":false, "asset_ids":["alice-photo"],
                "reference_asset_ids":["alice-photo"]
            }
        ]))
        .unwrap();
        state.assets = serde_json::from_value(json!([{
            "id":"alice-photo", "path":relative, "role":"body_reference",
            "status":"present", "character_id":"alice", "notes":"standing_front"
        }]))
        .unwrap();
        persist_state(root, &state).unwrap();

        let state_path = root.join("Preproduction/workflow_state.json");
        let state_before = fs::read(&state_path).unwrap();
        let manifest_before = fs::read(manifest_path(root)).unwrap();
        let photo_before = fs::read(&photo_path).unwrap();
        assert!(!root.join(".workbench-trash").exists());

        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        let error = dispatch(
            &app,
            &recorder,
            "workflow_delete_model_sheet_photo",
            &json!({
                "projectRoot":root, "characterId":"bob", "assetId":"alice-photo"
            }),
        )
        .unwrap_err();

        assert_eq!(error, "saved model-sheet photo not found");
        assert_eq!(fs::read(&state_path).unwrap(), state_before);
        assert_eq!(fs::read(manifest_path(root)).unwrap(), manifest_before);
        assert_eq!(fs::read(&photo_path).unwrap(), photo_before);
        assert!(!root.join(".workbench-trash").exists());
        let after = load_state(root).unwrap();
        assert_eq!(after.assets[0].character_id.as_deref(), Some("alice"));
        assert!(after.characters[0].asset_ids.iter().any(|id| id == "alice-photo"));
    });
}

#[test]
fn model_sheet_delete_trash_exact_bytes() {
    fixture(|root| {
        use workbench_host::{
            ctx::HostCtx,
            dispatch,
            voice::RecorderState,
            workflow::{import::persist_state, state::ProjectState},
        };

        prepare(root, &request()).unwrap();
        let mut manifest = disk_manifest(root);
        let photo_path = Path::new(manifest["shots"][3]["path"].as_str().unwrap()).to_path_buf();
        let original_bytes = b"W4-07 selected photo bytes\n\0exact payload\xff";
        fs::write(&photo_path, original_bytes).unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("selected-photo-prediction");
        save_disk(root, &manifest);
        let relative = photo_path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{
            "id":"alice", "name":"Alice", "dir":"Characters/Alice",
            "profile_path":"Characters/Alice/character.yaml",
            "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
            "identity_ready":true, "voice_ready":false, "asset_ids":["selected-photo"],
            "reference_asset_ids":["selected-photo"]
        }]))
        .unwrap();
        state.assets = serde_json::from_value(json!([{
            "id":"selected-photo", "path":relative, "role":"body_reference",
            "status":"present", "character_id":"alice", "notes":"standing_front"
        }]))
        .unwrap();
        persist_state(root, &state).unwrap();

        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        dispatch(
            &app,
            &recorder,
            "workflow_delete_model_sheet_photo",
            &json!({
                "projectRoot":root, "characterId":"alice", "assetId":"selected-photo"
            }),
        )
        .unwrap();

        assert!(!photo_path.exists(), "selected source must disappear");
        let trash_entries = fs::read_dir(root.join(".workbench-trash"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(trash_entries.len(), 1, "deletion must create one trash entry");
        assert_eq!(fs::read(&trash_entries[0]).unwrap(), original_bytes);
    });
}

#[test]
fn model_sheet_delete_survivor_unchanged() {
    fixture(|root| {
        use workbench_host::{
            ctx::HostCtx,
            dispatch,
            voice::RecorderState,
            workflow::{import::persist_state, state::ProjectState},
        };

        prepare(root, &request()).unwrap();
        let mut manifest = disk_manifest(root);
        let selected_path = Path::new(manifest["shots"][3]["path"].as_str().unwrap()).to_path_buf();
        let survivor_path = Path::new(manifest["shots"][4]["path"].as_str().unwrap()).to_path_buf();
        let selected_bytes = b"W4-08 selected photo bytes";
        let survivor_bytes = b"W4-08 neighboring photo bytes\n\0must remain exact\xff";
        fs::write(&selected_path, selected_bytes).unwrap();
        fs::write(&survivor_path, survivor_bytes).unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("selected-prediction");
        manifest["shots"][4]["status"] = json!("saved");
        manifest["shots"][4]["prediction_id"] = json!("survivor-prediction");
        save_disk(root, &manifest);

        let relative = |path: &Path| {
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        };
        let selected_relative = relative(&selected_path);
        let survivor_relative = relative(&survivor_path);

        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{
            "id":"alice", "name":"Alice", "dir":"Characters/Alice",
            "profile_path":"Characters/Alice/character.yaml",
            "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
            "identity_ready":true, "voice_ready":false,
            "asset_ids":["selected-photo", "survivor-photo"],
            "reference_asset_ids":["selected-photo", "survivor-photo"]
        }]))
        .unwrap();
        state.assets = serde_json::from_value(json!([
            {
                "id":"selected-photo", "path":selected_relative, "role":"body_reference",
                "status":"present", "character_id":"alice", "notes":"standing_front"
            },
            {
                "id":"survivor-photo", "path":survivor_relative, "role":"body_reference",
                "status":"present", "character_id":"alice", "notes":"standing_back"
            }
        ]))
        .unwrap();
        persist_state(root, &state).unwrap();

        let original_survivor_path = survivor_path.clone();
        let original_survivor_bytes = fs::read(&survivor_path).unwrap();
        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        dispatch(
            &app,
            &recorder,
            "workflow_delete_model_sheet_photo",
            &json!({
                "projectRoot":root, "characterId":"alice", "assetId":"selected-photo"
            }),
        )
        .unwrap();

        assert!(!selected_path.exists(), "selected source must disappear");
        assert_eq!(survivor_path, original_survivor_path);
        assert!(survivor_path.exists(), "neighbor must remain at its original path");
        assert_eq!(fs::read(&survivor_path).unwrap(), original_survivor_bytes);
        assert_eq!(original_survivor_bytes, survivor_bytes);
    });
}

#[test]
fn model_sheet_delete_asset_inventory() {
    fixture(|root| {
        use workbench_host::{
            ctx::HostCtx,
            dispatch,
            voice::RecorderState,
            workflow::{
                import::{load_state, persist_state},
                state::ProjectState,
            },
        };

        prepare(root, &request()).unwrap();
        let mut manifest = disk_manifest(root);
        let selected_path = Path::new(manifest["shots"][3]["path"].as_str().unwrap()).to_path_buf();
        fs::write(&selected_path, b"W4-09 selected photo bytes").unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("selected-prediction");
        save_disk(root, &manifest);
        let selected_relative = selected_path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{
            "id":"alice", "name":"Alice", "dir":"Characters/Alice",
            "profile_path":"Characters/Alice/character.yaml",
            "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
            "identity_ready":true, "voice_ready":false,
            "asset_ids":["selected-photo", "survivor-photo"],
            "reference_asset_ids":["selected-photo", "survivor-photo"]
        }]))
        .unwrap();
        state.assets = serde_json::from_value(json!([
            {
                "id":"selected-photo", "path":selected_relative, "role":"body_reference",
                "status":"present", "character_id":"alice", "notes":"standing_front"
            },
            {
                "id":"survivor-photo", "path":"Characters/Alice/ref.png",
                "role":"body_reference", "status":"present", "character_id":"alice",
                "notes":"standing_back"
            }
        ]))
        .unwrap();
        persist_state(root, &state).unwrap();

        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        let result = dispatch(
            &app,
            &recorder,
            "workflow_delete_model_sheet_photo",
            &json!({
                "projectRoot":root, "characterId":"alice", "assetId":"selected-photo"
            }),
        )
        .unwrap();

        let persisted_ids = load_state(root)
            .unwrap()
            .assets
            .into_iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>();
        let returned_ids = result["state"]["assets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|asset| asset["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(persisted_ids, vec!["survivor-photo"]);
        assert_eq!(returned_ids, vec!["survivor-photo"]);
    });
}

#[test]
fn model_sheet_delete_character_binding() {
    fixture(|root| {
        use workbench_host::{
            ctx::HostCtx,
            dispatch,
            voice::RecorderState,
            workflow::{
                import::{load_state, persist_state},
                state::ProjectState,
            },
        };

        prepare(root, &request()).unwrap();
        let mut manifest = disk_manifest(root);
        let selected_path =
            Path::new(manifest["shots"][3]["path"].as_str().unwrap()).to_path_buf();
        fs::write(&selected_path, b"W4-10 selected photo bytes").unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("selected-prediction");
        save_disk(root, &manifest);
        let selected_relative = selected_path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{
            "id":"alice", "name":"Alice", "dir":"Characters/Alice",
            "profile_path":"Characters/Alice/character.yaml",
            "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
            "identity_ready":true, "voice_ready":false,
            "asset_ids":["selected-photo", "survivor-photo"],
            "reference_asset_ids":["selected-photo", "survivor-photo"]
        }]))
        .unwrap();
        state.assets = serde_json::from_value(json!([
            {
                "id":"selected-photo", "path":selected_relative, "role":"body_reference",
                "status":"present", "character_id":"alice", "notes":"standing_front"
            },
            {
                "id":"survivor-photo", "path":"Characters/Alice/ref.png",
                "role":"body_reference", "status":"present", "character_id":"alice",
                "notes":"standing_back"
            }
        ]))
        .unwrap();
        persist_state(root, &state).unwrap();

        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        let result = dispatch(
            &app,
            &recorder,
            "workflow_delete_model_sheet_photo",
            &json!({
                "projectRoot":root, "characterId":"alice", "assetId":"selected-photo"
            }),
        )
        .unwrap();

        let persisted = load_state(root).unwrap();
        assert_eq!(persisted.characters[0].asset_ids, vec!["survivor-photo"]);
        assert_eq!(
            persisted.characters[0].reference_asset_ids,
            vec!["survivor-photo"]
        );
        assert_eq!(
            result["state"]["characters"][0]["asset_ids"],
            json!(["survivor-photo"])
        );
        assert_eq!(
            result["state"]["characters"][0]["reference_asset_ids"],
            json!(["survivor-photo"])
        );
    });
}

#[test]
fn model_sheet_delete_job_references() {
    fixture(|root| {
        use workbench_host::{
            ctx::HostCtx,
            dispatch,
            voice::RecorderState,
            workflow::{
                import::{load_state, persist_state},
                state::{JobRecord, ProjectState},
            },
        };

        prepare(root, &request()).unwrap();
        let mut manifest = disk_manifest(root);
        let selected_path =
            Path::new(manifest["shots"][3]["path"].as_str().unwrap()).to_path_buf();
        fs::write(&selected_path, b"W4-11 selected photo bytes").unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("selected-prediction");
        save_disk(root, &manifest);
        let selected_relative = selected_path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{
            "id":"alice", "name":"Alice", "dir":"Characters/Alice",
            "profile_path":"Characters/Alice/character.yaml",
            "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
            "identity_ready":true, "voice_ready":false,
            "asset_ids":["selected-photo", "survivor-photo"],
            "reference_asset_ids":["selected-photo", "survivor-photo"]
        }]))
        .unwrap();
        state.assets = serde_json::from_value(json!([
            {
                "id":"selected-photo", "path":selected_relative, "role":"body_reference",
                "status":"present", "character_id":"alice", "notes":"standing_front"
            },
            {
                "id":"survivor-photo", "path":"Characters/Alice/ref.png",
                "role":"body_reference", "status":"present", "character_id":"alice",
                "notes":"standing_back"
            }
        ]))
        .unwrap();
        let mut affected_job = JobRecord::open("affected-job", "image");
        affected_job.input_asset_ids = vec!["selected-photo".into(), "survivor-photo".into()];
        affected_job.output_asset_ids = vec!["selected-photo".into(), "survivor-photo".into()];
        let mut unrelated_job = JobRecord::open("unrelated-job", "image");
        unrelated_job.input_asset_ids = vec!["survivor-photo".into()];
        unrelated_job.output_asset_ids = vec!["survivor-photo".into()];
        state.jobs = vec![affected_job, unrelated_job];
        persist_state(root, &state).unwrap();

        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        let result = dispatch(
            &app,
            &recorder,
            "workflow_delete_model_sheet_photo",
            &json!({
                "projectRoot":root, "characterId":"alice", "assetId":"selected-photo"
            }),
        )
        .unwrap();

        let persisted = load_state(root).unwrap();
        assert_eq!(persisted.jobs.len(), 2);
        for job in &persisted.jobs {
            assert!(!job.input_asset_ids.iter().any(|id| id == "selected-photo"));
            assert!(!job.output_asset_ids.iter().any(|id| id == "selected-photo"));
            assert_eq!(job.input_asset_ids, vec!["survivor-photo"]);
            assert_eq!(job.output_asset_ids, vec!["survivor-photo"]);
        }
        for job in result["state"]["jobs"].as_array().unwrap() {
            assert_eq!(job["input_asset_ids"], json!(["survivor-photo"]));
            assert_eq!(job["output_asset_ids"], json!(["survivor-photo"]));
        }
    });
}

#[test]
fn ms05_delete_saved_photo_trashes_file_and_reopens_manifest_shot() {
    fixture(|root| {
        use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import::{load_state, persist_state}, state::ProjectState}};
        let mut req = request();
        req["reference_images"] = json!(["Characters/Alice/ref.png"]);
        prepare(root, &req).unwrap();
        let mut manifest = disk_manifest(root);
        let absolute = manifest["shots"][3]["path"].as_str().unwrap().to_string();
        fs::copy(root.join("Characters/Alice/ref.png"), &absolute).unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("prediction-to-clear");
        save_disk(root, &manifest);
        let relative = Path::new(&absolute).strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{
            "id":"alice", "name":"Alice", "dir":"Characters/Alice",
            "profile_path":"Characters/Alice/character.yaml",
            "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
            "identity_ready":true, "voice_ready":false, "asset_ids":["model-photo"],
            "reference_asset_ids":["model-photo"]
        }])).unwrap();
        state.assets = serde_json::from_value(json!([{
            "id":"model-photo", "path":relative, "role":"body_reference",
            "status":"present", "character_id":"alice", "notes":"standing_front"
        }])).unwrap();
        persist_state(root, &state).unwrap();

        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        let result = dispatch(&app, &recorder, "workflow_delete_model_sheet_photo", &json!({
            "projectRoot":root, "characterId":"alice", "assetId":"model-photo"
        })).unwrap();

        assert!(!Path::new(&absolute).exists());
        assert!(fs::read_dir(root.join(".workbench-trash")).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().ends_with("alice_standing_front.png")));
        let after_manifest = disk_manifest(root);
        assert_eq!(after_manifest["shots"][3]["status"], "pending");
        assert_eq!(after_manifest["shots"][3]["prediction_id"], "");
        let after = load_state(root).unwrap();
        assert!(after.assets.iter().all(|asset| asset.id != "model-photo"));
        assert!(after.characters[0].asset_ids.is_empty());
        assert!(after.characters[0].reference_asset_ids.is_empty());
        assert!(result["state"]["characters"][0]["asset_ids"].as_array().is_none_or(|ids| ids.is_empty()));
    });
}

#[test]
fn model_sheet_delete_manifest_reopens_shot() {
    fixture(|root| {
        use workbench_host::{
            ctx::HostCtx,
            dispatch,
            voice::RecorderState,
            workflow::{import::persist_state, state::ProjectState},
        };

        let mut req = request();
        req["reference_images"] = json!(["Characters/Alice/ref.png"]);
        prepare(root, &req).unwrap();
        let mut manifest = disk_manifest(root);
        let selected_path = manifest["shots"][3]["path"].as_str().unwrap().to_owned();
        fs::copy(root.join("Characters/Alice/ref.png"), &selected_path).unwrap();
        manifest["shots"][3]["status"] = json!("saved");
        manifest["shots"][3]["prediction_id"] = json!("selected-prediction");
        manifest["shots"][7]["status"] = json!("submitted");
        manifest["shots"][7]["prediction_id"] = json!("neighbor-prediction");
        save_disk(root, &manifest);
        let before_shots = manifest["shots"].as_array().unwrap().clone();
        let selected_relative = Path::new(&selected_path)
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let mut state = ProjectState::default();
        state.profile = Some(Default::default());
        state.characters = serde_json::from_value(json!([{
            "id":"alice", "name":"Alice", "dir":"Characters/Alice",
            "profile_path":"Characters/Alice/character.yaml",
            "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
            "identity_ready":true, "voice_ready":false,
            "asset_ids":["selected-photo"], "reference_asset_ids":["selected-photo"]
        }]))
        .unwrap();
        state.assets = serde_json::from_value(json!([{
            "id":"selected-photo", "path":selected_relative, "role":"body_reference",
            "status":"present", "character_id":"alice", "notes":"standing_front"
        }]))
        .unwrap();
        persist_state(root, &state).unwrap();

        let app = HostCtx::new(root.join("settings"));
        let recorder = RecorderState::new();
        dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
        dispatch(
            &app,
            &recorder,
            "workflow_delete_model_sheet_photo",
            &json!({
                "projectRoot":root, "characterId":"alice", "assetId":"selected-photo"
            }),
        )
        .unwrap();

        let after_shots = disk_manifest(root)["shots"].as_array().unwrap().clone();
        assert_eq!(after_shots[3]["status"], "pending");
        assert_eq!(after_shots[3]["prediction_id"], "");
        for (index, before) in before_shots.iter().enumerate() {
            if index != 3 {
                assert_eq!(&after_shots[index], before, "manifest slot {index} changed");
            }
        }
    });
}

#[test]
fn model_sheet_delete_approval_plate_reset() {
    for delete_approval_plate in [true, false] {
        fixture(|root| {
            use workbench_host::{
                ctx::HostCtx,
                dispatch,
                voice::RecorderState,
                workflow::{import::persist_state, state::ProjectState},
            };

            let mut req = request();
            req["reference_images"] = json!(["Characters/Alice/ref.png"]);
            prepare(root, &req).unwrap();
            let mut manifest = disk_manifest(root);
            let approval_path = manifest["shots"][0]["path"].as_str().unwrap().to_owned();
            let other_path = manifest["shots"][3]["path"].as_str().unwrap().to_owned();
            fs::copy(root.join("Characters/Alice/ref.png"), &approval_path).unwrap();
            fs::copy(root.join("Characters/Alice/ref.png"), &other_path).unwrap();
            manifest["shots"][0]["status"] = json!("saved");
            manifest["shots"][0]["prediction_id"] = json!("approval-prediction");
            manifest["shots"][3]["status"] = json!("saved");
            manifest["shots"][3]["prediction_id"] = json!("pose-prediction");
            manifest["identity_source"]["approval_shot"] = json!("closeup_front");
            manifest["identity_source"]["approval_shot_path"] = json!(approval_path);
            manifest["identity_source"]["reference_images"] = json!([approval_path]);
            manifest["identity_locked"] = json!(true);
            manifest["mode"] = json!("image_conditioned");
            save_disk(root, &manifest);
            let original_identity = manifest["identity_source"].clone();

            let deleted_path = if delete_approval_plate {
                approval_path
            } else {
                other_path
            };
            let deleted_relative = Path::new(&deleted_path)
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let mut state = ProjectState::default();
            state.profile = Some(Default::default());
            state.characters = serde_json::from_value(json!([{
                "id":"alice", "name":"Alice", "dir":"Characters/Alice",
                "profile_path":"Characters/Alice/character.yaml",
                "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml",
                "identity_ready":true, "voice_ready":false,
                "asset_ids":["deleted-photo"], "reference_asset_ids":["deleted-photo"]
            }]))
            .unwrap();
            state.assets = serde_json::from_value(json!([{
                "id":"deleted-photo", "path":deleted_relative, "role":"body_reference",
                "status":"present", "character_id":"alice", "notes":"generated plate"
            }]))
            .unwrap();
            persist_state(root, &state).unwrap();

            let app = HostCtx::new(root.join("settings"));
            let recorder = RecorderState::new();
            dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
            dispatch(
                &app,
                &recorder,
                "workflow_delete_model_sheet_photo",
                &json!({
                    "projectRoot":root, "characterId":"alice", "assetId":"deleted-photo"
                }),
            )
            .unwrap();

            let after = disk_manifest(root);
            if delete_approval_plate {
                assert_eq!(after["identity_source"]["approval_shot"], "");
                assert_eq!(after["identity_source"]["approval_shot_path"], "");
                assert_eq!(after["identity_source"]["reference_images"], json!([]));
                assert_eq!(after["identity_locked"], false);
                assert_eq!(after["mode"], "text_to_image");
            } else {
                assert_eq!(after["identity_source"], original_identity);
                assert_eq!(after["identity_locked"], true);
                assert_eq!(after["mode"], "image_conditioned");
            }
        });
    }
}

use serde_json::{json, Value};
use std::{fs, path::Path, process::Command, sync::Mutex};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState};

static SESSION: Mutex<()> = Mutex::new(());

fn project(test: impl FnOnce(&Path, &HostCtx, &RecorderState)) {
    let _lock = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Characters/Alice")).unwrap();
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    // Ingestion must not load/repair state or change identity/reference associations.
    fs::write(root.join("Preproduction/workflow_state.json"), b"identity and associations untouched").unwrap();
    fs::write(root.join("Characters/Alice/character.yaml"), b"name: Alice\n").unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
    test(&root, &app, &recorder);
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(), b"identity and associations untouched");
    assert_eq!(fs::read(root.join("Characters/Alice/character.yaml")).unwrap(), b"name: Alice\n");
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}

fn image(path: &Path) -> Vec<u8> {
    let output = Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "color=red:s=16x16", "-frames:v", "1", "-threads", "1", "-y"]).arg(path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    fs::read(path).unwrap()
}

fn ingest(app: &HostCtx, recorder: &RecorderState, root: &Path, character: &str, source: &Path) -> Result<Value, String> {
    dispatch(app, recorder, "ingest_character_reference_image", &json!({"projectRoot":root, "characterDir":character, "sourcePath":source}))
}

#[test]
fn cr04_acquisition_receipt_distinguishes_owned_copy_from_existing_bytes() {
    project(|root, app, recorder| {
        let source = root.join("portrait.png");
        let bytes = image(&source);
        let args = json!({"projectRoot":root, "characterDir":"Characters/Alice", "sourcePath":source, "receipt":true});
        let first = dispatch(app, recorder, "ingest_character_reference_image", &args).unwrap();
        assert_eq!(first, json!({"path":"Characters/Alice/refs/portrait.png", "created":true}));
        let again = dispatch(app, recorder, "ingest_character_reference_image", &args).unwrap();
        assert_eq!(again, json!({"path":"Characters/Alice/refs/portrait.png", "created":false}));
        // Existing CR01 callers still receive the stable path string.
        assert_eq!(ingest(app, recorder, root, "Characters/Alice", &source).unwrap(), json!("Characters/Alice/refs/portrait.png"));
        assert_eq!(fs::read(root.join(first["path"].as_str().unwrap())).unwrap(), bytes);
        assert_eq!(fs::read(source).unwrap(), bytes);
    });
}

#[test]
fn cr01_rejects_corrupt_unsupported_missing_and_directory_sources_without_assets() {
    project(|root, app, recorder| {
        let png = image(&root.join("valid.png"));
        let jpeg = image(&root.join("valid.jpg"));
        let mut broken_payload = png.clone();
        // Preserve signature and IEND, destroy IDAT payload: headers alone aren't validation.
        let idat = broken_payload.windows(4).position(|w| w == b"IDAT").unwrap();
        broken_payload[idat + 4] ^= 0xff;
        for (name, bytes) in [
            ("corrupt.png", b"not an image".to_vec()),
            ("truncated.jpg", jpeg[..jpeg.len() / 2].to_vec()),
            ("payload.png", broken_payload),
            ("mismatch.jpg", png.clone()),
            ("unsupported.gif", png),
        ] {
            let source = root.join(name);
            fs::write(&source, &bytes).unwrap();
            assert!(ingest(app, recorder, root, "Characters/Alice", &source).is_err(), "{name}");
            assert_eq!(fs::read(source).unwrap(), bytes);
        }
        fs::create_dir(root.join("directory.png")).unwrap();
        for source in ["missing.png", "directory.png"] {
            assert!(ingest(app, recorder, root, "Characters/Alice", Path::new(source)).is_err());
        }
        assert!(!root.join("Characters/Alice/refs").exists());
    });
}

#[cfg(unix)]
#[test]
fn cr01_unreadable_image_is_rejected_without_partial_asset() {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    project(|root, app, recorder| {
        let source = root.join("denied.png");
        image(&source);
        fs::set_permissions(&source, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&source).is_ok() {
            // The workbench runs as root; exercise real permission denial in a
            // child of this same integration executable, not a mocked reader.
            assert!(std::env::var_os("CR01_UNPRIVILEGED").is_none());
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "cr01_unreadable_image_is_rejected_without_partial_asset", "--nocapture"])
                .env("CR01_UNPRIVILEGED", "1").uid(65534).gid(65534).output().unwrap();
            assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        } else {
            assert!(ingest(app, recorder, root, "Characters/Alice", &source).unwrap_err().contains("Could not read image"));
            assert!(!root.join("Characters/Alice/refs").exists());
        }
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
    });
}

#[test]
fn cr01_conflicting_destination_preserves_both_files() {
    project(|root, app, recorder| {
        let bytes = image(&root.join("portrait.png"));
        let refs = root.join("Characters/Alice/refs");
        fs::create_dir(&refs).unwrap();
        fs::write(refs.join("portrait.png"), b"existing asset").unwrap();
        assert!(ingest(app, recorder, root, "Characters/Alice", Path::new("portrait.png")).is_err());
        assert_eq!(fs::read(refs.join("portrait.png")).unwrap(), b"existing asset");
        assert_eq!(fs::read(root.join("portrait.png")).unwrap(), bytes);
        assert_eq!(fs::read_dir(refs).unwrap().count(), 1);
    });
}

#[test]
fn cr01_external_read_requires_exact_consumed_once_grant_and_open_root() {
    project(|root, app, recorder| {
        let source = root.parent().unwrap().join("external.png");
        let bytes = image(&source);
        assert!(ingest(app, recorder, root, "Characters/Alice", &source).is_err());
        dispatch(app, recorder, "grant_path", &json!({"path":source,"intent":"read"})).unwrap();
        assert!(ingest(app, recorder, root.parent().unwrap(), "Characters/Alice", &source).is_err());
        assert_eq!(ingest(app, recorder, root, "Characters/Alice", &source).unwrap(), json!("Characters/Alice/refs/external.png"));
        assert!(ingest(app, recorder, root, "Characters/Alice", &source).is_err());
        assert_eq!(fs::read(source).unwrap(), bytes);
    });
}

#[cfg(unix)]
#[test]
fn cr01_destination_and_source_escapes_cannot_write_outside_or_other_character() {
    use std::os::unix::fs::symlink;
    project(|root, app, recorder| {
        let source = root.join("portrait.png");
        let bytes = image(&source);
        let outside = root.parent().unwrap().join("outside");
        fs::create_dir(&outside).unwrap();
        for character in ["../outside", outside.to_str().unwrap(), "Scripts/Alice", "Characters/Alice/../../outside", "Characters/Missing"] {
            assert!(ingest(app, recorder, root, character, &source).is_err(), "{character}");
        }
        let bob = root.join("Characters/Bob");
        fs::create_dir(&bob).unwrap();
        symlink(&bob, root.join("Characters/Alias")).unwrap();
        assert!(ingest(app, recorder, root, "Characters/Alias", &source).is_err());
        let refs = root.join("Characters/Alice/refs");
        for redirect in [&outside, &bob, &outside.join("missing")] {
            symlink(redirect, &refs).unwrap();
            assert!(ingest(app, recorder, root, "Characters/Alice", &source).is_err());
            fs::remove_file(&refs).unwrap();
        }
        fs::create_dir(&refs).unwrap();
        for redirect in [&source, &outside.join("portrait.png"), &bob.join("portrait.png")] {
            symlink(redirect, refs.join("portrait.png")).unwrap();
            assert!(ingest(app, recorder, root, "Characters/Alice", &source).is_err());
            fs::remove_file(refs.join("portrait.png")).unwrap();
        }
        let external = outside.join("external.png");
        fs::write(&external, &bytes).unwrap();
        symlink(&external, root.join("source-alias.png")).unwrap();
        assert!(ingest(app, recorder, root, "Characters/Alice", Path::new("source-alias.png")).is_err());
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&bob).unwrap().count(), 0);
        assert_eq!(fs::read_dir(refs).unwrap().count(), 0);
    });
}

#[test]
fn cr01_png_jpeg_exact_copy_and_idempotent_path_without_workflow_mutation() {
    project(|root, app, recorder| {
        for name in ["portrait.png", "portrait.jpg", "portrait.jpeg"] {
            let source = root.parent().unwrap().join(name);
            let bytes = image(&source);
            for _ in 0..2 {
                dispatch(app, recorder, "grant_path", &json!({"path":source,"intent":"read"})).unwrap();
                let relative = format!("Characters/Alice/refs/{name}");
                assert_eq!(ingest(app, recorder, root, "Characters/Alice", &source).unwrap(), json!(relative));
                assert_eq!(fs::read(root.join(relative)).unwrap(), bytes);
                assert_eq!(fs::read(&source).unwrap(), bytes);
            }
        }
        assert_eq!(fs::read_dir(root.join("Characters/Alice/refs")).unwrap().count(), 3);
        assert!(!root.join("Characters/Alice/model_sheet").exists());
    });
}

fn selection_project(test: impl FnOnce(&Path, &HostCtx, &RecorderState)) {
    let _lock = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    for name in ["Alice", "Bob"] {
        fs::create_dir_all(root.join(format!("Characters/{name}/refs"))).unwrap();
    }
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    let mut state = workbench_host::workflow::state::ProjectState::new();
    state.profile = Some(Default::default());
    let character = |name: &str| json!({
        "id":name.to_lowercase(), "name":name, "dir":format!("Characters/{name}"),
        "identity_ready":name == "Bob", "voice_ready":false,
        "profile":{"description":"Preserved", "appearance":["red hair"], "identity_prompt":"historical prompt"},
        "asset_ids":[format!("{}-1", name.to_lowercase()), format!("{}-2", name.to_lowercase())],
        "reference_asset_ids":[format!("{}-1", name.to_lowercase())]
    });
    state.characters = serde_json::from_value(json!([character("Alice"), character("Bob")])).unwrap();
    for name in ["alice", "bob"] {
        for n in [1, 2] {
            let path = format!("Characters/{}/refs/{n}.png", if name == "alice" {"Alice"} else {"Bob"});
            image(&root.join(&path));
            state.assets.push(serde_json::from_value(json!({"id":format!("{name}-{n}"), "path":path, "role":"identity_reference", "status":"present", "character_id":name})).unwrap());
        }
    }
    fs::write(root.join("Preproduction/workflow_state.json"), state.to_json().unwrap()).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
    test(&root, &app, &recorder);
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
}

#[test]
fn cr04_saved_profile_acquisition_discovery_selection_and_reload_use_native_boundaries() {
    selection_project(|root, app, recorder| {
        let imported = dispatch(app, recorder, "workflow_save_character", &json!({"projectRoot":root, "characterId":"alice", "name":"Alice", "profile":{"description":"Keep me"}})).unwrap();
        let alice_id = imported["state"]["characters"].as_array().unwrap().iter().find(|c| c["name"] == "Alice").unwrap()["id"].clone();
        let bob = imported["state"]["characters"].as_array().unwrap().iter().find(|c| c["name"] == "Bob").unwrap().clone();
        let source = root.join("portrait.png");
        image(&source);
        let receipt = dispatch(app, recorder, "ingest_character_reference_image", &json!({"projectRoot":root,"characterDir":"Characters/Alice","sourcePath":source,"receipt":true})).unwrap();
        let imported = dispatch(app, recorder, "workflow_import", &json!({"projectRoot":root,"persist":true})).unwrap();
        let asset = imported["state"]["assets"].as_array().unwrap().iter().find(|a| a["path"] == receipt["path"])
            .expect("CR01 copy must be discovered for CR02 without a model-sheet manifest");
        let selected = dispatch(app, recorder, "workflow_save_character_reference_selection", &json!({"projectRoot":root,"characterId":alice_id,"assetIds":[asset["id"]]})).unwrap();
        assert_eq!(selected["reference_asset_ids"], json!([asset["id"]]));
        assert_eq!(selected["identity_ready"], false);
        let reloaded = dispatch(app, recorder, "workflow_import", &json!({"projectRoot":root,"persist":true})).unwrap();
        let people = reloaded["state"]["characters"].as_array().unwrap();
        assert_eq!(people.iter().find(|c| c["name"] == "Alice").unwrap()["reference_asset_ids"], selected["reference_asset_ids"]);
        assert_eq!(people.iter().find(|c| c["name"] == "Bob").unwrap(), &bob);
        assert!(!root.join("Characters/Alice/model_sheet_manifest.yaml").exists());
        // Removal is selection-only; acquisition cleanup reuses project trash.
        dispatch(app, recorder, "workflow_save_character_reference_selection", &json!({"projectRoot":root,"characterId":alice_id,"assetIds":[]})).unwrap();
        assert!(root.join(receipt["path"].as_str().unwrap()).is_file());
        let failing_source = root.join("failed.png");
        image(&failing_source);
        let failed_receipt = dispatch(app, recorder, "ingest_character_reference_image", &json!({"projectRoot":root,"characterDir":"Characters/Alice","sourcePath":failing_source,"receipt":true})).unwrap();
        assert_eq!(failed_receipt["created"], true);
        dispatch(app, recorder, "workflow_import", &json!({"projectRoot":root,"persist":true})).unwrap();
        assert!(dispatch(app, recorder, "workflow_save_character_reference_selection", &json!({"projectRoot":root,"characterId":alice_id,"assetIds":["invalid"]})).is_err());
        dispatch(app, recorder, "delete_path", &json!({"path":root.join(failed_receipt["path"].as_str().unwrap())})).unwrap();
        let cleaned = dispatch(app, recorder, "workflow_import", &json!({"projectRoot":root,"persist":true})).unwrap();
        assert!(!root.join(failed_receipt["path"].as_str().unwrap()).exists());
        assert!(cleaned["state"]["assets"].as_array().unwrap().iter().all(|a| a["path"] != failed_receipt["path"]));
        assert!(root.join(receipt["path"].as_str().unwrap()).is_file());
    });
}

#[test]
fn cr02_invalid_missing_wrong_character_and_malformed_requests_preserve_prior_bytes() {
    selection_project(|root, app, recorder| {
        let path = root.join("Preproduction/workflow_state.json");
        let before = fs::read(&path).unwrap();
        for ids in [json!(["not-an-asset"]), json!(["bob-1"]), json!([""]), json!(["alice-1", "alice-1"]), json!([1]), json!(null)] {
            let error = select_refs(app, recorder, root, ids).unwrap_err();
            assert!(error.starts_with("associationError:"), "{error}");
            assert_eq!(fs::read(&path).unwrap(), before);
        }
        fs::remove_file(root.join("Characters/Alice/refs/2.png")).unwrap();
        assert!(select_refs(app, recorder, root, json!(["alice-2"])).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(dispatch(app, recorder, "workflow_save_character_reference_selection", &json!({"projectRoot":root,"characterId":"missing","assetIds":[]})).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        // A valid ID with a non-image role must not become a reference.
        let mut state: Value = serde_json::from_slice(&before).unwrap();
        state["assets"][0]["role"] = json!("voice_sample");
        let changed = serde_json::to_vec_pretty(&state).unwrap();
        fs::write(&path, &changed).unwrap();
        assert!(select_refs(app, recorder, root, json!(["alice-1"])).is_err());
        assert_eq!(fs::read(&path).unwrap(), changed);
    });
}

#[cfg(unix)]
#[test]
fn cr02_redirected_reference_is_rejected_without_modifying_source_or_other_character() {
    use std::os::unix::fs::symlink;
    selection_project(|root, app, recorder| {
        let path = root.join("Preproduction/workflow_state.json");
        let before = fs::read(&path).unwrap();
        let source = root.parent().unwrap().join("external.png");
        let external = image(&source);
        let bob = fs::read(root.join("Characters/Bob/refs/1.png")).unwrap();
        let reference = root.join("Characters/Alice/refs/2.png");
        fs::remove_file(&reference).unwrap();
        symlink(&source, &reference).unwrap();
        assert!(select_refs(app, recorder, root, json!(["alice-2"])).is_err());
        fs::remove_file(&reference).unwrap();
        symlink(root.join("Characters/Bob/refs/1.png"), &reference).unwrap();
        assert!(select_refs(app, recorder, root, json!(["alice-2"])).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        select_refs(app, recorder, root, json!([])).unwrap();
        assert_eq!(fs::read(&source).unwrap(), external);
        assert_eq!(fs::read(root.join("Characters/Bob/refs/1.png")).unwrap(), bob);
    });
}

#[cfg(unix)]
#[test]
fn cr02_failed_persistence_preserves_prior_selection() {
    use std::os::unix::{fs::PermissionsExt, process::CommandExt};
    selection_project(|root, app, recorder| {
        let path = root.join("Preproduction/workflow_state.json");
        select_refs(app, recorder, root, json!(["alice-1"])).unwrap();
        let before = fs::read(&path).unwrap();
        let parent = path.parent().unwrap();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o555)).unwrap();
        if fs::write(parent.join("permission-probe"), b"probe").is_ok() {
            fs::remove_file(parent.join("permission-probe")).unwrap();
            fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(std::env::var_os("CR02_UNPRIVILEGED").is_none());
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "cr02_failed_persistence_preserves_prior_selection", "--nocapture"])
                .env("CR02_UNPRIVILEGED", "1").uid(65534).gid(65534).output().unwrap();
            assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            return;
        }
        let result = select_refs(app, recorder, root, json!(["alice-2"]));
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
        let error = result.unwrap_err();
        assert!(error.contains("Permission denied"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), before);
        let loaded = dispatch(app, recorder, "workflow_status", &json!({"projectRoot":root})).unwrap();
        assert_eq!(loaded["state"]["characters"][0]["reference_asset_ids"], json!(["alice-1"]));
    });
}

fn select_refs(app: &HostCtx, recorder: &RecorderState, root: &Path, ids: Value) -> Result<Value, String> {
    dispatch(app, recorder, "workflow_save_character_reference_selection", &json!({"projectRoot":root,"characterId":"alice","assetIds":ids}))
}

#[test]
fn cr02_selection_round_trip_add_remove_and_character_isolation() {
    selection_project(|root, app, recorder| {
        let state_path = root.join("Preproduction/workflow_state.json");
        let before: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        let updated = select_refs(app, recorder, root, json!(["alice-2", "alice-1"])).unwrap();
        assert_eq!(updated["reference_asset_ids"], json!(["alice-2", "alice-1"]));
        let loaded = dispatch(app, recorder, "workflow_status", &json!({"projectRoot":root})).unwrap();
        assert_eq!(loaded["state"]["characters"][0]["reference_asset_ids"], updated["reference_asset_ids"]);
        assert_eq!(loaded["state"]["characters"][0]["profile"], before["characters"][0]["profile"]);
        assert_eq!(loaded["state"]["characters"][0]["identity_ready"], false);
        assert_eq!(loaded["state"]["characters"][1], before["characters"][1]);
        let removed = select_refs(app, recorder, root, json!([])).unwrap();
        assert_eq!(removed["reference_asset_ids"], json!([]));
        let loaded = dispatch(app, recorder, "workflow_status", &json!({"projectRoot":root})).unwrap();
        assert_eq!(loaded["state"]["characters"][0]["reference_asset_ids"], json!([]));
        assert_eq!(loaded["state"]["assets"], before["assets"]);
        for name in ["Alice", "Bob"] { for n in [1, 2] {
            assert!(root.join(format!("Characters/{name}/refs/{n}.png")).is_file());
        }}
    });
}

#[test]
fn saved_character_reference_delete_trashes_exact_copy_and_removes_only_its_graph_edges() {
    selection_project(|root, app, recorder| {
        let state_path = root.join("Preproduction/workflow_state.json");
        let mut state = workbench_host::workflow::state::ProjectState::from_json(
            &fs::read_to_string(&state_path).unwrap(),
        )
        .unwrap();
        state.characters[0].reference_asset_ids = vec!["alice-1".into(), "alice-2".into()];
        for asset in &mut state.assets {
            asset.notes = Some("uploaded reference image".into());
        }
        let mut job = workbench_host::workflow::state::JobRecord::open("reference-job", "fixture");
        job.input_asset_ids = vec!["alice-1".into(), "alice-2".into(), "bob-1".into()];
        job.output_asset_ids = job.input_asset_ids.clone();
        state.jobs.push(job);
        fs::write(&state_path, state.to_json().unwrap()).unwrap();

        let target = root.join("Characters/Alice/refs/1.png");
        let survivor = root.join("Characters/Alice/refs/2.png");
        let unrelated = root.join("Characters/Bob/refs/1.png");
        let target_bytes = fs::read(&target).unwrap();
        let survivor_bytes = fs::read(&survivor).unwrap();
        let unrelated_bytes = fs::read(&unrelated).unwrap();

        let returned = dispatch(
            app,
            recorder,
            "workflow_delete_character_reference",
            &json!({"projectRoot":root,"characterId":"alice","assetId":"alice-1"}),
        )
        .unwrap();

        assert!(!target.exists());
        assert_eq!(fs::read(&survivor).unwrap(), survivor_bytes);
        assert_eq!(fs::read(&unrelated).unwrap(), unrelated_bytes);
        let trash: Vec<_> = fs::read_dir(root.join(".workbench-trash"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(trash.len(), 1);
        assert_eq!(fs::read(&trash[0]).unwrap(), target_bytes);

        for snapshot in [
            returned["state"].clone(),
            serde_json::from_slice::<Value>(&fs::read(&state_path).unwrap()).unwrap(),
        ] {
            assert_eq!(
                snapshot["assets"].as_array().unwrap().iter().map(|asset| asset["id"].as_str().unwrap()).collect::<Vec<_>>(),
                vec!["alice-2", "bob-1", "bob-2"],
            );
            assert_eq!(snapshot["characters"][0]["asset_ids"], json!(["alice-2"]));
            assert_eq!(snapshot["characters"][0]["reference_asset_ids"], json!(["alice-2"]));
            assert_eq!(snapshot["jobs"][0]["input_asset_ids"], json!(["alice-2", "bob-1"]));
            assert_eq!(snapshot["jobs"][0]["output_asset_ids"], json!(["alice-2", "bob-1"]));
        }
    });
}

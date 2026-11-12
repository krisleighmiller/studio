use serde_json::json;
use std::{fs, path::Path};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import::{load_state, persist_state}, state::ProjectState}};

#[test]
fn photo_trash_retains_exact_recoverable_bytes_and_other_photo() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("Characters/Alice/model_sheet")).unwrap();
    fs::write(root.join("Characters/Alice/character.yaml"), "name: Alice\n").unwrap();
    let inputs = json!({"output_path":"Characters/Alice/model_sheet", "character_files":["Characters/Alice/character.yaml"], "character_slug":"Alice", "identity_text":"Alice", "reference_images":[]});
    workbench_host::specialists::prepare_model_sheet_manifest(inputs.as_object().unwrap(), None, root.to_str()).unwrap();
    let a = "Characters/Alice/model_sheet/alice_standing_front.png";
    let b = "Characters/Alice/model_sheet/alice_standing_back.png";
    let output = std::process::Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "color=red:s=16x16", "-frames:v", "1", "-threads", "1", "-y"]).arg(root.join(a)).output().unwrap();
    assert!(output.status.success());
    fs::copy(root.join(a), root.join(b)).unwrap();
    let bytes = fs::read(root.join(a)).unwrap();
    let manifest_path = root.join("Characters/Alice/model_sheet/model_sheet_manifest.yaml");
    let mut manifest: serde_json::Value = serde_yaml_ng::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    for shot in manifest["shots"].as_array_mut().unwrap() {
        if ["standing_front", "standing_back"].contains(&shot["id"].as_str().unwrap()) {
            shot["status"] = json!("saved");
        }
    }
    fs::write(&manifest_path, serde_yaml_ng::to_string(&manifest).unwrap()).unwrap();
    let mut state = ProjectState::default();
    state.profile = Some(Default::default());
    state.characters = serde_json::from_value(json!([{"id":"alice", "name":"Alice", "dir":"Characters/Alice", "profile_path":"Characters/Alice/character.yaml", "model_sheet_manifest_path":"Characters/Alice/model_sheet/model_sheet_manifest.yaml", "identity_ready":true, "voice_ready":false, "asset_ids":["a", "b"]}])).unwrap();
    state.assets = serde_json::from_value(json!([
        {"id":"a", "path":a, "role":"body_reference", "status":"present", "character_id":"alice", "notes":"standing_front"},
        {"id":"b", "path":b, "role":"body_reference", "status":"present", "character_id":"alice", "notes":"standing_back"}
    ])).unwrap();
    persist_state(root, &state).unwrap();
    let app = HostCtx::new(root.join("settings"));
    let recorder = RecorderState::new();
    dispatch(&app, &recorder, "open_project", &json!({"path":root})).unwrap();
    // A real refusal does not remove or rewrite either synthetic photo.
    let refused = dispatch(&app, &recorder, "workflow_delete_model_sheet_photo", &json!({"projectRoot":root,"characterId":"wrong","assetId":"a"})).unwrap_err();
    assert_eq!(fs::read(root.join(a)).unwrap(), bytes);
    let removed = dispatch(&app, &recorder, "workflow_delete_model_sheet_photo", &json!({"projectRoot":root,"characterId":"alice","assetId":"a"})).unwrap();
    assert!(!root.join(a).exists());
    assert_eq!(fs::read(root.join(b)).unwrap(), bytes);
    let trash = fs::read_dir(root.join(".workbench-trash")).unwrap().map(|p| p.unwrap().path()).find(|p| p.file_name().unwrap().to_string_lossy().ends_with("alice_standing_front.png")).unwrap();
    assert_eq!(fs::read(&trash).unwrap(), bytes);
    // Actual recovery readback, without reintroducing it into the active gallery.
    fs::copy(&trash, root.join("recovered.png")).unwrap();
    assert_eq!(fs::read(root.join("recovered.png")).unwrap(), bytes);
    let persisted = load_state(root).unwrap();
    assert!(!persisted.assets.iter().any(|asset| asset.path == a));
    assert!(persisted.assets.iter().any(|asset| asset.path == b));
    dispatch(&app, &recorder, "delete_path", &json!({"path":root.join(b)})).unwrap();
    let refreshed = dispatch(&app, &recorder, "workflow_import", &json!({"projectRoot":root})).unwrap();
    assert!(!root.join(b).exists());
    assert!(refreshed["state"]["assets"].as_array().unwrap().iter().all(|asset| asset["path"] != b));
    let trash_b = fs::read_dir(root.join(".workbench-trash")).unwrap().map(|p| p.unwrap().path()).find(|p| p.file_name().unwrap().to_string_lossy().ends_with("alice_standing_back.png")).unwrap();
    assert_eq!(fs::read(&trash_b).unwrap(), bytes);
    if let Ok(path) = std::env::var("SB17_NATIVE_RECEIPT") {
        fs::write(path, serde_json::to_vec_pretty(&json!({"refusal":refused,"gallery_result":removed,"detached_result":refreshed,"recoverable_bytes":bytes.len(),"recovered_exact":true,"survivor_unchanged":true,"both_trashed_exact":true})).unwrap()).unwrap();
    }
    dispatch(&app, &recorder, "close_project", &json!({})).unwrap();
    assert!(Path::new(&trash).exists());
}

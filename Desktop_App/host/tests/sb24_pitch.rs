//! Focused offline pitch regression through the production command handler.
use serde_json::json;
use std::{fs, path::Path, process::Command};
use workbench_host::{session, workflow::{audio_edit, import, performance, state::{AssetRecord, AssetRole, AssetStatus, ProjectState}}};
fn read(root: &Path) -> ProjectState {
    ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap()
}
fn measure(path: &Path) -> (f64, f64) {
    let raw = Command::new("ffmpeg").args(["-v", "error", "-xerror", "-err_detect", "explode", "-i"]).arg(path)
        .args(["-f", "s16le", "-ac", "1", "-ar", "48000", "pipe:1"]).output().unwrap();
    assert!(raw.status.success(), "strict full decode: {}", String::from_utf8_lossy(&raw.stderr));
    let samples: Vec<i16> = raw.stdout.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    let hz = samples[24000..72000].windows(2).filter(|s| s[0] <= 0 && s[1] > 0).count() as f64;
    (samples.len() as f64 / 48000.0, hz)
}
#[test]
fn independent_pitch_preserves_duration_history_and_explicit_selection() {
    let dir = tempfile::tempdir().unwrap(); let root = dir.path();
    session::open(root.to_str().unwrap()).unwrap();
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    let rel = "Scenes/Scene1/Cut1/performance.wav";
    let src = root.join(rel);
    assert!(Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=4", "-c:a", "pcm_s16le"]).arg(&src).status().unwrap().success());
    let original = fs::read(&src).unwrap();
    let mut state = ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Pitch fixture","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1"}]}]}).to_string()).unwrap();
    let source_id = state.push_asset(AssetRecord { id:String::new(),path:rel.into(),role:AssetRole::SourceAudio,status:AssetStatus::Draft,scene_id:Some(1),cut_id:Some(workbench_host::scene_plan::handoff::CutId::new(1,1)),character_id:None,source_job_id:None,parent_asset_id:None,notes:None });
    import::persist_state(root, &state).unwrap();
    let prior = audio_edit::version_speed(root, rel, 1.25).unwrap();
    let prior_bytes = fs::read(root.join(&prior.path)).unwrap();
    let before = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    let request = |pitch: serde_json::Value| json!({"projectRoot":root,"sceneId":1,"cut":1,"assetId":source_id,"pitch":pitch});
    for pitch in [json!(0),json!(-1),json!(0.49),json!(2.01),json!("NaN"),serde_json::Value::Null] {
        assert!(performance::version(&request(pitch)).is_err());
        assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before);
    }
    let mut ambiguous=request(json!(1.5));ambiguous["speed"]=json!(1.25);
    assert!(performance::version(&ambiguous).is_err(), "must not conflate pitch and speed");
    let outside=tempfile::tempdir().unwrap();let external=outside.path().join("external.wav");fs::write(&external,&original).unwrap();
    assert!(audio_edit::version_pitch(root, external.to_str().unwrap(),1.5).is_err());
    #[cfg(unix)] {
        std::os::unix::fs::symlink(&external, root.join("Scenes/Scene1/Cut1/link.wav")).unwrap();
        assert!(audio_edit::version_pitch(root,"Scenes/Scene1/Cut1/link.wav",1.5).is_err());
        std::os::unix::fs::symlink(&external,root.join("Scenes/Scene1/Cut1/performance_pitch_v2.wav")).unwrap();
    }
    fs::write(root.join("Scenes/Scene1/Cut1/performance_pitch_v3.wav"),b"occupied").unwrap();
    let response = performance::version(&request(json!(1.5))).expect("pitch-only command must succeed");
    assert!(response["note"].as_str().unwrap().contains("pitch"));
    let saved=read(root);let edited=saved.assets.iter().find(|a|a.path.ends_with("_pitch_v4.wav")).expect("occupied/symlink paths skipped, never overwritten");
    let bytes=fs::read(root.join(&edited.path)).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, bytes.len()-8);
    let notes:serde_json::Value=serde_json::from_str(edited.notes.as_deref().unwrap()).unwrap();
    assert_eq!(notes["operation"],"local_wav_pitch");assert_eq!(notes["pitch"],1.5);
    assert_eq!(edited.parent_asset_id.as_deref(),Some(source_id.as_str()));assert_eq!(edited.status,AssetStatus::Draft);
    assert!(saved.versions.iter().any(|v|v.asset_id==edited.id && v.path==edited.path));
    assert!(saved.scenes[0].cuts[0].selected_performance_asset_id.is_none());
    let (duration,hz)=measure(&root.join(&edited.path));
    assert!((duration-4.0).abs()<0.1,"expected ~4s, actual {duration}");assert!((hz-660.0).abs()<3.0,"expected 660Hz, actual {hz}");
    let low=audio_edit::version_pitch(root,rel,0.5).unwrap();let (low_duration,low_hz)=measure(&root.join(&low.path));
    assert!((low_duration-4.0).abs()<0.1);assert!((low_hz-220.0).abs()<3.0);
    let high=audio_edit::version_pitch(root,rel,2.0).unwrap();let (high_duration,high_hz)=measure(&root.join(&high.path));
    assert!((high_duration-4.0).abs()<0.1);assert!((high_hz-880.0).abs()<3.0);
    for bad in [f64::NAN,f64::INFINITY] {assert!(audio_edit::version_pitch(root,rel,bad).is_err());}
    assert_eq!(fs::read(&src).unwrap(),original);assert_eq!(fs::read(root.join(&prior.path)).unwrap(),prior_bytes);
    assert_eq!(fs::read(root.join(&edited.path)).unwrap(),bytes);assert_eq!(fs::read(&external).unwrap(),original);
    assert_eq!(fs::read(root.join("Scenes/Scene1/Cut1/performance_pitch_v3.wav")).unwrap(),b"occupied");
    let imported=import::import_project(root,Default::default()).unwrap().state;
    assert_eq!(imported.asset(&edited.id),Some(edited));assert_eq!(imported.assets.iter().filter(|a|a.path==edited.path).count(),1);
    assert_eq!(imported.jobs,state.jobs);assert_eq!(imported.spend_confirmations,state.spend_confirmations);
    println!("SB24 pitch: 1.5x={duration:.6}s/{hz:.2}Hz; 0.5x={low_duration:.6}s/{low_hz:.2}Hz; 2x={high_duration:.6}s/{high_hz:.2}Hz; source/prior unchanged; finite RIFF strict decode; explicit selection retained; invalid/ambiguous/external/symlink refused; occupied outputs preserved; lineage reimported; zero provider calls");
    if let Ok(dest)=std::env::var("SB24_PITCH_ARTIFACTS") {let dest=Path::new(&dest);fs::create_dir_all(dest).unwrap();fs::write(dest.join("source.wav"),original).unwrap();fs::write(dest.join("pitch-1.5.wav"),bytes).unwrap();fs::write(dest.join("native-state.json"),imported.to_json().unwrap()).unwrap();}
    session::close();
}

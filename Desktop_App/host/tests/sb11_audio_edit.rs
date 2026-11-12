//! Offline production-library WAV editing. No provider or video dispatch.
use std::{fs, path::Path, process::Command};
use serde_json::json;
use workbench_host::{session, workflow::{audio_edit, import, state::{AssetStatus, ProjectState}}};
fn run(args: &[&str]) { assert!(Command::new("ffmpeg").args(args).status().unwrap().success()); }
fn read(root: &Path) -> ProjectState { ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap() }
fn measure(path: &Path) -> (f64,f64) {
    let raw=Command::new("ffmpeg").args(["-v","error","-i"]).arg(path).args(["-f","s16le","-ac","1","-ar","48000","pipe:1"]).output().unwrap();
    assert!(raw.status.success(),"derivative must fully decode");
    let samples: Vec<i16>=raw.stdout.chunks_exact(2).map(|b|i16::from_le_bytes([b[0],b[1]])).collect();
    let middle=&samples[24000..72000];
    let hz=middle.windows(2).filter(|s| s[0]<=0 && s[1]>0).count() as f64;
    (samples.len() as f64/48000.0,hz)
}
#[test]
fn local_speed_edit_versions_without_overwriting_or_dispatch() {
    let dir=tempfile::tempdir().unwrap(); let root=dir.path();session::open(root.to_str().unwrap()).unwrap();
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    let rel="Scenes/Scene1/Cut1/performance.wav"; let src=root.join(rel);
    run(&["-v","error","-f","lavfi","-i","sine=frequency=440:sample_rate=48000:duration=4","-c:a","pcm_s16le",src.to_str().unwrap()]);
    let state=ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Audio edit fixture","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1"}]}]}).to_string()).unwrap();
    let mut state=state;
    state.jobs.push(workbench_host::workflow::state::JobRecord::open("historical_job", "render"));
    import::persist_state(root,&state).unwrap(); let original=fs::read(&src).unwrap();
    let before=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    for speed in [0.0,-1.0,0.49,2.01,f64::NAN,f64::INFINITY] {
        assert!(audio_edit::version_speed(root,rel,speed).is_err());
        assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before);
    }
    let outside=tempfile::tempdir().unwrap(); let external=outside.path().join("external.wav");fs::write(&external,&original).unwrap();
    assert!(audio_edit::version_speed(root,external.to_str().unwrap(),1.25).is_err());
    #[cfg(unix)] {
        std::os::unix::fs::symlink(&external,root.join("Scenes/Scene1/Cut1/link.wav")).unwrap();
        assert!(audio_edit::version_speed(root,"Scenes/Scene1/Cut1/link.wav",1.25).is_err());
    }
    assert_eq!(fs::read(&external).unwrap(),original);
    let malformed=root.join("not-audio.wav");fs::write(&malformed,b"#EXTM3U\nhttp://127.0.0.1:9/no-network\n").unwrap();
    assert!(audio_edit::version_speed(root,"not-audio.wav",1.25).is_err());
    fs::write(&src,&original[..original.len()/2]).unwrap();
    assert!(audio_edit::version_speed(root,rel,1.25).is_err(),"truncated WAV must refuse, not silently shorten");
    fs::write(&src,&original).unwrap();
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before);
    let occupied=root.join("Scenes/Scene1/Cut1/performance_speed_v2.wav");fs::write(&occupied,b"reserved original bytes").unwrap();
    let edited=audio_edit::version_speed(root,rel,1.25).expect("valid local speed edit must create a derivative");
    assert_eq!(edited.path,"Scenes/Scene1/Cut1/performance_speed_v3.wav");
    assert_eq!(fs::read(&occupied).unwrap(),b"reserved original bytes");
    let first=fs::read(root.join(&edited.path)).unwrap();
    let (duration,pitch)=measure(&root.join(&edited.path));
    assert!((duration-3.2).abs()<0.08,"expected ~3.2s, actual {duration}");
    assert!((pitch-440.0).abs()<2.0,"preserve 440Hz, actual {pitch}");
    assert_eq!(edited.status,AssetStatus::Draft); assert!(edited.parent_asset_id.is_some());
    let saved=read(root); assert!(saved.versions.iter().any(|v|v.asset_id==edited.id && v.path==edited.path));
    assert_eq!(saved.jobs,state.jobs); assert_eq!(saved.spend_confirmations,state.spend_confirmations);
    let slower=audio_edit::version_speed(root,rel,0.8).unwrap(); assert_ne!(slower.path,edited.path);
    let (slow_duration,slow_pitch)=measure(&root.join(&slower.path));
    assert!((slow_duration-5.0).abs()<0.08);assert!((slow_pitch-440.0).abs()<2.0);
    assert_eq!(fs::read(root.join(&edited.path)).unwrap(),first);assert_eq!(fs::read(&src).unwrap(),original);
    let imported=import::import_project(root,Default::default()).unwrap().state;
    let retained=imported.asset(&edited.id).expect("derived WAV survives real reimport");
    assert_eq!(retained,&edited); assert!(imported.asset(edited.parent_asset_id.as_deref().unwrap()).is_some());
    assert_eq!(imported.assets.iter().filter(|a|a.path==edited.path).count(),1,"one identity per WAV, no generic take duplicate");
    let imported=import::import_project(root,Default::default()).unwrap().state;
    assert_eq!(imported.asset(&edited.id),Some(&edited));
    assert_eq!(imported.assets.iter().filter(|a|a.path==edited.path).count(),1);
    let wrong=tempfile::tempdir().unwrap();assert!(audio_edit::version_speed(wrong.path(),rel,1.25).is_err());
    println!("SB11 expected=actual: fast duration={duration:.6}s pitch={pitch:.2}Hz; slow duration={slow_duration:.6}s pitch={slow_pitch:.2}Hz; original/prior derivative identical; draft lineage/version survives import; invalid speed/external/symlink/wrong-root refused; jobs/permissions unchanged; zero provider/video dispatch.");
    if let Ok(dest)=std::env::var("SB11_ARTIFACTS") { let dest=Path::new(&dest);fs::create_dir_all(dest).unwrap();fs::write(dest.join("source.wav"),original).unwrap();fs::write(dest.join("speed-1.25.wav"),first).unwrap();fs::copy(root.join(&slower.path),dest.join("speed-0.8.wav")).unwrap();fs::write(dest.join("native-state.json"),imported.to_json().unwrap()).unwrap(); }
    session::close();
}

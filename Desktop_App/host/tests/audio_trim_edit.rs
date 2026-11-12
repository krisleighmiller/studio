//! Sample-exact offline trim on a disposable shot; no provider/video dispatch.
use serde_json::{json, Value};
use std::{fs, process::Command};
use workbench_host::{session, workflow::{audio_edit, import, performance, state::{AssetRecord, AssetRole, AssetStatus, ProjectState}}};

#[test]
fn trim_source_window_is_a_draft_and_survives_reopen() {
    let dir=tempfile::tempdir().unwrap(); let root=dir.path();
    session::open(root.to_str().unwrap()).unwrap();
    let rel="Scenes/Scene1/Cut1/source.wav";
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","sine=frequency=440:sample_rate=48000:duration=4","-c:a","pcm_s16le"]).arg(root.join(rel)).status().unwrap().success());
    let original=fs::read(root.join(rel)).unwrap();
    let mut state=ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Trim","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1"}]}]}).to_string()).unwrap();
    let id=state.push_asset(AssetRecord{id:String::new(),path:rel.into(),role:AssetRole::SourceAudio,status:AssetStatus::Draft,scene_id:Some(1),cut_id:Some(workbench_host::scene_plan::handoff::CutId::new(1,1)),character_id:None,source_job_id:None,parent_asset_id:None,notes:None});
    import::persist_state(root,&state).unwrap();
    let request=|start:Value,end:Value|json!({"projectRoot":root,"sceneId":1,"cut":1,"assetId":id,"trim":{"startSeconds":start,"endSeconds":end}});
    let before=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    for (start,end) in [(json!(-1),json!(2)),(json!(0),json!(4.1)),(json!(2),json!(2)),(json!(3),json!(2)),(json!("bad"),json!(2)),(json!(0),json!(null))] {
        assert!(performance::version(&request(start,end)).is_err());
    }
    let mut ambiguous=request(json!(0),json!(2));ambiguous["speed"]=json!(1.25);
    assert!(performance::version(&ambiguous).is_err());
    let outside=tempfile::tempdir().unwrap();let external=outside.path().join("outside.wav");fs::write(&external,&original).unwrap();
    let mut escape=request(json!(1),json!(2));escape["assetId"]=json!("missing");
    assert!(performance::version(&escape).is_err());
    assert!(audio_edit::version_trim(root,external.to_str().unwrap(),0.5,2.5).is_err());
    #[cfg(unix)] {
        std::os::unix::fs::symlink(&external,root.join("Scenes/Scene1/Cut1/redirect.wav")).unwrap();
        assert!(audio_edit::version_trim(root,"Scenes/Scene1/Cut1/redirect.wav",0.5,2.5).is_err());
    }
    assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before);
    let result=performance::version(&request(json!(0.5),json!(2.5))).unwrap();
    assert!(result["note"].as_str().unwrap().contains("select it explicitly"));
    let saved=ProjectState::from_json(&fs::read_to_string(root.join("Preproduction/workflow_state.json")).unwrap()).unwrap();
    let edited=saved.assets.iter().find(|a|a.path.ends_with("_trim_v2.wav")).unwrap();
    assert_eq!(edited.parent_asset_id.as_deref(),Some(id.as_str()));assert_eq!(edited.status,AssetStatus::Draft);
    let recipe:Value=serde_json::from_str(edited.notes.as_deref().unwrap()).unwrap();
    assert_eq!(recipe["source_window"],json!({"start_seconds":0.5,"end_seconds":2.5,"start_sample":24000,"end_sample_exclusive":120000,"sample_rate":48000}));
    assert_eq!(recipe["speech_completeness_verified"],false);
    let output=root.join(&edited.path);
    let raw=Command::new("ffprobe").args(["-v","error","-show_entries","format=duration","-of","default=nw=1:nk=1"]).arg(&output).output().unwrap();
    assert!(raw.status.success());assert_eq!(String::from_utf8_lossy(&raw.stdout).trim(),"2.000000");
    assert!(Command::new("ffmpeg").args(["-v","error","-xerror","-i"]).arg(&output).args(["-f","null","-"]).status().unwrap().success());
    assert_eq!(fs::read(root.join(rel)).unwrap(),original);
    assert_eq!(saved.scenes[0].cuts[0].selected_performance_asset_id,None);
    assert_eq!(saved.jobs,state.jobs);assert_eq!(saved.spend_confirmations,state.spend_confirmations);
    assert!(saved.versions.iter().any(|v|v.asset_id==edited.id && v.path==edited.path));
    let reopened=import::import_project(root,Default::default()).unwrap().state;
    assert_eq!(reopened.asset(&edited.id),Some(edited));assert_eq!(reopened.assets.iter().filter(|a|a.path==edited.path).count(),1);
    assert_eq!(fs::read(&external).unwrap(),original);
    println!("Audio trim: exact [24000,120000) samples at 48000Hz, 2.000000s decode; source intact; draft lineage persists; no selection, jobs or permissions changed; invalid and ambiguous requests refused");
    session::close();
}

#[test]
fn disposable_spoken_wav_trim() {
    let Ok(spoken)=std::env::var("A2_SPOKEN_WAV") else { return; };
    let dir=tempfile::tempdir().unwrap();let root=dir.path();session::open(root.to_str().unwrap()).unwrap();
    let rel="Scenes/Scene1/Cut1/spoken.wav";fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();
    fs::copy(&spoken,root.join(rel)).unwrap();let original=fs::read(root.join(rel)).unwrap();
    let mut state=ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Spoken edit","cuts":[{"id":{"source_scene":1,"cut":1},"stable_id":"shot_1"}]}]}).to_string()).unwrap();
    let id=state.push_asset(AssetRecord{id:String::new(),path:rel.into(),role:AssetRole::SourceAudio,status:AssetStatus::Draft,scene_id:Some(1),cut_id:Some(workbench_host::scene_plan::handoff::CutId::new(1,1)),character_id:None,source_job_id:None,parent_asset_id:None,notes:None});
    import::persist_state(root,&state).unwrap();
    let response=performance::version(&json!({"projectRoot":root,"sceneId":1,"cut":1,"assetId":id,"trim":{"startSeconds":0.2,"endSeconds":1.2}})).unwrap();
    let edited=response["state"]["assets"].as_array().unwrap().iter().find(|a|a["path"].as_str().unwrap().ends_with("_trim_v2.wav")).unwrap();
    let output=root.join(edited["path"].as_str().unwrap());
    let result=Command::new("ffprobe").args(["-v","error","-show_entries","format=duration","-of","default=nw=1:nk=1"]).arg(&output).output().unwrap();
    assert!(result.status.success());assert_eq!(String::from_utf8_lossy(&result.stdout).trim(),"1.000000");
    assert!(Command::new("ffmpeg").args(["-v","error","-xerror","-i"]).arg(&output).args(["-f","null","-"]).status().unwrap().success());
    assert_eq!(fs::read(root.join(rel)).unwrap(),original);
    assert!(response["state"]["scenes"][0]["cuts"][0]["selected_performance_asset_id"].is_null());
    let reopened=import::import_project(root,Default::default()).unwrap().state;
    assert!(reopened.assets.iter().any(|a|a.path==edited["path"]));
    println!("Spoken WAV local trim: 1.000000s playable decode, source bytes preserved, unselected derivative survives reopen");
    if let Ok(dest)=std::env::var("A2_ARTIFACTS") {let dest=std::path::Path::new(&dest);fs::create_dir_all(dest).unwrap();fs::write(dest.join("source.wav"),original).unwrap();fs::copy(&output,dest.join("trimmed.wav")).unwrap();}
    session::close();
}

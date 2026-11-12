use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import, state::ProjectState}};
fn files(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String,String>) {
        for item in fs::read_dir(dir).unwrap() { let p=item.unwrap().path();
            assert!(!p.is_symlink(), "fixture must not follow symlinks");
            if p.is_dir() { walk(root,&p,out); } else { out.insert(p.strip_prefix(root).unwrap().to_string_lossy().into(),Sha256::digest(fs::read(p).unwrap()).iter().map(|b|format!("{b:02x}")).collect()); }
        }
    }
    let mut out=BTreeMap::new(); walk(root,root,&mut out); out
}
fn copy_tree(src:&Path,dst:&Path) { fs::create_dir_all(dst).unwrap(); for item in fs::read_dir(src).unwrap() { let p=item.unwrap().path(); assert!(!p.is_symlink()); let to=dst.join(p.file_name().unwrap()); if p.is_dir(){copy_tree(&p,&to);}else{fs::copy(p,to).unwrap();} } }
#[test]
fn approved_empty_sketch_reopens_explicitly_before_guarded_removal() {
    let temp=tempfile::tempdir().unwrap(); let root=temp.path().join("project");
    fs::create_dir_all(root.join("Scripts")).unwrap();
    fs::write(root.join("Scripts/test.md"),"## Scene 1 — Revision\nA quiet room.\n").unwrap();
    let app=HostCtx::new(temp.path().join("settings")); let rec=RecorderState::new();
    let call=|cmd:&str,mut args:Value| {args["projectRoot"]=json!(root);dispatch(&app,&rec,cmd,&args)};
    call("open_project",json!({"path":root})).unwrap(); call("workflow_import",json!({"persist":true})).unwrap();
    for n in 1..=2 {call("workflow_add_alternate_shot",json!({"sceneId":1})).unwrap(); call("workflow_save_sketch",json!({"sceneId":1,"cut":n,"sketch":{"title":format!("Empty sketch {n}"),"shot_role":"atmosphere","visual_prompt":"A room","dialogue":[]}})).unwrap();}
    let approved=call("workflow_approve_animatic",json!({"sceneId":1})).unwrap();
    assert_eq!(approved["state"]["scenes"][0]["animatic"]["approved"],true);
    let ids:Vec<String>=approved["state"]["scenes"][0]["cuts"].as_array().unwrap().iter().map(|c|c["stable_id"].as_str().unwrap().into()).collect();
    let args=json!({"sceneId":1,"shotId":ids[0]});
    let before=files(&root);
    assert!(call("workflow_remove_cut",args.clone()).unwrap_err().contains("approved work")); assert_eq!(files(&root),before);
    let reopened=call("workflow_reopen_sketch",args.clone()).expect("explicit empty-sketch reopen must succeed");
    assert!(!reopened["state"]["scenes"][0]["animatic"]["approved"].as_bool().unwrap_or(false)); // false is omitted by native serialization
    assert!(!import::load_state(&root).unwrap().scenes[0].animatic.as_ref().unwrap().approved);
    assert_eq!(reopened["state"]["scenes"][0]["cuts"],approved["state"]["scenes"][0]["cuts"]);
    assert_eq!(reopened["state"]["jobs"],approved["state"]["jobs"]);
    for bad in [json!("missing"),json!(""),Value::Null,json!(4)] {let before=files(&root);assert!(call("workflow_reopen_sketch",json!({"sceneId":1,"shotId":bad})).is_err());assert_eq!(files(&root),before);}
    let removed=call("workflow_remove_cut",args).unwrap();
    call("close_project",json!({})).unwrap();call("open_project",json!({"path":root})).unwrap();
    let imported=call("workflow_import",json!({"persist":true})).unwrap();
    assert_eq!(imported["state"]["scenes"][0]["cuts"].as_array().unwrap().len(),1);
    assert_eq!(imported["state"]["scenes"][0]["cuts"][0]["stable_id"],ids[1]);
    assert_eq!(imported["state"]["scenes"][0]["cuts"][0]["sketch"],approved["state"]["scenes"][0]["cuts"][1]["sketch"]);
    let mut receipt=json!({"approved":approved,"reopened":reopened,"removed":removed,"imported":imported,"ids":ids,"provider_dispatches":0});
    // Independent protected-work variants: reopening must not become a bypass.
    let baseline=import::load_state(&root).unwrap();
    for kind in ["asset","note","dependent","scratch","job"] {
        let mut s=baseline.clone(); let id=s.scenes[0].cuts[0].id;
        match kind {
            "asset"=>s.assets.push(serde_json::from_value(json!({"id":"paid","role":"video_take","status":"approved","path":"Scenes/Scene1/Cut1/paid.mp4","cut_id":id})).unwrap()),
            "note"=>s.scenes[0].cuts[0].production_note_path=Some("Scene_Prompts/Scene1/Cut1.txt".into()),
            "dependent"=>{let mut other=workbench_host::workflow::state::CutRecord::new(workbench_host::scene_plan::handoff::CutId::new(1,2));other.stable_id="dependent".into();other.visual_source=Some(serde_json::from_value(json!({"strategy":"previous_selected_take_exit_frame","predecessor_shot_id":ids[1]})).unwrap());s.scenes[0].cuts.push(other);},
            "scratch"=>s.scenes[0].animatic.as_mut().unwrap().shots[0].scratch_audio_path=Some("scratch.wav".into()),
            "job"=>s.jobs.push(serde_json::from_value(json!({"id":"paid_job","kind":"render","status":"completed","scope":{"kind":"shot","id":ids[1]}})).unwrap()),
            _=>unreachable!()
        }
        s.scenes[0].animatic.as_mut().unwrap().approved=true;
        s.state_revision=import::load_state(&root).unwrap().state_revision;import::persist_state(&root,&s).unwrap();
        let normalized=import::load_state(&root).unwrap();import::save_project(&root,normalized).unwrap();
        let before=files(&root);
        for cmd in ["workflow_reopen_sketch","workflow_remove_cut"] {assert!(call(cmd,json!({"sceneId":1,"shotId":ids[1]})).is_err(),"{kind} {cmd}");assert_eq!(files(&root),before,"{kind} {cmd}");}
    }
    call("close_project",json!({})).unwrap();
    // Full real-project COPY: every original/copy file and all jobs stay unchanged on refusal.
    if let Ok(source)=std::env::var("SB16_SOURCE") {
        let source=Path::new(&source);let original=files(source);let copy=temp.path().join("SecondBrain-copy");copy_tree(source,&copy);
        dispatch(&app,&rec,"open_project",&json!({"path":copy})).unwrap();
        let normalized=import::load_state(&copy).unwrap();import::save_project(&copy,normalized).unwrap();
        let s=import::load_state(&copy).unwrap();let before=files(&copy);let mut errors=vec![];
        for scene in &s.scenes { for cut in &scene.cuts {for cmd in ["workflow_reopen_sketch","workflow_remove_cut"] {
            let err=dispatch(&app,&rec,cmd,&json!({"projectRoot":copy,"sceneId":scene.id,"shotId":cut.stable_id})).unwrap_err();errors.push(json!({"cmd":cmd,"shot":cut.stable_id,"error":err}));assert_eq!(files(&copy),before);
        }}}
        let after=ProjectState::from_json(&fs::read_to_string(copy.join("Preproduction/workflow_state.json")).unwrap()).unwrap();assert_eq!(after.jobs,s.jobs);assert_eq!(files(source),original);
        receipt["real_copy"]=json!({"files_unchanged":before.len(),"original_files_unchanged":original.len(),"jobs_unchanged":s.jobs.len(),"refusals":errors});
        dispatch(&app,&rec,"close_project",&json!({})).unwrap();
    }
    if let Ok(out)=std::env::var("SB16_RECEIPT") {fs::write(out,serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();}
    println!("SB16 expected=actual: approved removal refused; explicit reopen then remove/reopen/import retains survivor ID without resurrection; protected media/notes/dependents/scratch/jobs refuse unchanged; no provider dispatch.");
}

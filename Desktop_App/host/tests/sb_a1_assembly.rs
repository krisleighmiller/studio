//! Connected S07 regression: real preparation/token/ffmpeg, no live planner/media.
use serde_json::{json,Value};
use std::{fs,path::Path,process::Command};
use workbench_host::{ctx::HostCtx,director,pending,session,workflow};
use workflow::{import,state::{ProjectState,JobStatus}};
struct Guard;impl Drop for Guard{fn drop(&mut self){pending::cancel_all();session::close();}}
fn ok(c:&mut Command)->Vec<u8>{let p=c.output().unwrap();assert!(p.status.success(),"{c:?}: {}",String::from_utf8_lossy(&p.stderr));p.stdout}
fn read(r:&Path)->ProjectState{ProjectState::from_json(&fs::read_to_string(r.join("Preproduction/workflow_state.json")).unwrap()).unwrap()}
fn save(r:&Path,s:&ProjectState){import::persist_state(r,s).unwrap();}
fn fixture(r:&Path){
 for d in ["Preproduction","Scripts","Scene_Prompts/Scene1","Scenes/Scene1/Cut1","Scenes/Scene1/Cut2"]{fs::create_dir_all(r.join(d)).unwrap();}
 fs::write(r.join("Scripts/script.md"),"# Scene 1 — S07\n\n- **Action:** Blue and gold.\n").unwrap();
 fs::write(r.join("prod.md"),"### Scene 1\n```yaml\ntransition_in: cut\ntransition_out: cut\nspeech:\n  mode: none\n```\n### Scene 2\n```yaml\ntransition_in: cut\ntransition_out: cut\nspeech:\n  mode: none\n```\n").unwrap();
 for (i,color) in [(1,"blue"),(2,"gold")]{ok(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i",&format!("color=c={color}:s=160x90:r=24:d=2"),"-c:v","libx264","-threads","1","-pix_fmt","yuv420p"]).arg(r.join(format!("Scenes/Scene1/Cut{i}/take.mp4"))));fs::write(r.join(format!("Scene_Prompts/Scene1/Cut{i}.md")),format!("```yaml\nsource_scene: 1\ncut: {i}\ntitle: Shot {i}\nscene_type: b_roll\ncontinuity:\n  group: test\n  relation_to_previous: new_setup\nediting:\n  transition_in: cut\n  transition_out: cut\nspeech:\n  mode: none\nprompt: color {color}\n```\n")).unwrap();}
 let s:ProjectState=serde_json::from_value(json!({"version":2,"project_name":"S07 regression","settings":{"aspect_ratio":"9:16"},"profile":{"recipe":"narrative_dialogue","resolution":"1080x1920","aspect_ratios":["9:16"],"deliverables":[{"id":"review","name":"Review","resolution":"1080x1920","aspect_ratio":"9:16"}]},"assets":([1,2].map(|i|json!({"id":format!("take-{i}"),"path":format!("Scenes/Scene1/Cut{i}/take.mp4"),"role":"video_take","status":"present","scene_id":1,"cut_id":{"source_scene":1,"cut":i}}))),"scenes":[{"id":1,"stable_id":"scene-1","title":"S07","cuts":([1,2].map(|i|json!({"id":{"source_scene":1,"cut":i},"stable_id":format!("shot-{i}"),"source_origin":{"source_scene":1,"cut":i},"production_note_path":format!("Scene_Prompts/Scene1/Cut{i}.md"),"sketch":{"title":format!("Shot{i}"),"shot_role":"insert"},"visual_source":{"strategy":"no_start_frame"},"takes":[{"id":format!("take-{i}"),"path":format!("Scenes/Scene1/Cut{i}/take.mp4"),"status":"selected"}],"selected_take":format!("take-{i}")}))),"animatic":{"id":"anim","approved":true,"shots":([2,1].map(|i|json!({"id":format!("anim-{i}"),"cut_id":{"source_scene":1,"cut":i},"duration_seconds":2,"trim_in_seconds":0.25,"trim_out_seconds":0.25,"transition":if i==2{"dissolve"}else{"cut"}})))}}]})).unwrap();save(r,&s);
}
fn token(v:&Value)->String{v["pendingAction"].as_str().unwrap().split(pending::APPROVE_TOKEN_PREFIX).nth(1).unwrap().split("]]").next().unwrap().into()}
/// SB-01 stops at native preparation: never approve or submit the action.
#[test]
fn sb01_no_submit_canonical_record_and_missing_note() {
 let dir=tempfile::tempdir().unwrap();let root=dir.path().join("project");fs::create_dir(&root).unwrap();fixture(&root);
 fs::remove_file(root.join("prod.md")).unwrap();
 // Normalize host-computed readiness/coverage, not creative timing or IDs.
 import::save_project(&root,read(&root)).unwrap();
 session::open(root.to_str().unwrap()).unwrap();let _g=Guard;
 let before=read(&root);
 let app=HostCtx::new(dir.path().join("settings"));
 let prep=workflow::cmd_prepare_assemble_with_host(&app,&json!({"projectRoot":root,"sceneId":1})).expect("canonical notes must replace absent prod.md");
 let action=pending::current().expect("native no-submit approval");
 let path=action.inputs["production_path"].as_str().unwrap();
 assert_eq!(path,"Final/Scene1_master.production.md");
 let document=fs::read_to_string(root.join(path)).unwrap();
 assert!(document.contains("color blue") && document.contains("color gold"),"{document}");
 assert!(prep["pendingAction"].as_str().unwrap().contains(path),"{prep}");
 let after=read(&root);
 assert_eq!(after.jobs.len(),before.jobs.len()+1);
 assert_eq!(after.jobs.last().unwrap().status,JobStatus::Pending);
 // Preparation recomputes readiness stages; creative inputs stay exact.
 assert_eq!(after.scenes[0].cuts,before.scenes[0].cuts);
 assert_eq!(after.scenes[0].animatic,before.scenes[0].animatic);
 assert!(!root.join("Final/Scene1_master.mp4").exists());
 assert!(!root.join("prod.md").exists());
 println!("SB01 expected=existing exact derived production path; actual={path}; pending job; no master; no dispatch");
 pending::cancel_all();
 // Independent disposable copy with a registered but absent canonical note.
 let missing=dir.path().join("missing-note");fs::create_dir(&missing).unwrap();fixture(&missing);
 fs::remove_file(missing.join("prod.md")).unwrap();fs::remove_file(missing.join("Scene_Prompts/Scene1/Cut2.md")).unwrap();
 session::close();session::open(missing.to_str().unwrap()).unwrap();
 // Materialize supported load-time enrichment before byte-preservation check.
 import::load_state(&missing).unwrap();
 let bytes=fs::read(missing.join("Preproduction/workflow_state.json")).unwrap();
 let error=workflow::cmd_prepare_assemble(&json!({"projectRoot":missing,"sceneId":1})).unwrap_err();
 assert!(error.contains("Scene 1 Cut 2 production_note_path"),"{error}");
 assert!(!error.contains("new still"),"{error}");
 assert!(pending::current().is_none());
 assert_eq!(fs::read(missing.join("Preproduction/workflow_state.json")).unwrap(),bytes);
 assert!(!missing.join("Final/Scene1_master.production.md").exists());
 println!("SB01 expected=note-key-specific refusal before approval; actual={error}");
}

/// SB-02: exact native persisted binding, no operation approval or dispatch.
#[test]
fn sb02_no_animatic_snapshot_once_and_approved_path_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let _g = Guard;
    for has_animatic in [true, false] {
        let root = dir.path().join(if has_animatic { "approved" } else { "no-animatic" });
        fs::create_dir(&root).unwrap();
        fixture(&root);
        fs::remove_file(root.join("prod.md")).unwrap();
        let mut state = read(&root);
        if !has_animatic {
            state.scenes[0].animatic = None;
            // Persisted cut order must not replace cut-number assembly order.
            state.scenes[0].cuts.reverse();
        }
        save(&root, &state);
        if has_animatic {
            // Existing coverage enrichment is not a creative timing change.
            import::save_project(&root, read(&root)).unwrap();
        }
        // Do not normalize missing animatics: save_project synthesizes one.
        session::open(root.to_str().unwrap()).unwrap();
        let before = read(&root);
        let app = HostCtx::new(dir.path().join("settings"));
        let args = json!({"projectRoot":root,"sceneId":1});
        let prep = workflow::cmd_prepare_assemble_with_host(&app, &args).unwrap();
        let action = pending::current().unwrap();
        let after = read(&root);
        assert_eq!(after.jobs.len(), before.jobs.len() + 1);
        let job = after.jobs.last().unwrap();
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.raw_provider_id.as_deref(), Some("scene_1"));
        let scope = job.scope.as_ref().unwrap();
        assert_eq!((&scope.kind[..], &scope.id[..]), ("scene", "scene-1"));
        assert_eq!(job.output_path.as_deref(), action.inputs["output_path"].as_str());
        let order = if has_animatic { [2, 1] } else { [1, 2] };
        let start = if has_animatic { 0.25 } else { 0.0 };
        let duration = if has_animatic { 1.5 } else { 2.0 };
        let expected: Vec<_> = order.iter().map(|i| workflow::state::AssemblyShotSnapshot {
            shot_id: format!("shot-{i}"), cut_label: *i,
            take_id: format!("take-{i}"), path: format!("Scenes/Scene1/Cut{i}/take.mp4"),
            start_seconds: start, duration_seconds: duration,
            transition: if has_animatic && *i == 2 { "dissolve" } else { "cut" }.into(),
        }).collect();
        assert_eq!(job.assembly_snapshot.as_ref(), Some(&expected));
        assert_eq!(job.input_asset_ids, order.map(|i| format!("take-{i}")));
        let entries: Vec<_> = expected.iter().map(|shot| json!({
            "id":1,"cut":shot.cut_label,"video":shot.path,"start":shot.start_seconds,
            "duration":shot.duration_seconds,"transition_out":shot.transition
        })).collect();
        assert_eq!(action.inputs["scenes"], json!(entries));
        assert!(prep["pendingAction"].as_str().unwrap().contains("Cuts: 2"));
        assert_eq!(after.scenes[0].animatic, before.scenes[0].animatic);
        assert_eq!(after.scenes[0].cuts, before.scenes[0].cuts);
        assert!(!root.join(job.output_path.as_ref().unwrap()).exists());
        // Reload exact state, then refuse an unchanged second preparation.
        session::close(); session::open(root.to_str().unwrap()).unwrap();
        assert_eq!(read(&root).jobs.last().unwrap(), job);
        assert_eq!(read(&root).scenes[0].animatic, before.scenes[0].animatic);
        let bytes = fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
        let error = workflow::cmd_prepare_assemble_with_host(&app, &args).unwrap_err();
        assert!(error.contains("unchanged assembly is already pending"), "{error}");
        assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(), bytes);
        println!("SB02 has_animatic={has_animatic}; expected=actual snapshot={}; one Pending job; exact native scenes binding; repeat refused; no dispatch", serde_json::to_string(&expected).unwrap());
        pending::cancel_all(); session::close();
    }
}

/// Explicit opt-in: the runner creates this copy, never pass the original root.
#[test]
#[ignore = "requires SB01_PROJECT_COPY disposable saved project"]
fn sb01_saved_project_copy_no_submit() {
 let root=std::path::PathBuf::from(std::env::var("SB01_PROJECT_COPY").expect("disposable copy path"));
 assert!(root.join(".sb01-disposable").is_file());
 assert!(!root.join("prod.md").exists());
 session::open(root.to_str().unwrap()).unwrap();let _g=Guard;
 let before=import::load_state(&root).unwrap();
 let settings=tempfile::tempdir().unwrap();let app=HostCtx::new(settings.path().join("settings"));
 let prep=workflow::cmd_prepare_assemble_with_host(&app,&json!({"projectRoot":root,"sceneId":1})).unwrap();
 let action=pending::current().unwrap();
 let path=action.inputs["production_path"].as_str().unwrap();
 let production=fs::read_to_string(root.join(path)).unwrap();
 let cut=&before.scene(1).unwrap().cuts[0];
 let canonical=fs::read_to_string(root.join(cut.production_note_path.as_ref().unwrap())).unwrap();
 let handoff=workbench_host::scene_plan::extract::load_handoff_document(&canonical).unwrap();
 let yaml=workbench_host::scene_plan::render::canonical_yaml(&handoff).unwrap();
 assert_eq!(production,format!("{}\n",workbench_host::scene_plan::render::master_block(&handoff,&yaml)));
 assert!(prep["pendingAction"].as_str().unwrap().contains(path));
 let after=read(&root);assert_eq!(after.jobs.len(),before.jobs.len()+1);
 assert_eq!(&after.jobs[..before.jobs.len()],before.jobs.as_slice());
 assert_eq!(after.jobs.last().unwrap().status,JobStatus::Pending);
 assert_eq!(after.scene(1).unwrap().cuts,before.scene(1).unwrap().cuts);
 assert_eq!(after.scene(1).unwrap().animatic,before.scene(1).unwrap().animatic);
 assert!(!root.join(action.inputs["output_path"].as_str().unwrap()).exists());
 println!("SB01 SAVED COPY expected=canonical Scene 1 selected cut metadata; actual={path}; exact typed note content; prior jobs retained; no dispatch/output");
}

#[test]
fn assembly_without_prod_uses_canonical_notes() {
 let dir=tempfile::tempdir().unwrap();let root=dir.path().join("project");fs::create_dir(&root).unwrap();fixture(&root);
 fs::remove_file(root.join("prod.md")).unwrap();
 session::open(root.to_str().unwrap()).unwrap();let _g=Guard;let app=HostCtx::new(dir.path().join("settings"));
 let prep=workflow::cmd_prepare_assemble_with_host(&app,&json!({"projectRoot":root,"sceneId":1})).expect("canonical notes must replace absent prod.md");
 let action=pending::current().unwrap();assert_ne!(action.inputs["production_path"],"prod.md");
 let result=director::director_approve(app,token(&prep)).unwrap();
 assert_eq!(read(&root).jobs.last().unwrap().status,JobStatus::Completed,"{result:?}");
 assert!(!root.join("prod.md").exists());
}

#[test]
fn project_assembly_preserves_order_and_no_animatic_snapshot_once() {
 let dir=tempfile::tempdir().unwrap();let root=dir.path().join("project");fs::create_dir(&root).unwrap();fixture(&root);fs::remove_file(root.join("prod.md")).unwrap();
 let mut state=read(&root);let mut second=state.scenes[0].clone();second.id=3;second.stable_id="scene-3".into();second.animatic=None;
 second.cuts.truncate(1);second.cuts[0].stable_id="third-shot".into();second.cuts[0].id.source_scene=3;second.cuts[0].source_origin=None;
 let path="Scene_Prompts/Scene3/Cut1.md";fs::create_dir_all(root.join("Scene_Prompts/Scene3")).unwrap();fs::write(root.join(path),fs::read_to_string(root.join("Scene_Prompts/Scene1/Cut1.md")).unwrap().replace("source_scene: 1","source_scene: 3")).unwrap();second.cuts[0].production_note_path=Some(path.into());
 state.scenes.push(second);save(&root,&state);
 session::open(root.to_str().unwrap()).unwrap();let _g=Guard;let app=HostCtx::new(dir.path().join("settings"));
 let prep=workflow::cmd_prepare_assemble_with_host(&app,&json!({"projectRoot":root})).expect("project-wide preparation");
 let action=pending::current().unwrap();let entries=action.inputs["scenes"].as_array().unwrap();assert_eq!(entries.len(),3);assert_eq!(entries[0]["cut"],2);assert_eq!(entries[1]["cut"],1);assert_eq!(entries[2]["id"],3);
 let before=read(&root);let snap=before.jobs.last().unwrap().assembly_snapshot.as_ref().unwrap();assert_eq!(snap.len(),3);assert_eq!(snap[2].shot_id,"third-shot");assert!((snap[2].duration_seconds-2.0).abs()<0.01);
 let result=director::director_approve(app,token(&prep)).unwrap();let after=read(&root);assert_eq!(after.jobs.last().unwrap().status,JobStatus::Completed,"{result:?}");
 session::close();session::open(root.to_str().unwrap()).unwrap();assert_eq!(read(&root).jobs.last().unwrap().assembly_snapshot.as_ref(),Some(snap));
 let output=root.join(after.jobs.last().unwrap().output_path.as_ref().unwrap());
 let probe:Value=serde_json::from_slice(&ok(Command::new("ffprobe").args(["-v","error","-show_format","-of","json"]).arg(&output))).unwrap();
 let duration:f64=probe["format"]["duration"].as_str().unwrap().parse().unwrap();assert!((duration-4.8).abs()<0.15,"{duration}");
 let rgb=ok(Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-vf","scale=1:1","-fps_mode","passthrough","-pix_fmt","rgb24","-f","rawvideo","-"]));assert!(rgb[0]>60&&rgb[1]>40&&rgb[2]<20);assert!(rgb[rgb.len()-1]>60&&rgb[rgb.len()-3]<20);
 assert!(!root.join("prod.md").exists());
}
/// SB03 exercises only the local adapter seam; it never consumes/clicks a
/// Director approval and never changes creative review/approval state.
fn sb03_local_copy(root: &Path, app: &HostCtx) -> std::path::PathBuf {
    let before = read(root);
    let sources: Vec<_> = before.scenes.iter().flat_map(|s| &s.cuts)
        .flat_map(|c| &c.takes).map(|t| (t.path.clone(), fs::read(root.join(&t.path)).unwrap())).collect();
    let prep = workflow::cmd_prepare_assemble_with_host(app, &json!({"projectRoot": root})).unwrap();
    let action = pending::current().unwrap();
    let prepared = read(root);
    let job = prepared.jobs.last().unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.scope.as_ref().unwrap().kind, "project");
    let snapshot = job.assembly_snapshot.clone().unwrap();
    assert_eq!(snapshot.len(), 3);
    let mut expected = Vec::new();
    let mut scenes: Vec<_> = before.scenes.iter().collect();
    scenes.sort_by_key(|s| s.id);
    for scene in scenes {
        let mut cuts: Vec<_> = scene.cuts.iter().collect();
        cuts.sort_by_key(|c| c.id.cut);
        if let Some(anim) = scene.animatic.as_ref().filter(|a| !a.shots.is_empty()) {
            cuts = anim.shots.iter().map(|s| scene.cuts.iter().find(|c| c.id == s.cut_id).unwrap()).collect();
        }
        for cut in cuts {
            let take = cut.takes.iter().find(|t| Some(&t.id) == cut.selected_take.as_ref()).unwrap();
            let timing = scene.animatic.as_ref().and_then(|a| a.shots.iter().find(|s| s.cut_id == cut.id));
            let (start, duration, transition) = if let Some(t) = timing {
                (t.trim_in_seconds.unwrap_or(0.0), t.duration_seconds - t.trim_in_seconds.unwrap_or(0.0) - t.trim_out_seconds.unwrap_or(0.0), t.transition.map(|t| t.label()).unwrap_or("cut").to_string())
            } else {
                (0.0, workflow::qc::probe_media(&root.join(&take.path)).unwrap().duration_secs.unwrap(), "cut".into())
            };
            expected.push(workflow::state::AssemblyShotSnapshot { shot_id: cut.stable_id.clone(), cut_label: cut.id.cut, take_id: take.id.clone(), path: take.path.clone(), start_seconds: start, duration_seconds: duration, transition });
        }
    }
    assert_eq!(snapshot, expected);
    assert_eq!(job.input_asset_ids, expected.iter().map(|s| s.take_id.clone()).collect::<Vec<_>>());
    assert_eq!(prepared.jobs[..before.jobs.len()], before.jobs);
    for scene in &before.scenes {
        assert_eq!(prepared.scene(scene.id).unwrap().cuts, scene.cuts);
        assert_eq!(prepared.scene(scene.id).unwrap().animatic, scene.animatic);
    }
    let output = root.join(job.output_path.as_ref().unwrap());
    assert!(!output.exists());
    // Exact native preparation/validation and ordinary FFmpeg lifecycle, without
    // invoking director_approve, generating media, or touching a provider.
    let validated = workbench_host::director_actions::validate_action(action, Some(root.to_str().unwrap())).unwrap();
    let result = workflow::assembly_jobs::with_validated_assembly_job(root, &validated, || {
        workbench_host::assemble::run(app, &validated, Some(root.to_str().unwrap()))
    }).unwrap();
    let complete = read(root);
    assert_eq!(complete.jobs.last().unwrap().status, JobStatus::Completed, "{result}");
    assert_eq!(complete.jobs[..before.jobs.len()], before.jobs);
    for scene in &before.scenes {
        assert_eq!(complete.scene(scene.id).unwrap().cuts, scene.cuts);
        assert_eq!(complete.scene(scene.id).unwrap().animatic, scene.animatic);
    }
    let asset = complete.asset(complete.jobs.last().unwrap().output_asset_ids.first().expect("project master must be registered for reopen")).unwrap();
    assert_eq!(asset.path, job.output_path.as_ref().unwrap().as_str());
    assert_eq!(asset.role, workflow::state::AssetRole::AssemblyDraft);
    assert_eq!(asset.status, workflow::state::AssetStatus::Present);
    assert_eq!(asset.scene_id, None);
    assert_eq!(asset.source_job_id.as_deref(), Some(job.id.as_str()));
    let probe = workflow::qc::probe_media(&output).unwrap();
    let expected_duration = workflow::post::snapshot_runtime(&snapshot);
    assert!((probe.duration_secs.unwrap() - expected_duration).abs() < 0.15, "{probe:?}, expected {expected_duration}");
    ok(Command::new("ffmpeg").args(["-v", "error", "-xerror", "-err_detect", "explode", "-i"]).arg(&output).args(["-f", "null", "-"]));
    pending::cancel_all(); session::close(); session::open(root.to_str().unwrap()).unwrap();
    // Full native importing reopen, not just JSON deserialization.
    let reopened = import::load_or_import(root).unwrap().state;
    let retained = reopened.jobs.iter().find(|j| j.id == job.id).unwrap();
    assert_eq!(retained.status, JobStatus::Completed);
    assert_eq!(retained.assembly_snapshot.as_ref(), Some(&expected));
    assert_eq!(reopened.asset(&asset.id).unwrap(), asset);
    for scene in &before.scenes {
        let after = reopened.scene(scene.id).unwrap();
        assert_eq!(after.animatic.as_ref().map(|a| a.approved).unwrap_or(false), scene.animatic.as_ref().map(|a| a.approved).unwrap_or(false));
        for cut in &scene.cuts {
            let after = after.cuts.iter().find(|c| c.stable_id == cut.stable_id).unwrap();
            assert_eq!(after.selected_take, cut.selected_take);
            assert_eq!(after.takes, cut.takes);
        }
    }
    for (path, bytes) in sources { assert_eq!(fs::read(root.join(path)).unwrap(), bytes); }
    assert!(!root.join("prod.md").exists());
    println!("SB03 expected=actual selected snapshot {}; measured_output={} expected_runtime={expected_duration}; master={}; registered draft/present, no auto-approval; full native reopen; source bytes and prior jobs retained; card={}", serde_json::to_string(&expected).unwrap(), probe.duration_secs.unwrap(), output.display(), prep["pendingAction"]);
    output
}

#[test]
fn sb03_three_selected_project_local_ffmpeg() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project"); fs::create_dir(&root).unwrap(); fixture(&root);
    fs::remove_file(root.join("prod.md")).unwrap();
    let mut state = read(&root);
    let mut third = state.scenes[0].clone();
    third.id = 3; third.stable_id = "scene-3".into(); third.animatic = None;
    third.cuts.truncate(1);
    let cut = &mut third.cuts[0]; cut.id.source_scene = 3; cut.source_origin = None; cut.stable_id = "shot-3".into();
    cut.takes[0].id = "take-3".into(); cut.takes[0].path = "Scenes/Scene3/Cut1/take.mp4".into(); cut.selected_take = Some("take-3".into());
    fs::create_dir_all(root.join("Scenes/Scene3/Cut1")).unwrap();
    ok(Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "color=c=green:s=160x90:r=24:d=2", "-c:v", "libx264", "-threads", "1", "-pix_fmt", "yuv420p"]).arg(root.join(&cut.takes[0].path)));
    fs::create_dir_all(root.join("Scene_Prompts/Scene3")).unwrap();
    let note = "Scene_Prompts/Scene3/Cut1.md";
    fs::write(root.join(note), fs::read_to_string(root.join("Scene_Prompts/Scene1/Cut1.md")).unwrap().replace("source_scene: 1", "source_scene: 3")).unwrap();
    cut.production_note_path = Some(note.into());
    let mut asset = state.assets[0].clone(); asset.id = "take-3".into(); asset.path = cut.takes[0].path.clone(); asset.scene_id = Some(3); asset.cut_id = Some(cut.id);
    state.assets.push(asset); state.scenes.push(third); save(&root, &state);
    session::open(root.to_str().unwrap()).unwrap(); let _g = Guard;
    // The two real Scene 1 renders enter the final selection through the host,
    // not by relabeling an empty alternate or replacing a file behind Studio.
    for cut in [1, 2] {
        let take = format!("take-{cut}");
        workflow::cmd_set_take_status(&json!({"projectRoot":root,"sceneId":1,"cut":cut,"takeId":take,"status":"alternate"})).unwrap();
        workflow::cmd_set_take_status(&json!({"projectRoot":root,"sceneId":1,"cut":cut,"takeId":take,"status":"selected"})).unwrap();
        assert_eq!(read(&root).scene(1).unwrap().cuts[(cut-1) as usize].selected_take.as_deref(),Some(take.as_str()));
    }
    // Selection's ordinary save auto-seeds the unrelated Scene 3; its 2s
    // fixture source has no authored 3s timing, so retain its prior absence.
    let mut selected = read(&root);
    selected.scenes.iter_mut().find(|scene|scene.id==3).unwrap().animatic=None;
    save(&root,&selected);
    let output = sb03_local_copy(&root, &HostCtx::new(dir.path().join("settings")));
    // Decode actual pixels from all three regions, not just duration/header.
    for (time, channel) in [(0.25, 0), (2.0, 2), (4.0, 1)] {
        let rgb = ok(Command::new("ffmpeg").args(["-v", "error", "-ss", &time.to_string(), "-i"]).arg(&output).args(["-frames:v", "1", "-vf", "crop=100:100,scale=1:1", "-pix_fmt", "rgb24", "-f", "rawvideo", "-"]));
        assert_eq!(rgb.len(), 3);
        assert!(rgb[channel] > 60 && rgb[channel] > rgb[(channel+1)%3] && rgb[channel] > rgb[(channel+2)%3], "time={time}, rgb={rgb:?}");
        println!("SB03 decoded order sample t={time}, rgb={rgb:?}");
    }
    // V2: reorder the two rendered cuts in Scene 1 and trim the selected blue source.
    let old_master = fs::read(&output).unwrap();
    let source = root.join("Scenes/Scene1/Cut1/take.mp4");
    let paid = fs::read(&source).unwrap();
    let first_job = read(&root).jobs.last().unwrap().clone();
    // The earlier SB03 reopen path may synthesize a planning animatic for the no-animatic scene.
    // Restore that scene's original explicit absence on this disposable copy before editing Scene 1.
    let mut copy = read(&root);
    copy.scenes.iter_mut().find(|scene| scene.id == 3).unwrap().animatic = None;
    save(&root, &copy);
    workflow::cmd_reorder_animatic(&json!({"projectRoot":root,"sceneId":1,"shotIds":["anim-1","anim-2"]})).unwrap();
    workflow::cmd_set_shot_trim(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":0.5,"trimOutSeconds":0.25,"expectedTakeId":"take-1"})).unwrap();
    let stale = read(&root);
    assert!(!workflow::post::snapshot_matches_scene(&stale.scenes[0], &first_job.assembly_snapshot.as_ref().unwrap()[..2]));
    assert!(workflow::cmd_approve_rendered_sequence(&json!({"projectRoot":root,"jobId":first_job.id})).unwrap_err().contains("stale"));
    let revised = sb03_local_copy(&root, &HostCtx::new(dir.path().join("settings-v2")));
    assert_ne!(revised, output);
    let saved = read(&root);
    let revised_job = saved.jobs.last().unwrap();
    assert_eq!(revised_job.assembly_snapshot.as_ref().unwrap().iter().map(|s|s.take_id.as_str()).collect::<Vec<_>>(),vec!["take-1","take-2","take-3"]);
    assert_eq!((revised_job.assembly_snapshot.as_ref().unwrap()[0].start_seconds,revised_job.assembly_snapshot.as_ref().unwrap()[0].duration_seconds),(0.5,1.25));
    for (time, channel) in [(0.3, 2), (1.5, 0), (3.2, 1)] {
        let rgb = ok(Command::new("ffmpeg").args(["-v","error","-ss",&time.to_string(),"-i"]).arg(&revised).args(["-frames:v","1","-vf","crop=100:100,scale=1:1","-pix_fmt","rgb24","-f","rawvideo","-"]));
        assert_eq!(rgb.len(),3);
        assert!(rgb[channel]>60 && rgb[channel]>rgb[(channel+1)%3] && rgb[channel]>rgb[(channel+2)%3],"time={time}, rgb={rgb:?}");
    }
    session::close(); session::open(root.to_str().unwrap()).unwrap();
    let reopened=import::load_or_import(&root).unwrap().state;
    assert_eq!(reopened.jobs.last().unwrap().assembly_snapshot,revised_job.assembly_snapshot);
    assert_eq!(fs::read(output).unwrap(),old_master);
    assert_eq!(fs::read(source).unwrap(),paid);
}

#[test]
#[ignore = "requires SB03_PROJECT_COPY disposable saved project"]
fn sb03_saved_project_copy_local_ffmpeg() {
    let root = std::path::PathBuf::from(std::env::var("SB03_PROJECT_COPY").unwrap());
    assert!(root.join(".sb03-disposable").is_file());
    session::open(root.to_str().unwrap()).unwrap(); let _g = Guard;
    let settings = tempfile::tempdir().unwrap();
    let output = sb03_local_copy(&root, &HostCtx::new(settings.path().join("settings")));
    if let Ok(destination) = std::env::var("SB03_OUTPUT_COPY") { fs::copy(output, destination).unwrap(); }
}

#[test]
fn missing_cut_note_names_exact_key_without_still_advice() {
 let dir=tempfile::tempdir().unwrap();let root=dir.path().join("project");fs::create_dir(&root).unwrap();fixture(&root);fs::remove_file(root.join("prod.md")).unwrap();fs::remove_file(root.join("Scene_Prompts/Scene1/Cut2.md")).unwrap();
 session::open(root.to_str().unwrap()).unwrap();let _g=Guard;
 let error=workflow::cmd_prepare_assemble(&json!({"projectRoot":root,"sceneId":1})).unwrap_err();assert!(error.contains("Scene 1 Cut 2 production_note_path"),"{error}");assert!(!error.contains("new still"));assert!(read(&root).jobs.is_empty());
}

#[test]
fn selected_video_edit_can_set_explicit_source_end_beyond_animatic_duration() {
 let dir=tempfile::tempdir().unwrap();let root=dir.path().join("project");fs::create_dir(&root).unwrap();fixture(&root);
 fs::remove_file(root.join("prod.md")).unwrap();
 ok(Command::new("ffmpeg").args(["-y","-v","error","-f","lavfi","-i","color=c=blue:s=160x90:r=24:d=6","-c:v","libx264","-threads","1","-pix_fmt","yuv420p"]).arg(root.join("Scenes/Scene1/Cut1/take.mp4")));
 let mut state=read(&root);let shot=state.scenes[0].animatic.as_mut().unwrap().shots.iter_mut().find(|s|s.id=="anim-1").unwrap();shot.duration_seconds=5.0;save(&root,&state);
 session::open(root.to_str().unwrap()).unwrap();let _g=Guard;
 workflow::cmd_set_shot_trim(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":0.0,"trimOutSeconds":0.0,"expectedTakeId":"take-1","sourceEndSeconds":5.1})).unwrap();
 let saved=read(&root);let shot=saved.scenes[0].animatic.as_ref().unwrap().shots.iter().find(|s|s.id=="anim-1").unwrap();
 assert!((shot.duration_seconds - 5.1).abs()<0.001);assert_eq!(shot.trim_source_take_id.as_deref(),Some("take-1"));
 let before=serde_json::to_value(&saved).unwrap();
 for args in [
  json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":0.0,"trimOutSeconds":0.0,"expectedTakeId":"take-1","sourceEndSeconds":6.5}),
  json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":5.2,"trimOutSeconds":0.0,"expectedTakeId":"take-1","sourceEndSeconds":5.1}),
  json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":0.0,"trimOutSeconds":0.0,"sourceEndSeconds":5.1}),
 ] { assert!(workflow::cmd_set_shot_trim(&args).is_err()); assert_eq!(serde_json::to_value(read(&root)).unwrap(),before); }
 let app=HostCtx::new(dir.path().join("settings"));
 workflow::cmd_prepare_assemble_with_host(&app,&json!({"projectRoot":root,"sceneId":1})).unwrap();
 let snap=read(&root).jobs.last().unwrap().assembly_snapshot.clone().unwrap();
 let edited=snap.iter().find(|s|s.take_id=="take-1").unwrap();
 assert!((edited.start_seconds-0.0).abs()<0.001); assert!((edited.duration_seconds-5.1).abs()<0.001);
 let action=pending::current().unwrap();
 let validated=workbench_host::director_actions::validate_action(action,Some(root.to_str().unwrap())).unwrap();
 workflow::assembly_jobs::with_validated_assembly_job(&root,&validated,|| workbench_host::assemble::run(&app,&validated,Some(root.to_str().unwrap()))).unwrap();
 let state=read(&root); let output=root.join(state.jobs.last().unwrap().output_path.as_ref().unwrap());
 let duration=workflow::qc::probe_media(&output).unwrap().duration_secs.unwrap();
 assert!((duration-workflow::post::snapshot_runtime(&snap)).abs()<0.15);
 pending::cancel_all(); session::close(); session::open(root.to_str().unwrap()).unwrap();
 assert_eq!(read(&root).jobs.last().unwrap().assembly_snapshot.as_ref(),Some(&snap));
}

#[test]
fn selected_video_edit_binds_window_to_source_and_reopens_without_changing_bytes() {
 let dir=tempfile::tempdir().unwrap();let root=dir.path().join("project");fs::create_dir(&root).unwrap();fixture(&root);
 fs::remove_file(root.join("prod.md")).unwrap();
 let source=root.join("Scenes/Scene1/Cut1/take.mp4");let original=fs::read(&source).unwrap();
 session::open(root.to_str().unwrap()).unwrap();let _g=Guard;
 let args=json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":0.5,"trimOutSeconds":0.25,"expectedTakeId":"take-1"});
 assert!(workflow::cmd_set_shot_trim(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":1.9,"trimOutSeconds":0.2,"expectedTakeId":"take-1"})).unwrap_err().contains("exceeds"));
 assert!(workflow::cmd_set_shot_trim(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":0.5,"trimOutSeconds":0.25,"expectedTakeId":"another-take"})).unwrap_err().contains("missing"));
 workflow::cmd_set_shot_trim(&args).unwrap();
 let saved=read(&root);let shot=&saved.scenes[0].animatic.as_ref().unwrap().shots[1];
 assert_eq!(shot.trim_source_take_id.as_deref(),Some("take-1"));
 assert_eq!((shot.trim_in_seconds,shot.trim_out_seconds),(Some(0.5),Some(0.25)));
 assert_eq!(saved.scenes[0].cuts[0].selected_take.as_deref(),Some("take-1"));
 let app=HostCtx::new(dir.path().join("settings"));
 let prep=workflow::cmd_prepare_assemble_with_host(&app,&json!({"projectRoot":root,"sceneId":1})).unwrap();
 assert!(prep["pendingAction"].is_string());
 let snapshot=read(&root).jobs.last().unwrap().assembly_snapshot.clone().unwrap();
 let edited=snapshot.iter().find(|s|s.take_id=="take-1").unwrap();
 assert_eq!((edited.start_seconds,edited.duration_seconds),(0.5,1.25));
 let action=pending::current().unwrap();
 let validated=workbench_host::director_actions::validate_action(action,Some(root.to_str().unwrap())).unwrap();
 let result=workflow::assembly_jobs::with_validated_assembly_job(&root,&validated,|| {
   workbench_host::assemble::run(&app,&validated,Some(root.to_str().unwrap()))
 }).unwrap();
 let assembled=read(&root);let output=root.join(assembled.jobs.last().unwrap().output_path.as_ref().unwrap());
 let probe=workflow::qc::probe_media(&output).unwrap();
 assert!((probe.duration_secs.unwrap()-workflow::post::snapshot_runtime(&snapshot)).abs()<0.15,"{result}, {probe:?}");
 ok(Command::new("ffmpeg").args(["-v","error","-xerror","-err_detect","explode","-i"]).arg(&output).args(["-f","null","-"]));
 pending::cancel_all();session::close();session::open(root.to_str().unwrap()).unwrap();
 let reopened=import::load_or_import(&root).unwrap().state;
 assert_eq!(reopened.scenes[0].animatic.as_ref().unwrap().shots[1].trim_source_take_id.as_deref(),Some("take-1"));
 assert_eq!(fs::read(&source).unwrap(),original);
 let mut changed=read(&root);changed.scenes[0].cuts[0].selected_take=Some("another-take".into());save(&root,&changed);
 let error=workflow::cmd_prepare_assemble(&json!({"projectRoot":root,"sceneId":1})).unwrap_err();
 assert!(error.contains("edit belongs to take take-1"),"{error}");
 println!("V1 expected=actual source-bound editorial window take-1 [0.50,1.75), selected/source bytes retained; reopen preserved provenance; changed selection refused");
}

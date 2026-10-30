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
 for (i,color) in [(1,"blue"),(2,"gold")]{ok(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i",&format!("color=c={color}:s=160x90:r=24:d=2"),"-c:v","libx264","-threads","1","-pix_fmt","yuv420p"]).arg(r.join(format!("Scenes/Scene1/Cut{i}/take.mp4"))));fs::write(r.join(format!("Scene_Prompts/Scene1/Cut{i}.md")),format!("```yaml\nsource_scene: 1\ncut: {i}\ntitle: Shot {i}\nscene_type: b_roll\nspeech:\n  mode: none\nprompt: color {color}\n```\n")).unwrap();}
 let s:ProjectState=serde_json::from_value(json!({"version":2,"project_name":"S07 regression","settings":{"aspect_ratio":"9:16"},"profile":{"recipe":"narrative_dialogue","resolution":"1080x1920","aspect_ratios":["9:16"],"deliverables":[{"id":"review","name":"Review","resolution":"1080x1920","aspect_ratio":"9:16"}]},"assets":([1,2].map(|i|json!({"id":format!("take-{i}"),"path":format!("Scenes/Scene1/Cut{i}/take.mp4"),"role":"video_take","status":"present","scene_id":1,"cut_id":{"source_scene":1,"cut":i}}))),"scenes":[{"id":1,"stable_id":"scene-1","title":"S07","cuts":([1,2].map(|i|json!({"id":{"source_scene":1,"cut":i},"stable_id":format!("shot-{i}"),"source_origin":{"source_scene":1,"cut":i},"production_note_path":format!("Scene_Prompts/Scene1/Cut{i}.md"),"sketch":{"title":format!("Shot{i}"),"shot_role":"insert"},"visual_source":{"strategy":"no_start_frame"},"takes":[{"id":format!("take-{i}"),"path":format!("Scenes/Scene1/Cut{i}/take.mp4"),"status":"selected"}],"selected_take":format!("take-{i}")}))),"animatic":{"id":"anim","approved":true,"shots":([2,1].map(|i|json!({"id":format!("anim-{i}"),"cut_id":{"source_scene":1,"cut":i},"duration_seconds":2,"trim_in_seconds":0.25,"trim_out_seconds":0.25,"transition":if i==2{"dissolve"}else{"cut"}})))}}]})).unwrap();save(r,&s);
}
fn token(v:&Value)->String{v["pendingAction"].as_str().unwrap().split(pending::APPROVE_TOKEN_PREFIX).nth(1).unwrap().split("]]").next().unwrap().into()}
#[test]
fn selected_assembly_carries_cut_identity_and_saved_custom_windows() {
    let dir=tempfile::tempdir().unwrap();let root=dir.path().join("project");fs::create_dir(&root).unwrap();fixture(&root);
    session::open(root.to_str().unwrap()).unwrap();let _g=Guard;let app=HostCtx::new(dir.path().join("settings"));
    fs::write(root.join("prod.md"),"### Scene 1 — Cut 1
```yaml
source_scene: 1
cut: 1
head_handle_seconds: 0.5
estimated_screen_duration_seconds: 0.4
speech:
  mode: none
```
### Scene 1 — Cut 2
```yaml
source_scene: 1
cut: 2
head_handle_seconds: 0.1
estimated_screen_duration_seconds: 0.6
speech:
  mode: none
```
").unwrap();
    workflow::cmd_set_animatic_duration(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-2","durationSeconds":2.0})).unwrap();
    workflow::cmd_set_shot_trim(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-2","trimInSeconds":0.5,"trimOutSeconds":0.0})).unwrap();
    workflow::cmd_set_shot_transition(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-2","transition":"cut"})).unwrap();
    workflow::cmd_set_shot_trim(&json!({"projectRoot":root,"sceneId":1,"shotId":"anim-1","trimInSeconds":0.25,"trimOutSeconds":0.5})).unwrap();
    session::close();session::open(root.to_str().unwrap()).unwrap();
    let prep=workflow::cmd_prepare_assemble_with_host(&app,&json!({"projectRoot":root,"sceneId":1})).unwrap();
    let action=pending::current().unwrap();let entries=action.inputs["scenes"].as_array().unwrap();
    assert_eq!(entries[0]["id"],1,"source scene must not become cut number");
    assert_eq!(entries[0]["cut"],2,"exact handoff cut identity required");
    assert_eq!(entries[0]["start"],0.5);assert_eq!(entries[0]["duration"],1.5,"explicit editorial range beats shorter estimate");
    let before=read(&root);let job=before.jobs.last().unwrap();let snapshot=job.assembly_snapshot.clone().unwrap();
    for field in ["id","cut"] {
        let mut changed=action.clone();changed.inputs["scenes"][0][field]=json!(if field=="id" {2} else {1});
        let validated=workbench_host::director_actions::validate_action(changed,Some(root.to_str().unwrap())).unwrap();
        let called=std::cell::Cell::new(false);
        assert!(workflow::assembly_jobs::with_validated_assembly_job(&root,&validated,||{called.set(true);Ok("must not dispatch".into())}).is_err());
        assert!(!called.get());assert_eq!(read(&root).jobs.last().unwrap().status,JobStatus::Pending);
    }

    let result=director::director_approve(app.clone(),token(&prep)).unwrap();
    let after=read(&root);assert_eq!(after.jobs.last().unwrap().status,JobStatus::Completed,"{result:?}");
    let version=after.scene(1).unwrap().post.as_ref().unwrap().versions.last().unwrap();assert_eq!(version.assembly_snapshot.as_ref(),Some(&snapshot));assert_eq!(version.shots[0].take_id,"take-2");
    let probe:Value=serde_json::from_slice(&ok(Command::new("ffprobe").args(["-v","error","-show_format","-of","json"]).arg(root.join(job.output_path.as_ref().unwrap())))).unwrap();
    let duration:f64=probe["format"]["duration"].as_str().unwrap().parse().unwrap();assert!((duration-2.75).abs()<0.15,"{duration}");
    let rgb=ok(Command::new("ffmpeg").args(["-v","error","-i"]).arg(root.join(job.output_path.as_ref().unwrap())).args(["-vf","scale=1:1","-pix_fmt","rgb24","-f","rawvideo","-"]));
    assert!(rgb[0]>60 && rgb[1]>40 && rgb[2]<20,"first selected cut 2 must be gold");
    assert!(rgb[rgb.len()-1]>60 && rgb[rgb.len()-3]<20,"last selected cut 1 must be blue");
}

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use workbench_host::scene_plan::handoff::CutId;
use workbench_host::workflow::import::{import_project, save_project, ImportOptions};
use workbench_host::workflow::sources;
use workbench_host::workflow::state::{AssetRecord, AssetRole, AssetStatus, JobAttempt, JobRecord, JobStatus, ProjectState, TakeStatus, VisualSourceStrategy};

fn write(root: &Path, path: &str) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, b"A04 local importer test fixture; not playable media").unwrap();
}
fn scan(root: &Path) -> ProjectState {
    import_project(root, ImportOptions { persist: true }).unwrap().state
}
fn map(state: &ProjectState) -> BTreeMap<String, String> {
    state.assets.iter().map(|a| (a.path.clone(), a.id.clone())).collect()
}
fn assert_referents(old: &BTreeMap<String,String>, state: &ProjectState) {
    let new = map(state);
    for (path,id) in old { assert_eq!(new.get(path),Some(id),"unchanged path {path} changed persisted ID"); }
    let ids: std::collections::HashSet<_> = state.assets.iter().map(|a| &a.id).collect();
    assert_eq!(ids.len(),state.assets.len(),"duplicate IDs");
}

#[test]
fn earlier_sorted_take_insertions_preserve_graph_and_existing_ids() {
    let temp = tempfile::tempdir().unwrap(); let root=temp.path();
    for path in ["Scenes/Scene1/Cut1/b.mp4","Scenes/Scene1/Cut2/b.mp4","Scenes/Scene2/Cut1/b.mp4"] { write(root,path); }
    let mut old=scan(root);
    let first=old.scene(1).unwrap().cuts[0].takes[0].id.clone();
    let second=old.scene(1).unwrap().cuts[1].takes[0].id.clone();
    let later=old.scene(2).unwrap().cuts[0].takes[0].id.clone();
    for (s,c,id) in [(1,1,first.clone()),(1,2,second.clone()),(2,1,later.clone())] { sources::set_take_status(&mut old,s,c,&id,TakeStatus::Selected).unwrap(); }
    let frame_path="Preproduction/Scene1/continuity_frames/local.png"; write(root,frame_path);
    let frame=old.push_asset(AssetRecord { id:String::new(),path:frame_path.into(),role:AssetRole::ContinuityFrame,status:AssetStatus::Approved,scene_id:Some(1),cut_id:Some(CutId::new(1,2)),character_id:None,source_job_id:None,parent_asset_id:Some(first.clone()),notes:None });
    sources::set_strategy(&mut old,1,2,VisualSourceStrategy::PreviousSelectedTakeExitFrame,Some(CutId::new(1,1)),None).unwrap();
    let source=old.scenes.iter_mut().find(|s|s.id==1).unwrap().cuts.iter_mut().find(|c|c.id.cut==2).unwrap().visual_source.as_mut().unwrap();
    source.asset_id=Some(frame.clone());source.source_take_id=Some(first.clone());source.frame_time_seconds=Some(0.5);
    let mut job=JobRecord::open("job_fixture","identity_fixture");job.input_asset_ids=vec![first.clone(),frame.clone()];job.output_asset_ids=vec![later.clone()];job.attempts.push(JobAttempt{id:"attempt_fixture".into(),status:JobStatus::Completed,error:None,output_asset_ids:vec![second.clone()],started_at:None,completed_at:None});old.jobs.push(job);
    save_project(root,old.clone()).unwrap();let previous=map(&old);
    for path in ["Scenes/Scene1/Cut1/a.mp4","Scenes/Scene1/Cut2/a.mp4"] {
        write(root,path);let current=scan(root);assert_referents(&previous,&current);
        assert_eq!(current.scene(1).unwrap().cuts[0].selected_take.as_deref(),Some(first.as_str()));
        assert_eq!(current.scene(1).unwrap().cuts[1].selected_take.as_deref(),Some(second.as_str()));
        assert_eq!(current.scene(2).unwrap().cuts[0].selected_take.as_deref(),Some(later.as_str()));
        let source=current.scene(1).unwrap().cuts[1].visual_source.as_ref().unwrap();assert_eq!(source.asset_id.as_deref(),Some(frame.as_str()));assert_eq!(source.source_take_id.as_deref(),Some(first.as_str()));
        assert_eq!(current.jobs[0].input_asset_ids,old.jobs[0].input_asset_ids);assert_eq!(current.jobs[0].output_asset_ids,old.jobs[0].output_asset_ids);assert_eq!(current.jobs[0].attempts[0].output_asset_ids,old.jobs[0].attempts[0].output_asset_ids);
        let imported=map(&current);let again=scan(root);assert_eq!(map(&again),imported);
    }
}

#[test]
fn missing_video_is_not_retained_as_a_discovered_asset() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();let p="Scenes/Scene1/Cut1/b.mp4";write(root,p);let old=scan(root);assert!(old.assets.iter().any(|a|a.path==p));fs::remove_file(root.join(p)).unwrap();let missing=scan(root);assert!(!missing.assets.iter().any(|a|a.path==p),"missing video discovery semantics changed");
}

#[test]
fn new_discovery_does_not_reuse_a_missing_asset_id_referenced_by_a_job() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();let p="Scenes/Scene1/Cut1/b.mp4";write(root,p);let mut old=scan(root);let id=old.assets.iter().find(|a|a.path==p).unwrap().id.clone();let mut job=JobRecord::open("job_fixture","identity_fixture");job.output_asset_ids=vec![id.clone()];old.jobs.push(job);save_project(root,old).unwrap();fs::remove_file(root.join(p)).unwrap();let missing=scan(root);assert!(!missing.assets.iter().any(|a|a.path==p));write(root,"Scenes/Scene1/Cut1/a.mp4");let new=scan(root);assert!(new.assets.iter().all(|a|a.id!=id),"a historical job ID was reassigned to a different artifact");assert_eq!(new.jobs[0].output_asset_ids,vec![id]);
}

#[test]
fn fresh_discovery_allocates_asset_ids_above_the_persisted_graph_high_water() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();
    for path in ["Scenes/Scene1/Cut1/b.mp4","Scenes/Scene1/Cut2/b.mp4","Scenes/Scene2/Cut1/b.mp4"] { write(root,path); }
    let mut old=scan(root);
    let frame_path="Preproduction/Scene1/continuity_frames/local.png";write(root,frame_path);
    let frame=old.push_asset(AssetRecord { id:String::new(),path:frame_path.into(),role:AssetRole::ContinuityFrame,status:AssetStatus::Approved,scene_id:Some(1),cut_id:Some(CutId::new(1,2)),character_id:None,source_job_id:None,parent_asset_id:Some(old.assets.iter().find(|a|a.path=="Scenes/Scene1/Cut1/b.mp4").unwrap().id.clone()),notes:None });
    save_project(root,old.clone()).unwrap();
    let mut max=0u64; for a in &old.assets { if let Some(n)=a.id.strip_prefix("asset_").and_then(|s|s.parse::<u64>().ok()) { max=max.max(n); } }
    write(root,"Scenes/Scene1/Cut1/a.mp4");
    let current=scan(root);
    assert!(current.assets.iter().any(|a|a.path=="Scenes/Scene1/Cut1/a.mp4"), "newly scanned asset must exist");
    for a in &current.assets {
        if a.path=="Scenes/Scene1/Cut1/a.mp4" {
            assert!(a.id.strip_prefix("asset_").and_then(|s|s.parse::<u64>().ok()).unwrap() > max, "fresh asset id {} must exceed persisted high-water {}", a.id, max);
        }
    }
    assert_eq!(current.assets.iter().find(|a|a.path==frame_path).unwrap().id, frame, "persisted continuity frame id must not change");
}

#[test]
fn fresh_character_reference_assets_are_retargeted_without_rewriting_saved_refs() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();
    for path in ["Scenes/Scene1/Cut1/b.mp4","Scenes/Scene1/Cut2/b.mp4","Scenes/Scene2/Cut1/b.mp4"] { write(root,path); }
    let mut old=scan(root);
    let first=old.scene(1).unwrap().cuts[0].takes[0].id.clone();
    let frame_path="Preproduction/Scene1/continuity_frames/local.png";write(root,frame_path);
    let frame=old.push_asset(AssetRecord { id:String::new(),path:frame_path.into(),role:AssetRole::ContinuityFrame,status:AssetStatus::Approved,scene_id:Some(1),cut_id:Some(CutId::new(1,2)),character_id:None,source_job_id:None,parent_asset_id:Some(first.clone()),notes:None });
    let mut job=JobRecord::open("job_fixture","identity_fixture");job.input_asset_ids=vec![first.clone(),frame.clone()];job.output_asset_ids=vec![old.scene(2).unwrap().cuts[0].takes[0].id.clone()];old.jobs.push(job);
    save_project(root,old.clone()).unwrap();
    write(root,"Scenes/Scene1/Cut1/a.mp4");
    let current=scan(root);
    assert_eq!(current.jobs[0].input_asset_ids, old.jobs[0].input_asset_ids, "saved job input refs must not be rewritten");
    assert_eq!(current.jobs[0].output_asset_ids, old.jobs[0].output_asset_ids);
    assert_eq!(current.assets.iter().find(|a|a.path==frame_path).unwrap().id, frame);
}

#[test]
fn parent_real_character_scan_keeps_fresh_and_saved_namespaces_separate() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();
    write(root,"Scenes/Scene1/Cut1/b.mp4");
    fn character(root:&Path,name:&str) {
        write(root,&format!("Characters/{name}/model_sheet/front.png"));
        fs::write(root.join(format!("Characters/{name}/model_sheet_manifest.yaml")),format!("character: {name}\nidentity_source:\n  approval_shot_path: model_sheet/front.png\nshots:\n  - shot_id: closeup_front\n    path: model_sheet/front.png\n    status: saved\n")).unwrap();
    }
    character(root,"Zed");let mut old=scan(root);assert_eq!(old.characters.len(),1);
    let saved_char_id=old.characters[0].asset_ids[0].clone();
    let mut job=JobRecord::open("job_kept","identity_fixture");job.input_asset_ids=vec![saved_char_id.clone()];old.jobs.push(job);save_project(root,old.clone()).unwrap();
    character(root,"Amy");let now=scan(root);assert_eq!(now.characters.len(),2);
    for c in &now.characters { for id in &c.asset_ids {let a=now.asset(id).expect("fresh character reference points nowhere");assert_eq!(a.character_id.as_deref(),Some(c.id.as_str()));assert!(a.path.starts_with(&format!("Characters/{}/",c.name)));}}
    let zed=now.characters.iter().find(|c|c.name=="Zed").unwrap();assert_eq!(zed.asset_ids[0],saved_char_id);assert_eq!(now.jobs,old.jobs);
}

#[test]
fn parent_reload_allocator_reserves_reference_only_ids() {
    let mut state=ProjectState::new();let mut job=JobRecord::open("job_kept","identity_fixture");job.attempts[0].output_asset_ids=vec!["asset_901".into()];state.jobs.push(job);
    state.versions.push(workbench_host::workflow::state::ArtifactVersion{id:"ver_1".into(),asset_id:"asset_950".into(),version:1,path:"Final/old.mp4".into(),status:AssetStatus::Present});
    let back=ProjectState::from_json(&state.to_json().unwrap()).unwrap();assert_eq!(back.next_asset_id(),"asset_951");
}

#[test]
fn parent_reimport_keeps_artifact_version_history_and_missing_referent_reserved() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();write(root,"Scenes/Scene1/Cut1/b.mp4");let mut old=scan(root);
    old.versions.push(workbench_host::workflow::state::ArtifactVersion{id:"ver_1".into(),asset_id:"asset_950".into(),version:1,path:"Final/old.mp4".into(),status:AssetStatus::Present});
    save_project(root,old.clone()).unwrap();write(root,"Scenes/Scene1/Cut1/a.mp4");let now=scan(root);
    assert_eq!(now.versions,old.versions,"reimport discarded persisted artifact history");
    assert!(now.assets.iter().all(|a|a.id!="asset_950"));let back=ProjectState::from_json(&now.to_json().unwrap()).unwrap();assert!(back.next_asset_id().strip_prefix("asset_").unwrap().parse::<u64>().unwrap()>950);
}

#[test]
fn parent_missing_reference_keeps_historical_job_binding_and_reservation() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();write(root,"Scenes/Scene1/Cut1/b.mp4");let mut old=scan(root);
    let path="Preproduction/Scene1/missing.png";
    old.push_asset(AssetRecord { id:"asset_950".into(),path:path.into(),role:AssetRole::ContinuityFrame,status:AssetStatus::Approved,scene_id:Some(1),cut_id:Some(CutId::new(1,1)),character_id:None,source_job_id:None,parent_asset_id:None,notes:None });
    let mut job=JobRecord::open("job_kept","identity_fixture");job.input_asset_ids=vec!["asset_950".into()];old.jobs.push(job);save_project(root,old.clone()).unwrap();
    let now=scan(root);assert_eq!(now.jobs[0].input_asset_ids,old.jobs[0].input_asset_ids,"missing artifact erased historical job binding");assert_eq!(now.next_asset_id(),"asset_951");
}

#[test]
fn parent_shared_master_production_path_keeps_distinct_shot_ids() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();
    use workbench_host::scene_plan::fixtures::{broll_scene_yaml,cut_markdown};
    fs::write(root.join("prod.md"),format!("{}\n{}",cut_markdown(1,1,&broll_scene_yaml(1).replace("prompt: |-\n","prompt: |-\n  FIRST_SOURCE_CAR.\n")),cut_markdown(1,2,&broll_scene_yaml(2).replace("prompt: |-\n","prompt: |-\n  SECOND_SOURCE_CEMETERY.\n")))).unwrap();
    let old=scan(root);assert_eq!(old.scene(1).unwrap().cuts.len(),2);let ids:Vec<_>=old.scene(1).unwrap().cuts.iter().map(|c|c.stable_id.clone()).collect();assert_ne!(ids[0],ids[1]);
    let bindings:BTreeMap<_,_>=old.scene(1).unwrap().cuts.iter().map(|c|(c.stable_id.clone(),c.prompt.clone())).collect();
    assert_ne!(bindings[&ids[0]],bindings[&ids[1]],"fixture must have distinct shot content");
    let mut reordered=old.clone();let reversed=vec![ids[1].clone(),ids[0].clone()];workbench_host::workflow::identity::reorder_cuts(&mut reordered,1,&reversed).unwrap();save_project(root,reordered).unwrap();
    let now=scan(root);let new_ids:Vec<_>=now.scene(1).unwrap().cuts.iter().map(|c|c.stable_id.clone()).collect();assert_eq!(new_ids,reversed,"shared master production path collapsed reordered persisted shots");
    for current in [&now,&scan(root)] {
        for c in &current.scene(1).unwrap().cuts {assert_eq!(Some(&c.prompt),bindings.get(&c.stable_id),"durable ID changed source content after reorder/reimport");}
    }
}


fn shared_master_fixture(root: &Path) {
    use workbench_host::scene_plan::fixtures::{broll_scene_yaml,cut_markdown};
    fs::write(root.join("prod.md"),format!("{}\n{}",cut_markdown(1,1,&broll_scene_yaml(1).replace("prompt: |-\n","prompt: |-\n  CAR_SOURCE_ONLY.\n")),cut_markdown(1,2,&broll_scene_yaml(2).replace("prompt: |-\n","prompt: |-\n  CEMETERY_SOURCE_ONLY.\n")))).unwrap();
}
#[test]
fn parent_shared_master_binds_metadata_and_media_to_source_after_reorder_twice() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();shared_master_fixture(root);
    write(root,"Scenes/Scene1/Cut1/first.mp4");write(root,"Scenes/Scene1/Cut2/second.mp4");let mut old=scan(root);
    for number in 1..=2 { let id=old.scene(1).unwrap().cuts[(number-1) as usize].takes[0].id.clone();sources::set_take_status(&mut old,1,number,&id,TakeStatus::Selected).unwrap();old.scene_mut(1,"").cuts[(number-1) as usize].start_frame_prompt=Some(format!("source-owned-metadata-{number}")); }
    let bindings:BTreeMap<_,_>=old.scene(1).unwrap().cuts.iter().map(|c|(c.stable_id.clone(),(c.prompt.clone(),c.start_frame_prompt.clone(),c.selected_take.clone(),c.takes.clone(),c.source_origin))).collect();
    let reversed:Vec<_>=old.scene(1).unwrap().cuts.iter().rev().map(|c|c.stable_id.clone()).collect();workbench_host::workflow::identity::reorder_cuts(&mut old,1,&reversed).unwrap();save_project(root,old).unwrap();
    for _ in 0..2 {let now=scan(root);for c in &now.scene(1).unwrap().cuts {assert_eq!(&(c.prompt.clone(),c.start_frame_prompt.clone(),c.selected_take.clone(),c.takes.clone(),c.source_origin),bindings.get(&c.stable_id).unwrap(),"source metadata/media attached to wrong durable shot");}}
}
#[test]
fn parent_ambiguous_originless_reordered_shared_master_preserves_saved_bytes() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();shared_master_fixture(root);let mut old=scan(root);let reversed:Vec<_>=old.scene(1).unwrap().cuts.iter().rev().map(|c|c.stable_id.clone()).collect();workbench_host::workflow::identity::reorder_cuts(&mut old,1,&reversed).unwrap();for c in &mut old.scene_mut(1,"").cuts {c.source_origin=None;}save_project(root,old).unwrap();let path=root.join("Preproduction/workflow_state.json");let bytes=fs::read(&path).unwrap();
    let err=import_project(root,ImportOptions{persist:true}).unwrap_err();assert!(err.contains("ambiguous"),"{err}");assert_eq!(fs::read(path).unwrap(),bytes,"ambiguity destructively rewrote legacy state");
}
#[test]
fn parent_duplicate_saved_source_origins_are_not_many_to_one() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();shared_master_fixture(root);let mut old=scan(root);let origin=old.scene(1).unwrap().cuts[0].source_origin;old.scene_mut(1,"").cuts[1].source_origin=origin;save_project(root,old).unwrap();let path=root.join("Preproduction/workflow_state.json");let bytes=fs::read(&path).unwrap();
    assert!(import_project(root,ImportOptions{persist:true}).is_err(),"duplicate saved source origins were silently mapped many-to-one");assert_eq!(fs::read(path).unwrap(),bytes);
}
#[test]
fn parent_saved_alternate_without_discovered_source_is_preserved() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();write(root,"Scenes/Scene1/Cut1/b.mp4");let mut old=scan(root);let id=workbench_host::workflow::identity::add_alternate_shot(&mut old,1).unwrap();let c=old.scene_mut(1,"").cuts.iter_mut().find(|c|c.stable_id==id).unwrap();c.start_frame_prompt=Some("retained alternate planning".into());let expected=c.clone();save_project(root,old).unwrap();
    let now=scan(root);assert_eq!(now.scene(1).unwrap().cuts.iter().find(|c|c.stable_id==id),Some(&expected),"reimport discarded native saved alternate/history");
}
#[test]
fn parent_unique_per_cut_legacy_path_reimport_remains_compatible() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();use workbench_host::scene_plan::fixtures::{broll_scene_yaml,cut_markdown};
    fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();for n in 1..=2 {fs::write(root.join(format!("Scene_Prompts/Scene1/Cut{n}.md")),cut_markdown(1,n,&broll_scene_yaml(n))).unwrap();}
    let mut old=scan(root);assert_eq!(old.scene(1).unwrap().cuts.len(),2);for c in &mut old.scene_mut(1,"").cuts{c.source_origin=None;}let reversed:Vec<_>=old.scene(1).unwrap().cuts.iter().rev().map(|c|c.stable_id.clone()).collect();workbench_host::workflow::identity::reorder_cuts(&mut old,1,&reversed).unwrap();save_project(root,old).unwrap();for _ in 0..2{let now=scan(root);assert_eq!(now.scene(1).unwrap().cuts.iter().map(|c|c.stable_id.clone()).collect::<Vec<_>>(),reversed);}
}


#[test]
fn parent_removed_shared_source_block_never_silently_drops_saved_history() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();shared_master_fixture(root);write(root,"Scenes/Scene1/Cut1/first.mp4");write(root,"Scenes/Scene1/Cut2/second.mp4");let mut old=scan(root);
    for n in 1..=2 {let take=old.scene(1).unwrap().cuts[(n-1) as usize].takes[0].id.clone();sources::set_take_status(&mut old,1,n,&take,TakeStatus::Selected).unwrap();old.scene_mut(1,"").cuts[(n-1) as usize].start_frame_prompt=Some(format!("saved-source-{n}"));}save_project(root,old.clone()).unwrap();let bytes=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();
    use workbench_host::scene_plan::fixtures::{broll_scene_yaml,cut_markdown};fs::write(root.join("prod.md"),cut_markdown(1,1,&broll_scene_yaml(1))).unwrap();
    for _ in 0..2 {match import_project(root,ImportOptions{persist:true}) {Ok(result)=>{for c in &old.scene(1).unwrap().cuts {let now=result.state.scene(1).unwrap().cuts.iter().find(|x|x.stable_id==c.stable_id).expect("source removal silently discarded saved shot");assert_eq!(now.source_origin,c.source_origin);assert_eq!(now.start_frame_prompt,c.start_frame_prompt);assert_eq!(now.selected_take,c.selected_take);assert_eq!(now.takes,c.takes);}},Err(message)=>{assert!(message.contains("source")||message.contains("ambiguous"),"{message}");assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),bytes);}}}
}
#[test]
fn parent_missing_unique_source_never_relabels_saved_cut_as_unrelated_discovery() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();use workbench_host::scene_plan::fixtures::{broll_scene_yaml,cut_markdown};fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();let path=root.join("Scene_Prompts/Scene1/Cut1.md");fs::write(&path,cut_markdown(1,1,&broll_scene_yaml(1))).unwrap();let old=scan(root);let id=old.scene(1).unwrap().cuts[0].stable_id.clone();let bytes=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();fs::remove_file(path).unwrap();write(root,"Scenes/Scene1/Cut1/new.mp4");
    for _ in 0..2{match import_project(root,ImportOptions{persist:true}){Ok(result)=>{let kept=result.state.scene(1).unwrap().cuts.iter().find(|c|c.stable_id==id).expect("missing source relabelled saved durable shot");assert_eq!(kept.source_origin,old.scene(1).unwrap().cuts[0].source_origin);assert_eq!(kept.prompt,old.scene(1).unwrap().cuts[0].prompt);},Err(message)=>{assert!(message.contains("source")||message.contains("ambiguous"),"{message}");assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),bytes);}}}
}

#[test]
fn parent_lexically_equivalent_root_preserves_relative_source_and_asset_identity() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();fs::create_dir_all(root.join("alias")).unwrap();use workbench_host::scene_plan::fixtures::{broll_scene_yaml,cut_markdown};fs::create_dir_all(root.join("Scene_Prompts/Scene1")).unwrap();fs::write(root.join("Scene_Prompts/Scene1/Cut1.txt"),cut_markdown(1,1,&broll_scene_yaml(1))).unwrap();write(root,"Scenes/Scene1/Cut1/b.mp4");let old=scan(root);let now=scan(&root.join("alias/.."));assert_eq!(now.scene(1).unwrap().cuts[0].stable_id,old.scene(1).unwrap().cuts[0].stable_id);assert_eq!(map(&now),map(&old));assert_eq!(now.scene(1).unwrap().cuts[0].production_note_path,old.scene(1).unwrap().cuts[0].production_note_path);for asset in &now.assets{assert!(!Path::new(&asset.path).is_absolute());}
}

#[test]
fn parent_reimport_retains_assembly_asset_and_idempotent_job_version_history() {
    let temp=tempfile::tempdir().unwrap();let root=temp.path();write(root,"Scenes/Scene1/Cut1/b.mp4");let mut old=scan(root);let output="Final/Scene1_master.mp4";write(root,output);let id=old.push_asset(AssetRecord{id:String::new(),path:output.into(),role:AssetRole::AssemblyDraft,status:AssetStatus::Present,scene_id:Some(1),cut_id:None,character_id:None,source_job_id:Some("job_assembly".into()),parent_asset_id:None,notes:Some("review export".into())});workbench_host::workflow::identity::record_version(&mut old,&id,AssetStatus::Present);let mut job=JobRecord::open("job_assembly","assemble");job.raw_provider_id=Some("scene_1".into());job.finish(Some(output),Some(&id));old.jobs.push(job);save_project(root,old.clone()).unwrap();
    for _ in 0..2 {let now=scan(root);assert_eq!(now.assets.iter().find(|a|a.path==output).unwrap().id,id,"recovery allocated a new ID for unchanged native output");assert_eq!(now.versions,old.versions,"reimport added a phantom version");assert_eq!(now.jobs[0].output_asset_ids,old.jobs[0].output_asset_ids,"reimport appended phantom job outputs");}
}

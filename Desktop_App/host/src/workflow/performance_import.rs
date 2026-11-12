//! Offline native acquisition; importing never selects, approves or submits.
use super::{identity, import, render_jobs, state::{AssetRecord, AssetRole, AssetStatus}};
use crate::{media_inspect::run_bounded, project_path::{self, PathIntent}, scene_plan::handoff::{CutId, SpeechMode}, session};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::{self, OpenOptions}, io::{Read, Write}, path::{Path, PathBuf}, process::Command, sync::atomic::{AtomicU64, Ordering}, time::{Duration, Instant}};
const MAX_BYTES:u64=64*1024*1024;

fn positive(args:&Value,key:&str)->Result<u32,String>{
    args[key].as_u64().and_then(|n|u32::try_from(n).ok()).filter(|n|*n>0).ok_or_else(||format!("Invalid {key}"))
}
/// Exact lexical bucket AND canonical write policy. Reject links, including links
/// into another shot and dangling links, before any directory/file write.
fn destination(root:&Path,relative:&str)->Result<PathBuf,String>{
    let mut path=root.to_path_buf();
    for part in Path::new(relative).components(){
        if !matches!(part,std::path::Component::Normal(_)){return Err("Invalid import destination".into());}
        path.push(part);
        match fs::symlink_metadata(&path){
            Ok(meta) if meta.file_type().is_symlink()=>return Err("Dialogue destination must not contain symlinks".into()),
            Err(e) if e.kind()!=std::io::ErrorKind::NotFound=>return Err(e.to_string()),
            _=>{}
        }
    }
    let resolved=project_path::resolve(relative,root.to_str(),PathIntent::Write).map_err(|e|e.to_string())?.into_path_buf();
    if resolved!=path{return Err("Dialogue destination redirected outside exact shot bucket".into());}
    Ok(resolved)
}
struct Snapshot {file:PathBuf,dir:PathBuf}
impl Drop for Snapshot {fn drop(&mut self){let _=fs::remove_file(&self.file);let _=fs::remove_dir(&self.dir);}}
fn snapshot(bucket:&Path,bytes:&[u8])->Result<Snapshot,String>{
    static NEXT:AtomicU64=AtomicU64::new(0);
    for _ in 0..100 {
        let dir=bucket.join(format!(".dialogue-import-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        let mut builder=fs::DirBuilder::new();
        #[cfg(unix)] {use std::os::unix::fs::DirBuilderExt;builder.mode(0o700);}
        match builder.create(&dir){
            Ok(())=>{
                let owned=Snapshot{file:dir.join("snapshot.wav"),dir};
                let mut file=OpenOptions::new().write(true).create_new(true).open(&owned.file).map_err(|e|e.to_string())?;
                file.write_all(bytes).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;
                return Ok(owned);
            },
            Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,
            Err(e)=>return Err(e.to_string())
        }
    }
    Err("Cannot reserve private WAV snapshot".into())
}
fn validate(path:&Path,bytes:&[u8])->Result<(),String>{
    if bytes.len()<12 || !matches!(&bytes[..4],b"RIFF"|b"RF64") || &bytes[8..12]!=b"WAVE" {return Err("Import requires WAV bytes, not a renamed container".into());}
    let deadline=Instant::now()+Duration::from_secs(60);
    // Read only the exclusively owned snapshot, using WAV-only demuxing and no
    // network/playlist protocols. Never probe the mutable external path.
    let mut probe=Command::new("ffprobe");
    probe.args(["-v","error","-protocol_whitelist","file","-format_whitelist","wav","-f","wav","-show_entries","format=duration:stream=codec_type,sample_rate","-of","json"]).arg(path);
    let (raw,_)=run_bounded(&mut probe,deadline,64*1024)?;
    let data:Value=serde_json::from_slice(&raw).map_err(|_|"Invalid WAV inspection")?;
    data["format"]["duration"].as_str().and_then(|v|v.parse::<f64>().ok()).filter(|v|v.is_finite() && *v>0.0 && *v<=300.0).ok_or("WAV duration must be positive and at most 300 seconds")?;
    let streams=data["streams"].as_array().ok_or("WAV must contain one audio stream")?;
    if streams.len()!=1 || streams[0]["codec_type"]!="audio" {return Err("WAV must contain one audio stream".into());}
    streams[0]["sample_rate"].as_str().and_then(|v|v.parse::<u32>().ok()).filter(|v|(8000..=192000).contains(v)).ok_or("WAV sample rate must be 8000–192000 Hz")?;
    let mut decode=Command::new("ffmpeg");
    decode.args(["-nostdin","-v","error","-xerror","-err_detect","explode","-protocol_whitelist","file","-format_whitelist","wav","-f","wav","-i"]).arg(path).args(["-map","0:a:0","-f","null","-"]);
    run_bounded(&mut decode,deadline,64*1024)?;
    if fs::read(path).map_err(|e|e.to_string())?!=bytes {return Err("Owned WAV snapshot changed during validation".into());}
    Ok(())
}

pub fn acquire(args:&Value)->Result<Value,String>{
    let supplied=args["projectRoot"].as_str().filter(|s|!s.is_empty()).ok_or("Missing projectRoot")?;
    let root=session::command_root(Some(supplied))?;
    let scene=positive(args,"sceneId")?;let number=positive(args,"cut")?;let shot=CutId::new(scene,number);
    let stable=args["shotStableId"].as_str().filter(|s|!s.is_empty()).ok_or("Missing shotStableId")?;
    let revision=args["expectedRevision"].as_u64().filter(|v|*v<=9_007_199_254_740_991).ok_or("Invalid expectedRevision")?;
    let source=args["sourcePath"].as_str().filter(|s|!s.is_empty()).ok_or("Missing sourcePath")?;
    let _lock=import::lock_project(&root)?;
    let mut state=render_jobs::read_persisted(&root)?;
    if state.state_revision!=revision{return Err("Project revision changed; reload and reselect the WAV".into());}
    let cut=state.scene(scene).and_then(|s|s.cuts.iter().find(|c|c.id==shot)).ok_or("Performance shot not found")?;
    if cut.stable_id!=stable || cut.speech_mode!=Some(SpeechMode::SourceVideoLipsync){return Err("Exact talking-shot identity changed; reload and reselect the WAV".into());}
    let input=session::authorize(source,PathIntent::Read)?;
    if !input.extension().is_some_and(|v|v.eq_ignore_ascii_case("wav")){return Err("Import requires a .wav file".into());}
    // Inspect before opening: opening a FIFO may block forever, even when the
    // post-open regular-file check would eventually reject it.
    if !fs::metadata(&input).map_err(|e|e.to_string())?.is_file() {return Err("WAV must be a regular file".into());}
    let file=fs::File::open(&input).map_err(|e|format!("Cannot open selected WAV: {e}"))?;
    let meta=file.metadata().map_err(|e|e.to_string())?;
    if !meta.is_file() || meta.len()>MAX_BYTES{return Err("WAV must be a regular file of at most 64 MiB".into());}
    let mut bytes=Vec::new();file.take(MAX_BYTES+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len() as u64>MAX_BYTES{return Err("WAV exceeds 64 MiB".into());}
    let bucket_rel=format!("Scenes/Scene{scene}/Cut{number}");
    let bucket=destination(&root,&bucket_rel)?;
    fs::create_dir_all(&bucket).map_err(|e|e.to_string())?;
    let bucket=destination(&root,&bucket_rel)?;
    let owned=snapshot(&bucket,&bytes)?;validate(&owned.file,&bytes)?;
    let hash=Sha256::digest(&bytes).iter().map(|b|format!("{b:02x}")).collect::<String>();
    let eligible_input=project_path::relative_to_root(&root,&input).ok().and_then(|p|p.to_str().map(str::to_owned)).and_then(|relative|state.assets.iter().find(|a|a.path==relative).cloned());
    let existing=if let Some(a)=eligible_input {
        super::performance::asset(&root,&state,shot,&a.id)?;
        if a.scene_id.is_some_and(|s|s!=scene) || a.cut_id.is_some_and(|s|s!=shot){return Err("Registered WAV has conflicting shot scope".into());}
        Some(a)
    }else{None};
    let path=existing.as_ref().map(|a|a.path.clone()).unwrap_or_else(||format!("{bucket_rel}/dialogue_import_{hash}.wav"));
    let target=destination(&root,&path)?;
    let registered:Vec<_>=state.assets.iter().filter(|a|a.path==path).cloned().collect();
    if registered.len()>1{return Err("Ambiguous registered dialogue path".into());}
    if let Some(a)=registered.first(){
        super::performance::asset(&root,&state,shot,&a.id)?;
        if a.scene_id.is_some_and(|s|s!=scene) || a.cut_id.is_some_and(|s|s!=shot){return Err("Registered WAV has conflicting shot scope".into());}
    }
    let created=match fs::symlink_metadata(&target){
        Ok(meta)=>{
            if !meta.is_file() || fs::read(&target).map_err(|e|e.to_string())?!=bytes{return Err("Occupied dialogue path differs; existing bytes preserved".into());}
            false
        },
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{
            let mut file=OpenOptions::new().write(true).create_new(true).open(&target).map_err(|e|format!("Cannot reserve dialogue WAV: {e}"))?;
            if let Err(e)=file.write_all(&bytes).and_then(|_|file.sync_all()) {drop(file);let _=fs::remove_file(&target);return Err(format!("Cannot save dialogue WAV: {e}"));}
            true
        },
        Err(e)=>return Err(e.to_string())
    };
    let added=registered.is_empty();
    let asset=if let Some(a)=registered.first(){a.clone()}else{
        let id=state.push_asset(AssetRecord{id:String::new(),path:path.clone(),role:AssetRole::SourceAudio,status:AssetStatus::Draft,scene_id:Some(scene),cut_id:Some(shot),character_id:None,source_job_id:None,parent_asset_id:None,
            notes:Some(json!({"operation":"local_wav_import","sha256":hash,"byte_count":bytes.len(),"speech_completeness_verified":false}).to_string())});
        identity::record_version(&mut state,&id,AssetStatus::Draft);
        import::persist_state_locked(&root,&state).map_err(|e|format!("WAV retained at {path}, but registration failed: {e}"))?;
        state.asset(&id).unwrap().clone()
    };
    let mut state=render_jobs::read_persisted(&root)?;super::recipe::apply_stages(&mut state);
    Ok(json!({"report":super::report::project_report(&state),"state":state,
        "note":format!("Draft imported at {path}; audition and select explicitly. Import does not verify the transcript or submit speech/video."),
        "performanceImport":{"sceneId":scene,"cut":number,"shotStableId":stable,"assetId":asset.id,"path":path,"created":created,"registered":added}}))
}

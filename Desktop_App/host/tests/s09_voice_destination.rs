//! Production-library voice confinement regressions; no cfg(test) authorization bypass.
use serde_json::{json, Value};
use std::{fs, os::unix::fs::symlink, path::Path};
use workbench_host::{ctx::HostCtx, project_path::PathIntent, session, voice::RecorderState};
fn call(app:&HostCtx,rec:&RecorderState,cmd:&str,args:Value)->Result<Value,String>{workbench_host::dispatch(app,rec,cmd,&args)}
fn request(dir:&Path,source:&Path)->Value{json!({"characterDirectory":dir,"characterName":"Mira","characterReferencePath":null,"sampleText":"Local synthetic reference","audioPath":source,"tempPath":source,"voiceId":"fixture","voiceName":"Local","audioBase64":"UklGRg==","mediaType":"audio/wav","voiceDescription":"Local guide","generatedVoiceId":"candidate","projectRoot":dir.parent().unwrap().parent().unwrap(),"confirmation":"not-issued"})}
#[test]
fn destination_confinement_and_exact_source_grants(){
 for route in ["voice_attach_existing","voice_save_recording","voice_save_library_voice","voice_save_designed_voice"]{
  for case in ["outside_bucket","other_character_bucket","inner_redirect","profile_outside","profile_inside","dangling_audio"]{
   let t=tempfile::tempdir().unwrap();let root=t.path().join("project");let dir=root.join("Characters/Mira");let bucket=dir.join("voice");let outside=t.path().join("outside");fs::create_dir_all(&bucket).unwrap();fs::create_dir(&outside).unwrap();fs::write(outside.join("marker"),b"unchanged").unwrap();let source=root.join("source.wav");fs::write(&source,b"RIFF source unchanged").unwrap();let other=root.join("Characters/Other/voice");fs::create_dir_all(&other).unwrap();
   match case{
    "outside_bucket"=>{fs::remove_dir(&bucket).unwrap();symlink(&outside,&bucket).unwrap();},
    "other_character_bucket"=>{fs::remove_dir(&bucket).unwrap();symlink(&other,&bucket).unwrap();},
    "inner_redirect"=>{let inner=dir.join("different");fs::create_dir(&inner).unwrap();fs::remove_dir(&bucket).unwrap();symlink(inner,&bucket).unwrap();},
    "profile_outside"=>symlink(outside.join("marker"),bucket.join("Mira_voice_profile.yaml")).unwrap(),
    "profile_inside"=>{fs::write(bucket.join("other.yaml"),b"other metadata").unwrap();symlink(bucket.join("other.yaml"),bucket.join("Mira_voice_profile.yaml")).unwrap();},
    "dangling_audio"=>symlink(outside.join("new.wav"),bucket.join("Mira_voice_sample_01.wav")).unwrap(),_=>unreachable!()
   }
   session::open(root.to_str().unwrap()).unwrap();let app=HostCtx::new(t.path().join("settings"));let rec=RecorderState::new();let before:Vec<_>=fs::read_dir(&bucket).map(|v|v.map(|p|p.unwrap().file_name()).collect()).unwrap_or_default();let result=call(&app,&rec,route,json!({"request":request(&dir,&source)}));
   assert!(result.is_err(),"{route}/{case}: unsafe destination accepted: {result:?}");
   if route=="voice_save_designed_voice"{let error=result.unwrap_err();assert!(error.to_lowercase().contains("voice")||error.to_lowercase().contains("destination"),"wrong rejection seam: {error}");assert!(!error.contains("ElevenLabs"),"provider touched before local rejection: {error}");}
   assert_eq!(fs::read(outside.join("marker")).unwrap(),b"unchanged","{route}/{case}");assert_eq!(fs::read_dir(&outside).unwrap().count(),1,"{route}/{case}: outside creation");assert_eq!(fs::read(&source).unwrap(),b"RIFF source unchanged","{route}/{case}: source changed");assert_eq!(fs::read_dir(&other).unwrap().count(),0,"wrong character written");let after:Vec<_>=fs::read_dir(&bucket).map(|v|v.map(|p|p.unwrap().file_name()).collect()).unwrap_or_default();assert_eq!(after,before,"{route}/{case}: partial bucket write");session::close();
  }
 }
 let t=tempfile::tempdir().unwrap();let root=t.path().join("project");let dir=root.join("Characters/Mira");fs::create_dir_all(dir.join("voice")).unwrap();session::open(root.to_str().unwrap()).unwrap();let app=HostCtx::new(t.path().join("settings"));let rec=RecorderState::new();let source=t.path().join("external.wav");fs::write(&source,b"external reference").unwrap();let req=request(&dir,&source);
 assert!(call(&app,&rec,"voice_attach_existing",json!({"request":req})).is_err());session::grant(source.to_str().unwrap(),PathIntent::Read).unwrap();let saved=call(&app,&rec,"voice_attach_existing",json!({"request":req})).unwrap();assert_eq!(fs::read(saved["audioPath"].as_str().unwrap()).unwrap(),b"external reference");assert_eq!(fs::read(&source).unwrap(),b"external reference");assert!(call(&app,&rec,"voice_attach_existing",json!({"request":req})).is_err(),"grant replay");
 let inside=dir.join("voice/local.wav");fs::write(&inside,b"local reference").unwrap();let saved=call(&app,&rec,"voice_attach_existing",json!({"request":request(&dir,&inside)})).unwrap();assert_eq!(saved["audioPath"].as_str().unwrap(),inside.to_str().unwrap());assert_eq!(fs::read(&inside).unwrap(),b"local reference");
 let wrong=t.path().join("other-project");fs::create_dir(&wrong).unwrap();let mut bad=request(&wrong,&inside);bad["characterDirectory"]=json!(wrong);assert!(call(&app,&rec,"voice_attach_existing",json!({"request":bad})).is_err());assert_eq!(fs::read_dir(&wrong).unwrap().count(),0);session::close();
}

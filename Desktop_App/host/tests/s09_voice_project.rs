use serde_json::json;
use std::fs;
use workbench_host::{ctx::HostCtx, session, voice::RecorderState, workflow::{import,state::ProjectState}};
#[test]
fn direct_voice_authorization_requires_exact_open_project_root(){
 let t=tempfile::tempdir().unwrap();let root=t.path().join("project");fs::create_dir(&root).unwrap();let mut state=ProjectState::new();state.profile=Some(Default::default());import::persist_state(&root,&state).unwrap();session::open(root.to_str().unwrap()).unwrap();let app=HostCtx::new(t.path().join("settings"));let rec=RecorderState::new();
 for (op,model,input) in [("elevenlabs_tts_preview","eleven_multilingual_v2",r#"{"text":"local sample","voice_id":"fixture"}"#),("elevenlabs_design_voice","eleven_multilingual_ttv_v2",r#"{"text":"local sample","voice_description":"fixture"}"#),("elevenlabs_create_designed_voice","text-to-voice/create",r#"{"voice_name":"Mira","voice_description":"fixture","generated_voice_id":"candidate"}"#)]{
  let req=json!({"projectRoot":root,"operation":op,"model":model,"input":input});let quote=workbench_host::dispatch(&app,&rec,"elevenlabs_quote_voice_spend",&req).expect("valid open root must quote");let mut consent=req.clone();consent["requestHash"]=quote["requestHash"].clone();consent["ledgerRevision"]=quote["ledgerRevision"].clone();
  for wrong in [t.path().join("wrong"),root.join("Characters")]{let mut bad=req.clone();bad["projectRoot"]=json!(wrong);let before=fs::read(root.join("Preproduction/workflow_state.json")).unwrap();assert!(workbench_host::dispatch(&app,&rec,"elevenlabs_quote_voice_spend",&bad).is_err());let mut bad=consent.clone();bad["projectRoot"]=json!(wrong);assert!(workbench_host::dispatch(&app,&rec,"elevenlabs_confirm_unbounded_voice_spend",&bad).is_err());assert_eq!(fs::read(root.join("Preproduction/workflow_state.json")).unwrap(),before);}
  let token=workbench_host::dispatch(&app,&rec,"elevenlabs_confirm_unbounded_voice_spend",&consent).unwrap().as_str().unwrap().to_owned();assert_eq!(workbench_host::voice_spend::run(&root,op,model,input,Some(&token),||Ok("mock operation".into())).unwrap(),"mock operation");assert!(workbench_host::voice_spend::run(&root,op,model,input,Some(&token),||panic!("replay")).is_err());
 }
 session::close();
}

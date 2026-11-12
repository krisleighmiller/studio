use super::*;
use std::{io::{BufRead, BufReader, Read, Write}, net::TcpListener, sync::{Arc, Mutex}};
use workbench_host::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import::{persist_state, load_state}, state::ProjectState, spend}};
pub(super) static SESSION: Mutex<()> = Mutex::new(());

struct Fake { url: String, calls: Arc<Mutex<Vec<Value>>>, failure: Arc<Mutex<bool>>, delivery: Arc<Mutex<Option<(std::path::PathBuf,std::path::PathBuf)>>> }
impl Fake {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let calls = Arc::new(Mutex::new(Vec::new()));
        let failure = Arc::new(Mutex::new(false));
        let delivery: Arc<Mutex<Option<(std::path::PathBuf,std::path::PathBuf)>>> = Arc::new(Mutex::new(None));
        let (c,f,d) = (calls.clone(),failure.clone(),delivery.clone());
        std::thread::spawn(move || for connection in listener.incoming() {
            let mut stream = connection.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new(); reader.read_line(&mut first).unwrap();
            let mut length = 0;
            loop { let mut line=String::new(); reader.read_line(&mut line).unwrap(); if line.trim().is_empty(){break} if let Some((k,v))=line.split_once(':'){if k.eq_ignore_ascii_case("content-length"){length=v.trim().parse().unwrap();}} }
            let mut bytes=vec![0;length]; reader.read_exact(&mut bytes).unwrap();
            let response = if first.starts_with("POST /v1/models/fake/sheet/predictions ") {
                c.lock().unwrap().push(serde_json::from_slice(&bytes).unwrap());
                json!({"id":"fake-sheet", "status":"starting", "output":null})
            } else if first.starts_with("GET /v1/predictions/fake-sheet ") {
                if *f.lock().unwrap() { json!({"id":"fake-sheet","status":"failed","error":"explicit fake failure"}) }
                else { if let Some((src,dst))=d.lock().unwrap().take(){fs::create_dir_all(dst.parent().unwrap()).unwrap();fs::copy(src,dst).unwrap();} json!({"id":"fake-sheet","status":"succeeded","output":["https://fake.invalid/sheet.png"]}) }
            } else { panic!("Unexpected provider request: {first}") };
            let body=response.to_string(); write!(stream,"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        Self{url,calls,failure,delivery}
    }
    fn count(&self)->usize{self.calls.lock().unwrap().len()}
}
struct Harness { app:HostCtx, recorder:RecorderState, root:std::path::PathBuf, fake:Fake, visual_text: String, _dir:tempfile::TempDir }
impl Harness {
    fn new(root:&Path, refs:bool)->Self {
        let dir=tempfile::tempdir().unwrap(); let app=HostCtx::new(dir.path().to_path_buf()); let fake=Fake::new();
        fs::write(app.settings_path(),json!({"apis":[{"id":"fake","name":"Explicit fake only","kind":"replicate","baseUrl":fake.url,"apiKey":"fake-key","enabled":true}],"capabilityDefaults":{"image.text_to_image":{"apiId":"fake","modelId":"fake/sheet"},"image.character_reference":{"apiId":"fake","modelId":"fake/sheet"}}}).to_string()).unwrap();
        let mut state=ProjectState::default(); state.profile=Some(Default::default());
        state.characters=serde_json::from_value(json!([{"id":"alice","name":"Alice","dir":"Characters/Alice","profile_path":"Characters/Alice/character.yaml","identity_ready":false,"voice_ready":false,"reference_asset_ids":if refs{vec!["ref"]}else{vec![]}}])).unwrap();
        if refs {state.assets=serde_json::from_value(json!([{"id":"ref","path":"Characters/Alice/ref.png","role":"identity_reference","status":"present","character_id":"alice"}])).unwrap();}
        persist_state(root,&state).unwrap();
        let h=Self{app,recorder:RecorderState::new(),root:root.to_path_buf(),fake,visual_text: "Original identity brief".into(),_dir:dir}; h.call("open_project",json!({"path":root})).unwrap();h
    }
    fn call(&self,cmd:&str,mut args:Value)->Result<Value,String>{args["projectRoot"]=json!(self.root);dispatch(&self.app,&self.recorder,cmd,&args)}
    fn req(&self)->Value {let mut inputs=request(); inputs["identity_text"]=json!(self.visual_text); if !load_state(&self.root).unwrap().characters[0].reference_asset_ids.is_empty(){inputs["reference_images"]=json!(["Characters/Alice/ref.png"]);} json!({"characterId":"alice","action":"run_specialist","specialist":"model_sheet","inputs":inputs})}
    fn start(&self)->Value{self.call("start_character_model_sheet",json!({"request":self.req()})).unwrap()}
    fn id(card:&str)->String{card.split("[[workbench_approve:").nth(1).unwrap().split("]]").next().unwrap().into()}
    fn approve(&self,card:&str)->Value{self.call("director_approve",json!({"id":Self::id(card)})).unwrap()}
    fn consent(&self, path:&str)->Result<Value,String> {
        let manifest=disk_manifest(&self.root);
        let shot=manifest["shots"].as_array().unwrap().iter().find(|s|s["path"]==path).unwrap();
        let identity = if self.visual_text.trim().is_empty() { "Use the selected reference images as the visual source. Preserve the depicted character appearance across views." } else { &self.visual_text };
        let prompt=format!("{identity}\n\nStyle: {}.\nWardrobe: simple solid-color form-fitting top and pants suitable for turnarounds.\nShot: {}.\nSeamless chroma-green backdrop (#00B140), even neutral studio lighting, no scenic background, no props, no furniture, no logos, no readable text, no other people. Camera appropriate to the shot; production still for character continuity.\nNegative: extra people, background scenery, beauty filters that change identity, deformed hands, cropped limbs on full-body shots, watermark, text overlays.",manifest["style"].as_str().unwrap(),shot["framing"].as_str().unwrap());
        let mut params=json!({"prompt":prompt,"aspect_ratio":manifest["aspect_ratio"],"resolution":manifest["resolution"],"output_format":"png"});
        let refs=manifest["identity_source"]["reference_images"].as_array().unwrap();
        if !refs.is_empty(){params["images"]=json!(refs);params["image"]=refs[0].clone();params["image_url"]=refs[0].clone();params["media_files"]=json!(refs);}
        let args=json!({"model":"fake/sheet","api_id":"fake","params":params,"save_path":path,"wait_seconds":240});
        let hash=spend::fingerprint(&serde_json::to_vec(&json!(["atlas_generate_image","replicate",args])).unwrap()).unwrap();
        let quote=spend::quote_unbounded_scoped(&self.root,&hash).unwrap();
        let token=self.call("confirm_unbounded_spend",json!({"provider":"replicate","category":"media","operation":"atlas_generate_image","model":"fake/sheet","requestHash":quote.request_hash,"ledgerRevision":quote.ledger_revision})).unwrap();
        self.call("media_generate_confirmed",json!({"operation":"atlas_generate_image","mediaArgs":args,"token":token}))
    }
}
impl Drop for Harness {fn drop(&mut self){let _=self.call("close_project",json!({}));}}

#[test]
fn pack_repair_reference_twelve_unique_poses_resume_without_repeating_first() {
    let _lock=SESSION.lock().unwrap_or_else(|e|e.into_inner());
    fixture(|root| {
        let h=Harness::new(root,true);
        let mut card=h.start()["pendingAction"].as_str().unwrap().to_string();
        let mut paths=std::collections::HashSet::new();
        for i in 0..12 {
            h.approve(&card);
            let m=disk_manifest(root);
            assert_eq!(m["shots"].as_array().unwrap().iter().filter(|s|s["status"]=="saved").count(),i);
            let path=m["shots"][i]["path"].as_str().unwrap().to_string();
            assert!(paths.insert(path.clone()),"must not repeat saved pose");
            *h.fake.delivery.lock().unwrap()=Some((root.join("Characters/Alice/ref.png"),path.clone().into()));
            let result=h.consent(&path).unwrap();
            assert_eq!(h.fake.count(),i+1);
            if i < 11 {
                card=result["continuation"]["reply"].as_str().expect("direct callback needs continuation").to_string();
                assert!(card.contains("[[workbench_approve:"),"{result}");
                if i==0 { h.call("director_cancel",json!({})).unwrap(); card=h.start()["pendingAction"].as_str().unwrap().to_string(); }
            }
        }
        assert_eq!(paths.len(),12);
        assert!(disk_manifest(root)["shots"].as_array().unwrap().iter().all(|s|s["status"]=="saved"));
    });
}

#[test]
fn ms03_fake_text_plate_settles_manifest_and_requires_rest_approval(){
    let _lock=SESSION.lock().unwrap_or_else(|e|e.into_inner());
    fixture(|root|{
        let h=Harness::new(root,false);
        let card=h.start();let gate=h.approve(card["pendingAction"].as_str().unwrap());
        assert!(gate["reply"].as_str().unwrap().contains("awaiting spend authorization"),"{gate}");assert_eq!(h.fake.count(),0);
        let before=disk_manifest(root);let path=before["shots"][0]["path"].as_str().unwrap().to_string();
        assert_eq!(path,root.join("Characters/Alice/model_sheet/alice_closeup_front.png").to_str().unwrap());
        *h.fake.delivery.lock().unwrap()=Some((root.join("Characters/Alice/ref.png"),path.clone().into()));
        let result=h.consent(&path).unwrap();assert_eq!(h.fake.count(),1,"{result}");
        let after=disk_manifest(root);assert_eq!(after["shots"][0]["status"],"saved");
        assert!(after["shots"].as_array().unwrap()[1..].iter().all(|s|s["status"]=="held"));
        assert_eq!(after["identity_locked"],false);
        assert_eq!(fs::read(&path).unwrap(),fs::read(root.join("Characters/Alice/ref.png")).unwrap());
        assert!(fs::read_to_string(root.join("Characters/Alice/character.yaml")).unwrap().contains(&path));
        assert!(h.call("director_approve",json!({"id":Harness::id(card["pendingAction"].as_str().unwrap())})).is_err());assert_eq!(h.fake.count(),1);
        let rest=result["continuation"]["reply"].as_str().unwrap();assert!(rest.contains("Review it"),"{result}");
        h.approve(rest);assert_eq!(h.fake.count(),1,"rest approval must still stop at exact media consent");
        let unlocked=disk_manifest(root);assert_eq!(unlocked["identity_locked"],true);
        assert_eq!(unlocked["identity_source"]["reference_images"],json!([path]));
        assert_eq!(unlocked["shots"][1]["status"],"pending");
        let next_path=unlocked["shots"][1]["path"].as_str().unwrap().to_string();
        *h.fake.delivery.lock().unwrap()=Some((root.join("Characters/Alice/ref.png"),next_path.clone().into()));
        h.consent(&next_path).unwrap();assert_eq!(h.fake.count(),2);
        assert_eq!(disk_manifest(root)["shots"][1]["status"],"saved");
        assert!(fs::read_to_string(root.join("Characters/Alice/character.yaml")).unwrap().contains(&next_path));
    });
}

#[test]
fn ms03_fake_failure_retry_preserves_exact_refs_and_one_generation_per_consent(){
    let _lock=SESSION.lock().unwrap_or_else(|e|e.into_inner());
    fixture(|root|{
        let h=Harness::new(root,true);
        let card=h.start(); h.approve(card["pendingAction"].as_str().unwrap()); assert_eq!(h.fake.count(),0);
        let path=disk_manifest(root)["shots"][0]["path"].as_str().unwrap().to_string();
        *h.fake.failure.lock().unwrap()=true;
        assert!(h.consent(&path).unwrap_err().contains("provider-failed:"));
        assert_eq!(h.fake.count(),1);assert!(!Path::new(&path).exists());assert_eq!(disk_manifest(root)["shots"][0]["status"],"pending");
        *h.fake.failure.lock().unwrap()=false;
        let failed=disk_manifest(root);assert_eq!(failed["shots"][0]["attempts"][0]["path"],path);
        let retry_path=failed["shots"][0]["path"].as_str().unwrap().to_string();assert_ne!(retry_path,path);
        assert_eq!(retry_path,Path::new(&path).with_file_name("alice_closeup_front_retry1.png").to_str().unwrap());
        let card=h.start();h.approve(card["pendingAction"].as_str().unwrap());assert_eq!(h.fake.count(),1);
        *h.fake.delivery.lock().unwrap()=Some((root.join("Characters/Alice/ref.png"),retry_path.clone().into()));
        h.consent(&retry_path).unwrap(); assert_eq!(h.fake.count(),2);
        assert_eq!(disk_manifest(root)["shots"][0]["status"],"saved");
        let calls=h.fake.calls.lock().unwrap();assert_eq!(calls[0],calls[1]);
        let input=&calls[1]["input"];
        use base64::Engine;
        assert_eq!(input["images"],json!([root.join("Characters/Alice/ref.png")]));
        let encoded=input["image"].as_str().unwrap().strip_prefix("data:image/png;base64,").unwrap();
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(encoded).unwrap(),fs::read(root.join("Characters/Alice/ref.png")).unwrap());
        assert_eq!(input["image"],input["image_url"]);
        assert!(input["prompt"].as_str().unwrap().starts_with("Original identity brief\n\nStyle:"));
        assert!(!input["prompt"].as_str().unwrap().contains("Character sheet canon"));
    });
}

#[test]
fn ms03_minimal_visual_inputs_run_with_fake_transport_and_no_biography() {
    let _lock=SESSION.lock().unwrap_or_else(|e|e.into_inner());
    for refs in [false, true] {
        fixture(|root| {
            fs::write(root.join("Characters/Alice/character.yaml"), "name: Alice\nbiography: PRIVATE BIOGRAPHY NOT VISUAL INPUT\n").unwrap();
            let mut h=Harness::new(root,refs);
            h.visual_text = if refs { "" } else { "Silver bob, blue coat" }.into();
            let card=h.start(); let approval=h.approve(card["pendingAction"].as_str().unwrap());
            assert!(approval["reply"].as_str().unwrap().contains("awaiting spend authorization"));
            assert_eq!(h.fake.count(),0);
            let path=disk_manifest(root)["shots"][0]["path"].as_str().unwrap().to_string();
            *h.fake.delivery.lock().unwrap()=Some((root.join("Characters/Alice/ref.png"),path.clone().into()));
            let result=h.consent(&path).unwrap();
            assert_eq!(h.fake.count(),1,"{result}");
            let calls=h.fake.calls.lock().unwrap();
            let prompt=calls[0]["input"]["prompt"].as_str().unwrap();
            assert!(!prompt.contains("PRIVATE BIOGRAPHY"));
            assert!(prompt.starts_with(if refs { "Use the selected reference images" } else { "Silver bob, blue coat" }));
            let manifest=disk_manifest(root);
            assert_eq!(manifest["preparation_inputs"]["identity_text"],h.visual_text);
            assert_eq!(manifest["shots"][0]["status"],"saved");
            if !refs { assert_eq!(manifest["shots"][1]["status"],"held"); }
        });
    }
}

#[test]
fn ms03_uncertain_plate_is_not_regenerated_or_reported_complete(){
    let _lock=SESSION.lock().unwrap_or_else(|e|e.into_inner());
    fixture(|root|{
        let h=Harness::new(root,false);let card=h.start();h.approve(card["pendingAction"].as_str().unwrap());
        let mut manifest=disk_manifest(root);manifest["shots"][0]["status"]=json!("uncertain");save_disk(root,&manifest);
        let card=h.start();let result=h.call("director_approve",json!({"id":Harness::id(card["pendingAction"].as_str().unwrap())}));
        assert!(result.is_err(),"{result:?}");assert_eq!(h.fake.count(),0);
    });
}

#[test]
fn ms03_stop_after_card_approval_refuses_late_media_consent(){
    let _lock=SESSION.lock().unwrap_or_else(|e|e.into_inner());
    fixture(|root|{
        let h=Harness::new(root,false);let card=h.start();h.approve(card["pendingAction"].as_str().unwrap());
        let path=disk_manifest(root)["shots"][0]["path"].as_str().unwrap().to_string();
        h.call("director_cancel",json!({})).unwrap();
        assert!(h.consent(&path).is_err());assert_eq!(h.fake.count(),0);assert!(!Path::new(&path).exists());
    });
}

#[test]
fn ms03_denial_cancellation_and_validation_do_not_call_provider(){
    let _lock=SESSION.lock().unwrap_or_else(|e|e.into_inner());
    fixture(|root|{
        let h=Harness::new(root,false);
        for (field,value) in [("specialist",json!("still_image")),("characterId",json!("missing"))]{let mut req=h.req();req[field]=value;assert!(h.call("start_character_model_sheet",json!({"request":req})).is_err());}
        for (field,value) in [("identity_text",json!(" ")), ("output_path",json!("../elsewhere")), ("character_files",json!(["Characters/Bob/character.yaml"])), ("reference_images",json!(["Characters/Alice/ref.png"])), ("spend_confirmation",json!("not-consent"))] {
            let mut req=h.req();req["inputs"][field]=value;
            assert!(h.call("start_character_model_sheet",json!({"request":req})).is_err(),"{field}");
        }
        assert!(!manifest_path(root).exists());
        let card=h.start(); assert_eq!(card["status"],"pendingApproval");assert_eq!(h.fake.count(),0);
        let id=Harness::id(card["pendingAction"].as_str().unwrap());
        h.call("director_cancel",json!({})).unwrap();
        assert!(h.call("director_approve",json!({"id":id})).is_err());assert_eq!(h.fake.count(),0);
    });
}

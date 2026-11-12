//! Offline adapter acceptance: public-schema-shaped fixtures, synthetic URLs/receipt only.
#![cfg(feature = "mock-provider")]
use serde_json::{json, Value};
use std::{io::{Read, Write}, net::{TcpListener, TcpStream}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, thread, time::Duration};
use crate::atlas;

const MODEL: &str = "bytedance/seedream-v5.0-lite/sequential";
const EDIT_MODEL: &str = "bytedance/seedream-v5.0-lite/edit-sequential";
struct Fixture { base: String, requests: Arc<Mutex<Vec<(String, Value)>>>, stop: Arc<AtomicBool>, worker: Option<thread::JoinHandle<()>> }
impl Fixture {
    fn new(schema: Option<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let schema_url = format!("{base}/schema");
        let requests = Arc::new(Mutex::new(Vec::new())); let log = requests.clone();
        let stop = Arc::new(AtomicBool::new(false)); let done = stop.clone();
        let worker = thread::spawn(move || while !done.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    let (route, body) = read_request(&mut stream);
                    log.lock().unwrap().push((route.clone(), body));
                    let response = if route == "GET /api/v1/models" {
                        let mut entry = json!({"model":MODEL});
                        if schema.is_some() { entry["schema"] = json!(schema_url); }
                        let mut edit_entry=entry.clone(); edit_entry["model"]=json!(EDIT_MODEL);
                        json!({"data":[entry,edit_entry]})
                    } else if route == "GET /schema" { schema.clone().unwrap() }
                    else { json!({"data":{"id":"synthetic-local-only","status":"starting"}}) };
                    // String fixtures deliberately serve invalid raw JSON to cover parse failure.
                    let text = response.as_str().map(str::to_string).unwrap_or_else(|| response.to_string());
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", text.len(), text).unwrap();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(2)),
                Err(e) => panic!("{e}"),
            }
        });
        std::env::set_var("WORKBENCH_ATLAS_BASE_URL", format!("{base}/api/v1"));
        Self { base, requests, stop, worker: Some(worker) }
    }
    fn posts(&self) -> Vec<Value> { self.requests.lock().unwrap().iter().filter(|(r,_)| r.starts_with("POST ")).map(|(_,v)|v.clone()).collect() }
}
impl Drop for Fixture { fn drop(&mut self) { self.stop.store(true, Ordering::SeqCst); self.worker.take().unwrap().join().unwrap(); std::env::remove_var("WORKBENCH_ATLAS_BASE_URL"); } }
fn read_request(s: &mut TcpStream) -> (String, Value) {
    let mut bytes = Vec::new(); let mut buf = [0;4096];
    let end = loop { let n=s.read(&mut buf).unwrap(); assert!(n>0); bytes.extend_from_slice(&buf[..n]); if let Some(i)=bytes.windows(4).position(|w|w==b"\r\n\r\n") { break i+4; } };
    let headers=String::from_utf8_lossy(&bytes[..end]).to_string();
    let len=headers.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v|v.trim().parse::<usize>().unwrap())).unwrap_or(0);
    while bytes.len()<end+len { let n=s.read(&mut buf).unwrap(); assert!(n>0); bytes.extend_from_slice(&buf[..n]); }
    let route=headers.lines().next().unwrap().split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    let body=if len==0 {Value::Null} else {serde_json::from_slice(&bytes[end..end+len]).unwrap_or(json!("synthetic non-JSON upload body"))}; (route,body)
}
fn text_schema() -> Value { json!({"properties":{"prompt":{"type":"string"},"size":{"type":"string"},"max_images":{"type":"integer"},"output_format":{"type":"string"},"enable_base64":{"type":"boolean"}},"required":["prompt"]}) }

#[test]
fn reference_delivery_offline_acceptance() {
    let _lock = super::MOCK_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Wrong behavior: schema filtering erases images and successfully POSTs prompt only.
    let f=Fixture::new(Some(text_schema()));
    let result=atlas::generate_image("synthetic-not-a-key", MODEL, &json!({"prompt":"fixture", "images":["https://fixture.invalid/brown-front.png"]}));
    assert!(result.is_err(), "must reject reference loss, got {result:?}; captured {:?}", f.posts());
    assert!(f.posts().is_empty(), "unsupported reference model must not submit");
    let error=result.unwrap_err(); assert!(error.contains(MODEL)); assert!(!error.contains("brown-front.png"));
    assert!(error.contains("Character reference"));
    assert!(f.base.starts_with("http://127.0.0.1:"));
    drop(f);

    // The exact FINAL HTTP POST, not an intermediate map, is the oracle.
    let refs = json!(["https://fixture.invalid/Z-front.png?sig=A%2Fb", "https://fixture.invalid/a-left.png?sig=C%2Bd"]);
    for (field, spec, values) in [
        ("images", json!({"type":"array","items":{"type":"string"},"minItems":1,"maxItems":14}), refs.clone()),
        ("reference_images", json!({"type":"array","items":{"type":"string"}}), refs.clone()),
        ("image_url", json!({"type":"string"}), json!([refs[0].clone()])),
    ] {
        let mut schema=text_schema(); schema["properties"][field]=spec;
        let f=Fixture::new(Some(schema));
        atlas::generate_image("synthetic-not-a-key", EDIT_MODEL, &json!({"prompt":"fixture","images":values,"image":values[0],"media_files":values})).unwrap();
        assert_eq!(f.posts().len(),1);
        assert!(f.requests.lock().unwrap().iter().any(|(route, _)| route == "POST /api/v1/model/generateImage"));
        assert_eq!(f.posts()[0][field],if field=="image_url" {values[0].clone()} else {values});
        println!("supported final wire: {}",f.posts()[0]);
    }
    for schema in [None, Some(json!({})), Some(json!("malformed schema")), Some(json!({"properties":{"image":{"type":"string"}}})), Some(json!({"properties":{"images":{"type":"array","items":{"type":"string"},"maxItems":1}}}))] {
        let f=Fixture::new(schema);
        assert!(atlas::generate_image("synthetic-not-a-key",MODEL,&json!({"prompt":"fixture","images":refs})).is_err());
        assert!(f.posts().is_empty());
    }
    for schema in [Some(text_schema()),None] {
        let f=Fixture::new(schema);
        atlas::generate_image("synthetic-not-a-key",MODEL,&json!({"prompt":"text only"})).unwrap();
        assert_eq!(f.posts(),vec![json!({"model":MODEL,"prompt":"text only"})]);
    }
    // Duplicate entries are still entries; normalization must not shrink them.
    {
        let mut schema=text_schema(); schema["properties"]["images"]=json!({"type":"array","items":{"type":"string"},"maxItems":14});
        let f=Fixture::new(Some(schema));
        let duplicate=json!([refs[0],refs[0],refs[1]]);
        atlas::generate_image("synthetic-not-a-key",EDIT_MODEL,&json!({"prompt":"duplicate fixture","images":duplicate,"media_files":duplicate})).unwrap();
        assert_eq!(f.posts()[0]["images"],duplicate,"reference multiplicity must survive aliases");
        assert!(atlas::generate_image("synthetic-not-a-key",MODEL,&json!({"prompt":"invalid fixture","images":[refs[0],"not-a-url"]})).is_err());
        assert_eq!(f.posts().len(),1);
    }
    native_unsupported_stops_before_upload();
    look_unsupported_stops_before_upload();
}

fn look_unsupported_stops_before_upload() {
    use crate::{ctx::HostCtx, dispatch, voice::RecorderState, workflow::{import::persist_state, state::ProjectState}};
    let f=Fixture::new(Some(text_schema()));
    let dir=tempfile::tempdir().unwrap(); let root=dir.path();
    std::fs::write(root.join("reference.png"),b"fixture image").unwrap();
    let state=ProjectState::from_json(&json!({"version":2,"scenes":[{"id":1,"title":"Room","look_board":{"slots":[{"id":"slot","role":"interior_reference","title":"Room","prompt":"A room","asset_ids":[]}]}}],"assets":[{"id":"ref","path":"reference.png","role":"interior_reference","status":"draft","scene_id":1}]}).to_string()).unwrap();
    persist_state(root,&state).unwrap();
    let app=HostCtx::new(root.join("settings")); std::fs::create_dir_all(root.join("settings")).unwrap();
    std::fs::write(app.settings_path(),json!({"apis":[{"id":"fixture","name":"Synthetic local only","kind":"atlas-cloud","apiKey":"synthetic-not-a-key","enabled":true}],"capabilityDefaults":{"image.text_to_image":{"apiId":"fixture","modelId":MODEL}}}).to_string()).unwrap();
    let rec=RecorderState::new(); dispatch(&app,&rec,"open_project",&json!({"path":root})).unwrap();
    let card=dispatch(&app,&rec,"workflow_generate_look",&json!({"projectRoot":root,"sceneId":1,"slotId":"slot","imageCapability":"image.text_to_image","referenceAssetIds":["ref"]})).unwrap();
    let text=card["pendingAction"].as_str().unwrap(); let id=text.split("[[workbench_approve:").nth(1).unwrap().split("]]").next().unwrap();
    let result=dispatch(&app,&rec,"director_approve",&json!({"id":id}));
    let text=format!("{result:?}"); assert!(text.contains("reference"),"{text}");
    assert!(f.posts().is_empty(),"no upload or generation POST: {:?}",f.posts());
    assert!(f.requests.lock().unwrap().iter().any(|(r,_)|r=="GET /schema"),"schema preflight must run");
    dispatch(&app,&rec,"close_project",&json!({})).unwrap();
}

fn native_unsupported_stops_before_upload() {
    use crate::{ctx::HostCtx,dispatch,voice::RecorderState,workflow::{import::persist_state,state::ProjectState}};
    let f=Fixture::new(Some(text_schema()));
    let dir=tempfile::tempdir().unwrap(); let root=dir.path();
    std::fs::create_dir_all(root.join("Characters/Alice")).unwrap();
    std::fs::write(root.join("Characters/Alice/character.yaml"),"name: Alice\n").unwrap();
    let output=std::process::Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","color=red:s=16x16","-frames:v","1","-threads","1","-y"]).arg(root.join("Characters/Alice/ref.png")).output().unwrap();
    assert!(output.status.success());
    let mut state=ProjectState::default(); state.profile=Some(Default::default());
    state.characters=serde_json::from_value(json!([{"id":"alice","name":"Alice","dir":"Characters/Alice","profile_path":"Characters/Alice/character.yaml","identity_ready":false,"voice_ready":false,"reference_asset_ids":["ref"]}])).unwrap();
    state.assets=serde_json::from_value(json!([{"id":"ref","path":"Characters/Alice/ref.png","role":"identity_reference","status":"present","character_id":"alice"}])).unwrap();
    persist_state(root,&state).unwrap();
    let app=HostCtx::new(root.join("settings")); std::fs::create_dir_all(root.join("settings")).unwrap();
    std::fs::write(app.settings_path(),json!({"apis":[{"id":"fixture","name":"Synthetic local only","kind":"atlas-cloud","apiKey":"synthetic-not-a-key","enabled":true}],"capabilityDefaults":{"image.character_reference":{"apiId":"fixture","modelId":MODEL}}}).to_string()).unwrap();
    let rec=RecorderState::new();
    dispatch(&app,&rec,"open_project",&json!({"path":root})).unwrap();
    let req=json!({"characterId":"alice","action":"run_specialist","specialist":"model_sheet","inputs":{"output_path":"Characters/Alice/model_sheet","character_files":["Characters/Alice/character.yaml"],"character_slug":"Alice","identity_text":"","reference_images":["Characters/Alice/ref.png"]}});
    let card=dispatch(&app,&rec,"start_character_model_sheet",&json!({"projectRoot":root,"request":req})).unwrap();
    let card=card["pendingAction"].as_str().unwrap(); let id=card.split("[[workbench_approve:").nth(1).unwrap().split("]]").next().unwrap();
    // A rejected initial approval must not even reach schema preflight.
    dispatch(&app,&rec,"director_cancel",&json!({})).unwrap();
    assert!(dispatch(&app,&rec,"director_approve",&json!({"id":id})).is_err());
    assert!(f.requests.lock().unwrap().is_empty());
    let req=json!({"characterId":"alice","action":"run_specialist","specialist":"model_sheet","inputs":{"output_path":"Characters/Alice/model_sheet","character_files":["Characters/Alice/character.yaml"],"character_slug":"Alice","identity_text":"","reference_images":["Characters/Alice/ref.png"]}});
    let card=dispatch(&app,&rec,"start_character_model_sheet",&json!({"projectRoot":root,"request":req})).unwrap();
    let card=card["pendingAction"].as_str().unwrap(); let id=card.split("[[workbench_approve:").nth(1).unwrap().split("]]").next().unwrap();
    let result=dispatch(&app,&rec,"director_approve",&json!({"id":id}));
    let text=format!("{result:?}");
    assert!(text.contains("Character reference"),"native error must explain incompatible selected model: {text}");
    println!("native approval reply (synthetic fixture): {text}");
    assert!(f.posts().is_empty(),"preflight must precede all upload/generation POSTs: {:?}",f.posts());
    assert!(f.requests.lock().unwrap().iter().any(|(r,_)|r=="GET /schema"));
    assert!(!root.join("Characters/Alice/model_sheet/alice_closeup_front.png").exists());
    dispatch(&app,&rec,"close_project",&json!({})).unwrap();
}

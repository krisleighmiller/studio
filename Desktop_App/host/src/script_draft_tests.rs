use serde_json::{json, Value};
use std::{fs};
use crate::{ctx::HostCtx, dispatch, voice::RecorderState};

fn project(test: impl FnOnce(&std::path::Path, &HostCtx, &RecorderState)) {
    let _lock = crate::session::test_session_lock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project"); fs::create_dir(&root).unwrap();
    let app = HostCtx::new(dir.path().join("settings")); let recorder = RecorderState::new();
    fs::write(app.settings_path(), json!({"apis":[{"id":"local","name":"Local fixture","kind":"openai-chat","baseUrl":"http://127.0.0.1:9","enabled":true}],"selectedChatModel":{"apiId":"local","modelId":"writer"}}).to_string()).unwrap();
    dispatch(&app,&recorder,"open_project",&json!({"path":root})).unwrap();
    test(&root,&app,&recorder);
    dispatch(&app,&recorder,"close_project",&json!({})).unwrap();
}
fn args(root: &std::path::Path) -> Value { json!({"projectRoot":root,"title":"Rain","notes":"  Mara meets a stranger.\n","apiId":"local","modelId":"writer","replaceExisting":true}) }
#[test]
fn notes_draft_disclosure_keeps_endpoint_routing_path_without_url_secrets() {
    project(|root,app,recorder| {
        use_endpoint(app,"http://fixture-user:fixture-password@127.0.0.1:9/tenant/creative/v1?token=fixture-secret#private");
        let prepared = dispatch(app,recorder,"prepare_script_draft",&args(root)).unwrap();
        let message = prepared["message"].as_str().unwrap();
        assert!(message.contains("Endpoint: http://127.0.0.1:9/tenant/creative/v1"),"{message}");
        for secret in ["fixture-user", "fixture-password", "fixture-secret", "#private"] {
            assert!(!message.contains(secret),"URL secret disclosed: {secret}");
        }
        assert!(message.contains("query/fragment omitted"));
        dispatch(app,recorder,"finish_script_draft",&json!({"id":prepared["id"],"approved":false})).unwrap();
        assert_eq!(fs::read_dir(root).unwrap().count(),0);
    });
}
#[test]
fn notes_draft_denial_consumes_exact_snapshot_without_transport_or_project_writes() {
    project(|root,app,recorder| {
        let prepared = dispatch(app,recorder,"prepare_script_draft",&args(root)).unwrap();
        let message = prepared["message"].as_str().unwrap();
        for text in ["Local fixture", "writer", "Title", "Notes", "Mara meets a stranger.", "replace", "cost", "Save script"] { assert!(message.contains(text), "{message} missing {text}"); }
        let denied = dispatch(app,recorder,"finish_script_draft",&json!({"id":prepared["id"],"approved":false})).unwrap();
        assert_eq!(denied,json!({"cancelled":true}));
        assert!(dispatch(app,recorder,"finish_script_draft",&json!({"id":prepared["id"],"approved":true})).is_err());
        assert_eq!(fs::read_dir(root).unwrap().count(),0);
    });
}

// Controlled loopback HTTP fixture, not a real/free/paid provider or a production fallback.
fn fake_reply(reply: &str, status: u16, before_reply: impl FnOnce() + Send + 'static) -> (String, std::thread::JoinHandle<Value>) {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}",listener.local_addr().unwrap());
    let reply = json!({"choices":[{"message":{"content":reply}}]}).to_string();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now()+Duration::from_secs(4);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket,_)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => { assert!(Instant::now()<deadline,"No fake request"); std::thread::sleep(Duration::from_millis(5)); },
                Err(e) => panic!("{e}"),
            }
        };
        socket.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
        let mut bytes=Vec::new();
        let request = loop {
            let mut buf=[0;4096]; let n=socket.read(&mut buf).unwrap(); assert!(n>0); bytes.extend_from_slice(&buf[..n]);
            if let Some(end)=bytes.windows(4).position(|w|w==b"\r\n\r\n") {
                let headers=String::from_utf8_lossy(&bytes[..end]);
                let length:usize=headers.lines().find_map(|line| { let (k,v)=line.split_once(':')?; k.eq_ignore_ascii_case("content-length").then(||v.trim().parse().unwrap()) }).unwrap();
                if bytes.len()>=end+4+length { break serde_json::from_slice::<Value>(&bytes[end+4..end+4+length]).unwrap(); }
            }
        };
        before_reply();
        write!(socket,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",reply.len(),reply).unwrap();
        request
    });
    (base,server)
}
fn use_endpoint(app:&HostCtx, base:&str) {
    let mut config:Value=serde_json::from_str(&fs::read_to_string(app.settings_path()).unwrap()).unwrap();
    config["apis"][0]["baseUrl"]=json!(base);
    fs::write(app.settings_path(),config.to_string()).unwrap();
}
#[test]
fn notes_draft_exact_transport_snapshot_then_explicit_edited_save_and_parser() {
    project(|root,app,recorder| {
        let script="Scene 1 — Rain\n- **Dialogue:**\n- **Mara:** Come inside.\n\nScene 2 — Door\n- **Action:** The stranger leaves.";
        let (base,server)=fake_reply(script,200,||{}); use_endpoint(app,&base);
        let prepared=dispatch(app,recorder,"prepare_script_draft",&args(root)).unwrap();
        // A settings edit after consent preparation must not redirect the approved request.
        use_endpoint(app,"http://127.0.0.1:9");
        let result=dispatch(app,recorder,"finish_script_draft",&json!({"id":prepared["id"],"approved":true,"notes":"tampered","modelId":"other"}));
        let request=server.join().unwrap();
        assert_eq!(result.unwrap(),json!({"script":script}));
        assert_eq!(request["model"],"writer");
        assert_eq!(request["messages"][1]["content"],"Title:\nRain\n\nNotes:\n  Mara meets a stranger.\n");
        assert_eq!(fs::read_dir(root).unwrap().count(),0,"generation is in memory only");
        let edited=script.replace("Come inside.","Keep my edited dialogue.");
        let path=dispatch(app,recorder,"save_project_script",&json!({"projectRoot":root,"filename":"Rain.md","text":edited})).unwrap();
        assert_eq!(path,"Scripts/Rain.md");
        let saved=fs::read_to_string(root.join(path.as_str().unwrap())).unwrap();
        assert_eq!(saved,edited);
        let scenes=crate::script::extract_scenes(&saved); assert_eq!(scenes.len(),2);
        assert_eq!(scenes[0].dialogue[0].text,"Keep my edited dialogue.");
        let imported=dispatch(app,recorder,"workflow_import",&json!({"projectRoot":root,"persist":true})).unwrap();
        assert!(imported["state"]["scenes"].as_array().unwrap().iter().any(|s| s["source_path"] == "Scripts/Rain.md"));
        assert!(dispatch(app,recorder,"finish_script_draft",&json!({"id":prepared["id"],"approved":true})).is_err());
    });
}
#[test]
fn notes_draft_rejects_missing_disabled_changed_or_nontext_route_without_fallback() {
    project(|root,app,recorder| {
        for case in ["missing-api","missing-model","wrong-api","disabled","nontext","no-selected"] {
            let original=fs::read_to_string(app.settings_path()).unwrap();
            let mut config:Value=serde_json::from_str(&original).unwrap(); let mut input=args(root);
            match case {
                "missing-api" => input["apiId"]=json!(""),
                "missing-model" => input["modelId"]=json!(""),
                "wrong-api" => input["apiId"]=json!("unknown"),
                "disabled" => config["apis"][0]["enabled"]=json!(false),
                "nontext" => config["apis"][0]["kind"]=json!("elevenlabs"),
                _ => config["selectedChatModel"]=Value::Null,
            }
            fs::write(app.settings_path(),config.to_string()).unwrap();
            let error=dispatch(app,recorder,"prepare_script_draft",&input).unwrap_err();
            assert!(error.contains("Settings") || error.contains("dropdown"),"{case}: {error}");
            fs::write(app.settings_path(),original).unwrap();
        }
        assert_eq!(fs::read_dir(root).unwrap().count(),0);
    });
}
#[test]
fn notes_draft_provider_failure_and_invalid_scripts_write_nothing_and_cannot_retry_snapshot() {
    project(|root,app,recorder| {
        for (script,status) in [("",200),("not a screenplay",200),("Scene 1\nA\nScene 1\nB",200),("Scene 1",200),("Scene 0\nA",200),("error",500)] {
            let (base,server)=fake_reply(script,status,||{}); use_endpoint(app,&base);
            let p=dispatch(app,recorder,"prepare_script_draft",&args(root)).unwrap();
            let result=dispatch(app,recorder,"finish_script_draft",&json!({"id":p["id"],"approved":true}));
            server.join().unwrap(); assert!(result.is_err(),"accepted {script}");
            assert!(dispatch(app,recorder,"finish_script_draft",&json!({"id":p["id"],"approved":true})).is_err());
            assert_eq!(fs::read_dir(root).unwrap().count(),0);
        }
    });
}
#[test]
fn notes_draft_close_and_reopen_same_project_during_transport_discards_late_response() {
    project(|root,app,recorder| {
        let root_owned=root.to_owned();
        let (base,server)=fake_reply("Scene 1\nLate fixture.",200,move || {
            super::clear(); crate::session::close(); crate::session::open(root_owned.to_str().unwrap()).unwrap();
        }); use_endpoint(app,&base);
        let p=dispatch(app,recorder,"prepare_script_draft",&args(root)).unwrap();
        let result=dispatch(app,recorder,"finish_script_draft",&json!({"id":p["id"],"approved":true}));
        server.join().unwrap(); assert!(result.is_err(),"late response from an old session accepted");
        assert_eq!(fs::read_dir(root).unwrap().count(),0);
    });
}


#[test]
fn notes_draft_stop_while_native_consent_is_pending_prevents_dispatch() {
    project(|root,app,recorder| {
        let p=dispatch(app,recorder,"prepare_script_draft",&args(root)).unwrap();
        app.cancel_director();
        let error=dispatch(app,recorder,"finish_script_draft",&json!({"id":p["id"],"approved":true})).unwrap_err();
        assert!(error.contains("cancelled"),"{error}");
        assert_eq!(fs::read_dir(root).unwrap().count(),0);
    });
}

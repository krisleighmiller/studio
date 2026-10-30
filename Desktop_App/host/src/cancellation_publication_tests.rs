//! The only synthetic seam is a unit-only barrier after input hashing. All
//! planning, inspection, approval preparation and cancellation use public dispatch.
use crate::{
    ctx::HostCtx, director_actions::SNAPSHOT_BARRIER, dispatch, pending, session,
    voice::RecorderState,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

fn run_preparation_stop(nested: bool) {
    let _session = session::test_session_lock();
    let _pending = pending::test_pending_lock();
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            SNAPSHOT_BARRIER.with(|b| {
                b.borrow_mut().take();
            });
            pending::cancel_all();
            session::close();
        }
    }
    let _cleanup = Cleanup;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Research")).unwrap();
    fs::write(root.join("source.md"), "COMPLETED_INSPECTION_RESULT").unwrap();
    fs::write(root.join("Research/digest.md"), "old digest").unwrap();
    crate::workflow::import::persist_state(&root, &crate::workflow::state::ProjectState::new())
        .unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let app = HostCtx::new(dir.path().join("settings"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    fs::write(app.settings_path(), json!({"apis":[{"id":"prep-loopback","name":"prep","kind":"ollama","baseUrl":format!("http://{}",listener.local_addr().unwrap()),"apiKey":"","secretKey":"","enabled":true}],"capabilityDefaults":{}}).to_string()).unwrap();
    let inspect = json!({"action":"inspect","op":"read_file","path":"source.md"});
    let proposal = if nested {
        json!({"action":"run_specialist","specialist":"production_helper","inputs":{"question":"digest source","paths":["source.md"],"mode":"digest","output_path":"Research/digest.md"}})
    } else {
        json!({"action":"run_specialist","specialist":"still_image","inputs":{"prompt":"fixture","output_path":"fixture.png"}})
    };
    let replies = if nested {
        vec![format!("<director_action>{inspect}</director_action>\n<director_action>{proposal}</director_action>"), "new digest".into()]
    } else {
        vec![format!("<director_action>{inspect}</director_action>\n<director_action>{proposal}</director_action>")]
    };
    let server = thread::spawn(move || {
        for content in replies {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some((key, val)) = line.split_once(':') {
                    if key.eq_ignore_ascii_case("content-length") {
                        length = val.trim().parse().unwrap();
                    }
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            drop(reader);
            let body = json!({"choices":[{"message":{"content":content}}]}).to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let saved_id = Arc::new(Mutex::new(None));
    let saved = saved_id.clone();
    let cancel_app = app.clone();
    let cancel_root = root.clone();
    SNAPSHOT_BARRIER.with(|b| {
        *b.borrow_mut() = Some(Box::new(move || {
            dispatch(
                &cancel_app,
                &RecorderState::new(),
                "director_cancel",
                &json!({}),
            )
            .unwrap();
            let unrelated =
                serde_json::from_value(json!({"action":"done","message":"unrelated parked work"}))
                    .unwrap();
            *saved.lock().unwrap() = Some(pending::park(unrelated, Some(cancel_root), None, None));
        }))
    });
    let response = dispatch(&app, &RecorderState::new(), "director_turn", &json!({"request":{"model":"fixture","apiId":"prep-loopback","messages":[{"role":"user","content":"Inspect then prepare this fixture action."}],"context":{"projectRoot":root}}})).unwrap();
    server.join().unwrap();
    println!("PREPARATION_STOP={response}");
    assert_eq!(response["stopped"], true);
    assert_eq!(response["continueSuggested"], false);
    let id = saved_id
        .lock()
        .unwrap()
        .take()
        .expect("snapshot barrier executed");
    assert!(
        pending::take(&id).is_ok(),
        "late approval publication erased unrelated pending action"
    );
    assert!(
        pending::current().is_none(),
        "cancelled approval leaked into authoritative pending store"
    );
    assert!(
        response["reply"]
            .as_str()
            .unwrap()
            .contains("COMPLETED_INSPECTION_RESULT"),
        "completed inspection lost: {response}"
    );
    assert!(!response["reply"]
        .as_str()
        .unwrap()
        .contains(pending::APPROVE_TOKEN_PREFIX));
    assert_eq!(
        fs::read_to_string(root.join("Research/digest.md")).unwrap(),
        "old digest"
    );
    let events: Vec<Value> = response["toolEvents"].as_array().unwrap().clone();
    assert!(events.iter().any(|v| v == "inspect:read_file"));
    assert!(!events
        .iter()
        .any(|v| v.as_str().unwrap().starts_with("approval_required:")));
}
#[test]
fn stop_during_top_level_approval_preparation_preserves_pending_and_completed_inspection() {
    run_preparation_stop(false);
}
#[test]
fn stop_during_nested_approval_preparation_preserves_pending_and_completed_inspection() {
    run_preparation_stop(true);
}

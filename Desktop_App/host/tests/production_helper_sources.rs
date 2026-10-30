//! Real specialist + loopback HTTP transport; never contacts a live provider.
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard,
};
use std::time::Duration;
use workbench_host::{
    ctx::HostCtx,
    director::DirectorContext,
    director_actions::ValidatedAction,
    session,
    settings::ApiEntry,
    specialists::{run_specialist, SpecialistLlm, SpecialistResult},
};

static SESSION: Mutex<()> = Mutex::new(());
struct Fixture {
    _lock: MutexGuard<'static, ()>,
    dir: tempfile::TempDir,
    app: HostCtx,
}
impl Fixture {
    fn new() -> Self {
        let lock = SESSION.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        session::open(dir.path().to_str().unwrap()).unwrap();
        let app = HostCtx::new(dir.path().join("settings"));
        Self {
            _lock: lock,
            dir,
            app,
        }
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn run(
        &self,
        inputs: Value,
        context: DirectorContext,
    ) -> (Result<SpecialistResult, String>, Vec<Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let api = ApiEntry {
            id: "loopback".into(),
            name: "Loopback".into(),
            kind: "atlas-cloud".into(),
            base_url: format!("http://{}", listener.local_addr().unwrap()),
            api_key: String::new(),
            secret_key: String::new(),
            enabled: true,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            loop {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut bytes = Vec::new();
                        loop {
                            let mut buffer = [0u8; 4096];
                            let n = socket.read(&mut buffer).unwrap();
                            assert!(n > 0);
                            bytes.extend_from_slice(&buffer[..n]);
                            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                                let headers = String::from_utf8_lossy(&bytes[..end]);
                                let length: usize = headers
                                    .lines()
                                    .find_map(|line| {
                                        let (key, value) = line.split_once(':')?;
                                        key.eq_ignore_ascii_case("content-length")
                                            .then(|| value.trim().parse().unwrap())
                                    })
                                    .unwrap();
                                if bytes.len() >= end + 4 + length {
                                    requests.push(
                                        serde_json::from_slice(&bytes[end + 4..end + 4 + length])
                                            .unwrap(),
                                    );
                                    break;
                                }
                            }
                        }
                        let reply = json!({"choices": [{"message": {"content": "# Digest\nGrounded review."}}]}).to_string();
                        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if server_stop.load(Ordering::SeqCst) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(e) => panic!("loopback accept: {e}"),
                }
            }
            requests
        });
        let validated = ValidatedAction {
            action: serde_json::from_value(json!({"action": "run_specialist", "specialist": "production_helper", "inputs": inputs})).unwrap(),
            resolved_paths: Vec::new(),
        };
        let llm = SpecialistLlm {
            api: &api,
            api_key: "",
            model: "fixture-model",
            user_question: "Review the documents",
        };
        let result = run_specialist(
            &self.app,
            &context,
            &validated,
            self.dir.path().to_str(),
            Some(&llm),
        );
        stop.store(true, Ordering::SeqCst);
        (result, server.join().unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        session::close();
    }
}
fn user_message(requests: &[Value]) -> &str {
    assert_eq!(requests.len(), 1, "expected exactly one real HTTP request");
    requests[0]["messages"][1]["content"].as_str().unwrap()
}

#[test]
fn file_context_truncation_preserves_utf8_boundaries() {
    let f = Fixture::new();
    f.write("script.md", &format!("{}🎬END", "x".repeat(47_999)));
    let (result, requests) = f.run(json!({"paths":["script.md"]}), DirectorContext::default());
    result.unwrap();
    let user = user_message(&requests);
    assert!(user.contains("…[truncated for helper context]…"));
    assert!(!user.contains("END"));
}

#[test]
fn focused_excerpt_truncation_preserves_utf8_boundaries() {
    let f = Fixture::new();
    let context = DirectorContext {
        focused_editor_excerpt: Some(format!("{}🎬END", "x".repeat(7_999))),
        ..Default::default()
    };
    let (result, requests) = f.run(json!({}), context);
    result.unwrap();
    let user = user_message(&requests);
    assert!(user.contains("…[excerpt truncated]…"));
    assert!(!user.contains("END"));
}

#[test]
fn excess_requested_sources_fail_instead_of_silently_dropping_files() {
    let f = Fixture::new();
    let mut paths = Vec::new();
    for index in 0..10 {
        let path = format!("source-{index}.md");
        f.write(&path, "VALID SOURCE");
        paths.push(path);
    }
    paths.push("missing-eleventh.md".into());
    let (result, requests) = f.run(
        json!({"paths":paths, "output_path":"Research/new.md"}),
        DirectorContext::default(),
    );
    assert!(
        requests.is_empty(),
        "must not dispatch a silently incomplete dossier"
    );
    assert!(result.unwrap_err().contains("source"));
    assert!(!f.dir.path().join("Research/new.md").exists());
}

#[test]
fn context_fallback_keeps_sources_but_excludes_existing_output_and_excerpt() {
    let f = Fixture::new();
    f.write("script.md", "ATTACHED SCRIPT");
    f.write("prod.md", "SELECTED PRODUCTION");
    f.write("Research/old.md", "STALE DIGEST BODY");
    let context = DirectorContext {
        attached_paths: vec!["script.md".into()],
        selected_path: Some("prod.md".into()),
        focused_editor_path: Some("Research/old.md".into()),
        focused_editor_excerpt: Some("STALE DIGEST EXCERPT".into()),
        ..Default::default()
    };
    let (result, requests) = f.run(
        json!({"save_path":f.dir.path().join("Research/old.md")}),
        context,
    );
    let user = user_message(&requests);
    assert!(user.contains("ATTACHED SCRIPT"));
    assert!(user.contains("SELECTED PRODUCTION"));
    assert!(!user.contains("STALE DIGEST"));
    assert!(!user.contains("Paths that could not be loaded"));
    assert!(result
        .unwrap()
        .message
        .contains("Digest overwrite is waiting for approval."));
    assert_eq!(
        fs::read_to_string(f.dir.path().join("Research/old.md")).unwrap(),
        "STALE DIGEST BODY"
    );
}

#[test]
fn approval_labels_distinguish_new_nested_digest_from_overwrite() {
    let f = Fixture::new();
    f.write("script.md", "SCRIPT FOR APPROVAL");
    f.write("Research/existing.md", "ORIGINAL DIGEST");
    fs::write(f.app.settings_path(), r#"{"apis":[{"id":"disabled","name":"Disabled","kind":"test-only","baseUrl":"http://127.0.0.1:1","apiKey":"","secretKey":"","enabled":false}],"capabilityDefaults":{}}"#).unwrap();
    for (output, replacing) in [
        ("Research/nested/new.md", false),
        ("Research/existing.md", true),
    ] {
        let (result, requests) = f.run(
            json!({"paths":["script.md"], "output_path":output}),
            DirectorContext::default(),
        );
        assert!(user_message(&requests).contains("SCRIPT FOR APPROVAL"));
        let message = result.unwrap().message;
        if replacing {
            assert!(
                message.contains("Digest overwrite is waiting for approval."),
                "{message}"
            );
            assert_eq!(
                fs::read_to_string(f.dir.path().join(output)).unwrap(),
                "ORIGINAL DIGEST"
            );
        } else {
            assert!(
                message.contains("New digest file is waiting for approval."),
                "{message}"
            );
            assert!(!message.contains("Digest overwrite"), "{message}");
            assert!(!f.dir.path().join(output).exists());
        }
        let id = message
            .split(workbench_host::pending::APPROVE_TOKEN_PREFIX)
            .nth(1)
            .and_then(|s| s.split("]]").next())
            .expect("approval remains required")
            .to_string();
        workbench_host::director::director_approve(f.app.clone(), id.clone()).unwrap();
        assert_eq!(
            fs::read_to_string(f.dir.path().join(output)).unwrap(),
            "# Digest\nGrounded review.\n"
        );
        assert!(
            workbench_host::director::director_approve(f.app.clone(), id).is_err(),
            "approval must be one-time"
        );
    }
}

#[test]
fn failed_or_absent_digest_sources_never_call_provider_write_or_offer_approval() {
    let f = Fixture::new();
    f.write("script.md", "VALID SCRIPT");
    f.write("binary.md", "temporary");
    fs::write(f.dir.path().join("binary.md"), [0xff, 0xfe]).unwrap();
    f.write("Research/existing.md", "KEEP EXISTING DIGEST");
    for index in 0..3 {
        f.write(&format!("large-{index}.md"), &"x".repeat(48_000));
    }
    fs::File::create(f.dir.path().join("too-large.md"))
        .unwrap()
        .set_len(2_000_001)
        .unwrap();
    let cases = [
        json!({"paths":["missing.md"], "output_path":"Research/new.md"}),
        json!({"paths":["script.md", "missing.md"], "output_path":"Research/nested/new.md"}),
        json!({"paths":["binary.md"], "output_path":"Research/existing.md"}),
        json!({"paths":["script.md", "binary.md"], "output_path":"Research/existing.md"}),
        json!({"paths":["script.md", "Research"], "output_path":"Research/existing.md"}),
        json!({"paths":["script.md", "too-large.md"], "output_path":"Research/new.md"}),
        json!({"paths":["large-0.md", "large-1.md", "large-2.md"], "output_path":"Research/new.md"}),
        json!({"mode":"digest", "output_path":"Research/new.md"}),
        json!({"mode":"digest"}),
        json!({"paths":["Research/existing.md"], "output_path":f.dir.path().join("Research/existing.md")}),
    ];
    for inputs in cases {
        let (result, requests) = f.run(inputs.clone(), DirectorContext::default());
        assert!(
            requests.is_empty(),
            "source failure dispatched model for {inputs}: {result:?}"
        );
        let error = result.expect_err("source failure must not become an approvable digest");
        assert!(error.contains("source"), "actionable source error: {error}");
        assert!(!error.contains("workbench_pending_action"));
        assert!(!f.dir.path().join("Research/new.md").exists());
        assert!(!f.dir.path().join("Research/nested/new.md").exists());
        assert_eq!(
            fs::read_to_string(f.dir.path().join("Research/existing.md")).unwrap(),
            "KEEP EXISTING DIGEST"
        );
    }
}

#[test]
fn explicit_sources_exclude_output_aliases_and_unrelated_editor_context() {
    let f = Fixture::new();
    f.write("script.md", "SCRIPT: the captain changes course.");
    f.write("prod.md", "PRODUCTION: keep the red cockpit lighting.");
    f.write("unrelated.md", "UNRELATED ATTACHMENT");
    let absolute_output = f.dir.path().join("Research/stop_check.md");
    let context = DirectorContext {
        attached_paths: vec!["unrelated.md".into()],
        selected_path: Some("unrelated.md".into()),
        focused_editor_path: Some("Research/stop_check.md".into()),
        focused_editor_excerpt: Some("STALE OUTPUT EXCERPT".into()),
        ..Default::default()
    };
    let (result, requests) = f.run(
        json!({"mode":"digest", "source_paths":["script.md", "prod.md"],
        "path": absolute_output, "output_path":"Research/stop_check.md"}),
        context,
    );
    let result = result.unwrap();
    let user = user_message(&requests);
    assert!(user.contains("SCRIPT: the captain changes course."));
    assert!(user.contains("PRODUCTION: keep the red cockpit lighting."));
    assert!(
        !user.contains("UNRELATED ATTACHMENT"),
        "explicit sources must be authoritative: {user}"
    );
    assert!(!user.contains("STALE OUTPUT EXCERPT"));
    assert!(
        !user.contains("Paths that could not be loaded"),
        "output is not a missing source: {user}"
    );
    assert!(!user
        .split("## Document dossier")
        .nth(1)
        .unwrap()
        .contains("stop_check.md"));
    assert!(result.message.contains("Saved digest"));
    assert_eq!(
        fs::read_to_string(absolute_output).unwrap(),
        "# Digest\nGrounded review.\n"
    );
}

//! Public-dispatch cooperative Stop regressions. No live providers or credentials.
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};
use workbench_host::{
    ctx::HostCtx,
    director_actions::DirectorAction,
    dispatch, pending, session,
    voice::RecorderState,
    workflow::{import::persist_state, state::ProjectState},
};
static SESSION: Mutex<()> = Mutex::new(());

struct Fixture {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    app: HostCtx,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join("Research")).unwrap();
        persist_state(&root, &ProjectState::new()).unwrap();
        session::open(root.to_str().unwrap()).unwrap();
        let app = HostCtx::new(dir.path().join("settings"));
        Self {
            _dir: dir,
            root,
            app,
        }
    }
    fn configure(&self, base: &str) {
        fs::write(
            self.app.settings_path(),
            json!({
                "apis": [{"id":"cancel-local", "name":"Local fixture", "kind":"ollama",
                    "baseUrl":base, "apiKey":"", "secretKey":"", "enabled":true}],
                "capabilityDefaults": {}
            })
            .to_string(),
        )
        .unwrap();
    }
    fn args(&self) -> Value {
        json!({"request":{"model":"fixture", "apiId":"cancel-local",
            "messages":[{"role":"user","content":"Perform this isolated cancellation test."}],
            "context":{"projectRoot":self.root}}})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        pending::cancel_all();
        session::close();
    }
}
fn call(app: &HostCtx, cmd: &str, args: Value) -> Result<Value, String> {
    dispatch(app, &RecorderState::new(), cmd, &args)
}
fn cancel(app: &HostCtx) {
    assert_eq!(
        call(app, "director_cancel", json!({})).unwrap()["cancelled"],
        true
    );
}
fn stopped(response: &Value) {
    assert_eq!(response["stopped"], true, "{response}");
    assert_eq!(response["continueSuggested"], false, "{response}");
    assert!(response["reply"]
        .as_str()
        .unwrap()
        .contains("Stopped by user"));
    assert!(!response["reply"]
        .as_str()
        .unwrap()
        .contains(pending::APPROVE_TOKEN_PREFIX));
}
fn action(value: Value) -> String {
    format!("<director_action>\n{value}\n</director_action>")
}
fn done() -> String {
    action(json!({"action":"done", "message":"Fresh task finished."}))
}
fn read_http(socket: &mut TcpStream) -> Value {
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0; 4096];
        let n = socket.read(&mut buffer).unwrap();
        assert!(n > 0 && bytes.len() < 2_000_000);
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length: usize = headers
                .lines()
                .find_map(|l| {
                    let (key, value) = l.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
            }
        }
    }
}
// Nonblocking listener, bounded barrier waits and joined server, even on panic.
struct Server {
    release: Option<mpsc::Sender<()>>,
    arrived: mpsc::Receiver<()>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    join: Option<thread::JoinHandle<Vec<Value>>>,
}
impl Server {
    fn start(f: &Fixture, replies: Vec<String>, block_at: Option<usize>, http_error: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        f.configure(&format!("http://{}", listener.local_addr().unwrap()));
        let (arrived_tx, arrived) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let join = thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            while !stop2.load(std::sync::atomic::Ordering::SeqCst)
                && std::time::Instant::now() < deadline
            {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        requests.push(read_http(&mut socket));
                        let index = requests.len() - 1;
                        if block_at == Some(index) {
                            let _ = arrived_tx.send(());
                            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                        }
                        let content = replies.get(index).cloned().unwrap_or_else(done);
                        let body = json!({"choices":[{"message":{"content":content}}]}).to_string();
                        let status = if http_error {
                            "500 Fixture error"
                        } else {
                            "200 OK"
                        };
                        write!(socket, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("listener: {e}"),
                }
            }
            requests
        });
        Self {
            release: Some(release),
            arrived,
            stop,
            join: Some(join),
        }
    }
    fn release(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
    }
    fn finish(mut self) -> Vec<Value> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.join.take().unwrap().join().unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[test]
fn delayed_admission_repeated_stop_clone_sharing_and_pending_preservation() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    let admitted = f.app.clone().for_request();
    let other = HostCtx::new(f._dir.path().join("independent-settings")).for_request();
    let parked: DirectorAction =
        serde_json::from_value(json!({"action":"done", "message":"unrelated"})).unwrap();
    let id = pending::park(parked, Some(f.root.clone()), None, None);
    cancel(&f.app.clone());
    cancel(&f.app);
    assert!(admitted.director_cancelled());
    assert!(!other.director_cancelled());
    assert!(!f.app.for_request().director_cancelled());
    let response = call(&admitted, "director_turn", f.args()).unwrap();
    stopped(&response); // must occur BEFORE settings validation or pending mutation
    assert!(
        pending::take(&id).is_ok(),
        "cancelled request erased unrelated pending state"
    );
}

#[test]
fn fresh_and_independent_requests_after_stop_can_finish() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    cancel(&f.app);
    let server = Server::start(&f, vec![done(), done()], None, false);
    let fresh = call(&f.app, "director_turn", f.args()).unwrap();
    assert_eq!(fresh["stopped"], false);
    let independent = HostCtx::new(f.app.settings_dir().to_path_buf());
    let isolated = call(&independent, "director_turn", f.args()).unwrap();
    assert_eq!(isolated["stopped"], false);
    assert_eq!(server.finish().len(), 2);
}

#[test]
fn blocked_planner_error_is_stopped_not_interpreted_as_failure() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    let mut server = Server::start(&f, vec![done()], Some(0), true);
    let app = f.app.clone();
    let args = f.args();
    let worker = thread::spawn(move || call(&app, "director_turn", args));
    server
        .arrived
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    cancel(&f.app);
    server.release();
    let response = worker.join().unwrap().unwrap();
    stopped(&response);
    assert_eq!(server.finish().len(), 1);
}

#[test]
fn blocked_planner_cannot_publish_approval_or_start_specialist() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    for proposal in [
        action(
            json!({"action":"run_specialist","specialist":"still_image","inputs":{"prompt":"fixture","output_path":"fixture.png"}}),
        ),
        action(
            json!({"action":"run_specialist","specialist":"production_helper","inputs":{"question":"fixture"}}),
        ),
    ] {
        let mut server = Server::start(&f, vec![proposal], Some(0), false);
        let app = f.app.clone();
        let args = f.args();
        let worker = thread::spawn(move || call(&app, "director_turn", args));
        server
            .arrived
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        cancel(&f.app);
        server.release();
        let response = worker.join().unwrap().unwrap();
        stopped(&response);
        assert_eq!(response["toolEvents"], json!([]));
        assert!(pending::current().is_none());
        assert_eq!(server.finish().len(), 1);
    }
}

#[test]
fn cancelled_pre_admitted_approval_does_not_consume_or_execute_then_fresh_approval_is_one_shot() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    let target = f.root.join("Research/note.md");
    fs::write(&target, "before").unwrap();
    let authorized = workbench_host::text_writes::authorize_text_write(
        "production_helper",
        "Research/note.md",
        f.root.to_str(),
        5,
        0,
        0,
    )
    .unwrap();
    let staged = workbench_host::text_writes::stage_authorized_text_writes(
        "production_helper",
        vec![(authorized, "after".into())],
    );
    let card = workbench_host::text_writes::text_write_approval_card(
        &staged,
        &["fixture overwrite".into()],
        f.root.to_str(),
    )
    .unwrap();
    let id = card
        .split(pending::APPROVE_TOKEN_PREFIX)
        .nth(1)
        .unwrap()
        .strip_suffix("]]")
        .unwrap();
    let admitted = f.app.for_request();
    cancel(&f.app);
    let response = call(&admitted, "director_approve", json!({"id":id})).unwrap();
    stopped(&response);
    assert_eq!(fs::read_to_string(&target).unwrap(), "before");
    assert!(pending::current().is_some());
    let fresh = call(&f.app, "director_approve", json!({"id":id})).unwrap();
    assert_eq!(fresh["stopped"], false);
    assert_eq!(fs::read_to_string(&target).unwrap(), "after");
    assert!(call(&f.app, "director_approve", json!({"id":id})).is_err());
}

#[cfg(unix)]
#[test]
fn two_action_barrier_reports_completed_inspection_without_second_action() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    let fifo = f.root.join("barrier.md");
    assert!(std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let first = action(json!({"action":"inspect","op":"read_file","path":"barrier.md"}));
    let second = action(json!({"action":"inspect","op":"read_file","path":"Research/second.md"}));
    fs::write(f.root.join("Research/second.md"), "MUST_NOT_BE_READ").unwrap();
    let server = Server::start(&f, vec![format!("{first}\n{second}")], None, false);
    let app = f.app.clone();
    let args = f.args();
    let worker = thread::spawn(move || call(&app, "director_turn", args));
    // Opening the writer completes only after run_inspect has opened its reader.
    let mut writer = fs::OpenOptions::new().write(true).open(&fifo).unwrap();
    cancel(&f.app);
    writer.write_all(b"FIRST_INSPECTION_COMPLETED").unwrap();
    drop(writer);
    let response = worker.join().unwrap().unwrap();
    stopped(&response);
    assert_eq!(response["toolEvents"], json!(["inspect:read_file"]));
    assert!(response["reply"]
        .as_str()
        .unwrap()
        .contains("FIRST_INSPECTION_COMPLETED"));
    assert!(!response["reply"]
        .as_str()
        .unwrap()
        .contains("MUST_NOT_BE_READ"));
    assert_eq!(server.finish().len(), 1);
}

#[cfg(unix)]
#[test]
fn stop_after_approved_inspection_reports_result_and_keeps_consumption() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    let fifo = f.root.join("approved-barrier.md");
    assert!(std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let inspect = serde_json::from_value(
        json!({"action":"inspect","op":"read_file","path":"approved-barrier.md"}),
    )
    .unwrap();
    let id = pending::park(inspect, Some(f.root.clone()), None, None);
    let app = f.app.clone();
    let worker_id = id.clone();
    let worker = thread::spawn(move || call(&app, "director_approve", json!({"id":worker_id})));
    let mut writer = fs::OpenOptions::new().write(true).open(&fifo).unwrap();
    cancel(&f.app);
    writer.write_all(b"APPROVED_RESULT_COMPLETED").unwrap();
    drop(writer);
    let response = worker.join().unwrap().unwrap();
    stopped(&response);
    assert!(response["reply"]
        .as_str()
        .unwrap()
        .contains("APPROVED_RESULT_COMPLETED"));
    assert_eq!(response["toolEvents"], json!(["approve:Inspect"]));
    assert!(call(&f.app, "director_approve", json!({"id":id})).is_err());
}

#[test]
fn stop_inside_blocked_specialist_does_not_publish_nested_save_approval() {
    let _lock = SESSION.lock().unwrap();
    let f = Fixture::new();
    fs::write(f.root.join("source.md"), "source material").unwrap();
    fs::write(f.root.join("Research/digest.md"), "unchanged digest").unwrap();
    let proposal = action(
        json!({"action":"run_specialist","specialist":"production_helper",
        "inputs":{"question":"Digest source.md", "paths":["source.md"], "mode":"digest", "output_path":"Research/digest.md"}}),
    );
    let mut server = Server::start(
        &f,
        vec![proposal, "Prepared replacement digest".into()],
        Some(1),
        false,
    );
    let app = f.app.clone();
    let args = f.args();
    let worker = thread::spawn(move || call(&app, "director_turn", args));
    server
        .arrived
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    cancel(&f.app);
    server.release();
    let response = worker.join().unwrap().unwrap();
    stopped(&response);
    assert!(
        pending::current().is_none(),
        "specialist published a nested approval after Stop"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("Research/digest.md")).unwrap(),
        "unchanged digest"
    );
    assert_eq!(server.finish().len(), 2);
}

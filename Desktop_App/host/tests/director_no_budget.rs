//! Connected Director -> real chat HTTP transport, with a loopback-only provider.
//! No credentials, pricing API, real provider calls, or original project writes.
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};
use workbench_host::workflow::{import, state::ProjectState};
use workbench_host::{ctx::HostCtx, director, pending, session};

struct SessionGuard;
impl Drop for SessionGuard {
    fn drop(&mut self) {
        pending::cancel_all();
        session::close();
    }
}

#[test]
fn director_returns_a_provider_reply_without_a_project_budget_or_price() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    import::persist_state(&root, &ProjectState::new()).unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let _guard = SessionGuard;
    let state_path = root.join(workbench_host::workflow::state::STATE_REL);
    let before = fs::read(&state_path).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = HostCtx::new(dir.path().join("settings"));
    fs::write(
        app.settings_path(),
        json!({
            "apis": [{"id": "local-test", "name": "Loopback test", "kind": "atlas-cloud",
                "baseUrl": base, "apiKey": "", "secretKey": "", "enabled": true}],
            "capabilityDefaults": {}
        })
        .to_string(),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "Director never dispatched chat");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("loopback accept: {e}"),
            }
        };
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
                    break;
                }
            }
        }
        let reply = json!({"choices": [{"message": {"content": "<director_action>\n{\"action\":\"done\",\"message\":\"Ready for the no-budget test.\"}\n</director_action>"}}]}).to_string();
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
        String::from_utf8(bytes).unwrap()
    });
    let request = serde_json::from_value(json!({
        "model": "deepseek-ai/deepseek-v4-pro", "apiId": "local-test",
        "messages": [{"role": "user", "content": "Reply only: Ready for the no-budget test."}],
        "context": {}
    }))
    .unwrap();
    let result = director::director_turn(app, request);
    let received = server.join().unwrap();
    let response = result.unwrap();
    assert_eq!(response.reply, "Ready for the no-budget test.");
    assert!(!response.continue_suggested);
    assert!(response.tool_events.is_empty());
    assert!(received.starts_with("POST /v1/chat/completions "));
    assert!(received.contains("deepseek-ai/deepseek-v4-pro"));
    assert_eq!(
        fs::read(state_path).unwrap(),
        before,
        "chat must not invent budget/cost state"
    );
}

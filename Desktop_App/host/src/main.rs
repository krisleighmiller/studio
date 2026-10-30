//! Stdio JSON-RPC host for the Electron workbench shell.
//!
//! Request:  `{ "id": "1", "cmd": "list_dir", "args": { "path": "…" } }`
//! Response: `{ "id": "1", "ok": true, "result": … }` or `{ "id": "1", "ok": false, "error": "…" }`
//! Events:   `{ "event": "director-progress", "payload": "…" }` (no id)

use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use workbench_host::ctx::HostCtx;
use workbench_host::dispatch;
use workbench_host::voice::RecorderState;

#[derive(Debug, Deserialize)]
struct Request {
    id: String,
    cmd: String,
    #[serde(default)]
    args: Value,
}

fn default_settings_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("WORKBENCH_SETTINGS_DIR") {
        return PathBuf::from(dir);
    }
    dirs_fallback()
}

fn dirs_fallback() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("video-workbench");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config").join("video-workbench");
    }
    PathBuf::from(".").join("workbench-settings")
}

fn respond(app: &HostCtx, recorder: &RecorderState, request: Request) {
    let result = dispatch(app, recorder, &request.cmd, &request.args);
    let response = match result {
        Ok(value) => json!({
            "id": request.id,
            "ok": true,
            "result": workbench_host::director::redact_publication(app, value),
        }),
        Err(error) => json!({
            "id": request.id,
            "ok": false,
            "error": workbench_host::secrets::redact_text(app, &error),
        }),
    };
    app.write_response(&response);
}

fn main() {
    // Keep stderr for diagnostics; protocol traffic is stdout-only.
    eprintln!("workbench-host: ready (stdio JSON-RPC)");

    let app = HostCtx::new(default_settings_dir());
    let recorder = Arc::new(RecorderState::new());

    // Keep input consumption independent of the eight execution workers. In
    // particular, Stop must not wait for a slot held by blocked provider calls.
    let (queue, work) = mpsc::sync_channel::<(HostCtx, Request)>(64);
    let work = Arc::new(Mutex::new(work));
    for _ in 0..8 {
        let work = Arc::clone(&work);
        let recorder = Arc::clone(&recorder);
        thread::spawn(move || loop {
            let next = work.lock().expect("request queue").recv();
            let Ok((app, request)) = next else { break };
            respond(&app, &recorder, request);
        });
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("workbench-host: stdin error: {error}");
                break;
            }
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let request: Request = match serde_json::from_str(line) {
            Ok(request) => request,
            Err(error) => {
                let response = json!({
                    "id": null,
                    "ok": false,
                    "error": format!("Invalid request JSON: {error}"),
                });
                app.write_response(&response);
                continue;
            }
        };

        // Immutable admission snapshot, before queuing. Handle Stop on this
        // reader, in input order, so queued older requests stay cancelled.
        let request_app = app.for_request();
        if request.cmd == "director_cancel" {
            respond(&request_app, &recorder, request);
        } else {
            match queue.try_send((request_app, request)) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full((app, request))) => app.write_response(&json!({
                    "id": request.id,
                    "ok": false,
                    "error": "Host request queue is full; request was not executed.",
                })),
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
        }
    }

    let _ = io::stderr().flush();
}

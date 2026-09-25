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

struct JobSlots {
    available: Mutex<mpsc::Receiver<()>>,
    release: mpsc::Sender<()>,
}

fn job_slots() -> &'static JobSlots {
    static SLOTS: std::sync::OnceLock<JobSlots> = std::sync::OnceLock::new();
    SLOTS.get_or_init(|| {
        let (release, available) = mpsc::channel();
        for _ in 0..8 {
            let _ = release.send(());
        }
        JobSlots {
            available: Mutex::new(available),
            release,
        }
    })
}

fn main() {
    // Keep stderr for diagnostics; protocol traffic is stdout-only.
    eprintln!("workbench-host: ready (stdio JSON-RPC)");

    let app = HostCtx::new(default_settings_dir());
    let recorder = Arc::new(RecorderState::new());

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

        let app = app.clone();
        let recorder = Arc::clone(&recorder);
        let slots = job_slots();
        // Wait for a free slot before starting another command thread.
        let _ = slots.available.lock().expect("job slots").recv();
        let release = slots.release.clone();
        // Each command on its own thread so long director/Atlas work can emit progress
        // while still accepting later requests (e.g. voice level polls).
        thread::spawn(move || {
            let result = dispatch(&app, &recorder, &request.cmd, &request.args);
            let response = match result {
                Ok(value) => {
                    let value = workbench_host::secrets::redact_json(&app, value);
                    json!({
                        "id": request.id,
                        "ok": true,
                        "result": value,
                    })
                }
                Err(error) => json!({
                    "id": request.id,
                    "ok": false,
                    "error": workbench_host::secrets::redact_text(&app, &error),
                }),
            };
            app.write_response(&response);
            let _ = release.send(());
        });
    }

    let _ = io::stderr().flush();
}

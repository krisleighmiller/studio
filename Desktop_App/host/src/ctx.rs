//! Process-wide host context: settings directory + event sink for Electron.

use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct HostCtx {
    inner: Arc<HostInner>,
}

struct HostInner {
    settings_dir: PathBuf,
    /// Serialized stdout writer so responses and events do not interleave bytes.
    out: Mutex<io::Stdout>,
}

impl HostCtx {
    pub fn new(settings_dir: PathBuf) -> Self {
        if let Err(error) = std::fs::create_dir_all(&settings_dir) {
            eprintln!("workbench-host: could not create settings dir: {error}");
        }
        Self {
            inner: Arc::new(HostInner {
                settings_dir,
                out: Mutex::new(io::stdout()),
            }),
        }
    }

    pub fn settings_dir(&self) -> &Path {
        &self.inner.settings_dir
    }

    pub fn settings_path(&self) -> PathBuf {
        self.inner.settings_dir.join("settings.json")
    }

    pub fn emit(&self, event: &str, payload: impl Into<Value>) {
        let line = json!({
            "event": event,
            "payload": payload.into(),
        });
        self.write_line(&line);
    }

    pub fn write_response(&self, value: &Value) {
        self.write_line(value);
    }

    fn write_line(&self, value: &Value) {
        if let Ok(mut out) = self.inner.out.lock() {
            let _ = writeln!(out, "{value}");
            let _ = out.flush();
        }
    }
}

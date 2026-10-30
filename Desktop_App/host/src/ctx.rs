//! Process-wide host context: settings directory + event sink for Electron.

use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct HostCtx {
    inner: Arc<HostInner>,
    request_generation: Option<u64>,
    progress_id: Option<String>,
    pub(crate) workflow_parent: Option<Arc<crate::workflow_continuation::Parent>>,
    workflow_reply: Option<Arc<Mutex<Option<Value>>>>,
    #[cfg(test)]
    pub(crate) test_turn_elapsed:
        Option<Arc<dyn Fn(std::time::Duration) -> std::time::Duration + Send + Sync>>,
}

struct HostInner {
    settings_dir: PathBuf,
    cancel_generation: AtomicU64,
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
                cancel_generation: AtomicU64::new(0),
                out: Mutex::new(io::stdout()),
            }),
            request_generation: None,
            progress_id: None,
            workflow_parent: None,
            workflow_reply: None,
            #[cfg(test)]
            test_turn_elapsed: None,
        }
    }

    pub fn settings_dir(&self) -> &Path {
        &self.inner.settings_dir
    }

    /// Capture admission before queuing. Clones retain this immutable snapshot;
    /// starting a request never clears a Stop issued against older work.
    pub fn for_request(&self) -> Self {
        let mut request = self.clone();
        request
            .request_generation
            .get_or_insert_with(|| self.inner.cancel_generation.load(Ordering::SeqCst));
        if request.workflow_reply.is_none() { request.workflow_reply = Some(Arc::new(Mutex::new(None))); }
        request
    }

    /// Process-context cooperative Stop, not per-chat interruption or rollback.
    pub fn cancel_director(&self) {
        self.inner.cancel_generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn director_cancelled(&self) -> bool {
        self.request_generation.is_some_and(|generation| {
            generation != self.inner.cancel_generation.load(Ordering::SeqCst)
        })
    }

    pub fn settings_path(&self) -> PathBuf {
        self.inner.settings_dir.join("settings.json")
    }

    /// Presentation correlation only; never authority to execute an action.
    pub fn with_progress_id(mut self, id: Option<&str>) -> Self {
        self.progress_id = id
            .filter(|id| !id.is_empty() && id.len() <= 128 && id.is_ascii())
            .map(str::to_owned);
        self
    }

    pub(crate) fn with_workflow_parent(&self, parent: Option<Arc<crate::workflow_continuation::Parent>>) -> Self { let mut app = self.clone(); app.workflow_parent = parent; app }
    pub(crate) fn set_workflow_reply(&self, reply: Value) { if let Some(slot) = &self.workflow_reply { *slot.lock().expect("workflow reply") = Some(reply); } }
    pub(crate) fn take_workflow_reply(&self) -> Option<Value> { self.workflow_reply.as_ref().and_then(|s| s.lock().expect("workflow reply").take()) }

    pub fn emit(&self, event: &str, payload: impl Into<Value>) {
        let mut payload = payload.into();
        if event == "spend-confirmation-required" { if let (Some(id), Some(object)) = (&self.progress_id, payload.as_object_mut()) { object.insert("ownerRequestId".into(), json!(id)); if self.workflow_parent.is_some() { object.insert("workflowContinuationBound".into(), json!(true)); } } }
        let payload = if event == "director-progress" {
            match &self.progress_id {
                Some(id) => json!({ "requestId": id, "message": payload }),
                None => payload,
            }
        } else {
            payload
        };
        let line = json!({
            "event": event,
            "payload": payload,
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

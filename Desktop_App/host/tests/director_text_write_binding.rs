//! Real local approval boundaries with configured (credential-free) providers.
use serde_json::json;
use std::{
    fs,
    sync::{Mutex, MutexGuard},
};
use workbench_host::{
    ctx::HostCtx,
    director::{self, DirectorContext},
    director_actions::{self, ActionKind, DirectorAction},
    pending, session, text_writes,
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
        fs::create_dir_all(dir.path().join("Research")).unwrap();
        session::open(dir.path().to_str().unwrap()).unwrap();
        let app = HostCtx::new(dir.path().join("settings"));
        let f = Self {
            _lock: lock,
            dir,
            app,
        };
        f.settings(true);
        f
    }
    fn settings(&self, enabled: bool) {
        fs::write(self.app.settings_path(), json!({
            "apis": [{"id":"configured-test", "name":"Test provider", "kind":"test-only",
                "baseUrl":"http://127.0.0.1:1", "apiKey":"", "secretKey":"", "enabled":enabled}],
            "capabilityDefaults":{}
        }).to_string()).unwrap();
    }
    fn stage(&self, path: &str) -> String {
        let body = "Approved local bytes.\n";
        let root = self.dir.path().to_str().unwrap();
        let write = text_writes::authorize_text_write(
            "production_helper",
            path,
            Some(root),
            body.len(),
            0,
            0,
        )
        .unwrap();
        let previous = fs::read_to_string(self.dir.path().join(path)).ok();
        let summary = text_writes::approval_summary(&write, previous.as_deref(), body);
        let staged = text_writes::stage_authorized_text_writes(
            "production_helper",
            vec![(write, body.into())],
        );
        let card = text_writes::text_write_approval_card(&staged, &[summary], Some(root)).unwrap();
        token(&card)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        pending::cancel_all();
        session::close();
    }
}
fn token(card: &str) -> String {
    card.split(pending::APPROVE_TOKEN_PREFIX)
        .nth(1)
        .and_then(|s| s.split("]]").next())
        .expect("approval token")
        .to_string()
}

#[test]
fn nested_local_save_survives_unrelated_provider_settings_changes() {
    let f = Fixture::new();
    let path = "Research/Director_Test/stop_check.md";
    let id = f.stage(path);
    assert!(!f.dir.path().join(path).exists());
    f.settings(false);
    let reply = director::director_approve(f.app.clone(), id).unwrap();
    assert_eq!(reply.tool_events, vec!["approve:text_write"]);
    assert_eq!(
        fs::read_to_string(f.dir.path().join(path)).unwrap(),
        "Approved local bytes.\n"
    );
}

#[test]
fn local_save_rejects_existing_file_changed_since_approval_card() {
    let f = Fixture::new();
    let path = "Research/note.md";
    fs::write(f.dir.path().join(path), "Original file.\n").unwrap();
    let id = f.stage(path);
    fs::write(f.dir.path().join(path), "New user edits.\n").unwrap();
    let error = director::director_approve(f.app.clone(), id).unwrap_err();
    assert!(error.contains("approved input file changed"), "{error}");
    assert_eq!(
        fs::read_to_string(f.dir.path().join(path)).unwrap(),
        "New user edits.\n"
    );
}

#[test]
fn local_save_rejects_new_target_created_since_approval_card() {
    let f = Fixture::new();
    let path = "Research/Director_Test/stop_check.md";
    let id = f.stage(path);
    fs::create_dir_all(f.dir.path().join("Research/Director_Test")).unwrap();
    fs::write(f.dir.path().join(path), "User-created file.\n").unwrap();
    let error = director::director_approve(f.app.clone(), id).unwrap_err();
    assert!(
        error.contains("changed between preparation and approval"),
        "{error}"
    );
    assert_eq!(
        fs::read_to_string(f.dir.path().join(path)).unwrap(),
        "User-created file.\n"
    );
}

#[test]
fn local_save_rejects_project_switch() {
    let f = Fixture::new();
    let path = "Research/nested/note.md";
    let id = f.stage(path);
    let other = f.dir.path().join("other-project");
    fs::create_dir_all(&other).unwrap();
    session::open(other.to_str().unwrap()).unwrap();
    let error = director::director_approve(f.app.clone(), id).unwrap_err();
    // Opening a project may invalidate the token before the root check.
    assert!(
        error.contains("project changed") || error.contains("no longer active"),
        "{error}"
    );
    assert!(!f.dir.path().join(path).exists());
    assert!(!other.join(path).exists());
}

#[cfg(unix)]
#[test]
fn local_save_rejects_parent_symlink_redirect_after_approval_card() {
    let f = Fixture::new();
    let path = "Research/notes/digest.md";
    let id = f.stage(path);
    let redirected = f.dir.path().join("Research/redirected");
    fs::create_dir_all(&redirected).unwrap();
    std::os::unix::fs::symlink(&redirected, f.dir.path().join("Research/notes")).unwrap();
    let reply = director::director_approve(f.app.clone(), id).unwrap();
    // Execution failures use the approval response envelope, not an IPC error.
    assert_eq!(reply.tool_events, vec!["approve:error"]);
    assert!(reply.reply.contains("changed after approval"), "{reply:?}");
    assert!(!redirected.join("digest.md").exists());
}

#[test]
fn model_action_still_rejects_provider_enabled_after_snapshot() {
    let f = Fixture::new();
    f.settings(false);
    let context = DirectorContext {
        project_root: Some(f.dir.path().to_str().unwrap().into()),
        ..Default::default()
    };
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("production_helper".into()),
        inputs: serde_json::Map::new(),
        message: None,
        op: None,
        path: None,
    };
    let validated =
        director_actions::validate_action(action.clone(), f.dir.path().to_str()).unwrap();
    let card = director_actions::format_approval_request_with_context(
        &action,
        &validated,
        &context,
        "No provider",
        "No model",
        "Unknown",
    );
    let id = token(&card);
    f.settings(true);
    let error = director::director_approve(f.app.clone(), id).unwrap_err();
    assert!(
        error.contains("Provider or model settings changed"),
        "{error}"
    );
}

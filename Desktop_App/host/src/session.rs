//! The host owns the open project. The renderer can refer to it, but it cannot
//! choose a different root or read outside it without a dialog grant.

use crate::project_path::{self, PathIntent};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const GRANT_TTL: Duration = Duration::from_secs(10 * 60);

#[cfg(test)]
pub(crate) fn test_session_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

struct Grant {
    path: PathBuf,
    intent: PathIntent,
    at: Instant,
}

struct State {
    root: Option<PathBuf>,
    grants: Vec<Grant>,
}

fn state() -> &'static Mutex<State> {
    static STATE: Mutex<State> = Mutex::new(State {
        root: None,
        grants: Vec::new(),
    });
    &STATE
}

pub fn open(raw: &str) -> Result<String, String> {
    let path = PathBuf::from(raw.trim());
    if !path.is_dir() {
        return Err(format!("Not a project folder: {}", path.display()));
    }
    let root = path
        .canonicalize()
        .map_err(|error| format!("Could not open project: {error}"))?;
    project_path::reject_sensitive(&root)?;
    let mut guard = state().lock().expect("project session");
    guard.grants.clear();
    guard.root = Some(root.clone());
    Ok(root.to_string_lossy().into_owned())
}

pub fn close() {
    let mut guard = state().lock().expect("project session");
    guard.root = None;
    guard.grants.clear();
}

pub fn root() -> Option<PathBuf> {
    state().lock().expect("project session").root.clone()
}

pub fn grant(raw: &str, intent: PathIntent) -> Result<String, String> {
    let path = PathBuf::from(raw.trim());
    if !path.is_absolute() {
        return Err("A grant has to be an absolute path from a dialog.".into());
    }
    let stored = if path.exists() {
        path.canonicalize()
            .map_err(|error| format!("Could not grant path: {error}"))?
    } else {
        lexical_absolute(&path)?
    };
    project_path::reject_sensitive(&stored)?;
    let mut guard = state().lock().expect("project session");
    let now = Instant::now();
    guard
        .grants
        .retain(|grant| now.duration_since(grant.at) < GRANT_TTL);
    guard.grants.push(Grant {
        path: stored.clone(),
        intent,
        at: now,
    });
    Ok(stored.to_string_lossy().into_owned())
}

/// Workflow and director commands use this root. A supplied path is accepted
/// only when it is the open project.
pub fn command_root(supplied: Option<&str>) -> Result<PathBuf, String> {
    let Some(root) = root() else {
        return Err("Open a project first.".into());
    };
    if let Some(supplied) = supplied.map(str::trim).filter(|value| !value.is_empty()) {
        let supplied_path = Path::new(supplied);
        let canon = supplied_path
            .canonicalize()
            .unwrap_or_else(|_| supplied_path.to_path_buf());
        if canon != root {
            return Err("That path is not the open project.".into());
        }
    }
    Ok(root)
}

pub fn authorize(raw: &str, intent: PathIntent) -> Result<PathBuf, String> {
    authorize_with_grant(raw, intent, true)
}

/// Validate a path for a pending approval card without spending its one-time
/// dialog grant. The grant is consumed only when the approved operation reads it.
pub fn peek_authorize(raw: &str, intent: PathIntent) -> Result<PathBuf, String> {
    authorize_with_grant(raw, intent, false)
}

fn authorize_with_grant(
    raw: &str,
    intent: PathIntent,
    consume_grant: bool,
) -> Result<PathBuf, String> {
    let active_root = root();
    if let Some(root) = active_root.as_ref() {
        let root_string = root.to_string_lossy();
        if let Ok(path) = project_path::resolve(raw, Some(root_string.as_ref()), intent) {
            return Ok(path.into_path_buf());
        }
    }

    let candidate = candidate_path(raw)?;
    project_path::reject_sensitive(&candidate)?;

    let mut guard = state().lock().expect("project session");
    let now = Instant::now();
    guard
        .grants
        .retain(|grant| now.duration_since(grant.at) < GRANT_TTL);
    let grant_index = guard
        .grants
        .iter()
        .position(|grant| grant.path == candidate && grant.intent == intent);
    if let Some(index) = grant_index {
        if consume_grant {
            guard.grants.remove(index);
        }
        return Ok(candidate);
    }

    if active_root.is_none() {
        return Err("Open a project first.".into());
    }
    Err("That path is outside the open project or lacks a one-time grant.".into())
}

/// Test builds may touch fixture paths before a session exists. The packaged
/// host always requires a project or a grant.
pub fn authorize_user_path(raw: &str, intent: PathIntent) -> Result<PathBuf, String> {
    #[cfg(test)]
    {
        let _ = intent;
        return Ok(PathBuf::from(raw));
    }
    #[cfg(not(test))]
    {
        authorize(raw, intent)
    }
}

fn candidate_path(raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw.trim());
    if path.as_os_str().is_empty() {
        return Err("Path is empty.".into());
    }
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|error| format!("Could not resolve path: {error}"));
    }
    if path.is_absolute() {
        return lexical_absolute(&path);
    }
    if let Some(root) = root() {
        return lexical_absolute(&root.join(path));
    }
    Err("Open a project first.".into())
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Path is not absolute.".into());
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err("Path escapes its root.".into());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        test_session_lock()
    }

    #[test]
    fn open_project_rejects_paths_outside_it_until_granted() {
        let _guard = lock();
        close();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let outside = dir.path().join("outside.txt");
        fs::create_dir_all(root.join("Scripts")).unwrap();
        fs::write(&outside, b"secret").unwrap();
        open(root.to_str().unwrap()).unwrap();

        let read = authorize("Scripts/draft.md", PathIntent::Read).unwrap();
        assert!(read.ends_with("Scripts/draft.md"));
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_err());
        assert!(command_root(Some("/etc")).is_err());
        assert_eq!(
            command_root(Some(root.to_str().unwrap())).unwrap(),
            root.canonicalize().unwrap()
        );

        grant(outside.to_str().unwrap(), PathIntent::Read).unwrap();
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_ok());
        close();
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_err());
    }

    #[test]
    fn outside_grants_are_exact_and_consumed_after_one_authorization() {
        let _guard = lock();
        close();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let outside = dir.path().join("reference.png");
        let sibling = dir.path().join("other.png");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, b"selected").unwrap();
        fs::write(&sibling, b"not selected").unwrap();
        open(root.to_str().unwrap()).unwrap();
        grant(outside.to_str().unwrap(), PathIntent::Read).unwrap();

        assert_eq!(
            peek_authorize(outside.to_str().unwrap(), PathIntent::Read).unwrap(),
            outside.canonicalize().unwrap()
        );
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_ok());
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_err());
        assert!(authorize(sibling.to_str().unwrap(), PathIntent::Read).is_err());
        close();
    }

    #[test]
    fn a_read_grant_cannot_authorize_a_write() {
        let _guard = lock();
        close();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let outside = dir.path().join("selected.txt");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, b"selected").unwrap();
        open(root.to_str().unwrap()).unwrap();
        grant(outside.to_str().unwrap(), PathIntent::Read).unwrap();

        assert!(authorize(outside.to_str().unwrap(), PathIntent::Write).is_err());
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_ok());
        close();
    }

    #[test]
    fn a_write_grant_cannot_authorize_a_read() {
        let _guard = lock();
        close();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let outside = dir.path().join("selected.txt");
        fs::create_dir_all(&root).unwrap();
        open(root.to_str().unwrap()).unwrap();
        grant(outside.to_str().unwrap(), PathIntent::Write).unwrap();

        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_err());
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Write).is_ok());
        close();
    }

    #[test]
    fn a_grant_cannot_authorize_descendants_of_a_selected_directory() {
        let _guard = lock();
        close();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let selected = dir.path().join("selected");
        let child = selected.join("secret.txt");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&selected).unwrap();
        fs::write(&child, b"secret").unwrap();
        open(root.to_str().unwrap()).unwrap();
        grant(selected.to_str().unwrap(), PathIntent::Read).unwrap();

        assert!(authorize(child.to_str().unwrap(), PathIntent::Read).is_err());
        assert!(authorize(selected.to_str().unwrap(), PathIntent::Read).is_ok());
        close();
    }

    #[test]
    fn a_voice_reference_sheet_outside_the_project_is_not_authorized_for_write() {
        let _guard = lock();
        close();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let outside = dir.path().join("other_character.md");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, b"# Another character").unwrap();
        open(root.to_str().unwrap()).unwrap();

        // A character reference sheet must live inside the project. The host must
        // not append a voice profile to an existing file outside it, even when the
        // renderer supplies the path.
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Write).is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "# Another character");

        // An in-project sheet is authorized, so normal voice saves keep working.
        let inside = root.join("Characters/Ada/Ada.md");
        fs::create_dir_all(inside.parent().unwrap()).unwrap();
        fs::write(&inside, b"# Ada").unwrap();
        assert!(authorize(inside.to_str().unwrap(), PathIntent::Write).is_ok());
        close();
    }
}

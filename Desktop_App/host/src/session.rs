//! The host owns the open project. The renderer can refer to it, but it cannot
//! choose a different root or read outside it without a dialog grant.

use crate::project_path::{self, PathIntent};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const GRANT_TTL: Duration = Duration::from_secs(10 * 60);

struct Grant {
    path: PathBuf,
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

pub fn grant(raw: &str) -> Result<String, String> {
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
    guard.grants.retain(|grant| now.duration_since(grant.at) < GRANT_TTL);
    guard.grants.push(Grant {
        path: stored.clone(),
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
    if let Some(root) = root() {
        let root_string = root.to_string_lossy();
        if let Ok(path) = project_path::resolve(raw, Some(root_string.as_ref()), intent) {
            return Ok(path.into_path_buf());
        }
    }
    let candidate = candidate_path(raw)?;
    let mut guard = state().lock().expect("project session");
    let now = Instant::now();
    guard.grants.retain(|grant| now.duration_since(grant.at) < GRANT_TTL);
    let allowed = guard.grants.iter().any(|grant| path_within(&grant.path, &candidate));
    drop(guard);
    if allowed {
        project_path::reject_sensitive(&candidate)?;
        return Ok(candidate);
    }
    if root().is_none() {
        return Err("Open a project first.".into());
    }
    Err("That path is outside the open project.".into())
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

fn path_within(grant: &Path, candidate: &Path) -> bool {
    candidate == grant || candidate.starts_with(grant)
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
    use std::sync::MutexGuard;

    fn lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|error| error.into_inner())
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
        assert_eq!(command_root(Some(root.to_str().unwrap())).unwrap(), root.canonicalize().unwrap());

        grant(outside.to_str().unwrap()).unwrap();
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_ok());
        close();
        assert!(authorize(outside.to_str().unwrap(), PathIntent::Read).is_err());
    }
}

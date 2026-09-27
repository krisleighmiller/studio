//! Host-owned project path confinement.
//!
//! Model output, project state, and handoff files are untrusted. Every path they
//! can influence has to land inside a canonical project root, compared as path
//! components rather than a string prefix.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathIntent {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError(pub String);

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PathError {}

/// A path that has already been confined to a project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPath {
    absolute: PathBuf,
}

impl ProjectPath {
    pub fn as_path(&self) -> &Path {
        &self.absolute
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.absolute
    }
}

pub fn resolve(
    raw: &str,
    project_root: Option<&str>,
    intent: PathIntent,
) -> Result<ProjectPath, PathError> {
    let Some(root) = project_root.map(str::trim).filter(|root| !root.is_empty()) else {
        return Err(PathError("open a project before using a file path".into()));
    };
    let absolute = confine(raw, Path::new(root), intent)?;
    Ok(ProjectPath { absolute })
}

pub fn relative_to_root(root: &Path, absolute: &Path) -> Result<PathBuf, String> {
    let root_canon = root
        .canonicalize()
        .map_err(|error| format!("project root: {error}"))?;
    absolute
        .strip_prefix(&root_canon)
        .map(|path| path.to_path_buf())
        .map_err(|_| {
            format!(
                "path {} is outside {}",
                absolute.display(),
                root_canon.display()
            )
        })
}

/// `Preproduction/Scene<digits>/<bucket>/file`.
pub fn under_scene_bucket(root: &Path, absolute: &Path, bucket: &str) -> Result<(), String> {
    let rel = relative_to_root(root, absolute)?;
    let parts: Vec<String> = rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    let scene_ok = parts.get(1).is_some_and(|scene| {
        let Some(number) = scene.strip_prefix("Scene") else {
            return false;
        };
        !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
    });
    if parts.len() >= 4 && parts[0] == "Preproduction" && scene_ok && parts[2] == bucket {
        return Ok(());
    }
    Err(format!(
        "refusing to touch a file outside Preproduction/SceneN/{bucket}"
    ))
}

pub fn under_cut_dir(
    root: &Path,
    absolute: &Path,
    scene_id: u32,
    cut_number: u32,
) -> Result<(), String> {
    let rel = relative_to_root(root, absolute)?;
    let expected = PathBuf::from(format!("Scenes/Scene{scene_id}/Cut{cut_number}"));
    if rel.starts_with(&expected) && rel != expected {
        return Ok(());
    }
    Err(format!(
        "refusing to delete a take outside {}",
        expected.display()
    ))
}

fn confine(raw: &str, root: &Path, intent: PathIntent) -> Result<PathBuf, PathError> {
    if raw.contains('\0') {
        return Err(PathError("path contains a NUL byte".into()));
    }
    if intent == PathIntent::Write && (raw.contains('\n') || raw.contains('\r')) {
        return Err(PathError(
            "write path contains a newline, which is not allowed".into(),
        ));
    }
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(PathError("empty path".into()));
    }

    let root_canon = root.canonicalize().map_err(|error| {
        PathError(format!(
            "project root `{}` is not available: {error}",
            root.display()
        ))
    })?;

    let joined = {
        let path = Path::new(raw);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            root_canon.join(path)
        }
    };
    let lexical = lexical_normalize(&joined)?;

    if let Ok(meta) = fs::symlink_metadata(&lexical) {
        if meta.file_type().is_symlink() {
            return follow_or_reject_symlink(&lexical, &root_canon, intent);
        }
    }

    let (ancestor, tail) = deepest_existing_ancestor(&lexical)?;
    let ancestor_canon = fs::canonicalize(&ancestor).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            PathError("dangling symlink".into())
        } else {
            PathError(format!("path error: {error}"))
        }
    })?;
    if ancestor_canon != root_canon && !ancestor_canon.starts_with(&root_canon) {
        return Err(escaped(&lexical, &root_canon));
    }

    let mut final_path = ancestor_canon;
    for part in tail {
        final_path.push(part);
    }
    if final_path == root_canon {
        if intent == PathIntent::Read {
            deny_sensitive(&final_path)?;
            return Ok(final_path);
        }
        return Err(PathError("refusing to modify the project root".into()));
    }
    if !final_path.starts_with(&root_canon) {
        return Err(escaped(&final_path, &root_canon));
    }
    deny_sensitive(&final_path)?;
    Ok(final_path)
}

pub fn move_to_trash(root: &Path, file: &Path) -> Result<PathBuf, String> {
    if !file.is_file() {
        return Err(format!("not a file: {}", file.display()));
    }
    let root_canon = root
        .canonicalize()
        .map_err(|error| format!("project root: {error}"))?;
    let file_canon = file
        .canonicalize()
        .map_err(|error| format!("could not trash `{}`: {error}", file.display()))?;
    if file_canon != root_canon && !file_canon.starts_with(&root_canon) {
        return Err("refusing to trash a file outside the project".into());
    }
    let root_string = root_canon.to_string_lossy();
    let trash = resolve(
        ".workbench-trash",
        Some(root_string.as_ref()),
        PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    fs::create_dir_all(&trash).map_err(|error| format!("could not create trash: {error}"))?;
    let trash = resolve(
        ".workbench-trash",
        Some(root_string.as_ref()),
        PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    if !trash.is_dir() {
        return Err("project trash path is not a directory".into());
    }
    let name = file_canon
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let dest = resolve(
        &trash.join(format!("{millis}-{name}")).to_string_lossy(),
        Some(root_string.as_ref()),
        PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    if fs::rename(&file_canon, &dest).is_err() {
        fs::copy(&file_canon, &dest)
            .map_err(|error| format!("could not trash `{}`: {error}", file.display()))?;
        fs::remove_file(&file_canon)
            .map_err(|error| format!("could not trash `{}`: {error}", file.display()))?;
    }
    Ok(dest)
}

pub fn reject_sensitive(path: &Path) -> Result<(), String> {
    deny_sensitive(path).map_err(|error| error.to_string())
}

fn deny_sensitive(path: &Path) -> Result<(), PathError> {
    let names = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let sensitive_name = |name: &str| {
        matches!(
            name,
            ".ssh"
                | ".gnupg"
                | "video-workbench"
                | "videoworkbench"
                | "settings.json"
                | "credentials"
                | "credential"
                | "id_rsa"
                | "id_ed25519"
                | "id_ecdsa"
                | "id_dsa"
                | "private_key"
                | "privatekey"
                | "auth.json"
                | "secrets.json"
                | "token"
                | "token.json"
        ) || name == ".env"
            || name.starts_with(".env.")
            || name.starts_with("credentials.")
            || name.starts_with("credential.")
            || name.ends_with(".pem")
            || name.ends_with(".key")
            || name.ends_with(".p12")
            || name.ends_with(".pfx")
    };
    if names.iter().any(|name| sensitive_name(name)) {
        return Err(PathError("refusing a sensitive path".into()));
    }
    Ok(())
}

fn follow_or_reject_symlink(
    link: &Path,
    root_canon: &Path,
    intent: PathIntent,
) -> Result<PathBuf, PathError> {
    if intent == PathIntent::Write {
        return Err(PathError("refusing to write through a symlink".into()));
    }
    let canon = fs::canonicalize(link).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            PathError("dangling symlink".into())
        } else {
            PathError(format!("path error: {error}"))
        }
    })?;
    if canon != *root_canon && canon.starts_with(root_canon) {
        deny_sensitive(&canon)?;
        Ok(canon)
    } else {
        Err(escaped(link, root_canon))
    }
}

fn deepest_existing_ancestor(path: &Path) -> Result<(PathBuf, Vec<std::ffi::OsString>), PathError> {
    let mut tail = Vec::new();
    let mut current = path.to_path_buf();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(_) => {
                tail.reverse();
                return Ok((current, tail));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = current
                    .file_name()
                    .ok_or_else(|| escaped(path, Path::new("")))?;
                tail.push(name.to_os_string());
                let Some(parent) = current.parent() else {
                    return Err(PathError("path escapes the project root".into()));
                };
                if parent == current {
                    return Err(PathError("path escapes the project root".into()));
                }
                current = parent.to_path_buf();
            }
            Err(error) => return Err(PathError(format!("path error: {error}"))),
        }
    }
}

fn lexical_normalize(path: &Path) -> Result<PathBuf, PathError> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                out.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(PathError("path escapes the project root".into()));
                }
            }
            Component::Normal(part) => {
                if part.to_string_lossy().contains('\0') {
                    return Err(PathError("path contains a NUL byte".into()));
                }
                out.push(part);
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err(PathError("empty path".into()));
    }
    Ok(out)
}

fn escaped(path: &Path, root: &Path) -> PathError {
    PathError(format!(
        "path `{}` escapes project root `{}`",
        path.display(),
        root.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        (dir, root)
    }

    fn root_str(root: &Path) -> String {
        root.to_string_lossy().into_owned()
    }

    #[test]
    fn rejects_empty_and_nul() {
        let (_dir, root) = root();
        let root = root_str(&root);
        assert!(resolve("  ", Some(&root), PathIntent::Read).is_err());
        assert!(resolve("a\0b", Some(&root), PathIntent::Read).is_err());
    }

    #[test]
    fn rejects_traversal_and_sibling_prefix_and_outside_absolute() {
        let (dir, root) = root();
        let root_s = root_str(&root);
        assert!(
            resolve("newdir/../../escaped.txt", Some(&root_s), PathIntent::Write).is_err(),
            "newdir/../../x"
        );
        assert!(
            resolve(
                "a/b/../../../../escaped2.txt",
                Some(&root_s),
                PathIntent::Write
            )
            .is_err(),
            "deep traversal"
        );
        let evil = dir.path().join("proj-evil").join("sub").join("x.txt");
        fs::create_dir_all(evil.parent().unwrap()).unwrap();
        assert!(
            resolve(evil.to_str().unwrap(), Some(&root_s), PathIntent::Write).is_err(),
            "sibling prefix"
        );
        assert!(resolve("/etc/passwd", Some(&root_s), PathIntent::Read).is_err());
    }

    #[test]
    fn rejects_no_project_absolute_path() {
        assert!(resolve("/etc/passwd", None, PathIntent::Read).is_err());
        assert!(resolve("/etc/passwd", Some("  "), PathIntent::Write).is_err());
        assert!(resolve("Scripts/a.md", None, PathIntent::Write).is_err());
    }

    #[test]
    fn rejects_write_newlines_and_dangling_or_escaping_symlinks() {
        let (dir, root) = root();
        let root_s = root_str(&root);
        assert!(resolve("Scripts/a\n.md", Some(&root_s), PathIntent::Write).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(root.join("missing-target"), root.join("dangling")).unwrap();
            let dangling = resolve("dangling", Some(&root_s), PathIntent::Write);
            assert!(dangling.is_err(), "{dangling:?}");

            let outside = dir.path().join("outside");
            fs::create_dir_all(&outside).unwrap();
            symlink(&outside, root.join("linkdir")).unwrap();
            let escaped = resolve("linkdir/new.txt", Some(&root_s), PathIntent::Write);
            assert!(escaped.is_err(), "{escaped:?}");
        }
    }

    #[test]
    fn rejects_sensitive_names_inside_the_project() {
        let (_dir, root) = root();
        let root_s = root_str(&root);
        assert!(resolve("Scripts/.env", Some(&root_s), PathIntent::Read).is_err());
        assert!(resolve("Characters/id_rsa", Some(&root_s), PathIntent::Read).is_err());
        assert!(resolve("keys/studio.pem", Some(&root_s), PathIntent::Write).is_err());
        assert!(resolve(".ssh/id_ed25519", Some(&root_s), PathIntent::Read).is_err());
    }

    #[test]
    fn reads_the_project_root_and_refuses_to_modify_it() {
        let (_dir, root) = root();
        let root_s = root_str(&root);
        let read = resolve(&root_s, Some(&root_s), PathIntent::Read).unwrap();
        assert_eq!(read.as_path(), root.canonicalize().unwrap());
        assert!(resolve(&root_s, Some(&root_s), PathIntent::Write).is_err());
        assert!(resolve(".", Some(&root_s), PathIntent::Read).is_ok());
        assert!(resolve(".", Some(&root_s), PathIntent::Write).is_err());
    }

    #[test]
    fn accepts_a_relative_path_inside_the_project() {
        let (_dir, root) = root();
        let root_s = root_str(&root);
        let path = resolve("Scripts/draft.md", Some(&root_s), PathIntent::Write).unwrap();
        assert_eq!(path.as_path(), root.join("Scripts/draft.md"));
        let folded = resolve(
            "Scripts/../Scripts/draft.md",
            Some(&root_s),
            PathIntent::Write,
        )
        .unwrap();
        assert_eq!(folded.as_path(), root.join("Scripts/draft.md"));
    }

    #[test]
    fn cut_and_scene_buckets_use_the_resolved_path() {
        let (_dir, root) = root();
        let root_s = root_str(&root);
        let take = resolve(
            "Scenes/Scene1/Cut1/../../Scene2/Cut9/take.mp4",
            Some(&root_s),
            PathIntent::Write,
        )
        .unwrap();
        assert!(under_cut_dir(&root, take.as_path(), 1, 1).is_err());
        assert!(under_cut_dir(&root, take.as_path(), 2, 9).is_ok());

        let sneaky = resolve(
            "Preproduction/Scene1/references/../../../Scripts/secret.md",
            Some(&root_s),
            PathIntent::Write,
        )
        .unwrap();
        assert!(under_scene_bucket(&root, sneaky.as_path(), "references").is_err());
    }

    #[test]
    fn sensitive_names_are_denied_in_any_path_component() {
        let (_dir, root) = root();
        let root_s = root_str(&root);
        for raw in [
            "notes/.env/child.txt",
            "notes/credentials.yaml",
            "keys/server.key/child.txt",
            "notes/secrets.json/child.txt",
        ] {
            assert!(
                resolve(raw, Some(&root_s), PathIntent::Read).is_err(),
                "accepted sensitive path {raw}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn move_to_trash_rejects_an_escaping_trash_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let source = root.join("take.png");
        fs::write(&source, b"image").unwrap();
        symlink(&outside, root.join(".workbench-trash")).unwrap();

        assert!(move_to_trash(&root, &source).is_err());
        assert!(source.is_file());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    }
}

//! Allowlists for model-emitted text writes.
//!
//! The model may supply content and a filename. The host decides which
//! directories and extensions that filename is allowed to use. Paths outside
//! those directories are rejected. Overwrites of protected project state are
//! rejected. An overwrite inside the allowlist is a host-owned destination, so
//! it can be written, and the caller reports a short change summary.

use crate::project_path::{self, PathIntent};
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_FILES: usize = 8;
pub const MAX_BYTES_PER_FILE: usize = 200_000;
pub const MAX_TOTAL_BYTES: usize = 400_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedWrite {
    pub path: PathBuf,
    pub relative: String,
    pub replacing: bool,
}

pub fn authorize_text_write(
    specialist: &str,
    raw_path: &str,
    project_root: Option<&str>,
    contents_len: usize,
    file_index: usize,
    total_bytes_before: usize,
) -> Result<AuthorizedWrite, String> {
    if file_index >= MAX_FILES {
        return Err(format!(
            "{specialist} can write at most {MAX_FILES} files in one reply"
        ));
    }
    if contents_len > MAX_BYTES_PER_FILE {
        return Err(format!(
            "{specialist} file is {contents_len} bytes; the limit is {MAX_BYTES_PER_FILE}"
        ));
    }
    if total_bytes_before.saturating_add(contents_len) > MAX_TOTAL_BYTES {
        return Err(format!(
            "{specialist} reply exceeds the {MAX_TOTAL_BYTES} byte write limit"
        ));
    }

    let confined = project_path::resolve(raw_path, project_root, PathIntent::Write)
        .map_err(|error| error.to_string())?;
    let root = project_root
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .ok_or_else(|| "open a project before writing files".to_string())?;
    let relative = project_path::relative_to_root(Path::new(root), confined.as_path())?;
    let relative_text = relative.to_string_lossy().replace('\\', "/");
    if is_protected(&relative_text) {
        return Err(format!(
            "refusing to write protected project file `{relative_text}`"
        ));
    }
    if !extension_allowed(specialist, &relative) {
        return Err(allowlist_error(specialist, &relative_text));
    }

    let replacing = fs::symlink_metadata(confined.as_path())
        .map(|meta| meta.file_type().is_file())
        .unwrap_or(false);
    Ok(AuthorizedWrite {
        path: confined.into_path_buf(),
        relative: relative_text,
        replacing,
    })
}

pub fn change_summary(previous: Option<&str>, next: &str) -> String {
    let Some(previous) = previous else {
        return format!("new file, {} characters", next.chars().count());
    };
    if previous == next {
        return "unchanged".into();
    }
    format!(
        "overwrite ({} → {} characters)",
        previous.chars().count(),
        next.chars().count()
    )
}

fn extension_allowed(specialist: &str, relative: &Path) -> bool {
    let ext = relative
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let under = |dir: &str| relative.starts_with(Path::new(dir)) && relative != Path::new(dir);
    match specialist {
        "script" => under("Scripts") && matches!(ext.as_str(), "md" | "txt"),
        "character_creation" => under("Characters") && ext == "md",
        "production_helper" => under("Research") && ext == "md",
        _ => false,
    }
}

fn is_protected(relative: &str) -> bool {
    let base = relative.rsplit('/').next().unwrap_or(relative);
    if matches!(
        base,
        "workflow_state.json" | "scene_plan_state.json" | "settings.json" | ".env" | "prod.md"
    ) {
        return true;
    }
    relative
        .split('/')
        .any(|part| matches!(part, ".git" | ".ssh" | ".gnupg"))
}

fn allowlist_error(specialist: &str, relative: &str) -> String {
    let rule = match specialist {
        "script" => "Scripts/**/*.md or Scripts/**/*.txt",
        "character_creation" => "Characters/**/*.md",
        "production_helper" => "Research/**/*.md",
        _ => "no model-chosen write path",
    };
    format!("`{relative}` is not an allowed {specialist} output ({rule})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(root.join("Scripts")).unwrap();
        fs::create_dir_all(root.join("Characters")).unwrap();
        fs::create_dir_all(root.join("Research")).unwrap();
        fs::create_dir_all(root.join("Preproduction")).unwrap();
        let root_s = root.to_string_lossy().into_owned();
        (dir, root_s)
    }

    #[test]
    fn script_rejects_traversal_absolute_and_protected_state() {
        let (_dir, root) = project();
        let root = Some(root.as_str());
        let traversal = authorize_text_write("script", "../../x.md", root, 10, 0, 0);
        assert!(traversal.is_err(), "{traversal:?}");
        let absolute = authorize_text_write("script", "/etc/passwd", root, 10, 0, 0);
        assert!(absolute.is_err(), "{absolute:?}");
        let state = authorize_text_write(
            "script",
            "Preproduction/workflow_state.json",
            root,
            10,
            0,
            0,
        );
        assert!(state.is_err(), "{state:?}");
        let via_scripts = authorize_text_write(
            "script",
            "Scripts/../Preproduction/workflow_state.json",
            root,
            10,
            0,
            0,
        );
        assert!(via_scripts.is_err(), "{via_scripts:?}");
        let outside_dir = authorize_text_write("script", "Notes/draft.md", root, 10, 0, 0);
        assert!(outside_dir.is_err(), "{outside_dir:?}");
    }

    #[test]
    fn script_and_character_and_digest_accept_their_directories() {
        let (_dir, root) = project();
        let root_ref = Some(root.as_str());
        let script = authorize_text_write("script", "Scripts/draft.md", root_ref, 12, 0, 0)
            .unwrap();
        assert_eq!(script.relative, "Scripts/draft.md");
        assert!(!script.replacing);
        let txt = authorize_text_write("script", "Scripts/notes/outline.txt", root_ref, 4, 0, 0);
        assert!(txt.is_ok(), "{txt:?}");
        let character = authorize_text_write(
            "character_creation",
            "Characters/Ada_ref.md",
            root_ref,
            8,
            0,
            0,
        )
        .unwrap();
        assert_eq!(character.relative, "Characters/Ada_ref.md");
        let digest =
            authorize_text_write("production_helper", "Research/digest.md", root_ref, 8, 0, 0)
                .unwrap();
        assert_eq!(digest.relative, "Research/digest.md");
        let helper_elsewhere =
            authorize_text_write("production_helper", "Scripts/digest.md", root_ref, 8, 0, 0);
        assert!(helper_elsewhere.is_err(), "{helper_elsewhere:?}");
    }

    #[test]
    fn script_marks_an_existing_allowlisted_file_as_a_replacement() {
        let (_dir, root) = project();
        let path = Path::new(&root).join("Scripts/draft.md");
        fs::write(&path, "old").unwrap();
        let write = authorize_text_write("script", "Scripts/draft.md", Some(&root), 3, 0, 0)
            .unwrap();
        assert!(write.replacing);
        assert_eq!(change_summary(Some("old"), "new"), "overwrite (3 → 3 characters)");
    }

    #[test]
    fn caps_file_count_and_bytes() {
        let (_dir, root) = project();
        let root = Some(root.as_str());
        assert!(authorize_text_write(
            "script",
            "Scripts/a.md",
            root,
            MAX_BYTES_PER_FILE + 1,
            0,
            0
        )
        .is_err());
        assert!(authorize_text_write("script", "Scripts/a.md", root, 10, MAX_FILES, 0).is_err());
        assert!(authorize_text_write(
            "script",
            "Scripts/a.md",
            root,
            10,
            1,
            MAX_TOTAL_BYTES
        )
        .is_err());
    }

    #[test]
    fn no_project_rejects_writes() {
        let error = authorize_text_write("script", "Scripts/draft.md", None, 4, 0, 0);
        assert!(error.is_err(), "{error:?}");
    }
}

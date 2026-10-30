//! Allowlists for model-emitted text writes.
//!
//! The model may supply content and a filename. The host decides which
//! directories and extensions that filename is allowed to use. Paths outside
//! those directories are rejected. Overwrites of protected project state are
//! rejected. A new file directly in the specialist directory can be written.
//! An overwrite, or a path nested deeper than that directory, is staged until
//! the user approves the change summary.

use crate::director_actions::{validate_action, ActionKind, DirectorAction};
use crate::project_path::{self, PathIntent};
use serde_json::{json, Map};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const MAX_FILES: usize = 8;
pub const MAX_BYTES_PER_FILE: usize = 200_000;
pub const MAX_TOTAL_BYTES: usize = 400_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedWrite {
    pub path: PathBuf,
    pub relative: String,
    pub replacing: bool,
    /// Nested under the specialist directory, so it is not a host default path.
    pub unusual: bool,
}

struct StagedBatch {
    specialist: String,
    files: Vec<StagedFile>,
}

struct StagedFile {
    relative: String,
    body: String,
    resolved_path: PathBuf,
    replacing: bool,
}

fn staged_store() -> &'static Mutex<HashMap<String, StagedBatch>> {
    static STORE: OnceLock<Mutex<HashMap<String, StagedBatch>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
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
        relative: relative_text.clone(),
        replacing,
        unusual: is_unusual(&relative_text),
    })
}

pub fn approval_required(write: &AuthorizedWrite) -> bool {
    write.replacing || write.unusual
}

pub fn approval_summary(write: &AuthorizedWrite, previous: Option<&str>, next: &str) -> String {
    let mut parts = Vec::new();
    if write.unusual {
        parts.push(format!(
            "unusual path `{}` (a host default is one file directly in the specialist directory)",
            write.relative
        ));
    }
    if write.replacing {
        parts.push(change_summary(previous, next));
    } else {
        parts.push(change_summary(None, next));
    }
    format!("`{}`: {}", write.relative, parts.join("; "))
}

/// Write a new default file. Overwrites and unusual paths stay on disk only
/// after approval.
pub fn commit_text_write(
    write: &AuthorizedWrite,
    body: &str,
    approved: bool,
) -> Result<(), String> {
    if approval_required(write) && !approved {
        let previous = fs::read_to_string(&write.path).ok();
        return Err(format!(
            "approval required: {}",
            approval_summary(write, previous.as_deref(), body)
        ));
    }
    if let Some(parent) = write.path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create directory {}: {error}", parent.display()))?;
    }
    fs::write(&write.path, body)
        .map_err(|error| format!("Could not write {}: {error}", write.path.display()))
}

pub fn stage_authorized_text_writes(
    specialist: &str,
    files: Vec<(AuthorizedWrite, String)>,
) -> String {
    let id = crate::pending::random_id();
    let batch = StagedBatch {
        specialist: specialist.to_string(),
        files: files
            .into_iter()
            .map(|(write, body)| StagedFile {
                relative: write.relative,
                body,
                resolved_path: write.path,
                replacing: write.replacing,
            })
            .collect(),
    };
    staged_store()
        .lock()
        .expect("staged text writes")
        .insert(id.clone(), batch);
    id
}

pub fn append_staged_paths_for_action(
    validated: &mut crate::director_actions::ValidatedAction,
) -> Result<(), String> {
    if validated.action.specialist.as_deref() != Some("text_write") {
        return Ok(());
    }
    let staged_id = validated
        .action
        .inputs
        .get("staged_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "text_write needs the host staged_id".to_string())?
        .to_string();
    append_staged_paths(&staged_id, validated)
}

fn append_staged_paths(
    staged_id: &str,
    validated: &mut crate::director_actions::ValidatedAction,
) -> Result<(), String> {
    let target_files: Vec<_> = staged_store()
        .lock()
        .expect("staged text writes")
        .get(staged_id)
        .ok_or_else(|| "That text change is no longer waiting for approval.".to_string())?
        .files
        .iter()
        .map(|file| (file.resolved_path.clone(), file.replacing))
        .collect();
    if target_files.is_empty() {
        return Err("The staged text change has no approved output files.".into());
    }
    for (index, (path, replacing)) in target_files.iter().enumerate() {
        if path.exists() != *replacing {
            return Err(format!(
                "Staged target `{}` changed between preparation and approval. Request a new plan.",
                path.display()
            ));
        }
        if *replacing {
            let label = if index == 0 {
                "existing_file".to_string()
            } else {
                format!("existing_file[{index}]")
            };
            validated.resolved_paths.push((label, path.clone()));
        }
        let label = if index == 0 {
            "output_path".to_string()
        } else {
            format!("output_path[{index}]")
        };
        validated.resolved_paths.push((label, path.clone()));
    }
    Ok(())
}

pub fn text_write_approval_card(
    staged_id: &str,
    summaries: &[String],
    project_root: Option<&str>,
) -> Result<String, String> {
    prepare_text_write_approval_card(staged_id, summaries, project_root)
        .map(|card| crate::pending::publish_user_text(&card))
}

pub(crate) fn prepare_text_write_approval_card(
    staged_id: &str,
    summaries: &[String],
    project_root: Option<&str>,
) -> Result<String, String> {
    let mut inputs = Map::new();
    inputs.insert("staged_id".into(), json!(staged_id));
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("text_write".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let mut validated = validate_action(action, None).map_err(|error| error.to_string())?;
    append_staged_paths(staged_id, &mut validated)?;
    let project_root = project_root
        .ok_or_else(|| "Text-write approval requires an active project root.".to_string())?;
    let context = crate::director::DirectorContext {
        project_root: Some(project_root.to_string()),
        ..Default::default()
    };
    let mut card = crate::director_actions::prepare_approval_request_with_context(
        &validated.action,
        &validated,
        &context,
        "Local filesystem",
        "None (host-side text write)",
        "No provider charge for this write; prior text generation is outside this approval.",
    );
    let mut changes = String::from("\n### Text changes\n");
    for summary in summaries {
        changes.push_str(&format!("- {summary}\n"));
    }
    let insert_at = card.find("<!--DIRECTOR_ACTION:").unwrap_or(card.len());
    card.insert_str(insert_at, &changes);
    Ok(card)
}

pub fn commit_staged_text_write(
    staged_id: &str,
    project_root: Option<&str>,
) -> Result<Vec<String>, String> {
    let batch = staged_store()
        .lock()
        .expect("staged text writes")
        .remove(staged_id)
        .ok_or_else(|| "That text change is no longer waiting for approval.".to_string())?;
    let mut written = Vec::new();
    for file in batch.files {
        let authorized = authorize_text_write(
            &batch.specialist,
            &file.relative,
            project_root,
            file.body.len(),
            0,
            0,
        )?;
        // Re-resolving a parent symlink must not move an approved write to a
        // different destination, even if both paths are inside allowed scope.
        if authorized.path != file.resolved_path || authorized.replacing != file.replacing {
            return Err(format!(
                "Staged target `{}` changed after approval. Request a new plan.",
                authorized.path.display()
            ));
        }
        commit_text_write(&authorized, &file.body, true)?;
        written.push(authorized.relative);
    }
    Ok(written)
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

fn is_unusual(relative: &str) -> bool {
    relative.split('/').filter(|part| !part.is_empty()).count() != 2
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
        let script =
            authorize_text_write("script", "Scripts/draft.md", root_ref, 12, 0, 0).unwrap();
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
        let write =
            authorize_text_write("script", "Scripts/draft.md", Some(&root), 3, 0, 0).unwrap();
        assert!(write.replacing);
        assert!(approval_required(&write));
        let refused = commit_text_write(&write, "new\n", false);
        assert!(refused.is_err(), "{refused:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert!(refused.unwrap_err().contains("overwrite"));
    }

    #[test]
    fn an_unusual_path_is_not_written_until_approved() {
        let (_dir, root) = project();
        let write =
            authorize_text_write("script", "Scripts/notes/outline.txt", Some(&root), 4, 0, 0)
                .unwrap();
        assert!(write.unusual);
        let path = Path::new(&root).join("Scripts/notes/outline.txt");
        assert!(commit_text_write(&write, "note\n", false).is_err());
        assert!(!path.exists());
        commit_text_write(&write, "note\n", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "note\n");
    }

    #[test]
    fn an_unusual_new_file_approval_summary_describes_creation() {
        let (_dir, root) = project();
        let body = "new digest\n";
        let write = authorize_text_write(
            "production_helper",
            "Research/notes/digest.md",
            Some(&root),
            body.len(),
            0,
            0,
        )
        .unwrap();
        assert!(write.unusual);
        assert!(!write.replacing);
        assert!(approval_required(&write));

        let summary = approval_summary(&write, None, body);
        assert!(summary.contains("unusual path `Research/notes/digest.md`"));
        assert!(summary.contains("new file, 11 characters"), "{summary}");
        assert!(!summary.contains("overwrite"), "{summary}");
        assert!(commit_text_write(&write, body, false).is_err());
        assert!(!write.path.exists());
    }

    #[test]
    fn a_new_file_in_the_specialist_directory_writes_without_approval() {
        let (_dir, root) = project();
        let write = authorize_text_write(
            "production_helper",
            "Research/digest.md",
            Some(&root),
            8,
            0,
            0,
        )
        .unwrap();
        assert!(!approval_required(&write));
        commit_text_write(&write, "digest\n", false).unwrap();
        assert_eq!(
            fs::read_to_string(Path::new(&root).join("Research/digest.md")).unwrap(),
            "digest\n"
        );
    }

    #[test]
    fn a_pending_new_file_card_lists_proposed_artifacts_without_writing() {
        let _pending_guard = crate::pending::test_pending_lock();
        let (_dir, root) = project();
        let relative = "Research/notes/digest.md";
        let body = "new digest\n";
        let write =
            authorize_text_write("production_helper", relative, Some(&root), body.len(), 0, 0)
                .unwrap();
        let path = write.path.clone();
        let summary = approval_summary(&write, None, body);
        let id = stage_authorized_text_writes("production_helper", vec![(write, body.into())]);
        let card = text_write_approval_card(&id, &[summary], Some(&root)).unwrap();
        let approval_id = card
            .split(crate::pending::APPROVE_TOKEN_PREFIX)
            .nth(1)
            .and_then(|rest| rest.strip_suffix("]]"))
            .expect("approval id");
        let pending = crate::pending::take(approval_id).unwrap();
        assert_eq!(pending.action.specialist.as_deref(), Some("text_write"));
        assert_eq!(pending.action.inputs.get("staged_id"), Some(&json!(id)));
        assert_eq!(
            pending
                .context
                .as_ref()
                .and_then(|value| value.get("projectRoot")),
            Some(&json!(root))
        );
        assert_eq!(pending.input_fingerprints, Some(json!({})));
        assert!(card.contains("new file, 11 characters"), "{card}");
        assert!(!card.contains("overwrite"), "{card}");
        assert!(!card.contains("existing_file"), "{card}");
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
        assert!(card.contains("### Proposed artifacts"), "{card}");
        assert!(!card.contains("### Artifacts modified"), "{card}");
        let proposed = card.split("### Proposed artifacts").nth(1).unwrap();
        assert!(proposed.contains("output_path"));
        assert!(proposed.contains(&path.display().to_string()));

        let written = commit_staged_text_write(&id, Some(&root)).unwrap();
        assert_eq!(written, vec![relative.to_string()]);
        assert_eq!(fs::read_to_string(&path).unwrap(), body);
        assert!(commit_staged_text_write(&id, Some(&root)).is_err());
    }

    #[test]
    fn a_staged_new_file_cannot_overwrite_a_target_created_while_pending() {
        let _pending_guard = crate::pending::test_pending_lock();
        let (_dir, root) = project();
        let relative = "Research/notes/digest.md";
        let body = "proposed digest\n";
        let write =
            authorize_text_write("production_helper", relative, Some(&root), body.len(), 0, 0)
                .unwrap();
        let path = write.path.clone();
        let summary = approval_summary(&write, None, body);
        let id = stage_authorized_text_writes("production_helper", vec![(write, body.into())]);
        let card = text_write_approval_card(&id, &[summary], Some(&root)).unwrap();
        let approval_id = card
            .split(crate::pending::APPROVE_TOKEN_PREFIX)
            .nth(1)
            .and_then(|rest| rest.strip_suffix("]]"))
            .expect("approval id");
        let pending = crate::pending::take(approval_id).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "created independently").unwrap();

        let mut validated = validate_action(pending.action, None).unwrap();
        let error = append_staged_paths_for_action(&mut validated).unwrap_err();
        assert!(
            error.contains("changed between preparation and approval"),
            "{error}"
        );
        let error = commit_staged_text_write(&id, Some(&root)).unwrap_err();
        assert!(error.contains("changed after approval"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "created independently");
    }

    #[test]
    fn an_approved_overwrite_uses_the_staged_body() {
        let _pending_guard = crate::pending::test_pending_lock();
        let (_dir, root) = project();
        let path = Path::new(&root).join("Research/digest.md");
        fs::write(&path, "old").unwrap();
        let write = authorize_text_write(
            "production_helper",
            "Research/digest.md",
            Some(&root),
            4,
            0,
            0,
        )
        .unwrap();
        let summary = approval_summary(&write, Some("old"), "new\n");
        let id = stage_authorized_text_writes("production_helper", vec![(write, "new\n".into())]);
        let card = text_write_approval_card(&id, &[summary], Some(&root)).unwrap();
        assert!(card.contains("overwrite"));
        assert!(card.contains(crate::pending::APPROVE_TOKEN_PREFIX));
        let approval_id = card
            .split(crate::pending::APPROVE_TOKEN_PREFIX)
            .nth(1)
            .and_then(|rest| rest.strip_suffix("]]"))
            .expect("approval id");
        let pending = crate::pending::take(approval_id).unwrap();
        assert_eq!(pending.action.specialist.as_deref(), Some("text_write"));
        assert_eq!(
            pending
                .context
                .as_ref()
                .and_then(|value| value.get("projectRoot")),
            Some(&json!(root))
        );
        assert!(pending.input_fingerprints.is_some());
        assert!(card.contains("Provider: Local filesystem"));
        assert!(card.contains("Model: None (host-side text write)"));
        assert!(!card.contains("Cost estimate:"));
        assert!(card.contains("### Files read or uploaded"));
        assert!(card.contains("### Proposed artifacts"));
        assert!(!card.contains("### Artifacts modified"));
        assert!(card.contains(&path.display().to_string()));
        assert!(card.contains("existing_file"));
        assert!(card.contains("output_path"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        let written = commit_staged_text_write(&id, Some(&root)).unwrap();
        assert_eq!(written, vec!["Research/digest.md".to_string()]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
    }

    #[test]
    fn caps_file_count_and_bytes() {
        let (_dir, root) = project();
        let root = Some(root.as_str());
        assert!(
            authorize_text_write("script", "Scripts/a.md", root, MAX_BYTES_PER_FILE + 1, 0, 0)
                .is_err()
        );
        assert!(authorize_text_write("script", "Scripts/a.md", root, 10, MAX_FILES, 0).is_err());
        assert!(
            authorize_text_write("script", "Scripts/a.md", root, 10, 1, MAX_TOTAL_BYTES).is_err()
        );
    }

    #[test]
    fn no_project_rejects_writes() {
        let error = authorize_text_write("script", "Scripts/draft.md", None, 4, 0, 0);
        assert!(error.is_err(), "{error:?}");
    }
}

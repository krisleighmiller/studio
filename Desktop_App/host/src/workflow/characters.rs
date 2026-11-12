//! Typed character profiles stored beside model-sheet and voice assets.
use crate::workflow::state::{character_id, CharacterProfile, ProjectState, STATE_REL};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_VERSION: u32 = 1;

/// Save only the selection, never the discovered inventory, files or readiness.
pub fn save_character_reference_selection(
    root: &Path,
    character_id: &str,
    asset_ids: Vec<String>,
) -> Result<crate::workflow::state::CharacterRecord, String> {
    use crate::workflow::{import, render_jobs, state::AssetRole};
    let _lock = import::lock_project(root)?;
    // Do not import/repair on an invalid request: failure must preserve prior bytes.
    let mut state = render_jobs::read_persisted(root)?;
    let indices: Vec<_> = state.characters.iter().enumerate()
        .filter(|(_, character)| character.id == character_id).map(|(index, _)| index).collect();
    if indices.len() != 1 { return Err("character not found or ambiguous".into()); }
    let index = indices[0];
    let character = &state.characters[index];
    let root_string = root.to_string_lossy();
    let dir = crate::project_path::resolve(&character.dir, Some(&root_string), crate::project_path::PathIntent::Read)
        .map_err(|error| error.to_string())?.into_path_buf();
    validate_character_directory(root, &dir, Some(&character.name))?;
    let mut seen = std::collections::HashSet::new();
    for id in &asset_ids {
        if id.is_empty() || !seen.insert(id) { return Err("empty or duplicate reference asset ID".into()); }
        let matches: Vec<_> = state.assets.iter().filter(|asset| asset.id == *id).collect();
        if matches.len() != 1 { return Err(format!("reference asset `{id}` not found or ambiguous")); }
        let asset = matches[0];
        if asset.character_id.as_deref() != Some(character_id)
            || !matches!(asset.role, AssetRole::IdentityReference | AssetRole::BodyReference | AssetRole::ExpressionReference)
        {
            return Err(format!("asset `{id}` is not an available reference for this character"));
        }
        let path = crate::project_path::resolve(&asset.path, Some(&root_string), crate::project_path::PathIntent::Read)
            .map_err(|error| error.to_string())?.into_path_buf();
        if !path.is_file() || !path.starts_with(&dir)
            || relative(root, &path)? != asset.path
            || !matches!(path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("png" | "jpg" | "jpeg" | "webp")) {
            return Err(format!("reference asset `{id}` is missing, redirected or not a character image"));
        }
    }
    state.characters[index].reference_asset_ids = asset_ids;
    import::persist_state_locked(root, &state)?;
    Ok(state.characters[index].clone())
}

/// Delete one saved uploaded reference and its project-owned copy as one transaction.
pub fn delete_character_reference(
    root: &Path,
    character_id: &str,
    asset_id: &str,
) -> Result<ProjectState, String> {
    use crate::workflow::{import, render_jobs, state::AssetRole};
    let _lock = import::lock_project(root)?;
    let mut state = render_jobs::read_persisted(root)?;
    let state_path = root.join(STATE_REL);
    let original_state_bytes = fs::read(&state_path)
        .map_err(|error| format!("could not capture prior workflow state: {error}"))?;
    let character_indices: Vec<_> = state.characters.iter().enumerate()
        .filter(|(_, character)| character.id == character_id).map(|(index, _)| index).collect();
    if character_indices.len() != 1 { return Err("character not found or ambiguous".into()); }
    let character_index = character_indices[0];
    let character = &state.characters[character_index];
    if !character.reference_asset_ids.iter().any(|id| id == asset_id)
        || !character.asset_ids.iter().any(|id| id == asset_id)
    {
        return Err("uploaded reference is not bound to this character".into());
    }
    let assets: Vec<_> = state.assets.iter().filter(|asset| {
        asset.id == asset_id
            && asset.character_id.as_deref() == Some(character_id)
            && matches!(asset.role, AssetRole::IdentityReference | AssetRole::BodyReference | AssetRole::ExpressionReference)
            && asset.notes.as_deref() == Some("uploaded reference image")
    }).collect();
    if assets.len() != 1 { return Err("saved uploaded reference not found or ambiguous".into()); }
    let asset = assets[0];
    let root_string = root.to_string_lossy();
    let character_dir = crate::project_path::resolve(
        &character.dir,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    ).map_err(|error| error.to_string())?.into_path_buf();
    validate_character_directory(root, &character_dir, Some(&character.name))?;
    let path = crate::project_path::resolve(
        &asset.path,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    ).map_err(|error| error.to_string())?.into_path_buf();
    if !path.is_file() || !path.starts_with(&character_dir)
        || relative(root, &path)? != asset.path
        || path.components().any(|part| part.as_os_str().to_string_lossy().eq_ignore_ascii_case("model_sheet"))
    {
        return Err("uploaded reference is missing, redirected, or is a generated model-sheet plate".into());
    }

    let trashed = crate::project_path::move_to_trash(root, &path)?;
    state.assets.retain(|asset| asset.id != asset_id);
    state.characters[character_index].asset_ids.retain(|id| id != asset_id);
    state.characters[character_index].reference_asset_ids.retain(|id| id != asset_id);
    for job in &mut state.jobs {
        job.input_asset_ids.retain(|id| id != asset_id);
        job.output_asset_ids.retain(|id| id != asset_id);
        for attempt in &mut job.attempts {
            attempt.output_asset_ids.retain(|id| id != asset_id);
        }
    }
    if let Err(error) = import::persist_state_locked(root, &state) {
        let mut rollback_errors = Vec::new();
        if let Err(restore) = fs::rename(&trashed, &path) {
            rollback_errors.push(format!("could not restore trashed reference: {restore}"));
        }
        if fs::read(&state_path).ok().as_deref() != Some(original_state_bytes.as_slice()) {
            let temp = state_path.with_extension("json.rollback.tmp");
            if let Err(restore) = fs::write(&temp, &original_state_bytes)
                .and_then(|_| fs::rename(&temp, &state_path))
            {
                rollback_errors.push(format!("could not restore workflow state: {restore}"));
            }
        }
        return if rollback_errors.is_empty() { Err(error) }
            else { Err(format!("{error}; deletion rollback failed: {}", rollback_errors.join("; "))) };
    }
    render_jobs::read_persisted(root)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CharacterProfileFile {
    profile_version: u32,
    name: String,
    #[serde(flatten)]
    profile: CharacterProfile,
}

pub fn load_profile(
    root: &Path,
    character_dir: &Path,
) -> Result<(Option<PathBuf>, CharacterProfile), String> {
    let root_string = root.to_string_lossy();
    let character_dir = crate::project_path::resolve(
        &character_dir.to_string_lossy(),
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    validate_character_directory(root, &character_dir, None)?;
    let path = crate::project_path::resolve(
        &character_dir.join("character.yaml").to_string_lossy(),
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    if !path.is_file() {
        return Ok((None, CharacterProfile::default()));
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read `{}`: {error}", path.display()))?;
    let file: CharacterProfileFile = serde_yaml_ng::from_str(&text)
        .map_err(|error| format!("could not parse `{}`: {error}", path.display()))?;
    if file.profile_version != PROFILE_VERSION {
        return Err(format!(
            "`{}` uses character profile version {}; expected {PROFILE_VERSION}",
            path.display(),
            file.profile_version
        ));
    }
    Ok((Some(path), clean_profile(file.profile)))
}

pub fn save_profile(
    root: &Path,
    state: &ProjectState,
    existing_id: Option<&str>,
    name: &str,
    profile: CharacterProfile,
) -> Result<String, String> {
    let name = valid_name(name)?;
    let character_dir = if let Some(existing_id) = existing_id {
        let character = state
            .characters
            .iter()
            .find(|character| character.id == existing_id)
            .ok_or_else(|| format!("character `{existing_id}` not found"))?;
        if character.name != name {
            return Err(
                "renaming an existing character is not supported; create a new profile instead"
                    .into(),
            );
        }
        character.dir.clone().into()
    } else {
        let id = character_id(&name);
        if state
            .characters
            .iter()
            .any(|character| character.id == id || character.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("character `{name}` already exists"));
        }
        PathBuf::from("Characters").join(&name)
    };
    let root_string = root.to_string_lossy();
    let character_dir = crate::project_path::resolve(
        &character_dir.to_string_lossy(),
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    validate_character_directory(root, &character_dir, Some(&name))?;
    fs::create_dir_all(&character_dir).map_err(|error| {
        format!(
            "could not create character directory `{}`: {error}",
            character_dir.display()
        )
    })?;
    let path = crate::project_path::resolve(
        &character_dir.join("character.yaml").to_string_lossy(),
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    let file = CharacterProfileFile {
        profile_version: PROFILE_VERSION,
        name,
        profile: clean_profile(profile),
    };
    let text = serde_yaml_ng::to_string(&file)
        .map_err(|error| format!("could not serialize character profile: {error}"))?;
    let temp = crate::project_path::resolve(
        &path.with_extension("yaml.tmp").to_string_lossy(),
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    fs::write(&temp, text)
        .map_err(|error| format!("could not write `{}`: {error}", temp.display()))?;
    fs::rename(&temp, &path)
        .map_err(|error| format!("could not save `{}`: {error}", path.display()))?;
    relative(root, &path)
}

fn validate_character_directory(
    root: &Path,
    character_dir: &Path,
    expected_name: Option<&str>,
) -> Result<(), String> {
    let relative = crate::project_path::relative_to_root(root, character_dir)?;
    let parts: Vec<_> = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();
    if parts.len() != 2
        || parts[0] != "Characters"
        || expected_name.is_some_and(|name| parts[1] != name)
    {
        return Err(
            "character profile path must be Characters/<character name> inside the project".into(),
        );
    }
    Ok(())
}

fn valid_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("character name cannot be empty".into());
    }
    if value == "."
        || value == ".."
        || value.contains(['/', '\\'])
        || value.chars().any(char::is_control)
    {
        return Err("character name cannot contain path separators or control characters".into());
    }
    Ok(value.to_string())
}

fn clean_profile(mut profile: CharacterProfile) -> CharacterProfile {
    profile.description = clean_option(profile.description);
    profile.identity_prompt = clean_option(profile.identity_prompt);
    profile.appearance = clean_list(profile.appearance);
    profile.wardrobe = clean_list(profile.wardrobe);
    profile.personality = clean_list(profile.personality);
    profile.continuity_notes = clean_list(profile.continuity_notes);
    profile
}

fn clean_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    let mut cleaned = Vec::new();
    for value in values {
        let value = value.trim().to_string();
        if !value.is_empty() && !cleaned.contains(&value) {
            cleaned.push(value);
        }
    }
    cleaned
}

fn relative(root: &Path, path: &Path) -> Result<String, String> {
    crate::project_path::relative_to_root(root, path)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::CharacterRecord;

    #[test]
    fn creates_and_updates_typed_profile() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let state = ProjectState::new();
        let path = save_profile(
            root,
            &state,
            None,
            "Tom Vale",
            CharacterProfile {
                description: Some("  Careful older brother. ".into()),
                appearance: vec![" dark hair ".into()],
                wardrobe: vec!["worn dark coat".into()],
                personality: vec!["responsible".into()],
                continuity_notes: vec!["guards the parcel".into()],
                identity_prompt: Some(" solemn twelve-year-old ".into()),
            },
        )
        .unwrap();
        assert_eq!(path, "Characters/Tom Vale/character.yaml");
        let (loaded_path, profile) = load_profile(root, &root.join("Characters/Tom Vale")).unwrap();
        assert_eq!(loaded_path.unwrap(), root.join(&path));
        assert_eq!(
            profile.description.as_deref(),
            Some("Careful older brother.")
        );
        assert_eq!(profile.appearance, vec!["dark hair"]);

        let state = ProjectState {
            characters: vec![CharacterRecord {
                id: "tom_vale".into(),
                name: "Tom Vale".into(),
                dir: "Characters/Tom Vale".into(),
                identity_ready: false,
                voice_ready: false,
                profile_path: Some(path),
                model_sheet_manifest_path: None,
                reference_asset_ids: Vec::new(),
                profile: profile.clone(),
                asset_ids: Vec::new(),
                notes: Vec::new(),
            }],
            ..ProjectState::new()
        };
        save_profile(
            root,
            &state,
            Some("tom_vale"),
            "Tom Vale",
            CharacterProfile {
                description: Some("Updated".into()),
                ..profile
            },
        )
        .unwrap();
        assert_eq!(
            load_profile(root, &root.join("Characters/Tom Vale"))
                .unwrap()
                .1
                .description
                .as_deref(),
            Some("Updated")
        );
    }

    #[test]
    fn rejects_duplicate_and_unsafe_names() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = ProjectState::new();
        state.characters.push(CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "Characters/Tom Vale".into(),
            identity_ready: false,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            reference_asset_ids: Vec::new(),
            profile: CharacterProfile::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        });
        assert!(save_profile(root, &state, None, "Tom Vale", CharacterProfile::default()).is_err());
        assert!(save_profile(root, &state, None, "../Nell", CharacterProfile::default()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn saving_existing_profile_rejects_traversal_and_escaping_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let character = CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "../../outside".into(),
            identity_ready: false,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            reference_asset_ids: Vec::new(),
            profile: CharacterProfile::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        };
        let state = ProjectState {
            characters: vec![character.clone()],
            ..ProjectState::new()
        };
        assert!(save_profile(
            &root,
            &state,
            Some("tom_vale"),
            "Tom Vale",
            CharacterProfile::default()
        )
        .is_err());
        assert!(!outside.join("character.yaml").exists());

        fs::create_dir_all(root.join("Characters")).unwrap();
        symlink(&outside, root.join("Characters/Tom Vale")).unwrap();
        let mut symlink_state = state;
        symlink_state.characters[0].dir = "Characters/Tom Vale".into();
        assert!(save_profile(
            &root,
            &symlink_state,
            Some("tom_vale"),
            "Tom Vale",
            CharacterProfile::default()
        )
        .is_err());
        assert!(!outside.join("character.yaml").exists());
    }

    #[test]
    fn importer_discovers_a_profile_only_character() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        save_profile(
            root,
            &ProjectState::new(),
            None,
            "Nell Vale",
            CharacterProfile {
                description: Some("Alert and practical.".into()),
                continuity_notes: vec!["Watches the platform.".into()],
                ..CharacterProfile::default()
            },
        )
        .unwrap();

        let imported = crate::workflow::import::import_project(
            root,
            crate::workflow::import::ImportOptions { persist: false },
        )
        .unwrap();
        let character = &imported.state.characters[0];
        assert_eq!(character.name, "Nell Vale");
        assert_eq!(
            character.profile_path.as_deref(),
            Some("Characters/Nell Vale/character.yaml")
        );
        assert_eq!(
            character.profile.description.as_deref(),
            Some("Alert and practical.")
        );
        assert!(!character.identity_ready);
        assert!(!character.voice_ready);
    }
}

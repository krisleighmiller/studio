//! Typed character profiles stored beside model-sheet and voice assets.
use crate::workflow::state::{character_id, CharacterProfile, ProjectState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CharacterProfileFile {
    profile_version: u32,
    name: String,
    #[serde(flatten)]
    profile: CharacterProfile,
}

pub fn load_profile(character_dir: &Path) -> Result<(Option<PathBuf>, CharacterProfile), String> {
    let path = character_dir.join("character.yaml");
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
        root.join(&character.dir)
    } else {
        let id = character_id(&name);
        if state
            .characters
            .iter()
            .any(|character| character.id == id || character.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("character `{name}` already exists"));
        }
        root.join("Characters").join(&name)
    };
    fs::create_dir_all(&character_dir).map_err(|error| {
        format!(
            "could not create character directory `{}`: {error}",
            character_dir.display()
        )
    })?;
    let path = character_dir.join("character.yaml");
    let file = CharacterProfileFile {
        profile_version: PROFILE_VERSION,
        name,
        profile: clean_profile(profile),
    };
    let text = serde_yaml_ng::to_string(&file)
        .map_err(|error| format!("could not serialize character profile: {error}"))?;
    let temp = path.with_extension("yaml.tmp");
    fs::write(&temp, text)
        .map_err(|error| format!("could not write `{}`: {error}", temp.display()))?;
    fs::rename(&temp, &path)
        .map_err(|error| format!("could not save `{}`: {error}", path.display()))?;
    Ok(relative(root, &path))
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

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
        let (loaded_path, profile) = load_profile(&root.join("Characters/Tom Vale")).unwrap();
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
            load_profile(&root.join("Characters/Tom Vale"))
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
            profile: CharacterProfile::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        });
        assert!(save_profile(root, &state, None, "Tom Vale", CharacterProfile::default()).is_err());
        assert!(save_profile(root, &state, None, "../Nell", CharacterProfile::default()).is_err());
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

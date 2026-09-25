//! Discover usable character assets under `<project>/Characters/` from the
//! model-sheet manifests and voice profiles the studio already writes.
//! Only shots that are `status: saved` *and* exist on disk count.
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotAsset {
    pub shot_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceAsset {
    pub profile_path: PathBuf,
    pub provider: String,
    pub voice_id: String,
    pub voice_name: Option<String>,
    /// On-disk clip named by the profile. Present for a local or recorded sample
    /// even when the profile has no provider voice id.
    pub sample_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterAssets {
    pub name: String,
    pub dir: PathBuf,
    pub manifest_path: Option<PathBuf>,
    /// Approval shot, only if the file exists.
    pub approval_shot: Option<PathBuf>,
    /// Saved shots that exist on disk.
    pub shots: Vec<ShotAsset>,
    /// Shot ids listed as pending / missing — not usable.
    pub pending_shots: Vec<String>,
    pub voice: Option<VoiceAsset>,
    pub notes: Vec<String>,
}

impl CharacterAssets {
    pub fn best_reference_image(&self) -> Option<&Path> {
        self.approval_shot
            .as_deref()
            .or_else(|| {
                self.shots
                    .iter()
                    .find(|s| s.shot_id == "closeup_front")
                    .map(|s| s.path.as_path())
            })
            .or_else(|| self.shots.first().map(|s| s.path.as_path()))
    }
    pub fn lipsync_ready(&self) -> bool {
        self.best_reference_image().is_some()
    }
    pub fn owns_image(&self, path: &str) -> bool {
        let p = Path::new(path.trim());
        self.approval_shot.as_deref() == Some(p) || self.shots.iter().any(|s| s.path == p)
    }
    pub fn matches_speaker(&self, speaker: &str) -> bool {
        let s = name_tokens(speaker);
        let n = name_tokens(&self.name);
        !s.is_empty() && s.iter().all(|t| n.contains(t))
    }
}

const TITLES: &[&str] = &[
    "mr", "mrs", "ms", "miss", "dr", "aunt", "uncle", "sir", "lady", "lord",
];

/// `"Guard, outside"` -> ["guard"], `"Aunt Marian"` -> ["marian"], `"Tom Vale"` -> ["tom","vale"].
pub fn name_tokens(name: &str) -> Vec<String> {
    let head = name.split(|c| c == ',' || c == '(').next().unwrap_or("");
    head.split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|t| !t.is_empty() && !TITLES.contains(&t.as_str()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetRegistry {
    pub root: Option<PathBuf>,
    pub characters: Vec<CharacterAssets>,
    pub scan_notes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Manifest {
    character: String,
    identity_source: IdentitySource,
    shots: Vec<ManifestShot>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct IdentitySource {
    approval_shot_path: String,
    reference_images: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ManifestShot {
    shot_id: String,
    path: String,
    status: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct VoiceProfile {
    character: String,
    provider: Option<String>,
    voice_id: Option<String>,
    voice_name: Option<String>,
    approved_audio: Vec<ApprovedAudio>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApprovedAudio {
    path: String,
}

fn resolve(raw: &str, dir: &Path) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let p = Path::new(raw);
    Some(if p.is_absolute() {
        p.to_path_buf()
    } else {
        dir.join(p)
    })
}

impl AssetRegistry {
    pub fn discover(project_root: &Path) -> AssetRegistry {
        let mut reg = AssetRegistry {
            root: Some(project_root.to_path_buf()),
            ..Default::default()
        };
        let chars_dir = project_root.join("Characters");
        let Ok(read) = fs::read_dir(&chars_dir) else {
            reg.scan_notes.push(format!(
                "no Characters/ directory at {}",
                chars_dir.display()
            ));
            return reg;
        };
        let mut dirs: Vec<PathBuf> = read
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            if let Some(c) = scan_character_dir(&dir, &mut reg.scan_notes) {
                reg.characters.push(c);
            }
        }
        reg
    }

    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }

    /// Exact full-name match first, then unique first-name / token subset.
    pub fn find_character(&self, speaker: &str) -> Option<&CharacterAssets> {
        let s = name_tokens(speaker);
        if s.is_empty() {
            return None;
        }
        if let Some(c) = self.characters.iter().find(|c| name_tokens(&c.name) == s) {
            return Some(c);
        }
        self.characters.iter().find(|c| c.matches_speaker(speaker))
    }

    pub fn character_for_image(&self, path: &str) -> Option<&CharacterAssets> {
        self.characters.iter().find(|c| c.owns_image(path))
    }

    /// Markdown for the planner prompt: exact paths the model may use.
    pub fn prompt_block(&self) -> String {
        let mut out = String::from(
            "## Character assets discovered by the host\n\nName the speaker; the host attaches their reference image. A speaker marked lip-sync ready can be lip-synced when on camera. Speakers not listed have no assets.\n\n",
        );
        if self.characters.is_empty() {
            out.push_str("- none found\n");
        }
        for c in &self.characters {
            let best = c.best_reference_image();
            let status = if best.is_some() {
                "lip-sync ready"
            } else {
                "NO usable reference image"
            };
            out.push_str(&format!("- **{}** — {status}.", c.name));
            if let Some(p) = best {
                out.push_str(&format!(" reference_image: `{}`", p.display()));
            }
            let others: Vec<&str> = c
                .shots
                .iter()
                .filter(|s| Some(s.path.as_path()) != best)
                .map(|s| s.shot_id.as_str())
                .take(6)
                .collect();
            if !others.is_empty() {
                out.push_str(&format!("; other saved shots: {}", others.join(", ")));
            }
            match &c.voice {
                Some(v) if !v.voice_id.trim().is_empty() => out.push_str(&format!(
                    "; voice: {} `{}`{}",
                    v.provider,
                    v.voice_id,
                    v.voice_name
                        .as_deref()
                        .map(|n| format!(" ({n})"))
                        .unwrap_or_default()
                )),
                Some(v) => {
                    out.push_str("; voice: local sample");
                    if let Some(path) = &v.sample_path {
                        out.push_str(&format!(" `{}`", path.display()));
                    }
                }
                None => out.push_str("; voice: none on file"),
            }
            if !c.pending_shots.is_empty() {
                out.push_str(&format!("; not usable: {}", c.pending_shots.join(", ")));
            }
            out.push('\n');
        }
        for n in &self.scan_notes {
            out.push_str(&format!("- note: {n}\n"));
        }
        out
    }
}

fn scan_character_dir(dir: &Path, notes: &mut Vec<String>) -> Option<CharacterAssets> {
    let dir_name = dir.file_name()?.to_string_lossy().to_string();
    let manifest_path = [
        dir.join("model_sheet_manifest.yaml"),
        dir.join("model_sheet").join("model_sheet_manifest.yaml"),
    ]
    .into_iter()
    .find(|p| p.is_file());
    let mut c = CharacterAssets {
        name: dir_name,
        dir: dir.to_path_buf(),
        manifest_path: manifest_path.clone(),
        approval_shot: None,
        shots: Vec::new(),
        pending_shots: Vec::new(),
        voice: None,
        notes: Vec::new(),
    };
    match &manifest_path {
        Some(mp) => {
            let parsed = fs::read_to_string(mp)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_yaml_ng::from_str::<Manifest>(&t).map_err(|e| e.to_string()));
            match parsed {
                Ok(m) => {
                    if !m.character.trim().is_empty() {
                        c.name = m.character.trim().to_string();
                    }
                    match resolve(&m.identity_source.approval_shot_path, dir) {
                        Some(p) if p.is_file() => c.approval_shot = Some(p),
                        Some(_) => c
                            .notes
                            .push("approval shot listed in manifest is missing on disk".into()),
                        None => {}
                    }
                    for s in m.shots {
                        let saved = s.status.trim().eq_ignore_ascii_case("saved");
                        match (saved, resolve(&s.path, dir)) {
                            (true, Some(p)) if p.is_file() => c.shots.push(ShotAsset {
                                shot_id: s.shot_id,
                                path: p,
                            }),
                            (true, _) => {
                                c.notes.push(format!(
                                    "shot {} marked saved but file is missing",
                                    s.shot_id
                                ));
                                c.pending_shots.push(s.shot_id);
                            }
                            (false, _) => c.pending_shots.push(s.shot_id),
                        }
                    }
                    for r in m.identity_source.reference_images {
                        if let Some(p) = resolve(&r, dir).filter(|p| p.is_file()) {
                            if !c.owns_image(&p.to_string_lossy()) {
                                c.shots.push(ShotAsset {
                                    shot_id: "reference_image".into(),
                                    path: p,
                                });
                            }
                        }
                    }
                }
                Err(e) => notes.push(format!(
                    "{}: could not parse model sheet manifest: {e}",
                    mp.display()
                )),
            }
        }
        None => c.notes.push("no model_sheet_manifest.yaml".into()),
    }
    c.voice = find_voice_profile(dir, &c.name, notes);
    Some(c)
}

fn approved_sample(character_dir: &Path, profile: &VoiceProfile) -> Option<PathBuf> {
    profile.approved_audio.iter().find_map(|audio| {
        let raw = audio.path.trim();
        if raw.is_empty() {
            return None;
        }
        let path = Path::new(raw);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            character_dir.join(path)
        };
        resolved.is_file().then_some(resolved)
    })
}

fn find_voice_profile(dir: &Path, character: &str, notes: &mut Vec<String>) -> Option<VoiceAsset> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for d in [dir.join("voice"), dir.to_path_buf()] {
        if let Ok(read) = fs::read_dir(&d) {
            for entry in read.flatten() {
                let p = entry.path();
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if p.is_file()
                    && name.contains("voice_profile")
                    && (name.ends_with(".yaml") || name.ends_with(".yml"))
                {
                    candidates.push(p);
                }
            }
        }
    }
    candidates.sort();
    for p in candidates {
        match fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_yaml_ng::from_str::<VoiceProfile>(&t).map_err(|e| e.to_string()))
        {
            Ok(v) => {
                let voice_id = v
                    .voice_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .unwrap_or("")
                    .to_string();
                let sample_path = approved_sample(dir, &v);
                if voice_id.is_empty() && sample_path.is_none() {
                    notes.push(format!(
                        "{}: voice profile has no voice_id and no audio file on disk",
                        p.display()
                    ));
                    continue;
                }
                if !v.character.trim().is_empty()
                    && name_tokens(&v.character) != name_tokens(character)
                {
                    notes.push(format!(
                        "{}: voice profile is for `{}` but sits under `{character}`",
                        p.display(),
                        v.character
                    ));
                }
                return Some(VoiceAsset {
                    profile_path: p,
                    provider: v
                        .provider
                        .as_deref()
                        .map(str::trim)
                        .filter(|provider| !provider.is_empty())
                        .unwrap_or("")
                        .to_string(),
                    voice_id,
                    voice_name: v
                        .voice_name
                        .map(|n| n.trim().to_string())
                        .filter(|n| !n.is_empty()),
                    sample_path,
                });
            }
            Err(e) => notes.push(format!(
                "{}: could not parse voice profile: {e}",
                p.display()
            )),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::fixtures::write_fixture_project;

    #[test]
    fn name_tokens_strip_titles_and_asides() {
        assert_eq!(name_tokens("Guard, outside"), vec!["guard"]);
        assert_eq!(name_tokens("Aunt Marian"), vec!["marian"]);
        assert_eq!(name_tokens("Mrs. Finch (off)"), vec!["finch"]);
        assert_eq!(name_tokens("Tom Vale"), vec!["tom", "vale"]);
    }

    #[test]
    fn discovers_saved_shots_voices_and_gaps() {
        let dir = tempfile::tempdir().unwrap();
        let fx = write_fixture_project(dir.path());
        let reg = AssetRegistry::discover(dir.path());
        let names: Vec<&str> = reg.characters.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Aunt Marian",
                "Mr Harker",
                "Nell Vale",
                "Pip Vale",
                "Tom Vale"
            ]
        );

        let tom = reg.find_character("Tom").unwrap();
        assert!(tom.lipsync_ready());
        assert_eq!(tom.best_reference_image(), Some(fx.tom_closeup.as_path()));
        assert_eq!(tom.shots.len(), 2, "{:?}", tom.shots);
        assert!(tom.pending_shots.contains(&"expression_sad".to_string()));
        assert!(tom.voice.is_none());
        assert!(tom.owns_image(&fx.tom_standing.to_string_lossy()));

        let nell = reg.find_character("Nell Vale").unwrap();
        assert!(nell.lipsync_ready());
        assert!(nell
            .pending_shots
            .contains(&"expression_excited".to_string()));
        let nell_voice = nell.voice.as_ref().unwrap();
        assert_eq!(nell_voice.provider, "elevenlabs");

        let pip = reg.find_character("Pip").unwrap();
        assert_eq!(pip.voice.as_ref().unwrap().voice_id, "MlxAbUenU34aJO4OFnwY");
        assert_eq!(
            pip.voice.as_ref().unwrap().voice_name.as_deref(),
            Some("Benny")
        );

        let marian = reg.find_character("Marian").unwrap();
        assert_eq!(marian.name, "Aunt Marian");
        assert!(!marian.lipsync_ready());

        let harker = reg.find_character("Mr Harker").unwrap();
        assert!(!harker.lipsync_ready());
        assert!(harker.voice.is_some());

        assert!(reg.find_character("Guard, outside").is_none());
        assert_eq!(
            reg.character_for_image(&fx.nell_closeup.to_string_lossy())
                .unwrap()
                .name,
            "Nell Vale"
        );
    }

    #[test]
    fn prompt_block_lists_exact_paths_and_readiness() {
        let dir = tempfile::tempdir().unwrap();
        let fx = write_fixture_project(dir.path());
        let block = AssetRegistry::discover(dir.path()).prompt_block();
        assert!(block.contains(&format!(
            "**Tom Vale** — lip-sync ready. reference_image: `{}`",
            fx.tom_closeup.display()
        )));
        assert!(block.contains("**Aunt Marian** — NO usable reference image"));
        assert!(block.contains("voice: elevenlabs `MlxAbUenU34aJO4OFnwY` (Benny)"));
        assert!(block.contains("not usable: expression_excited"));
    }

    #[test]
    fn local_sample_without_voice_id_counts_as_voice() {
        let dir = tempfile::tempdir().unwrap();
        let voice = dir.path().join("Characters/Tom Vale/voice");
        fs::create_dir_all(&voice).unwrap();
        fs::write(voice.join("Tom_voice_sample_01.wav"), b"RIFF").unwrap();
        fs::write(
            voice.join("Tom_voice_profile.yaml"),
            "\
profile_version: 1
character: \"Tom Vale\"
source_mode: existing_file
provider: null
voice_id: null
voice_name: null
approved_audio:
- path: \"voice/Tom_voice_sample_01.wav\"
  purpose: canonical_sample
",
        )
        .unwrap();

        let tom = AssetRegistry::discover(dir.path())
            .find_character("Tom")
            .unwrap()
            .clone();
        let voice_asset = tom.voice.as_ref().unwrap();
        assert!(voice_asset.voice_id.is_empty());
        assert!(voice_asset
            .sample_path
            .as_ref()
            .unwrap()
            .ends_with("Tom_voice_sample_01.wav"));
        let block = AssetRegistry::discover(dir.path()).prompt_block();
        assert!(block.contains("voice: local sample"));
        assert!(block.contains("Tom_voice_sample_01.wav"));
    }

    #[test]
    fn missing_characters_dir_is_a_note_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let reg = AssetRegistry::discover(dir.path());
        assert!(reg.is_empty());
        assert!(reg.scan_notes[0].contains("no Characters/ directory"));
        assert!(reg.prompt_block().contains("- none found"));
    }
}

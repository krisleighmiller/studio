//! Scan an existing project folder into V2 workflow state.
//!
//! Does not generate media or call the LLM. Existing `Scripts/`, `Characters/`,
//! `prod.md`, `Scene_Prompts/`, and `scene_plan_state.json` are mapped in place.
use crate::scene_plan::assets::AssetRegistry;
use crate::scene_plan::commit::write_atomic;
use crate::scene_plan::extract::{
    load_handoff_document, parse_handoff_yaml, scene_blocks_from_text,
};
use crate::scene_plan::handoff::{AssetRole as HandoffAssetRole, CutId, SceneHandoff};
use crate::scene_plan::state::{scene_number_from_stem, PlanState};
use crate::script::{aspect_from_text, extract_scenes, ParsedScene};
use crate::workflow::report::{project_report, ProjectReport};
use crate::workflow::state::{
    character_id, role_for_shot_id, AssetRecord, AssetRole, AssetStatus, CharacterRecord,
    ProjectState, ScriptRecord, StageStatus, TakeRecord, TakeStatus, STATE_REL, WORKFLOW_VERSION,
};
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportResult {
    pub state: ProjectState,
    pub report: ProjectReport,
    pub persisted_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub persist: bool,
}

fn cut_file_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^cut(\d+)\.(txt|md)$").unwrap())
}

fn cut_dir_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^cut[_ -]?(\d+)$").unwrap())
}

fn cut_number_from_name(name: &str) -> Option<u32> {
    cut_file_re()
        .captures(name)
        .and_then(|c| c.get(1)?.as_str().parse().ok())
        .or_else(|| {
            cut_dir_re()
                .captures(name)
                .and_then(|c| c.get(1)?.as_str().parse().ok())
        })
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"))
}

fn list_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    out.sort();
    out
}

fn is_script_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "md" | "markdown" | "txt")
    )
}

fn discover_scripts(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let scripts = root.join("Scripts");
    if scripts.is_dir() {
        for p in list_files(&scripts) {
            if p.is_file() && is_script_path(&p) {
                out.push(p);
            }
        }
    }
    out
}

fn discover_prod_md(root: &Path, plan: Option<&PlanState>) -> Option<PathBuf> {
    if let Some(plan) = plan {
        for section in &plan.sections {
            if let Some(p) = &section.production_path {
                let path = PathBuf::from(p);
                if path.is_file() {
                    return Some(path);
                }
                let rel_path = root.join(p);
                if rel_path.is_file() {
                    return Some(rel_path);
                }
            }
        }
    }
    let candidates = [
        root.join("prod.md"),
        root.join("Prod_Notes").join("prod.md"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn discover_sidecar_root(root: &Path, plan: Option<&PlanState>) -> PathBuf {
    if let Some(plan) = plan.filter(|p| !p.sidecar_root.trim().is_empty()) {
        let recorded = PathBuf::from(&plan.sidecar_root);
        if recorded.is_dir() {
            return recorded;
        }
        let rel_path = root.join(&plan.sidecar_root);
        if rel_path.is_dir() {
            return rel_path;
        }
    }
    for name in ["Scene_Prompts", "Prod_Notes"] {
        let p = root.join(name);
        if p.is_dir() {
            return p;
        }
    }
    root.join("Scene_Prompts")
}

fn load_plan_state(root: &Path) -> Option<PlanState> {
    for p in [
        root.join("scene_plan_state.json"),
        root.join("Preproduction").join("scene_plan_state.json"),
    ] {
        if let Ok(text) = fs::read_to_string(&p) {
            if let Ok((state, _)) = PlanState::load(&text) {
                return Some(state);
            }
        }
    }
    None
}

fn load_previous(root: &Path) -> Option<ProjectState> {
    let path = root.join(STATE_REL);
    fs::read_to_string(path)
        .ok()
        .and_then(|t| ProjectState::from_json(&t).ok())
}

fn apply_script_scene(state: &mut ProjectState, source_path: &str, parsed: ParsedScene) {
    let scene = state.scene_mut(parsed.id, &parsed.title);
    scene.source_path = Some(source_path.to_string());
    scene.source_excerpt = Some(parsed.source_text.clone());
    scene.title = parsed.title;
    scene.location = parsed.location;
    scene.action = parsed.action;
    scene.sound = parsed.sound;
    scene.transitions = parsed.transitions;
    scene.dialogue = parsed.dialogue;
    scene.beats = parsed.beats;
    scene.stages.script = StageStatus::Present;
}

fn import_characters(root: &Path, state: &mut ProjectState) {
    let registry = AssetRegistry::discover(root);
    state.notes.extend(registry.scan_notes.clone());
    for c in registry.characters {
        let id = character_id(&c.name);
        let mut asset_ids = Vec::new();
        let mut push =
            |state: &mut ProjectState, path: PathBuf, role: AssetRole, notes: Option<String>| {
                let rec = AssetRecord {
                    id: String::new(),
                    path: rel(root, &path),
                    role,
                    status: AssetStatus::Present,
                    scene_id: None,
                    cut_id: None,
                    character_id: Some(id.clone()),
                    source_job_id: None,
                    parent_asset_id: None,
                    notes,
                };
                asset_ids.push(state.push_asset(rec));
            };
        if let Some(p) = &c.approval_shot {
            push(
                state,
                p.clone(),
                AssetRole::IdentityReference,
                Some("approval shot".into()),
            );
        }
        for shot in &c.shots {
            if c.approval_shot.as_ref() == Some(&shot.path) {
                continue;
            }
            push(
                state,
                shot.path.clone(),
                role_for_shot_id(&shot.shot_id),
                Some(shot.shot_id.clone()),
            );
        }
        if let Some(v) = &c.voice {
            let note = if v.voice_id.trim().is_empty() {
                let file = v
                    .sample_path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("sample");
                format!("local sample {file}")
            } else {
                format!("{} {}", v.provider, v.voice_id)
            };
            push(
                state,
                v.profile_path.clone(),
                AssetRole::VoiceProfile,
                Some(note),
            );
            if let Some(sample) = &v.sample_path {
                push(
                    state,
                    sample.clone(),
                    AssetRole::VoiceSample,
                    Some("canonical sample".into()),
                );
            }
        }
        let identity_ready = c.lipsync_ready();
        let voice_ready = c.voice.is_some();
        let (profile_path, profile) = match crate::workflow::characters::load_profile(&c.dir) {
            Ok((path, profile)) => (path.map(|path| rel(root, &path)), profile),
            Err(error) => {
                state.notes.push(error);
                (None, Default::default())
            }
        };
        state.characters.push(CharacterRecord {
            id,
            name: c.name,
            dir: rel(root, &c.dir),
            identity_ready,
            voice_ready,
            profile_path,
            model_sheet_manifest_path: c.manifest_path.as_ref().map(|path| rel(root, path)),
            profile,
            asset_ids,
            notes: c.notes,
        });
    }
}

fn apply_handoff(root: &Path, state: &mut ProjectState, handoff: SceneHandoff, source_path: &Path) {
    let id = handoff.id();
    let title = handoff.title.clone().unwrap_or_else(|| id.to_string());
    let scene = state.scene_mut(id.source_scene, &format!("Scene {}", id.source_scene));
    let cut = scene.cut_mut(id.cut);
    cut.title = handoff.title.clone().or(cut.title.clone());
    cut.speech_mode = handoff.speech.mode;
    cut.scene_type = Some(handoff.scene_type);
    cut.render_lane = handoff.derived.as_ref().map(|d| d.render_lane);
    cut.prompt = Some(handoff.prompt.clone());
    cut.negative_prompt = handoff.negative_prompt.clone();
    cut.production_note_path = Some(rel(root, source_path));
    cut.dialogue = handoff
        .speech
        .cues
        .iter()
        .map(|c| crate::script::DialogueLine {
            speaker: c.speaker.clone(),
            text: c.text.clone(),
        })
        .collect();
    cut.subject = handoff
        .speech
        .cues
        .iter()
        .find(|c| c.on_camera)
        .map(|c| c.speaker.clone())
        .or_else(|| handoff.distinct_speakers().into_iter().next());
    cut.identity_ref_paths = handoff
        .reference_assets
        .iter()
        .filter(|a| a.role == HandoffAssetRole::CharacterReference)
        .map(|a| rel(root, Path::new(&a.path)))
        .collect();
    if let Some(start) = handoff
        .reference_assets
        .iter()
        .find(|a| a.role == HandoffAssetRole::StartFrame)
    {
        let path = rel(root, Path::new(&start.path));
        cut.approved_start_frame = Some(path.clone());
        state.push_asset(AssetRecord {
            id: String::new(),
            path,
            role: AssetRole::CutStartFrame,
            status: AssetStatus::Approved,
            scene_id: Some(id.source_scene),
            cut_id: Some(id),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: Some(title),
        });
    }
}

fn import_sidecars(root: &Path, sidecar_root: &Path, state: &mut ProjectState) {
    if !sidecar_root.is_dir() {
        return;
    }
    for scene_dir in list_files(sidecar_root) {
        if !scene_dir.is_dir() {
            continue;
        }
        if scene_dir
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(scene_number_from_stem)
            .is_none()
        {
            continue;
        }
        for file in list_files(&scene_dir) {
            if !file.is_file() {
                continue;
            }
            let Some(cut) = file
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(cut_number_from_name)
            else {
                continue;
            };
            let Ok(text) = fs::read_to_string(&file) else {
                state
                    .notes
                    .push(format!("could not read {}", file.display()));
                continue;
            };
            match load_handoff_document(&text).or_else(|_| parse_handoff_yaml(&text)) {
                Ok(h) => apply_handoff(root, state, h, &file),
                Err(e) => state.notes.push(format!(
                    "sidecar `{}` (Cut {cut}) did not parse: {e}",
                    rel(root, &file)
                )),
            }
        }
    }
}

fn import_prod_md(root: &Path, prod: &Path, state: &mut ProjectState) {
    let Ok(text) = fs::read_to_string(prod) else {
        state
            .notes
            .push(format!("could not read {}", prod.display()));
        return;
    };
    match scene_blocks_from_text(&rel(root, prod), &text) {
        Ok(blocks) => {
            for block in blocks {
                let already = state
                    .scene(block.heading.source_scene)
                    .and_then(|s| s.cuts.iter().find(|c| c.id == block.heading))
                    .is_some_and(|c| c.has_production_note());
                if already {
                    continue;
                }
                match parse_handoff_yaml(&block.yaml) {
                    Ok(h) => apply_handoff(root, state, h, prod),
                    Err(e) => {
                        state
                            .notes
                            .push(format!("{}: {}: {e}", rel(root, prod), block.heading))
                    }
                }
            }
        }
        Err(issues) => {
            if let Ok(h) = load_handoff_document(&text) {
                apply_handoff(root, state, h, prod);
            } else {
                for i in issues {
                    state.notes.push(format!("{}: {i}", rel(root, prod)));
                }
            }
        }
    }
}

fn import_plan_coverage(plan: &PlanState, state: &mut ProjectState) {
    for section in &plan.sections {
        let scene = state.scene_mut(section.source_scene, &section.title);
        if scene.title.starts_with("Scene ")
            && scene.title == format!("Scene {}", section.source_scene)
        {
            scene.title = section.title.clone();
        } else if scene.source_excerpt.is_none() {
            scene.title = section.title.clone();
        }
        for cut_n in &section.cuts {
            let _ = scene.cut_mut(*cut_n);
        }
    }
}

fn import_takes(root: &Path, state: &mut ProjectState) {
    let scenes_dir = root.join("Scenes");
    if !scenes_dir.is_dir() {
        return;
    }
    for scene_dir in list_files(&scenes_dir) {
        if !scene_dir.is_dir() {
            continue;
        }
        let Some(scene_id) = scene_dir
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(scene_number_from_stem)
        else {
            continue;
        };
        for cut_dir in list_files(&scene_dir) {
            let cut_n = cut_dir
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(cut_number_from_name);
            let search = if cut_dir.is_dir() {
                list_files(&cut_dir)
            } else {
                vec![cut_dir.clone()]
            };
            for file in search {
                let ext = file
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase());
                let role = match ext.as_deref() {
                    Some("mp4" | "webm" | "mov" | "mkv") => AssetRole::VideoTake,
                    Some("wav" | "mp3" | "m4a" | "flac" | "ogg") => AssetRole::AudioTake,
                    _ => continue,
                };
                let cut_id = cut_n.map(|c| CutId::new(scene_id, c));
                if let Some(id) = cut_id {
                    let scene = state.scene_mut(scene_id, &format!("Scene {scene_id}"));
                    let _ = scene.cut_mut(id.cut);
                }
                let path = rel(root, &file);
                let asset_id = state.push_asset(AssetRecord {
                    id: String::new(),
                    path: path.clone(),
                    role,
                    status: AssetStatus::Present,
                    scene_id: Some(scene_id),
                    cut_id,
                    character_id: None,
                    source_job_id: None,
                    parent_asset_id: None,
                    notes: None,
                });
                if role == AssetRole::VideoTake {
                    if let Some(cut_id) = cut_id {
                        let cut = state
                            .scene_mut(scene_id, &format!("Scene {scene_id}"))
                            .cut_mut(cut_id.cut);
                        if !cut.takes.iter().any(|take| take.path == path) {
                            cut.takes.push(TakeRecord {
                                id: asset_id,
                                path,
                                status: TakeStatus::Unreviewed,
                                notes: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn refresh_stage_flags(state: &mut ProjectState) {
    crate::workflow::recipe::apply_stages(state);
}

fn merge_preserved(previous: ProjectState, state: &mut ProjectState) {
    let saved = previous.clone();
    for old_scene in previous.scenes {
        let scene = state.scene_mut(old_scene.id, &old_scene.title);
        if scene.intent.is_none() {
            scene.intent = old_scene.intent;
        }
        if !scene.intent_approved {
            scene.intent_approved = old_scene.intent_approved;
        }
        if scene.strategy.is_none() {
            scene.strategy = old_scene.strategy;
        }
        if !scene.strategy_override {
            scene.strategy_override = old_scene.strategy_override;
        }
        if scene.look_board.is_none() {
            scene.look_board = old_scene.look_board;
        }
        for old_cut in old_scene.cuts {
            let old_selected = old_cut.selected_take.clone();
            let old_selected_path = old_selected.as_ref().and_then(|selected| {
                old_cut
                    .takes
                    .iter()
                    .find(|take| take.id == *selected || take.path == *selected)
                    .map(|take| take.path.clone())
            });
            let cut = scene.cut_mut(old_cut.id.cut);
            if cut.sketch.is_none() {
                cut.sketch = old_cut.sketch;
            }
            if cut.approved_start_frame.is_none() {
                cut.approved_start_frame = old_cut.approved_start_frame;
            }
            if cut.start_frame_prompt.is_none() {
                cut.start_frame_prompt = old_cut.start_frame_prompt;
            }
            if cut.start_frame_negative_prompt.is_none() {
                cut.start_frame_negative_prompt = old_cut.start_frame_negative_prompt;
            }
            if cut.visual_source.is_none() {
                cut.visual_source = old_cut.visual_source;
            }
            for old_take in old_cut.takes {
                if let Some(current) = cut.takes.iter_mut().find(|take| take.path == old_take.path)
                {
                    current.status = old_take.status;
                    current.notes = old_take.notes;
                } else {
                    cut.takes.push(old_take);
                }
            }
            if cut.selected_take.is_none() {
                cut.selected_take = old_selected_path
                    .as_ref()
                    .and_then(|path| {
                        cut.takes
                            .iter()
                            .find(|take| take.path == *path)
                            .map(|take| take.id.clone())
                    })
                    .or(old_selected);
            }
            if !cut.package_needs_review {
                cut.package_needs_review = old_cut.package_needs_review;
            }
        }
        if scene.animatic.is_none() {
            scene.animatic = old_scene.animatic;
        }
        if scene.post.is_none() {
            scene.post = old_scene.post;
        }
    }
    if state.jobs.is_empty() {
        state.jobs = previous.jobs;
    }
    if previous.assets.iter().any(|asset| {
        asset.role.is_scene_look()
            || asset.role.is_cut_image()
            || asset.role == AssetRole::WardrobePlate
    }) {
        for old in previous.assets {
            if (old.role.is_scene_look()
                || old.role.is_cut_image()
                || old.role == AssetRole::WardrobePlate)
                && !state
                    .assets
                    .iter()
                    .any(|a| a.path == old.path && a.role == old.role)
            {
                state.assets.push(old);
            }
        }
    }
    uniquify_duplicate_asset_ids(state);
    crate::workflow::identity::adopt(state, &saved);
}

fn prune_missing_scene_look_assets(project_root: &Path, state: &mut ProjectState) -> bool {
    let missing_assets: Vec<(String, String)> = state
        .assets
        .iter()
        .filter(|asset| asset.role.is_scene_look() || asset.role.is_cut_image())
        .filter(|asset| {
            let path = PathBuf::from(&asset.path);
            let path = if path.is_absolute() {
                path
            } else {
                project_root.join(path)
            };
            !path.is_file()
        })
        .map(|asset| (asset.id.clone(), asset.path.clone()))
        .collect();
    let missing: HashSet<String> = missing_assets.iter().map(|(id, _)| id.clone()).collect();
    let missing_paths: HashSet<String> = missing_assets.into_iter().map(|(_, path)| path).collect();
    if missing.is_empty() {
        return false;
    }
    state.assets.retain(|asset| !missing.contains(&asset.id));
    for asset in &mut state.assets {
        if asset
            .parent_asset_id
            .as_ref()
            .is_some_and(|id| missing.contains(id))
        {
            asset.parent_asset_id = None;
        }
    }
    for scene in &mut state.scenes {
        if let Some(board) = &mut scene.look_board {
            for slot in &mut board.slots {
                slot.asset_ids.retain(|id| !missing.contains(id));
                if slot
                    .approved_asset_id
                    .as_ref()
                    .is_some_and(|id| missing.contains(id))
                {
                    slot.approved_asset_id = None;
                }
            }
            board.sync_asset_ids();
        }
        for cut in &mut scene.cuts {
            if cut
                .approved_start_frame
                .as_ref()
                .is_some_and(|path| missing_paths.contains(path))
            {
                cut.approved_start_frame = None;
            }
            if cut
                .visual_source
                .as_ref()
                .and_then(|source| source.asset_id.as_ref())
                .is_some_and(|id| missing.contains(id))
            {
                if let Some(source) = &mut cut.visual_source {
                    source.asset_id = None;
                    source.source_take_id = None;
                    source.frame_time_seconds = None;
                }
            }
        }
    }
    for job in &mut state.jobs {
        job.input_asset_ids.retain(|id| !missing.contains(id));
        job.output_asset_ids.retain(|id| !missing.contains(id));
    }
    true
}

/// A reimport can assign a character file the same `asset_N` already used by a
/// scene still. Keep the first record and give each later duplicate a fresh id,
/// then point look slots and jobs at the still they actually named.
fn uniquify_duplicate_asset_ids(state: &mut ProjectState) -> bool {
    let mut seen = HashSet::new();
    let mut next = state
        .assets
        .iter()
        .filter_map(|asset| asset.id.strip_prefix("asset_")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let mut moves = Vec::new();
    for (index, asset) in state.assets.iter_mut().enumerate() {
        if seen.insert(asset.id.clone()) {
            continue;
        }
        let old = asset.id.clone();
        let new_id = format!("asset_{next}");
        next += 1;
        asset.id = new_id.clone();
        let moved_is_board = asset.role.is_scene_look()
            || asset.role.is_cut_image()
            || asset.role == AssetRole::WardrobePlate;
        moves.push((
            old,
            new_id,
            index,
            asset.role,
            asset.scene_id,
            asset.character_id.clone(),
            moved_is_board,
        ));
    }
    if moves.is_empty() {
        return false;
    }
    for (old, new_id, _index, role, scene_id, character_id, moved_is_board) in moves {
        let kept_character = state.assets.iter().any(|asset| {
            asset.id == old && asset.character_id.is_some() && asset.character_id == character_id
        });
        if moved_is_board {
            for scene in &mut state.scenes {
                let Some(board) = &mut scene.look_board else {
                    continue;
                };
                for slot in &mut board.slots {
                    if slot.role != role || scene_id != Some(scene.id) {
                        continue;
                    }
                    for id in &mut slot.asset_ids {
                        if *id == old {
                            *id = new_id.clone();
                        }
                    }
                    if slot.approved_asset_id.as_deref() == Some(old.as_str()) {
                        slot.approved_asset_id = Some(new_id.clone());
                    }
                }
                board.sync_asset_ids();
            }
            for job in &mut state.jobs {
                for id in job
                    .input_asset_ids
                    .iter_mut()
                    .chain(job.output_asset_ids.iter_mut())
                {
                    if *id == old {
                        *id = new_id.clone();
                    }
                }
            }
            for asset in &mut state.assets {
                if asset.parent_asset_id.as_deref() == Some(old.as_str())
                    && (asset.role.is_scene_look()
                        || asset.role.is_cut_image()
                        || asset.role == AssetRole::WardrobePlate)
                {
                    asset.parent_asset_id = Some(new_id.clone());
                }
            }
            for scene in &mut state.scenes {
                for cut in &mut scene.cuts {
                    if cut
                        .visual_source
                        .as_ref()
                        .and_then(|source| source.asset_id.as_deref())
                        == Some(old.as_str())
                    {
                        if let Some(source) = &mut cut.visual_source {
                            source.asset_id = Some(new_id.clone());
                        }
                    }
                }
            }
        } else if let Some(character_id) = character_id {
            if !kept_character {
                for character in &mut state.characters {
                    if character.id != character_id {
                        continue;
                    }
                    for id in &mut character.asset_ids {
                        if *id == old {
                            *id = new_id.clone();
                        }
                    }
                }
            }
        }
    }
    true
}

/// The still can be status=approved while the slot pointer is empty. The desk
/// and plate builder both follow the pointer, so restore it from the still.
fn restore_approved_look_pointers(state: &mut ProjectState) -> bool {
    let mut changed = false;
    for scene in &mut state.scenes {
        let Some(board) = &mut scene.look_board else {
            continue;
        };
        for slot in &mut board.slots {
            if slot.approved_asset_id.is_some() {
                continue;
            }
            let approved = slot.asset_ids.iter().rev().find(|id| {
                state.assets.iter().any(|asset| {
                    &asset.id == *id
                        && asset.role == slot.role
                        && asset.status == AssetStatus::Approved
                })
            });
            if let Some(id) = approved {
                slot.approved_asset_id = Some(id.clone());
                changed = true;
            }
        }
    }
    changed
}

fn restore_visual_sources(state: &mut ProjectState) -> bool {
    let approved: Vec<(CutId, String, String)> = state
        .assets
        .iter()
        .filter(|asset| {
            asset.role == AssetRole::CutStartFrame && asset.status == AssetStatus::Approved
        })
        .filter_map(|asset| Some((asset.cut_id?, asset.id.clone(), asset.path.clone())))
        .collect();
    let existing_ids: HashSet<String> = state.assets.iter().map(|asset| asset.id.clone()).collect();
    let mut changed = false;
    for scene in &mut state.scenes {
        for cut in &mut scene.cuts {
            if let Some(source) = &mut cut.visual_source {
                let invalid_start_frame_binding = source.strategy
                    == crate::workflow::state::VisualSourceStrategy::PreproductionStartFrame
                    && source.asset_id.as_ref().is_some_and(|id| {
                        !approved.iter().any(|(cut_id, asset_id, path)| {
                            *cut_id == cut.id
                                && asset_id == id
                                && cut
                                    .approved_start_frame
                                    .as_ref()
                                    .is_none_or(|approved_path| approved_path == path)
                        })
                    });
                if source
                    .asset_id
                    .as_ref()
                    .is_some_and(|id| !existing_ids.contains(id) || invalid_start_frame_binding)
                {
                    source.asset_id = None;
                    source.source_take_id = None;
                    source.frame_time_seconds = None;
                    changed = true;
                }
                if source.strategy
                    == crate::workflow::state::VisualSourceStrategy::PreproductionStartFrame
                    && source.asset_id.is_none()
                {
                    let approved_for_cut = approved
                        .iter()
                        .filter(|(cut_id, _, _)| *cut_id == cut.id)
                        .find(|(_, _, path)| {
                            cut.approved_start_frame.as_deref() == Some(path.as_str())
                        })
                        .or_else(|| approved.iter().find(|(cut_id, _, _)| *cut_id == cut.id));
                    if let Some((_, asset_id, path)) = approved_for_cut {
                        source.asset_id = Some(asset_id.clone());
                        cut.approved_start_frame = Some(path.clone());
                        changed = true;
                    }
                }
                continue;
            }
            let Some((_, asset_id, path)) =
                approved.iter().find(|(cut_id, _, _)| *cut_id == cut.id)
            else {
                continue;
            };
            let mut source = crate::workflow::state::VisualSourceSpec::new(
                crate::workflow::state::VisualSourceStrategy::PreproductionStartFrame,
            );
            source.asset_id = Some(asset_id.clone());
            cut.approved_start_frame = Some(path.clone());
            cut.visual_source = Some(source);
            changed = true;
        }
    }
    changed
}

fn restore_wardrobe_plate_assets(project_root: &Path, state: &mut ProjectState) -> bool {
    let jobs: Vec<(String, String, String, Option<String>)> = state
        .jobs
        .iter()
        .filter(|job| job.kind == "wardrobe_plate")
        .filter_map(|job| {
            Some((
                job.id.clone(),
                job.output_path.clone()?,
                job.raw_provider_id.clone()?,
                job.output_asset_ids.first().cloned(),
            ))
        })
        .collect();
    let mut changed = false;
    for (job_id, requested, raw, preferred_id) in jobs {
        let Some((character_id, shot)) = raw.split_once('|') else {
            continue;
        };
        let Some(actual) = existing_media_path(project_root, &requested) else {
            continue;
        };
        let actual_rel = rel(project_root, &actual);
        let scene_id = scene_id_from_preproduction_path(&actual_rel);
        let asset_id = if let Some(existing) = state
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::WardrobePlate && asset.path == actual_rel)
            .map(|asset| asset.id.clone())
        {
            existing
        } else {
            let id = preferred_id
                .filter(|id| state.asset(id).is_none())
                .unwrap_or_default();
            let id = state.push_asset(AssetRecord {
                id,
                path: actual_rel.clone(),
                role: AssetRole::WardrobePlate,
                status: AssetStatus::Approved,
                scene_id,
                cut_id: None,
                character_id: Some(character_id.to_string()),
                source_job_id: Some(job_id.clone()),
                parent_asset_id: None,
                notes: Some(shot.to_string()),
            });
            changed = true;
            id
        };
        if let Some(character) = state
            .characters
            .iter_mut()
            .find(|character| character.id == character_id)
        {
            if !character.asset_ids.contains(&asset_id) {
                character.asset_ids.push(asset_id.clone());
                changed = true;
            }
        }
        if let Some(job) = state.jobs.iter_mut().find(|job| job.id == job_id) {
            if job.status != crate::workflow::state::JobStatus::Completed {
                job.status = crate::workflow::state::JobStatus::Completed;
                changed = true;
            }
            if job.output_path.as_deref() != Some(actual_rel.as_str()) {
                job.output_path = Some(actual_rel.clone());
                changed = true;
            }
            if !job.output_asset_ids.contains(&asset_id) {
                job.output_asset_ids.push(asset_id);
                changed = true;
            }
        }
    }
    changed
}

fn restore_render_jobs(project_root: &Path, state: &mut ProjectState) -> bool {
    let jobs: Vec<(String, String)> = state
        .jobs
        .iter()
        .filter(|job| job.kind == "render")
        .filter_map(|job| Some((job.id.clone(), job.output_path.clone()?)))
        .collect();
    let mut changed = false;
    for (job_id, requested) in jobs {
        let Some(actual) = existing_media_path(project_root, &requested) else {
            continue;
        };
        let actual_rel = rel(project_root, &actual);
        let Some(asset_id) = state
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::VideoTake && asset.path == actual_rel)
            .map(|asset| asset.id.clone())
        else {
            continue;
        };
        if let Some(asset) = state.asset_mut(&asset_id) {
            if asset.source_job_id.as_deref() != Some(job_id.as_str()) {
                asset.source_job_id = Some(job_id.clone());
                changed = true;
            }
        }
        if let Some(job) = state.jobs.iter_mut().find(|job| job.id == job_id) {
            if job.status != crate::workflow::state::JobStatus::Completed {
                job.status = crate::workflow::state::JobStatus::Completed;
                changed = true;
            }
            if job.output_path.as_deref() != Some(actual_rel.as_str()) {
                job.output_path = Some(actual_rel);
                changed = true;
            }
            if !job.output_asset_ids.contains(&asset_id) {
                job.output_asset_ids.push(asset_id);
                changed = true;
            }
        }
    }
    changed
}

fn existing_media_path(root: &Path, requested: &str) -> Option<PathBuf> {
    let requested = {
        let path = PathBuf::from(requested);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    if requested.is_file() {
        return Some(requested);
    }
    let parent = requested.parent()?;
    let stem = requested.file_stem()?;
    fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_file() && path.file_stem() == Some(stem))
}

fn scene_id_from_preproduction_path(path: &str) -> Option<u32> {
    let normalized = path.replace('\\', "/");
    let rest = normalized.split("Preproduction/Scene").nth(1)?;
    let number: String = rest.chars().take_while(char::is_ascii_digit).collect();
    number.parse().ok()
}

pub(crate) fn sort_state(state: &mut ProjectState) {
    state.scenes.sort_by_key(|s| s.id);
    for scene in &mut state.scenes {
        scene.cuts.sort_by_key(|c| c.id.cut);
    }
    state.characters.sort_by(|a, b| a.name.cmp(&b.name));
}

/// Scan `project_root` into typed V2 state. Optionally write `Preproduction/workflow_state.json`.
pub fn import_project(project_root: &Path, options: ImportOptions) -> Result<ImportResult, String> {
    if !project_root.is_dir() {
        return Err(format!(
            "project root does not exist: {}",
            project_root.display()
        ));
    }
    let previous = load_previous(project_root);
    let mut state = ProjectState::new();
    state.version = WORKFLOW_VERSION;
    state.project_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    let plan = load_plan_state(project_root);
    let scripts = discover_scripts(project_root);
    let mut script_texts = Vec::new();
    for path in &scripts {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        script_texts.push(text.clone());
        let rel_path = rel(project_root, path);
        if state.scripts.iter().all(|s| s.path != rel_path) {
            let title = text.lines().find_map(|line| {
                let t = line.trim().trim_start_matches('#').trim();
                (!t.is_empty()).then(|| t.to_string())
            });
            state.scripts.push(ScriptRecord {
                path: rel_path.clone(),
                title,
                aspect_ratio: aspect_from_text(&text),
            });
        }
        if let Some(aspect) = aspect_from_text(&text) {
            if state.settings.aspect_ratio.is_none() {
                state.settings.aspect_ratio = Some(aspect);
            }
        }
        for parsed in extract_scenes(&text) {
            apply_script_scene(&mut state, &rel_path, parsed);
        }
    }

    import_characters(project_root, &mut state);
    if let Some(plan) = &plan {
        if state.settings.aspect_ratio.is_none() {
            state.settings.aspect_ratio = plan.aspect_ratio.clone();
        }
        import_plan_coverage(plan, &mut state);
    }
    let sidecar_root = discover_sidecar_root(project_root, plan.as_ref());
    import_sidecars(project_root, &sidecar_root, &mut state);
    if let Some(prod) = discover_prod_md(project_root, plan.as_ref()) {
        import_prod_md(project_root, &prod, &mut state);
    }
    import_takes(project_root, &mut state);
    let (old_profile, old_requirements) = previous
        .as_ref()
        .map(|prev| (prev.profile.clone(), prev.requirements.clone()))
        .unwrap_or_else(|| (None, Vec::new()));
    if let Some(recipe) = old_profile.as_ref().map(|profile| profile.recipe) {
        state
            .profile
            .get_or_insert_with(crate::workflow::state::ProductionProfile::default)
            .recipe = recipe;
    }
    if let Some(prev) = previous {
        merge_preserved(prev, &mut state);
    }
    restore_wardrobe_plate_assets(project_root, &mut state);
    restore_render_jobs(project_root, &mut state);
    prune_missing_scene_look_assets(project_root, &mut state);
    restore_visual_sources(&mut state);
    crate::workflow::breakdown::apply(&mut state, &script_texts);
    crate::workflow::breakdown::merge_preserved(&mut state, old_profile, old_requirements);
    crate::workflow::identity::assign(&mut state);
    crate::workflow::animatic::sync(&mut state);
    refresh_stage_flags(&mut state);
    sort_state(&mut state);

    if state.scenes.is_empty() {
        state.notes.push(
            "no scenes found; expected Scripts/ with `Scene N` headings and/or Scene_Prompts/"
                .into(),
        );
    }

    let persisted_path = if options.persist {
        let path = project_root.join(STATE_REL);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        write_atomic(&path, state.to_json()?.as_bytes())?;
        Some(path)
    } else {
        None
    };

    let report = project_report(&state);
    Ok(ImportResult {
        state,
        report,
        persisted_path,
    })
}

pub fn load_or_import(project_root: &Path) -> Result<ImportResult, String> {
    let path = project_root.join(STATE_REL);
    if path.is_file() {
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let mut state = ProjectState::from_json(&text)?;
        let mut repaired = uniquify_duplicate_asset_ids(&mut state);
        repaired |= restore_approved_look_pointers(&mut state);
        repaired |= restore_wardrobe_plate_assets(project_root, &mut state);
        repaired |= restore_render_jobs(project_root, &mut state);
        prune_missing_scene_look_assets(project_root, &mut state);
        repaired |= restore_visual_sources(&mut state);
        if state.profile.is_none() {
            let texts = state
                .scripts
                .iter()
                .filter_map(|script| fs::read_to_string(project_root.join(&script.path)).ok())
                .collect::<Vec<_>>();
            crate::workflow::breakdown::apply(&mut state, &texts);
            repaired = true;
        }
        repaired |= crate::workflow::identity::assign(&mut state);
        repaired |= crate::workflow::animatic::sync(&mut state);
        refresh_stage_flags(&mut state);
        if repaired {
            let _ = persist_state(project_root, &state);
        }
        let report = project_report(&state);
        return Ok(ImportResult {
            state,
            report,
            persisted_path: Some(path),
        });
    }
    import_project(project_root, ImportOptions { persist: false })
}

pub fn persist_state(project_root: &Path, state: &ProjectState) -> Result<PathBuf, String> {
    let path = project_root.join(STATE_REL);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    write_atomic(&path, state.to_json()?.as_bytes())?;
    Ok(path)
}

pub fn save_project(project_root: &Path, mut state: ProjectState) -> Result<ImportResult, String> {
    crate::workflow::identity::assign(&mut state);
    crate::workflow::animatic::sync(&mut state);
    refresh_stage_flags(&mut state);
    sort_state(&mut state);
    let persisted_path = persist_state(project_root, &state)?;
    let report = project_report(&state);
    Ok(ImportResult {
        state,
        report,
        persisted_path: Some(persisted_path),
    })
}

pub fn load_state(project_root: &Path) -> Result<ProjectState, String> {
    Ok(load_or_import(project_root)?.state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{
        AssetRecord, AssetRole, AssetStatus, CharacterRecord, JobRecord, JobStatus, LookBoard,
        LookSlot,
    };

    #[test]
    fn duplicate_look_id_stays_on_the_character_and_the_still_gets_a_new_id() {
        let mut state = ProjectState::new();
        state.assets.push(AssetRecord {
            id: "asset_74".into(),
            path: "Characters/Tom Vale/model_sheet/tom_expression_excited.png".into(),
            role: AssetRole::ExpressionReference,
            status: AssetStatus::Present,
            scene_id: None,
            cut_id: None,
            character_id: Some("tom_vale".into()),
            source_job_id: None,
            parent_asset_id: None,
            notes: Some("expression_excited".into()),
        });
        state.assets.push(AssetRecord {
            id: "asset_74".into(),
            path: "Preproduction/Scene1/references/exterior_2.jpg".into(),
            role: AssetRole::ExteriorReference,
            status: AssetStatus::Draft,
            scene_id: Some(1),
            cut_id: None,
            character_id: None,
            source_job_id: Some("job_4".into()),
            parent_asset_id: None,
            notes: None,
        });
        state.characters.push(CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "Characters/Tom Vale".into(),
            identity_ready: true,
            voice_ready: true,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: Default::default(),
            asset_ids: vec!["asset_74".into()],
            notes: Vec::new(),
        });
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        let mut slot = LookSlot::new("s1_look_02", AssetRole::ExteriorReference, "Exterior");
        slot.asset_ids.push("asset_74".into());
        slot.approved_asset_id = Some("asset_74".into());
        board.slots.push(slot);
        scene.look_board = Some(board);
        state.jobs.push(JobRecord {
            id: "job_4".into(),
            kind: "scene_look".into(),
            provider: None,
            model: None,
            input_asset_ids: vec!["asset_74".into()],
            output_asset_ids: vec!["asset_74".into()],
            status: JobStatus::Completed,
            output_path: Some("Preproduction/Scene1/references/exterior_2.jpg".into()),
            raw_provider_id: None,
            error: None,
            attempts: Vec::new(),
        });

        assert!(uniquify_duplicate_asset_ids(&mut state));
        assert_eq!(state.assets[0].id, "asset_74");
        assert_eq!(
            state.assets[0].path,
            "Characters/Tom Vale/model_sheet/tom_expression_excited.png"
        );
        assert_ne!(state.assets[1].id, "asset_74");
        assert_eq!(
            state.assets[1].path,
            "Preproduction/Scene1/references/exterior_2.jpg"
        );
        let still_id = state.assets[1].id.clone();
        assert_eq!(state.characters[0].asset_ids, vec!["asset_74".to_string()]);
        let slot = &state.scenes[0].look_board.as_ref().unwrap().slots[0];
        assert_eq!(slot.asset_ids, vec![still_id.clone()]);
        assert_eq!(slot.approved_asset_id.as_deref(), Some(still_id.as_str()));
        assert_eq!(state.jobs[0].output_asset_ids, vec![still_id.clone()]);
        assert_eq!(state.jobs[0].input_asset_ids, vec![still_id]);
        assert!(!uniquify_duplicate_asset_ids(&mut state));
    }

    #[test]
    fn approved_still_restores_a_missing_slot_pointer() {
        let mut state = ProjectState::new();
        state.assets.push(AssetRecord {
            id: "asset_86".into(),
            path: "Preproduction/Scene1/references/wardrobe_1.png".into(),
            role: AssetRole::WardrobeReference,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: None,
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        let mut slot = LookSlot::new("s1_look_03", AssetRole::WardrobeReference, "Wardrobe");
        slot.asset_ids.push("asset_86".into());
        board.slots.push(slot);
        scene.look_board = Some(board);

        assert!(restore_approved_look_pointers(&mut state));
        assert_eq!(
            state.scenes[0].look_board.as_ref().unwrap().slots[0]
                .approved_asset_id
                .as_deref(),
            Some("asset_86")
        );
        assert!(!restore_approved_look_pointers(&mut state));
    }

    #[test]
    fn completed_wardrobe_job_restores_a_missing_plate_record() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "Preproduction/Scene1/wardrobe_plates/pip_vale_closeup_front.png";
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"png").unwrap();
        let mut state = ProjectState::new();
        state.characters.push(CharacterRecord {
            id: "pip_vale".into(),
            name: "Pip Vale".into(),
            dir: "Characters/Pip Vale".into(),
            identity_ready: true,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: Default::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        });
        state.jobs.push(JobRecord {
            id: "job_24".into(),
            kind: "wardrobe_plate".into(),
            provider: None,
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: vec!["asset_93".into()],
            status: JobStatus::Completed,
            output_path: Some(rel.into()),
            raw_provider_id: Some("pip_vale|closeup_front".into()),
            error: None,
            attempts: Vec::new(),
        });

        assert!(restore_wardrobe_plate_assets(dir.path(), &mut state));
        let plate = state
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::WardrobePlate)
            .unwrap();
        assert_eq!(plate.id, "asset_93");
        assert_eq!(plate.character_id.as_deref(), Some("pip_vale"));
        assert_eq!(plate.scene_id, Some(1));
        assert_eq!(plate.notes.as_deref(), Some("closeup_front"));
        assert!(state.characters[0].asset_ids.contains(&"asset_93".into()));
        assert!(!restore_wardrobe_plate_assets(dir.path(), &mut state));
    }

    #[test]
    fn rendered_take_completes_its_pending_job() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "Scenes/Scene1/Cut2/S01C02.mp4";
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"video").unwrap();
        let mut state = ProjectState::new();
        state.assets.push(AssetRecord {
            id: "asset_7".into(),
            path: rel.into(),
            role: AssetRole::VideoTake,
            status: AssetStatus::Present,
            scene_id: Some(1),
            cut_id: Some(crate::scene_plan::handoff::CutId::new(1, 2)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        state.jobs.push(JobRecord {
            id: "job_3".into(),
            kind: "render".into(),
            provider: Some("atlascloud".into()),
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: Vec::new(),
            status: JobStatus::Pending,
            output_path: Some(rel.into()),
            raw_provider_id: Some("S01C02".into()),
            error: None,
            attempts: Vec::new(),
        });

        assert!(restore_render_jobs(dir.path(), &mut state));
        assert_eq!(state.jobs[0].status, JobStatus::Completed);
        assert_eq!(state.jobs[0].output_asset_ids, vec!["asset_7"]);
        assert_eq!(state.assets[0].source_job_id.as_deref(), Some("job_3"));
        assert!(!restore_render_jobs(dir.path(), &mut state));
    }

    #[test]
    fn stale_start_frame_asset_id_rebinds_to_the_approved_path() {
        let mut state = ProjectState::new();
        state.assets.push(AssetRecord {
            id: "asset_78".into(),
            path: "Preproduction/Scene1/start_frames/cut_1_v4.png".into(),
            role: AssetRole::CutStartFrame,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: Some(crate::scene_plan::handoff::CutId::new(1, 1)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        state.assets.push(AssetRecord {
            id: "asset_82".into(),
            path: "Preproduction/Scene1/start_frames/cut_2_v2.png".into(),
            role: AssetRole::CutStartFrame,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: Some(crate::scene_plan::handoff::CutId::new(1, 2)),
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        let cut = state.scene_mut(1, "Scene 1").cut_mut(1);
        cut.approved_start_frame = Some("Preproduction/Scene1/start_frames/cut_1_v4.png".into());
        cut.visual_source = Some(crate::workflow::state::VisualSourceSpec {
            strategy: crate::workflow::state::VisualSourceStrategy::PreproductionStartFrame,
            predecessor: None,
            asset_id: Some("asset_82".into()),
            source_take_id: None,
            frame_time_seconds: None,
            predecessor_shot_id: None,
        });

        assert!(restore_visual_sources(&mut state));
        assert_eq!(
            state.scenes[0].cuts[0]
                .visual_source
                .as_ref()
                .and_then(|source| source.asset_id.as_deref()),
            Some("asset_78")
        );
    }
}

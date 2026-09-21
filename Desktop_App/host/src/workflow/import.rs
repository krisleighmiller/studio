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
    ProjectState, ScriptRecord, StageStatus, STATE_REL, WORKFLOW_VERSION,
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
            push(
                state,
                v.profile_path.clone(),
                AssetRole::VoiceProfile,
                Some(format!("{} {}", v.provider, v.voice_id)),
            );
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
                state.push_asset(AssetRecord {
                    id: String::new(),
                    path: rel(root, &file),
                    role,
                    status: AssetStatus::Present,
                    scene_id: Some(scene_id),
                    cut_id,
                    character_id: None,
                    source_job_id: None,
                    parent_asset_id: None,
                    notes: None,
                });
            }
        }
    }
}

pub(crate) fn refresh_stage_flags(state: &mut ProjectState) {
    let any_characters = !state.characters.is_empty();
    for scene in &mut state.scenes {
        if scene.source_excerpt.is_some() || !scene.dialogue.is_empty() {
            scene.stages.script = StageStatus::Present;
        }
        scene.stages.characters = if any_characters {
            StageStatus::Present
        } else {
            StageStatus::Missing
        };
        scene.stages.scene_intent = if scene.intent.as_ref().is_some_and(|i| !i.is_empty()) {
            if scene.intent_approved {
                StageStatus::Approved
            } else {
                StageStatus::Draft
            }
        } else {
            StageStatus::Missing
        };
        scene.stages.scene_look = {
            let board = scene.look_board.as_ref();
            if board.map(|b| b.is_empty()).unwrap_or(true) {
                StageStatus::Missing
            } else if board.is_some_and(|b| b.approved_count() > 0) {
                StageStatus::Approved
            } else {
                StageStatus::Draft
            }
        };
        let sketches = scene
            .cuts
            .iter()
            .filter(|c| c.sketch.as_ref().is_some_and(|s| !s.is_empty()))
            .count();
        scene.stages.cut_sketches = if sketches == 0 {
            StageStatus::Missing
        } else if sketches == scene.cuts.len() {
            StageStatus::Draft
        } else {
            StageStatus::Draft
        };
        let start_frames = scene
            .cuts
            .iter()
            .filter(|c| c.approved_start_frame.is_some())
            .count();
        scene.stages.reference_images = if start_frames == 0 {
            StageStatus::Missing
        } else {
            StageStatus::Present
        };
        scene.stages.production_notes = if scene.cuts.iter().any(|c| c.has_production_note()) {
            StageStatus::Present
        } else {
            StageStatus::Missing
        };
        scene.stages.render = if scene.cuts.iter().any(|c| !c.takes.is_empty()) {
            StageStatus::Present
        } else {
            StageStatus::Missing
        };
        scene.stages.take_review = if scene.cuts.iter().any(|c| c.selected_take.is_some()) {
            StageStatus::Present
        } else {
            StageStatus::Missing
        };
    }
}

fn merge_preserved(previous: ProjectState, state: &mut ProjectState) {
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
            if cut.takes.is_empty() {
                cut.takes = old_cut.takes;
            }
            if cut.selected_take.is_none() {
                cut.selected_take = old_cut.selected_take;
            }
        }
    }
    if state.jobs.is_empty() {
        state.jobs = previous.jobs;
    }
    if previous
        .assets
        .iter()
        .any(|asset| asset.role.is_scene_look() || asset.role.is_cut_image())
    {
        for old in previous.assets {
            if (old.role.is_scene_look() || old.role.is_cut_image())
                && !state
                    .assets
                    .iter()
                    .any(|a| a.path == old.path && a.role == old.role)
            {
                state.assets.push(old);
            }
        }
    }
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
        }
    }
    for job in &mut state.jobs {
        job.input_asset_ids.retain(|id| !missing.contains(id));
        job.output_asset_ids.retain(|id| !missing.contains(id));
    }
    true
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
    for path in &scripts {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
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
    if let Some(prev) = previous {
        merge_preserved(prev, &mut state);
    }
    prune_missing_scene_look_assets(project_root, &mut state);
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
        prune_missing_scene_look_assets(project_root, &mut state);
        refresh_stage_flags(&mut state);
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

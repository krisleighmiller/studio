//! Scene look board: reference prompts, attach, generate, approve.
//!
//! Host owns IDs, paths, versions, jobs, and registry. The model only drafts
//! prompts and suggested roles. Character model-sheet closeups are identity
//! inputs for people stills, never look outputs.
use crate::director_actions::{
    format_approval_request, resolve_under_project, validate_action, ActionKind, DirectorAction,
};
use crate::scene_plan::assets::name_tokens;
use crate::workflow::import;
use crate::workflow::state::{
    AssetRecord, AssetRole, AssetStatus, CharacterRecord, JobRecord, LookBoard, LookSlot,
    ProjectState, SceneIntent, SceneRecord,
};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_LOOK_IDENTITY_CHARS: usize = 3;
const MAX_LOOK_IDENTITY_REFS: usize = 6;
const LOOK_IDENTITY_INSTRUCTION: &str = "Use the attached character model-sheet images for identity and body proportions. Do not copy their model-sheet clothing when an approved scene wardrobe reference is attached. This is a scene look still, not a model-sheet closeup.";
const LOOK_WARDROBE_INSTRUCTION: &str = "Match the named characters' clothing to the attached approved scene wardrobe reference. Keep its garment design, colors, layers, and condition consistent.";

fn approval_details(
    app: Option<&crate::ctx::HostCtx>,
    action: &DirectorAction,
    validated: &crate::director_actions::ValidatedAction,
    root: &str,
) -> Result<String, String> {
    match app {
        Some(app) => {
            crate::director::format_approval_request_for_app(app, action, validated, Some(root))
        }
        None => Ok(format_approval_request(action, validated)),
    }
}

pub const CORE_LOOK_ROLES: [AssetRole; 5] = [
    AssetRole::InteriorReference,
    AssetRole::ExteriorReference,
    AssetRole::WardrobeReference,
    AssetRole::PropReference,
    AssetRole::MoodReference,
];

pub fn look_system_prompt() -> &'static str {
    "You draft a scene look / reference board for still-image generation. Return ONLY JSON:\n\
{\"notes\":[\"optional board note\"],\"slots\":[{\"role\":\"interior_reference\",\"title\":\"...\",\"prompt\":\"still-image prompt\",\"negative_prompt\":\"optional\"}]}\n\
Roles (use these exact strings): interior_reference, exterior_reference, wardrobe_reference, prop_reference, mood_reference, location_reference, lighting_reference, composition_reference, group_reference.\n\
Always include the five core roles: interior, exterior, wardrobe, prop, mood. Add others only if they earn a still.\n\
Prompts describe the scene world (place, clothes, props, light, mood).\n\
When people belong in a still (interior, wardrobe, group), name them exactly as in the character list (e.g. Tom Vale, Pip Vale). The host will attach their model-sheet identity and body images at generate time. Do not invent faces. Do not output model-sheet closeups; the still is a scene reference.\n\
Do not emit file paths, asset ids, models, or production notes. The host owns those."
}

pub fn look_user_prompt(
    scene: &SceneRecord,
    characters: &[(String, String)],
    brief: &str,
) -> String {
    let mut s = format!("Scene {}: {}\n", scene.id, scene.title);
    if let Some(loc) = &scene.location {
        s.push_str(&format!("Location: {loc}\n"));
    }
    if let Some(intent) = &scene.intent {
        s.push_str("\nScene intent:\n");
        push_intent(&mut s, intent);
    }
    if !characters.is_empty() {
        s.push_str("\nCharacters (model sheets are identity inputs; host attaches them when generating people stills):\n");
        for (name, notes) in characters {
            if notes.is_empty() {
                s.push_str(&format!("- {name}\n"));
            } else {
                s.push_str(&format!("- {name}: {notes}\n"));
            }
        }
    }
    s.push_str("\nScript / beats:\n");
    s.push_str(scene.source_excerpt.as_deref().unwrap_or("(no excerpt)"));
    s.push('\n');
    if let Some(board) = &scene.look_board {
        if !board.slots.is_empty() {
            s.push_str("\nExisting look slots (revise prompts; keep useful roles):\n");
            for slot in &board.slots {
                s.push_str(&format!(
                    "- {} ({}) approved={} prompt={}\n",
                    slot.title,
                    slot.role.label(),
                    slot.approved_asset_id.is_some(),
                    slot.prompt.as_deref().unwrap_or("(none)")
                ));
            }
        }
    }
    if !brief.trim().is_empty() {
        s.push_str("\nUser direction:\n");
        s.push_str(brief.trim());
        s.push('\n');
    }
    s
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookIdentityRef {
    pub character_id: String,
    pub character_name: String,
    pub asset_id: String,
    pub path: String,
    pub role: AssetRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookContinuityRef {
    pub asset_id: String,
    pub path: String,
    pub role: AssetRole,
    pub title: String,
}

pub fn collect_look_identity_refs(
    state: &ProjectState,
    scene_id: u32,
    role: AssetRole,
    prompt: &str,
    title: &str,
) -> Vec<LookIdentityRef> {
    let mut refs = Vec::new();
    for character in mentioned_look_characters(state, scene_id, role, prompt, title) {
        if refs.len() >= MAX_LOOK_IDENTITY_REFS {
            break;
        }
        if let Some(asset) = pick_wardrobe_plate(state, scene_id, &character.id, "closeup_front")
            .or_else(|| pick_identity_asset(state, &character.id))
        {
            push_unique_ref(&mut refs, character, asset);
        }
        if refs.len() >= MAX_LOOK_IDENTITY_REFS {
            break;
        }
        if let Some(asset) = pick_wardrobe_plate(state, scene_id, &character.id, "standing_front")
            .or_else(|| pick_body_asset(state, &character.id))
        {
            push_unique_ref(&mut refs, character, asset);
        }
    }
    refs
}

pub fn keep_existing_identity_refs(
    root: &Path,
    refs: Vec<LookIdentityRef>,
) -> Vec<LookIdentityRef> {
    refs.into_iter()
        .filter(|r| identity_file_exists(root, &r.path))
        .collect()
}

pub fn collect_approved_look_continuity_refs(
    root: &Path,
    state: &ProjectState,
    scene_id: u32,
    include_people_continuity: bool,
) -> Vec<LookContinuityRef> {
    if !include_people_continuity {
        return Vec::new();
    }
    let Some(board) = state
        .scene(scene_id)
        .and_then(|scene| scene.look_board.as_ref())
    else {
        return Vec::new();
    };
    board
        .slots
        .iter()
        .filter(|slot| slot.role == AssetRole::WardrobeReference)
        .filter_map(|slot| {
            let asset_id = slot.approved_asset_id.clone().or_else(|| {
                slot.asset_ids
                    .iter()
                    .rev()
                    .find(|id| {
                        state.asset(id).is_some_and(|asset| {
                            asset.status == AssetStatus::Approved
                                && asset.role == AssetRole::WardrobeReference
                        })
                    })
                    .cloned()
            })?;
            let asset = state.asset(&asset_id)?;
            if asset.status != AssetStatus::Approved
                || asset.role != AssetRole::WardrobeReference
                || !identity_file_exists(root, &asset.path)
            {
                return None;
            }
            Some(LookContinuityRef {
                asset_id: asset.id.clone(),
                path: asset.path.clone(),
                role: asset.role,
                title: slot.title.clone(),
            })
        })
        .collect()
}

fn mentioned_look_characters<'a>(
    state: &'a ProjectState,
    scene_id: u32,
    role: AssetRole,
    prompt: &str,
    title: &str,
) -> Vec<&'a CharacterRecord> {
    let hay = look_identity_haystack(state, scene_id, role, prompt, title).to_ascii_lowercase();
    let mut ranked: Vec<(usize, &CharacterRecord)> = state
        .characters
        .iter()
        .filter_map(|c| mention_index(&hay, &c.name, &state.characters).map(|i| (i, c)))
        .collect();
    ranked.sort_by_key(|(i, _)| *i);
    ranked
        .into_iter()
        .map(|(_, c)| c)
        .take(MAX_LOOK_IDENTITY_CHARS)
        .collect()
}

fn look_identity_haystack(
    state: &ProjectState,
    scene_id: u32,
    role: AssetRole,
    prompt: &str,
    title: &str,
) -> String {
    let mut hay = format!("{title}\n{prompt}");
    if !role.look_usually_shows_people() {
        return hay;
    }
    let Some(scene) = state.scene(scene_id) else {
        return hay;
    };
    hay.push('\n');
    hay.push_str(&scene.title);
    if let Some(intent) = &scene.intent {
        hay.push('\n');
        hay.push_str(&intent.character_focus.join("\n"));
    }
    if let Some(excerpt) = &scene.source_excerpt {
        hay.push('\n');
        hay.push_str(excerpt);
    }
    for line in &scene.dialogue {
        hay.push('\n');
        hay.push_str(&line.speaker);
    }
    for cut in &scene.cuts {
        for line in &cut.dialogue {
            hay.push('\n');
            hay.push_str(&line.speaker);
        }
        if let Some(sketch) = &cut.sketch {
            if let Some(speaker) = &sketch.speaker {
                hay.push('\n');
                hay.push_str(speaker);
            }
            for line in &sketch.dialogue {
                hay.push('\n');
                hay.push_str(&line.speaker);
            }
        }
    }
    hay
}

fn mention_index(hay_lower: &str, name: &str, all: &[CharacterRecord]) -> Option<usize> {
    let tokens = name_tokens(name);
    if tokens.is_empty() {
        return None;
    }
    let full = tokens.join(" ");
    if let Some(i) = hay_lower.find(&full) {
        return Some(i);
    }
    let first = tokens.first()?;
    if first.len() < 3 {
        return None;
    }
    let unique = all
        .iter()
        .filter(|c| name_tokens(&c.name).first() == Some(first))
        .count()
        == 1;
    if unique && whole_word(hay_lower, first) {
        return hay_lower.find(first.as_str());
    }
    None
}

fn whole_word(hay_lower: &str, word: &str) -> bool {
    hay_lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == word)
}

fn identity_clothing_instruction(refs: &[LookIdentityRef]) -> String {
    let plates = refs
        .iter()
        .any(|reference| reference.role == AssetRole::WardrobePlate);
    let sheets = refs
        .iter()
        .any(|reference| reference.role != AssetRole::WardrobePlate);
    if plates && sheets {
        "Use each attached wardrobe plate for that character's identity and clothing. Keep the plate's garment design, colors, layers, and condition. Where only a model sheet is attached, use it for identity and body proportions, but do not copy its clothes; match the approved scene wardrobe instead.".into()
    } else if plates {
        "Use the attached wardrobe plates for identity and clothing. Keep each plate's face, proportions, garment design, colors, layers, and condition. Do not revert to plain or modern clothes.".into()
    } else {
        LOOK_IDENTITY_INSTRUCTION.into()
    }
}

fn pick_wardrobe_plate<'a>(
    state: &'a ProjectState,
    scene_id: u32,
    character_id: &str,
    shot: &str,
) -> Option<&'a AssetRecord> {
    state.assets.iter().rev().find(|asset| {
        asset.character_id.as_deref() == Some(character_id)
            && asset.scene_id == Some(scene_id)
            && asset.role == AssetRole::WardrobePlate
            && asset.notes.as_deref() == Some(shot)
            && asset.status != AssetStatus::Rejected
    })
}

fn pick_identity_asset<'a>(state: &'a ProjectState, character_id: &str) -> Option<&'a AssetRecord> {
    let owned: Vec<&AssetRecord> = state
        .assets
        .iter()
        .filter(|a| a.character_id.as_deref() == Some(character_id))
        .collect();
    owned
        .iter()
        .copied()
        .find(|a| a.role == AssetRole::IdentityReference)
        .or_else(|| {
            owned.iter().copied().find(|a| {
                a.notes
                    .as_deref()
                    .is_some_and(|n| n.contains("closeup") || n.contains("approval"))
            })
        })
}

fn pick_body_asset<'a>(state: &'a ProjectState, character_id: &str) -> Option<&'a AssetRecord> {
    let owned: Vec<&AssetRecord> = state
        .assets
        .iter()
        .filter(|a| {
            a.character_id.as_deref() == Some(character_id) && a.role == AssetRole::BodyReference
        })
        .collect();
    owned
        .iter()
        .copied()
        .find(|a| a.notes.as_deref() == Some("standing_front"))
        .or_else(|| {
            owned.iter().copied().find(|a| {
                a.notes
                    .as_deref()
                    .is_some_and(|n| n.starts_with("standing"))
            })
        })
        .or_else(|| owned.first().copied())
}

fn push_unique_ref(
    refs: &mut Vec<LookIdentityRef>,
    character: &CharacterRecord,
    asset: &AssetRecord,
) {
    if refs
        .iter()
        .any(|r| r.path == asset.path || r.asset_id == asset.id)
    {
        return;
    }
    refs.push(LookIdentityRef {
        character_id: character.id.clone(),
        character_name: character.name.clone(),
        asset_id: asset.id.clone(),
        path: asset.path.clone(),
        role: asset.role,
    });
}

fn identity_file_exists(root: &Path, rel: &str) -> bool {
    let raw = Path::new(rel);
    if raw.is_absolute() {
        raw.is_file()
    } else {
        root.join(raw).is_file()
    }
}

fn push_intent(s: &mut String, intent: &SceneIntent) {
    if let Some(v) = &intent.narrative_purpose {
        s.push_str(&format!("- narrative: {v}\n"));
    }
    if let Some(v) = &intent.emotional_purpose {
        s.push_str(&format!("- emotional: {v}\n"));
    }
    if !intent.character_focus.is_empty() {
        s.push_str(&format!("- focus: {}\n", intent.character_focus.join(", ")));
    }
    if let Some(v) = &intent.visual_contrast {
        s.push_str(&format!("- visual contrast: {v}\n"));
    }
    if !intent.important_props.is_empty() {
        s.push_str(&format!("- props: {}\n", intent.important_props.join(", ")));
    }
    if let Some(v) = &intent.audience_must_know {
        s.push_str(&format!("- audience must know: {v}\n"));
    }
}

pub fn apply_llm_look(scene: &mut SceneRecord, value: &Value) -> Result<(), String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "look JSON must be an object with slots".to_string())?;
    let mut board = scene.look_board.take().unwrap_or_default();
    if let Some(notes) = obj.get("notes").or_else(|| obj.get("board_notes")) {
        let parsed = string_list(notes);
        if !parsed.is_empty() {
            board.notes = parsed;
        }
    }
    let slots = obj
        .get("slots")
        .or_else(|| obj.get("references"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "look JSON needs a slots array".to_string())?;
    let mut incoming: Vec<(AssetRole, String, Option<String>, Option<String>)> = Vec::new();
    for item in slots {
        let Some(map) = item.as_object() else {
            continue;
        };
        let _ = map.get("path").or_else(|| map.get("output_path"));
        let role = map
            .get("role")
            .and_then(|v| v.as_str())
            .and_then(AssetRole::parse_look)
            .ok_or_else(|| "look slot is missing a scene-look role".to_string())?;
        let title = map
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| role.display_label())
            .to_string();
        let prompt = nonempty_field(map.get("prompt").or_else(|| map.get("image_prompt")));
        let negative = nonempty_field(map.get("negative_prompt").or_else(|| map.get("negative")));
        incoming.push((role, title, prompt, negative));
    }
    if incoming.is_empty() {
        scene.look_board = Some(board);
        return Err("look JSON had no usable slots".into());
    }
    for (role, title, prompt, negative) in incoming {
        upsert_slot(&mut board, scene.id, role, title, prompt, negative);
    }
    ensure_core_slots(&mut board, scene.id);
    board.sync_asset_ids();
    scene.look_board = Some(board);
    Ok(())
}

fn upsert_slot(
    board: &mut LookBoard,
    scene_id: u32,
    role: AssetRole,
    title: String,
    prompt: Option<String>,
    negative: Option<String>,
) {
    if let Some(existing) = board.slots.iter_mut().find(|s| s.role == role) {
        if !title.is_empty() {
            existing.title = title;
        }
        if prompt.is_some() {
            existing.prompt = prompt;
        }
        if negative.is_some() {
            existing.negative_prompt = negative;
        }
        return;
    }
    let mut slot = LookSlot::new(next_slot_id(board, scene_id), role, title);
    slot.prompt = prompt;
    slot.negative_prompt = negative;
    board.slots.push(slot);
}

fn ensure_core_slots(board: &mut LookBoard, scene_id: u32) {
    for role in CORE_LOOK_ROLES {
        if board.slots.iter().any(|s| s.role == role) {
            continue;
        }
        board.slots.push(LookSlot::new(
            next_slot_id(board, scene_id),
            role,
            role.display_label(),
        ));
    }
}

fn next_slot_id(board: &LookBoard, scene_id: u32) -> String {
    format!("s{}_look_{:02}", scene_id, board.slots.len() + 1)
}

fn nonempty_field(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn string_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        Value::String(s) => s
            .split(|c| c == '\n' || c == ';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn look_rel(scene_id: u32, role: AssetRole, version: u32, ext: &str) -> String {
    let ext = ext.trim_start_matches('.');
    format!(
        "Preproduction/Scene{scene_id}/references/{}_{version}.{ext}",
        role.file_stem()
    )
}

pub fn next_look_rel(root: &Path, scene: &SceneRecord, role: AssetRole, ext: &str) -> String {
    next_available_look_rel(root, scene.id, role, ext, |_| false)
}

fn next_look_rel_for_state(
    root: &Path,
    state: &ProjectState,
    scene_id: u32,
    role: AssetRole,
    ext: &str,
) -> String {
    next_available_look_rel(root, scene_id, role, ext, |rel| {
        state.assets.iter().any(|a| same_path_stem(&a.path, rel))
            || state.jobs.iter().any(|j| {
                j.output_path
                    .as_deref()
                    .is_some_and(|path| same_path_stem(path, rel))
            })
    })
}

fn next_available_look_rel(
    root: &Path,
    scene_id: u32,
    role: AssetRole,
    ext: &str,
    reserved: impl Fn(&str) -> bool,
) -> String {
    let mut version = 1u32;
    loop {
        let rel = look_rel(scene_id, role, version, ext);
        if !look_stem_exists(root, &rel) && !reserved(&rel) {
            return rel;
        }
        version += 1;
        if version > 99 {
            return rel;
        }
    }
}

fn look_stem_exists(root: &Path, rel: &str) -> bool {
    let requested = root.join(rel);
    let Some(parent) = requested.parent() else {
        return false;
    };
    let Some(stem) = requested.file_stem() else {
        return requested.is_file();
    };
    fs::read_dir(parent).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.path().is_file()
                && entry.path().file_stem() == Some(stem)
                && is_image_path(&entry.path())
        })
    })
}

fn same_path_stem(a: &str, b: &str) -> bool {
    let normalize = |value: &str| {
        Path::new(value)
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    };
    normalize(a) == normalize(b)
}

fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif"
            )
        })
}

pub fn patch_slot(
    scene: &mut SceneRecord,
    slot_id: &str,
    role: Option<AssetRole>,
    title: Option<String>,
    prompt: Option<String>,
    negative: Option<String>,
) -> Result<(), String> {
    let board = scene.look_board.get_or_insert_with(LookBoard::default);
    let slot = board
        .slot_mut(slot_id)
        .ok_or_else(|| format!("look slot `{slot_id}` not found"))?;
    if let Some(role) = role {
        if !role.is_scene_look() {
            return Err("that role is not a scene look reference".into());
        }
        slot.role = role;
    }
    if let Some(title) = title {
        slot.title = title.trim().to_string();
    }
    if let Some(prompt) = prompt {
        slot.prompt = nonempty_opt(&prompt);
    }
    if let Some(negative) = negative {
        slot.negative_prompt = nonempty_opt(&negative);
    }
    Ok(())
}

fn nonempty_opt(value: &str) -> Option<String> {
    let t = value.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

pub fn attach_image(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    slot_id: &str,
    source: &str,
) -> Result<String, String> {
    let abs = resolve_source(root, source)?;
    if !abs.is_file() {
        return Err(format!("image not found: {}", abs.display()));
    }
    let ext = abs
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
        return Err("attach a png, jpg, webp, or gif".into());
    }
    let scene_title = state
        .scene(scene_id)
        .map(|s| s.title.clone())
        .unwrap_or_else(|| format!("Scene {scene_id}"));
    let role = {
        let scene = state
            .scenes
            .iter()
            .find(|s| s.id == scene_id)
            .ok_or_else(|| format!("scene {scene_id} not found"))?;
        let slot = scene
            .look_board
            .as_ref()
            .and_then(|b| b.slots.iter().find(|s| s.id == slot_id))
            .ok_or_else(|| format!("look slot `{slot_id}` not found"))?;
        slot.role
    };
    let rel = next_look_rel_for_state(root, state, scene_id, role, &ext);
    let dest = root.join(&rel);
    if dest != abs {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("could not create look folder: {e}"))?;
        }
        fs::copy(&abs, &dest).map_err(|e| format!("could not copy image: {e}"))?;
    }
    let parent_id = state
        .scene(scene_id)
        .and_then(|s| s.look_board.as_ref())
        .and_then(|b| b.slots.iter().find(|s| s.id == slot_id))
        .and_then(|s| {
            s.approved_asset_id
                .clone()
                .or_else(|| s.asset_ids.last().cloned())
        });
    let asset_id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel.replace('\\', "/"),
        role,
        status: AssetStatus::Draft,
        scene_id: Some(scene_id),
        cut_id: None,
        character_id: None,
        source_job_id: None,
        parent_asset_id: parent_id,
        notes: Some(format!("attached look for {scene_title}")),
    });
    let scene = state
        .scenes
        .iter_mut()
        .find(|s| s.id == scene_id)
        .expect("scene exists");
    let board = scene.look_board.get_or_insert_with(LookBoard::default);
    let slot = board
        .slot_mut(slot_id)
        .ok_or_else(|| format!("look slot `{slot_id}` not found"))?;
    slot.asset_ids.push(asset_id.clone());
    board.sync_asset_ids();
    Ok(asset_id)
}

pub fn set_asset_status(
    state: &mut ProjectState,
    scene_id: u32,
    slot_id: &str,
    asset_id: Option<&str>,
    status: AssetStatus,
) -> Result<String, String> {
    let target = {
        let scene = state
            .scenes
            .iter_mut()
            .find(|s| s.id == scene_id)
            .ok_or_else(|| format!("scene {scene_id} not found"))?;
        let board = scene
            .look_board
            .as_mut()
            .ok_or_else(|| "this scene has no look board".to_string())?;
        let slot = board
            .slot_mut(slot_id)
            .ok_or_else(|| format!("look slot `{slot_id}` not found"))?;
        let target = asset_id
            .map(|s| s.to_string())
            .or_else(|| slot.asset_ids.last().cloned())
            .ok_or_else(|| "this look slot has no image to approve or reject".to_string())?;
        if !slot.asset_ids.iter().any(|id| id == &target) {
            return Err(format!("asset `{target}` is not on look slot `{slot_id}`"));
        }
        let previous_approved = if status == AssetStatus::Approved {
            slot.approved_asset_id
                .clone()
                .filter(|current| current != &target)
        } else {
            None
        };
        match status {
            AssetStatus::Approved => slot.approved_asset_id = Some(target.clone()),
            AssetStatus::Rejected => {
                if slot.approved_asset_id.as_deref() == Some(target.as_str()) {
                    slot.approved_asset_id = None;
                }
            }
            _ => {}
        }
        (target, previous_approved)
    };
    let (target, previous_approved) = target;
    if let Some(previous) = &previous_approved {
        if let Some(asset) = state.asset_mut(previous) {
            asset.status = AssetStatus::Superseded;
        }
        crate::workflow::identity::record_version(state, previous, AssetStatus::Superseded);
        crate::workflow::identity::mark_asset_dependents(state, scene_id, previous);
    }
    if status == AssetStatus::Approved {
        crate::workflow::identity::record_version(state, &target, AssetStatus::Approved);
    }
    if let Some(asset) = state.asset_mut(&target) {
        asset.status = status;
    }
    Ok(target)
}

pub fn delete_image(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    slot_id: &str,
    asset_id: &str,
) -> Result<String, String> {
    let slot = state
        .scene(scene_id)
        .and_then(|scene| scene.look_board.as_ref())
        .and_then(|board| board.slots.iter().find(|slot| slot.id == slot_id))
        .ok_or_else(|| format!("look slot `{slot_id}` not found"))?;
    if !slot.asset_ids.iter().any(|id| id == asset_id) {
        return Err(format!(
            "asset `{asset_id}` is not on look slot `{slot_id}`"
        ));
    }
    let asset = state
        .assets
        .iter()
        .find(|asset| {
            asset.id == asset_id && asset.role.is_scene_look() && asset.scene_id == Some(scene_id)
        })
        .ok_or_else(|| format!("look asset `{asset_id}` not found"))?;
    let rel = asset.path.clone();
    let root_s = root.to_string_lossy().to_string();
    let path = resolve_under_project(&rel, Some(&root_s)).map_err(|error| error.to_string())?;
    crate::project_path::under_scene_bucket(root, &path, "references")?;
    if path.is_file() {
        crate::project_path::move_to_trash(root, &path)?;
    }

    state
        .assets
        .retain(|asset| !(asset.id == asset_id && asset.path == rel));
    for asset in &mut state.assets {
        if asset.parent_asset_id.as_deref() == Some(asset_id) {
            asset.parent_asset_id = None;
        }
    }
    for scene in &mut state.scenes {
        if let Some(board) = &mut scene.look_board {
            for slot in &mut board.slots {
                slot.asset_ids.retain(|id| id != asset_id);
                if slot.approved_asset_id.as_deref() == Some(asset_id) {
                    slot.approved_asset_id = None;
                }
            }
            board.sync_asset_ids();
        }
    }
    for job in &mut state.jobs {
        job.input_asset_ids.retain(|id| id != asset_id);
        job.output_asset_ids.retain(|id| id != asset_id);
    }
    Ok(rel)
}

pub fn prepare_generate(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    slot_id: &str,
) -> Result<(String, String), String> {
    prepare_generate_inner(root, state, scene_id, slot_id, None)
}

pub fn prepare_generate_with_host(
    app: &crate::ctx::HostCtx,
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    slot_id: &str,
) -> Result<(String, String), String> {
    prepare_generate_inner(root, state, scene_id, slot_id, Some(app))
}

fn prepare_generate_inner(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    slot_id: &str,
    approval_app: Option<&crate::ctx::HostCtx>,
) -> Result<(String, String), String> {
    let aspect = state.settings.aspect_ratio.clone();
    let (prompt, title, role_label, role) = {
        let scene = state
            .scenes
            .iter()
            .find(|s| s.id == scene_id)
            .ok_or_else(|| format!("scene {scene_id} not found"))?;
        let slot = scene
            .look_board
            .as_ref()
            .and_then(|b| b.slots.iter().find(|s| s.id == slot_id))
            .ok_or_else(|| format!("look slot `{slot_id}` not found"))?;
        let prompt = slot
            .prompt
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "draft a prompt for this look slot before generating".to_string())?
            .to_string();
        (
            prompt,
            slot.title.clone(),
            slot.role.display_label().to_string(),
            slot.role,
        )
    };
    let named = mentioned_look_characters(state, scene_id, role, &prompt, &title);
    let identity_refs = keep_existing_identity_refs(
        root,
        collect_look_identity_refs(state, scene_id, role, &prompt, &title),
    );
    let continuity_refs = collect_approved_look_continuity_refs(
        root,
        state,
        scene_id,
        role.look_usually_shows_people() || !identity_refs.is_empty(),
    );
    let missing: Vec<String> = named
        .iter()
        .filter(|c| !identity_refs.iter().any(|r| r.character_id == c.id))
        .map(|c| c.name.clone())
        .collect();
    let rel = next_look_rel_for_state(root, state, scene_id, role, "png");
    let dest = root.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("could not create look folder: {e}"))?;
    }
    let mut gen_prompt = prompt;
    if !identity_refs.is_empty() {
        gen_prompt.push_str("\n\n");
        gen_prompt.push_str(&identity_clothing_instruction(&identity_refs));
    }
    if !continuity_refs.is_empty() {
        gen_prompt.push_str("\n\n");
        gen_prompt.push_str(LOOK_WARDROBE_INSTRUCTION);
    }
    let mut inputs = Map::new();
    inputs.insert("prompt".into(), Value::String(gen_prompt));
    inputs.insert("output_path".into(), Value::String(rel.clone()));
    if let Some(ar) = aspect {
        inputs.insert("aspect_ratio".into(), Value::String(ar));
    }
    let mut reference_images: Vec<String> = identity_refs
        .iter()
        .map(|reference| reference.path.clone())
        .collect();
    for reference in &continuity_refs {
        if !reference_images.contains(&reference.path) {
            reference_images.push(reference.path.clone());
        }
    }
    if !reference_images.is_empty() {
        inputs.insert("reference_images".into(), json!(reference_images));
    }
    debug_assert!(
        !inputs.contains_key("source_frame") && !inputs.contains_key("source_image"),
        "scene look must not treat model sheets as source_frame"
    );
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("still_image".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let root_s = root.to_string_lossy().to_string();
    let validated = validate_action(action.clone(), Some(&root_s)).map_err(|e| e.to_string())?;
    let mut card = format!(
        "Scene look generate · {title} ({role_label})\nHost will save to `{rel}`.\nThis is a scene reference, not a character model-sheet closeup."
    );
    if !identity_refs.is_empty() {
        card.push_str("\nIdentity refs (model sheets as inputs, not outputs):");
        for r in &identity_refs {
            card.push_str(&format!(
                "\n- {} · {} · `{}`",
                r.character_name,
                r.role.label(),
                r.path
            ));
        }
    }
    if !continuity_refs.is_empty() {
        card.push_str("\nApproved continuity refs:");
        for reference in &continuity_refs {
            card.push_str(&format!(
                "\n- {} · {} · `{}`",
                reference.title,
                reference.role.label(),
                reference.path
            ));
        }
    }
    for name in &missing {
        card.push_str(&format!("\nNo model-sheet file found for {name}."));
    }
    card.push_str("\n\n");
    card.push_str(&approval_details(
        approval_app,
        &action,
        &validated,
        &root_s,
    )?);
    let job_id = state.next_job_id();
    let mut input_asset_ids: Vec<String> = identity_refs
        .iter()
        .map(|reference| reference.asset_id.clone())
        .collect();
    for reference in &continuity_refs {
        if !input_asset_ids.contains(&reference.asset_id) {
            input_asset_ids.push(reference.asset_id.clone());
        }
    }
    let mut job = JobRecord {
        input_asset_ids,
        output_path: Some(rel),
        raw_provider_id: Some(slot_id.to_string()),
        ..JobRecord::open(job_id, "scene_look")
    };
    if let Some(app) = approval_app {
        if let Ok(Some(estimate)) =
            crate::director::estimate_for_action(app, &action, Some(&root_s))
        {
            job.provider = Some(estimate.kind);
            job.model = estimate.model;
            job.currency = Some(estimate.currency);
            job.estimated_cost_minor = Some(estimate.minor);
        }
    }
    state.jobs.push(job);
    let note = if approval_app.is_some() {
        format!("Look generate ready for `{slot_id}`. Use the one-time approval button; chat text cannot approve it.")
    } else {
        format!("Look generate plan prepared for `{slot_id}`; no provider operation is authorized here.")
    };
    Ok((card, note))
}

const WARDROBE_PLATE_SHOTS: &[(&str, &str)] = &[
    (
        "closeup_front",
        "Close-up from the front, head and shoulders, face fully visible, neutral expression, eyes to camera.",
    ),
    (
        "standing_front",
        "Full-body standing front view, entire body in frame including the feet, arms relaxed, neutral expression.",
    ),
];

struct WardrobePlatePlan {
    character_id: String,
    character_name: String,
    shot: String,
    identity_id: String,
    identity_path: String,
    rel: String,
    prompt: String,
}

/// Every missing close-up and standing plate, wearing the scene's approved wardrobe.
/// One approval generates the whole set.
pub fn prepare_wardrobe_plate(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
) -> Result<(String, String), String> {
    prepare_wardrobe_plate_inner(root, state, scene_id, None)
}

pub fn prepare_wardrobe_plate_with_host(
    app: &crate::ctx::HostCtx,
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
) -> Result<(String, String), String> {
    prepare_wardrobe_plate_inner(root, state, scene_id, Some(app))
}

fn prepare_wardrobe_plate_inner(
    root: &Path,
    state: &mut ProjectState,
    scene_id: u32,
    approval_app: Option<&crate::ctx::HostCtx>,
) -> Result<(String, String), String> {
    let wardrobe = approved_wardrobe(root, state, scene_id)?;
    let characters = wardrobe_plate_characters(state, scene_id);
    if characters.is_empty() {
        return Err(
            "No named characters to dress. Name them on the wardrobe prompt or in the scene."
                .into(),
        );
    }
    let mut missing_sheets = Vec::new();
    let mut plans = Vec::new();
    for character in &characters {
        for (shot, framing) in WARDROBE_PLATE_SHOTS {
            if pick_wardrobe_plate(state, scene_id, &character.id, shot).is_some() {
                continue;
            }
            let identity = if *shot == "standing_front" {
                pick_body_asset(state, &character.id)
                    .or_else(|| pick_identity_asset(state, &character.id))
            } else {
                pick_identity_asset(state, &character.id)
                    .or_else(|| pick_body_asset(state, &character.id))
            };
            let Some(identity) = identity else {
                missing_sheets.push(format!("{} ({shot})", character.name));
                continue;
            };
            if !identity_file_exists(root, &identity.path) {
                missing_sheets.push(format!("{} ({shot}): {}", character.name, identity.path));
                continue;
            }
            let rel = next_plate_rel(root, scene_id, &character.id, shot)?;
            let prompt = format!(
                "Wardrobe plate of {}. {framing} Plain seamless neutral background. One person only. No scenery, no props, no text.\n\
Keep the same person as the identity image: face, age, hair, and body proportions.\n\
Dress them only in the clothes from the wardrobe reference: the same garments, colors, layers, wear, and period. Do not keep the identity image's original clothing.",
                character.name
            );
            plans.push(WardrobePlatePlan {
                character_id: character.id.clone(),
                character_name: character.name.clone(),
                shot: (*shot).to_string(),
                identity_id: identity.id.clone(),
                identity_path: identity.path.clone(),
                rel,
                prompt,
            });
        }
    }
    if !missing_sheets.is_empty() {
        return Err(format!(
            "These wardrobe plates need a model sheet on disk first: {}.",
            missing_sheets.join(", ")
        ));
    }
    if plans.is_empty() {
        let names = characters
            .iter()
            .map(|character| character.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Wardrobe plates already exist for {names}. Delete one to rebuild it."
        ));
    }
    let root_string = root.to_string_lossy();
    let plate_dir = crate::project_path::resolve(
        &format!("Preproduction/Scene{scene_id}/wardrobe_plates"),
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Write,
    )
    .map_err(|error| error.to_string())?
    .into_path_buf();
    fs::create_dir_all(&plate_dir)
        .map_err(|error| format!("could not create wardrobe plate folder: {error}"))?;
    let batch: Vec<Value> = plans
        .iter()
        .map(|plan| {
            json!({
                "prompt": plan.prompt,
                "output_path": plan.rel,
                "aspect_ratio": "3:4",
                "reference_images": [plan.identity_path, wardrobe.path],
            })
        })
        .collect();
    let mut inputs = Map::new();
    inputs.insert("batch".into(), Value::Array(batch));
    let action = DirectorAction {
        action: ActionKind::RunSpecialist,
        specialist: Some("still_image".into()),
        inputs,
        message: None,
        op: None,
        path: None,
    };
    let root_s = root.to_string_lossy().to_string();
    let validated = validate_action(action.clone(), Some(&root_s)).map_err(|e| e.to_string())?;
    let mut lines = vec![format!(
        "Wardrobe plates · Scene {scene_id} · {} stills",
        plans.len()
    )];
    lines.push(
        "One approval generates every missing close-up and standing plate. The host saves each 3:4 plate, then moves to the next."
            .into(),
    );
    for plan in &plans {
        lines.push(format!(
            "- {} · {} → `{}` (identity `{}`)",
            plan.character_name, plan.shot, plan.rel, plan.identity_path
        ));
    }
    lines.push(format!("Wardrobe lock: `{}`", wardrobe.path));
    lines.push(String::new());
    lines.push(approval_details(
        approval_app,
        &action,
        &validated,
        &root_s,
    )?);
    let wardrobe_id = wardrobe.asset_id.clone();
    let estimate = match approval_app {
        Some(app) => crate::director::estimate_for_action(app, &action, Some(&root_s))
            .ok()
            .flatten(),
        None => None,
    };
    for plan in &plans {
        let job_id = state.next_job_id();
        let mut job = JobRecord {
            input_asset_ids: vec![plan.identity_id.clone(), wardrobe_id.clone()],
            output_path: Some(plan.rel.clone()),
            raw_provider_id: Some(format!("{}|{}", plan.character_id, plan.shot)),
            ..JobRecord::open(job_id, "wardrobe_plate")
        };
        if let Some(estimate) = &estimate {
            job.provider = Some(estimate.kind.clone());
            job.model = estimate.model.clone();
            job.currency = Some(estimate.currency.clone());
            job.estimated_cost_minor = Some(estimate.minor);
        }
        state.jobs.push(job);
    }
    let count = plans.len();
    let names = plans
        .iter()
        .map(|plan| format!("{} {}", plan.character_name, plan.shot.replace('_', " ")))
        .collect::<Vec<_>>()
        .join(", ");
    let note = if approval_app.is_some() {
        format!("{count} wardrobe plates ready ({names}). Use the one-time approval button; chat text cannot approve it.")
    } else {
        format!(
            "{count} wardrobe plates prepared ({names}); no provider operation is authorized here."
        )
    };
    Ok((lines.join("\n"), note))
}

fn approved_wardrobe(
    root: &Path,
    state: &ProjectState,
    scene_id: u32,
) -> Result<LookContinuityRef, String> {
    collect_approved_look_continuity_refs(root, state, scene_id, true)
        .into_iter()
        .next()
        .ok_or_else(|| "Approve a wardrobe still before building wardrobe plates.".into())
}

fn wardrobe_plate_characters(state: &ProjectState, scene_id: u32) -> Vec<&CharacterRecord> {
    let Some(slot) = state
        .scene(scene_id)
        .and_then(|scene| scene.look_board.as_ref())
        .and_then(|board| {
            board
                .slots
                .iter()
                .find(|slot| slot.role == AssetRole::WardrobeReference)
        })
    else {
        return Vec::new();
    };
    mentioned_look_characters(
        state,
        scene_id,
        AssetRole::WardrobeReference,
        slot.prompt.as_deref().unwrap_or(""),
        &slot.title,
    )
}

fn next_plate_rel(
    root: &Path,
    scene_id: u32,
    character_id: &str,
    shot: &str,
) -> Result<String, String> {
    let root_string = root.to_string_lossy();
    let mut index = 1u32;
    loop {
        let name = if index == 1 {
            format!("{character_id}_{shot}.png")
        } else {
            format!("{character_id}_{shot}_{index:02}.png")
        };
        let rel = format!("Preproduction/Scene{scene_id}/wardrobe_plates/{name}");
        let path = crate::project_path::resolve(
            &rel,
            Some(root_string.as_ref()),
            crate::project_path::PathIntent::Write,
        )
        .map_err(|error| error.to_string())?;
        if !path.as_path().exists() {
            return Ok(rel);
        }
        index += 1;
    }
}

pub fn on_wardrobe_plate_saved(project_root: Option<&str>, save_path: &str) -> Option<String> {
    let root = project_root.filter(|value| !value.trim().is_empty())?;
    let root_path = Path::new(root);
    if !crate::project_path::resolve(
        crate::workflow::state::STATE_REL,
        Some(root),
        crate::project_path::PathIntent::Read,
    )
    .is_ok_and(|path| path.as_path().is_file())
    {
        return None;
    }
    let mut state = import::load_state(root_path).ok()?;
    let actual = actual_look_path(root_path, save_path)?;
    let rel = relativize(root_path, &actual.to_string_lossy())?;
    if !is_wardrobe_plate_rel(&rel) {
        return None;
    }
    let scene_id = scene_id_from_look_rel(&rel)?;
    if state.assets.iter().any(|asset| asset.path == rel) {
        return Some(format!("Wardrobe plate already registered: `{rel}`."));
    }
    let (job_id, raw) = state
        .jobs
        .iter()
        .find(|job| {
            job.kind == "wardrobe_plate"
                && job
                    .output_path
                    .as_deref()
                    .is_some_and(|path| same_path_stem(path, &rel))
        })
        .map(|job| (job.id.clone(), job.raw_provider_id.clone()))?;
    let (character_id, shot) = raw
        .as_deref()
        .and_then(|value| value.split_once('|'))
        .map(|(character_id, shot)| (character_id.to_string(), shot.to_string()))?;
    let asset_id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel.clone(),
        role: AssetRole::WardrobePlate,
        status: AssetStatus::Approved,
        scene_id: Some(scene_id),
        cut_id: None,
        character_id: Some(character_id.clone()),
        source_job_id: Some(job_id.clone()),
        parent_asset_id: None,
        notes: Some(shot.clone()),
    });
    if let Some(character) = state
        .characters
        .iter_mut()
        .find(|character| character.id == character_id)
    {
        if !character.asset_ids.contains(&asset_id) {
            character.asset_ids.push(asset_id.clone());
        }
    }
    if let Some(job) = state.jobs.iter_mut().find(|job| job.id == job_id) {
        job.finish(Some(&rel), Some(&asset_id));
    }
    let _ = import::save_project(root_path, state);
    Some(format!(
        "Registered wardrobe plate `{asset_id}` ({shot}) at `{rel}`. Later stills for this scene will use it instead of the bare model sheet."
    ))
}

pub fn delete_wardrobe_plate(
    root: &Path,
    state: &mut ProjectState,
    asset_id: &str,
) -> Result<String, String> {
    let asset = state
        .asset(asset_id)
        .filter(|asset| asset.role == AssetRole::WardrobePlate)
        .cloned()
        .ok_or_else(|| format!("wardrobe plate `{asset_id}` not found"))?;
    let root_s = root.to_string_lossy().to_string();
    let path =
        resolve_under_project(&asset.path, Some(&root_s)).map_err(|error| error.to_string())?;
    crate::project_path::under_scene_bucket(root, &path, "wardrobe_plates")?;
    if path.is_file() {
        crate::project_path::move_to_trash(root, &path)?;
    }
    state.assets.retain(|item| item.id != asset_id);
    for character in &mut state.characters {
        character.asset_ids.retain(|id| id != asset_id);
    }
    Ok(asset.path)
}

fn is_wardrobe_plate_rel(rel: &str) -> bool {
    let normalized = rel.replace('\\', "/");
    let parts: Vec<_> = normalized.split('/').collect();
    parts.len() == 4
        && parts[0] == "Preproduction"
        && parts[1].strip_prefix("Scene").is_some_and(|number| {
            !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
        })
        && parts[2] == "wardrobe_plates"
        && is_image_path(Path::new(parts[3]))
}

/// After still_image writes a file under Preproduction/SceneN/references/, register it.
pub fn on_still_saved(project_root: Option<&str>, save_path: &str) -> Option<String> {
    let root = project_root.filter(|s| !s.trim().is_empty())?;
    let root_path = Path::new(root);
    if !crate::project_path::resolve(
        crate::workflow::state::STATE_REL,
        Some(root),
        crate::project_path::PathIntent::Read,
    )
    .is_ok_and(|path| path.as_path().is_file())
    {
        return None;
    }
    let mut state = import::load_state(root_path).ok()?;
    let actual_path = actual_look_path(root_path, save_path)?;
    let rel = relativize(root_path, &actual_path.to_string_lossy())?;
    if !is_look_rel(&rel) {
        return None;
    }
    let scene_id = scene_id_from_look_rel(&rel)?;
    let role = role_from_look_rel(&rel)?;
    if state.assets.iter().any(|a| a.path == rel) {
        complete_matching_job(&mut state, &rel);
        let _ = import::save_project(root_path, state);
        return Some(format!("Look still already registered: `{rel}`."));
    }
    let parent_id = matching_slot(&state, scene_id, role).and_then(|s| {
        s.approved_asset_id
            .clone()
            .or_else(|| s.asset_ids.last().cloned())
    });
    let job_id = matching_job(&state, &rel).map(|j| j.id.clone());
    let asset_id = state.push_asset(AssetRecord {
        id: String::new(),
        path: rel.clone(),
        role,
        status: AssetStatus::Draft,
        scene_id: Some(scene_id),
        cut_id: None,
        character_id: None,
        source_job_id: job_id.clone(),
        parent_asset_id: parent_id,
        notes: Some("generated scene look".into()),
    });
    let scene = state.scene_mut(scene_id, &format!("Scene {scene_id}"));
    let board = scene.look_board.get_or_insert_with(LookBoard::default);
    ensure_core_slots(board, scene_id);
    if !board.slots.iter().any(|s| s.role == role) {
        board.slots.push(LookSlot::new(
            next_slot_id(board, scene_id),
            role,
            role.display_label(),
        ));
    }
    if let Some(slot) = board.slots.iter_mut().find(|s| s.role == role) {
        if !slot.asset_ids.contains(&asset_id) {
            slot.asset_ids.push(asset_id.clone());
        }
    }
    board.sync_asset_ids();
    if let Some(job) = state
        .jobs
        .iter_mut()
        .find(|j| job_id.as_deref() == Some(j.id.as_str()))
    {
        job.finish(Some(&rel), Some(&asset_id));
    }
    let _ = import::save_project(root_path, state);
    Some(format!(
        "Registered scene look draft `{asset_id}` at `{rel}`. Approve it on the Scene Look stage."
    ))
}

fn matching_slot(state: &ProjectState, scene_id: u32, role: AssetRole) -> Option<&LookSlot> {
    state
        .scene(scene_id)?
        .look_board
        .as_ref()?
        .slots
        .iter()
        .find(|s| s.role == role)
}

fn matching_job<'a>(state: &'a ProjectState, rel: &str) -> Option<&'a JobRecord> {
    state.jobs.iter().find(|j| {
        j.kind == "scene_look"
            && j.output_path
                .as_deref()
                .is_some_and(|path| same_path_stem(path, rel))
    })
}

fn complete_matching_job(state: &mut ProjectState, rel: &str) {
    if let Some(job) = state.jobs.iter_mut().find(|j| {
        j.output_path
            .as_deref()
            .is_some_and(|path| same_path_stem(path, rel))
    }) {
        job.finish(Some(rel), None);
    }
}

fn actual_look_path(root: &Path, requested: &str) -> Option<PathBuf> {
    let root_string = root.to_string_lossy();
    let requested = crate::project_path::resolve(
        requested,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .ok()?
    .into_path_buf();
    if requested.is_file() {
        return Some(requested);
    }
    let parent = requested.parent()?;
    let stem = requested.file_stem()?;
    fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find_map(|path| {
            if path.file_stem() != Some(stem) || !is_image_path(&path) {
                return None;
            }
            let confined = crate::project_path::resolve(
                &path.to_string_lossy(),
                Some(root_string.as_ref()),
                crate::project_path::PathIntent::Read,
            )
            .ok()?
            .into_path_buf();
            confined.is_file().then_some(confined)
        })
}

pub fn is_look_rel(rel: &str) -> bool {
    let n = rel.replace('\\', "/");
    let parts: Vec<_> = n.split('/').collect();
    parts.len() == 4
        && parts[0] == "Preproduction"
        && parts[1].strip_prefix("Scene").is_some_and(|number| {
            !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
        })
        && parts[2] == "references"
        && is_image_path(Path::new(parts[3]))
}

fn scene_id_from_look_rel(rel: &str) -> Option<u32> {
    let n = rel.replace('\\', "/");
    let rest = n.split("Preproduction/Scene").nth(1)?;
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}

fn role_from_look_rel(rel: &str) -> Option<AssetRole> {
    let name = Path::new(rel).file_stem()?.to_str()?;
    let stem = name.split('_').next().unwrap_or(name);
    AssetRole::parse_look(stem)
}

fn relativize(root: &Path, path: &str) -> Option<String> {
    let root_string = root.to_string_lossy();
    let confined = crate::project_path::resolve(
        path,
        Some(root_string.as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .ok()?;
    crate::project_path::relative_to_root(root, confined.as_path())
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn resolve_source(root: &Path, source: &str) -> Result<PathBuf, String> {
    crate::project_path::resolve(
        source,
        Some(root.to_string_lossy().as_ref()),
        crate::project_path::PathIntent::Read,
    )
    .map(|path| path.into_path_buf())
    .map_err(|error| error.to_string())
}

pub fn look_summary(scene: &SceneRecord) -> String {
    let Some(board) = &scene.look_board else {
        return "Scene look board: missing".into();
    };
    if board.is_empty() {
        return "Scene look board: empty".into();
    }
    let mut lines = vec![format!(
        "Scene look: {} slot(s), {} approved.",
        board.slots.len(),
        board.approved_count()
    )];
    for slot in &board.slots {
        let prompt = slot.prompt.as_deref().unwrap_or("(no prompt)");
        lines.push(format!(
            "- {} [{}] approved={} :: {}",
            slot.title,
            slot.role.label(),
            slot.approved_asset_id.is_some(),
            prompt.chars().take(160).collect::<String>()
        ));
    }
    lines.join("\n")
}

pub fn format_look_reply(
    scene_id: u32,
    title: &str,
    note: &str,
    board: Option<&LookBoard>,
) -> String {
    let mut lines = vec![
        format!("Scene {scene_id} — {title} · scene look (draft)"),
        String::new(),
        "This is the scene look board, not production notes and not character identity closeups."
            .into(),
    ];
    if !note.is_empty() {
        lines.push(String::new());
        lines.push(note.to_string());
    }
    if let Some(board) = board {
        for slot in &board.slots {
            lines.push(String::new());
            lines.push(format!(
                "{} ({})",
                if slot.title.is_empty() {
                    slot.role.display_label().to_string()
                } else {
                    slot.title.clone()
                },
                slot.role.label()
            ));
            if let Some(p) = &slot.prompt {
                lines.push(p.clone());
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::{JobStatus, SceneRecord};
    use serde_json::json;

    #[test]
    fn core_roles_parse_from_plain_words() {
        assert_eq!(
            AssetRole::parse_look("interior"),
            Some(AssetRole::InteriorReference)
        );
        assert_eq!(
            AssetRole::parse_look("train exterior"),
            Some(AssetRole::ExteriorReference)
        );
        assert_eq!(
            AssetRole::parse_look("wardrobe_reference"),
            Some(AssetRole::WardrobeReference)
        );
        assert!(AssetRole::parse_look("identity_reference").is_none());
        assert!(AssetRole::parse_look("cut_start_frame").is_none());
    }

    #[test]
    fn manual_prompt_edit_preserves_slot_versions_and_approval() {
        let mut scene = SceneRecord::new(1, "The last train");
        let mut board = LookBoard::default();
        let mut slot = LookSlot::new("s1_look_01", AssetRole::InteriorReference, "Old title");
        slot.prompt = Some("Old prompt".into());
        slot.negative_prompt = Some("Old negative".into());
        slot.asset_ids = vec!["asset_7".into(), "asset_8".into()];
        slot.approved_asset_id = Some("asset_8".into());
        board.slots.push(slot);
        scene.look_board = Some(board);

        patch_slot(
            &mut scene,
            "s1_look_01",
            Some(AssetRole::CompositionReference),
            Some(" New title ".into()),
            Some(" New prompt ".into()),
            Some(String::new()),
        )
        .unwrap();

        let slot = &scene.look_board.unwrap().slots[0];
        assert_eq!(slot.role, AssetRole::CompositionReference);
        assert_eq!(slot.title, "New title");
        assert_eq!(slot.prompt.as_deref(), Some("New prompt"));
        assert!(slot.negative_prompt.is_none());
        assert_eq!(slot.asset_ids, vec!["asset_7", "asset_8"]);
        assert_eq!(slot.approved_asset_id.as_deref(), Some("asset_8"));
    }

    #[test]
    fn llm_cannot_set_paths_and_core_roles_are_filled() {
        let mut scene = SceneRecord::new(1, "The last train");
        apply_llm_look(
            &mut scene,
            &json!({
                "slots": [
                    {
                        "role": "interior_reference",
                        "title": "Carriage",
                        "prompt": "narrow third-class carriage, rain on glass",
                        "path": "/tmp/hack.png",
                        "output_path": "Stills/nope.png"
                    },
                    {
                        "role": "mood_reference",
                        "title": "Lantern",
                        "prompt": "warm yellow lamp vs wet night"
                    }
                ]
            }),
        )
        .unwrap();
        let board = scene.look_board.unwrap();
        assert!(board
            .slots
            .iter()
            .any(|s| s.role == AssetRole::InteriorReference
                && s.prompt.as_deref() == Some("narrow third-class carriage, rain on glass")));
        for role in CORE_LOOK_ROLES {
            assert!(
                board.slots.iter().any(|s| s.role == role),
                "missing {role:?}",
            );
        }
        assert!(board
            .slots
            .iter()
            .all(|s| !s.prompt.as_deref().unwrap_or("").contains("/tmp")));
    }

    #[test]
    fn attach_copies_under_host_path_and_approve_sets_status() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Preproduction")).unwrap();
        let src = root.join("drop.jpg");
        fs::write(&src, b"fake-jpg").unwrap();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        board.slots.push(LookSlot::new(
            "s1_look_01",
            AssetRole::PropReference,
            "Parcel",
        ));
        scene.look_board = Some(board);
        let asset_id =
            attach_image(root, &mut state, 1, "s1_look_01", src.to_str().unwrap()).unwrap();
        let dest = root.join("Preproduction/Scene1/references/prop_1.jpg");
        assert!(dest.is_file());
        assert_eq!(
            state.asset(&asset_id).unwrap().role,
            AssetRole::PropReference
        );
        assert_eq!(state.asset(&asset_id).unwrap().status, AssetStatus::Draft);
        set_asset_status(
            &mut state,
            1,
            "s1_look_01",
            Some(&asset_id),
            AssetStatus::Approved,
        )
        .unwrap();
        assert_eq!(
            state.asset(&asset_id).unwrap().status,
            AssetStatus::Approved
        );
        assert_eq!(
            state.scenes[0].look_board.as_ref().unwrap().slots[0]
                .approved_asset_id
                .as_deref(),
            Some(asset_id.as_str())
        );
    }

    #[test]
    fn look_rel_versions_skip_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let dest_dir = root.join("Preproduction/Scene1/references");
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(dest_dir.join("mood_1.jpg"), b"a").unwrap();
        let scene = SceneRecord::new(1, "The last train");
        let rel = next_look_rel(root, &scene, AssetRole::MoodReference, "png");
        assert_eq!(rel, "Preproduction/Scene1/references/mood_2.png");
    }

    #[test]
    fn look_rel_versions_skip_pending_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = ProjectState::new();
        state.scene_mut(1, "The last train");
        state.jobs.push(JobRecord {
            id: "job_1".into(),
            kind: "scene_look".into(),
            provider: None,
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: Vec::new(),
            status: JobStatus::Pending,
            output_path: Some("Preproduction/Scene1/references/interior_1.png".into()),
            raw_provider_id: Some("s1_look_01".into()),
            error: None,
            attempts: Vec::new(),
            created_at: None,
            started_at: None,
            completed_at: None,
            scope: None,
            progress: None,
            cost_minor: None,
            estimated_cost_minor: None,
            currency: None,
            retry_of: None,
            spend_category: None,
            reconciliation: None,
        });
        let rel = next_look_rel_for_state(root, &state, 1, AssetRole::InteriorReference, "png");
        assert_eq!(rel, "Preproduction/Scene1/references/interior_2.png");
    }

    #[test]
    fn on_still_saved_registers_pending_job() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        let mut slot = LookSlot::new("s1_look_05", AssetRole::MoodReference, "Mood");
        slot.prompt = Some("wet night, red lantern".into());
        board.slots.push(slot);
        scene.look_board = Some(board);
        let rel = "Preproduction/Scene1/references/mood_1.png";
        state.jobs.push(JobRecord {
            id: "job_1".into(),
            kind: "scene_look".into(),
            provider: None,
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: Vec::new(),
            status: JobStatus::Pending,
            output_path: Some(rel.into()),
            raw_provider_id: Some("s1_look_05".into()),
            error: None,
            attempts: Vec::new(),
            created_at: None,
            started_at: None,
            completed_at: None,
            scope: None,
            progress: None,
            cost_minor: None,
            estimated_cost_minor: None,
            currency: None,
            retry_of: None,
            spend_category: None,
            reconciliation: None,
        });
        import::save_project(root, state).unwrap();
        let abs = root.join(rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"png").unwrap();
        let note = on_still_saved(Some(root.to_str().unwrap()), abs.to_str().unwrap()).unwrap();
        assert!(note.contains("Registered scene look"));
        let again = import::load_state(root).unwrap();
        assert_eq!(again.jobs[0].status, JobStatus::Completed);
        assert_eq!(again.assets[0].role, AssetRole::MoodReference);
        assert_eq!(again.assets[0].status, AssetStatus::Draft);
        assert_eq!(
            again.scenes[0].look_board.as_ref().unwrap().slots[0]
                .asset_ids
                .len(),
            1
        );
    }

    #[test]
    fn on_still_saved_registers_provider_changed_extension() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = ProjectState::new();
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        board.slots.push(LookSlot::new(
            "s1_look_01",
            AssetRole::InteriorReference,
            "Interior",
        ));
        scene.look_board = Some(board);
        let requested = "Preproduction/Scene1/references/interior_1.png";
        state.jobs.push(JobRecord {
            id: "job_1".into(),
            kind: "scene_look".into(),
            provider: None,
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: Vec::new(),
            status: JobStatus::Pending,
            output_path: Some(requested.into()),
            raw_provider_id: Some("s1_look_01".into()),
            error: None,
            attempts: Vec::new(),
            created_at: None,
            started_at: None,
            completed_at: None,
            scope: None,
            progress: None,
            cost_minor: None,
            estimated_cost_minor: None,
            currency: None,
            retry_of: None,
            spend_category: None,
            reconciliation: None,
        });
        import::save_project(root, state).unwrap();
        let actual = root.join("Preproduction/Scene1/references/interior_1.jpg");
        fs::create_dir_all(actual.parent().unwrap()).unwrap();
        fs::write(&actual, b"jpg").unwrap();
        let note = on_still_saved(Some(root.to_str().unwrap()), requested).unwrap();
        assert!(note.contains("interior_1.jpg"));
        let again = import::load_state(root).unwrap();
        assert_eq!(
            again.assets[0].path,
            "Preproduction/Scene1/references/interior_1.jpg"
        );
        assert_eq!(again.jobs[0].status, JobStatus::Completed);
        assert_eq!(
            again.jobs[0].output_path.as_deref(),
            Some("Preproduction/Scene1/references/interior_1.jpg")
        );
    }

    fn add_character(state: &mut ProjectState, name: &str, identity_path: &str, body_path: &str) {
        let id = crate::workflow::state::character_id(name);
        let ident_id = format!("{id}_ident");
        let body_id = format!("{id}_body");
        state.assets.push(AssetRecord {
            id: ident_id.clone(),
            path: identity_path.into(),
            role: AssetRole::IdentityReference,
            status: AssetStatus::Present,
            scene_id: None,
            cut_id: None,
            character_id: Some(id.clone()),
            source_job_id: None,
            parent_asset_id: None,
            notes: Some("approval shot".into()),
        });
        state.assets.push(AssetRecord {
            id: body_id.clone(),
            path: body_path.into(),
            role: AssetRole::BodyReference,
            status: AssetStatus::Present,
            scene_id: None,
            cut_id: None,
            character_id: Some(id.clone()),
            source_job_id: None,
            parent_asset_id: None,
            notes: Some("standing_front".into()),
        });
        state.characters.push(CharacterRecord {
            id,
            name: name.into(),
            dir: format!("Characters/{name}"),
            identity_ready: true,
            voice_ready: false,
            profile_path: None,
            model_sheet_manifest_path: None,
            profile: Default::default(),
            asset_ids: vec![ident_id, body_id],
            notes: vec![],
        });
    }

    fn tom_pip_state() -> ProjectState {
        let mut state = ProjectState::new();
        add_character(
            &mut state,
            "Tom Vale",
            "Characters/Tom Vale/model_sheet/tom_closeup_front.png",
            "Characters/Tom Vale/model_sheet/tom_standing_front.png",
        );
        add_character(
            &mut state,
            "Pip Vale",
            "Characters/Pip Vale/model_sheet/pip_closeup_front.png",
            "Characters/Pip Vale/model_sheet/pip_standing_front.png",
        );
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        let mut interior =
            LookSlot::new("s1_look_01", AssetRole::InteriorReference, "Interior Scene");
        interior.prompt = Some(
            "Tom Vale and Pip Vale in a narrow third-class carriage, rain on the glass".into(),
        );
        let mut mood = LookSlot::new("s1_look_05", AssetRole::MoodReference, "Mood");
        mood.prompt = Some("warm yellow lamp vs wet night, empty corridor".into());
        board.slots.push(interior);
        board.slots.push(mood);
        scene.look_board = Some(board);
        state
    }

    #[test]
    fn interior_naming_tom_and_pip_collects_model_sheets() {
        let state = tom_pip_state();
        let scene = &state.scenes[0];
        let slot = &scene.look_board.as_ref().unwrap().slots[0];
        let refs = collect_look_identity_refs(
            &state,
            1,
            slot.role,
            slot.prompt.as_deref().unwrap(),
            &slot.title,
        );
        let paths: Vec<&str> = refs.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"Characters/Tom Vale/model_sheet/tom_closeup_front.png"));
        assert!(paths.contains(&"Characters/Tom Vale/model_sheet/tom_standing_front.png"));
        assert!(paths.contains(&"Characters/Pip Vale/model_sheet/pip_closeup_front.png"));
        assert!(paths.contains(&"Characters/Pip Vale/model_sheet/pip_standing_front.png"));
        assert_eq!(refs.len(), 4);
        assert!(refs
            .iter()
            .all(|r| r.role == AssetRole::IdentityReference || r.role == AssetRole::BodyReference));
    }

    #[test]
    fn mood_without_names_skips_identity_refs() {
        let state = tom_pip_state();
        let slot = &state.scenes[0].look_board.as_ref().unwrap().slots[1];
        let refs = collect_look_identity_refs(
            &state,
            1,
            slot.role,
            slot.prompt.as_deref().unwrap(),
            &slot.title,
        );
        assert!(refs.is_empty());
    }

    #[test]
    fn interior_without_names_falls_back_to_character_focus() {
        let mut state = tom_pip_state();
        {
            let scene = state.scene_mut(1, "The last train");
            scene.intent = Some(SceneIntent {
                character_focus: vec!["Tom Vale".into(), "Pip".into()],
                ..SceneIntent::default()
            });
            let slot = scene
                .look_board
                .as_mut()
                .unwrap()
                .slots
                .iter_mut()
                .find(|s| s.role == AssetRole::InteriorReference)
                .unwrap();
            slot.prompt = Some("narrow third-class carriage, rain on glass".into());
        }
        let refs = collect_look_identity_refs(
            &state,
            1,
            AssetRole::InteriorReference,
            "narrow third-class carriage, rain on glass",
            "Interior Scene",
        );
        let names: Vec<&str> = refs.iter().map(|r| r.character_name.as_str()).collect();
        assert!(names.contains(&"Tom Vale"));
        assert!(names.contains(&"Pip Vale"));
    }

    #[test]
    fn prepare_generate_attaches_sheets_not_as_source_or_output() {
        let _session_guard = crate::session::test_session_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = tom_pip_state();
        for path in [
            "Characters/Tom Vale/model_sheet/tom_closeup_front.png",
            "Characters/Tom Vale/model_sheet/tom_standing_front.png",
            "Characters/Pip Vale/model_sheet/pip_closeup_front.png",
            "Characters/Pip Vale/model_sheet/pip_standing_front.png",
        ] {
            let abs = root.join(path);
            fs::create_dir_all(abs.parent().unwrap()).unwrap();
            fs::write(&abs, b"png").unwrap();
        }
        let (card, _) = prepare_generate(root, &mut state, 1, "s1_look_01").unwrap();
        assert!(card.contains("Characters/Tom Vale/model_sheet/tom_closeup_front.png"));
        assert!(card.contains("Characters/Pip Vale/model_sheet/pip_closeup_front.png"));
        assert!(card.contains("Identity refs"));
        let job = state.jobs.last().unwrap();
        assert_eq!(job.input_asset_ids.len(), 4);
        let out = job.output_path.as_deref().unwrap();
        assert!(out.starts_with("Preproduction/Scene1/references/interior_"));
        assert!(!out.contains("Characters/"));
        assert!(!out.contains("closeup"));
        assert!(card.contains("### Files read or uploaded"));
        assert!(card.contains("### Artifacts modified"));
        assert!(!card.contains("```json"));
        let reads = card.split("### Artifacts modified").next().unwrap_or(&card);
        assert!(reads.contains("tom_closeup_front.png"));
        assert!(!reads.contains("source_frame"));
    }

    #[test]
    fn approved_wardrobe_is_attached_after_model_sheets() {
        let _session_guard = crate::session::test_session_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = tom_pip_state();
        for path in [
            "Characters/Tom Vale/model_sheet/tom_closeup_front.png",
            "Characters/Tom Vale/model_sheet/tom_standing_front.png",
            "Characters/Pip Vale/model_sheet/pip_closeup_front.png",
            "Characters/Pip Vale/model_sheet/pip_standing_front.png",
            "Preproduction/Scene1/references/wardrobe_1.png",
        ] {
            let abs = root.join(path);
            fs::create_dir_all(abs.parent().unwrap()).unwrap();
            fs::write(&abs, b"png").unwrap();
        }
        state.assets.push(AssetRecord {
            id: "wardrobe_lock".into(),
            path: "Preproduction/Scene1/references/wardrobe_1.png".into(),
            role: AssetRole::WardrobeReference,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: None,
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: Some("approved clothing continuity".into()),
        });
        let board = state.scenes[0].look_board.as_mut().unwrap();
        let mut wardrobe = LookSlot::new(
            "s1_look_03",
            AssetRole::WardrobeReference,
            "Wet journey clothes",
        );
        wardrobe.asset_ids.push("wardrobe_lock".into());
        wardrobe.approved_asset_id = Some("wardrobe_lock".into());
        board.slots.push(wardrobe);
        board.sync_asset_ids();

        let (card, _) = prepare_generate(root, &mut state, 1, "s1_look_01").unwrap();
        assert!(card.contains("Approved continuity refs"));
        assert!(card.contains("Preproduction/Scene1/references/wardrobe_1.png"));
        let job = state.jobs.last().unwrap();
        assert_eq!(
            job.input_asset_ids.last().map(String::as_str),
            Some("wardrobe_lock")
        );
        assert_eq!(job.input_asset_ids.len(), 5);
        assert!(card.contains("### Files read or uploaded"));
        assert!(card.contains("### Artifacts modified"));
        assert!(!card.contains("```json"));
        let reads = card.split("### Artifacts modified").next().unwrap_or(&card);
        let model_sheet = reads
            .find("Characters/Tom Vale/model_sheet/tom_closeup_front.png")
            .unwrap();
        let wardrobe = reads
            .find("Preproduction/Scene1/references/wardrobe_1.png")
            .unwrap();
        assert!(model_sheet < wardrobe);
        assert!(!card.contains("Match the named characters' clothing"));
    }

    #[test]
    fn wardrobe_plate_replaces_the_closeup_model_sheet() {
        let mut state = tom_pip_state();
        state.assets.push(AssetRecord {
            id: "tom_coat".into(),
            path: "Preproduction/Scene1/wardrobe_plates/tom_vale_closeup_front.png".into(),
            role: AssetRole::WardrobePlate,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: None,
            character_id: Some("tom_vale".into()),
            source_job_id: None,
            parent_asset_id: None,
            notes: Some("closeup_front".into()),
        });
        let refs = collect_look_identity_refs(
            &state,
            1,
            AssetRole::InteriorReference,
            "Tom Vale and Pip Vale in the carriage",
            "Interior",
        );
        let paths: Vec<&str> = refs
            .iter()
            .map(|reference| reference.path.as_str())
            .collect();
        assert!(paths.contains(&"Preproduction/Scene1/wardrobe_plates/tom_vale_closeup_front.png"));
        assert!(!paths.contains(&"Characters/Tom Vale/model_sheet/tom_closeup_front.png"));
        assert!(paths.contains(&"Characters/Tom Vale/model_sheet/tom_standing_front.png"));
    }

    #[test]
    fn prepare_wardrobe_plate_uses_identity_and_approved_wardrobe() {
        let _session_guard = crate::session::test_session_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = tom_pip_state();
        for path in [
            "Characters/Tom Vale/model_sheet/tom_closeup_front.png",
            "Characters/Tom Vale/model_sheet/tom_standing_front.png",
            "Characters/Pip Vale/model_sheet/pip_closeup_front.png",
            "Characters/Pip Vale/model_sheet/pip_standing_front.png",
            "Preproduction/Scene1/references/wardrobe_1.png",
        ] {
            let abs = root.join(path);
            fs::create_dir_all(abs.parent().unwrap()).unwrap();
            fs::write(&abs, b"png").unwrap();
        }
        state.assets.push(AssetRecord {
            id: "wardrobe_lock".into(),
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
        let scene = state.scenes.iter_mut().find(|scene| scene.id == 1).unwrap();
        let mut wardrobe =
            LookSlot::new("s1_look_03", AssetRole::WardrobeReference, "Travel coats");
        wardrobe.prompt = Some("Tom Vale and Pip Vale in worn period coats".into());
        wardrobe.asset_ids.push("wardrobe_lock".into());
        wardrobe.approved_asset_id = Some("wardrobe_lock".into());
        scene.look_board.as_mut().unwrap().slots.push(wardrobe);

        let (card, _) = prepare_wardrobe_plate(root, &mut state, 1).unwrap();
        assert!(card.contains("4 stills"));
        assert!(card.contains("Tom Vale"));
        assert!(card.contains("Pip Vale"));
        assert!(card.contains("closeup_front"));
        assert!(card.contains("standing_front"));
        assert!(card.contains("tom_closeup_front.png"));
        assert!(card.contains("pip_standing_front.png"));
        assert!(card.contains("wardrobe_1.png"));
        assert!(card.contains("### Files read or uploaded"));
        assert!(card.contains("### Artifacts modified"));
        assert!(!card.contains("```json"));
        assert!(!card.contains("\"aspect_ratio\": \"3:4\""));
        assert!(!card.contains("\"aspect_ratio\":\"3:4\""));
        let reads = card.split("### Artifacts modified").next().unwrap_or(&card);
        assert!(reads.contains("tom_closeup_front.png"));
        assert!(reads.contains("wardrobe_1.png"));
        let writes = card.split("### Artifacts modified").nth(1).unwrap_or("");
        assert!(writes.contains("wardrobe_plates"));
        let ids: Vec<_> = state
            .jobs
            .iter()
            .filter(|job| job.kind == "wardrobe_plate")
            .map(|job| job.raw_provider_id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                Some("tom_vale|closeup_front".into()),
                Some("tom_vale|standing_front".into()),
                Some("pip_vale|closeup_front".into()),
                Some("pip_vale|standing_front".into()),
            ]
        );
    }

    #[test]
    fn delete_image_removes_exact_version_file_and_registry_links() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rel = "Preproduction/Scene1/references/interior_2.png";
        let abs = root.join(rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"png").unwrap();
        let mut state = ProjectState::new();
        state.assets.push(AssetRecord {
            id: "look_2".into(),
            path: rel.into(),
            role: AssetRole::InteriorReference,
            status: AssetStatus::Approved,
            scene_id: Some(1),
            cut_id: None,
            character_id: None,
            source_job_id: Some("job_2".into()),
            parent_asset_id: None,
            notes: None,
        });
        state.jobs.push(JobRecord {
            id: "job_2".into(),
            kind: "scene_look".into(),
            provider: None,
            model: None,
            input_asset_ids: Vec::new(),
            output_asset_ids: vec!["look_2".into()],
            status: JobStatus::Completed,
            output_path: Some(rel.into()),
            raw_provider_id: Some("s1_look_01".into()),
            error: None,
            attempts: Vec::new(),
            created_at: None,
            started_at: None,
            completed_at: None,
            scope: None,
            progress: None,
            cost_minor: None,
            estimated_cost_minor: None,
            currency: None,
            retry_of: None,
            spend_category: None,
            reconciliation: None,
        });
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        let mut slot = LookSlot::new("s1_look_01", AssetRole::InteriorReference, "Interior Scene");
        slot.asset_ids.push("look_2".into());
        slot.approved_asset_id = Some("look_2".into());
        board.slots.push(slot);
        board.sync_asset_ids();
        scene.look_board = Some(board);

        let deleted = delete_image(root, &mut state, 1, "s1_look_01", "look_2").unwrap();
        assert_eq!(deleted, rel);
        assert!(!abs.exists());
        assert!(state.asset("look_2").is_none());
        assert!(state.scenes[0]
            .look_board
            .as_ref()
            .unwrap()
            .asset_ids
            .is_empty());
        assert!(state.scenes[0].look_board.as_ref().unwrap().slots[0]
            .approved_asset_id
            .is_none());
        assert!(state.jobs[0].output_asset_ids.is_empty());
        assert_eq!(state.jobs[0].output_path.as_deref(), Some(rel));
    }

    #[test]
    fn load_prunes_thumbnail_when_file_was_deleted_manually() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rel = "Preproduction/Scene1/references/interior_1.jpg";
        let abs = root.join(rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, b"jpg").unwrap();
        let mut state = ProjectState::new();
        state.assets.push(AssetRecord {
            id: "look_1".into(),
            path: rel.into(),
            role: AssetRole::InteriorReference,
            status: AssetStatus::Draft,
            scene_id: Some(1),
            cut_id: None,
            character_id: None,
            source_job_id: None,
            parent_asset_id: None,
            notes: None,
        });
        let scene = state.scene_mut(1, "The last train");
        let mut board = LookBoard::default();
        let mut slot = LookSlot::new("s1_look_01", AssetRole::InteriorReference, "Interior Scene");
        slot.asset_ids.push("look_1".into());
        board.slots.push(slot);
        board.sync_asset_ids();
        scene.look_board = Some(board);
        import::save_project(root, state).unwrap();
        fs::remove_file(abs).unwrap();

        let loaded = import::load_state(root).unwrap();
        assert!(loaded.asset("look_1").is_none());
        assert!(loaded.scenes[0]
            .look_board
            .as_ref()
            .unwrap()
            .asset_ids
            .is_empty());
        assert!(loaded.scenes[0].look_board.as_ref().unwrap().slots[0]
            .asset_ids
            .is_empty());
    }

    #[test]
    fn llm_cannot_invent_identity_paths() {
        let mut scene = SceneRecord::new(1, "The last train");
        apply_llm_look(
            &mut scene,
            &json!({
                "slots": [{
                    "role": "interior_reference",
                    "title": "Interior Scene",
                    "prompt": "Tom Vale and Pip Vale in the carriage",
                    "reference_images": ["Characters/Tom Vale/model_sheet/hack.png"],
                    "source_frame": "Characters/Tom Vale/model_sheet/tom_closeup_front.png"
                }]
            }),
        )
        .unwrap();
        let slot = scene
            .look_board
            .unwrap()
            .slots
            .into_iter()
            .find(|s| s.role == AssetRole::InteriorReference)
            .unwrap();
        assert_eq!(
            slot.prompt.as_deref(),
            Some("Tom Vale and Pip Vale in the carriage")
        );
        assert!(slot.asset_ids.is_empty());
    }

    #[test]
    fn saved_look_paths_must_be_confined_and_use_the_exact_project_layout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let outside = dir
            .path()
            .join("Preproduction/Scene1/references/interior_1.png");
        fs::create_dir_all(root.join("Preproduction/Scene1/references")).unwrap();
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(&outside, b"outside").unwrap();

        assert!(actual_look_path(&root, outside.to_str().unwrap()).is_none());
        assert!(!is_look_rel(
            "../Preproduction/Scene1/references/interior_1.png"
        ));
        assert!(!is_look_rel(
            "elsewhere/Preproduction/Scene1/references/interior_1.png"
        ));
        assert!(is_look_rel(
            "Preproduction/Scene1/references/interior_1.png"
        ));
        assert!(!is_wardrobe_plate_rel(
            "elsewhere/Preproduction/Scene1/wardrobe_plates/plate.png"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn wardrobe_plate_destination_rejects_escaping_directory_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let outside = dir.path().join("outside");
        fs::create_dir_all(root.join("Preproduction/Scene1")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("Preproduction/Scene1/wardrobe_plates")).unwrap();

        assert!(next_plate_rel(&root, 1, "tom_vale", "closeup_front").is_err());
    }
}

//! Production profile and script-breakdown requirements.
//!
//! Derived on import from the script, characters, and scene intent. Display
//! order is not identity: registry and deliverable ids come from kind plus a
//! content key.
use crate::workflow::state::{
    deliverable_id, requirement_id, AssetRole, AssetStatus, CharacterRecord, DeliverableRecord,
    ProductionProfile, ProductionRecipe, ProjectState, RequirementKind, RequirementRecord,
    RequirementStatus, SceneRecord,
};

#[derive(Debug, Default)]
struct ScriptHeader {
    runtime: Option<String>,
    aspect: Option<String>,
    audience: Option<String>,
    platforms: Option<String>,
    resolution: Option<String>,
    frame_rate: Option<String>,
    language: Option<String>,
    medium: Option<String>,
    style_notes: Option<String>,
    audio_plan: Option<String>,
    continuity: Option<String>,
    safety: Option<String>,
    budget: Option<String>,
    schedule: Option<String>,
    rights: Option<String>,
    characters: Vec<String>,
}

/// Fill `profile` and `requirements` from the current scenes and script text.
pub fn apply(state: &mut ProjectState, script_texts: &[String]) {
    let header = parse_headers(script_texts);
    let recipe = state
        .profile
        .as_ref()
        .map(|profile| profile.recipe.clone())
        .unwrap_or_default();
    let mut profile = profile_from(state, &header);
    profile.recipe = recipe;
    state.profile = Some(profile);
    state.requirements = requirements_from(state, &header);
}

/// Keep edited profile fields and explicit requirement statuses across a rescan.
pub fn merge_preserved(
    state: &mut ProjectState,
    previous_profile: Option<ProductionProfile>,
    previous_requirements: Vec<RequirementRecord>,
) {
    if let Some(previous) = previous_profile {
        let derived = state.profile.clone().unwrap_or_default();
        state.profile = Some(merge_profile(previous, derived));
    }
    let derived = std::mem::take(&mut state.requirements);
    let mut merged = Vec::new();
    for mut item in derived {
        if let Some(previous) = previous_requirements.iter().find(|req| req.id == item.id) {
            if previous.status_override {
                item.status = previous.status;
                item.status_override = true;
            }
            if previous.user_added {
                item.user_added = true;
            }
            if item.notes.is_none() {
                item.notes = previous.notes.clone();
            }
        }
        merged.push(item);
    }
    for previous in previous_requirements {
        if previous.user_added && merged.iter().all(|item| item.id != previous.id) {
            merged.push(previous);
        }
    }
    sort_requirements(&mut merged);
    state.requirements = merged;
}

fn profile_from(state: &ProjectState, header: &ScriptHeader) -> ProductionProfile {
    let title = state
        .scripts
        .iter()
        .find_map(|script| script.title.clone())
        .or_else(|| state.project_name.clone())
        .unwrap_or_else(|| "Untitled".into());
    let aspect = header
        .aspect
        .clone()
        .or_else(|| state.settings.aspect_ratio.clone())
        .or_else(|| {
            state
                .scripts
                .iter()
                .find_map(|script| script.aspect_ratio.clone())
        });
    let platforms = split_list(header.platforms.as_deref());
    let deliverable = DeliverableRecord {
        id: deliverable_id(&title),
        name: title,
        audience: header.audience.clone(),
        platforms: platforms.clone(),
        runtime: header.runtime.clone(),
        aspect_ratio: aspect.clone(),
        resolution: header.resolution.clone(),
        frame_rate: header.frame_rate.clone(),
        language: header.language.clone(),
    };
    ProductionProfile {
        recipe: ProductionRecipe::narrative(),
        recipe_document: None,
        deliverables: vec![deliverable],
        audience: header.audience.clone(),
        platforms,
        runtime: header.runtime.clone(),
        aspect_ratios: aspect.into_iter().collect(),
        resolution: header.resolution.clone(),
        frame_rate: header.frame_rate.clone(),
        language: header.language.clone(),
        medium: header.medium.clone(),
        style_notes: header.style_notes.clone(),
        audio_plan: header.audio_plan.clone(),
        continuity_rules: header.continuity.clone().into_iter().collect(),
        safety_constraints: header.safety.clone().into_iter().collect(),
        budget: header.budget.clone(),
        schedule: header.schedule.clone(),
        rights_notes: header.rights.clone(),
    }
}

fn requirements_from(state: &ProjectState, header: &ScriptHeader) -> Vec<RequirementRecord> {
    let mut requirements = Vec::new();
    let graph = crate::workflow::recipe::bound(state);
    if graph.requires_role("cast") {
        let mut cast_names = header.characters.clone();
        for character in &state.characters {
            if !cast_names
                .iter()
                .any(|name| fold_alnum(name) == fold_alnum(&character.name))
            {
                cast_names.push(character.name.clone());
            }
        }
        for name in &cast_names {
            let character = find_character(&state.characters, name);
            let scene_ids = scenes_mentioning(state, name);
            let key = character
                .map(|character| character.id.clone())
                .unwrap_or_else(|| slug_key(name));
            upsert(
                &mut requirements,
                RequirementRecord {
                    id: requirement_id(RequirementKind::Cast, &key),
                    kind: RequirementKind::Cast,
                    name: character
                        .map(|character| character.name.clone())
                        .unwrap_or_else(|| name.clone()),
                    status: if character.is_some_and(|character| character.identity_ready) {
                        RequirementStatus::Present
                    } else {
                        RequirementStatus::Missing
                    },
                    scene_ids,
                    character_id: character.map(|character| character.id.clone()),
                    notes: None,
                    status_override: false,
                    user_added: false,
                },
            );
        }
    }

    for scene in &state.scenes {
        if graph.requires_role("voice") {
            for line in &scene.dialogue {
                let speaker = speaker_name(&line.speaker);
                if speaker.is_empty() {
                    continue;
                }
                let character = find_character(&state.characters, &speaker);
                let key = character
                    .map(|character| character.id.clone())
                    .unwrap_or_else(|| slug_key(&speaker));
                let name = character
                    .map(|character| character.name.clone())
                    .unwrap_or(speaker);
                upsert(
                    &mut requirements,
                    RequirementRecord {
                        id: requirement_id(RequirementKind::Voice, &key),
                        kind: RequirementKind::Voice,
                        name,
                        status: if character.is_some_and(|character| character.voice_ready) {
                            RequirementStatus::Present
                        } else {
                            RequirementStatus::Missing
                        },
                        scene_ids: vec![scene.id],
                        character_id: character.map(|character| character.id.clone()),
                        notes: None,
                        status_override: false,
                        user_added: false,
                    },
                );
            }
        }
        if let Some(location) = scene
            .location
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let (label, key) = location_label_and_key(location);
            upsert(
                &mut requirements,
                RequirementRecord {
                    id: requirement_id(RequirementKind::Location, &key),
                    kind: RequirementKind::Location,
                    name: label.clone(),
                    status: location_status(scene, &label),
                    scene_ids: vec![scene.id],
                    character_id: None,
                    notes: Some(location.to_string()),
                    status_override: false,
                    user_added: false,
                },
            );
        }
        if scene_mentions_parcel(scene) {
            upsert(
                &mut requirements,
                RequirementRecord {
                    id: requirement_id(RequirementKind::Prop, "parcel"),
                    kind: RequirementKind::Prop,
                    name: "Parcel".into(),
                    status: prop_status(state, "parcel"),
                    scene_ids: vec![scene.id],
                    character_id: None,
                    notes: Some("Named in the script.".into()),
                    status_override: false,
                    user_added: false,
                },
            );
        }
        if let Some(intent) = &scene.intent {
            for prop in &intent.important_props {
                let prop = prop.trim();
                if prop.is_empty() {
                    continue;
                }
                let key = prop_key(prop);
                upsert(
                    &mut requirements,
                    RequirementRecord {
                        id: requirement_id(RequirementKind::Prop, &key),
                        kind: RequirementKind::Prop,
                        name: prop.to_string(),
                        status: prop_status(state, &key),
                        scene_ids: vec![scene.id],
                        character_id: None,
                        notes: None,
                        status_override: false,
                        user_added: false,
                    },
                );
            }
        }
        for sound in &scene.sound {
            let sound = sound.trim();
            if sound.is_empty() {
                continue;
            }
            upsert(
                &mut requirements,
                RequirementRecord {
                    id: requirement_id(RequirementKind::Sound, sound),
                    kind: RequirementKind::Sound,
                    name: shorten(sound, 72),
                    status: RequirementStatus::Draft,
                    scene_ids: vec![scene.id],
                    character_id: None,
                    notes: Some(sound.to_string()),
                    status_override: false,
                    user_added: false,
                },
            );
        }
        if let Some(board) = &scene.look_board {
            for slot in &board.slots {
                if slot.role != AssetRole::WardrobeReference || slot.title.trim().is_empty() {
                    continue;
                }
                upsert(
                    &mut requirements,
                    RequirementRecord {
                        id: requirement_id(RequirementKind::Wardrobe, &slot.title),
                        kind: RequirementKind::Wardrobe,
                        name: slot.title.clone(),
                        status: if slot.approved_asset_id.is_some() {
                            RequirementStatus::Approved
                        } else {
                            RequirementStatus::Draft
                        },
                        scene_ids: vec![scene.id],
                        character_id: None,
                        notes: None,
                        status_override: false,
                        user_added: false,
                    },
                );
            }
        }
    }

    if graph.requires_role("voice") || graph.requires_role("cast") {
        for character in &state.characters {
            if graph.requires_role("voice") && character.voice_ready {
                let scene_ids = scenes_mentioning(state, &character.name);
                upsert(
                    &mut requirements,
                    RequirementRecord {
                        id: requirement_id(RequirementKind::Voice, &character.id),
                        kind: RequirementKind::Voice,
                        name: character.name.clone(),
                        status: RequirementStatus::Present,
                        scene_ids,
                        character_id: Some(character.id.clone()),
                        notes: None,
                        status_override: false,
                        user_added: false,
                    },
                );
            }
            if graph.requires_role("cast") {
                let scenes = scenes_mentioning(state, &character.name);
                for item in &character.profile.wardrobe {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    upsert(
                        &mut requirements,
                        RequirementRecord {
                            id: requirement_id(
                                RequirementKind::Wardrobe,
                                &format!("{} {item}", character.id),
                            ),
                            kind: RequirementKind::Wardrobe,
                            name: item.to_string(),
                            status: RequirementStatus::Present,
                            scene_ids: scenes.clone(),
                            character_id: Some(character.id.clone()),
                            notes: None,
                            status_override: false,
                            user_added: false,
                        },
                    );
                }
            }
        }
    }

    if graph.requires_role("cast") {
        for asset in &state.assets {
            if asset.role != AssetRole::WardrobePlate {
                continue;
            }
            let name = asset
                .notes
                .as_deref()
                .map(str::trim)
                .filter(|note| !note.is_empty())
                .unwrap_or("Wardrobe plate")
                .to_string();
            let key = format!(
                "{} {name}",
                asset.character_id.as_deref().unwrap_or("plate")
            );
            let status = match asset.status {
                AssetStatus::Approved => RequirementStatus::Approved,
                AssetStatus::Rejected => RequirementStatus::Missing,
                AssetStatus::Draft => RequirementStatus::Draft,
                AssetStatus::Present | AssetStatus::Superseded => RequirementStatus::Present,
            };
            upsert(
                &mut requirements,
                RequirementRecord {
                    id: requirement_id(RequirementKind::Plate, &key),
                    kind: RequirementKind::Plate,
                    name,
                    status,
                    scene_ids: asset.scene_id.into_iter().collect(),
                    character_id: asset.character_id.clone(),
                    notes: Some(asset.path.clone()),
                    status_override: false,
                    user_added: false,
                },
            );
        }
    }

    sort_requirements(&mut requirements);
    requirements
}

fn upsert(requirements: &mut Vec<RequirementRecord>, incoming: RequirementRecord) {
    if let Some(existing) = requirements.iter_mut().find(|item| item.id == incoming.id) {
        for scene_id in incoming.scene_ids {
            if !existing.scene_ids.contains(&scene_id) {
                existing.scene_ids.push(scene_id);
            }
        }
        existing.scene_ids.sort_unstable();
        if incoming.status.rank() > existing.status.rank() {
            existing.status = incoming.status;
        }
        if existing.character_id.is_none() {
            existing.character_id = incoming.character_id;
        }
        if existing.notes.is_none() {
            existing.notes = incoming.notes;
        }
        if incoming.name.len() > existing.name.len() {
            existing.name = incoming.name;
        }
        return;
    }
    requirements.push(incoming);
}

fn sort_requirements(requirements: &mut [RequirementRecord]) {
    for requirement in requirements.iter_mut() {
        requirement.scene_ids.sort_unstable();
        requirement.scene_ids.dedup();
    }
    requirements.sort_by(|a, b| {
        a.kind
            .sort_key()
            .cmp(&b.kind.sort_key())
            .then_with(|| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
            })
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn location_status(scene: &SceneRecord, label: &str) -> RequirementStatus {
    let interior = label.eq_ignore_ascii_case("Train interior");
    let approved = scene.look_board.as_ref().is_some_and(|board| {
        board.slots.iter().any(|slot| {
            let role_ok = if interior {
                slot.role == AssetRole::InteriorReference
            } else {
                matches!(
                    slot.role,
                    AssetRole::LocationReference
                        | AssetRole::InteriorReference
                        | AssetRole::ExteriorReference
                )
            };
            role_ok && slot.approved_asset_id.is_some()
        })
    });
    if approved {
        RequirementStatus::Approved
    } else {
        RequirementStatus::Draft
    }
}

fn prop_status(state: &ProjectState, key: &str) -> RequirementStatus {
    let needle = key.replace('_', " ");
    for scene in &state.scenes {
        let Some(board) = &scene.look_board else {
            continue;
        };
        for slot in &board.slots {
            if slot.role != AssetRole::PropReference || slot.approved_asset_id.is_none() {
                continue;
            }
            let haystack = format!("{} {}", slot.title, slot.prompt.as_deref().unwrap_or(""))
                .to_ascii_lowercase();
            if haystack.contains(&needle) {
                return RequirementStatus::Approved;
            }
        }
    }
    RequirementStatus::Draft
}

fn location_label_and_key(raw: &str) -> (String, String) {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("train")
        && (lower.contains("carriage")
            || lower.contains("compartment")
            || lower.contains("interior"))
    {
        return ("Train interior".into(), "train_interior".into());
    }
    let label = raw.split([',', '.']).next().unwrap_or(raw).trim();
    let label = shorten(label, 64);
    let key = slug_key(&label);
    (label, key)
}

fn prop_key(name: &str) -> String {
    let folded = fold_alnum(name);
    if folded.contains("parcel") {
        "parcel".into()
    } else {
        slug_key(name)
    }
}

fn scene_mentions_parcel(scene: &SceneRecord) -> bool {
    let mut blob = scene.action.join(" ");
    blob.push(' ');
    blob.push_str(
        &scene
            .dialogue
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    if let Some(intent) = &scene.intent {
        blob.push(' ');
        blob.push_str(&intent.important_props.join(" "));
    }
    tokens(&blob)
        .iter()
        .any(|token| token == "parcel" || token == "parcels")
}

fn scenes_mentioning(state: &ProjectState, name: &str) -> Vec<u32> {
    let mut ids: Vec<u32> = state
        .scenes
        .iter()
        .filter(|scene| scene_mentions(scene, name))
        .map(|scene| scene.id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn scene_mentions(scene: &SceneRecord, name: &str) -> bool {
    let name_tokens = tokens(name);
    if name_tokens.is_empty() {
        return false;
    }
    let first = name_tokens[0].as_str();
    if scene.dialogue.iter().any(|line| {
        let speaker = speaker_name(&line.speaker);
        let speaker_tokens = tokens(&speaker);
        speaker_tokens == name_tokens || speaker_tokens == vec![first.to_string()]
    }) {
        return true;
    }
    let focus = scene
        .intent
        .as_ref()
        .map(|intent| intent.character_focus.join(" "))
        .unwrap_or_default();
    let blob = format!(
        "{} {} {}",
        scene.location.as_deref().unwrap_or(""),
        scene.action.join(" "),
        focus
    );
    contains_sequence(&tokens(&blob), &name_tokens)
}

fn contains_sequence(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn find_character<'a>(
    characters: &'a [CharacterRecord],
    name: &str,
) -> Option<&'a CharacterRecord> {
    let folded = fold_alnum(name);
    if folded.is_empty() {
        return None;
    }
    characters
        .iter()
        .find(|character| fold_alnum(&character.name) == folded)
        .or_else(|| {
            characters.iter().find(|character| {
                let full = tokens(&character.name);
                let query = tokens(name);
                !query.is_empty()
                    && (full == query
                        || full.first().map(String::as_str) == query.first().map(String::as_str)
                            && query.len() == 1
                            && full.len() > 1)
            })
        })
}

fn speaker_name(raw: &str) -> String {
    raw.split(',')
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_end_matches(':')
        .to_string()
}

fn parse_headers(script_texts: &[String]) -> ScriptHeader {
    let mut header = ScriptHeader::default();
    for text in script_texts {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("## Scenes") || line.starts_with("### Scene") {
                break;
            }
            let Some((label, value)) = bold_field(line) else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match label.as_str() {
                "estimated length" | "runtime" | "length" => set_opt(&mut header.runtime, value),
                "format" => {
                    if header.aspect.is_none() {
                        header.aspect = crate::script::aspect_from_text(value);
                    }
                }
                "audience" => set_opt(&mut header.audience, value),
                "platforms" | "platform" => set_opt(&mut header.platforms, value),
                "resolution" => set_opt(&mut header.resolution, value),
                "frame rate" | "fps" => set_opt(&mut header.frame_rate, value),
                "language" => set_opt(&mut header.language, value),
                "medium" => set_opt(&mut header.medium, value),
                "style" | "style notes" | "style bible" | "vertical direction" => {
                    set_opt(&mut header.style_notes, value)
                }
                "sound palette" | "audio" | "audio plan" => set_opt(&mut header.audio_plan, value),
                "continuity" | "continuity rules" => set_opt(&mut header.continuity, value),
                "safety" | "safety constraints" | "content constraints" => {
                    set_opt(&mut header.safety, value)
                }
                "budget" => set_opt(&mut header.budget, value),
                "schedule" => set_opt(&mut header.schedule, value),
                "rights" | "rights notes" => set_opt(&mut header.rights, value),
                "characters" | "cast" => {
                    for name in split_list(Some(value)) {
                        if !header
                            .characters
                            .iter()
                            .any(|existing| fold_alnum(existing) == fold_alnum(&name))
                        {
                            header.characters.push(name);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    header
}

fn bold_field(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("**")?;
    let (label, value) = rest.split_once(":**")?;
    Some((
        label
            .trim()
            .trim_end_matches('*')
            .trim()
            .to_ascii_lowercase(),
        value.to_string(),
    ))
}

fn set_opt(slot: &mut Option<String>, value: &str) {
    if slot.is_none() {
        *slot = Some(value.to_string());
    }
}

fn split_list(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

fn merge_profile(previous: ProductionProfile, derived: ProductionProfile) -> ProductionProfile {
    ProductionProfile {
        recipe: previous.recipe,
        recipe_document: previous.recipe_document.or(derived.recipe_document),
        deliverables: merge_deliverables(previous.deliverables, derived.deliverables),
        audience: prefer(previous.audience, derived.audience),
        platforms: if previous.platforms.is_empty() {
            derived.platforms
        } else {
            previous.platforms
        },
        runtime: prefer(previous.runtime, derived.runtime),
        aspect_ratios: if previous.aspect_ratios.is_empty() {
            derived.aspect_ratios
        } else {
            previous.aspect_ratios
        },
        resolution: prefer(previous.resolution, derived.resolution),
        frame_rate: prefer(previous.frame_rate, derived.frame_rate),
        language: prefer(previous.language, derived.language),
        medium: prefer(previous.medium, derived.medium),
        style_notes: prefer(previous.style_notes, derived.style_notes),
        audio_plan: prefer(previous.audio_plan, derived.audio_plan),
        continuity_rules: if previous.continuity_rules.is_empty() {
            derived.continuity_rules
        } else {
            previous.continuity_rules
        },
        safety_constraints: if previous.safety_constraints.is_empty() {
            derived.safety_constraints
        } else {
            previous.safety_constraints
        },
        budget: prefer(previous.budget, derived.budget),
        schedule: prefer(previous.schedule, derived.schedule),
        rights_notes: prefer(previous.rights_notes, derived.rights_notes),
    }
}

fn merge_deliverables(
    previous: Vec<DeliverableRecord>,
    derived: Vec<DeliverableRecord>,
) -> Vec<DeliverableRecord> {
    if previous.is_empty() {
        return derived;
    }
    let mut out = Vec::new();
    for (index, mut old) in previous.into_iter().enumerate() {
        let fresh = derived
            .iter()
            .find(|item| item.id == old.id)
            .or_else(|| derived.get(index));
        if let Some(fresh) = fresh {
            if old.name.trim().is_empty() {
                old.name = fresh.name.clone();
            }
            old.audience = prefer(old.audience, fresh.audience.clone());
            if old.platforms.is_empty() {
                old.platforms = fresh.platforms.clone();
            }
            old.runtime = prefer(old.runtime, fresh.runtime.clone());
            old.aspect_ratio = prefer(old.aspect_ratio, fresh.aspect_ratio.clone());
            old.resolution = prefer(old.resolution, fresh.resolution.clone());
            old.frame_rate = prefer(old.frame_rate, fresh.frame_rate.clone());
            old.language = prefer(old.language, fresh.language.clone());
        }
        if old.id.trim().is_empty() {
            old.id = deliverable_id(&old.name);
        }
        out.push(old);
    }
    if !(out.len() == 1 && derived.len() == 1) {
        for fresh in derived {
            if !out.iter().any(|item| item.id == fresh.id) {
                out.push(fresh);
            }
        }
    }
    out
}

fn prefer(previous: Option<String>, derived: Option<String>) -> Option<String> {
    match previous {
        Some(value) if !value.trim().is_empty() => Some(value),
        _ => derived.filter(|value| !value.trim().is_empty()),
    }
}

fn slug_key(value: &str) -> String {
    let slug = crate::output_paths::slugify(value);
    if slug.is_empty() {
        "item".into()
    } else {
        slug
    }
}

fn fold_alnum(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn tokens(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn shorten(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::state::ProjectState;

    #[test]
    fn registry_ids_follow_content_not_display_order() {
        assert_eq!(
            requirement_id(RequirementKind::Location, "train_interior"),
            "req_location_train_interior"
        );
        assert_eq!(
            requirement_id(RequirementKind::Prop, "parcel"),
            "req_prop_parcel"
        );
        assert_ne!(
            requirement_id(RequirementKind::Cast, "tom_vale"),
            requirement_id(RequirementKind::Voice, "tom_vale")
        );
    }

    #[test]
    fn train_scene_breaks_down_cast_interior_parcel_and_voices() {
        let mut state = ProjectState::new();
        state.project_name = Some("Lite_Challenge".into());
        state.scripts.push(crate::workflow::state::ScriptRecord {
            path: "Scripts/story.md".into(),
            title: Some("The Lantern at Willowby Halt".into()),
            aspect_ratio: Some("9:16".into()),
        });
        state.characters.push(CharacterRecord {
            id: "tom_vale".into(),
            name: "Tom Vale".into(),
            dir: "Characters/Tom Vale".into(),
            identity_ready: true,
            voice_ready: true,
            profile_path: None,
            model_sheet_manifest_path: None,
            reference_asset_ids: Vec::new(),
            profile: Default::default(),
            asset_ids: Vec::new(),
            notes: Vec::new(),
        });
        let scene = state.scene_mut(1, "Scene 1 — The last train");
        scene.location = Some("A two-carriage night train approaching Willowby Halt.".into());
        scene
            .action
            .push("Tom Vale guards a brown paper parcel on his knees.".into());
        scene.dialogue.push(crate::script::DialogueLine {
            speaker: "Tom".into(),
            text: "We shall be there before supper.".into(),
        });
        scene.dialogue.push(crate::script::DialogueLine {
            speaker: "Guard, outside".into(),
            text: "Willowby Halt!".into(),
        });
        scene.sound.push("Train wheels clatter.".into());
        let script = r#"# The Lantern at Willowby Halt

**Estimated length:** Approximately 8 minutes
**Format:** Vertical 9:16
**Medium:** Clay
**Sound palette:** Rain and train wheels.
**Characters:** Tom Vale, Nell Vale

## Scenes
"#;
        apply(&mut state, &[script.to_string()]);
        let profile = state.profile.clone().expect("profile");
        assert_eq!(profile.recipe, ProductionRecipe::narrative());
        assert_eq!(profile.medium.as_deref(), Some("Clay"));
        assert_eq!(profile.deliverables[0].name, "The Lantern at Willowby Halt");
        assert_eq!(
            profile.deliverables[0].id,
            "del_the_lantern_at_willowby_halt"
        );
        assert_eq!(
            profile.deliverables[0].aspect_ratio.as_deref(),
            Some("9:16")
        );
        assert!(profile.runtime.as_deref().unwrap().contains('8'));

        let train = state
            .requirements
            .iter()
            .find(|req| req.id == "req_location_train_interior")
            .expect("train interior");
        assert_eq!(train.kind, RequirementKind::Location);
        assert_eq!(train.name, "Train interior");
        assert_eq!(train.scene_ids, vec![1]);
        assert_eq!(train.status, RequirementStatus::Draft);

        let parcel = state
            .requirements
            .iter()
            .find(|req| req.id == "req_prop_parcel")
            .expect("parcel");
        assert_eq!(parcel.scene_ids, vec![1]);

        let tom = state
            .requirements
            .iter()
            .find(|req| req.id == "req_cast_tom_vale")
            .expect("tom");
        assert_eq!(tom.status, RequirementStatus::Present);
        let tom_voice = state
            .requirements
            .iter()
            .find(|req| req.id == "req_voice_tom_vale")
            .expect("tom voice");
        assert_eq!(tom_voice.status, RequirementStatus::Present);
        let guard = state
            .requirements
            .iter()
            .find(|req| req.id == "req_voice_guard")
            .expect("guard voice");
        assert_eq!(guard.status, RequirementStatus::Missing);
        assert!(guard.blocks_scene());

        let blockers = state.blockers_for_scene(1);
        assert!(blockers
            .iter()
            .any(|req| req.id == "req_location_train_interior"));
        assert!(blockers.iter().any(|req| req.id == "req_prop_parcel"));
        assert!(blockers.iter().any(|req| req.id == "req_voice_guard"));
        assert!(blockers.iter().all(|req| req.blocks_scene()));
        assert!(blockers.iter().all(|req| req.id != "req_cast_tom_vale"));
    }
}

use crate::chat;
use crate::ctx::HostCtx;
use crate::settings;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Hard wall-clock budget for one Director invoke so the desk cannot freeze for
/// an entire 12-shot pack. The frontend / user continues with another turn.
const TURN_WALL_CLOCK: Duration = Duration::from_secs(110);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Settings API entry id (user-named instance).
    #[serde(alias = "providerId")]
    pub api_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DirectorContext {
    pub project_root: Option<String>,
    pub selected_path: Option<String>,
    pub focused_editor_path: Option<String>,
    pub focused_editor_excerpt: Option<String>,
    #[serde(default)]
    pub attached_paths: Vec<String>,
    /// Settings → Generation defaults (capability slot → model). Empty/null = unset.
    #[serde(default)]
    pub capability_defaults: std::collections::HashMap<String, Option<ModelRef>>,
    /// Host-owned API labels are only for approval-card disclosure.
    #[serde(default)]
    pub api_labels: std::collections::HashMap<String, String>,
    /// api id → provider kind (`ApiEntry.kind`), the data key the pricing
    /// registry resolves against. Kept separate from the display label so the
    /// cost layer stays API-agnostic.
    #[serde(default)]
    pub api_kinds: std::collections::HashMap<String, String>,
    /// Non-secret endpoint identity; approvals stale if a provider endpoint changes.
    #[serde(default)]
    pub api_fingerprints: std::collections::HashMap<String, u64>,
    /// The chat model used while planning, bound for approved specialists that need it.
    #[serde(default)]
    pub planner_model: Option<ModelRef>,
    /// Settings → Adult content enabled (slots configured / UI opt-in). Not sufficient alone to use spicy models.
    #[serde(default)]
    pub adult_content_enabled: bool,
    /// Settings → Adult generation defaults (spicy models). Unset slots fall back to capabilityDefaults.
    #[serde(default)]
    pub adult_capability_defaults: std::collections::HashMap<String, Option<ModelRef>>,
    /// Host-set for this turn when the user asks for adult/nsfw/spicy models (see looks_like_adult_routing_request).
    #[serde(default)]
    pub adult_routing_requested: bool,
    /// Production desk stage (`scene_intent`, `cut_sketches`, `production_notes`, …).
    #[serde(default)]
    pub workflow_stage: Option<String>,
    /// Production desk selected scene id.
    #[serde(default)]
    pub workflow_scene_id: Option<u32>,
    /// Stable scene id. Display numbers are labels.
    #[serde(default)]
    pub workflow_scene_stable_id: Option<String>,
    #[serde(default)]
    pub workflow_shot_stable_id: Option<String>,
    #[serde(default)]
    pub workflow_asset_id: Option<String>,
    #[serde(default)]
    pub workflow_look_slot_id: Option<String>,
    #[serde(default)]
    pub workflow_requirement_id: Option<String>,
    /// UI lens: `plan`, `produce`, or `finish`.
    #[serde(default)]
    pub workflow_work_area: Option<String>,
}

fn hydrate_context_from_app(app: &HostCtx, context: &mut DirectorContext) -> Result<(), String> {
    let settings_value = settings::load_settings(app.clone())?;
    let apis = settings::list_apis(app)?;
    hydrate_routing_context(context, &settings_value, &apis)
}

fn hydrate_routing_context(
    context: &mut DirectorContext,
    settings_value: &Value,
    apis: &[settings::ApiEntry],
) -> Result<(), String> {
    let enabled: std::collections::HashSet<&str> = apis
        .iter()
        .filter(|api| api.enabled)
        .map(|api| api.id.as_str())
        .collect();
    context.api_labels = apis
        .iter()
        .filter(|api| api.enabled)
        .map(|api| (api.id.clone(), format!("{} ({})", api.name, api.kind)))
        .collect();
    context.api_kinds = apis
        .iter()
        .filter(|api| api.enabled)
        .map(|api| (api.id.clone(), api.kind.clone()))
        .collect();
    context.api_fingerprints = apis
        .iter()
        .filter(|api| api.enabled)
        .map(|api| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            api.id.hash(&mut hasher);
            api.kind.hash(&mut hasher);
            api.base_url.hash(&mut hasher);
            (api.id.clone(), hasher.finish())
        })
        .collect();

    fn read_models(
        settings_value: &Value,
        key: &str,
        enabled: &std::collections::HashSet<&str>,
    ) -> Result<std::collections::HashMap<String, Option<ModelRef>>, String> {
        let mut result = std::collections::HashMap::new();
        let Some(object) = settings_value.get(key).and_then(Value::as_object) else {
            return Ok(result);
        };
        for (slot, raw) in object {
            if raw.is_null() {
                result.insert(slot.clone(), None);
                continue;
            }
            let model: ModelRef = serde_json::from_value(raw.clone())
                .map_err(|error| format!("Invalid model route in {key}.{slot}: {error}"))?;
            if !model.model_id.trim().is_empty() && enabled.contains(model.api_id.as_str()) {
                result.insert(slot.clone(), Some(model));
            } else {
                result.insert(slot.clone(), None);
            }
        }
        Ok(result)
    }

    context.capability_defaults = read_models(settings_value, "capabilityDefaults", &enabled)?;
    context.adult_capability_defaults =
        read_models(settings_value, "adultCapabilityDefaults", &enabled)?;
    context.adult_content_enabled = settings_value
        .get("adultContentEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    context.adult_routing_requested &= context.adult_content_enabled;
    Ok(())
}

fn routing_snapshot_matches(expected: &DirectorContext, current: &DirectorContext) -> bool {
    expected.capability_defaults == current.capability_defaults
        && expected.adult_capability_defaults == current.adult_capability_defaults
        && expected.adult_content_enabled == current.adult_content_enabled
        && expected.api_labels == current.api_labels
        && expected.api_fingerprints == current.api_fingerprints
}

fn specialist_needs_planner_llm(specialist: &str) -> bool {
    matches!(
        specialist,
        "production_helper"
            | "scene_plan"
            | "scene_intent"
            | "cut_sketches"
            | "scene_look"
            | "script"
            | "character_creation"
    )
}

fn approval_route_details(
    action: &crate::director_actions::DirectorAction,
    context: &DirectorContext,
    project_root: Option<&str>,
) -> (String, String, String) {
    let specialist = action.specialist.as_deref().unwrap_or("");
    if matches!(specialist, "assemble" | "text_write") {
        return (
            "Local Workbench".into(),
            "Local operation (no model)".into(),
            "No provider charge".into(),
        );
    }
    if action.inputs.contains_key("collect_prediction_id") {
        return (
            "Existing provider job".into(),
            "Resume/download only".into(),
            "No new generation charge expected".into(),
        );
    }
    let mut routes = crate::specialists::approval_model_routes(action, context, project_root);
    if specialist_needs_planner_llm(specialist) {
        if let Some(model) = context.planner_model.clone() {
            routes.push(("Director chat".into(), model));
        }
    }
    if routes.is_empty() {
        return (
            "Not resolved from host settings".into(),
            "No compatible model route configured".into(),
            "Unknown; no provider call until a route is configured".into(),
        );
    }
    let mut providers = Vec::new();
    let mut models = Vec::new();
    let mut known_cost: Option<String> = None;
    for (slot, model) in routes {
        if known_cost.is_none() {
            if let Some(kind) = context.api_kinds.get(&model.api_id) {
                known_cost = crate::workflow::cost::known_price_label(kind, specialist);
            }
        }
        let provider = context
            .api_labels
            .get(&model.api_id)
            .cloned()
            .unwrap_or_else(|| {
                if model.api_id.trim().is_empty() {
                    "Provider selected by default routing".into()
                } else {
                    format!("API {}", model.api_id)
                }
            });
        if !providers.contains(&provider) {
            providers.push(provider);
        }
        models.push(format!("{slot}: `{}`", model.model_id));
    }
    (
        providers.join(", "),
        models.join("; "),
        known_cost.unwrap_or_else(|| "Unknown; provider pricing applies".into()),
    )
}

pub(crate) fn approval_route_details_for_app(
    app: &HostCtx,
    action: &crate::director_actions::DirectorAction,
    project_root: Option<&str>,
) -> Result<(String, String, String), String> {
    let mut context = DirectorContext::default();
    context.project_root = project_root.map(str::to_string);
    hydrate_context_from_app(app, &mut context)?;
    Ok(approval_route_details(action, &context, project_root))
}

/// A resolved paid estimate for one specialist action.
pub struct ActionEstimate {
    pub kind: String,
    pub model: Option<String>,
    pub currency: String,
    pub minor: i64,
}

/// Resolve the first routed `(kind, model, price)` for a specialist action, or
/// `None` when there is no route or the price is unknown. Used to stamp a known
/// estimate onto a workflow job before generation. Unknown stays unknown.
pub fn estimate_for_action(
    app: &HostCtx,
    action: &crate::director_actions::DirectorAction,
    project_root: Option<&str>,
) -> Result<Option<ActionEstimate>, String> {
    let mut context = DirectorContext::default();
    context.project_root = project_root.map(str::to_string);
    hydrate_context_from_app(app, &mut context)?;
    let specialist = action.specialist.as_deref().unwrap_or("");
    let routes = crate::specialists::approval_model_routes(action, &context, project_root);
    for (_slot, model) in routes {
        if let Some(kind) = context.api_kinds.get(&model.api_id) {
            if let Some((currency, minor)) = crate::workflow::cost::known_price(kind, specialist) {
                return Ok(Some(ActionEstimate {
                    kind: kind.clone(),
                    model: Some(model.model_id.clone()),
                    currency,
                    minor,
                }));
            }
        }
    }
    Ok(None)
}

pub(crate) fn format_approval_request_for_app(
    app: &HostCtx,
    action: &crate::director_actions::DirectorAction,
    validated: &crate::director_actions::ValidatedAction,
    project_root: Option<&str>,
) -> Result<String, String> {
    let root = crate::session::command_root(project_root)?;
    let root = root.to_string_lossy().into_owned();
    let mut context = DirectorContext::default();
    context.project_root = Some(root.clone());
    hydrate_context_from_app(app, &mut context)?;
    let (provider, model, cost) = approval_route_details(action, &context, Some(&root));
    if provider == "Not resolved from host settings" {
        return Err(format!(
            "No compatible host provider/model route is configured for `{}`.",
            action.specialist.as_deref().unwrap_or("this action")
        ));
    }
    Ok(
        crate::director_actions::format_approval_request_with_context(
            action, validated, &context, &provider, &model, &cost,
        ),
    )
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorTurnRequest {
    pub model: String,
    /// Settings `apis[].id` for the chat model. Required for non-Atlas chat.
    #[serde(default)]
    pub api_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub context: DirectorContext,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectorTurnResponse {
    pub reply: String,
    pub model: String,
    pub tool_events: Vec<String>,
    /// True when the host stopped early so the UI could unfreeze; user should continue.
    pub continue_suggested: bool,
}

fn resolve_path(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    if crate::session::root().is_some() || !cfg!(test) {
        return crate::session::authorize(raw, crate::project_path::PathIntent::Read);
    }
    crate::project_path::resolve(raw, project_root, crate::project_path::PathIntent::Read)
        .map(|path| path.into_path_buf())
        .map_err(|error| error.to_string())
}

fn looks_like_text_path(path: &PathBuf) -> bool {
    const TEXT_EXTENSIONS: &[&str] = &[
        "md", "markdown", "txt", "yaml", "yml", "json", "toml", "csv", "ts", "tsx", "js", "jsx",
        "css", "html", "htm", "rs", "py", "sh", "xml", "svg", "env", "prompt",
    ];
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            TEXT_EXTENSIONS
                .iter()
                .any(|allowed| *allowed == ext.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

fn append_attached_file_context(
    lines: &mut Vec<String>,
    path_str: &str,
    project_root: Option<&str>,
) {
    let path = match resolve_path(path_str, project_root) {
        Ok(path) => path,
        Err(error) => {
            lines.push(format!("### Attached: {path_str}"));
            lines.push(format!("(not attached: {error})"));
            return;
        }
    };
    lines.push(format!("### Attached: {}", path.display()));
    if !path.exists() {
        lines.push("(path does not exist)".into());
        return;
    }
    if path.is_dir() {
        lines.push("(directory — use list_dir / read children as needed)".into());
        return;
    }
    if !looks_like_text_path(&path) {
        lines.push(
            "(binary/media path attached; do not invent contents — use tools only if appropriate)"
                .into(),
        );
        return;
    }
    match fs::read_to_string(&path) {
        Ok(text) => {
            const MAX: usize = 40_000;
            if text.len() > MAX {
                lines.push(format!(
                    "{}\n\n[truncated at {MAX} chars; use read_file for the remainder]",
                    &text[..MAX]
                ));
            } else {
                lines.push(text);
            }
        }
        Err(error) => lines.push(format!("(could not read: {error})")),
    }
}

fn looks_like_video_conditioned_image_model(model_id: &str) -> bool {
    let id = model_id.to_ascii_lowercase();
    id.contains("reference-to-image")
        || id.contains("video-to-image")
        || id.contains("video_to_image")
}

/// Meta questions about whether adult routing is active — not a request to enable it.
fn is_adult_routing_meta_question(lower: &str) -> bool {
    let asks_status = lower.contains("are you using")
        || lower.contains("did you use")
        || lower.contains("will you use")
        || lower.contains("are we using")
        || lower.contains("did we use");
    asks_status && (lower.contains("adult") || lower.contains("nsfw") || lower.contains("spicy"))
}

/// User asked to use Adult / NSFW generation defaults for this job.
/// Matches words like `adult`, `nsfw`, and phrases like `adult models` / `spicy models`.
pub fn looks_like_adult_routing_request(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if is_adult_routing_meta_question(&lower) {
        return false;
    }
    const PHRASES: &[&str] = &[
        "adult model",
        "adult models",
        "adult content",
        "adult default",
        "adult defaults",
        "nsfw model",
        "nsfw models",
        "spicy model",
        "spicy models",
        "use adult",
        "using adult",
        "use the adult",
        "using the adult",
        "use nsfw",
        "using nsfw",
        "with adult",
        "with nsfw",
        "with spicy",
    ];
    if PHRASES.iter().any(|phrase| lower.contains(phrase)) {
        return true;
    }
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| matches!(tok, "adult" | "nsfw"))
}

/// Scan recent user turns so “Approved” after an adult ask still keeps spicy defaults.
fn adult_routing_from_history(messages: &[ChatMessage]) -> bool {
    messages
        .iter()
        .rev()
        .filter(|m| m.role.trim().eq_ignore_ascii_case("user"))
        .take(8)
        .any(|m| looks_like_adult_routing_request(&m.content))
}

fn resolved_capability_model<'a>(context: &'a DirectorContext, slot: &str) -> Option<&'a ModelRef> {
    if context.adult_routing_requested {
        if let Some(Some(model)) = context.adult_capability_defaults.get(slot) {
            if !model.model_id.trim().is_empty() {
                return Some(model);
            }
        }
    }
    context
        .capability_defaults
        .get(slot)
        .and_then(|v| v.as_ref())
        .filter(|m| !m.model_id.trim().is_empty())
}

fn format_capability_defaults(context: &DirectorContext) -> Vec<String> {
    // Prefer the slots media specialists care about first; then any other set slots.
    const PRIORITY: &[&str] = &[
        "image.edit",
        "image.character_reference",
        "image.text_to_image",
        "image.image_to_image",
        "video.edit",
        "video.start_frame",
        "video.character_reference",
        "video.text_only",
        "video.location_prop_other_reference",
        "video.lip_sync_text_narration",
        "video.lip_sync_audio_included",
        "audio.tts_clone",
        "audio.tts_profile",
    ];
    let mut lines = vec![if context.adult_routing_requested {
        "- Generation defaults from Settings (adult/nsfw routing ON for this turn — adult slot first, else main default):"
                .to_string()
    } else {
        "- Generation defaults from Settings (use these model IDs unless the user or handoff overrides):"
                .to_string()
    }];
    if context.adult_content_enabled && !context.adult_routing_requested {
        lines.push(
            "  Adult content defaults are configured in Settings. The user must say **adult** or **nsfw** (e.g. “generate with adult models”) to use them on a turn."
                .into(),
        );
    }
    let mut seen = std::collections::HashSet::new();
    let mut any = false;
    for slot in PRIORITY {
        seen.insert(*slot);
        match resolved_capability_model(context, slot) {
            Some(model) => {
                any = true;
                let id = model.model_id.trim();
                let api = if model.api_id.trim().is_empty() {
                    "default"
                } else {
                    model.api_id.trim()
                };
                let source = if context.adult_routing_requested
                    && context
                        .adult_capability_defaults
                        .get(*slot)
                        .and_then(|v| v.as_ref())
                        .is_some_and(|m| !m.model_id.trim().is_empty())
                {
                    "adult"
                } else {
                    "main"
                };
                lines.push(format!("  - `{slot}` → `{id}` (api: {api}, {source})"));
                if looks_like_video_conditioned_image_model(id) {
                    lines.push(
                        "    ⚠ Unsuitable for *still* model sheets / still character refs: this ID is typically video→image (requires video_clips). For text-only packs use `image.text_to_image`. For still refs pick a still edit / image-to-image model in Settings (e.g. an `*/edit` model)."
                            .into(),
                    );
                }
            }
            None => {
                lines.push(format!("  - `{slot}` → (not set in Settings)"));
            }
        }
    }
    for (slot, value) in &context.capability_defaults {
        if seen.contains(slot.as_str()) {
            continue;
        }
        if let Some(model) = value {
            if model.model_id.trim().is_empty() {
                continue;
            }
            any = true;
            lines.push(format!(
                "  - `{slot}` → `{}` (api: {})",
                model.model_id.trim(),
                if model.api_id.trim().is_empty() {
                    "default"
                } else {
                    model.api_id.trim()
                }
            ));
        }
    }
    if !any {
        lines.push(
            "  (No generation defaults set — for Model Sheet / Still Image / Scene Generation, ask the user to set Generation defaults in Settings, or to name model IDs explicitly.)"
                .into(),
        );
    }
    lines.push(
        "  These ids are Settings capability slots (model defaults). They are NOT specialist names — specialists stay `still_image`, `scene_generation`, `model_sheet`, etc."
            .into(),
    );
    lines.push(
        "  Do not marketplace-browse or invent a different model family when a Settings default exists for that capability."
            .into(),
    );
    lines.push(
        "  Host maps Still Image inputs → slots: focused edit (existing source_frame) → `image.edit` (fallback `image.image_to_image`); text-only new still → `image.text_to_image`; still/revision refs → `image.image_to_image`; character-in-scene → `image.character_reference`."
            .into(),
    );
    lines.push(
        "  Host maps Scene Generation → slots: focused edit → `video.edit`; freeform start-frame → `video.start_frame`; freeform refs → `video.character_reference` / `video.location_prop_other_reference`; freeform text-only prompt → `video.text_only` (no handoff required). Handoff YAML still uses the same slots by source_mode/refs. Audio: clone → `audio.tts_clone`; profile → YAML model_id or `audio.tts_profile`."
            .into(),
    );
    lines.push(
        "  Host maps Text-Sync → `video.lip_sync_text_narration` (fallback `video.start_frame`) for Van/Wan image-to-video + audio; TTS via `audio.tts_clone` / `audio.tts_profile`. Sync Labs post lip-sync stays on `lip_sync` + `video.lip_sync_audio_included`."
            .into(),
    );
    lines
}

fn append_workflow_scope(lines: &mut Vec<String>, context: &DirectorContext) {
    let scope = crate::workflow::scope::DirectorScope {
        work_area: context.workflow_work_area.clone(),
        scene_stable_id: context.workflow_scene_stable_id.clone(),
        shot_stable_id: context.workflow_shot_stable_id.clone(),
        asset_id: context.workflow_asset_id.clone(),
        look_slot_id: context.workflow_look_slot_id.clone(),
        requirement_id: context.workflow_requirement_id.clone(),
    };
    if scope.is_empty() {
        return;
    }
    let state = context
        .project_root
        .as_deref()
        .filter(|root| !root.is_empty())
        .and_then(|root| crate::workflow::import::load_state(std::path::Path::new(root)).ok());
    lines.push(crate::workflow::scope::describe(state.as_ref(), &scope));
}

fn context_system_message(context: &DirectorContext) -> String {
    let project_root = context.project_root.as_deref().filter(|s| !s.is_empty());
    let mut lines = vec![
        "Context packet for this turn (attached by the workbench; treat as ground truth):"
            .to_string(),
        format!("- Project root: {}", project_root.unwrap_or("(none open)")),
        format!(
            "- Selected path: {}",
            context
                .selected_path
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("(none)")
        ),
        format!(
            "- Focused editor file: {}",
            context
                .focused_editor_path
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("(none)")
        ),
        "- Wired specialists (emit run_specialist; host executes): script, character_creation, scene_intent, scene_look, cut_sketches, scene_plan (chat-model text runners → local files), model_sheet, still_image, scene_generation, text_sync, lip_sync, assemble, production_helper. Route by intent. Text planning uses the selected chat model (commercial or local); generation spend uses Settings / Adult slots when requested.".to_string(),
        format!(
            "- Workflow stage: {}",
            context
                .workflow_stage
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("(none)")
        ),
        format!(
            "- Workflow scene: {}",
            context
                .workflow_scene_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "(none)".to_string())
        ),
        "- Scene intent is not production notes. Scene look is not character identity. Existing prod.md / Scene_Prompts coverage does not complete those stages. Route “draft the scene intent” to scene_intent and “draft the scene look” to scene_look, never scene_plan.".to_string(),
    ];
    append_workflow_scope(&mut lines, context);
    lines.extend(format_capability_defaults(context));
    if !context.attached_paths.is_empty() {
        lines.push(format!(
            "- Chat attachments ({}): user explicitly added these — prefer them over guessing paths.",
            context.attached_paths.len()
        ));
        for path in &context.attached_paths {
            if path.trim().is_empty() {
                continue;
            }
            append_attached_file_context(&mut lines, path, project_root);
        }
    }
    if let Some(excerpt) = context
        .focused_editor_excerpt
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        let clipped = if excerpt.len() > 12_000 {
            format!(
                "{}…\n\n[excerpt truncated at 12000 chars; attach the file or use inspect if you need more]",
                &excerpt[..12_000]
            )
        } else {
            excerpt.to_string()
        };
        lines.push("- Focused editor excerpt:".to_string());
        lines.push(clipped);
    }
    lines.join("\n")
}

/// User asked to generate / send a scene (counts as intent to spend once the handoff is known).
fn looks_like_generate_scene_request(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("do not generate")
        || lower.contains("don't generate")
        || lower.contains("not yet")
    {
        return false;
    }
    let phrases = [
        "video generation",
        "generate this scene",
        "generate the scene",
        "send the attached scene",
        "send this scene",
        "send the scene",
        "for video generation",
        "run this scene",
        "generate video",
        "make the video",
        "create the video",
        "render this scene",
        "render the scene",
    ];
    phrases.iter().any(|p| lower.contains(p))
}

/// Clarifying question about a pending approval plan (model, slot, paths) — not an approve/reject.
fn looks_like_pending_plan_question(text: &str) -> bool {
    let trimmed = text.trim().to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.len() > 240 {
        return false;
    }
    if looks_like_paid_approval(text) || looks_like_continue_request(text) {
        return false;
    }
    let asks_model = trimmed.contains("model")
        || trimmed.contains("sending this to")
        || trimmed.contains("send this to")
        || trimmed.contains("which slot")
        || trimmed.contains("what slot")
        || trimmed.contains("capability");
    let question_shape = trimmed.contains('?')
        || trimmed.starts_with("what ")
        || trimmed.starts_with("which ")
        || trimmed.starts_with("where ")
        || trimmed.starts_with("how ");
    asks_model && question_shape
}

fn answer_pending_plan_question(
    context: &DirectorContext,
    pending: &crate::director_actions::DirectorAction,
    user_text: &str,
    project_root: Option<&str>,
) -> Option<String> {
    let specialist = pending.specialist.as_deref()?;
    let notes = match specialist {
        "still_image" => {
            crate::specialists::still_image_approval_notes(context, &pending.inputs, project_root)
        }
        "scene_generation" => crate::specialists::scene_generation_approval_notes(
            context,
            &pending.inputs,
            project_root,
        ),
        "text_sync" => {
            crate::specialists::text_sync_approval_notes(context, &pending.inputs, project_root)
        }
        "model_sheet" => {
            crate::specialists::model_sheet_approval_notes(&pending.inputs, project_root)
        }
        "assemble" => crate::assemble::approval_notes(&pending.inputs, project_root),
        _ => return None,
    };
    let reply = format!(
        "Host answer (from Settings / the parked plan) — for: _{}_\n\n{notes}\n\nThe Approve button on the plan still runs it. Chat text does not.",
        user_text.trim()
    );
    Some(reply)
}

fn should_keep_pending_for_chat_turn(text: &str) -> bool {
    looks_like_pending_plan_question(text)
}

/// User message that means paid-batch approval was just given.
fn looks_like_paid_approval(text: &str) -> bool {
    let trimmed = text.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return false;
    }
    if looks_like_generate_scene_request(trimmed.as_str()) {
        return true;
    }
    // Short approvals
    if matches!(
        trimmed.as_str(),
        "approved"
            | "approve"
            | "yes"
            | "y"
            | "ok"
            | "okay"
            | "go"
            | "go ahead"
            | "do it"
            | "run it"
            | "generate"
            | "send it"
            | "send the closeup"
            | "yes, generate"
            | "yes generate"
    ) {
        return true;
    }
    let phrases = [
        "approved",
        "go ahead",
        "send it",
        "send the closeup",
        "run it",
        "do it",
        "generate it",
        "generate the",
        "yes, generate",
        "yes generate",
        "approve this",
        "i approve",
    ];
    phrases.iter().any(|phrase| trimmed.contains(phrase))
        && !trimmed.contains("do not generate")
        && !trimmed.contains("don't generate")
        && !trimmed.contains("not yet")
}

fn last_user_message(messages: &[ChatMessage]) -> Option<&str> {
    messages
        .iter()
        .rev()
        .find(|message| message.role.trim() == "user")
        .map(|message| message.content.as_str())
}

fn looks_like_continue_request(text: &str) -> bool {
    let trimmed = text.trim().to_ascii_lowercase();
    trimmed == "continue"
        || trimmed == "cont"
        || trimmed == "resume"
        || trimmed.starts_with("continue ")
        || trimmed.starts_with("resume ")
}

fn director_turn_inner(
    app: HostCtx,
    request: DirectorTurnRequest,
) -> Result<DirectorTurnResponse, String> {
    use crate::director_actions::{
        format_approval_request_with_context, parse_actions, planner_system_prompt,
        validate_action, ActionKind,
    };
    use crate::specialists::{self, specialist_requires_approval, SpecialistLlm};

    let model = request.model.trim().to_string();
    if model.is_empty() {
        return Err("Select a chat model in the chat dropdown (or Settings) first.".into());
    }
    if request.messages.is_empty() {
        return Err("No messages to send.".into());
    }

    let chat_api_id = request
        .api_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            settings::list_apis(&app).ok().and_then(|apis| {
                apis.into_iter()
                    .find(|api| {
                        api.enabled
                            && matches!(
                                api.kind.as_str(),
                                "atlas-cloud"
                                    | "openai-chat"
                                    | "anthropic-messages"
                                    | "gemini"
                                    | "ollama"
                            )
                    })
                    .map(|api| api.id)
            })
        })
        .ok_or_else(|| {
            "Select a chat model (with an API) in the chat dropdown first.".to_string()
        })?;

    let (chat_api, chat_key) = settings::api_key_for(&app, &chat_api_id)?;
    let user_question = last_user_message(&request.messages)
        .unwrap_or("")
        .to_string();
    let specialist_llm = SpecialistLlm {
        api: &chat_api,
        api_key: &chat_key,
        model: &model,
        user_question: &user_question,
    };
    let mut context = request.context;
    context.project_root = crate::session::root().map(|path| path.to_string_lossy().into_owned());
    // Renderer-supplied model maps are untrusted; resolve routing only from host settings.
    context.adult_routing_requested = adult_routing_from_history(&request.messages);
    hydrate_context_from_app(&app, &mut context)?;
    context.planner_model = Some(ModelRef {
        api_id: chat_api_id.clone(),
        model_id: model.clone(),
    });
    let project_root = context
        .project_root
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let project_root_ref = project_root.as_deref();

    let mut tool_events = Vec::new();

    if context.adult_routing_requested {
        app.emit(
            "director-progress",
            "Adult/NSFW routing active — using Adult generation defaults when set…".to_string(),
        );
        tool_events.push("adult_routing:on".into());
    }

    // A question about the parked plan keeps that plan. Any other chat text,
    // including "ok" and "continue", drops it. Only director_approve runs it.
    if let Some(user_text) = last_user_message(&request.messages) {
        if should_keep_pending_for_chat_turn(user_text) {
            if let Some(pending) = crate::pending::current() {
                if let Some(reply) =
                    answer_pending_plan_question(&context, &pending, user_text, project_root_ref)
                {
                    tool_events.push("pending_plan_question".into());
                    return Ok(DirectorTurnResponse {
                        reply,
                        model,
                        tool_events,
                        continue_suggested: false,
                    });
                }
            }
        }
    }
    crate::pending::cancel_all();

    let system = format!(
        "{}\n\n{}",
        planner_system_prompt(),
        context_system_message(&context)
    );

    let mut api_messages: Vec<Value> = Vec::new();
    api_messages.push(json!({
        "role": "system",
        "content": system,
    }));
    for message in &request.messages {
        let role = message.role.trim();
        if role != "user" && role != "assistant" {
            continue;
        }
        api_messages.push(json!({
            "role": role,
            "content": crate::pending::strip_control_text(&message.content),
        }));
    }

    let mut final_reply = String::new();
    let mut continue_suggested = false;
    let turn_started = Instant::now();
    let mut format_nudge_sent = false;
    const MAX_ACTION_ROUNDS: usize = 12;

    for _round in 0..MAX_ACTION_ROUNDS {
        if turn_started.elapsed() >= TURN_WALL_CLOCK {
            continue_suggested = true;
            final_reply = "Paused to keep the desk responsive. Say **continue** to resume.".into();
            break;
        }

        app.emit("director-progress", "Planner thinking…".to_string());
        let content = chat::chat_completion(&chat_api, &chat_key, &model, &api_messages)?;
        api_messages.push(json!({
            "role": "assistant",
            "content": content,
        }));

        let actions = match parse_actions(&content) {
            Ok(actions) => actions,
            Err(error) => {
                // Pure prose with no action — treat as done if it looks like a normal reply.
                let trimmed = content.trim();
                if !trimmed.is_empty()
                    && !trimmed.contains("tool_call")
                    && !format_nudge_sent
                    && trimmed.len() < 4000
                    && !looks_like_generate_scene_request(trimmed)
                {
                    // One nudge to emit an action when the user clearly wanted work done.
                    if last_user_message(&request.messages)
                        .is_some_and(|t| looks_like_generate_scene_request(t) || t.len() > 20)
                    {
                        format_nudge_sent = true;
                        api_messages.push(json!({
                            "role": "user",
                            "content": format!(
                                "Workbench host: {error}. Emit a <director_action> JSON block now. Do not dump handoffs or invent tool results."
                            ),
                        }));
                        continue;
                    }
                    final_reply = trimmed.to_string();
                    break;
                }
                if !format_nudge_sent {
                    format_nudge_sent = true;
                    api_messages.push(json!({
                        "role": "user",
                        "content": format!(
                            "Workbench host: {error}\n\nEmit exactly:\n\n<director_action>\n{{\"action\":\"inspect\",\"op\":\"read_file\",\"path\":\"…\"}}\n</director_action>\n\nor run_specialist / ask_user / done."
                        ),
                    }));
                    continue;
                }
                final_reply = format!(
                    "Planner did not emit a valid director action.\n\n{error}\n\nLast reply:\n{content}"
                );
                break;
            }
        };

        let mut result_chunks = Vec::new();
        let mut stop_after = false;

        for action in actions {
            let validated = match validate_action(action, project_root_ref) {
                Ok(v) => v,
                Err(error) => {
                    result_chunks.push(format!("### Action rejected\n{error}"));
                    continue;
                }
            };

            match validated.action.action {
                ActionKind::AskUser | ActionKind::Done => {
                    final_reply = validated
                        .action
                        .message
                        .clone()
                        .unwrap_or_else(|| content.clone());
                    stop_after = true;
                    break;
                }
                ActionKind::Inspect => {
                    app.emit("director-progress", "Inspecting project path…".to_string());
                    match specialists::run_inspect(&validated, project_root_ref) {
                        Ok(text) => {
                            tool_events.push(format!(
                                "inspect:{}",
                                validated.action.op.as_deref().unwrap_or("read_file")
                            ));
                            result_chunks.push(format!(
                                "### Inspect ({})\n{}",
                                validated.action.op.as_deref().unwrap_or("read_file"),
                                text
                            ));
                        }
                        Err(error) => result_chunks.push(format!("### Inspect error\n{error}")),
                    }
                }
                ActionKind::RunSpecialist => {
                    let specialist = validated
                        .action
                        .specialist
                        .clone()
                        .unwrap_or_else(|| "unknown".into());
                    // Host-enforced gate. The planner cannot waive approval.
                    // Billable specialists always gate; assemble is local ffmpeg
                    // but still plan-gated.
                    let needs_approval = specialist_requires_approval(&specialist);
                    if needs_approval {
                        // Budget gate: check before creating the approval card.
                        // Blocked = hard stop (no card).  Confirm / Warn = add
                        // banner to the card so the user sees the budget status
                        // before approving.
                        //
                        // FAIL CLOSED: if project state cannot be loaded and
                        // routes are payable, treat as Blocked.  Paid operations
                        // must never proceed when budget state is unverifiable.
                        let mut budget_gate = crate::workflow::cost::SpendGate::NoLimit;
                        if let Some(root) = project_root_ref {
                            match crate::workflow::import::load_state(std::path::Path::new(root)) {
                                Ok(state) => {
                                    budget_gate = worst_gate_for_routes(
                                        &state,
                                        &validated,
                                        &context,
                                        project_root_ref,
                                    );
                                }
                                Err(_) => {
                                    // State unreadable — fail closed.
                                    // We cannot verify whether the
                                    // operation is payable, so block.
                                    budget_gate = crate::workflow::cost::SpendGate::Blocked;
                                }
                            }
                        }
                        if matches!(budget_gate, crate::workflow::cost::SpendGate::Blocked) {
                            final_reply =
                                "🚫 Budget hard stop: this operation would exceed the project budget limit. Adjust the budget or the operation before retrying."
                                    .into();
                            tool_events.push("budget_blocked".into());
                            stop_after = true;
                            break;
                        }
                        let (provider, routed_model, cost) =
                            approval_route_details(&validated.action, &context, project_root_ref);
                        let mut approval = format_approval_request_with_context(
                            &validated.action,
                            &validated,
                            &context,
                            &provider,
                            &routed_model,
                            &cost,
                        );
                        if let Some(banner) = budget_gate_banner(&budget_gate) {
                            approval = format!("{banner}\n\n{approval}");
                        }
                        if specialist == "model_sheet" {
                            approval.push_str("\n\n");
                            approval.push_str(&specialists::model_sheet_approval_notes(
                                &validated.action.inputs,
                                project_root_ref,
                            ));
                        } else if specialist == "scene_generation" {
                            approval.push_str("\n\n");
                            approval.push_str(&specialists::scene_generation_approval_notes(
                                &context,
                                &validated.action.inputs,
                                project_root_ref,
                            ));
                        } else if specialist == "text_sync" {
                            approval.push_str("\n\n");
                            approval.push_str(&specialists::text_sync_approval_notes(
                                &context,
                                &validated.action.inputs,
                                project_root_ref,
                            ));
                        } else if specialist == "assemble" {
                            approval.push_str("\n\n");
                            approval.push_str(&crate::assemble::approval_notes(
                                &validated.action.inputs,
                                project_root_ref,
                            ));
                        } else if specialist == "still_image" {
                            approval.push_str("\n\n");
                            approval.push_str(&specialists::still_image_approval_notes(
                                &context,
                                &validated.action.inputs,
                                project_root_ref,
                            ));
                        }
                        final_reply = approval;
                        tool_events.push(format!("approval_required:{specialist}"));
                        stop_after = true;
                        break;
                    }
                    match specialists::run_specialist(
                        &app,
                        &context,
                        &validated,
                        project_root_ref,
                        Some(&specialist_llm),
                    ) {
                        Ok(result) => {
                            tool_events.push(format!("specialist:{specialist}"));
                            final_reply = result.message;
                            continue_suggested = result.continue_suggested;
                            stop_after = true;
                            break;
                        }
                        Err(error) => {
                            result_chunks
                                .push(format!("### Specialist `{specialist}` error\n{error}"));
                        }
                    }
                }
            }
        }

        if stop_after {
            break;
        }
        if result_chunks.is_empty() {
            final_reply = if content.trim().is_empty() {
                "No actionable director action produced.".into()
            } else {
                content
            };
            break;
        }

        api_messages.push(json!({
            "role": "user",
            "content": format!(
                "Workbench host results (authoritative — do not invent different results):\n\n{}\n\nContinue with the next <director_action> (run_specialist / ask_user / done).",
                result_chunks.join("\n\n")
            ),
        }));
    }

    if final_reply.is_empty() {
        final_reply =
            "Stopped without a final reply. Try again with a clearer request or say **continue**."
                .into();
        continue_suggested = true;
    }

    Ok(DirectorTurnResponse {
        reply: final_reply,
        model,
        tool_events,
        continue_suggested,
    })
}

/// Run a director turn. Progress events stream on the host stdout event channel.
pub fn director_turn(
    app: HostCtx,
    request: DirectorTurnRequest,
) -> Result<DirectorTurnResponse, String> {
    app.emit("director-progress", "Director turn started…".to_string());
    finish_director_reply(app.clone(), director_turn_inner(app, request))
}

/// Gate the aggregate known estimate across all billable routes in an action.
/// Unknown-cost routes remain unresolved and require separate preflight policy.
fn worst_gate_for_routes(
    state: &crate::workflow::state::ProjectState,
    action: &crate::director_actions::ValidatedAction,
    context: &DirectorContext,
    project_root: Option<&str>,
) -> crate::workflow::cost::SpendGate {
    let routes = crate::specialists::approval_model_routes(&action.action, context, project_root);
    let specialist = action.action.specialist.as_deref().unwrap_or("");
    aggregate_route_gate(
        state,
        routes.into_iter().filter_map(|(_slot, model)| {
            let kind = context.api_kinds.get(&model.api_id)?;
            crate::workflow::cost::known_spend_minor(kind, specialist)
        }),
    )
}

fn aggregate_route_gate(
    state: &crate::workflow::state::ProjectState,
    estimates: impl IntoIterator<Item = i64>,
) -> crate::workflow::cost::SpendGate {
    use crate::workflow::cost::SpendGate;
    let mut total = 0i64;
    for minor in estimates {
        match total.checked_add(minor) {
            Some(sum) if minor >= 0 => total = sum,
            _ => return SpendGate::Blocked,
        }
    }
    crate::workflow::cost::gate_spend(state, total)
}

/// Format a budget-gate status banner for injection into the approval card text.
fn budget_gate_banner(gate: &crate::workflow::cost::SpendGate) -> Option<&'static str> {
    use crate::workflow::cost::SpendGate;
    match gate {
        SpendGate::NoLimit | SpendGate::Allow => None,
        SpendGate::Warn => Some("💡 Budget: your spend is approaching the warning threshold. "),
        SpendGate::Confirm => {
            Some("⚠️  Budget: this operation pushes you near or over the monthly limit. ")
        }
        SpendGate::Blocked => {
            Some("🚫 Budget: this operation would EXCEED the project budget limit. ")
        }
    }
}

fn director_budget_status(ledger: &crate::workflow::cost::BudgetLedger) -> String {
    let accounted = ledger
        .committed_minor
        .saturating_add(ledger.unsettled_estimated_minor)
        .saturating_add(ledger.completed_minor)
        .saturating_add(ledger.failed_minor);
    let limit = ledger
        .limit_minor
        .map(|value| {
            format!(
                "Configured project limit: {}.",
                crate::workflow::cost::format_minor(value, &ledger.currency)
            )
        })
        .unwrap_or_else(|| "No configured project limit.".into());
    format!(
        "Budget status: {:?} — {} accounted exposure. {limit} {} job(s) have unknown cost; {} job(s) have unverified maximum exposure. Provider charges are not verified.",
        ledger.gate,
        crate::workflow::cost::format_minor(accounted, &ledger.currency),
        ledger.unknown_cost_jobs,
        ledger.unknown_exposure_jobs,
    )
}

fn approved_action_error_reply(error: &str) -> (String, &'static str) {
    if matches!(
        error,
        "unbounded request approval required" | "budget confirmation required; generation blocked"
    ) {
        return (
            "Paid media is awaiting spend authorization. Approve or decline the separate confirmation prompt. If authorized, the submitted operation and recovery status will appear in Jobs; the Director will not automatically retry it.".into(),
            "approve:awaiting-spend-authorization",
        );
    }
    (
        format!("Host could not execute the approved action: {error}"),
        "approve:error",
    )
}

pub fn director_approve(app: HostCtx, id: String) -> Result<DirectorTurnResponse, String> {
    use crate::director_actions::{validate_action, ActionKind};
    use crate::specialists::{self, SpecialistLlm};

    let pending = crate::pending::take(id.trim())?;
    let current_root = crate::session::root();
    if pending.project_root != current_root {
        return Err(
            "The project changed after this approval was issued. Request a new plan.".into(),
        );
    }
    let project_root = pending
        .project_root
        .as_ref()
        .map(|root| root.to_string_lossy().into_owned());
    let project_root_ref = project_root.as_deref();
    let has_snapshot = pending.context.is_some();
    let mut context = match pending.context {
        Some(value) => serde_json::from_value::<DirectorContext>(value)
            .map_err(|error| format!("Stored approval context is invalid: {error}"))?,
        None => {
            let mut context = DirectorContext::default();
            hydrate_context_from_app(&app, &mut context)?;
            context
        }
    };
    if has_snapshot {
        let mut current_routing = DirectorContext::default();
        hydrate_context_from_app(&app, &mut current_routing)?;
        if !routing_snapshot_matches(&context, &current_routing) {
            return Err(
                "Provider or model settings changed after approval was issued. Request a new plan."
                    .into(),
            );
        }
    }
    context.project_root = project_root.clone();
    let mut validated =
        validate_action(pending.action, project_root_ref).map_err(|error| error.to_string())?;
    crate::text_writes::append_staged_paths_for_action(&mut validated)?;
    crate::director_actions::append_context_attachments(
        &mut validated,
        &context.attached_paths,
        project_root_ref,
    )?;
    if let Some(expected) = pending.input_fingerprints.as_ref() {
        let current = crate::director_actions::snapshot_input_files(&validated)?;
        if expected != &current {
            return Err("An approved input file changed after the approval card was issued. Request a new plan.".into());
        }
    }

    let specialist = validated.action.specialist.as_deref().unwrap_or("");
    let planner_llm = if specialist_needs_planner_llm(specialist) {
        let planner_model = context.planner_model.as_ref().ok_or_else(|| {
            "This approval needs the chat model used to plan it; request a new plan.".to_string()
        })?;
        let apis = settings::list_apis(&app)?;
        if !apis
            .iter()
            .any(|api| api.enabled && api.id == planner_model.api_id)
        {
            return Err("The chat provider used to plan this action is no longer enabled. Request a new plan.".into());
        }
        let (api, key) = settings::api_key_for(&app, &planner_model.api_id)?;
        Some((api, key, planner_model.model_id.clone()))
    } else {
        None
    };
    let response_model = context
        .planner_model
        .as_ref()
        .map(|model| model.model_id.clone())
        .unwrap_or_else(|| "approved action".into());

    app.emit(
        "director-progress",
        "Host executing approved action…".to_string(),
    );

    // Emit current budget gate status so the frontend can display it.
    if let Some(root) = project_root_ref {
        if let Ok(state) = crate::workflow::import::load_state(std::path::Path::new(root)) {
            let ledger = crate::workflow::cost::compute_ledger(&state);
            app.emit("director-progress", director_budget_status(&ledger));
        }
    }

    // Budget gate: veto a strict hard-stop request before any paid provider
    // call. Unknown-cost operations are also vetoed under strict policy because
    // v1 cannot prove a provider-backed maximum. This is the one path the
    // Director cannot bypass.
    //
    // FAIL CLOSED: if project state cannot be loaded, paid operations are
    // blocked — we must be able to verify budget state before spending money.
    if validated.action.action == ActionKind::RunSpecialist {
        let gate = if let Some(root) = project_root_ref {
            match crate::workflow::import::load_state(std::path::Path::new(root)) {
                Ok(state) => worst_gate_for_routes(&state, &validated, &context, project_root_ref),
                Err(_) => crate::workflow::cost::SpendGate::Blocked,
            }
        } else {
            crate::workflow::cost::SpendGate::Blocked
        };
        match gate {
            crate::workflow::cost::SpendGate::Blocked => {
                return Err(
                    "Budget hard stop: this operation would exceed the project budget limit. Adjust the budget or the operation before retrying."
                        .into(),
                );
            }
            crate::workflow::cost::SpendGate::Confirm => {
                app.emit(
                    "director-progress",
                    "⚠️ Budget warning: this operation pushes you near or over the monthly limit. You approved the approval card with this warning — proceeding.".to_string(),
                );
            }
            crate::workflow::cost::SpendGate::Warn => {
                app.emit(
                    "director-progress",
                    "ℹ️ Budget notice: your spend is approaching the warning threshold."
                        .to_string(),
                );
            }
            _ => {}
        }
    }

    let outcome = match validated.action.action {
        ActionKind::RunSpecialist => {
            let specialist_llm = planner_llm.as_ref().map(|(api, key, model)| SpecialistLlm {
                api,
                api_key: key,
                model,
                user_question: "",
            });
            specialists::run_specialist(
                &app,
                &context,
                &validated,
                project_root_ref,
                specialist_llm.as_ref(),
            )
        }
        ActionKind::Inspect => specialists::run_inspect(&validated, project_root_ref).map(|text| {
            specialists::SpecialistResult {
                message: text,
                continue_suggested: false,
            }
        }),
        ActionKind::AskUser | ActionKind::Done => Ok(specialists::SpecialistResult {
            message: validated.action.message.clone().unwrap_or_default(),
            continue_suggested: false,
        }),
    };
    let result = match outcome {
        Ok(result) => result,
        Err(error) => {
            let (reply, event) = approved_action_error_reply(&error);
            return finish_director_reply(
                app,
                Ok(DirectorTurnResponse {
                    reply,
                    model: response_model,
                    tool_events: vec![event.into()],
                    continue_suggested: false,
                }),
            );
        }
    };
    finish_director_reply(
        app,
        Ok(DirectorTurnResponse {
            reply: result.message,
            model: response_model,
            tool_events: vec![format!(
                "approve:{}",
                validated
                    .action
                    .specialist
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", validated.action.action))
            )],
            continue_suggested: result.continue_suggested,
        }),
    )
}

fn finish_director_reply(
    app: HostCtx,
    response: Result<DirectorTurnResponse, String>,
) -> Result<DirectorTurnResponse, String> {
    let mut response = response?;
    response.reply = crate::pending::publish_user_text(&response.reply);
    response.reply = crate::secrets::redact_text(&app, &response.reply);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::director_actions::{parse_actions, validate_action, ActionKind};

    #[test]
    fn deferred_paid_media_is_reported_as_awaiting_authorization_not_failure() {
        for error in [
            "unbounded request approval required",
            "budget confirmation required; generation blocked",
        ] {
            let (reply, event) = approved_action_error_reply(error);
            assert!(reply.contains("awaiting spend authorization"), "{reply}");
            assert!(reply.contains("Jobs"), "{reply}");
            assert!(!reply.contains("could not execute"), "{reply}");
            assert_eq!(event, "approve:awaiting-spend-authorization");
        }
    }

    #[test]
    fn budget_status_describes_accounted_exposure_without_claiming_verified_spend() {
        let ledger = crate::workflow::cost::BudgetLedger {
            currency: "USD".into(),
            limit_minor: None,
            warning_at_minor: None,
            hard_stop: false,
            committed_minor: 10,
            unsettled_estimated_minor: 20,
            completed_minor: 30,
            failed_minor: 40,
            remaining_minor: None,
            unknown_cost_jobs: 2,
            unknown_exposure_jobs: 3,
            gate: crate::workflow::cost::SpendGate::NoLimit,
        };
        let message = director_budget_status(&ledger);
        assert!(message.contains("1.00 USD accounted exposure"), "{message}");
        assert!(message.contains("No configured project limit"), "{message}");
        assert!(message.contains("2 job(s) have unknown cost"), "{message}");
        assert!(
            message.contains("3 job(s) have unverified maximum exposure"),
            "{message}"
        );
        assert!(
            message.contains("Provider charges are not verified"),
            "{message}"
        );
        assert!(
            !message.to_ascii_lowercase().contains(" spent"),
            "{message}"
        );
        assert!(
            !message.to_ascii_lowercase().contains("unlimited"),
            "{message}"
        );
    }

    #[test]
    fn parses_director_action_block() {
        let text = r#"
I'll route this to scene generation.
<director_action>
{"action":"run_specialist","specialist":"scene_generation","inputs":{"handoff_path":"Prod_Notes/FlightPt1/Scene1.md"},"requires_approval":true}
</director_action>
"#;
        let actions = parse_actions(text).expect("parse");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, ActionKind::RunSpecialist);
        assert_eq!(actions[0].specialist.as_deref(), Some("scene_generation"));
        assert!(actions[0].inputs.get("model").is_none());
    }

    #[test]
    fn parses_assemble_map_action_with_prose_padding() {
        let text = r#"
Assembling from the final prompt map.
<director_action>
Here is the action:
{"action":"run_specialist","specialist":"assemble","inputs":{"assemble_map":"Final/final_prompt.txt","overwrite":true},"requires_approval":true}
</director_action>
"#;
        let actions = parse_actions(text).expect("parse");
        assert_eq!(actions[0].specialist.as_deref(), Some("assemble"));
        assert_eq!(
            actions[0]
                .inputs
                .get("assemble_map")
                .and_then(|v| v.as_str()),
            Some("Final/final_prompt.txt")
        );
    }

    #[test]
    fn rejects_unknown_specialist_field() {
        let text = r#"<director_action>
{"action":"run_specialist","specialist":"still_image","inputs":{"prompt":"x","output_path":"/tmp/out.png","hack":true}}
</director_action>"#;
        let actions = parse_actions(text).expect("parse");
        let err = validate_action(actions[0].clone(), Some("/tmp")).expect_err("should reject");
        assert!(err.to_string().contains("hack"), "{err}");
    }

    #[test]
    fn accepts_scene_intent_specialist() {
        let text = r#"<director_action>
{"action":"run_specialist","specialist":"scene_intent","inputs":{"brief":"Draft scene intent for Scene 1.","scene":1}}
</director_action>"#;
        let actions = parse_actions(text).expect("parse");
        validate_action(actions[0].clone(), Some("/tmp")).expect("scene_intent should be allowed");
    }

    #[test]
    fn accepts_scene_look_specialist() {
        let text = r#"<director_action>
{"action":"run_specialist","specialist":"scene_look","inputs":{"brief":"Draft the scene look board for Scene 1.","scene":1}}
</director_action>"#;
        let actions = parse_actions(text).expect("parse");
        validate_action(actions[0].clone(), Some("/tmp")).expect("scene_look should be allowed");
    }

    #[test]
    fn detects_generate_scene_request() {
        assert!(looks_like_generate_scene_request(
            "Please send the attached scene for video generation. All relevant information is in the file."
        ));
        assert!(looks_like_paid_approval(
            "I'd like you to send the attached scene for video generation."
        ));
    }

    #[test]
    fn chat_approval_words_do_not_preserve_or_execute_a_parked_plan() {
        assert!(looks_like_paid_approval("ok"));
        assert!(looks_like_continue_request("continue"));
        assert!(!should_keep_pending_for_chat_turn("ok"));
        assert!(!should_keep_pending_for_chat_turn("continue"));
        assert!(should_keep_pending_for_chat_turn(
            "Which model will this use?"
        ));
    }

    #[test]
    fn detects_adult_routing_keywords() {
        assert!(looks_like_adult_routing_request(
            "Please generate this scene using the adult models"
        ));
        assert!(looks_like_adult_routing_request("use nsfw for this cut"));
        assert!(looks_like_adult_routing_request("spicy models please"));
        assert!(looks_like_adult_routing_request("adult"));
        assert!(!looks_like_adult_routing_request(
            "Generate Scene1 with the usual defaults"
        ));
        assert!(!looks_like_adult_routing_request(
            "the father walks into town with his children"
        ));
        assert!(!looks_like_adult_routing_request(
            "Are you using the adult/nsfw settings?"
        ));
    }

    #[test]
    fn rejects_capability_slot_as_specialist() {
        let text = r#"<director_action>
{"action":"run_specialist","specialist":"image.text_to_image","inputs":{"prompt":"x","output_path":"/tmp/out.png"}}
</director_action>"#;
        let actions = parse_actions(text).expect("parse");
        let err = validate_action(actions[0].clone(), Some("/tmp")).expect_err("should reject");
        let msg = err.to_string();
        assert!(msg.contains("capability slot"), "{msg}");
        assert!(msg.contains("still_image"), "{msg}");
    }

    #[test]
    fn rejects_missing_still_source_frame() {
        let text = r#"<director_action>
{"action":"run_specialist","specialist":"still_image","inputs":{"edit_brief":"make it adult","output_path":"/tmp/out.png","source_frame":"/tmp/does-not-exist-source.png"}}
</director_action>"#;
        let actions = parse_actions(text).expect("parse");
        let err = validate_action(actions[0].clone(), Some("/tmp")).expect_err("should reject");
        let msg = err.to_string();
        assert!(msg.contains("source_frame"), "{msg}");
        assert!(msg.contains("existing file"), "{msg}");
    }

    #[test]
    fn adult_routing_survives_approval_in_history() {
        let messages = vec![
            ChatMessage {
                role: "user".into(),
                content: "Generate Scene1 using the adult models".into(),
            },
            ChatMessage {
                role: "assistant".into(),
                content: "## Approval required\n…".into(),
            },
            ChatMessage {
                role: "user".into(),
                content: "Approved".into(),
            },
        ];
        assert!(adult_routing_from_history(&messages));
        assert!(!adult_routing_from_history(&[ChatMessage {
            role: "user".into(),
            content: "Approved".into(),
        }]));
    }

    #[test]
    fn accepts_scene_generation_start_frame_lane() {
        let dir = std::env::temp_dir().join(format!(
            "wb_i2v_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let frame = dir.join("start.png");
        std::fs::write(&frame, b"fake").unwrap();
        let out = dir.join("out.mp4");
        let text = format!(
            r#"<director_action>
{{"action":"run_specialist","specialist":"scene_generation","inputs":{{"source_frame":"{}","prompt":"camera drifts left","output_path":"{}","duration":5}},"requires_approval":true}}
</director_action>"#,
            frame.display(),
            out.display()
        );
        let actions = parse_actions(&text).expect("parse");
        let validated = validate_action(actions[0].clone(), Some(dir.to_str().unwrap()))
            .expect("start-frame lane should validate");
        assert_eq!(
            validated.action.specialist.as_deref(),
            Some("scene_generation")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_scene_generation_text_to_video_freeform() {
        let dir = std::env::temp_dir().join(format!(
            "wb_t2v_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("clip.mp4");
        let text = format!(
            r#"<director_action>
{{"action":"run_specialist","specialist":"scene_generation","inputs":{{"prompt":"misty pine forest at dawn","output_path":"{}","duration":5}},"requires_approval":true}}
</director_action>"#,
            out.display()
        );
        let actions = parse_actions(&text).expect("parse");
        let validated = validate_action(actions[0].clone(), Some(dir.to_str().unwrap()))
            .expect("text-to-video freeform should validate without handoff");
        assert_eq!(
            validated.action.specialist.as_deref(),
            Some("scene_generation")
        );
        assert!(validated.action.inputs.get("handoff_path").is_none());
        assert!(
            validated
                .action
                .inputs
                .get("output_path")
                .and_then(|v| v.as_str())
                .is_some_and(|p| p.ends_with(".mp4")),
            "host should keep or derive an output path"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn host_derives_still_output_and_strips_model() {
        let text = r#"<director_action>
{"action":"run_specialist","specialist":"still_image","inputs":{"prompt":"wide rainy alley","model":"secret-id"},"requires_approval":false}
</director_action>"#;
        let actions = parse_actions(text).expect("parse");
        assert!(actions[0].inputs.get("model").is_none());
        let validated = validate_action(actions[0].clone(), Some("/tmp"))
            .expect("still_image should validate without an invented output_path");
        assert_eq!(
            validated
                .action
                .inputs
                .get("output_path")
                .and_then(|v| v.as_str()),
            Some("Stills/wide_rainy_alley.png")
        );
        assert!(crate::specialists::specialist_requires_approval(
            "still_image"
        ));
        assert!(crate::specialists::specialist_requires_approval("assemble"));
        assert!(!crate::specialists::specialist_requires_approval("script"));
    }

    #[test]
    fn host_derives_freeform_video_without_output_path() {
        let text = r#"<director_action>
{"action":"run_specialist","specialist":"scene_generation","inputs":{"prompt":"misty pine forest at dawn"}}
</director_action>"#;
        let actions = parse_actions(text).expect("parse");
        let validated = validate_action(actions[0].clone(), Some("/tmp"))
            .expect("freeform video should not require the model to invent output_path");
        assert_eq!(
            validated
                .action
                .inputs
                .get("output_path")
                .and_then(|v| v.as_str()),
            Some("Videos/misty_pine_forest_at_dawn.mp4")
        );
    }

    #[test]
    fn renderer_model_defaults_are_replaced_by_enabled_host_settings() {
        let mut context = DirectorContext::default();
        context.capability_defaults.insert(
            "image.text_to_image".into(),
            Some(ModelRef {
                api_id: "renderer-api".into(),
                model_id: "untrusted-model".into(),
            }),
        );
        context.adult_content_enabled = true;
        let settings = serde_json::json!({
            "capabilityDefaults": {
                "image.text_to_image": {
                    "apiId": "host-api",
                    "modelId": "host-model"
                }
            },
            "adultContentEnabled": false
        });
        let apis = vec![settings::ApiEntry {
            id: "host-api".into(),
            name: "Host Provider".into(),
            kind: "image-provider".into(),
            base_url: "https://provider.invalid".into(),
            api_key: String::new(),
            secret_key: String::new(),
            enabled: true,
        }];

        hydrate_routing_context(&mut context, &settings, &apis).unwrap();

        let route = context.capability_defaults["image.text_to_image"]
            .as_ref()
            .unwrap();
        assert_eq!(route.api_id, "host-api");
        assert_eq!(route.model_id, "host-model");
        assert_eq!(
            context.api_labels["host-api"],
            "Host Provider (image-provider)"
        );
        assert!(!context.adult_content_enabled);
        assert!(context.adult_capability_defaults.is_empty());
    }

    #[test]
    fn workflow_approval_route_disclosure_uses_host_model_defaults() {
        let mut context = DirectorContext::default();
        context.capability_defaults.insert(
            "image.text_to_image".into(),
            Some(ModelRef {
                api_id: "host-api".into(),
                model_id: "host-image-model".into(),
            }),
        );
        context
            .api_labels
            .insert("host-api".into(), "Image Provider (image-provider)".into());
        let action = crate::director_actions::DirectorAction {
            action: ActionKind::RunSpecialist,
            specialist: Some("still_image".into()),
            inputs: serde_json::json!({ "prompt": "approved input" })
                .as_object()
                .unwrap()
                .clone(),
            message: None,
            op: None,
            path: None,
        };

        let (provider, model, cost) = approval_route_details(&action, &context, Some("/project"));

        assert!(provider.contains("Image Provider"), "{provider}");
        assert!(model.contains("host-image-model"), "{model}");
        assert!(cost.contains("Unknown"), "{cost}");
    }

    #[test]
    fn aggregate_budget_gate_blocks_routes_individually_affordable_but_jointly_over_limit() {
        use crate::workflow::cost::SpendGate;
        use crate::workflow::state::{ProjectBudget, ProjectState};
        let mut state = ProjectState::new();
        state.budget = Some(ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(10),
            warning_at_minor: None,
            hard_stop: true,
            policy: None,
        });
        assert_eq!(aggregate_route_gate(&state, [6]), SpendGate::Allow);
        assert_eq!(aggregate_route_gate(&state, [6, 5]), SpendGate::Blocked);
        assert_eq!(aggregate_route_gate(&state, [10]), SpendGate::Allow);
        assert_eq!(aggregate_route_gate(&state, [10, 1]), SpendGate::Blocked);
        assert_eq!(
            aggregate_route_gate(&state, [i64::MAX, 1]),
            SpendGate::Blocked
        );
    }

    #[test]
    fn workflow_card_parks_the_host_resolved_action_and_input_snapshot() {
        let _session_guard = crate::session::test_session_lock();
        let _pending_guard = crate::pending::test_pending_lock();
        crate::session::close();
        crate::pending::cancel_all();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("Refs")).unwrap();
        std::fs::write(root.join("Refs/source.png"), b"source image").unwrap();
        let root = root.canonicalize().unwrap();
        crate::session::open(root.to_str().unwrap()).unwrap();
        let app = crate::ctx::HostCtx::new(dir.path().join("settings"));
        let settings = serde_json::json!({
            "apis": [{
                "id": "host-api",
                "name": "Host Provider",
                "kind": "image-provider",
                "baseUrl": "https://provider.invalid",
                "apiKey": "",
                "secretKey": "",
                "enabled": true
            }],
            "capabilityDefaults": {
                "image.text_to_image": {
                    "apiId": "host-api",
                    "modelId": "host-image-model"
                }
            }
        });
        std::fs::write(app.settings_path(), serde_json::to_vec(&settings).unwrap()).unwrap();
        let action = crate::director_actions::DirectorAction {
            action: ActionKind::RunSpecialist,
            specialist: Some("still_image".into()),
            inputs: serde_json::json!({
                "prompt": "approved prompt",
                "reference_images": ["Refs/source.png"],
                "output_path": "Stills/output.png"
            })
            .as_object()
            .unwrap()
            .clone(),
            message: None,
            op: None,
            path: None,
        };
        let validated = validate_action(action.clone(), Some(root.to_str().unwrap())).unwrap();

        let card = format_approval_request_for_app(
            &app,
            &action,
            &validated,
            Some(root.to_str().unwrap()),
        )
        .unwrap();
        assert!(card.contains("Provider: Host Provider (image-provider)"));
        assert!(card.contains("Model: image.text_to_image: `host-image-model`"));
        assert!(card.contains("Cost estimate: Unknown; provider pricing applies"));
        assert!(card.contains("Refs/source.png"));
        assert!(card.contains("Stills/output.png"));

        let id = card
            .split(crate::pending::APPROVE_TOKEN_PREFIX)
            .nth(1)
            .and_then(|rest| rest.strip_suffix("]]"))
            .expect("one-time approval ID");
        let pending = crate::pending::take(id).unwrap();
        assert_eq!(pending.action.specialist.as_deref(), Some("still_image"));
        assert_eq!(pending.project_root, Some(root));
        assert!(pending
            .input_fingerprints
            .unwrap()
            .get("reference_images[0]")
            .is_some());
        assert!(
            crate::pending::take(id).is_err(),
            "approval cannot be replayed"
        );
        crate::session::close();
        crate::pending::cancel_all();
    }
}

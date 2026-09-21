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

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Settings API entry id (user-named instance).
    #[serde(alias = "providerId")]
    pub api_id: String,
    pub model_id: String,
}

#[derive(Debug, Deserialize)]
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

fn resolve_path(raw: &str, project_root: Option<&str>) -> PathBuf {
    let path = PathBuf::from(raw.trim());
    if path.is_absolute() {
        return path;
    }
    if let Some(root) = project_root {
        return PathBuf::from(root).join(path);
    }
    path
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
    let path = resolve_path(path_str, project_root);
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
    use crate::director_actions::encode_pending_action;
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
    let mut reply = format!(
        "Host answer (from Settings / validated pending plan) — for: _{}_\n\n{notes}\n\n{}",
        user_text.trim(),
        encode_pending_action(pending)
    );
    if specialist == "still_image" || specialist == "scene_generation" || specialist == "text_sync"
    {
        reply.push_str("\n\nReply **Approved** to run this pending plan, or say what to change.");
    }
    Some(reply)
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
        find_pending_action_in_history, format_approval_request, parse_actions,
        planner_system_prompt, validate_action, ActionKind,
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
    // Per-turn adult routing from user keywords (adult / nsfw / …). Survives Approve/continue
    // via recent user-message history.
    context.adult_routing_requested = adult_routing_from_history(&request.messages);
    let project_root = context
        .project_root
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let project_root_ref = project_root.as_deref();

    let history_pairs: Vec<(String, String)> = request
        .messages
        .iter()
        .map(|m| (m.role.clone(), m.content.clone()))
        .collect();
    let user_approved = last_user_message(&request.messages).is_some_and(looks_like_paid_approval);
    let user_continue =
        last_user_message(&request.messages).is_some_and(looks_like_continue_request);
    let mut tool_events = Vec::new();

    if context.adult_routing_requested {
        app.emit(
            "director-progress",
            "Adult/NSFW routing active — using Adult generation defaults when set…".to_string(),
        );
        tool_events.push("adult_routing:on".into());
    }

    // Pending specialist plan is still awaiting approval — answer model/plan
    // questions from host Settings instead of re-emitting the same approval card.
    if !user_approved && !user_continue {
        if let Some(user_text) = last_user_message(&request.messages) {
            if looks_like_pending_plan_question(user_text) {
                if let Some(pending) = find_pending_action_in_history(&history_pairs) {
                    if let Some(reply) = answer_pending_plan_question(
                        &context,
                        &pending,
                        user_text,
                        project_root_ref,
                    ) {
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
    }

    // If the user approved/continued and a pending action is embedded in history, execute it.
    if user_approved || user_continue {
        if let Some(pending) = find_pending_action_in_history(&history_pairs) {
            match validate_action(pending.clone(), project_root_ref) {
                Ok(validated) => {
                    app.emit(
                        "director-progress",
                        "Host executing pending specialist action…".to_string(),
                    );
                    let outcome = match validated.action.action {
                        ActionKind::RunSpecialist => specialists::run_specialist(
                            &app,
                            &context,
                            &validated,
                            project_root_ref,
                            Some(&specialist_llm),
                        )
                        .map(Some),
                        ActionKind::Inspect => {
                            specialists::run_inspect(&validated, project_root_ref).map(|text| {
                                Some(specialists::SpecialistResult {
                                    message: text,
                                    continue_suggested: false,
                                })
                            })
                        }
                        ActionKind::AskUser | ActionKind::Done => {
                            Ok(Some(specialists::SpecialistResult {
                                message: validated.action.message.clone().unwrap_or_default(),
                                continue_suggested: false,
                            }))
                        }
                    };
                    match outcome {
                        Ok(Some(result)) => {
                            tool_events.push(format!(
                                "pending:{}",
                                validated
                                    .action
                                    .specialist
                                    .clone()
                                    .unwrap_or_else(|| format!("{:?}", validated.action.action))
                            ));
                            return Ok(DirectorTurnResponse {
                                reply: result.message,
                                model,
                                tool_events,
                                continue_suggested: result.continue_suggested,
                            });
                        }
                        Ok(None) => {}
                        Err(error) => {
                            return Ok(DirectorTurnResponse {
                                reply: format!("Host could not execute pending action: {error}"),
                                model,
                                tool_events,
                                continue_suggested: false,
                            });
                        }
                    }
                }
                Err(error) => {
                    app.emit(
                        "director-progress",
                        format!("Pending action failed validation: {error}"),
                    );
                }
            }
        }
    }

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
            "content": message.content,
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
                        let mut approval = format_approval_request(&validated.action, &validated);
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
    director_turn_inner(app, request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::director_actions::{parse_actions, validate_action, ActionKind};

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
}

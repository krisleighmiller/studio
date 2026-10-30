//! Host-owned specialist runners.
//!
//! The chat model never constructs provider requests. These functions do.

use crate::atlas;
use crate::chat;
use crate::ctx::HostCtx;
use crate::director::{DirectorContext, ModelRef};
use crate::director_actions::{
    encode_pending_action, find_pending_action_in_history, resolve_under_project, DirectorAction,
    ValidatedAction, PENDING_ACTION_START,
};
use crate::media;
use crate::prompts;
use crate::scene_plan::{RenderLane, RenderRequest};
use crate::settings::{self, ApiEntry};
use crate::text_writes::{
    approval_required, approval_summary, authorize_text_write, commit_staged_text_write,
    commit_text_write, prepare_text_write_approval_card, stage_authorized_text_writes,
};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Chat credentials for specialists that call the LLM (advisory helpers).
pub struct SpecialistLlm<'a> {
    pub api: &'a ApiEntry,
    pub api_key: &'a str,
    pub model: &'a str,
    /// Latest user turn — fallback when inputs omit question/brief.
    pub user_question: &'a str,
}

#[derive(Debug, Clone)]
pub struct SpecialistResult {
    pub message: String,
    pub continue_suggested: bool,
}

impl SpecialistResult {
    pub(crate) fn done(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            continue_suggested: false,
        }
    }

    pub(crate) fn cont(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            continue_suggested: true,
        }
    }
}

pub(crate) fn input_str<'a>(inputs: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(s) = inputs.get(*key).and_then(|v| v.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

fn input_u32(inputs: &Map<String, Value>, keys: &[&str]) -> Option<u32> {
    for key in keys {
        if let Some(value) = inputs.get(*key) {
            if let Some(n) = value.as_u64() {
                return Some(n as u32);
            }
            if let Some(n) = value.as_str().and_then(|s| s.trim().parse().ok()) {
                return Some(n);
            }
        }
    }
    None
}

fn input_path_list(inputs: &Map<String, Value>, keys: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for key in keys {
        match inputs.get(*key) {
            Some(Value::String(s)) if !s.trim().is_empty() => out.push(s.trim().to_string()),
            Some(Value::Array(items)) => {
                for item in items {
                    if let Some(s) = item.as_str() {
                        if !s.trim().is_empty() {
                            out.push(s.trim().to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub(crate) fn capability_model(context: &DirectorContext, slot: &str) -> Option<ModelRef> {
    // Adult spicy slots only when this turn requested adult/nsfw routing (user keywords).
    if context.adult_routing_requested {
        if let Some(model) = context
            .adult_capability_defaults
            .get(slot)
            .and_then(|v| v.as_ref())
            .filter(|m| !m.model_id.trim().is_empty())
            .cloned()
        {
            return Some(model);
        }
    }
    context
        .capability_defaults
        .get(slot)
        .and_then(|v| v.as_ref())
        .filter(|m| !m.model_id.trim().is_empty())
        .cloned()
}

fn select_capability<'a>(
    context: &DirectorContext,
    slots: &'a [&'static str],
) -> Option<(ModelRef, &'static str)> {
    slots
        .iter()
        .find_map(|slot| capability_model(context, slot).map(|model| (model, *slot)))
}

fn run_media_generate(
    app: &HostCtx,
    project_root: Option<&str>,
    kind: &str,
    model: &str,
    api_id: Option<&str>,
    params: Value,
    save_path: &str,
) -> Result<String, String> {
    // Keep polling in-process so the Director turn only returns after download
    // (or a hard timeout with a resumable prediction_id). Seedream/edit and
    // video jobs routinely exceed 90s.
    let wait_seconds = match kind {
        "video" => 300,
        "audio" => 180,
        _ => 240,
    };
    let mut args = json!({
        "model": model,
        "params": params,
        "save_path": save_path,
        "wait_seconds": wait_seconds,
    });
    if let Some(api_id) = api_id {
        args["api_id"] = json!(api_id);
    }
    let tool = match kind {
        "video" => "atlas_generate_video",
        "audio" => "atlas_generate_audio",
        _ => "atlas_generate_image",
    };
    match media::run_media_tool(tool, &args, project_root, app) {
        Some(Ok(text)) => Ok(text),
        Some(Err(error)) => Err(error),
        None => Err(format!("Media router ignored tool `{tool}`")),
    }
}

/// Interpret media-router JSON: require a local saved file, else error with prediction_id.
fn require_media_saved(raw: &str, save_path: &str) -> Result<String, String> {
    let parsed: Value = serde_json::from_str(raw).unwrap_or(json!({ "raw": raw }));
    let saved = parsed.get("saved");
    let file_nonempty =
        |p: &str| Path::new(p).is_file() && fs::metadata(p).map(|m| m.len()).unwrap_or(0) > 0;
    // Never treat a media-layer error payload as success, even if an older file
    // already sits at save_path from a previous run.
    let has_error = parsed
        .get("error")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty());
    let saved_ok = !has_error
        && (saved
            .and_then(|s| s.get("saved"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || saved
                .and_then(|s| s.get("path"))
                .and_then(|v| v.as_str())
                .is_some_and(file_nonempty)
            || file_nonempty(save_path));

    if saved_ok {
        let bytes = saved
            .and_then(|s| s.get("bytes"))
            .and_then(|v| v.as_u64())
            .or_else(|| fs::metadata(save_path).ok().map(|m| m.len()))
            .unwrap_or(0);
        let prediction_id = saved
            .and_then(|s| s.get("prediction_id"))
            .and_then(|v| v.as_str())
            .or_else(|| {
                parsed
                    .pointer("/submitted/prediction_id")
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("");
        let status = saved
            .and_then(|s| s.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("completed");
        return Ok(format!(
            "- local file: `{save_path}` ({bytes} bytes)\n\
- prediction_id: `{prediction_id}`\n\
- status: `{status}`"
        ));
    }

    let prediction_id = parsed
        .pointer("/submitted/prediction_id")
        .or_else(|| parsed.get("prediction_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let err = parsed
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("download did not complete inside the wait window");
    if prediction_id.is_empty() {
        Err(format!(
            "Media job did not finish downloading to `{save_path}`: {err}"
        ))
    } else {
        Err(format!(
            "Media job submitted but not downloaded yet.\n\
- prediction_id: `{prediction_id}`\n\
- save_path: `{save_path}`\n\
- detail: {err}\n\n\
Say **continue** to keep polling/downloading this prediction (do not re-generate)."
        ))
    }
}

fn prediction_id_from_media_result(raw: &str) -> String {
    let parsed: Value = serde_json::from_str(raw).unwrap_or(json!({}));
    parsed
        .pointer("/submitted/prediction_id")
        .or_else(|| parsed.get("prediction_id"))
        .or_else(|| parsed.pointer("/saved/prediction_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Resume atlas_collect_media for a timed-out prediction; used by still/scene/text-sync.
fn resume_media_collect(
    app: &HostCtx,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    prediction_id: &str,
    save_path: &Path,
    output_key: &str,
    done_label: &str,
) -> Result<SpecialistResult, String> {
    app.emit(
        "director-progress",
        format!("Resuming download for prediction `{prediction_id}`…"),
    );
    let collect_args = json!({
        "prediction_id": prediction_id,
        "path": save_path.to_string_lossy(),
        "wait_seconds": 240,
    });
    let result =
        match media::run_media_tool("atlas_collect_media", &collect_args, project_root, app) {
            Some(Ok(text)) => text,
            Some(Err(error)) => return Err(error),
            None => return Err("Media router ignored atlas_collect_media".into()),
        };
    match require_media_saved(&result, &save_path.to_string_lossy()) {
        Ok(summary) => Ok(SpecialistResult::done(format!(
            "{done_label} download finished.\n{summary}"
        ))),
        Err(error) => {
            let mut pending = validated.action.clone();
            pending
                .inputs
                .insert("collect_prediction_id".into(), json!(prediction_id));
            pending.inputs.insert(
                output_key.into(),
                json!(save_path.to_string_lossy().to_string()),
            );
            Ok(SpecialistResult::cont(format!(
                "{error}\n\n{}",
                encode_pending_action(&pending)
            )))
        }
    }
}

/// After generate+collect: confirm local file, or encode a resumable pending collect.
fn finish_media_or_resume(
    validated: &ValidatedAction,
    label: &str,
    model_id: &str,
    save_path: &str,
    result: &str,
    output_key: &str,
) -> Result<SpecialistResult, String> {
    match require_media_saved(result, save_path) {
        Ok(summary) => Ok(SpecialistResult::done(format!(
            "{label} finished (host waited for download).\n\
- model: `{model_id}`\n\
{summary}"
        ))),
        Err(error) => {
            let prediction_id = prediction_id_from_media_result(result);
            if prediction_id.is_empty() {
                return Err(error);
            }
            let mut pending = validated.action.clone();
            pending
                .inputs
                .insert("collect_prediction_id".into(), json!(prediction_id));
            pending.inputs.insert(output_key.into(), json!(save_path));
            Ok(SpecialistResult::cont(format!(
                "{error}\n\n{}",
                encode_pending_action(&pending)
            )))
        }
    }
}

/// Pull a usable https URL out of an Atlas upload JSON blob or a bare URL string.
fn extract_upload_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Some(trimmed.to_string());
    }
    let parsed: Value = serde_json::from_str(trimmed).ok()?;
    parsed
        .get("download_url")
        .or_else(|| parsed.get("url"))
        .or_else(|| parsed.pointer("/data/download_url"))
        .or_else(|| parsed.pointer("/data/url"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
}

fn upload_local(
    app: &HostCtx,
    project_root: Option<&str>,
    path: &str,
    role: crate::media_util::MediaRole,
) -> Result<String, String> {
    let api = settings::find_api_by_kind(app, "atlas-cloud").or_else(|_| {
        settings::list_apis(app)?
            .into_iter()
            .next()
            .ok_or_else(|| "No APIs configured".to_string())
    })?;
    if api.kind == "atlas-cloud" && !api.api_key.trim().is_empty() {
        let result = atlas::upload_media(api.api_key.trim(), path, project_root, role)?;
        return extract_upload_url(&result).ok_or_else(|| {
            format!(
                "Atlas upload did not return a download_url for `{path}`. Raw: {}",
                result.chars().take(400).collect::<String>()
            )
        });
    }
    // Leave an outside one-time grant unspent until the selected adapter reads it.
    let resolved = crate::media_util::peek_path(path, project_root)?;
    crate::media_util::ensure_media_file_as(&resolved, role)?;
    Ok(resolved.to_string_lossy().to_string())
}

fn parse_handoff_headers_and_yaml(text: &str) -> (HashMap<String, String>, Option<Value>) {
    let mut headers = HashMap::new();
    for line in text.lines().take(40) {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase().replace(' ', "_");
            if matches!(
                key.as_str(),
                "output" | "filenames" | "character_reference" | "voice" | "style"
            ) {
                headers.insert(key, v.trim().trim_matches('"').to_string());
            }
        }
        if line.trim().starts_with("```") {
            break;
        }
    }
    let yaml_json = extract_handoff_yaml_as_json(text);
    (headers, yaml_json)
}

/// Minimal YAML→JSON for the fields we need (not a full YAML parser).
fn extract_handoff_yaml_as_json(text: &str) -> Option<Value> {
    let start = text.find("```yaml").or_else(|| text.find("```yml"))?;
    let after = text[start..].lines().skip(1);
    let mut body = String::new();
    for line in after {
        if line.trim().starts_with("```") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    let mut map = Map::new();
    let mut current_prompt = None::<String>;
    let mut in_prompt = false;
    for line in body.lines() {
        let trimmed = line.trim_end();
        if in_prompt {
            if !line.starts_with(' ') && !line.starts_with('\t') && trimmed.contains(':') {
                in_prompt = false;
            } else {
                if let Some(p) = current_prompt.as_mut() {
                    if !p.is_empty() {
                        p.push('\n');
                    }
                    p.push_str(trimmed.trim());
                }
                continue;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("prompt:") {
            let rest = rest.trim();
            if rest == "|-" || rest == "|" || rest.is_empty() {
                in_prompt = true;
                current_prompt = Some(String::new());
            } else {
                map.insert("prompt".into(), json!(rest.trim_matches('"').to_string()));
            }
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            if key.is_empty() {
                continue;
            }
            if key == "reference_assets" {
                let list = parse_yaml_string_list(val, &body, key);
                map.insert(key.into(), json!(list));
            } else if key == "target_duration_seconds" || key == "estimated_screen_duration_seconds"
            {
                if let Ok(n) = val.parse::<u64>() {
                    map.insert(key.into(), json!(n));
                }
            } else if !val.is_empty() && val != "|" && val != "|-" && !val.starts_with('[') {
                map.insert(key.into(), json!(val.trim_matches('"')));
            } else if val.starts_with('[') {
                let list = parse_yaml_string_list(val, &body, key);
                map.insert(key.into(), json!(list));
            }
        }
    }
    if let Some(p) = current_prompt {
        if !p.trim().is_empty() {
            map.insert("prompt".into(), json!(p.trim()));
        }
    }
    // Nested generation_requirements.source_mode — scan raw
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("source_mode:") {
            map.insert("source_mode".into(), json!(rest.trim()));
        }
        if let Some(rest) = t.strip_prefix("aspect_ratio:") {
            map.insert("aspect_ratio".into(), json!(rest.trim()));
        }
        if let Some(rest) = t.strip_prefix("negative_prompt:") {
            map.insert("negative_prompt".into(), json!(rest.trim()));
        }
    }
    Some(Value::Object(map))
}

fn parse_yaml_string_list(inline: &str, body: &str, _key: &str) -> Vec<String> {
    let inline = inline.trim();
    if inline.starts_with('[') && inline.ends_with(']') {
        return inline[1..inline.len() - 1]
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    // Fallback: find paths on following lines — rarely needed
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("- ") {
            let v = rest.trim().trim_matches('"');
            if v.contains('/') || v.ends_with(".png") || v.ends_with(".jpg") {
                out.push(v.to_string());
            }
        }
    }
    out
}

fn join_output(
    headers: &HashMap<String, String>,
    project_root: Option<&str>,
) -> Result<PathBuf, String> {
    let output = headers
        .get("output")
        .ok_or_else(|| "Handoff missing Output header".to_string())?;
    let filename = headers
        .get("filenames")
        .ok_or_else(|| "Handoff missing Filenames header".to_string())?
        .trim()
        .trim_matches('"')
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"');
    if filename.is_empty() {
        return Err("Handoff Filenames header is empty".into());
    }
    let dir = resolve_under_project(output, project_root).map_err(|e| e.to_string())?;
    Ok(dir.join(filename))
}

pub fn run_inspect(
    action: &ValidatedAction,
    _project_root: Option<&str>,
) -> Result<String, String> {
    let path = action
        .resolved_paths
        .iter()
        .find(|(k, _)| k == "path")
        .map(|(_, p)| p.clone())
        .ok_or_else(|| "inspect missing resolved path".to_string())?;
    let op = action.action.op.as_deref().unwrap_or("read_file");
    match op {
        "media_probe" => {
            let report = crate::media_inspect::inspect_media(&path, &action.action.inputs)?;
            serde_json::to_string_pretty(&report).map_err(|e| format!("media_probe JSON: {e}"))
        }
        "path_exists" => Ok(json!({
            "exists": path.exists(),
            "isFile": path.is_file(),
            "isDir": path.is_dir(),
            "path": path.to_string_lossy(),
        })
        .to_string()),
        "list_dir" => {
            let read = fs::read_dir(&path).map_err(|e| format!("list_dir: {e}"))?;
            let mut items = Vec::new();
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                items.push(json!({
                    "name": name,
                    "path": entry.path().to_string_lossy(),
                    "isDir": entry.file_type().map(|t| t.is_dir()).unwrap_or(false),
                }));
            }
            Ok(serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".into()))
        }
        _ => {
            let text = fs::read_to_string(&path).map_err(|e| format!("read_file: {e}"))?;
            // Cap huge files for planner context
            if text.len() > 80_000 {
                let mut end = 80_000;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                Ok(format!(
                    "(read_file truncated: omitted {} UTF-8 bytes of {} total bytes)\n\n{}…",
                    text.len() - end,
                    text.len(),
                    &text[..end]
                ))
            } else {
                Ok(text)
            }
        }
    }
}

pub fn run_specialist(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    llm: Option<&SpecialistLlm<'_>>,
) -> Result<SpecialistResult, String> {
    let specialist = validated
        .action
        .specialist
        .as_deref()
        .ok_or_else(|| "missing specialist".to_string())?;
    app.emit(
        "director-progress",
        format!("Running specialist `{specialist}`…"),
    );
    match specialist {
        "scene_generation" => run_scene_generation(app, context, validated, project_root),
        "still_image" => run_still_image(app, context, validated, project_root),
        "text_sync" => run_text_sync(app, context, validated, project_root),
        "lip_sync" => run_lip_sync(app, context, validated, project_root),
        "assemble" => {
            let save_path = validated.action.inputs.get("output_path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !save_path.is_empty() && project_root.is_some() {
                let result = crate::workflow::assembly_jobs::with_validated_assembly_job(
                    std::path::Path::new(project_root.unwrap()),
                    validated,
                    || crate::assemble::run(app, validated, project_root),
                );
                result.map(SpecialistResult::done)
            } else {
                crate::assemble::run(app, validated, project_root).map(SpecialistResult::done)
            }
        }
        "model_sheet" => run_model_sheet(app, context, validated, project_root),
        "production_helper" => {
            let llm = llm.ok_or_else(|| {
                "production_helper needs the Director chat model (pick one in the chat dropdown)."
                    .to_string()
            })?;
            run_production_helper(app, context, validated, project_root, llm)
        }
        "scene_plan" | "scene_intent" | "cut_sketches" | "scene_look" => {
            let llm = llm.ok_or_else(|| {
                format!(
                    "`{specialist}` needs the Director chat model (pick one in the chat dropdown)."
                )
            })?;
            run_scene_planning(app, context, validated, project_root, llm, specialist)
        }
        "text_write" => run_text_write(validated, project_root),
        "script" | "character_creation" => {
            let llm = llm.ok_or_else(|| {
                format!(
                    "`{specialist}` needs the Director chat model (pick one in the chat dropdown)."
                )
            })?;
            run_text_specialist(app, context, validated, project_root, llm, specialist)
        }
        other => Err(format!("Specialist `{other}` has no host runner")),
    }
}

const HELPER_MAX_FILES: usize = 10;
const HELPER_MAX_CHARS_PER_FILE: usize = 48_000;
const HELPER_MAX_TOTAL_CHARS: usize = 120_000;

const TEXT_SPECIALIST_MAX_FILES: usize = 14;
const TEXT_SPECIALIST_MAX_CHARS_PER_FILE: usize = 60_000;
const TEXT_SPECIALIST_MAX_TOTAL_CHARS: usize = 160_000;

/// Host wrapper so text specialists write via tagged blocks (no tool_call loop).
pub(crate) const TEXT_RUNNER_HOST_PREAMBLE: &str = r#"## Workbench host text runner

You are running inside the Video Production Workbench desktop host under the Director.
You do **not** have MCP, shell, or `<tool_call>` tools. The host reads sources for you and writes files you emit.

### Output protocol (required)

**Blocker question only** (missing brief item the contract says you must ask — nothing else):
<workbench_ask>
Your one concise question (or short consolidated brief questions).
</workbench_ask>

**Ready to save files** — emit one or more file blocks, then a short summary:
<workbench_file path="project-relative-or-absolute-path">
full file body exactly as it should be saved
</workbench_file>

<workbench_summary>
Short chat summary: paths written, scene/sidecar counts, open blockers. Do not paste full YAML/scripts unless asked.
</workbench_summary>

Rules:
- Obey the specialist contract below for content, modes, phases, and formats.
- Do not ask the user to approve ordinary local file writes — emit the files when ready.
- Prefer project-relative paths. The host rejects anything outside these locations:
  - Script: `Scripts/**/*.md` or `Scripts/**/*.txt`
  - Character sheet: `Characters/**/*.md` (contract filename shape `Characters/<Name>_ref.md`)
  - Scene plan is not written here. The host derives `prod.md` and `Scene_Prompts/SceneN.txt`.
- If references/voices are not ready, still write the plan; leave `Character_Reference: none`, `Voice: none`, empty `reference_assets` as the contract allows.
- Human-in-the-loop desk: stop after writing (or after one ask). Do not claim to generate media.
"#;

fn text_specialist_prompt(specialist: &str) -> Result<&'static str, String> {
    match specialist {
        "script" => Ok(prompts::SCRIPT_ASSISTANT_PROMPT),
        "character_creation" => Ok(prompts::CHARACTER_CREATION_PROMPT),
        "scene_plan" => Ok(prompts::SCENE_PLAN_PROMPT),
        other => Err(format!("No text prompt bundled for `{other}`")),
    }
}

fn run_scene_planning(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    llm: &SpecialistLlm<'_>,
    specialist: &str,
) -> Result<SpecialistResult, String> {
    let brief = input_str(
        &validated.action.inputs,
        &["brief", "prompt", "question", "message"],
    )
    .unwrap_or(llm.user_question);
    let combined = format!("{}\n{}", llm.user_question, brief);
    let kind = match specialist {
        "scene_intent" => crate::workflow::PlanningKind::SceneIntent,
        "cut_sketches" => crate::workflow::PlanningKind::CutSketches,
        "scene_look" => crate::workflow::PlanningKind::SceneLook,
        _ => crate::workflow::classify_planning(&combined, context.workflow_stage.as_deref()),
    };
    if kind == crate::workflow::PlanningKind::ProductionNotes {
        return crate::scene_plan::run(app, context, validated, project_root, llm);
    }
    let root = project_root
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "Open a project before drafting scene intent or cut sketches.".to_string()
        })?;
    let input_scene = input_u32(
        &validated.action.inputs,
        &["scene", "source_scene", "scene_id", "sceneId"],
    );
    let scene_id =
        crate::workflow::route::resolve_scene_id(&combined, input_scene, context.workflow_scene_id);
    let strategy = input_str(
        &validated.action.inputs,
        &["strategy", "scene_strategy", "shot_strategy"],
    );
    let message = crate::workflow::draft_from_director(
        app,
        root,
        scene_id,
        llm.model,
        &llm.api.id,
        brief,
        kind,
        strategy,
    )?;
    Ok(SpecialistResult::done(message))
}

fn text_specialist_label(specialist: &str) -> &'static str {
    match specialist {
        "script" => "Script Assistant",
        "character_creation" => "Character Creation",
        "scene_plan" => "Scene Plan",
        _ => "Text specialist",
    }
}

#[derive(Debug, Default)]
pub(crate) struct TextSpecialistEmit {
    pub(crate) ask: Option<String>,
    pub(crate) files: Vec<(String, String)>,
    pub(crate) summary: Option<String>,
}

pub(crate) fn parse_text_specialist_emit(reply: &str) -> TextSpecialistEmit {
    let mut out = TextSpecialistEmit::default();
    if let Some(ask) = extract_tagged_block(reply, "workbench_ask") {
        let t = ask.trim().to_string();
        if !t.is_empty() {
            out.ask = Some(t);
        }
    }
    if let Some(summary) = extract_tagged_block(reply, "workbench_summary") {
        let t = summary.trim().to_string();
        if !t.is_empty() {
            out.summary = Some(t);
        }
    }
    out.files = extract_workbench_files(reply);
    out
}

fn extract_tagged_block(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(&close)?;
    Some(rest[..end].to_string())
}

fn extract_workbench_files(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut search = text;
    let open = "<workbench_file";
    let close = "</workbench_file>";
    while let Some(start) = search.find(open) {
        let after_open = &search[start + open.len()..];
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let header = &after_open[..gt];
        let body_start = &after_open[gt + 1..];
        let Some(end) = body_start.find(close) else {
            break;
        };
        let body = body_start[..end].to_string();
        let path = parse_workbench_file_path(header);
        if let Some(path) = path {
            // Trim a single leading newline commonly left after the tag.
            let contents = if let Some(stripped) = body.strip_prefix('\n') {
                stripped.to_string()
            } else {
                body
            };
            let contents = contents.strip_suffix('\n').unwrap_or(&contents).to_string();
            if !path.is_empty() {
                out.push((path, contents));
            }
        }
        search = &body_start[end + close.len()..];
    }
    out
}

fn parse_workbench_file_path(header: &str) -> Option<String> {
    // path="..." or path='...'
    for quote in ['"', '\''] {
        let key = format!("path={quote}");
        if let Some(i) = header.find(&key) {
            let rest = &header[i + key.len()..];
            if let Some(end) = rest.find(quote) {
                let path = rest[..end].trim();
                if !path.is_empty() {
                    return Some(path.to_string());
                }
            }
        }
    }
    // path=unquoted
    if let Some(i) = header.find("path=") {
        let rest = header[i + "path=".len()..].trim();
        let path = rest
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|c| c == '"' || c == '\'');
        if !path.is_empty() {
            return Some(path.to_string());
        }
    }
    None
}

pub(crate) fn collect_text_specialist_paths(
    context: &DirectorContext,
    inputs: &Map<String, Value>,
) -> Vec<String> {
    let mut paths = input_path_list(
        inputs,
        &[
            "paths",
            "source_paths",
            "documents",
            "files",
            "path",
            "source_path",
            "handoff_path",
            "character_files",
            "production_path",
            "production",
            "prod_path",
        ],
    );
    for path in &context.attached_paths {
        let t = path.trim();
        if !t.is_empty() && !paths.iter().any(|p| p == t) {
            paths.push(t.to_string());
        }
    }
    for key in [
        context.selected_path.as_deref(),
        context.focused_editor_path.as_deref(),
    ] {
        if let Some(sel) = key.map(str::trim).filter(|s| !s.is_empty()) {
            if !paths.iter().any(|p| p == sel) {
                paths.push(sel.to_string());
            }
        }
    }
    if paths.len() > TEXT_SPECIALIST_MAX_FILES {
        paths.truncate(TEXT_SPECIALIST_MAX_FILES);
    }
    paths
}

pub(crate) fn build_text_specialist_dossier(
    paths: &[String],
    project_root: Option<&str>,
    context: &DirectorContext,
) -> (String, Vec<String>, Vec<String>) {
    let mut dossier = String::new();
    let mut total_chars = 0usize;
    let mut read_labels = Vec::new();
    let mut missing = Vec::new();

    for raw in paths {
        let resolved = match crate::media_util::resolve_path(raw, project_root) {
            Ok(p) => p,
            Err(e) => {
                missing.push(format!("{raw} ({e})"));
                continue;
            }
        };
        if !resolved.is_file() {
            missing.push(format!("{} (not a file)", resolved.display()));
            continue;
        }
        let bytes = fs::metadata(&resolved).map(|m| m.len()).unwrap_or(0);
        if bytes > 2_500_000 {
            missing.push(format!(
                "{} ({} bytes — too large for text specialist; attach a text excerpt)",
                resolved.display(),
                bytes
            ));
            continue;
        }
        let text = match fs::read_to_string(&resolved) {
            Ok(t) => t,
            Err(e) => {
                missing.push(format!("{} ({e})", resolved.display()));
                continue;
            }
        };
        let mut body = text;
        if body.len() > TEXT_SPECIALIST_MAX_CHARS_PER_FILE {
            body.truncate(TEXT_SPECIALIST_MAX_CHARS_PER_FILE);
            body.push_str("\n\n…[truncated for specialist context]…\n");
        }
        if total_chars + body.len() > TEXT_SPECIALIST_MAX_TOTAL_CHARS {
            missing.push(format!(
                "{} (skipped — specialist context budget full)",
                resolved.display()
            ));
            break;
        }
        total_chars += body.len();
        let label = resolved.display().to_string();
        read_labels.push(label.clone());
        dossier.push_str(&format!("### File: `{label}`\n\n```\n{body}\n```\n\n"));
    }

    if let Some(excerpt) = context
        .focused_editor_excerpt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if !dossier.contains(excerpt) {
            let mut clip = excerpt.to_string();
            if clip.len() > 12_000 {
                clip.truncate(12_000);
                clip.push_str("\n…[excerpt truncated]…");
            }
            dossier.push_str(&format!(
                "### Focused editor excerpt\n\n```\n{clip}\n```\n\n"
            ));
        }
    }

    if dossier.trim().is_empty() && missing.is_empty() {
        dossier.push_str(
            "(No project files were attached. Use the brief alone, or ask which paths to open if you need them.)\n",
        );
    }

    (dossier, read_labels, missing)
}

fn run_text_specialist(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    llm: &SpecialistLlm<'_>,
    specialist: &str,
) -> Result<SpecialistResult, String> {
    let contract = text_specialist_prompt(specialist)?;
    let label = text_specialist_label(specialist);
    let inputs = &validated.action.inputs;
    let brief = input_str(inputs, &["brief", "prompt", "question", "message"])
        .unwrap_or(llm.user_question)
        .trim();
    if brief.is_empty() {
        return Err(format!(
            "`{specialist}` needs a brief/prompt (or a non-empty user message)."
        ));
    }

    let paths = collect_text_specialist_paths(context, inputs);
    app.emit(
        "director-progress",
        format!("{label} reading {} file(s)…", paths.len()),
    );
    let (dossier, read_labels, missing) =
        build_text_specialist_dossier(&paths, project_root, context);

    let output_path = input_str(
        inputs,
        &["output_path", "save_path", "production_path", "production"],
    );
    let sidecar_dir = input_str(
        inputs,
        &[
            "sidecar_dir",
            "sidecar_folder",
            "scene_prompts_dir",
            "prompts_dir",
        ],
    );
    let aspect = input_str(inputs, &["aspect_ratio", "format"]);

    let mut user_block = String::new();
    user_block.push_str(&format!("## Specialist\n{label} (`{specialist}`)\n\n"));
    user_block.push_str(&format!("## User request\n{brief}\n\n"));
    if let Some(root) = project_root {
        user_block.push_str(&format!("## Project root\n`{root}`\n\n"));
    }
    if let Some(out) = output_path {
        user_block.push_str(&format!("## Requested output path\n`{out}`\n\n"));
    }
    if let Some(dir) = sidecar_dir {
        user_block.push_str(&format!("## Requested sidecar directory\n`{dir}`\n\n"));
    }
    if let Some(ar) = aspect {
        user_block.push_str(&format!("## Format / aspect\n{ar}\n\n"));
    }
    if specialist == "scene_plan" {
        user_block.push_str(
            "## Scene Plan defaults (use unless user named other paths)\n\
- Master production file: `prod.md`\n\
- Sidecars: `Scene_Prompts/SceneN.txt`\n\
- Leave Character_Reference / Voice empty/`none` when the user said they will add refs/voices later.\n\n",
        );
    }
    if !read_labels.is_empty() {
        user_block.push_str("## Files provided by host\n");
        for item in &read_labels {
            user_block.push_str(&format!("- `{item}`\n"));
        }
        user_block.push('\n');
    }
    if !missing.is_empty() {
        user_block.push_str("## Paths that could not be loaded\n");
        for m in &missing {
            user_block.push_str(&format!("- {m}\n"));
        }
        user_block.push('\n');
    }
    user_block.push_str("## Document dossier\n\n");
    user_block.push_str(&dossier);
    user_block.push_str(
        "\n## Reminder\nEmit `<workbench_ask>` **or** `<workbench_file>` block(s) + `<workbench_summary>`. No `<tool_call>`.\n",
    );

    let system = format!("{TEXT_RUNNER_HOST_PREAMBLE}\n\n---\n\n{contract}");
    let mut messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user_block }),
    ];

    app.emit("director-progress", format!("{label} thinking…"));
    let reply = chat::chat_completion(llm.api, llm.api_key, llm.model, &messages);
    if app.director_cancelled() {
        return Ok(SpecialistResult::done(
            "Stopped before preparing file writes.",
        ));
    }
    let mut reply = reply?;
    let mut emit = parse_text_specialist_emit(&reply);

    // One format nudge for models that answer in prose (common on smaller local LLMs).
    if emit.ask.is_none() && emit.files.is_empty() {
        app.emit(
            "director-progress",
            format!("{label} nudging for file tags…"),
        );
        messages.push(json!({ "role": "assistant", "content": reply }));
        messages.push(json!({
            "role": "user",
            "content": "Workbench host: your reply had no <workbench_ask> or <workbench_file> tags. \
Emit the required tags now. If you only need a blocker question, use <workbench_ask>. \
Otherwise emit every file to write with <workbench_file path=\"…\"> and a <workbench_summary>."
        }));
        if app.director_cancelled() {
            return Ok(SpecialistResult::done(
                "Stopped before preparing file writes.",
            ));
        }
        let next = chat::chat_completion(llm.api, llm.api_key, llm.model, &messages);
        if app.director_cancelled() {
            return Ok(SpecialistResult::done(
                "Stopped before preparing file writes.",
            ));
        }
        reply = next?;
        emit = parse_text_specialist_emit(&reply);
    }

    if emit.files.is_empty() {
        if let Some(ask) = emit.ask {
            return Ok(SpecialistResult::done(ask));
        }
        let trimmed = reply.trim();
        if trimmed.is_empty() {
            return Err(format!("{label} returned an empty reply."));
        }
        return Ok(SpecialistResult::done(format!(
            "{label} did not emit `<workbench_file>` or `<workbench_ask>` tags, so nothing was written.\n\n{trimmed}"
        )));
    }

    let mut planned = Vec::new();
    let mut total_bytes = 0usize;
    for (index, (raw_path, contents)) in emit.files.iter().enumerate() {
        let body = if contents.ends_with('\n') {
            contents.clone()
        } else {
            format!("{contents}\n")
        };
        let authorized = authorize_text_write(
            specialist,
            raw_path,
            project_root,
            body.len(),
            index,
            total_bytes,
        )?;
        total_bytes += body.len();
        planned.push((authorized, body));
    }

    let mut written = Vec::new();
    let mut staged = Vec::new();
    for (authorized, body) in planned {
        if app.director_cancelled() {
            return Ok(SpecialistResult::done(format!(
                "Stopped before the next file write. Saved new files: {written:?}"
            )));
        }
        let previous = if authorized.replacing {
            fs::read_to_string(&authorized.path).ok()
        } else {
            None
        };
        if approval_required(&authorized) {
            let summary = approval_summary(&authorized, previous.as_deref(), &body);
            staged.push((authorized, body, summary));
            continue;
        }
        app.emit(
            "director-progress",
            format!("{label} writing {}…", authorized.path.display()),
        );
        commit_text_write(&authorized, &body, false)
            .map_err(|error| format!("Could not write {}: {error}", authorized.path.display()))?;
        written.push(authorized.path.display().to_string());
    }
    if !staged.is_empty() {
        if app.director_cancelled() {
            return Ok(SpecialistResult::done(format!(
                "Stopped before publishing file approval. Saved new files: {written:?}"
            )));
        }
        let summaries: Vec<String> = staged
            .iter()
            .map(|(_, _, summary)| summary.clone())
            .collect();
        let id = stage_authorized_text_writes(
            specialist,
            staged
                .into_iter()
                .map(|(write, body, _)| (write, body))
                .collect(),
        );
        let card = prepare_text_write_approval_card(&id, &summaries, project_root)?;
        if app.director_cancelled() {
            return Ok(SpecialistResult::done(format!(
                "Stopped before publishing file approval. Saved new files: {written:?}"
            )));
        }
        let card = crate::pending::publish_user_text(&card);
        let mut message = format!(
            "{label} prepared {} file change(s) for approval.",
            summaries.len()
        );
        if !written.is_empty() {
            message.push_str("\n\nSaved new files:\n");
            for path in &written {
                message.push_str(&format!("- `{path}`\n"));
            }
        }
        message.push_str("\n\n");
        message.push_str(&card);
        return Ok(SpecialistResult::done(message));
    }

    let mut message = emit
        .summary
        .unwrap_or_else(|| format!("{label} wrote {} file(s).", written.len()));
    if !written.is_empty() {
        message.push_str("\n\n---\nSaved:\n");
        for path in &written {
            message.push_str(&format!("- `{path}`\n"));
        }
    }
    if let Some(ask) = emit.ask {
        message.push_str("\n");
        message.push_str(&ask);
    }

    Ok(SpecialistResult::done(message))
}

fn run_text_write(
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    let staged_id = input_str(&validated.action.inputs, &["staged_id"])
        .ok_or_else(|| "text_write needs the host staged_id".to_string())?;
    let written = commit_staged_text_write(staged_id, project_root)?;
    let mut message = format!("Approved {} text file(s).", written.len());
    if !written.is_empty() {
        message.push_str("\n\nSaved:\n");
        for path in &written {
            message.push_str(&format!("- `{path}`\n"));
        }
    }
    Ok(SpecialistResult::done(message))
}

fn run_production_helper(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    llm: &SpecialistLlm<'_>,
) -> Result<SpecialistResult, String> {
    let inputs = &validated.action.inputs;
    let question = input_str(inputs, &["question", "brief", "prompt", "message"])
        .unwrap_or(llm.user_question)
        .trim();
    if question.is_empty() {
        return Err(
            "production_helper needs a question/brief (or a non-empty user message).".into(),
        );
    }

    let mut paths = input_path_list(
        inputs,
        &[
            "paths",
            "source_paths",
            "documents",
            "files",
            "path",
            "source_path",
            "handoff_path",
            "character_files",
        ],
    );
    let explicit_sources = !paths.is_empty();
    let output_path = input_str(inputs, &["output_path", "save_path"]);
    // Compare resolved locations, not spellings (relative, absolute, and aliases).
    // Peeking does not consume a one-time file grant.
    let output_location =
        output_path.and_then(|out| crate::media_util::peek_path(out, project_root).ok());
    let is_output = |raw: &str| {
        output_path == Some(raw)
            || output_location.as_ref().is_some_and(|out| {
                crate::media_util::peek_path(raw, project_root).is_ok_and(|path| &path == out)
            })
    };
    if !explicit_sources {
        for path in context
            .attached_paths
            .iter()
            .map(String::as_str)
            .chain(context.selected_path.as_deref())
            .chain(context.focused_editor_path.as_deref())
        {
            let path = path.trim();
            if !path.is_empty() && !paths.iter().any(|p| p == path) {
                paths.push(path.to_string());
            }
        }
    }
    // A destination is never implicit input, even if the planner repeats it in
    // a source alias. To review the previous digest, use a distinct output path.
    paths.retain(|path| !is_output(path));

    if paths.len() > HELPER_MAX_FILES {
        return Err(format!(
            "Production Helper received {} source documents; the limit is {HELPER_MAX_FILES}. Select fewer sources; no digest was generated or saved.",
            paths.len()
        ));
    }

    app.emit(
        "director-progress",
        format!("Production Helper reading {} file(s)…", paths.len()),
    );

    let mut dossier = String::new();
    let mut total_chars = 0usize;
    let mut read_labels: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for raw in &paths {
        let resolved = match crate::media_util::resolve_path(raw, project_root) {
            Ok(p) => p,
            Err(e) => {
                missing.push(format!("{raw} ({e})"));
                continue;
            }
        };
        if !resolved.is_file() {
            missing.push(format!("{} (not a file)", resolved.display()));
            continue;
        }
        let bytes = fs::metadata(&resolved).map(|m| m.len()).unwrap_or(0);
        // Skip obvious binaries / huge media.
        if bytes > 2_000_000 {
            missing.push(format!(
                "{} ({} bytes — too large for helper; attach a text doc or excerpt)",
                resolved.display(),
                bytes
            ));
            continue;
        }
        let text = match fs::read_to_string(&resolved) {
            Ok(t) => t,
            Err(e) => {
                missing.push(format!("{} ({e})", resolved.display()));
                continue;
            }
        };
        let mut body = text;
        if body.len() > HELPER_MAX_CHARS_PER_FILE {
            let mut end = HELPER_MAX_CHARS_PER_FILE;
            while !body.is_char_boundary(end) {
                end -= 1;
            }
            body.truncate(end);
            body.push_str("\n\n…[truncated for helper context]…\n");
        }
        if total_chars + body.len() > HELPER_MAX_TOTAL_CHARS {
            missing.push(format!(
                "{} (skipped — helper context budget full)",
                resolved.display()
            ));
            break;
        }
        total_chars += body.len();
        let label = resolved.display().to_string();
        read_labels.push(label.clone());
        dossier.push_str(&format!("### File: `{label}`\n\n```\n{body}\n```\n\n"));
    }

    if let Some(excerpt) = context
        .focused_editor_excerpt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|_| !explicit_sources)
        .filter(|_| {
            !context
                .focused_editor_path
                .as_deref()
                .is_some_and(is_output)
        })
    {
        if !dossier.contains(excerpt) {
            let mut clip = excerpt.to_string();
            if clip.len() > 8_000 {
                let mut end = 8_000;
                while !clip.is_char_boundary(end) {
                    end -= 1;
                }
                clip.truncate(end);
                clip.push_str("\n…[excerpt truncated]…");
            }
            dossier.push_str(&format!(
                "### Focused editor excerpt\n\n```\n{clip}\n```\n\n"
            ));
        }
    }

    if dossier.trim().is_empty() && missing.is_empty() {
        dossier.push_str(
            "(No project files were attached. Answer from the question alone, and ask which paths to open if you need them.)\n",
        );
    }

    let mode = input_str(inputs, &["mode"]).unwrap_or("review");
    // Do not turn a failed or partial read into model prose that can be saved
    // (or staged for approval) as if it were a grounded document digest.
    if !missing.is_empty() {
        return Err(format!(
            "Production Helper could not load all source documents; no digest was generated or saved:\n- {}",
            missing.join("\n- ")
        ));
    }
    if read_labels.is_empty()
        && (explicit_sources || !paths.is_empty() || mode == "digest" || output_path.is_some())
    {
        return Err(
            "Production Helper needs readable source documents distinct from the output destination; no digest was generated or saved."
                .into(),
        );
    }

    let mut user_block = String::new();
    user_block.push_str(&format!("## Mode hint\n{mode}\n\n"));
    user_block.push_str(&format!("## User question\n{question}\n\n"));
    if let Some(root) = project_root {
        user_block.push_str(&format!("## Project root\n`{root}`\n\n"));
    }
    if !read_labels.is_empty() {
        user_block.push_str("## Files provided by host\n");
        for label in &read_labels {
            user_block.push_str(&format!("- `{label}`\n"));
        }
        user_block.push('\n');
    }
    if let Some(out) = output_path {
        user_block.push_str(&format!(
            "## Optional digest output path\nThe host saves a digest only when `{out}` is under `Research/**/*.md`. Structure the reply as digest markdown when mode is digest or the user asked for a digest file.\n\n"
        ));
    }
    user_block.push_str("## Document dossier\n\n");
    user_block.push_str(&dossier);

    app.emit(
        "director-progress",
        "Production Helper thinking…".to_string(),
    );

    let messages = vec![
        json!({
            "role": "system",
            "content": prompts::PRODUCTION_HELPER_PROMPT,
        }),
        json!({
            "role": "user",
            "content": user_block,
        }),
    ];

    let reply = chat::chat_completion(llm.api, llm.api_key, llm.model, &messages);
    if app.director_cancelled() {
        return Ok(SpecialistResult::done(
            "Stopped before saving or staging the digest.",
        ));
    }
    let reply = reply?.trim().to_string();
    if reply.is_empty() {
        return Err("Production Helper returned an empty reply.".into());
    }

    let mut message = reply.clone();
    if let Some(out) = output_path {
        if app.director_cancelled() {
            return Ok(SpecialistResult::done(
                "Stopped before saving or staging the digest.",
            ));
        }
        let body = format!("{reply}\n");
        match authorize_text_write("production_helper", out, project_root, body.len(), 0, 0) {
            Ok(authorized) => {
                let previous = if authorized.replacing {
                    fs::read_to_string(&authorized.path).ok()
                } else {
                    None
                };
                if approval_required(&authorized) {
                    let summary = approval_summary(&authorized, previous.as_deref(), &body);
                    let approval_message = if authorized.replacing {
                        "\n\nDigest overwrite is waiting for approval.\n\n"
                    } else {
                        "\n\nNew digest file is waiting for approval.\n\n"
                    };
                    let id =
                        stage_authorized_text_writes("production_helper", vec![(authorized, body)]);
                    let card = prepare_text_write_approval_card(&id, &[summary], project_root)?;
                    if app.director_cancelled() {
                        return Ok(SpecialistResult::done(
                            "Stopped before publishing the digest approval.",
                        ));
                    }
                    let card = crate::pending::publish_user_text(&card);
                    message.push_str(approval_message);
                    message.push_str(&card);
                } else {
                    commit_text_write(&authorized, &body, false).map_err(|error| {
                        format!(
                            "Could not write digest to {}: {error}",
                            authorized.path.display()
                        )
                    })?;
                    message.push_str(&format!(
                        "\n\n---\nSaved digest to `{}`.",
                        authorized.path.display()
                    ));
                    app.emit(
                        "director-progress",
                        format!(
                            "Production Helper saved digest → {}",
                            authorized.path.display()
                        ),
                    );
                }
            }
            Err(error) => {
                message.push_str(&format!("\n\nDigest was not saved: {error}"));
            }
        }
    }

    Ok(SpecialistResult::done(message))
}

/// Pick the Settings video slot for a freeform (non-handoff) prompt job.
fn freeform_video_route(
    context: &DirectorContext,
    has_explicit_start_frame: bool,
    has_refs: bool,
) -> Result<(ModelRef, &'static str), String> {
    if has_explicit_start_frame {
        return select_capability(
            context,
            &["video.start_frame", "video.character_reference", "video.location_prop_other_reference"],
        )
        .ok_or_else(|| {
            "Set Settings → Video → Start frame (or Adult start frame when using adult/nsfw routing)."
                .to_string()
        });
    }
    if has_refs {
        return select_capability(
            context,
            &[
                "video.character_reference",
                "video.location_prop_other_reference",
                "video.start_frame",
            ],
        )
        .ok_or_else(|| {
            "Set Settings → Video → Character reference / Location-prop reference / Start frame."
                .to_string()
        });
    }
    select_capability(context, &["video.text_only"]).ok_or_else(|| {
        "Set Settings → Video → Text only (or Adult text only when using adult/nsfw routing)."
            .to_string()
    })
}

fn freeform_video_model(
    context: &DirectorContext,
    has_explicit_start_frame: bool,
    has_refs: bool,
) -> Result<ModelRef, String> {
    freeform_video_route(context, has_explicit_start_frame, has_refs).map(|(model, _)| model)
}

pub(crate) fn approval_model_routes(
    action: &DirectorAction,
    context: &DirectorContext,
    project_root: Option<&str>,
) -> Vec<(String, ModelRef)> {
    let specialist = action.specialist.as_deref().unwrap_or("");
    let inputs = &action.inputs;
    match specialist {
        "still_image" => resolve_still_image_model(context, inputs)
            .ok()
            .map(|(model, _, slot)| vec![(slot.to_string(), model)])
            .unwrap_or_default(),
        "scene_generation" => {
            if input_str(inputs, &["collect_prediction_id"]).is_some() {
                return Vec::new();
            }
            if input_str(inputs, &["source_video"]).is_some() {
                return select_capability(context, &["video.edit"])
                    .map(|(model, slot)| vec![(slot.to_string(), model)])
                    .unwrap_or_default();
            }
            let has_handoff_input = input_str(inputs, &["handoff", "handoff_path"]).is_some();
            let has_freeform_job = input_str(inputs, &["prompt", "brief", "edit_brief"]).is_some()
                && input_str(inputs, &["output_path", "save_path"]).is_some();
            if !has_handoff_input && has_freeform_job {
                let has_start =
                    input_str(inputs, &["source_frame", "source_image", "start_frame"]).is_some();
                let has_refs =
                    !input_path_list(inputs, &["reference_images", "reference_assets"]).is_empty();
                return freeform_video_route(context, has_start, has_refs)
                    .ok()
                    .map(|(model, slot)| vec![(slot.to_string(), model)])
                    .unwrap_or_default();
            }
            let handoff = approval_handoff_text(inputs, context, project_root);
            let Some(handoff) = handoff else {
                return Vec::new();
            };
            let Ok(request) = RenderRequest::from_document(&handoff) else {
                return Vec::new();
            };
            let explicit_start =
                input_str(inputs, &["source_frame", "source_image", "start_frame"]);
            let workflow_source = if explicit_start.is_none() {
                if let Some(root) = project_root.filter(|root| {
                    Path::new(root)
                        .join(crate::workflow::state::STATE_REL)
                        .is_file()
                }) {
                    let Ok(state) = crate::workflow::import::load_state(Path::new(root)) else {
                        return Vec::new();
                    };
                    match crate::workflow::sources::resolve_render_source(&state, request.id) {
                        Ok(source) => Some(source),
                        Err(_) => return Vec::new(),
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let start_frame =
                explicit_start
                    .map(str::to_string)
                    .or_else(|| match &workflow_source {
                        Some(crate::workflow::sources::RenderSourceResolution::Bound(path)) => {
                            Some(path.clone())
                        }
                        Some(crate::workflow::sources::RenderSourceResolution::NoStartFrame) => {
                            None
                        }
                        Some(crate::workflow::sources::RenderSourceResolution::Unplanned)
                        | None => request.start_frame.clone(),
                    });
            let lane = if start_frame.is_some()
                && matches!(
                    request.lane,
                    RenderLane::TextToVideo | RenderLane::ReferenceToVideo
                ) {
                RenderLane::ImageToVideo
            } else {
                request.lane
            };
            let route = match lane {
                RenderLane::AudioDrivenI2v => {
                    select_capability(context, &["video.lip_sync_text_narration"])
                }
                RenderLane::ImageToVideo => select_capability(context, &["video.start_frame"]),
                RenderLane::ReferenceToVideo => select_capability(
                    context,
                    &[
                        "video.character_reference",
                        "video.location_prop_other_reference",
                    ],
                ),
                RenderLane::TextToVideo => select_capability(context, &["video.text_only"]),
            };
            route
                .map(|(model, slot)| vec![(slot.to_string(), model)])
                .unwrap_or_default()
        }
        "lip_sync" => select_capability(
            context,
            &[
                "video.lip_sync_audio_included",
                "video.lip_sync_text_narration",
            ],
        )
        .map(|(model, slot)| vec![(slot.to_string(), model)])
        .unwrap_or_default(),
        "text_sync" => {
            let mut routes = Vec::new();
            let reuses_audio = input_str(inputs, &["performance_wav", "reuse_audio"]).is_some();
            let clone_audio =
                input_str(inputs, &["voice_clone", "voice_clone_path", "clone_wav"]).is_some();
            let profile_audio =
                input_str(inputs, &["voice_profile", "voice_profile_path"]).is_some();
            if !reuses_audio {
                let audio_slot = if clone_audio {
                    "audio.tts_clone"
                } else if profile_audio {
                    "audio.tts_profile"
                } else {
                    "audio.tts_clone"
                };
                if let Some((model, slot)) = select_capability(context, &[audio_slot]) {
                    routes.push((slot.to_string(), model));
                }
            }
            if let Some((model, slot)) = select_capability(
                context,
                &["video.lip_sync_text_narration", "video.start_frame"],
            ) {
                routes.push((slot.to_string(), model));
            }
            routes
        }
        "model_sheet" => {
            let has_refs =
                !input_path_list(inputs, &["reference_images", "reference_assets"]).is_empty();
            let slots: &[&'static str] = if has_refs {
                &[
                    "image.character_reference",
                    "image.edit",
                    "image.image_to_image",
                ]
            } else {
                &["image.text_to_image"]
            };
            select_capability(context, slots)
                .map(|(model, slot)| vec![(slot.to_string(), model)])
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn approval_handoff_text(
    inputs: &Map<String, Value>,
    context: &DirectorContext,
    project_root: Option<&str>,
) -> Option<String> {
    if let Some(text) = input_str(inputs, &["handoff"]) {
        return Some(text.to_string());
    }
    if let Some(path) = input_str(inputs, &["handoff_path"]) {
        let path = crate::media_util::peek_path(path, project_root).ok()?;
        let metadata = fs::metadata(&path).ok()?;
        if metadata.len() > 2_000_000 {
            return None;
        }
        return fs::read_to_string(path).ok();
    }
    for raw in &context.attached_paths {
        let path = crate::media_util::peek_path(raw, project_root).ok()?;
        let metadata = fs::metadata(&path).ok()?;
        if metadata.len() > 2_000_000 {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let lowered = text.to_ascii_lowercase();
        if lowered.contains("stage:") || lowered.contains("filenames:") {
            return Some(text);
        }
    }
    None
}

fn run_freeform_video(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    prompt: &str,
    save: &str,
) -> Result<SpecialistResult, String> {
    let inputs = &validated.action.inputs;
    let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
    if let Some(parent) = save_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create output dir: {e}"))?;
    }

    let explicit_start =
        input_str(inputs, &["source_frame", "source_image", "start_frame"]).map(|s| s.to_string());
    let mut ref_paths = input_path_list(inputs, &["reference_images", "reference_assets"]);
    if let Some(frame) = explicit_start.clone() {
        if !ref_paths.iter().any(|p| p == &frame) {
            ref_paths.insert(0, frame);
        }
    }
    let has_explicit_start_frame = explicit_start.is_some();
    let has_refs = !ref_paths.is_empty();
    let model = freeform_video_model(context, has_explicit_start_frame, has_refs)?;

    let mut uploaded_urls = Vec::new();
    for path in &ref_paths {
        app.emit(
            "director-progress",
            format!("Uploading video reference {}…", path),
        );
        uploaded_urls.push(upload_local(
            app,
            project_root,
            path,
            crate::media_util::MediaRole::Image,
        )?);
    }

    let duration = input_str(inputs, &["duration"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5);
    let aspect = input_str(inputs, &["aspect_ratio"]).unwrap_or("16:9");
    let mut params = json!({
        "prompt": prompt,
        "duration": duration,
        "aspect_ratio": aspect,
    });
    if !uploaded_urls.is_empty() {
        params["image_urls"] = json!(uploaded_urls.clone());
        params["images"] = json!(uploaded_urls.clone());
        params["media_files"] = json!(uploaded_urls.clone());
        params["image"] = json!(uploaded_urls[0]);
        params["image_url"] = json!(uploaded_urls[0]);
        params["reference_image"] = json!(uploaded_urls[0]);
        if has_explicit_start_frame {
            params["start_image"] = json!(uploaded_urls[0]);
            params["start_image_url"] = json!(uploaded_urls[0]);
            params["starting_visual_input"] = json!(uploaded_urls[0]);
        }
    }

    let lane = if has_explicit_start_frame {
        "start-frame / image-to-video"
    } else if has_refs {
        "reference-to-video"
    } else {
        "text-to-video"
    };

    app.emit(
        "director-progress",
        format!(
            "Generating {lane} with `{}` → {}…",
            model.model_id,
            save_path.display()
        ),
    );
    let api_id = if model.api_id.trim().is_empty() {
        None
    } else {
        Some(model.api_id.as_str())
    };
    let result = run_media_generate(
        app,
        project_root,
        "video",
        &model.model_id,
        api_id,
        params,
        &save_path.to_string_lossy(),
    )?;

    match require_media_saved(&result, &save_path.to_string_lossy()) {
        Ok(summary) => Ok(SpecialistResult::done(format!(
            "Freeform video saved ({lane}; host waited for download).\n\
- specialist: `scene_generation`\n\
- model: `{}`\n\
- references uploaded: {}\n\
{summary}",
            model.model_id,
            uploaded_urls.len()
        ))),
        Err(error) => {
            let parsed: Value = serde_json::from_str(&result).unwrap_or(json!({ "raw": result }));
            let prediction_id = parsed
                .pointer("/submitted/prediction_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if prediction_id.is_empty() {
                return Err(error);
            }
            let mut pending = validated.action.clone();
            pending
                .inputs
                .insert("collect_prediction_id".into(), json!(prediction_id));
            pending.inputs.insert(
                "output_path".into(),
                json!(save_path.to_string_lossy().to_string()),
            );
            Ok(SpecialistResult::cont(format!(
                "{error}\n\n{}",
                encode_pending_action(&pending)
            )))
        }
    }
}

fn run_scene_generation(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    let inputs = &validated.action.inputs;

    // Focused video edit lane
    if let Some(source_video) = input_str(inputs, &["source_video"]) {
        let brief = input_str(inputs, &["edit_brief", "prompt", "brief"])
            .ok_or_else(|| "focused video edit needs edit_brief/prompt".to_string())?;
        let save = input_str(inputs, &["output_path", "save_path"])
            .ok_or_else(|| "focused video edit needs output_path".to_string())?;
        let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
        let model = capability_model(context, "video.edit")
            .ok_or_else(|| "Set Settings → Video → Focused edit before video edits.".to_string())?;
        let uploaded = upload_local(
            app,
            project_root,
            source_video,
            crate::media_util::MediaRole::Video,
        )?;
        let params = json!({
            "prompt": format!("{brief}\n\nPreserve everything else: composition, camera motion, identity, style, lighting, timing, and unmentioned details."),
            "video": uploaded,
            "video_url": uploaded,
        });
        let result = run_media_generate(
            app,
            project_root,
            "video",
            &model.model_id,
            Some(&model.api_id),
            params,
            &save_path.to_string_lossy(),
        )?;
        let summary = require_media_saved(&result, &save_path.to_string_lossy())?;
        return Ok(SpecialistResult::done(format!(
            "Focused video edit saved (host waited for download).\n\
- model: `{}`\n\
{summary}",
            model.model_id
        )));
    }

    // Freeform video lane (no handoff): text-to-video, start-frame I2V, or reference-to-video.
    let has_handoff_input = input_str(inputs, &["handoff", "handoff_path"]).is_some();
    if !has_handoff_input
        && input_str(inputs, &["source_video"]).is_none()
        && input_str(inputs, &["collect_prediction_id"]).is_none()
    {
        if let Some(prompt) = input_str(inputs, &["prompt", "brief", "edit_brief"]) {
            if let Some(save) = input_str(inputs, &["output_path", "save_path"]) {
                return run_freeform_video(app, context, validated, project_root, prompt, save);
            }
        }
    }

    // Resume download for a prediction that timed out inside the wait window.
    if let Some(prediction_id) = input_str(inputs, &["collect_prediction_id"]) {
        let save = input_str(inputs, &["output_path", "save_path"])
            .ok_or_else(|| "collect resume needs output_path".to_string())?;
        let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
        app.emit(
            "director-progress",
            format!("Resuming download for prediction `{prediction_id}`…"),
        );
        let collect_args = json!({
            "prediction_id": prediction_id,
            "path": save_path.to_string_lossy(),
            "wait_seconds": 240,
        });
        let result =
            match media::run_media_tool("atlas_collect_media", &collect_args, project_root, app) {
                Some(Ok(text)) => text,
                Some(Err(error)) => return Err(error),
                None => return Err("Media router ignored atlas_collect_media".into()),
            };
        match require_media_saved(&result, &save_path.to_string_lossy()) {
            Ok(summary) => {
                return Ok(SpecialistResult::done(format!(
                    "Scene generation download finished.\n{summary}"
                )));
            }
            Err(error) => {
                let mut pending = validated.action.clone();
                pending
                    .inputs
                    .insert("collect_prediction_id".into(), json!(prediction_id));
                pending.inputs.insert(
                    "output_path".into(),
                    json!(save_path.to_string_lossy().to_string()),
                );
                return Ok(SpecialistResult::cont(format!(
                    "{error}\n\n{}",
                    encode_pending_action(&pending)
                )));
            }
        }
    }

    // Handoff lane
    let handoff_text = if let Some(raw) = input_str(inputs, &["handoff"]) {
        raw.to_string()
    } else if let Some(path) = input_str(inputs, &["handoff_path"]) {
        let resolved = resolve_under_project(path, project_root).map_err(|e| e.to_string())?;
        fs::read_to_string(&resolved).map_err(|e| format!("Could not read handoff: {e}"))?
    } else {
        // Fall back to first attached path that looks like a handoff
        let mut found = None;
        for path in &context.attached_paths {
            let resolved = resolve_under_project(path, project_root).map_err(|e| e.to_string())?;
            if let Ok(text) = fs::read_to_string(&resolved) {
                if text.to_ascii_lowercase().contains("stage:")
                    || text.to_ascii_lowercase().contains("filenames:")
                {
                    found = Some(text);
                    break;
                }
            }
        }
        found.ok_or_else(|| {
            "scene_generation needs handoff_path, handoff text, or an attached scene handoff file"
                .to_string()
        })?
    };

    let (headers, _) = parse_handoff_headers_and_yaml(&handoff_text);
    let save_path = if let Some(p) = input_str(inputs, &["output_path", "save_path"]) {
        resolve_under_project(p, project_root).map_err(|e| e.to_string())?
    } else {
        join_output(&headers, project_root)?
    };
    if let Some(parent) = save_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create output dir: {e}"))?;
    }

    let request = RenderRequest::from_document(&handoff_text)
        .map_err(|e| format!("Handoff is not a validated scene-plan cut: {e}"))?;
    let prompt = request.prompt.clone();
    let duration = request.duration_seconds;
    let aspect = request.aspect_ratio.as_str();
    let resolution = input_str(inputs, &["resolution"]).unwrap_or("1080p");
    let negative = request.negative_prompt.clone().unwrap_or_default();

    let explicit_start =
        input_str(inputs, &["source_frame", "source_image", "start_frame"]).map(str::to_string);
    let workflow_source = if explicit_start.is_none() {
        project_root
            .filter(|root| {
                Path::new(root)
                    .join(crate::workflow::state::STATE_REL)
                    .is_file()
            })
            .map(|root| {
                let state = crate::workflow::import::load_state(Path::new(root))?;
                crate::workflow::sources::resolve_render_source(&state, request.id)
            })
            .transpose()?
    } else {
        None
    };
    let start_frame = explicit_start.or_else(|| match &workflow_source {
        Some(crate::workflow::sources::RenderSourceResolution::Bound(path)) => Some(path.clone()),
        Some(crate::workflow::sources::RenderSourceResolution::NoStartFrame) => None,
        Some(crate::workflow::sources::RenderSourceResolution::Unplanned) | None => {
            request.start_frame.clone()
        }
    });
    let mut ref_paths = input_path_list(inputs, &["reference_assets", "reference_images"]);
    let explicit_references = !ref_paths.is_empty();
    if ref_paths.is_empty()
        && !matches!(
            &workflow_source,
            Some(crate::workflow::sources::RenderSourceResolution::Bound(_))
        )
    {
        ref_paths = request.reference_images.clone();
    }
    // An approved workflow start frame is the concrete visual source. Do not
    // silently add stale model-sheet references from an older handoff.
    if !explicit_references
        && matches!(
            &workflow_source,
            Some(crate::workflow::sources::RenderSourceResolution::Bound(_))
        )
    {
        ref_paths.clear();
    }
    if let Some(old_start) = &request.start_frame {
        ref_paths.retain(|path| path != old_start);
    }
    if let Some(start_frame) = &start_frame {
        ref_paths.retain(|path| path != start_frame);
        ref_paths.insert(0, start_frame.clone());
    }

    let lane = if start_frame.is_some()
        && matches!(
            request.lane,
            RenderLane::TextToVideo | RenderLane::ReferenceToVideo
        ) {
        RenderLane::ImageToVideo
    } else {
        request.lane
    };
    let model = match lane {
        RenderLane::AudioDrivenI2v => capability_model(context, "video.lip_sync_text_narration")
            .ok_or_else(|| "Set Settings → Video → Lip sync for audio-driven cuts.".to_string())?,
        RenderLane::ImageToVideo => capability_model(context, "video.start_frame")
            .ok_or_else(|| "Set Settings → Video → Start frame.".to_string())?,
        RenderLane::ReferenceToVideo => capability_model(context, "video.character_reference")
            .or_else(|| capability_model(context, "video.location_prop_other_reference"))
            .ok_or_else(|| "Set Settings → Video → Character reference.".to_string())?,
        RenderLane::TextToVideo => capability_model(context, "video.text_only")
            .ok_or_else(|| "Set Settings → Video → Text only.".to_string())?,
    };

    let mut uploaded_urls = Vec::new();
    for path in &ref_paths {
        app.emit(
            "director-progress",
            format!("Uploading reference {}…", path),
        );
        uploaded_urls.push(upload_local(
            app,
            project_root,
            path,
            crate::media_util::MediaRole::Image,
        )?);
    }

    // Include common aliases; atlas::generate_media remaps to the model schema
    // (e.g. Grok reference-to-video → image_urls) and drops unsupported keys.
    let mut params = json!({
        "prompt": prompt,
        "duration": duration,
        "aspect_ratio": aspect,
        "resolution": resolution,
    });
    if !negative.is_empty() {
        params["negative_prompt"] = json!(negative);
    }
    if !uploaded_urls.is_empty() {
        params["image_urls"] = json!(uploaded_urls.clone());
        params["images"] = json!(uploaded_urls.clone());
        params["media_files"] = json!(uploaded_urls.clone());
        params["image"] = json!(uploaded_urls[0]);
        params["image_url"] = json!(uploaded_urls[0]);
        params["reference_image"] = json!(uploaded_urls[0]);
        if start_frame.is_some() {
            params["start_image"] = json!(uploaded_urls[0]);
            params["start_image_url"] = json!(uploaded_urls[0]);
            params["starting_visual_input"] = json!(uploaded_urls[0]);
        }
    }

    app.emit(
        "director-progress",
        format!(
            "Generating scene video with `{}` → {}…",
            model.model_id,
            save_path.display()
        ),
    );
    let api_id = if model.api_id.trim().is_empty() {
        None
    } else {
        Some(model.api_id.as_str())
    };
    let result = run_media_generate(
        app,
        project_root,
        "video",
        &model.model_id,
        api_id,
        params,
        &save_path.to_string_lossy(),
    )?;

    match require_media_saved(&result, &save_path.to_string_lossy()) {
        Ok(summary) => Ok(SpecialistResult::done(format!(
            "Scene generation saved (host waited for Atlas to finish + download).\n\
- specialist: `scene_generation`\n\
- model: `{}`\n\
- references uploaded: {}\n\
{summary}",
            model.model_id,
            uploaded_urls.len()
        ))),
        Err(error) => {
            // Keep a resumable collect pending so "continue" polls the same prediction.
            let parsed: Value = serde_json::from_str(&result).unwrap_or(json!({ "raw": result }));
            let prediction_id = parsed
                .pointer("/submitted/prediction_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if prediction_id.is_empty() {
                return Err(error);
            }
            let mut pending = validated.action.clone();
            pending
                .inputs
                .insert("collect_prediction_id".into(), json!(prediction_id));
            pending.inputs.insert(
                "output_path".into(),
                json!(save_path.to_string_lossy().to_string()),
            );
            Ok(SpecialistResult::cont(format!(
                "{error}\n\n{}",
                encode_pending_action(&pending)
            )))
        }
    }
}

/// Resolve primary edit source + extra refs. `source_frame` may be a string or an
/// array (Image 1, Image 2, …) — first path is the primary source, the rest are refs.
fn still_image_sources(inputs: &Map<String, Value>) -> (Option<String>, Vec<String>) {
    let mut frames = input_path_list(inputs, &["source_frame", "source_image"]);
    if frames.is_empty() {
        if let Some(s) = input_str(inputs, &["source_frame", "source_image"]) {
            frames.push(s.to_string());
        }
    }
    let mut refs = input_path_list(inputs, &["reference_images", "reference_assets"]);
    let source = frames.first().cloned();
    if frames.len() > 1 {
        refs.splice(0..0, frames.into_iter().skip(1));
    }
    (source, refs)
}

pub(crate) fn resolve_still_image_model(
    context: &DirectorContext,
    inputs: &Map<String, Value>,
) -> Result<(ModelRef, &'static str, &'static str), String> {
    let (source, refs) = still_image_sources(inputs);
    if source.is_some() {
        let (model, slot) = select_capability(context, &["image.edit", "image.image_to_image"])
            .ok_or_else(|| {
                "Set Settings → Image → Focused edit (or Image to image).".to_string()
            })?;
        return Ok((model, "focused edit / multi-image edit", slot));
    }
    if !refs.is_empty() {
        let (model, slot) = select_capability(
            context,
            &[
                "image.character_reference",
                "image.image_to_image",
                "image.text_to_image",
            ],
        )
        .ok_or_else(|| "Set an Image generation default in Settings.".to_string())?;
        return Ok((model, "reference-conditioned still", slot));
    }
    let model = capability_model(context, "image.text_to_image").ok_or_else(|| {
        "Set Settings → Image → Text to image before generating stills.".to_string()
    })?;
    Ok((model, "text-to-image", "image.text_to_image"))
}

/// Host-authored approval notes so the user can see the exact model before spend.
pub fn still_image_approval_notes(
    context: &DirectorContext,
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
) -> String {
    if let Some(items) = inputs.get("batch").and_then(|value| value.as_array()) {
        return still_image_batch_approval_notes(context, items, project_root);
    }
    let (source, refs) = still_image_sources(inputs);
    let out = input_str(inputs, &["output_path", "save_path"]).unwrap_or("(output_path)");
    let brief = input_str(inputs, &["edit_brief", "prompt", "brief"]).unwrap_or("(brief)");
    let (model, lane, slot) = match resolve_still_image_model(context, inputs) {
        Ok(v) => v,
        Err(e) => {
            return format!("### Still image plan\n- model: **unset** ({e})\n- output: `{out}`");
        }
    };
    let routing = if context.adult_routing_requested {
        format!("- routing: **adult/nsfw** (Adult `{slot}` when set)\n")
    } else {
        "- routing: main Generation defaults\n".to_string()
    };
    let mut body = format!(
        "### Still image plan\n\
{routing}\
- **model:** `{model_id}`\n\
- Settings slot: `{slot}` ({lane})\n\
- output: `{out}`\n",
        model_id = model.model_id
    );
    if let Some(src) = source {
        let resolved = resolve_under_project(&src, project_root)
            .map(|p| p.display().to_string())
            .unwrap_or(src);
        body.push_str(&format!("- Image 1 (source): `{resolved}`\n"));
    }
    for (i, r) in refs.iter().enumerate() {
        let resolved = resolve_under_project(r, project_root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| r.clone());
        body.push_str(&format!("- Image {}: `{resolved}`\n", i + 2));
    }
    body.push_str(&format!("\nBrief:\n```\n{brief}\n```\n"));
    body.push_str(
        "\nUse the Approve button to generate with this Settings model, or change the slot in Settings.",
    );
    let _ = project_root;
    body
}

fn still_image_batch_approval_notes(
    context: &DirectorContext,
    items: &[Value],
    project_root: Option<&str>,
) -> String {
    let first = items.first().and_then(|item| item.as_object());
    let (model_line, slot_line) = match first {
        Some(item) => match resolve_still_image_model(context, item) {
            Ok((model, lane, slot)) => (
                format!("- **model:** `{}`\n", model.model_id),
                format!("- Settings slot: `{slot}` ({lane})\n"),
            ),
            Err(error) => (format!("- model: **unset** ({error})\n"), String::new()),
        },
        None => ("- model: **unset** (empty batch)\n".into(), String::new()),
    };
    let routing = if context.adult_routing_requested {
        "- routing: **adult/nsfw** (Adult image slot when set)\n"
    } else {
        "- routing: main Generation defaults\n"
    };
    let mut body = format!(
        "### Still image plan\n\
{routing}\
{model_line}\
{slot_line}\
- stills: {}\n",
        items.len()
    );
    for (index, item) in items.iter().enumerate() {
        let output = item
            .get("output_path")
            .and_then(|value| value.as_str())
            .unwrap_or("(output_path)");
        body.push_str(&format!("- {index}. `{output}`\n"));
    }
    body.push_str(
        "\nUse the Approve button to generate every still in this plan with the Settings model, or change the slot in Settings.",
    );
    let _ = project_root;
    body
}

fn run_still_image_batch(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    let Some(items) = validated
        .action
        .inputs
        .get("batch")
        .and_then(|value| value.as_array())
        .cloned()
    else {
        return Err("still_image batch must be a list of stills".into());
    };
    let mut collect = validated
        .action
        .inputs
        .get("collect_prediction_id")
        .cloned();
    let mut pending_items = items;
    let mut saved = Vec::new();
    while !pending_items.is_empty() {
        let Some(item) = pending_items[0].as_object().cloned() else {
            return Err("still_image batch item must be an object".into());
        };
        let mut one = validated.clone();
        one.action.inputs = item;
        one.action.inputs.remove("batch");
        if let Some(prediction_id) = collect.take() {
            one.action
                .inputs
                .insert("collect_prediction_id".into(), prediction_id);
        }
        match run_still_image(app, context, &one, project_root) {
            Ok(result) if result.continue_suggested => {
                let pending = find_pending_action_in_history(&[(
                    "assistant".to_string(),
                    result.message.clone(),
                )]);
                let Some(pending) = pending else {
                    return Ok(result);
                };
                let mut first = pending.inputs;
                let prediction = first.remove("collect_prediction_id");
                let mut batch = vec![Value::Object(first)];
                batch.extend(pending_items.into_iter().skip(1));
                let mut action = validated.action.clone();
                action.inputs = Map::new();
                action.inputs.insert("batch".into(), Value::Array(batch));
                if let Some(prediction) = prediction {
                    action
                        .inputs
                        .insert("collect_prediction_id".into(), prediction);
                }
                let notice = result
                    .message
                    .split(PENDING_ACTION_START)
                    .next()
                    .unwrap_or(result.message.as_str())
                    .trim();
                let mut message = notice.to_string();
                if !saved.is_empty() {
                    message.push_str("\n\nSaved so far:\n");
                    message.push_str(&saved.join("\n\n"));
                }
                message.push_str("\n\n");
                message.push_str(&encode_pending_action(&action));
                return Ok(SpecialistResult::cont(message));
            }
            Ok(result) => {
                saved.push(result.message);
                pending_items.remove(0);
            }
            Err(error) => {
                if saved.is_empty() {
                    return Err(error);
                }
                return Err(format!("{error}\n\nAlready saved:\n{}", saved.join("\n\n")));
            }
        }
    }
    Ok(SpecialistResult::done(format!(
        "Still image batch finished ({} stills).\n\n{}",
        saved.len(),
        saved.join("\n\n")
    )))
}

fn run_still_image(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    let inputs = &validated.action.inputs;
    if inputs
        .get("batch")
        .and_then(|value| value.as_array())
        .is_some()
    {
        return run_still_image_batch(app, context, validated, project_root);
    }

    // Resume download for a prediction that timed out inside the wait window.
    if let Some(prediction_id) = input_str(inputs, &["collect_prediction_id"]) {
        let save = input_str(inputs, &["output_path", "save_path"])
            .ok_or_else(|| "still_image resume needs output_path".to_string())?;
        let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
        return maybe_register_workflow_still(
            project_root,
            &save_path.to_string_lossy(),
            resume_media_collect(
                app,
                validated,
                project_root,
                prediction_id,
                &save_path,
                "output_path",
                "Still image",
            )?,
        );
    }

    let prompt = input_str(inputs, &["edit_brief", "prompt", "brief"])
        .ok_or_else(|| "still_image needs prompt/brief/edit_brief".to_string())?;
    let save = input_str(inputs, &["output_path", "save_path"])
        .ok_or_else(|| "still_image needs output_path".to_string())?;
    let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
    if let Some(parent) = save_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create output dir: {e}"))?;
    }

    let (source, refs) = still_image_sources(inputs);
    let preserve = inputs
        .get("preserve_props")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let (model, _lane, _slot) = resolve_still_image_model(context, inputs)?;

    let mut params = if source.is_some() && preserve {
        json!({
            "prompt": format!("{prompt}\n\nPreserve everything else: composition, camera, identity, style, lighting, and unmentioned details."),
        })
    } else {
        json!({ "prompt": prompt })
    };
    if let Some(ar) = input_str(inputs, &["aspect_ratio"]) {
        params["aspect_ratio"] = json!(ar);
        if source.is_none() && !refs.is_empty() {
            let prompt = params
                .get("prompt")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            params["prompt"] = json!(format!(
                "{prompt}\n\nOutput frame is {ar}. Keep that aspect ratio. Do not copy the aspect ratio of the reference images."
            ));
        }
    }
    if let Some(res) = input_str(inputs, &["resolution"]) {
        params["resolution"] = json!(res);
    }

    let mut uploaded = Vec::new();
    if let Some(ref src) = source {
        let url = upload_local(app, project_root, src, crate::media_util::MediaRole::Image)?;
        uploaded.push(url.clone());
        params["image"] = json!(url);
        params["image_url"] = json!(url);
    }
    for path in &refs {
        uploaded.push(upload_local(
            app,
            project_root,
            path,
            crate::media_util::MediaRole::Image,
        )?);
    }
    if !uploaded.is_empty() {
        params["media_files"] = json!(uploaded.clone());
        params["images"] = json!(uploaded.clone());
        params["image_urls"] = json!(uploaded.clone());
        if !params.as_object().unwrap().contains_key("image") {
            params["image"] = json!(uploaded[0]);
            params["image_url"] = json!(uploaded[0]);
        }
    }

    let api_id = if model.api_id.trim().is_empty() {
        None
    } else {
        Some(model.api_id.as_str())
    };
    let result = run_media_generate(
        app,
        project_root,
        "image",
        &model.model_id,
        api_id,
        params,
        &save_path.to_string_lossy(),
    )?;
    maybe_register_workflow_still(
        project_root,
        &save_path.to_string_lossy(),
        finish_media_or_resume(
            validated,
            "Still image",
            &model.model_id,
            &save_path.to_string_lossy(),
            &result,
            "output_path",
        )?,
    )
}

fn maybe_register_workflow_still(
    project_root: Option<&str>,
    save_path: &str,
    mut result: SpecialistResult,
) -> Result<SpecialistResult, String> {
    if !result.continue_suggested {
        if let Some(note) = crate::workflow::look::on_wardrobe_plate_saved(project_root, save_path)
        {
            result.message.push_str("\n\n");
            result.message.push_str(&note);
        }
        if let Some(note) = crate::workflow::look::on_still_saved(project_root, save_path) {
            result.message.push_str("\n\n");
            result.message.push_str(&note);
        }
        if let Some(note) = crate::workflow::start_frames::on_still_saved(project_root, save_path) {
            result.message.push_str("\n\n");
            result.message.push_str(&note);
        }
    }
    Ok(result)
}

fn extract_yaml_body(text: &str) -> String {
    let Some(start) = text.find("```yaml").or_else(|| text.find("```yml")) else {
        return text.to_string();
    };
    let mut body = String::new();
    for line in text[start..].lines().skip(1) {
        if line.trim().starts_with("```") {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    if body.trim().is_empty() {
        text.to_string()
    } else {
        body
    }
}

fn yaml_f64_field(body: &str, key: &str) -> Option<f64> {
    let prefix = format!("{key}:");
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&prefix) {
            let v = rest.trim().trim_matches('"');
            if let Ok(n) = v.parse::<f64>() {
                return Some(n);
            }
        }
    }
    None
}

fn yaml_string_field(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&prefix) {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() && v != "|" && v != "|-" && v != "[" {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// First on-camera cue text + delivery from a talking-shot handoff.
fn extract_first_speech_cue(body: &str) -> Option<(String, String)> {
    let cues_at = body.find("cues:")?;
    let after = &body[cues_at..];
    let mut text: Option<String> = None;
    let mut delivery = String::new();
    for line in after.lines().skip(1) {
        let t = line.trim();
        if t.starts_with("generation_requirements:")
            || t.starts_with("prompt:")
            || t.starts_with("reference_assets:")
            || t.starts_with("negative_prompt:")
            || t.starts_with("generation_notes:")
        {
            break;
        }
        if text.is_none() {
            if let Some(rest) = t.strip_prefix("text:") {
                let v = rest.trim().trim_matches('"').to_string();
                if !v.is_empty() {
                    text = Some(v);
                }
            }
        }
        if delivery.is_empty() {
            if let Some(rest) = t.strip_prefix("delivery:") {
                delivery = rest.trim().trim_matches('"').to_string();
            }
        }
        if text.is_some() && !delivery.is_empty() {
            break;
        }
    }
    text.map(|t| (t, delivery))
}

fn extract_reference_paths_from_body(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("- path:") {
            let p = rest.trim().trim_matches('"');
            if !p.is_empty() {
                out.push(p.to_string());
            }
        } else if let Some(rest) = t.strip_prefix("path:") {
            let p = rest.trim().trim_matches('"');
            if p.contains('/') {
                out.push(p.to_string());
            }
        } else if let Some(rest) = t.strip_prefix("- ") {
            let p = rest.trim().trim_matches('"');
            if p.ends_with(".png")
                || p.ends_with(".jpg")
                || p.ends_with(".jpeg")
                || p.ends_with(".webp")
            {
                out.push(p.to_string());
            }
        }
    }
    out
}

fn parse_filename_tokens(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in raw.chars() {
        match ch {
            '"' => {
                if in_quote {
                    if !current.trim().is_empty() {
                        out.push(current.trim().to_string());
                    }
                    current.clear();
                    in_quote = false;
                } else {
                    in_quote = true;
                }
            }
            _ if in_quote => current.push(ch),
            _ => {}
        }
    }
    if out.is_empty() {
        for part in raw.split([' ', ',', '&']) {
            let p = part.trim().trim_matches('"');
            if p.ends_with(".mp4")
                || p.ends_with(".webm")
                || p.ends_with(".wav")
                || p.ends_with(".mp3")
            {
                out.push(p.to_string());
            }
        }
    }
    out
}

fn join_output_pair(
    headers: &HashMap<String, String>,
    project_root: Option<&str>,
) -> Result<(Option<PathBuf>, Option<PathBuf>), String> {
    let output = headers
        .get("output")
        .ok_or_else(|| "Handoff missing Output header".to_string())?;
    let dir = resolve_under_project(output, project_root).map_err(|e| e.to_string())?;
    let names = headers
        .get("filenames")
        .map(|s| parse_filename_tokens(s))
        .unwrap_or_default();
    let mut video = None;
    let mut audio = None;
    for name in names {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".mp4") || lower.ends_with(".webm") || lower.ends_with(".mov") {
            video = Some(dir.join(&name));
        } else if lower.ends_with(".wav") || lower.ends_with(".mp3") {
            audio = Some(dir.join(&name));
        }
    }
    Ok((video, audio))
}

fn rewrite_talking_prompt(prompt: &str, head_s: f64, tail_s: f64) -> String {
    let lower = prompt.to_ascii_lowercase();
    let mut out = prompt.trim().to_string();
    if head_s > 0.0 || tail_s > 0.0 {
        let hold = format!(
            "Hold a settled pose with a quiet, nearly closed mouth for about {head_s:.1} second(s) before speech begins; after the last word, hold that settle for about {tail_s:.1} second(s). Do not mouth dialogue until the driving audio starts speaking. Natural speaking performance driven by the supplied audio; camera-facing unobstructed mouth; no zoom, no push-in, no captions."
        );
        if !lower.contains("before speech begins") && !lower.contains("driving audio") {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&hold);
        }
    } else if !lower.contains("driven by") && !lower.contains("driving audio") {
        out.push_str(
            " Natural speaking performance driven by the supplied audio; camera-facing unobstructed mouth; no captions.",
        );
    }
    out
}

fn load_voice_profile_fields(
    path: &Path,
) -> Result<(String, String, String, Map<String, Value>), String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("Could not read voice profile: {e}"))?;
    let model_id = yaml_string_field(&text, "model_id").unwrap_or_default();
    let voice_id = yaml_string_field(&text, "voice_id")
        .or_else(|| yaml_string_field(&text, "voice_name"))
        .unwrap_or_default();
    let language = yaml_string_field(&text, "language").unwrap_or_default();
    let mut settings = Map::new();
    if let Some(rest) = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("settings:")
            .map(|s| s.trim().to_string())
    }) {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&rest) {
            settings = map;
        }
    }
    Ok((model_id, voice_id, language, settings))
}

fn build_seed_tts_text(cue_text: &str, delivery: &str) -> String {
    let delivery = delivery.trim();
    if delivery.is_empty() {
        format!("Use the voice and delivery of @audio1 and say: {cue_text}")
    } else {
        format!("Use the voice and delivery of @audio1 and say in a {delivery} tone: {cue_text}")
    }
}

fn run_text_sync(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    use crate::fs_ops::{
        ensure_talking_still_9x16, pad_audio_handles, probe_duration_seconds, probe_image_size,
    };

    let inputs = &validated.action.inputs;

    // Resume download for a timed-out video prediction.
    if let Some(prediction_id) = input_str(inputs, &["collect_prediction_id"]) {
        let save = input_str(inputs, &["output_video", "output_path", "save_path"])
            .ok_or_else(|| "text_sync resume needs output_video/output_path".to_string())?;
        let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
        app.emit(
            "director-progress",
            format!("Resuming text-sync video download for `{prediction_id}`…"),
        );
        let collect_args = json!({
            "prediction_id": prediction_id,
            "path": save_path.to_string_lossy(),
            "wait_seconds": 240,
        });
        let result =
            match media::run_media_tool("atlas_collect_media", &collect_args, project_root, app) {
                Some(Ok(text)) => text,
                Some(Err(error)) => return Err(error),
                None => return Err("Media router ignored atlas_collect_media".into()),
            };
        match require_media_saved(&result, &save_path.to_string_lossy()) {
            Ok(summary) => {
                return Ok(SpecialistResult::done(format!(
                    "Text-sync video download finished.\n{summary}"
                )));
            }
            Err(error) => {
                let mut pending = validated.action.clone();
                pending
                    .inputs
                    .insert("collect_prediction_id".into(), json!(prediction_id));
                pending.inputs.insert(
                    "output_video".into(),
                    json!(save_path.to_string_lossy().to_string()),
                );
                return Ok(SpecialistResult::cont(format!(
                    "{error}\n\n{}",
                    encode_pending_action(&pending)
                )));
            }
        }
    }

    let handoff_text = if let Some(raw) = input_str(inputs, &["handoff"]) {
        raw.to_string()
    } else if let Some(path) = input_str(inputs, &["handoff_path"]) {
        let resolved = resolve_under_project(path, project_root).map_err(|e| e.to_string())?;
        fs::read_to_string(&resolved).map_err(|e| format!("Could not read handoff: {e}"))?
    } else {
        return Err("text_sync needs handoff_path or handoff text".into());
    };

    let (headers, yaml) = parse_handoff_headers_and_yaml(&handoff_text);
    let yaml = yaml.unwrap_or(json!({}));
    let body = extract_yaml_body(&handoff_text);

    let (cue_text, delivery) = extract_first_speech_cue(&body).ok_or_else(|| {
        "text_sync handoff missing speech.cues[].text (visible speaker cue)".to_string()
    })?;

    let head_s = yaml_f64_field(&body, "head_handle_seconds").unwrap_or(1.0);
    let tail_s = yaml_f64_field(&body, "tail_handle_seconds").unwrap_or(1.0);
    let sync_ms = input_str(inputs, &["sync_ms"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(150);
    let aspect = yaml
        .get("aspect_ratio")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| yaml_string_field(&body, "aspect_ratio"))
        .unwrap_or_else(|| "9:16".into());
    let negative = yaml
        .get("negative_prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| yaml_string_field(&body, "negative_prompt"))
        .unwrap_or_default();
    let prompt_raw = yaml
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if prompt_raw.trim().is_empty() {
        return Err("text_sync handoff YAML missing prompt".into());
    }
    let prompt = rewrite_talking_prompt(&prompt_raw, head_s, tail_s);

    let (header_video, header_audio) = join_output_pair(&headers, project_root)?;
    let output_video =
        if let Some(p) = input_str(inputs, &["output_video", "output_path", "save_path"]) {
            resolve_under_project(p, project_root).map_err(|e| e.to_string())?
        } else {
            header_video.ok_or_else(|| {
                "text_sync needs output_video/output_path or a Filenames *.mp4 in the handoff"
                    .to_string()
            })?
        };
    let output_audio = if let Some(p) = input_str(inputs, &["output_audio"]) {
        resolve_under_project(p, project_root).map_err(|e| e.to_string())?
    } else if let Some(p) = header_audio {
        p
    } else {
        let stem = output_video
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("talking");
        output_video
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!("{stem}.wav"))
    };
    if let Some(parent) = output_video.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create video dir: {e}"))?;
    }
    if let Some(parent) = output_audio.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create audio dir: {e}"))?;
    }

    // Character / start image
    let mut character_image =
        input_str(inputs, &["character_image", "source_frame", "source_image"])
            .map(|s| s.to_string());
    if character_image.is_none() {
        if let Some(root) = project_root.filter(|root| {
            Path::new(root)
                .join(crate::workflow::state::STATE_REL)
                .is_file()
        }) {
            let request = RenderRequest::from_document(&handoff_text)
                .map_err(|error| format!("Text-sync handoff is not a validated cut: {error}"))?;
            let state = crate::workflow::import::load_state(Path::new(root))?;
            match crate::workflow::sources::resolve_render_source(&state, request.id)? {
                crate::workflow::sources::RenderSourceResolution::Bound(path) => {
                    character_image = Some(path);
                }
                crate::workflow::sources::RenderSourceResolution::NoStartFrame => {
                    return Err(format!(
                        "Cut {} uses `no_start_frame`, but text-sync requires an image source. Change its execution strategy or visual-source strategy.",
                        request.id.cut
                    ));
                }
                crate::workflow::sources::RenderSourceResolution::Unplanned => {}
            }
        }
    }
    if character_image.is_none() {
        if let Some(cr) = headers.get("character_reference") {
            if cr != "none" && !cr.trim().is_empty() {
                character_image = Some(cr.clone());
            }
        }
    }
    if character_image.is_none() {
        let mut refs = input_path_list(inputs, &["reference_images", "reference_assets"]);
        refs.extend(extract_reference_paths_from_body(&body));
        if let Some(Value::Array(items)) = yaml.get("reference_assets") {
            for item in items {
                if let Some(s) = item.as_str() {
                    refs.push(s.to_string());
                }
            }
        }
        character_image = refs.into_iter().next();
    }
    let character_image = character_image.ok_or_else(|| {
        "text_sync needs character_image / Character_Reference / reference_assets still".to_string()
    })?;
    let character_path =
        resolve_under_project(&character_image, project_root).map_err(|e| e.to_string())?;
    if !character_path.is_file() {
        return Err(format!(
            "Character/start image not found: {}",
            character_path.display()
        ));
    }

    crate::workflow_continuation::set_outputs(app, &output_video, &output_audio)?;
    // Voice mode
    let reuse_wav = input_str(inputs, &["performance_wav", "reuse_audio"]);
    if reuse_wav.is_none() && output_audio.exists() { return Err("Saved performance audio already exists. Use Resume with saved audio; speech will not be generated again.".into()); }
    let clone_wav = input_str(inputs, &["voice_clone", "voice_clone_path", "clone_wav"])
        .map(|s| s.to_string())
        .or_else(|| {
            headers.get("voice").and_then(|v| {
                let lower = v.to_ascii_lowercase();
                if lower.ends_with(".wav") || lower.ends_with(".mp3") {
                    Some(v.clone())
                } else {
                    None
                }
            })
        });
    let profile_yaml = input_str(inputs, &["voice_profile", "voice_profile_path"])
        .map(|s| s.to_string())
        .or_else(|| {
            headers.get("voice").and_then(|v| {
                let lower = v.to_ascii_lowercase();
                if lower.ends_with(".yaml") || lower.ends_with(".yml") {
                    Some(v.clone())
                } else {
                    None
                }
            })
        });

    let audio_mode = if reuse_wav.is_some() {
        "reuse_wav"
    } else if clone_wav.is_some() {
        "voice_clone"
    } else if profile_yaml.is_some() {
        "voice_profile"
    } else {
        return Err(
            "text_sync needs voice_clone WAV, voice_profile YAML, or performance_wav to reuse"
                .into(),
        );
    };

    // --- Step 1: performance WAV ---
    if let Some(existing) = reuse_wav {
        let src = resolve_under_project(existing, project_root).map_err(|e| e.to_string())?;
        if !src.is_file() {
            return Err(format!("performance_wav not found: {}", src.display()));
        }
        if src != output_audio {
            fs::copy(&src, &output_audio)
                .map_err(|e| format!("Could not copy performance WAV: {e}"))?;
        }
        app.emit(
            "director-progress",
            format!("Reusing performance WAV → {}", output_audio.display()),
        );
    } else if audio_mode == "voice_clone" {
        let clone_path = resolve_under_project(clone_wav.as_deref().unwrap(), project_root)
            .map_err(|e| e.to_string())?;
        if !clone_path.is_file() {
            return Err(format!(
                "voice_clone WAV not found: {}",
                clone_path.display()
            ));
        }
        let audio_model = capability_model(context, "audio.tts_clone").ok_or_else(|| {
            "Set Settings → Audio → TTS (voice clone) for text_sync (e.g. bytedance/seed-audio-1.0)."
                .to_string()
        })?;
        app.emit(
            "director-progress",
            format!("Uploading clone WAV {}…", clone_path.display()),
        );
        let clone_url = upload_local(
            app,
            project_root,
            &clone_path.to_string_lossy(),
            crate::media_util::MediaRole::Audio,
        )?;
        let tts_text = build_seed_tts_text(&cue_text, &delivery);
        let params = json!({
            "text": tts_text,
            "prompt": tts_text,
            "references": [{ "audio_url": clone_url }],
            "audio_url": clone_url,
            "reference_audio": clone_url,
        });
        app.emit(
            "director-progress",
            format!(
                "Generating performance TTS with `{}` → {}…",
                audio_model.model_id,
                output_audio.display()
            ),
        );
        let api_id = if audio_model.api_id.trim().is_empty() {
            None
        } else {
            Some(audio_model.api_id.as_str())
        };
        let result = run_media_generate(
            app,
            project_root,
            "audio",
            &audio_model.model_id,
            api_id,
            params,
            &output_audio.to_string_lossy(),
        )?;
        require_media_saved(&result, &output_audio.to_string_lossy())?;
    } else {
        let profile_path = resolve_under_project(profile_yaml.as_deref().unwrap(), project_root)
            .map_err(|e| e.to_string())?;
        if !profile_path.is_file() {
            return Err(format!(
                "voice_profile YAML not found: {}",
                profile_path.display()
            ));
        }
        let (profile_model, voice_id, language, settings) =
            load_voice_profile_fields(&profile_path)?;
        let audio_model = if !profile_model.trim().is_empty() {
            ModelRef {
                api_id: String::new(),
                model_id: profile_model,
            }
        } else {
            capability_model(context, "audio.tts_profile").ok_or_else(|| {
                "Voice profile has no model_id; set Settings → Audio → TTS (profile) or fix the YAML."
                    .to_string()
            })?
        };
        let mut params = json!({ "text": cue_text });
        if !voice_id.is_empty() {
            params["voice"] = json!(voice_id);
            params["voice_id"] = json!(voice_id);
        }
        if !language.is_empty() {
            params["language"] = json!(language);
        }
        for (k, v) in settings {
            if !params.as_object().unwrap().contains_key(&k) {
                params[k] = v;
            }
        }
        if !delivery.is_empty() {
            params["prompt"] = json!(format!("{delivery}. Speak exactly: {cue_text}"));
        }
        app.emit(
            "director-progress",
            format!(
                "Generating profile TTS with `{}` → {}…",
                audio_model.model_id,
                output_audio.display()
            ),
        );
        let api_id = if audio_model.api_id.trim().is_empty() {
            None
        } else {
            Some(audio_model.api_id.as_str())
        };
        let result = run_media_generate(
            app,
            project_root,
            "audio",
            &audio_model.model_id,
            api_id,
            params,
            &output_audio.to_string_lossy(),
        )?;
        require_media_saved(&result, &output_audio.to_string_lossy())?;
    }

    if !output_audio.is_file() || fs::metadata(&output_audio).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err(format!(
            "Performance WAV missing or empty at {} — cannot pad or generate video.",
            output_audio.display()
        ));
    }

    // --- Step 2: pad handles ---
    let head_ms = ((head_s * 1000.0).round() as u64).saturating_add(sync_ms);
    let pad_name = format!(
        "{}_pad_h{head_s:.1}_t{tail_s:.1}.wav",
        output_audio
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("perf")
    );
    let padded_audio = output_audio
        .parent()
        .unwrap_or(Path::new("."))
        .join(pad_name);
    app.emit(
        "director-progress",
        format!(
            "Padding audio (head_ms={head_ms}, tail_s={tail_s}) → {}…",
            padded_audio.display()
        ),
    );
    pad_audio_handles(
        &output_audio.to_string_lossy(),
        &padded_audio.to_string_lossy(),
        head_ms,
        tail_s,
    )?;
    let padded_dur = probe_duration_seconds(&padded_audio.to_string_lossy())?;
    let video_duration: u64 = if padded_dur <= 5.0 {
        5
    } else if padded_dur <= 10.0 {
        10
    } else {
        return Err(format!(
            "Padded audio is {padded_dur:.2}s (>10s). Split the cue or trim before text-sync; Van/Wan only expose 5 or 10s."
        ));
    };

    // --- Step 2.5: start-image aspect for 9:16 shows ---
    let mut still_for_upload = character_path.clone();
    let mut still_note = String::new();
    let want_9x16 = aspect.contains("9:16") || aspect.contains("9/16");
    let force_pad = inputs
        .get("pad_to_9x16")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || want_9x16;
    if force_pad {
        if let Ok((w, h)) = probe_image_size(&character_path.to_string_lossy()) {
            let ratio = h as f64 / w.max(1) as f64;
            if !(1.7..=1.9).contains(&ratio) {
                let talking = character_path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(format!(
                        "{}_9x16.png",
                        character_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("talking")
                    ));
                if !talking.is_file() {
                    app.emit(
                        "director-progress",
                        format!(
                            "Creating 9:16 talking still from {}×{} → {}…",
                            w,
                            h,
                            talking.display()
                        ),
                    );
                    ensure_talking_still_9x16(
                        &character_path.to_string_lossy(),
                        &talking.to_string_lossy(),
                    )?;
                }
                still_for_upload = talking;
                still_note = format!(
                    "- start image padded to 9:16 (source was {w}×{h}) → `{}`\n",
                    still_for_upload.display()
                );
            } else {
                still_note = format!("- start image already ~9:16 ({w}×{h})\n");
            }
        }
    }

    // --- Step 3: talking video ---
    let video_model = capability_model(context, "video.lip_sync_text_narration")
        .or_else(|| capability_model(context, "video.start_frame"))
        .ok_or_else(|| {
            "Set Settings → Video → Lip sync (talking shot) to a Van/Wan image-to-video + audio model (fallback: Start frame)."
                .to_string()
        })?;

    app.emit(
        "director-progress",
        format!("Uploading talking still {}…", still_for_upload.display()),
    );
    let image_url = upload_local(
        app,
        project_root,
        &still_for_upload.to_string_lossy(),
        crate::media_util::MediaRole::Image,
    )?;
    app.emit(
        "director-progress",
        format!("Uploading padded WAV {}…", padded_audio.display()),
    );
    let audio_url = upload_local(
        app,
        project_root,
        &padded_audio.to_string_lossy(),
        crate::media_util::MediaRole::Audio,
    )?;

    let mut params = json!({
        "prompt": prompt,
        "image": image_url,
        "image_url": image_url,
        "audio": audio_url,
        "audio_url": audio_url,
        "duration": video_duration,
        "aspect_ratio": aspect,
        "enable_prompt_expansion": false,
        "generate_audio": false,
        "resolution": "1080p",
    });
    if !negative.is_empty() {
        params["negative_prompt"] = json!(negative);
    }

    app.emit(
        "director-progress",
        format!(
            "Generating text-sync video with `{}` ({video_duration}s) → {}…",
            video_model.model_id,
            output_video.display()
        ),
    );
    let api_id = if video_model.api_id.trim().is_empty() {
        None
    } else {
        Some(video_model.api_id.as_str())
    };
    let result = run_media_generate(
        app,
        project_root,
        "video",
        &video_model.model_id,
        api_id,
        params,
        &output_video.to_string_lossy(),
    )?;

    match require_media_saved(&result, &output_video.to_string_lossy()) {
        Ok(summary) => Ok(SpecialistResult::done(format!(
            "Text-sync talking shot finished (host-run).\n\
- cue text (verbatim): {cue_text}\n\
- audio mode: `{audio_mode}`\n\
- performance WAV: `{}`\n\
- pad: head_s={head_s:.1} + sync_ms={sync_ms} → head_ms={head_ms}; tail_s={tail_s:.1}\n\
- padded WAV: `{}` ({padded_dur:.2}s) → video duration {video_duration}s\n\
- character/start image: `{}`\n\
{still_note}\
- video model: `{}`\n\
- output video: `{}`\n\
{summary}\n\n\
Standalone talking shot — not a continuity out-frame link.",
            output_audio.display(),
            padded_audio.display(),
            still_for_upload.display(),
            video_model.model_id,
            output_video.display(),
        ))),
        Err(error) => {
            let parsed: Value = serde_json::from_str(&result).unwrap_or(json!({ "raw": result }));
            let prediction_id = parsed
                .pointer("/submitted/prediction_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if prediction_id.is_empty() {
                return Err(error);
            }
            let mut pending = validated.action.clone();
            pending
                .inputs
                .insert("collect_prediction_id".into(), json!(prediction_id));
            pending.inputs.insert(
                "output_video".into(),
                json!(output_video.to_string_lossy().to_string()),
            );
            Ok(SpecialistResult::cont(format!(
                "{error}\n\nPerformance WAV and pad are already local:\n\
- `{}`\n- `{}`\n\n{}",
                output_audio.display(),
                padded_audio.display(),
                encode_pending_action(&pending)
            )))
        }
    }
}

fn run_lip_sync(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    let inputs = &validated.action.inputs;

    if let Some(prediction_id) = input_str(inputs, &["collect_prediction_id"]) {
        let save = input_str(inputs, &["output_path", "save_path"])
            .ok_or_else(|| "lip_sync resume needs output_path".to_string())?;
        let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
        return resume_media_collect(
            app,
            validated,
            project_root,
            prediction_id,
            &save_path,
            "output_path",
            "Lip sync",
        );
    }

    let video = input_str(inputs, &["video", "source_video"])
        .ok_or_else(|| "lip_sync needs video".to_string())?;
    let audio = input_str(inputs, &["audio"]).ok_or_else(|| "lip_sync needs audio".to_string())?;
    let save = input_str(inputs, &["output_path", "save_path"])
        .ok_or_else(|| "lip_sync needs output_path".to_string())?;
    let save_path = resolve_under_project(save, project_root).map_err(|e| e.to_string())?;
    if let Some(parent) = save_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create output dir: {e}"))?;
    }

    let model = capability_model(context, "video.lip_sync_audio_included")
        .or_else(|| capability_model(context, "video.lip_sync_text_narration"))
        .ok_or_else(|| "Set Settings → Video → Lip sync before running lip_sync.".to_string())?;

    let video_path = resolve_under_project(video, project_root).map_err(|e| e.to_string())?;
    let audio_path = resolve_under_project(audio, project_root).map_err(|e| e.to_string())?;
    let params = json!({
        "video": video_path.to_string_lossy(),
        "audio": audio_path.to_string_lossy(),
    });
    let api_id = if model.api_id.trim().is_empty() {
        None
    } else {
        Some(model.api_id.as_str())
    };
    let result = run_media_generate(
        app,
        project_root,
        "video",
        &model.model_id,
        api_id,
        params,
        &save_path.to_string_lossy(),
    )?;
    finish_media_or_resume(
        validated,
        "Lip sync",
        &model.model_id,
        &save_path.to_string_lossy(),
        &result,
        "output_path",
    )
}

const DEFAULT_MODEL_SHEET_SHOTS: &[(&str, &str)] = &[
    (
        "closeup_front",
        "Head and shoulders, neutral expression, eyes to camera",
    ),
    (
        "closeup_left",
        "Head and shoulders, neutral, about 45 degrees left turn",
    ),
    (
        "closeup_right",
        "Head and shoulders, neutral, about 45 degrees right turn",
    ),
    (
        "standing_front",
        "Full body, standing front, relaxed neutral stance",
    ),
    ("standing_left", "Full body, standing about 45 degrees left"),
    (
        "standing_right",
        "Full body, standing about 45 degrees right",
    ),
    (
        "standing_back",
        "Full body, standing back view, head neutral",
    ),
    (
        "t_pose_front",
        "Full body, front T-pose, palms down, feet shoulder-width",
    ),
    (
        "expression_happy",
        "Head and shoulders, clear happy expression",
    ),
    ("expression_sad", "Head and shoulders, clear sad expression"),
    (
        "expression_excited",
        "Head and shoulders, clear excited expression",
    ),
    (
        "expression_surprised",
        "Head and shoulders, clear surprised expression",
    ),
];

#[derive(Debug, Clone)]
struct ManifestShot {
    id: String,
    status: String,
    path: String,
    prediction_id: String,
    framing: String,
}

#[derive(Debug, Clone)]
struct ModelSheetManifest {
    character: String,
    style: String,
    model_id: String,
    mode: String,
    aspect_ratio: String,
    resolution: String,
    output_directory: String,
    character_sheet: String,
    reference_images: Vec<String>,
    approval_shot: String,
    approval_shot_path: String,
    identity_locked: bool,
    text_only_gate: bool,
    shots: Vec<ManifestShot>,
}

fn slugify(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_whitespace()) && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

fn shot_framing(id: &str) -> String {
    DEFAULT_MODEL_SHEET_SHOTS
        .iter()
        .find(|(sid, _)| *sid == id)
        .map(|(_, f)| (*f).to_string())
        .unwrap_or_else(|| id.to_string())
}

fn load_identity_text(
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
) -> Result<String, String> {
    if let Some(text) = input_str(inputs, &["identity_text", "brief", "prompt"]) {
        let mut block = text.to_string();
        // Append character sheet canon when present.
        for path in input_path_list(inputs, &["character_files"]) {
            if let Ok(resolved) = resolve_under_project(&path, project_root) {
                if let Ok(sheet) = fs::read_to_string(&resolved) {
                    block.push_str("\n\nCharacter sheet canon:\n");
                    block.push_str(sheet.chars().take(4000).collect::<String>().as_str());
                }
            }
        }
        return Ok(block);
    }
    for path in input_path_list(inputs, &["character_files"]) {
        let resolved = resolve_under_project(&path, project_root).map_err(|e| e.to_string())?;
        let sheet = fs::read_to_string(&resolved)
            .map_err(|e| format!("Could not read character sheet {}: {e}", resolved.display()))?;
        return Ok(sheet.chars().take(6000).collect());
    }
    Err("model_sheet needs identity_text/brief or character_files".into())
}

fn manifest_path(output_dir: &Path) -> PathBuf {
    output_dir.join("model_sheet_manifest.yaml")
}

fn write_manifest(manifest: &ModelSheetManifest) -> Result<(), String> {
    let path = manifest_path(Path::new(&manifest.output_directory));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("manifest dirs: {e}"))?;
    }
    let mut refs = String::new();
    for r in &manifest.reference_images {
        refs.push_str(&format!("    - \"{}\"\n", r.replace('"', "\\\"")));
    }
    if refs.is_empty() {
        refs.push_str("    []\n");
    }
    let mut shots = String::new();
    for shot in &manifest.shots {
        shots.push_str(&format!(
            "  - id: {}\n    status: {}\n    path: \"{}\"\n    prediction_id: \"{}\"\n    framing: \"{}\"\n",
            shot.id,
            shot.status,
            shot.path.replace('"', "\\\""),
            shot.prediction_id.replace('"', "\\\""),
            shot.framing.replace('"', "\\\"")
        ));
    }
    let yaml = format!(
        "pack_version: 1\n\
character: \"{}\"\n\
style: \"{}\"\n\
model_id: \"{}\"\n\
mode: {}\n\
aspect_ratio: \"{}\"\n\
resolution: {}\n\
output_directory: \"{}\"\n\
identity_locked: {}\n\
text_only_gate: {}\n\
identity_source:\n\
  character_sheet: \"{}\"\n\
  reference_images:\n{}\
  approval_shot: \"{}\"\n\
  approval_shot_path: \"{}\"\n\
shots:\n{}",
        manifest.character.replace('"', "\\\""),
        manifest.style.replace('"', "\\\""),
        manifest.model_id.replace('"', "\\\""),
        manifest.mode,
        manifest.aspect_ratio,
        manifest.resolution,
        manifest.output_directory.replace('"', "\\\""),
        manifest.identity_locked,
        manifest.text_only_gate,
        manifest.character_sheet.replace('"', "\\\""),
        refs,
        manifest.approval_shot,
        manifest.approval_shot_path.replace('"', "\\\""),
        shots
    );
    fs::write(&path, yaml).map_err(|e| format!("Could not write manifest: {e}"))
}

fn read_manifest(output_dir: &Path) -> Option<ModelSheetManifest> {
    let path = manifest_path(output_dir);
    let text = fs::read_to_string(path).ok()?;
    let mut character = String::new();
    let mut style = String::new();
    let mut model_id = String::new();
    let mut mode = String::new();
    let mut aspect_ratio = "3:4".into();
    let mut resolution = "2k".into();
    let mut output_directory = output_dir.to_string_lossy().to_string();
    let mut character_sheet = String::new();
    let mut reference_images = Vec::new();
    let mut approval_shot = String::new();
    let mut approval_shot_path = String::new();
    let mut identity_locked = false;
    let mut text_only_gate = false;
    let mut shots = Vec::new();
    let mut in_refs = false;
    let mut current: Option<ManifestShot> = None;

    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("character:") {
            character = rest.trim().trim_matches('"').to_string();
            in_refs = false;
        } else if let Some(rest) = t.strip_prefix("style:") {
            style = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = t.strip_prefix("model_id:") {
            model_id = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = t.strip_prefix("mode:") {
            mode = rest.trim().to_string();
        } else if let Some(rest) = t.strip_prefix("aspect_ratio:") {
            aspect_ratio = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = t.strip_prefix("resolution:") {
            resolution = rest.trim().to_string();
        } else if let Some(rest) = t.strip_prefix("output_directory:") {
            output_directory = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = t.strip_prefix("identity_locked:") {
            identity_locked = rest.trim() == "true";
        } else if let Some(rest) = t.strip_prefix("text_only_gate:") {
            text_only_gate = rest.trim() == "true";
        } else if let Some(rest) = t.strip_prefix("character_sheet:") {
            character_sheet = rest.trim().trim_matches('"').to_string();
            in_refs = false;
        } else if t.starts_with("reference_images:") {
            in_refs = true;
        } else if in_refs {
            if let Some(rest) = t.strip_prefix("- ") {
                reference_images.push(rest.trim().trim_matches('"').to_string());
            } else if !t.is_empty() && !t.starts_with('[') {
                in_refs = false;
            }
        }
        if let Some(rest) = t.strip_prefix("approval_shot:") {
            approval_shot = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = t.strip_prefix("approval_shot_path:") {
            approval_shot_path = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = t.strip_prefix("- id:") {
            if let Some(prev) = current.take() {
                shots.push(prev);
            }
            current = Some(ManifestShot {
                id: rest.trim().to_string(),
                status: "pending".into(),
                path: String::new(),
                prediction_id: String::new(),
                framing: shot_framing(rest.trim()),
            });
        } else if let Some(shot) = current.as_mut() {
            if let Some(rest) = t.strip_prefix("status:") {
                shot.status = rest.trim().to_string();
            } else if let Some(rest) = t.strip_prefix("path:") {
                shot.path = rest.trim().trim_matches('"').to_string();
            } else if let Some(rest) = t.strip_prefix("prediction_id:") {
                shot.prediction_id = rest.trim().trim_matches('"').to_string();
            } else if let Some(rest) = t.strip_prefix("framing:") {
                shot.framing = rest.trim().trim_matches('"').to_string();
            }
        }
    }
    if let Some(prev) = current {
        shots.push(prev);
    }
    if shots.is_empty() {
        return None;
    }
    Some(ModelSheetManifest {
        character,
        style,
        model_id,
        mode,
        aspect_ratio,
        resolution,
        output_directory,
        character_sheet,
        reference_images,
        approval_shot,
        approval_shot_path,
        identity_locked,
        text_only_gate,
        shots,
    })
}

fn unlock_remaining_shots(manifest: &mut ModelSheetManifest) {
    for shot in &mut manifest.shots {
        if shot.status == "held" {
            shot.status = "pending".into();
        }
    }
    manifest.identity_locked = true;
    manifest.mode = "image_conditioned".into();
}

fn next_pending_shot(manifest: &ModelSheetManifest) -> Option<usize> {
    manifest
        .shots
        .iter()
        .position(|s| s.status == "pending" || s.status == "submitted")
}

fn build_shot_prompt(identity: &str, style: &str, wardrobe: &str, framing: &str) -> String {
    format!(
        "{identity}\n\nStyle: {style}.\nWardrobe: {wardrobe}.\n\
Shot: {framing}.\n\
Seamless chroma-green backdrop (#00B140), even neutral studio lighting, no scenic background, \
no props, no furniture, no logos, no readable text, no other people. \
Camera appropriate to the shot; production still for character continuity.\n\
Negative: extra people, background scenery, beauty filters that change identity, deformed hands, \
cropped limbs on full-body shots, watermark, text overlays."
    )
}

fn update_character_sheet_refs(
    sheet_path: &str,
    manifest: &ModelSheetManifest,
    project_root: Option<&str>,
) -> Result<(), String> {
    if sheet_path.trim().is_empty() {
        return Ok(());
    }
    let path = resolve_under_project(sheet_path, project_root).map_err(|e| e.to_string())?;
    if !path.is_file() {
        return Ok(());
    }
    let original = fs::read_to_string(&path).map_err(|e| format!("read sheet: {e}"))?;
    let closeup = manifest
        .shots
        .iter()
        .find(|s| s.id == "closeup_front" && s.status == "saved")
        .or_else(|| manifest.shots.iter().find(|s| s.status == "saved"));
    let Some(primary) = closeup else {
        return Ok(());
    };
    let others: Vec<&str> = manifest
        .shots
        .iter()
        .filter(|s| s.status == "saved" && s.path != primary.path)
        .map(|s| s.path.as_str())
        .collect();
    let mut lines: Vec<String> = original.lines().map(|l| l.to_string()).collect();
    let mut saw_char = false;
    let mut saw_additional = false;
    let mut saw_notes = false;
    for line in &mut lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("character image reference:") {
            *line = format!("Character image reference: {}", primary.path);
            saw_char = true;
        } else if lower.starts_with("additional image references:") {
            *line = format!("Additional image references: {}", others.join(", "));
            saw_additional = true;
        } else if lower.starts_with("asset notes:") {
            *line = format!(
                "Asset notes: model sheet ({}) style={} model={} dir={}",
                chrono_date(),
                manifest.style,
                manifest.model_id,
                manifest.output_directory
            );
            saw_notes = true;
        }
    }
    if !saw_char || !saw_additional || !saw_notes {
        if !original.ends_with('\n') {
            lines.push(String::new());
        }
        lines.push("## Reference Assets".into());
        if !saw_char {
            lines.push(format!("Character image reference: {}", primary.path));
        }
        if !saw_additional {
            lines.push(format!(
                "Additional image references: {}",
                others.join(", ")
            ));
        }
        if !saw_notes {
            lines.push(format!(
                "Asset notes: model sheet ({}) style={} model={}",
                chrono_date(),
                manifest.style,
                manifest.model_id
            ));
        }
    }
    fs::write(&path, lines.join("\n") + "\n").map_err(|e| format!("write sheet: {e}"))?;
    Ok(())
}

fn chrono_date() -> String {
    // Avoid extra deps: use system date via format from UNIX if needed — keep simple.
    "pack".into()
}

fn pending_model_sheet_action(validated: &ValidatedAction, identity_gate: &str) -> DirectorAction {
    let mut action = validated.action.clone();
    action
        .inputs
        .insert("identity_gate".into(), json!(identity_gate));
    action
}

fn run_model_sheet(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    let inputs = &validated.action.inputs;
    let out_raw = input_str(inputs, &["output_path", "save_path"])
        .ok_or_else(|| "model_sheet needs output_path".to_string())?;
    let output_dir = resolve_under_project(out_raw, project_root).map_err(|e| e.to_string())?;
    fs::create_dir_all(&output_dir).map_err(|e| format!("Could not create pack dir: {e}"))?;

    let identity_gate = input_str(inputs, &["identity_gate"]).unwrap_or("");
    let mut manifest = if let Some(existing) = read_manifest(&output_dir) {
        existing
    } else {
        // Rebuild init without the broken character_sheet logic — inline clean init
        let refs = input_path_list(inputs, &["reference_images", "reference_assets"]);
        let character_files = input_path_list(inputs, &["character_files"]);
        let sheet_raw = character_files.first().cloned().unwrap_or_default();
        let sheet_path = if sheet_raw.is_empty() {
            String::new()
        } else {
            resolve_under_project(&sheet_raw, project_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(sheet_raw)
        };
        let style = input_str(inputs, &["style"])
            .unwrap_or("hyperrealistic")
            .to_string();
        let slug = input_str(inputs, &["character_slug"])
            .map(slugify)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                Path::new(&sheet_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(slugify)
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "character".into());
        let text_only_gate = refs.is_empty();
        let mut shots = Vec::new();
        for (i, (id, framing)) in DEFAULT_MODEL_SHEET_SHOTS.iter().enumerate() {
            let status = if text_only_gate && i > 0 {
                "held"
            } else {
                "pending"
            };
            shots.push(ManifestShot {
                id: (*id).into(),
                status: status.into(),
                path: output_dir
                    .join(format!("{slug}_{id}.png"))
                    .to_string_lossy()
                    .to_string(),
                prediction_id: String::new(),
                framing: (*framing).into(),
            });
        }
        // Optional shot_ids filter
        if let Some(Value::Array(items)) = inputs.get("shot_ids") {
            let want: Vec<String> = items
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !want.is_empty() {
                shots.retain(|s| want.contains(&s.id));
                if text_only_gate {
                    for (i, shot) in shots.iter_mut().enumerate() {
                        shot.status = if i == 0 { "pending" } else { "held" }.into();
                    }
                }
            }
        }
        let m = ModelSheetManifest {
            character: slug,
            style: style
                .lines()
                .next()
                .unwrap_or("hyperrealistic")
                .trim()
                .into(),
            model_id: String::new(),
            mode: if text_only_gate {
                "text_to_image".into()
            } else {
                "image_conditioned".into()
            },
            aspect_ratio: input_str(inputs, &["aspect_ratio"]).unwrap_or("3:4").into(),
            resolution: input_str(inputs, &["resolution"]).unwrap_or("2k").into(),
            output_directory: output_dir.to_string_lossy().to_string(),
            character_sheet: sheet_path,
            reference_images: refs
                .iter()
                .filter_map(|p| {
                    resolve_under_project(p, project_root)
                        .ok()
                        .map(|x| x.to_string_lossy().to_string())
                })
                .collect(),
            approval_shot: String::new(),
            approval_shot_path: String::new(),
            identity_locked: !text_only_gate,
            text_only_gate,
            shots,
        };
        write_manifest(&m)?;
        m
    };

    // Identity gate: user approved the first plate — unlock the rest.
    if identity_gate == "rest" || identity_gate == "generate_rest" {
        if manifest.text_only_gate && !manifest.identity_locked {
            let first_saved = manifest.shots.iter().find(|s| s.status == "saved").cloned();
            if let Some(saved) = first_saved {
                manifest.approval_shot = saved.id.clone();
                manifest.approval_shot_path = saved.path.clone();
                if !manifest.reference_images.contains(&saved.path) {
                    manifest.reference_images.insert(0, saved.path.clone());
                }
                unlock_remaining_shots(&mut manifest);
                write_manifest(&manifest)?;
            } else {
                return Err(
                    "Cannot unlock remaining model-sheet shots: no saved identity plate yet."
                        .into(),
                );
            }
        }
    }

    let identity = load_identity_text(inputs, project_root)?;
    let wardrobe = input_str(inputs, &["wardrobe"])
        .unwrap_or("simple solid-color form-fitting top and pants suitable for turnarounds");

    let Some(idx) = next_pending_shot(&manifest) else {
        let saved = manifest
            .shots
            .iter()
            .filter(|s| s.status == "saved")
            .count();
        let _ = update_character_sheet_refs(&manifest.character_sheet, &manifest, project_root);
        return Ok(SpecialistResult::done(format!(
            "Model sheet pack complete ({saved} saved).\n- directory: `{}`\n- manifest: `{}`",
            manifest.output_directory,
            manifest_path(&output_dir).display()
        )));
    };

    // Resume submitted shot if needed
    let shot = manifest.shots[idx].clone();
    if shot.status == "submitted" && !shot.prediction_id.is_empty() {
        let collect_args = json!({
            "prediction_id": shot.prediction_id,
            "path": shot.path,
            "wait_seconds": 90,
        });
        app.emit(
            "director-progress",
            format!("Collecting model-sheet shot `{}`…", shot.id),
        );
        match media::run_media_tool("atlas_collect_media", &collect_args, project_root, app) {
            Some(Ok(text)) => {
                manifest.shots[idx].status = "saved".into();
                write_manifest(&manifest)?;
                let _ =
                    update_character_sheet_refs(&manifest.character_sheet, &manifest, project_root);
                return finish_model_sheet_shot(
                    validated, &manifest, &shot.id, &shot.path, &text, true,
                );
            }
            Some(Err(error)) => {
                return Err(format!(
                    "Could not collect submitted shot `{}` ({}): {error}",
                    shot.id, shot.prediction_id
                ));
            }
            None => {}
        }
    }

    let use_refs = !manifest.reference_images.is_empty() || manifest.identity_locked;
    let model = if use_refs {
        capability_model(context, "image.character_reference")
            .or_else(|| capability_model(context, "image.edit"))
            .or_else(|| capability_model(context, "image.image_to_image"))
            .ok_or_else(|| {
                "Set Settings → Image → Character reference (or Focused edit) for model sheets."
                    .to_string()
            })?
    } else {
        capability_model(context, "image.text_to_image").ok_or_else(|| {
            "Set Settings → Image → Text to image for the model-sheet identity plate.".to_string()
        })?
    };

    let mut uploaded = Vec::new();
    for path in &manifest.reference_images {
        app.emit(
            "director-progress",
            format!("Uploading model-sheet ref {}…", path),
        );
        uploaded.push(upload_local(
            app,
            project_root,
            path,
            crate::media_util::MediaRole::Image,
        )?);
    }

    let prompt = build_shot_prompt(&identity, &manifest.style, wardrobe, &shot.framing);
    let mut params = json!({
        "prompt": prompt,
        "aspect_ratio": manifest.aspect_ratio,
        "resolution": manifest.resolution,
        "output_format": "png",
    });
    if !uploaded.is_empty() {
        params["images"] = json!(uploaded.clone());
        params["image"] = json!(uploaded[0]);
        params["image_url"] = json!(uploaded[0]);
        params["media_files"] = json!(uploaded);
    }

    manifest.model_id = model.model_id.clone();
    manifest.mode = if uploaded.is_empty() {
        "text_to_image".into()
    } else {
        "image_conditioned".into()
    };
    write_manifest(&manifest)?;

    app.emit(
        "director-progress",
        format!(
            "Generating model-sheet `{}` with `{}`…",
            shot.id, model.model_id
        ),
    );
    let api_id = if model.api_id.trim().is_empty() {
        None
    } else {
        Some(model.api_id.as_str())
    };
    let result = run_media_generate(
        app,
        project_root,
        "image",
        &model.model_id,
        api_id,
        params,
        &shot.path,
    )?;

    // Mark saved if file exists; else keep prediction id if present.
    let saved_on_disk = Path::new(&shot.path).is_file();
    if saved_on_disk {
        manifest.shots[idx].status = "saved".into();
    } else if let Ok(parsed) = serde_json::from_str::<Value>(&result) {
        if let Some(pid) = parsed
            .pointer("/submitted/prediction_id")
            .or_else(|| parsed.get("prediction_id"))
            .and_then(|v| v.as_str())
        {
            manifest.shots[idx].status = "submitted".into();
            manifest.shots[idx].prediction_id = pid.to_string();
        } else {
            manifest.shots[idx].status = "saved".into();
        }
    } else {
        manifest.shots[idx].status = "saved".into();
    }
    if manifest.text_only_gate
        && !manifest.identity_locked
        && manifest.shots[idx].status == "saved"
        && manifest.approval_shot.is_empty()
    {
        manifest.approval_shot = shot.id.clone();
        manifest.approval_shot_path = shot.path.clone();
    }
    write_manifest(&manifest)?;
    let _ = update_character_sheet_refs(&manifest.character_sheet, &manifest, project_root);

    let more_pending = next_pending_shot(&manifest).is_some();
    let waiting_identity = manifest.text_only_gate
        && !manifest.identity_locked
        && manifest.shots.iter().any(|s| s.status == "held");

    if waiting_identity && manifest.shots[idx].status == "saved" {
        let pending = pending_model_sheet_action(validated, "rest");
        let msg = format!(
            "Model sheet identity plate saved.\n\
- shot: `{}`\n\
- path: `{}`\n\
- model: `{}`\n\n\
Review this plate. Use the Approve button to unlock and generate the remaining pack shots (host-run, one at a time).\n\n{result}\n\n{}",
            shot.id,
            shot.path,
            model.model_id,
            encode_pending_action(&pending)
        );
        return Ok(SpecialistResult::done(msg));
    }

    finish_model_sheet_shot(
        validated,
        &manifest,
        &shot.id,
        &shot.path,
        &result,
        more_pending || manifest.shots[idx].status == "submitted",
    )
}

fn finish_model_sheet_shot(
    validated: &ValidatedAction,
    manifest: &ModelSheetManifest,
    shot_id: &str,
    shot_path: &str,
    result: &str,
    continue_more: bool,
) -> Result<SpecialistResult, String> {
    let saved = manifest
        .shots
        .iter()
        .filter(|s| s.status == "saved")
        .count();
    let total = manifest.shots.len();
    if continue_more {
        let pending = pending_model_sheet_action(validated, "continue");
        let msg = format!(
            "Model sheet shot saved ({saved}/{total}).\n\
- shot: `{shot_id}`\n\
- path: `{shot_path}`\n\
- pack dir: `{}`\n\n\
Say **continue** (or **Approved**) for the next unfinished shot.\n\n{result}\n\n{}",
            manifest.output_directory,
            encode_pending_action(&pending)
        );
        Ok(SpecialistResult::cont(msg))
    } else {
        Ok(SpecialistResult::done(format!(
            "Model sheet pack finished ({saved}/{total} saved).\n\
- pack dir: `{}`\n\
- manifest: `{}/model_sheet_manifest.yaml`\n\n{result}",
            manifest.output_directory, manifest.output_directory
        )))
    }
}

/// Whether this specialist spends paid media credits when executed.
pub fn specialist_spends(specialist: &str) -> bool {
    matches!(
        specialist,
        "scene_generation" | "still_image" | "text_sync" | "lip_sync" | "model_sheet"
    )
}

/// Host-owned approval: any billable provider call, plus assemble (local but plan-gated).
pub fn specialist_requires_approval(specialist: &str) -> bool {
    specialist_spends(specialist) || specialist == "assemble" || specialist == "text_write"
}

/// Extra approval detail for text-sync talking shots.
pub fn text_sync_approval_notes(
    context: &DirectorContext,
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
) -> String {
    let handoff_path = input_str(inputs, &["handoff_path"]).unwrap_or("(handoff)");
    let handoff_text = if let Some(path) = input_str(inputs, &["handoff_path"]) {
        resolve_under_project(path, project_root)
            .ok()
            .and_then(|p| fs::read_to_string(p).ok())
    } else if let Some(text) = input_str(inputs, &["handoff"]) {
        Some(text.to_string())
    } else {
        None
    };
    let body = handoff_text
        .as_deref()
        .map(extract_yaml_body)
        .unwrap_or_default();
    let (headers, _) = handoff_text
        .as_deref()
        .map(parse_handoff_headers_and_yaml)
        .unwrap_or_default();
    let (cue_text, delivery) = extract_first_speech_cue(&body)
        .unwrap_or_else(|| ("(missing cue text)".into(), String::new()));
    let head_s = yaml_f64_field(&body, "head_handle_seconds").unwrap_or(1.0);
    let tail_s = yaml_f64_field(&body, "tail_handle_seconds").unwrap_or(1.0);
    let sync_ms = input_str(inputs, &["sync_ms"])
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(150);
    let video_model = capability_model(context, "video.lip_sync_text_narration")
        .or_else(|| capability_model(context, "video.start_frame"))
        .map(|m| m.model_id)
        .unwrap_or_else(|| "(set Settings → Video → Lip sync (talking shot) / Start frame)".into());
    let audio_hint = if input_str(inputs, &["performance_wav", "reuse_audio"]).is_some() {
        "reuse existing performance WAV (skip TTS)".into()
    } else if input_str(inputs, &["voice_clone", "voice_clone_path", "clone_wav"]).is_some()
        || headers
            .get("voice")
            .is_some_and(|v| v.to_ascii_lowercase().ends_with(".wav"))
    {
        capability_model(context, "audio.tts_clone")
            .map(|m| format!("voice_clone → `{}`", m.model_id))
            .unwrap_or_else(|| "voice_clone → (set audio.tts_clone)".into())
    } else {
        capability_model(context, "audio.tts_profile")
            .map(|m| format!("voice_profile → `{}` (or YAML model_id)", m.model_id))
            .unwrap_or_else(|| "voice_profile → YAML model_id / audio.tts_profile".into())
    };
    let (v_out, a_out) = join_output_pair(&headers, project_root).unwrap_or((None, None));
    let out_video = input_str(inputs, &["output_video", "output_path", "save_path"])
        .map(|s| s.to_string())
        .or_else(|| v_out.map(|p| p.display().to_string()))
        .unwrap_or_else(|| "(output video)".into());
    let out_audio = input_str(inputs, &["output_audio"])
        .map(|s| s.to_string())
        .or_else(|| a_out.map(|p| p.display().to_string()))
        .unwrap_or_else(|| "(output audio)".into());
    let delivery_line = if delivery.is_empty() {
        String::new()
    } else {
        format!("- delivery: {delivery}\n")
    };
    format!(
        "### Text-sync plan (TTS → pad → I2V+audio)\n\
- handoff: `{handoff_path}`\n\
- cue text: {cue_text}\n\
{delivery_line}\
- audio: {audio_hint}\n\
- video model: `{video_model}`\n\
- handles: head={head_s:.1}s + sync_ms={sync_ms}; tail={tail_s:.1}s\n\
- output audio: `{out_audio}`\n\
- output video: `{out_video}`\n\n\
Paid calls: TTS (unless reusing WAV) + talking video. Use the Approve button to run."
    )
}

/// Extra approval detail for model sheets (shot list).
pub fn model_sheet_approval_notes(
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
) -> String {
    let out = input_str(inputs, &["output_path", "save_path"]).unwrap_or("(output_path)");
    let refs = input_path_list(inputs, &["reference_images", "reference_assets"]);
    let gate = if refs.is_empty() {
        "Text-only identity gate: first paid batch is `closeup_front` only; remaining shots unlock after you approve that plate."
    } else {
        "Reference-anchored pack: default 12-shot green-screen continuity pack, generated one shot per host turn after approval."
    };
    let shots: String = DEFAULT_MODEL_SHEET_SHOTS
        .iter()
        .map(|(id, framing)| format!("- `{id}` — {framing}"))
        .collect::<Vec<_>>()
        .join("\n");
    let _ = project_root;
    format!("### Model sheet plan\n{gate}\n\nOutput directory: `{out}`\n\nDefault shots:\n{shots}")
}

/// Extra approval detail for scene generation (model, refs, prompt excerpt).
pub fn scene_generation_approval_notes(
    context: &DirectorContext,
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
) -> String {
    if input_str(inputs, &["source_video"]).is_some() {
        let model = capability_model(context, "video.edit")
            .map(|m| m.model_id)
            .unwrap_or_else(|| "(set Settings → Video → Focused edit)".into());
        let out = input_str(inputs, &["output_path", "save_path"]).unwrap_or("(output_path)");
        let brief = input_str(inputs, &["edit_brief", "prompt", "brief"]).unwrap_or("(brief)");
        return format!(
            "### Scene generation plan (focused edit)\n\
- model: `{model}`\n\
- source_video: `{}`\n\
- output: `{out}`\n\
- brief: {brief}",
            input_str(inputs, &["source_video"]).unwrap_or("")
        );
    }

    if input_str(inputs, &["handoff", "handoff_path"]).is_none()
        && input_str(inputs, &["prompt", "brief", "edit_brief"]).is_some()
        && input_str(inputs, &["output_path", "save_path"]).is_some()
    {
        let explicit_start = input_str(inputs, &["source_frame", "source_image", "start_frame"]);
        let refs = input_path_list(inputs, &["reference_images", "reference_assets"]);
        let has_explicit_start = explicit_start.is_some();
        let model = freeform_video_model(context, has_explicit_start, !refs.is_empty())
            .map(|m| m.model_id)
            .unwrap_or_else(|e| format!("({e})"));
        let out = input_str(inputs, &["output_path", "save_path"]).unwrap_or("(output_path)");
        let prompt = input_str(inputs, &["prompt", "brief", "edit_brief"]).unwrap_or("(prompt)");
        let duration = input_str(inputs, &["duration"]).unwrap_or("5");
        let lane = if has_explicit_start {
            "start-frame / image-to-video"
        } else if !refs.is_empty() {
            "reference-to-video"
        } else {
            "text-to-video"
        };
        let slot_hint = if has_explicit_start {
            "video.start_frame"
        } else if !refs.is_empty() {
            "video.character_reference / video.location_prop_other_reference"
        } else {
            "video.text_only"
        };
        let routing = if context.adult_routing_requested {
            format!("- routing: **adult/nsfw** (Adult `{slot_hint}` when set)\n")
        } else {
            "- routing: main Generation defaults\n".to_string()
        };
        let frame_line = if let Some(frame) = explicit_start {
            format!("- start frame: `{frame}`\n")
        } else if !refs.is_empty() {
            format!(
                "- references:\n{}\n",
                refs.iter()
                    .map(|r| format!("  - `{r}`"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else {
            String::new()
        };
        return format!(
            "### Scene generation plan ({lane})\n\
{routing}\
- model: `{model}`\n\
{frame_line}\
- output: `{out}`\n\
- duration: {duration}s\n\n\
Prompt:\n```\n{prompt}\n```"
        );
    }

    let handoff_path = input_str(inputs, &["handoff_path"]).unwrap_or("(handoff)");
    let handoff_text = if let Some(path) = input_str(inputs, &["handoff_path"]) {
        resolve_under_project(path, project_root)
            .ok()
            .and_then(|p| fs::read_to_string(p).ok())
    } else if let Some(text) = input_str(inputs, &["handoff"]) {
        Some(text.to_string())
    } else {
        None
    };

    let (headers, _) = handoff_text
        .as_deref()
        .map(parse_handoff_headers_and_yaml)
        .unwrap_or_default();
    let request = handoff_text
        .as_deref()
        .and_then(|text| RenderRequest::from_document(text).ok());
    let prompt = request
        .as_ref()
        .map(|r| r.prompt.as_str())
        .unwrap_or("(missing or invalid scene-plan handoff)")
        .chars()
        .take(420)
        .collect::<String>();
    let duration = request.as_ref().map(|r| r.duration_seconds).unwrap_or(0);
    let aspect = request
        .as_ref()
        .map(|r| r.aspect_ratio.as_str())
        .unwrap_or("(unknown)");

    let mut refs = input_path_list(inputs, &["reference_assets", "reference_images"]);
    if refs.is_empty() {
        if let Some(request) = request.as_ref() {
            refs = request.reference_images.clone();
        }
    }
    let lane = request.as_ref().map(|r| r.lane);
    let model = match lane {
        Some(RenderLane::AudioDrivenI2v) => {
            capability_model(context, "video.lip_sync_text_narration")
                .map(|m| m.model_id)
                .unwrap_or_else(|| "(set Settings → Video → Lip sync)".into())
        }
        Some(RenderLane::ImageToVideo) => capability_model(context, "video.start_frame")
            .map(|m| m.model_id)
            .unwrap_or_else(|| "(set Settings → Video → Start frame)".into()),
        Some(RenderLane::ReferenceToVideo) => {
            capability_model(context, "video.character_reference")
                .or_else(|| capability_model(context, "video.location_prop_other_reference"))
                .map(|m| m.model_id)
                .unwrap_or_else(|| "(set Settings → Video → Character reference)".into())
        }
        Some(RenderLane::TextToVideo) => capability_model(context, "video.text_only")
            .map(|m| m.model_id)
            .unwrap_or_else(|| "(set Settings → Video → Text only)".into()),
        None => "(missing or invalid scene-plan handoff)".into(),
    };

    let out = if let Some(p) = input_str(inputs, &["output_path", "save_path"]) {
        p.to_string()
    } else {
        join_output(&headers, project_root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "(from handoff Output/Filenames)".into())
    };

    let ref_lines = if refs.is_empty() {
        "- (none — text-to-video)".into()
    } else {
        refs.iter()
            .map(|r| format!("- `{r}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let routing = if context.adult_routing_requested {
        "- routing: **adult/nsfw** (Adult generation defaults when set)\n"
    } else {
        "- routing: main Generation defaults\n"
    };
    format!(
        "### Scene generation plan\n\
{routing}\
- model: `{model}`\n\
- handoff: `{handoff_path}`\n\
- output: `{out}`\n\
- duration: {duration}s · aspect: {aspect}\n\
- reference assets (host will upload → `image_urls` / schema fields):\n{ref_lines}\n\n\
Prompt excerpt:\n```\n{prompt}\n```\n\n\
Use the Approve button only if this model + refs + output path look right."
    )
}

#[cfg(test)]
mod text_runner_tests {
    use super::*;

    #[test]
    fn parses_workbench_file_and_summary() {
        let reply = r#"
<workbench_file path="prod.md">
# Production Notes
hello
</workbench_file>
<workbench_file path='Scene_Prompts/Scene1.txt'>
Character_Reference: none
</workbench_file>
<workbench_summary>
Wrote master + 1 sidecar.
</workbench_summary>
"#;
        let emit = parse_text_specialist_emit(reply);
        assert!(emit.ask.is_none());
        assert_eq!(emit.files.len(), 2);
        assert_eq!(emit.files[0].0, "prod.md");
        assert!(emit.files[0].1.contains("# Production Notes"));
        assert_eq!(emit.files[1].0, "Scene_Prompts/Scene1.txt");
        assert_eq!(emit.summary.as_deref(), Some("Wrote master + 1 sidecar."));
    }

    #[test]
    fn parses_workbench_ask() {
        let reply = "<workbench_ask>
Which aspect ratio?
</workbench_ask>
";
        let emit = parse_text_specialist_emit(reply);
        assert_eq!(emit.ask.as_deref(), Some("Which aspect ratio?"));
        assert!(emit.files.is_empty());
    }
}

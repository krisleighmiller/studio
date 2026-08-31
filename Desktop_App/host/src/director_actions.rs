//! Validated Director actions.
//!
//! The chat model is an unreliable planner only. It may propose actions; the host
//! alone validates paths, enforces approval/spend rules, and talks to providers.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const PENDING_ACTION_START: &str = "<!--workbench_pending_action";
pub const PENDING_ACTION_END: &str = "-->";

pub const SPECIALISTS: &[&str] = &[
    "script",
    "character_creation",
    "scene_plan",
    "model_sheet",
    "still_image",
    "scene_generation",
    "text_sync",
    "lip_sync",
    "assemble",
    "production_helper",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    RunSpecialist,
    Inspect,
    AskUser,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorAction {
    pub action: ActionKind,
    #[serde(default)]
    pub specialist: Option<String>,
    #[serde(default)]
    pub inputs: Map<String, Value>,
    #[serde(default)]
    pub requires_approval: bool,
    /// For ask_user / done
    #[serde(default)]
    pub message: Option<String>,
    /// For inspect
    #[serde(default)]
    pub op: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidatedAction {
    pub action: DirectorAction,
    pub resolved_paths: Vec<(String, PathBuf)>,
}

#[derive(Debug)]
pub enum ActionError {
    Parse(String),
    Validate(String),
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionError::Parse(m) | ActionError::Validate(m) => write!(f, "{m}"),
        }
    }
}

fn take_json_object(text: &str) -> Option<(Value, usize)> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let slice = &text[start..=i];
                    let value = serde_json::from_str(slice).ok()?;
                    return Some((value, i + 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn value_to_action(value: &Value) -> Result<DirectorAction, ActionError> {
    // Accept action as string.
    let action_str = value
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ActionError::Parse(
                "missing action (allowed: run_specialist, inspect, ask_user, done). For post/final cut emit specialist:\"assemble\" with assemble_map or production_path+output_path — do not invent scene_id JSON."
                    .into(),
            )
        })?
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");

    let kind = match action_str.as_str() {
        "run_specialist" => ActionKind::RunSpecialist,
        "inspect" => ActionKind::Inspect,
        "ask_user" => ActionKind::AskUser,
        "done" => ActionKind::Done,
        other => {
            return Err(ActionError::Parse(format!(
                "unknown action `{other}` (allowed: run_specialist, inspect, ask_user, done)"
            )));
        }
    };

    let mut inputs = match value.get("inputs") {
        Some(Value::Object(map)) => map.clone(),
        Some(Value::Null) | None => Map::new(),
        Some(_) => {
            return Err(ActionError::Parse(
                "inputs must be an object when present".into(),
            ));
        }
    };

    // Hoist common top-level fields into inputs for forgiving planners.
    for key in [
        "handoff_path",
        "handoff",
        "output_path",
        "save_path",
        "source_frame",
        "source_image",
        "start_frame",
        "source_video",
        "video",
        "audio",
        "prompt",
        "brief",
        "edit_brief",
        "character_files",
        "reference_images",
        "reference_assets",
        "preserve_props",
        "model",
        "production_path",
        "production",
        "assemble_map",
        "map_path",
        "assemble_path",
        "final_prompt",
        "discover",
        "trim",
        "scenes",
        "overwrite",
        "music_path",
        "caption_mode",
    ] {
        if !inputs.contains_key(key) {
            if let Some(v) = value.get(key) {
                if !v.is_null() {
                    inputs.insert(key.to_string(), v.clone());
                }
            }
        }
    }

    let requires_approval = value
        .get("requires_approval")
        .and_then(|v| v.as_bool())
        .unwrap_or(matches!(kind, ActionKind::RunSpecialist));

    Ok(DirectorAction {
        action: kind,
        specialist: value
            .get("specialist")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase().replace('-', "_")),
        inputs,
        requires_approval,
        message: value
            .get("message")
            .or_else(|| value.get("reply"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        op: value
            .get("op")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_ascii_lowercase()),
        path: value
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

/// Parse one or more director actions from model text.
pub fn parse_actions(text: &str) -> Result<Vec<DirectorAction>, ActionError> {
    let mut actions = Vec::new();
    let mut search = text;

    // Prefer fenced <director_action> blocks.
    while let Some(start) = search.find("<director_action>") {
        let after = &search[start + "<director_action>".len()..];
        let end = after
            .find("</director_action>")
            .ok_or_else(|| ActionError::Parse("unclosed <director_action>".into()))?;
        let body = after[..end].trim();
        // Accept pure JSON, or JSON buried in markdown/prose inside the tag.
        let value: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(first_err) => match take_json_object(body) {
                Some((v, _)) => v,
                None => {
                    return Err(ActionError::Parse(format!(
                        "invalid JSON inside <director_action>: {first_err}"
                    )));
                }
            },
        };
        actions.push(value_to_action(&value)?);
        search = &after[end + "</director_action>".len()..];
    }
    if !actions.is_empty() {
        return Ok(actions);
    }

    // Bare JSON object(s) that look like director actions.
    search = text;
    while let Some((value, json_end)) = take_json_object(search) {
        if value.get("action").and_then(|v| v.as_str()).is_some() {
            match value_to_action(&value) {
                Ok(action) => actions.push(action),
                Err(_) => {}
            }
        }
        if json_end >= search.len() {
            break;
        }
        search = &search[json_end..];
    }
    if actions.is_empty() {
        Err(ActionError::Parse(
            "no director action found (emit <director_action>{...}</director_action>)".into(),
        ))
    } else {
        Ok(actions)
    }
}

pub fn resolve_under_project(
    raw: &str,
    project_root: Option<&str>,
) -> Result<PathBuf, ActionError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(ActionError::Validate("empty path".into()));
    }
    let candidate = {
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            path
        } else if let Some(root) = project_root {
            PathBuf::from(root).join(path)
        } else {
            return Err(ActionError::Validate(format!(
                "relative path `{raw}` needs an open project root"
            )));
        }
    };
    let Some(root) = project_root.map(PathBuf::from) else {
        // No project: allow absolute paths only (already absolute).
        return Ok(candidate);
    };
    let root_canon = root
        .canonicalize()
        .unwrap_or_else(|_| root.clone());
    // For paths that do not exist yet (save targets), canonicalize parent.
    let checked = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|e| ActionError::Validate(format!("path error: {e}")))?
    } else if let Some(parent) = candidate.parent() {
        let parent_canon = if parent.as_os_str().is_empty() {
            root_canon.clone()
        } else if parent.exists() {
            parent
                .canonicalize()
                .map_err(|e| ActionError::Validate(format!("parent path error: {e}")))?
        } else {
            // Allow creating new subpaths under root without parent existing yet —
            // verify lexicographic prefix after normalize.
            let norm_root = normalize_for_prefix(&root_canon);
            let norm_cand = normalize_for_prefix(&candidate);
            if !norm_cand.starts_with(&norm_root) {
                return Err(ActionError::Validate(format!(
                    "path `{}` escapes project root `{}`",
                    candidate.display(),
                    root.display()
                )));
            }
            return Ok(candidate);
        };
        if !parent_canon.starts_with(&root_canon) && parent_canon != root_canon {
            return Err(ActionError::Validate(format!(
                "path `{}` escapes project root `{}`",
                candidate.display(),
                root.display()
            )));
        }
        candidate
    } else {
        candidate
    };

    if checked.exists() {
        let canon = checked
            .canonicalize()
            .map_err(|e| ActionError::Validate(format!("path error: {e}")))?;
        if !canon.starts_with(&root_canon) {
            return Err(ActionError::Validate(format!(
                "path `{}` escapes project root `{}`",
                canon.display(),
                root.display()
            )));
        }
        Ok(canon)
    } else {
        Ok(checked)
    }
}

fn normalize_for_prefix(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('\\', "/");
    while s.ends_with('/') {
        s.pop();
    }
    s
}

fn collect_path_keys(inputs: &Map<String, Value>) -> Vec<(String, String)> {
    const KEYS: &[&str] = &[
        "handoff_path",
        "output_path",
        "save_path",
        "output_video",
        "output_audio",
        "production_path",
        "production",
        "prod_path",
        "prod",
        "music_path",
        "music",
        "work_dir",
        "source_frame",
        "source_image",
        "start_frame",
        "character_image",
        "source_video",
        "video",
        "audio",
        "voice_clone",
        "voice_clone_path",
        "clone_wav",
        "voice_profile",
        "voice_profile_path",
        "performance_wav",
        "reuse_audio",
        "path",
    ];
    let mut out = Vec::new();
    for key in KEYS {
        match inputs.get(*key) {
            Some(Value::String(s)) if !s.trim().is_empty() => {
                out.push(((*key).to_string(), s.to_string()));
            }
            Some(Value::Array(items))
                if matches!(
                    *key,
                    "source_frame" | "source_image" | "reference_images" | "reference_assets"
                ) =>
            {
                for (i, item) in items.iter().enumerate() {
                    if let Some(s) = item.as_str() {
                        if !s.trim().is_empty() {
                            out.push((format!("{key}[{i}]"), s.to_string()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for key in ["character_files", "reference_images", "reference_assets"] {
        match inputs.get(key) {
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    if let Some(s) = item.as_str() {
                        if !s.trim().is_empty() {
                            out.push((format!("{key}[{i}]"), s.to_string()));
                        }
                    }
                }
            }
            Some(Value::String(s)) if !s.trim().is_empty() => {
                out.push(((*key).to_string(), s.to_string()));
            }
            _ => {}
        }
    }
    out
}

fn looks_like_capability_slot(name: &str) -> bool {
    name.starts_with("image.")
        || name.starts_with("video.")
        || name.starts_with("audio.")
}

/// Input paths that must already exist as files before a specialist may run / be approved.
fn require_existing_files(
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
    keys: &[&str],
) -> Result<(), ActionError> {
    for key in keys {
        match inputs.get(*key) {
            Some(Value::String(raw)) if !raw.trim().is_empty() => {
                let path = resolve_under_project(raw, project_root)?;
                if !path.is_file() {
                    return Err(ActionError::Validate(format!(
                        "`{key}` must be an existing file (got `{}`). For a new still, omit source_frame/source_image and pass prompt/brief + output_path only.",
                        path.display()
                    )));
                }
            }
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    if let Some(raw) = item.as_str() {
                        if raw.trim().is_empty() {
                            continue;
                        }
                        let path = resolve_under_project(raw, project_root)?;
                        if !path.is_file() {
                            return Err(ActionError::Validate(format!(
                                "`{key}[{i}]` must be an existing file (got `{}`)",
                                path.display()
                            )));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for key in ["reference_images", "reference_assets", "character_files"] {
        match inputs.get(key) {
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    if let Some(raw) = item.as_str() {
                        if raw.trim().is_empty() {
                            continue;
                        }
                        let path = resolve_under_project(raw, project_root)?;
                        if !path.is_file() {
                            return Err(ActionError::Validate(format!(
                                "`{key}[{i}]` must be an existing file (got `{}`)",
                                path.display()
                            )));
                        }
                    }
                }
            }
            Some(Value::String(raw)) if !raw.trim().is_empty() => {
                let path = resolve_under_project(raw, project_root)?;
                if !path.is_file() {
                    return Err(ActionError::Validate(format!(
                        "`{key}` must be an existing file (got `{}`)",
                        path.display()
                    )));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate an action and resolve project-relative paths.
pub fn validate_action(
    action: DirectorAction,
    project_root: Option<&str>,
) -> Result<ValidatedAction, ActionError> {
    let mut resolved_paths = Vec::new();
    match action.action {
        ActionKind::AskUser | ActionKind::Done => {
            let msg = action
                .message
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ActionError::Validate("ask_user/done requires a non-empty message".into())
                })?;
            if msg.len() > 20_000 {
                return Err(ActionError::Validate("message too long".into()));
            }
            Ok(ValidatedAction {
                action,
                resolved_paths,
            })
        }
        ActionKind::Inspect => {
            let op = action
                .op
                .as_deref()
                .unwrap_or("read_file")
                .to_ascii_lowercase();
            if !matches!(op.as_str(), "read_file" | "list_dir" | "path_exists") {
                return Err(ActionError::Validate(format!(
                    "inspect op `{op}` not allowed (read_file|list_dir|path_exists)"
                )));
            }
            let path = action
                .path
                .as_deref()
                .or_else(|| action.inputs.get("path").and_then(|v| v.as_str()))
                .ok_or_else(|| ActionError::Validate("inspect requires path".into()))?;
            let resolved = resolve_under_project(path, project_root)?;
            // Reject writes / escapes already handled. For list/read, path should be under root.
            resolved_paths.push(("path".into(), resolved));
            let mut action = action;
            action.op = Some(op);
            Ok(ValidatedAction {
                action,
                resolved_paths,
            })
        }
        ActionKind::RunSpecialist => {
            let specialist = action
                .specialist
                .as_deref()
                .ok_or_else(|| ActionError::Validate("run_specialist requires specialist".into()))?;
            if !SPECIALISTS.contains(&specialist) {
                let hint = if looks_like_capability_slot(specialist) {
                    format!(
                        "`{specialist}` is a Settings capability slot, not a specialist. \
Allowed specialists: {}. For stills use `still_image` (new image: prompt/brief + output_path; \
edit: existing source_frame + edit_brief). The host picks the slot model after you choose the specialist.",
                        SPECIALISTS.join(", ")
                    )
                } else {
                    format!(
                        "unknown specialist `{specialist}` (allowed: {})",
                        SPECIALISTS.join(", ")
                    )
                };
                return Err(ActionError::Validate(hint));
            }
            // Reject unknown input keys (strict allowlist per specialist).
            let allowed = allowed_input_keys(specialist);
            for key in action.inputs.keys() {
                if !allowed.contains(key.as_str()) {
                    return Err(ActionError::Validate(format!(
                        "specialist `{specialist}` does not accept input field `{key}`"
                    )));
                }
            }
            for (label, raw) in collect_path_keys(&action.inputs) {
                let resolved = resolve_under_project(&raw, project_root)?;
                resolved_paths.push((label, resolved));
            }
            // Specialist-specific required fields
            match specialist {
                "still_image" => {
                    require_any(&action.inputs, &["output_path", "save_path"])?;
                    require_any(&action.inputs, &["prompt", "brief", "edit_brief"])?;
                    require_existing_files(
                        &action.inputs,
                        project_root,
                        &["source_frame", "source_image"],
                    )?;
                }
                "scene_generation" => {
                    let has_handoff = action.inputs.contains_key("handoff_path")
                        || action.inputs.contains_key("handoff");
                    let has_edit = action.inputs.contains_key("source_video")
                        && (action.inputs.contains_key("edit_brief")
                            || action.inputs.contains_key("prompt")
                            || action.inputs.contains_key("brief"));
                    let has_prompt = action.inputs.contains_key("prompt")
                        || action.inputs.contains_key("brief")
                        || action.inputs.contains_key("edit_brief");
                    // Freeform video: prompt + output (optional start frame / refs). No handoff required.
                    let has_freeform_video = has_prompt;
                    if !has_handoff && !has_edit && !has_freeform_video {
                        return Err(ActionError::Validate(
                            "scene_generation needs handoff_path/handoff, or source_video + edit brief + output_path, or freeform prompt + output_path (optional source_frame / reference_images)"
                                .into(),
                        ));
                    }
                    if has_edit {
                        require_any(&action.inputs, &["output_path", "save_path"])?;
                        require_existing_files(
                            &action.inputs,
                            project_root,
                            &["source_video"],
                        )?;
                    }
                    if has_freeform_video && !has_handoff && !has_edit {
                        require_any(&action.inputs, &["output_path", "save_path"])?;
                        require_existing_files(
                            &action.inputs,
                            project_root,
                            &["source_frame", "source_image", "start_frame"],
                        )?;
                    }
                }
                "text_sync" => {
                    let has_handoff = action.inputs.contains_key("handoff_path")
                        || action.inputs.contains_key("handoff");
                    if !has_handoff {
                        return Err(ActionError::Validate(
                            "text_sync needs handoff_path or handoff YAML text".into(),
                        ));
                    }
                    // Output paths may come from handoff Filenames; when provided, accept them.
                    // Voice / character image may come from handoff headers or explicit inputs.
                    require_existing_files(
                        &action.inputs,
                        project_root,
                        &[
                            "character_image",
                            "source_frame",
                            "source_image",
                            "voice_clone",
                            "voice_clone_path",
                            "clone_wav",
                            "voice_profile",
                            "voice_profile_path",
                            "performance_wav",
                            "reuse_audio",
                        ],
                    )?;
                }
                "lip_sync" => {
                    require_any(&action.inputs, &["video", "source_video"])?;
                    require_any(&action.inputs, &["audio"])?;
                    require_any(&action.inputs, &["output_path", "save_path"])?;
                    require_existing_files(
                        &action.inputs,
                        project_root,
                        &["video", "source_video", "audio"],
                    )?;
                }
                "assemble" => {
                    let mut merged = action.inputs.clone();
                    crate::assemble::merge_assemble_map_into_inputs(&mut merged, project_root)
                        .map_err(ActionError::Validate)?;
                    let has_map = [
                        "assemble_map",
                        "map_path",
                        "assemble_path",
                        "final_prompt",
                        "assemble_prompt",
                    ]
                    .iter()
                    .any(|k| action.inputs.contains_key(*k) || merged.contains_key(*k));
                    if !has_map {
                        require_any(
                            &merged,
                            &["production_path", "production", "prod_path", "prod"],
                        )?;
                        require_any(&merged, &["output_path", "output", "save_path"])?;
                    } else {
                        // Map file may supply production/output/scenes; still require them after merge.
                        require_any(
                            &merged,
                            &["production_path", "production", "prod_path", "prod"],
                        )?;
                        require_any(&merged, &["output_path", "output", "save_path"])?;
                    }
                    require_existing_files(
                        &merged,
                        project_root,
                        &[
                            "production_path",
                            "production",
                            "prod_path",
                            "prod",
                            "music_path",
                            "music",
                            "assemble_map",
                            "map_path",
                            "assemble_path",
                            "final_prompt",
                            "assemble_prompt",
                        ],
                    )?;
                    let has_scenes = merged
                        .get("scenes")
                        .and_then(|v| v.as_array())
                        .is_some_and(|a| !a.is_empty());
                    let discover = merged
                        .get("discover")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        || merged.get("discover").and_then(|v| v.as_str()).is_some_and(
                            |s| {
                                matches!(
                                    s.trim().to_ascii_lowercase().as_str(),
                                    "true" | "yes" | "1"
                                )
                            },
                        );
                    if !has_scenes && !discover {
                        return Err(ActionError::Validate(
                            "assemble needs scenes[] (id + video per scene), discover:true, or an assemble_map that lists scenes"
                                .into(),
                        ));
                    }
                    // Nested scene videos must exist when provided.
                    if let Some(Value::Array(items)) = merged.get("scenes") {
                        for (i, item) in items.iter().enumerate() {
                            let Some(obj) = item.as_object() else {
                                return Err(ActionError::Validate(format!(
                                    "assemble scenes[{i}] must be an object"
                                )));
                            };
                            let video = obj
                                .get("video")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| {
                                    ActionError::Validate(format!(
                                        "assemble scenes[{i}] missing video"
                                    ))
                                })?;
                            let path = resolve_under_project(video, project_root)?;
                            if !path.is_file() {
                                return Err(ActionError::Validate(format!(
                                    "assemble scenes[{i}].video not found: {}",
                                    path.display()
                                )));
                            }
                            if let Some(audio) = obj.get("audio").and_then(|v| v.as_str()) {
                                let ap = resolve_under_project(audio, project_root)?;
                                if !ap.is_file() {
                                    return Err(ActionError::Validate(format!(
                                        "assemble scenes[{i}].audio not found: {}",
                                        ap.display()
                                    )));
                                }
                            }
                        }
                    }
                    // Host plan gate: probe durations / trim math before approval UI.
                    crate::assemble::validate_plan(&merged, project_root)
                        .map_err(ActionError::Validate)?;
                }
                "model_sheet" => {
                    require_any(&action.inputs, &["output_path", "save_path"])?;
                    // Identity: character sheet and/or refs and/or text brief.
                    let has_identity = action.inputs.contains_key("character_files")
                        || action.inputs.contains_key("reference_images")
                        || action.inputs.contains_key("reference_assets")
                        || action.inputs.contains_key("identity_text")
                        || action.inputs.contains_key("brief")
                        || action.inputs.contains_key("prompt");
                    if !has_identity {
                        return Err(ActionError::Validate(
                            "model_sheet needs character_files, reference_images, and/or identity_text/brief"
                                .into(),
                        ));
                    }
                    require_existing_files(&action.inputs, project_root, &[])?;
                }
                "script" | "character_creation" | "scene_plan" | "production_helper" => {
                    // Soft: path targets optional; message/brief helpful
                }
                _ => {}
            }
            Ok(ValidatedAction {
                action,
                resolved_paths,
            })
        }
    }
}

fn require_any(inputs: &Map<String, Value>, keys: &[&str]) -> Result<(), ActionError> {
    let ok = keys.iter().any(|k| {
        inputs
            .get(*k)
            .map(|v| match v {
                Value::String(s) => !s.trim().is_empty(),
                Value::Array(a) => !a.is_empty(),
                Value::Null => false,
                _ => true,
            })
            .unwrap_or(false)
    });
    if ok {
        Ok(())
    } else {
        Err(ActionError::Validate(format!(
            "missing required input (one of: {})",
            keys.join(", ")
        )))
    }
}

fn allowed_input_keys(specialist: &str) -> HashSet<&'static str> {
    let base: &[&str] = match specialist {
        "still_image" => &[
            "prompt",
            "brief",
            "edit_brief",
            "output_path",
            "save_path",
            "source_frame",
            "source_image",
            "character_files",
            "reference_images",
            "reference_assets",
            "preserve_props",
            "model",
            "aspect_ratio",
            "resolution",
            "collect_prediction_id",
        ],
        "scene_generation" => &[
            "handoff_path",
            "handoff",
            "output_path",
            "save_path",
            "source_video",
            "source_frame",
            "source_image",
            "start_frame",
            "prompt",
            "brief",
            "edit_brief",
            "reference_images",
            "reference_assets",
            "model",
            "duration",
            "aspect_ratio",
            "collect_prediction_id",
        ],
        "text_sync" => &[
            "handoff_path",
            "handoff",
            "output_video",
            "output_path",
            "save_path",
            "output_audio",
            "character_image",
            "source_frame",
            "source_image",
            "reference_images",
            "reference_assets",
            "voice_clone",
            "voice_clone_path",
            "clone_wav",
            "voice_profile",
            "voice_profile_path",
            "performance_wav",
            "reuse_audio",
            "sync_ms",
            "model",
            "audio_model",
            "pad_to_9x16",
            "collect_prediction_id",
        ],
        "lip_sync" => &[
            "video",
            "source_video",
            "audio",
            "output_path",
            "save_path",
            "model",
            "collect_prediction_id",
        ],
        "assemble" => &[
            "production_path",
            "production",
            "prod_path",
            "prod",
            "output_path",
            "output",
            "save_path",
            "assemble_map",
            "map_path",
            "assemble_path",
            "final_prompt",
            "assemble_prompt",
            "scenes",
            "discover",
            "prefer_retry",
            "trim",
            "caption_mode",
            "captions",
            "caption_language",
            "language",
            "music_path",
            "music",
            "music_volume",
            "volume",
            "duck_under_dialogue",
            "duck",
            "work_dir",
            "overwrite",
        ],
        "model_sheet" => &[
            "character_files",
            "output_path",
            "save_path",
            "brief",
            "prompt",
            "identity_text",
            "style",
            "wardrobe",
            "reference_images",
            "reference_assets",
            "model",
            "shot_ids",
            "phase",
            "aspect_ratio",
            "resolution",
            "character_slug",
            "identity_gate",
        ],
        "script" | "character_creation" | "scene_plan" => &[
            "brief",
            "prompt",
            "question",
            "message",
            "output_path",
            "save_path",
            "source_path",
            "paths",
            "source_paths",
            "documents",
            "files",
            "path",
            "character_files",
            "handoff_path",
            "production_path",
            "production",
            "prod_path",
            "sidecar_dir",
            "sidecar_folder",
            "scene_prompts_dir",
            "prompts_dir",
            "aspect_ratio",
            "format",
        ],
        "production_helper" => &[
            "brief",
            "prompt",
            "question",
            "message",
            "paths",
            "source_paths",
            "path",
            "source_path",
            "documents",
            "files",
            "output_path",
            "save_path",
            "mode",
        ],
        _ => &[],
    };
    base.iter().copied().collect()
}

pub fn encode_pending_action(action: &DirectorAction) -> String {
    let json = serde_json::to_string(action).unwrap_or_else(|_| "{}".into());
    format!("{PENDING_ACTION_START}\n{json}\n{PENDING_ACTION_END}")
}

pub fn find_pending_action_in_history(messages: &[(String, String)]) -> Option<DirectorAction> {
    for (role, content) in messages.iter().rev() {
        if role != "assistant" {
            continue;
        }
        if let Some(start) = content.find(PENDING_ACTION_START) {
            let after = &content[start + PENDING_ACTION_START.len()..];
            let end = after.find(PENDING_ACTION_END)?;
            let body = after[..end].trim();
            if let Ok(value) = serde_json::from_str::<Value>(body) {
                if let Ok(action) = value_to_action(&value) {
                    return Some(action);
                }
            }
        }
    }
    None
}

pub fn format_approval_request(action: &DirectorAction, resolved: &ValidatedAction) -> String {
    let specialist = action.specialist.as_deref().unwrap_or("(none)");
    let mut lines = vec![
        "## Approval required".to_string(),
        String::new(),
        format!("Specialist: `{specialist}`"),
        "The host validated this plan. Reply **Approved** to run it (provider spend and/or local render), or say what to change.".to_string(),
        String::new(),
        "### Inputs".to_string(),
        "```json".to_string(),
        serde_json::to_string_pretty(&Value::Object(action.inputs.clone()))
            .unwrap_or_else(|_| "{}".into()),
        "```".to_string(),
    ];
    if !resolved.resolved_paths.is_empty() {
        lines.push(String::new());
        lines.push("### Resolved paths".to_string());
        for (label, path) in &resolved.resolved_paths {
            lines.push(format!("- `{label}` → `{}`", path.display()));
        }
    }
    lines.push(String::new());
    lines.push(encode_pending_action(action));
    lines.join("\n")
}

pub fn planner_system_prompt() -> String {
    let specialists = SPECIALISTS.join(", ");
    format!(
        r#"You are the Director planner for the Video Production Workbench.

You are NOT allowed to call provider APIs, invent tool results, or access the filesystem yourself.
You ONLY emit one or more validated director actions. The host validates, enforces approval/spending, resolves paths, and runs specialists.
For script / character_creation / scene_plan / production_helper, the host loads the specialist contract and calls the selected chat model — you only route with a clear brief and paths.

## Output format (required)

Emit actions as:

<director_action>
{{"action":"run_specialist","specialist":"scene_generation","inputs":{{"handoff_path":"Prod_Notes/FlightPt1/Scene1.md"}},"requires_approval":true}}
</director_action>

Allowed actions:
- run_specialist — run a host specialist pipeline
- inspect — read-only project inspect (op: read_file | list_dir | path_exists, plus path)
- ask_user — ask a clarifying question (message)
- done — finished reply to the user (message)

Specialists: {specialists}

## Capability slots vs specialists (critical)

Context may list Settings slots like `image.text_to_image`, `image.edit`, `video.text_only`.
Those are **model defaults**, NOT specialists. Never put a slot id in the `specialist` field.
The host selects the correct slot after you choose a specialist and inputs.

## Routing
- Scene handoff / generate this scene / send scene for video → scene_generation (handoff_path or attached scene file)
- Focused video edit of an attached clip → scene_generation with source_video + edit_brief + output_path
- **Freeform video** (user prompt, no handoff file) → scene_generation with `prompt` + `output_path`. Optional stills:
  - start-frame I2V: also pass `source_frame` → host uses `video.start_frame`
  - reference-to-video: also pass `reference_images` → host uses `video.character_reference` / `video.location_prop_other_reference`
  - text-to-video only: just `prompt` + `output_path` → host uses `video.text_only`
  Adult/nsfw keywords switch those slots to Adult generation defaults. Never invent a handoff path for a freeform prompt.
- Still / new image / edit this image → still_image (png/jpg stills only — not video)
- **Text-sync / talking shot / audio-driven I2V** (handoff `speech.mode: source_video_lipsync` or `audio_driven_i2v`, Van/Wan image+audio) → text_sync
- Post lip-sync of an existing silent/base clip + WAV via Sync Labs → lip_sync
- **Assemble / post / final cut / master** → assemble (host ffmpeg; plan approval required)
- Character sheet text → character_creation (brief + paths/attachments; optional output_path). Host runs the Character Creation contract and writes the sheet.
- Script draft/revise → script (brief + source paths; optional output_path). Host runs the Script Assistant contract and writes the draft.
- Scene plan / prod.md / Scene_Prompts → scene_plan (brief + script/character paths; optional production_path / sidecar_dir / aspect_ratio). Host runs the Scene Plan contract and writes master + sidecars. No approval — local files only.
- Multi-angle / green-screen character identity pack → model_sheet (output_path = pack directory, character_files and/or reference_images). Required for character continuity in video — including spicy/NSFW — before scene_generation when faces/bodies must stay locked.
- **Review / sanity-check docs / “is this correct” / brainstorm failing image or video prompts / research digests / idea packet** → production_helper (advisory; no media spend)

### production_helper inputs
- `question` / `brief` / `prompt` / `message` — what to check or brainstorm (required in spirit; host can fall back to the user turn)
- `paths` / `source_paths` / `documents` / `files` / `path` — project docs to read (array or string). Prefer attachments / focused editor / selected path when already in context.
- Optional `output_path` — write a research/idea digest markdown there; otherwise reply in chat only
- Optional `mode`: `review` | `brainstorm` | `digest` (hint only)

### assemble inputs
- Preferred: `assemble_map` / `final_prompt` path to a YAML map (e.g. `Final/final_prompt.txt`) that already lists production, output, trim, captions, and scenes. Host loads it.
- Or inline: `production_path` (prod.md), `output_path` (master .mp4), plus `scenes` array of `{{id, video, audio?}}` **or** `discover:true`
- Optional: `trim` (`handles` | `estimated` | `full`; host may auto-pick `estimated` for talking-shot overshoot), `caption_mode`, `music_path`, `overwrite`, `work_dir`
- Host validates paths, probes durations, builds the Assemble Plan, and runs ffmpeg after **Approved**.
- Do **not** invent scene-generation JSON, fake `scene_id` blocks, or ffmpeg commands. Assemble is only `run_specialist` + `assemble`.

### text_sync inputs
- Required: `handoff_path` or `handoff` (talking-shot handoff with `speech.cues`)
- Paths (handoff headers OK if omitted): `output_video` / `output_path`, `output_audio`, `character_image` / `source_frame`
- Voice (one of): `voice_clone` / `clone_wav`, `voice_profile`, or `performance_wav` / `reuse_audio` to skip TTS
- Optional: `sync_ms` (default 150), `model` (video), `audio_model`, `pad_to_9x16`

### still_image inputs
- **New still** (text-to-image, including adult/NSFW prompts): `prompt` or `brief` + `output_path` only. Do **not** invent `source_frame`.
- **Focused edit**: existing `source_frame`/`source_image` (file must already exist) + `edit_brief` + `output_path`.
- Multi-image edit (“Image 1 / Image 2”): pass `source_frame` as an **array** of existing paths (first = Image 1, rest = Image 2…). Host uses Settings `image.edit`.
- Never set `source_frame` equal to a path that does not exist yet.
- When the user asks which model a pending still plan will use, answer with the Settings slot model — do **not** re-emit the same approval action.

### scene_generation freeform inputs (no handoff)
- Text-to-video: `prompt` + `output_path` (e.g. `…/Videos/clip.mp4`)
- Start-frame: existing `source_frame` + `prompt` + `output_path`
- Reference-to-video: `reference_images` + `prompt` + `output_path`
- Optional: `duration`, `aspect_ratio`
- Do **not** ask for a handoff path when the user already gave a prompt + video output path.

## Adult / NSFW model routing
When the user asks to **use** adult/nsfw/spicy models for generation, the host switches that turn to Settings → Adult content defaults. Unset adult slots fall back to main Generation defaults. Adult routing does **not** change which specialist you pick — still use `still_image` / `scene_generation` / etc. You do not pick model IDs yourself.

## Rules
1. Prefer absolute paths under the project root, or project-relative paths.
2. Paid / generative specialists should set requires_approval:true unless the user already clearly approved this exact plan.
3. Do not dump YAML handoffs as your answer. Propose an action.
4. Do not emit legacy <tool_call> provider tool blocks. They are ignored.
5. You may emit a short prose line before actions, but actions are what the host runs.
6. Use inspect when you need to read a project file that is not already in the context packet.
7. After inspect results, continue with run_specialist / ask_user / done.

## Examples

Inspect a file:
<director_action>
{{"action":"inspect","op":"read_file","path":"Prod_Notes/FlightPt1/Scene1.md"}}
</director_action>

Generate a scene from a handoff (asks host to validate then spend):
<director_action>
{{"action":"run_specialist","specialist":"scene_generation","inputs":{{"handoff_path":"Prod_Notes/FlightPt1/Scene1.md"}},"requires_approval":true}}
</director_action>

New still (text-to-image — no source_frame):
<director_action>
{{"action":"run_specialist","specialist":"still_image","inputs":{{"prompt":"wide establishing shot of a rainy alley at dusk","output_path":"Locations/alley_start.png"}},"requires_approval":true}}
</director_action>

Still edit (source file must already exist):
<director_action>
{{"action":"run_specialist","specialist":"still_image","inputs":{{"source_frame":"Locations/frame.png","edit_brief":"remove coffee cup","output_path":"Locations/frame_v2.png","preserve_props":true}},"requires_approval":false}}
</director_action>

Text-to-video (freeform prompt, no handoff):
<director_action>
{{"action":"run_specialist","specialist":"scene_generation","inputs":{{"prompt":"wide drone shot over misty pine forest at dawn","output_path":"Videos/forest_dawn.mp4","duration":5}},"requires_approval":true}}
</director_action>

Start-frame / image-to-video (no handoff):
<director_action>
{{"action":"run_specialist","specialist":"scene_generation","inputs":{{"source_frame":"Locations/alley_start.png","prompt":"slow push in as rain intensifies","output_path":"Videos/alley_start.mp4","duration":5}},"requires_approval":true}}
</director_action>

Character model sheet (continuity pack):
<director_action>
{{"action":"run_specialist","specialist":"model_sheet","inputs":{{"character_files":["Characters/Mom.md"],"reference_images":["Characters/Mom/refs/face.png"],"output_path":"Characters/Mom/model_sheet","style":"hyperrealistic","brief":"lock Mom identity for later video"}},"requires_approval":true}}
</director_action>

Text-sync talking shot (TTS → pad → Van/Wan I2V+audio):
<director_action>
{{"action":"run_specialist","specialist":"text_sync","inputs":{{"handoff_path":"Scene_Prompts/Scene1.txt","voice_clone":"Mom/voice/sample.wav","character_image":"mom_vert.png","output_video":"Scenes/Scene1/SC1_textsync.mp4","output_audio":"Scenes/Scene1/SC1.wav"}},"requires_approval":true}}
</director_action>

Assemble master from a final_prompt map (preferred):
<director_action>
{{"action":"run_specialist","specialist":"assemble","inputs":{{"assemble_map":"Final/final_prompt.txt","overwrite":true}},"requires_approval":true}}
</director_action>

Assemble master with discover (no map file):
<director_action>
{{"action":"run_specialist","specialist":"assemble","inputs":{{"production_path":"prod.md","output_path":"Final/show_final.mp4","discover":true,"trim":"estimated","caption_mode":"none"}},"requires_approval":true}}
</director_action>

Review docs / brainstorm a failing prompt (no approval — chat only):
<director_action>
{{"action":"run_specialist","specialist":"production_helper","inputs":{{"question":"Is prod.md ready for assemble? Flag anything Text-Sync or Assemble will choke on.","paths":["prod.md","Final/final_prompt.txt"]}},"requires_approval":false}}
</director_action>

Scene plan / production notes (host loads Scene Plan contract; no approval — local files):
<director_action>
{{"action":"run_specialist","specialist":"scene_plan","inputs":{{"brief":"Build prod.md and Scene_Prompts/ from the attached script. Vertical 9:16. Leave Character_Reference and Voice empty for later.","paths":["Scripts/Episode1.md"],"aspect_ratio":"9:16"}},"requires_approval":false}}
</director_action>

Script draft (host loads Script Assistant contract; local file write):
<director_action>
{{"action":"run_specialist","specialist":"script","inputs":{{"brief":"Draft a shootable short-form script from the attached chapter.","paths":["source_chapter.md"],"output_path":"Scripts/draft.md"}},"requires_approval":false}}
</director_action>

Character sheet (host loads Character Creation contract; local file write):
<director_action>
{{"action":"run_specialist","specialist":"character_creation","inputs":{{"brief":"Extract Mom into a character sheet.","paths":["Scripts/draft.md"],"output_path":"Characters/Mom_ref.md"}},"requires_approval":false}}
</director_action>
"#
    )
}

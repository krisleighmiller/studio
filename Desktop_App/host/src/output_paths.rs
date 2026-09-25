//! Host-owned output path conventions.
//!
//! The planner may pass `output_path` when the user named a file. Otherwise the
//! host derives stills, videos, sheets, and masters the same way scene_plan
//! derives sidecar paths: from inputs the host already has, not from LLM invention.

use crate::director_actions::resolve_under_project;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const HOST_OWNED_INPUT_KEYS: &[&str] = &["model", "audio_model"];

/// Drop LLM model-id overrides. Settings slots are the only source of model IDs.
pub fn strip_host_owned_inputs(inputs: &mut Map<String, Value>) {
    for key in HOST_OWNED_INPUT_KEYS {
        inputs.remove(*key);
    }
}

pub fn first_input_str<'a>(inputs: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        match inputs.get(*key) {
            Some(Value::String(s)) if !s.trim().is_empty() => return Some(s.as_str()),
            Some(Value::Array(items)) => {
                for item in items {
                    if let Some(s) = item.as_str() {
                        if !s.trim().is_empty() {
                            return Some(s);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

pub fn has_explicit_output(inputs: &Map<String, Value>, keys: &[&str]) -> bool {
    first_input_str(inputs, keys).is_some()
}

pub fn slugify(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_whitespace()) && !out.ends_with('_') {
            out.push('_');
        }
        if out.len() >= 40 {
            break;
        }
    }
    out.trim_matches('_').to_string()
}

fn stem_of(raw: &str) -> String {
    Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(slugify)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "untitled".into())
}

fn file_stem_raw(raw: &str) -> Option<String> {
    Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parent_rel(raw: &str) -> Option<String> {
    let parent = Path::new(raw).parent()?;
    if parent.as_os_str().is_empty() || parent == Path::new(".") {
        return None;
    }
    Some(parent.to_string_lossy().replace('\\', "/"))
}

fn join_rel(dir: &str, name: &str) -> String {
    let dir = dir.trim().trim_end_matches('/').trim_end_matches('\\');
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

fn unique_rel(rel: String, project_root: Option<&str>, as_dir: bool) -> String {
    let candidate =
        resolve_under_project(&rel, project_root).unwrap_or_else(|_| PathBuf::from(&rel));
    let exists = if as_dir {
        candidate.is_dir()
    } else {
        candidate.is_file()
    };
    if !exists {
        return rel;
    }
    let path = Path::new(&rel);
    let parent = path
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|s| !s.is_empty() && s != ".")
        .unwrap_or_default();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let ext = path.extension().and_then(|s| s.to_str());
    for n in 2..100 {
        let name = match ext {
            Some(e) if !as_dir => format!("{stem}_v{n}.{e}"),
            _ => format!("{stem}_v{n}"),
        };
        let next = if parent.is_empty() {
            name
        } else {
            format!("{parent}/{name}")
        };
        let resolved =
            resolve_under_project(&next, project_root).unwrap_or_else(|_| PathBuf::from(&next));
        let taken = if as_dir {
            resolved.is_dir()
        } else {
            resolved.is_file()
        };
        if !taken {
            return next;
        }
    }
    rel
}

fn slug_from_prompt(inputs: &Map<String, Value>, fallback: &str) -> String {
    first_input_str(
        inputs,
        &["prompt", "brief", "edit_brief", "question", "message"],
    )
    .map(slugify)
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| fallback.to_string())
}

fn project_slug(project_root: Option<&str>) -> String {
    project_root
        .and_then(|root| Path::new(root).file_name())
        .and_then(|n| n.to_str())
        .map(slugify)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "show".into())
}

/// Fill missing output paths on a specialist action. Existing output keys win.
pub fn ensure_output_paths(
    specialist: &str,
    inputs: &mut Map<String, Value>,
    project_root: Option<&str>,
) {
    match specialist {
        "still_image" => {
            if inputs.contains_key("batch") {
                return;
            }
            ensure_file(inputs, project_root, still_image_rel(inputs), false);
        }
        "scene_generation" => {
            if first_input_str(inputs, &["handoff_path", "handoff"]).is_some() {
                return;
            }
            ensure_file(inputs, project_root, scene_generation_rel(inputs), false);
        }
        "model_sheet" => ensure_file(inputs, project_root, model_sheet_rel(inputs), true),
        "assemble" => ensure_file(inputs, project_root, assemble_rel(project_root), false),
        "lip_sync" => ensure_file(inputs, project_root, lip_sync_rel(inputs), false),
        "text_sync" => {
            if has_explicit_output(inputs, &["output_video", "output_path", "save_path"]) {
                return;
            }
            if let Some(rel) = text_sync_rel(inputs) {
                inputs.insert("output_video".into(), Value::String(rel));
            }
        }
        "script" => ensure_file(inputs, project_root, script_rel(inputs), false),
        "character_creation" => {
            ensure_file(inputs, project_root, character_sheet_rel(inputs), false)
        }
        _ => {}
    }
}

fn ensure_file(
    inputs: &mut Map<String, Value>,
    project_root: Option<&str>,
    rel: Option<String>,
    as_dir: bool,
) {
    if has_explicit_output(
        inputs,
        &["output_path", "save_path", "output", "output_video"],
    ) {
        return;
    }
    let Some(rel) = rel else { return };
    let rel = unique_rel(rel, project_root, as_dir);
    inputs.insert("output_path".into(), Value::String(rel));
}

fn still_image_rel(inputs: &Map<String, Value>) -> Option<String> {
    if let Some(source) = first_input_str(inputs, &["source_frame", "source_image"]) {
        let stem = stem_of(source);
        let name = format!("{stem}_v2.png");
        return Some(match parent_rel(source) {
            Some(dir) => join_rel(&dir, &name),
            None => join_rel("Stills", &name),
        });
    }
    Some(join_rel(
        "Stills",
        &format!("{}.png", slug_from_prompt(inputs, "still")),
    ))
}

fn scene_generation_rel(inputs: &Map<String, Value>) -> Option<String> {
    if let Some(video) = first_input_str(inputs, &["source_video"]) {
        let stem = stem_of(video);
        let name = format!("{stem}_edit.mp4");
        return Some(match parent_rel(video) {
            Some(dir) => join_rel(&dir, &name),
            None => join_rel("Videos", &name),
        });
    }
    if let Some(frame) = first_input_str(inputs, &["source_frame", "source_image", "start_frame"]) {
        return Some(join_rel("Videos", &format!("{}.mp4", stem_of(frame))));
    }
    Some(join_rel(
        "Videos",
        &format!("{}.mp4", slug_from_prompt(inputs, "clip")),
    ))
}

fn model_sheet_rel(inputs: &Map<String, Value>) -> Option<String> {
    let slug = first_input_str(inputs, &["character_slug"])
        .map(slugify)
        .filter(|s| !s.is_empty())
        .or_else(|| first_input_str(inputs, &["character_files"]).and_then(file_stem_raw))
        .filter(|s| !s.is_empty() && s != "untitled")
        .unwrap_or_else(|| slug_from_prompt(inputs, "character"));
    Some(format!("Characters/{slug}/model_sheet"))
}

fn assemble_rel(project_root: Option<&str>) -> Option<String> {
    Some(format!("Final/{}_final.mp4", project_slug(project_root)))
}

fn lip_sync_rel(inputs: &Map<String, Value>) -> Option<String> {
    let video = first_input_str(inputs, &["video", "source_video"])?;
    let stem = stem_of(video);
    let name = format!("{stem}_lipsync.mp4");
    Some(match parent_rel(video) {
        Some(dir) => join_rel(&dir, &name),
        None => join_rel("Videos", &name),
    })
}

fn text_sync_rel(inputs: &Map<String, Value>) -> Option<String> {
    let handoff = first_input_str(inputs, &["handoff_path"])?;
    let stem = stem_of(handoff);
    Some(format!("Scenes/{stem}/{stem}.mp4"))
}

fn script_rel(inputs: &Map<String, Value>) -> Option<String> {
    Some(format!("Scripts/{}.md", slug_from_prompt(inputs, "draft")))
}

fn character_sheet_rel(inputs: &Map<String, Value>) -> Option<String> {
    Some(format!(
        "Characters/{}_ref.md",
        slug_from_prompt(inputs, "character")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn strips_model_overrides() {
        let mut inputs = map(json!({"prompt": "x", "model": "secret", "audio_model": "tts"}));
        strip_host_owned_inputs(&mut inputs);
        assert!(inputs.get("model").is_none());
        assert!(inputs.get("audio_model").is_none());
        assert_eq!(inputs.get("prompt").and_then(|v| v.as_str()), Some("x"));
    }

    #[test]
    fn derives_new_still_under_stills() {
        let mut inputs = map(json!({"prompt": "wide rainy alley at dusk"}));
        ensure_output_paths("still_image", &mut inputs, None);
        assert_eq!(
            inputs.get("output_path").and_then(|v| v.as_str()),
            Some("Stills/wide_rainy_alley_at_dusk.png")
        );
    }

    #[test]
    fn derives_still_edit_beside_source() {
        let mut inputs = map(json!({
            "source_frame": "Locations/frame.png",
            "edit_brief": "remove cup"
        }));
        ensure_output_paths("still_image", &mut inputs, None);
        assert_eq!(
            inputs.get("output_path").and_then(|v| v.as_str()),
            Some("Locations/frame_v2.png")
        );
    }

    #[test]
    fn keeps_explicit_output_path() {
        let mut inputs = map(json!({
            "prompt": "alley",
            "output_path": "Locations/alley_start.png"
        }));
        ensure_output_paths("still_image", &mut inputs, None);
        assert_eq!(
            inputs.get("output_path").and_then(|v| v.as_str()),
            Some("Locations/alley_start.png")
        );
    }

    #[test]
    fn derives_freeform_video_and_sheet() {
        let mut video = map(json!({"prompt": "misty pine forest at dawn"}));
        ensure_output_paths("scene_generation", &mut video, None);
        assert_eq!(
            video.get("output_path").and_then(|v| v.as_str()),
            Some("Videos/misty_pine_forest_at_dawn.mp4")
        );

        let mut sheet = map(json!({"character_files": ["Characters/Mom.md"]}));
        ensure_output_paths("model_sheet", &mut sheet, None);
        assert_eq!(
            sheet.get("output_path").and_then(|v| v.as_str()),
            Some("Characters/Mom/model_sheet")
        );

        let mut master = Map::new();
        ensure_output_paths("assemble", &mut master, Some("/tmp/My Show"));
        assert_eq!(
            master.get("output_path").and_then(|v| v.as_str()),
            Some("Final/my_show_final.mp4")
        );
    }

    #[test]
    fn skips_handoff_scene_generation() {
        let mut inputs = map(json!({"handoff_path": "Scene_Prompts/Scene1.txt"}));
        ensure_output_paths("scene_generation", &mut inputs, None);
        assert!(inputs.get("output_path").is_none());
    }
}

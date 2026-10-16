//! Host-owned assemble / post-production runner.
//!
//! The chat model only proposes structured inputs. This module validates paths,
//! builds the Assemble Plan (approval gate), and runs ffmpeg/ffprobe locally.
//! No Atlas generation. No LLM-invented filtergraphs at render time.

use crate::ctx::HostCtx;
use crate::director_actions::{resolve_under_project, ValidatedAction};
use crate::fs_ops::{probe_duration_seconds, probe_image_size};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrimMode {
    Handles,
    Estimated,
    Full,
    Custom,
}

impl TrimMode {
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "estimated" => Self::Estimated,
            "full" => Self::Full,
            "custom" => Self::Custom,
            _ => Self::Handles,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Handles => "handles",
            Self::Estimated => "estimated",
            Self::Full => "full",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptionMode {
    None,
    Soft,
    Burned,
    Both,
}

impl CaptionMode {
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "soft" => Self::Soft,
            "burned" | "burn" => Self::Burned,
            "both" => Self::Both,
            _ => Self::None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Soft => "soft",
            Self::Burned => "burned",
            Self::Both => "both",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransitionKind {
    Cut,
    Fade,
    Dissolve,
}

impl TransitionKind {
    fn parse(raw: &str) -> Self {
        let s = raw.trim().to_ascii_lowercase();
        if s.contains("dissolv") || s.contains("crossfade") || s.contains("xfade") {
            Self::Dissolve
        } else if s.contains("fade") {
            Self::Fade
        } else if s.contains("seamless") {
            Self::Cut
        } else {
            Self::Cut
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Cut => "cut",
            Self::Fade => "fade",
            Self::Dissolve => "dissolve",
        }
    }
}

#[derive(Debug, Clone)]
struct SpeechCue {
    text: String,
    estimated_duration: Option<f64>,
}

#[derive(Debug, Clone)]
struct SceneMeta {
    id: u32,
    head_s: f64,
    tail_s: f64,
    estimated_s: f64,
    speech_mode: String,
    transition_in: TransitionKind,
    transition_out: TransitionKind,
    cues: Vec<SpeechCue>,
}

#[derive(Debug, Clone)]
struct SceneAsset {
    id: u32,
    video: PathBuf,
    audio: Option<PathBuf>,
    trim_override: Option<TrimMode>,
    custom_start: Option<f64>,
    custom_duration: Option<f64>,
}

#[derive(Debug, Clone)]
struct ScenePlan {
    id: u32,
    video: PathBuf,
    audio: Option<PathBuf>,
    speech_mode: String,
    source_duration: f64,
    head_s: f64,
    tail_s: f64,
    estimated_s: f64,
    trim_start: f64,
    trim_duration: f64,
    transition_to_next: TransitionKind,
    has_audio_stream: bool,
    cues: Vec<SpeechCue>,
}

#[derive(Debug, Clone)]
struct AssemblePlan {
    production_path: PathBuf,
    output_path: PathBuf,
    work_dir: PathBuf,
    trim_mode: TrimMode,
    caption_mode: CaptionMode,
    caption_language: String,
    music_path: Option<PathBuf>,
    music_volume: f64,
    duck_under_dialogue: bool,
    overwrite: bool,
    fade_in_open: bool,
    fade_out_close: bool,
    scenes: Vec<ScenePlan>,
    sum_estimated: f64,
    warnings: Vec<String>,
}

fn input_str<'a>(inputs: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
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

fn input_bool(inputs: &Map<String, Value>, keys: &[&str], default: bool) -> bool {
    for key in keys {
        if let Some(v) = inputs.get(*key) {
            if let Some(b) = v.as_bool() {
                return b;
            }
            if let Some(s) = v.as_str() {
                match s.trim().to_ascii_lowercase().as_str() {
                    "true" | "yes" | "1" => return true,
                    "false" | "no" | "0" => return false,
                    _ => {}
                }
            }
        }
    }
    default
}

fn input_f64(inputs: &Map<String, Value>, keys: &[&str], default: f64) -> f64 {
    for key in keys {
        if let Some(v) = inputs.get(*key) {
            if let Some(n) = v.as_f64() {
                return n;
            }
            if let Some(n) = v.as_u64() {
                return n as f64;
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.trim().parse::<f64>() {
                    return n;
                }
            }
        }
    }
    default
}

fn is_talking_speech(mode: &str) -> bool {
    let m = mode.to_ascii_lowercase();
    m.contains("lipsync")
        || m.contains("voiceover")
        || m.contains("audio_driven")
        || m.contains("avatar")
}

fn unquote_yaml(raw: &str) -> String {
    let t = raw.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len().saturating_sub(1)].to_string()
    } else {
        t.to_string()
    }
}

fn yaml_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

/// Parse `Final/final_prompt.txt`-style assemble maps into action inputs.
/// Supports nested `assemble:`, list scenes, and `scenes: {1: path}` maps.
fn parse_assemble_map_text(text: &str) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    let mut scenes: Vec<Value> = Vec::new();
    let mut in_scenes = false;
    let mut in_captions = false;
    let mut pending_scene_id: Option<u32> = None;

    let flush_scene_path = |scenes: &mut Vec<Value>, id: Option<u32>, path: &str| {
        let Some(id) = id else { return };
        let path = path.trim();
        if path.is_empty() {
            return;
        }
        let mut obj = Map::new();
        obj.insert("id".into(), Value::from(id));
        obj.insert("video".into(), Value::String(unquote_yaml(path)));
        scenes.push(Value::Object(obj));
    };

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Top-level wrapper
        if trimmed == "assemble:" || trimmed.starts_with("assemble:") && !trimmed.contains('\n') {
            if trimmed == "assemble:" {
                in_scenes = false;
                in_captions = false;
                continue;
            }
        }

        let indent = line.chars().take_while(|c| *c == ' ').count();

        if trimmed.starts_with("scenes:") {
            in_scenes = true;
            in_captions = false;
            pending_scene_id = None;
            let rest = trimmed["scenes:".len()..].trim();
            if !rest.is_empty() && rest != "|" && rest != ">" {
                // inline not supported
            }
            continue;
        }
        if trimmed.starts_with("captions:") {
            in_scenes = false;
            in_captions = true;
            pending_scene_id = None;
            continue;
        }
        if indent == 0
            && !trimmed.starts_with('-')
            && trimmed.contains(':')
            && !trimmed.starts_with("assemble:")
        {
            // New top-level key ends scenes/captions blocks (except nested under assemble indent).
            let key = trimmed.split(':').next().unwrap_or("").trim();
            if !matches!(
                key,
                "production"
                    | "production_path"
                    | "output"
                    | "output_path"
                    | "trim"
                    | "discover"
                    | "prefer_retry"
                    | "overwrite"
                    | "music"
                    | "music_path"
                    | "music_volume"
                    | "volume"
                    | "duck_under_dialogue"
                    | "duck"
                    | "work_dir"
                    | "caption_mode"
                    | "caption_language"
                    | "language"
            ) && key.parse::<u32>().is_err()
            {
                // keep block state for indented assemble body
            } else if key.parse::<u32>().is_err() && key != "scenes" && key != "captions" {
                if indent <= 2 {
                    // stay in captions only while under captions keys
                }
            }
        }

        if in_captions {
            if let Some(rest) = trimmed.strip_prefix("mode:") {
                out.insert("caption_mode".into(), Value::String(unquote_yaml(rest)));
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("language:") {
                out.insert("caption_language".into(), Value::String(unquote_yaml(rest)));
                continue;
            }
            // Leaving captions when a non-caption key appears at same/outdent level
            if !trimmed.starts_with("mode:") && !trimmed.starts_with("language:") && indent <= 2 {
                in_captions = false;
            } else {
                continue;
            }
        }

        if in_scenes {
            // List form: - id: 1
            if let Some(rest) = trimmed.strip_prefix("- id:") {
                pending_scene_id = rest.trim().parse().ok();
                continue;
            }
            if trimmed.starts_with("- ") && trimmed.contains("id:") {
                if let Some(pos) = trimmed.find("id:") {
                    pending_scene_id = trimmed[pos + 3..].trim().parse().ok();
                }
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("video:") {
                if let Some(id) = pending_scene_id {
                    flush_scene_path(&mut scenes, Some(id), rest);
                    pending_scene_id = None;
                }
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("audio:") {
                if let Some(Value::Object(obj)) = scenes.last_mut() {
                    obj.insert("audio".into(), Value::String(unquote_yaml(rest)));
                }
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("trim:") {
                if let Some(Value::Object(obj)) = scenes.last_mut() {
                    obj.insert("trim".into(), Value::String(unquote_yaml(rest)));
                }
                continue;
            }
            // Map form: "1: /path/to.mp4" or "1:" then nested video
            if let Some((key, val)) = trimmed.split_once(':') {
                let key = key.trim();
                if let Ok(id) = key.parse::<u32>() {
                    let val = val.trim();
                    if val.is_empty() {
                        pending_scene_id = Some(id);
                    } else {
                        flush_scene_path(&mut scenes, Some(id), val);
                        pending_scene_id = None;
                    }
                    continue;
                }
            }
            // Non-scene key ends the scenes block
            if indent <= 2 && !trimmed.starts_with('-') && trimmed.contains(':') {
                let key = trimmed.split(':').next().unwrap_or("").trim();
                if key.parse::<u32>().is_err() && key != "id" && key != "video" && key != "audio" {
                    in_scenes = false;
                    pending_scene_id = None;
                } else {
                    continue;
                }
            } else {
                continue;
            }
        }

        if let Some((key, val)) = trimmed.split_once(':') {
            let key = key.trim();
            let val = unquote_yaml(val);
            if val.is_empty() {
                continue;
            }
            match key {
                "production" | "production_path" | "prod_path" | "prod" => {
                    out.insert("production_path".into(), Value::String(val));
                }
                "output" | "output_path" | "save_path" => {
                    out.insert("output_path".into(), Value::String(val));
                }
                "trim" => {
                    out.insert("trim".into(), Value::String(val));
                }
                "discover" => {
                    if let Some(b) = yaml_bool(&val) {
                        out.insert("discover".into(), Value::Bool(b));
                    }
                }
                "prefer_retry" => {
                    if let Some(b) = yaml_bool(&val) {
                        out.insert("prefer_retry".into(), Value::Bool(b));
                    }
                }
                "overwrite" => {
                    if let Some(b) = yaml_bool(&val) {
                        out.insert("overwrite".into(), Value::Bool(b));
                    }
                }
                "music" | "music_path" => {
                    out.insert("music_path".into(), Value::String(val));
                }
                "music_volume" | "volume" => {
                    if let Ok(n) = val.parse::<f64>() {
                        out.insert("music_volume".into(), Value::from(n));
                    }
                }
                "duck_under_dialogue" | "duck" => {
                    if let Some(b) = yaml_bool(&val) {
                        out.insert("duck_under_dialogue".into(), Value::Bool(b));
                    }
                }
                "work_dir" => {
                    out.insert("work_dir".into(), Value::String(val));
                }
                "caption_mode" | "captions" => {
                    out.insert("caption_mode".into(), Value::String(val));
                }
                "caption_language" | "language" => {
                    out.insert("caption_language".into(), Value::String(val));
                }
                _ => {}
            }
        }
    }

    if !scenes.is_empty() {
        out.insert("scenes".into(), Value::Array(scenes));
    }
    if out.is_empty() {
        return Err("assemble map file did not contain recognizable assemble fields".into());
    }
    Ok(out)
}

/// Load `assemble_map` / `final_prompt` YAML into inputs (does not overwrite explicit keys).
pub fn merge_assemble_map_into_inputs(
    inputs: &mut Map<String, Value>,
    project_root: Option<&str>,
) -> Result<(), String> {
    let map_raw = input_str(
        inputs,
        &[
            "assemble_map",
            "map_path",
            "assemble_path",
            "final_prompt",
            "assemble_prompt",
        ],
    );
    let Some(map_raw) = map_raw else {
        return Ok(());
    };
    let map_path = resolve_under_project(map_raw, project_root).map_err(|e| e.to_string())?;
    if !map_path.is_file() {
        return Err(format!("Assemble map not found: {}", map_path.display()));
    }
    let text =
        fs::read_to_string(&map_path).map_err(|e| format!("Could not read assemble map: {e}"))?;
    let parsed = parse_assemble_map_text(&text)?;
    for (k, v) in parsed {
        inputs.entry(k).or_insert(v);
    }
    // Keep the map path for reports
    inputs
        .entry("assemble_map".to_string())
        .or_insert(Value::String(map_path.to_string_lossy().to_string()));
    Ok(())
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
            if !v.is_empty() && v != "|" && v != "|-" {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn extract_yaml_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```yaml") || line.trim_start().starts_with("```yml") {
            let mut body = String::new();
            for inner in lines.by_ref() {
                if inner.trim().starts_with("```") {
                    break;
                }
                body.push_str(inner);
                body.push('\n');
            }
            blocks.push(body);
        }
    }
    blocks
}

fn parse_scene_id_from_heading(line: &str) -> Option<u32> {
    let t = line.trim();
    let rest = t.strip_prefix("### Scene ")?;
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse().ok()
}

fn parse_cues(body: &str) -> Vec<SpeechCue> {
    let Some(cues_at) = body.find("cues:") else {
        return Vec::new();
    };
    let after = &body[cues_at..];
    let mut cues = Vec::new();
    let mut current_text: Option<String> = None;
    let mut current_dur: Option<f64> = None;
    let flush = |cues: &mut Vec<SpeechCue>, text: &mut Option<String>, dur: &mut Option<f64>| {
        if let Some(t) = text.take() {
            cues.push(SpeechCue {
                text: t,
                estimated_duration: dur.take(),
            });
        }
    };
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
        if t.starts_with("- id:") || (t.starts_with("- ") && t.contains("id:")) {
            flush(&mut cues, &mut current_text, &mut current_dur);
        }
        if let Some(rest) = t.strip_prefix("text:") {
            current_text = Some(rest.trim().trim_matches('"').to_string());
        }
        if let Some(rest) = t.strip_prefix("estimated_duration_seconds:") {
            current_dur = rest.trim().trim_matches('"').parse().ok();
        }
    }
    flush(&mut cues, &mut current_text, &mut current_dur);
    cues
}

fn parse_prod_scene_meta(production_text: &str) -> Vec<SceneMeta> {
    let mut metas = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut chunk = String::new();
    let flush = |metas: &mut Vec<SceneMeta>, id: Option<u32>, chunk: &str| {
        let Some(id) = id else { return };
        let blocks = extract_yaml_blocks(chunk);
        let body = blocks.last().cloned().unwrap_or_default();
        if body.trim().is_empty() {
            return;
        }
        metas.push(SceneMeta {
            id,
            head_s: yaml_f64_field(&body, "head_handle_seconds").unwrap_or(0.0),
            tail_s: yaml_f64_field(&body, "tail_handle_seconds").unwrap_or(0.0),
            estimated_s: yaml_f64_field(&body, "estimated_screen_duration_seconds").unwrap_or(0.0),
            speech_mode: yaml_string_field(&body, "mode")
                .unwrap_or_else(|| "none".into())
                .trim_matches('"')
                .to_string(),
            transition_in: TransitionKind::parse(
                &yaml_string_field(&body, "transition_in").unwrap_or_else(|| "cut".into()),
            ),
            transition_out: TransitionKind::parse(
                &yaml_string_field(&body, "transition_out").unwrap_or_else(|| "cut".into()),
            ),
            cues: parse_cues(&body),
        });
    };

    for line in production_text.lines() {
        if let Some(id) = parse_scene_id_from_heading(line) {
            flush(&mut metas, current_id, &chunk);
            current_id = Some(id);
            chunk.clear();
            chunk.push_str(line);
            chunk.push('\n');
        } else if current_id.is_some() {
            chunk.push_str(line);
            chunk.push('\n');
        }
    }
    flush(&mut metas, current_id, &chunk);
    metas
}

fn has_audio_stream(path: &Path) -> bool {
    let Ok(output) = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
            &crate::ffmpeg_path::ffmpeg_local_arg(path).unwrap_or_default(),
        ])
        .output()
    else {
        return false;
    };
    output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
}

fn run_ffmpeg(args: &[String], label: &str) -> Result<(), String> {
    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|e| format!("Could not run ffmpeg for {label} (is it on PATH?): {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: String = stderr
            .chars()
            .rev()
            .take(1200)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        return Err(format!("ffmpeg failed ({label}): {tail}"));
    }
    Ok(())
}

fn discover_scene_video(
    project_root: &Path,
    scene_id: u32,
    prefer_retry: bool,
) -> Option<(PathBuf, Option<PathBuf>)> {
    let dir = project_root.join("Scenes").join(format!("Scene{scene_id}"));
    if !dir.is_dir() {
        // Also try zero-padded / lowercase variants
        for alt in [
            project_root.join("Scenes").join(format!("scene{scene_id}")),
            project_root.join("scenes").join(format!("Scene{scene_id}")),
        ] {
            if alt.is_dir() {
                return discover_in_dir(&alt, scene_id, prefer_retry);
            }
        }
        return None;
    }
    discover_in_dir(&dir, scene_id, prefer_retry)
}

fn discover_in_dir(
    dir: &Path,
    scene_id: u32,
    prefer_retry: bool,
) -> Option<(PathBuf, Option<PathBuf>)> {
    let read = fs::read_dir(dir).ok()?;
    let mut videos = Vec::new();
    let mut wavs = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let name = path.file_name()?.to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".mp4") || lower.ends_with(".webm") || lower.ends_with(".mov") {
            videos.push(path);
        } else if lower.ends_with(".wav") || lower.ends_with(".mp3") {
            wavs.push(path);
        }
    }
    if videos.is_empty() {
        return None;
    }
    let pick = if prefer_retry {
        videos
            .iter()
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase().contains("retry"))
            })
            .cloned()
            .or_else(|| {
                videos
                    .iter()
                    .find(|p| {
                        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                            n.to_ascii_lowercase().starts_with(&format!("sc{scene_id}"))
                        })
                    })
                    .cloned()
            })
            .unwrap_or_else(|| videos[0].clone())
    } else {
        videos
            .iter()
            .find(|p| {
                let n = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                !n.contains("retry")
                    && (n.starts_with(&format!("sc{scene_id}"))
                        || n.starts_with(&format!("scene{scene_id}")))
            })
            .cloned()
            .or_else(|| {
                videos
                    .iter()
                    .find(|p| {
                        !p.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase()
                            .contains("retry")
                    })
                    .cloned()
            })
            .unwrap_or_else(|| videos[0].clone())
    };
    let stem = pick.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let audio = wavs
        .iter()
        .find(|p| {
            p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| {
                s == stem
                    || s.starts_with(&format!("SC{scene_id}"))
                    || s.starts_with(&format!("sc{scene_id}"))
            })
        })
        .cloned()
        .or_else(|| wavs.first().cloned());
    Some((pick, audio))
}

fn parse_scene_assets(
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
    metas: &[SceneMeta],
) -> Result<Vec<SceneAsset>, String> {
    let prefer_retry = input_bool(inputs, &["prefer_retry"], false);
    let discover = input_bool(inputs, &["discover"], false);
    let mut assets = Vec::new();

    if let Some(Value::Array(items)) = inputs.get("scenes") {
        for item in items {
            let obj = item.as_object().ok_or_else(|| {
                "assemble scenes[] entries must be objects with id + video".to_string()
            })?;
            let id =
                obj.get("id")
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .ok_or_else(|| "assemble scene missing id".to_string())? as u32;
            let video_raw = obj
                .get("video")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("assemble scene {id} missing video path"))?;
            let video =
                resolve_under_project(video_raw, project_root).map_err(|e| e.to_string())?;
            if !video.is_file() {
                return Err(format!("Scene {id} video not found: {}", video.display()));
            }
            let audio = obj
                .get("audio")
                .and_then(|v| v.as_str())
                .map(|s| resolve_under_project(s, project_root).map_err(|e| e.to_string()))
                .transpose()?;
            if let Some(ref a) = audio {
                if !a.is_file() {
                    return Err(format!("Scene {id} audio not found: {}", a.display()));
                }
            }
            let trim_override = obj
                .get("trim")
                .and_then(|v| v.as_str())
                .map(TrimMode::parse);
            let custom_start = obj
                .get("start")
                .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|n| n as f64)));
            let custom_duration = obj
                .get("duration")
                .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|n| n as f64)));
            assets.push(SceneAsset {
                id,
                video,
                audio,
                trim_override,
                custom_start,
                custom_duration,
            });
        }
    }

    if assets.is_empty() {
        if !discover {
            return Err(
                "assemble needs scenes[] asset map, or discover:true with a conventional Scenes/ layout"
                    .into(),
            );
        }
        let root = project_root
            .map(PathBuf::from)
            .ok_or_else(|| "discover:true requires an open project root".to_string())?;
        let ids: Vec<u32> = if metas.is_empty() {
            // Scan Scenes/Scene*
            let scenes_dir = root.join("Scenes");
            let mut found = Vec::new();
            if let Ok(read) = fs::read_dir(&scenes_dir) {
                for entry in read.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(rest) = name
                        .strip_prefix("Scene")
                        .or_else(|| name.strip_prefix("scene"))
                    {
                        if let Ok(id) = rest.parse::<u32>() {
                            found.push(id);
                        }
                    }
                }
            }
            found.sort_unstable();
            found
        } else {
            metas.iter().map(|m| m.id).collect()
        };
        if ids.is_empty() {
            return Err("discover found no SceneN directories under Scenes/".into());
        }
        for id in ids {
            let Some((video, audio)) = discover_scene_video(&root, id, prefer_retry) else {
                return Err(format!(
                    "discover could not find video for Scene {id} under Scenes/Scene{id}/"
                ));
            };
            assets.push(SceneAsset {
                id,
                video,
                audio,
                trim_override: None,
                custom_start: None,
                custom_duration: None,
            });
        }
    }

    assets.sort_by_key(|a| a.id);
    Ok(assets)
}

fn compute_trim(
    mode: TrimMode,
    d: f64,
    h: f64,
    t: f64,
    s: f64,
    custom_start: Option<f64>,
    custom_duration: Option<f64>,
) -> Result<(f64, f64), String> {
    match mode {
        TrimMode::Full => Ok((0.0, d)),
        TrimMode::Custom => {
            let start = custom_start.unwrap_or(0.0).max(0.0);
            let dur = custom_duration.unwrap_or((d - start).max(0.0));
            if start >= d {
                return Err(format!("custom start {start} exceeds source duration {d}"));
            }
            Ok((start, dur.min(d - start)))
        }
        TrimMode::Estimated => {
            let start = h.max(0.0);
            if start >= d {
                return Err(format!("head handle {h} >= source duration {d}"));
            }
            let max_dur = d - start;
            let dur = if s > 0.0 { s.min(max_dur) } else { max_dur };
            Ok((start, dur))
        }
        TrimMode::Handles => {
            if h + t >= d && d > 0.0 {
                return Err(format!(
                    "handles H={h}+T={t} >= source duration D={d}; cannot trim"
                ));
            }
            let start = h.max(0.0);
            let dur = (d - h - t).max(0.0);
            Ok((start, dur))
        }
    }
}

fn transition_between(a: &SceneMeta, b: &SceneMeta) -> TransitionKind {
    match (a.transition_out, b.transition_in) {
        (TransitionKind::Dissolve, _) | (_, TransitionKind::Dissolve) => TransitionKind::Dissolve,
        (TransitionKind::Fade, _) | (_, TransitionKind::Fade) => TransitionKind::Fade,
        _ => TransitionKind::Cut,
    }
}

fn build_plan(
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
) -> Result<AssemblePlan, String> {
    let mut inputs = inputs.clone();
    merge_assemble_map_into_inputs(&mut inputs, project_root)?;
    let inputs = &inputs;

    let production_raw = input_str(
        inputs,
        &["production_path", "production", "prod_path", "prod"],
    )
    .ok_or_else(|| {
        "assemble needs production_path (prod.md) or an assemble_map that sets it".to_string()
    })?;
    let production_path =
        resolve_under_project(production_raw, project_root).map_err(|e| e.to_string())?;
    if !production_path.is_file() {
        return Err(format!(
            "Production notes not found: {}",
            production_path.display()
        ));
    }
    let production_text = fs::read_to_string(&production_path)
        .map_err(|e| format!("Could not read production notes: {e}"))?;
    let metas = parse_prod_scene_meta(&production_text);
    let meta_by_id: std::collections::HashMap<u32, &SceneMeta> =
        metas.iter().map(|m| (m.id, m)).collect();

    let output_raw =
        input_str(inputs, &["output_path", "output", "save_path"]).ok_or_else(|| {
            "assemble needs a master path — the host should have derived Final/<project>_final.mp4"
                .to_string()
        })?;
    let output_path = resolve_under_project(output_raw, project_root).map_err(|e| e.to_string())?;
    if output_path
        .extension()
        .and_then(|e| e.to_str())
        .is_none_or(|e| {
            let l = e.to_ascii_lowercase();
            !(l == "mp4" || l == "mov" || l == "webm" || l == "mkv")
        })
    {
        return Err("assemble output_path must be a video file (e.g. .mp4)".into());
    }

    let overwrite = input_bool(inputs, &["overwrite"], false);
    if output_path.is_file() && !overwrite {
        return Err(format!(
            "Master already exists at {} — pass overwrite:true after confirming, or choose a new path.",
            output_path.display()
        ));
    }

    let work_dir = if let Some(w) = input_str(inputs, &["work_dir"]) {
        resolve_under_project(w, project_root).map_err(|e| e.to_string())?
    } else {
        output_path.parent().unwrap_or(Path::new(".")).join("work")
    };

    let trim_explicit = input_str(inputs, &["trim"]).is_some();
    let mut trim_mode = TrimMode::parse(input_str(inputs, &["trim"]).unwrap_or("handles"));
    let caption_mode =
        CaptionMode::parse(input_str(inputs, &["caption_mode", "captions"]).unwrap_or("none"));
    let caption_language = input_str(inputs, &["caption_language", "language"])
        .unwrap_or("en")
        .to_string();

    let music_path = input_str(inputs, &["music_path", "music"])
        .map(|s| resolve_under_project(s, project_root).map_err(|e| e.to_string()))
        .transpose()?;
    if let Some(ref m) = music_path {
        if !m.is_file() {
            return Err(format!("Music file not found: {}", m.display()));
        }
    }
    let music_volume = input_f64(inputs, &["music_volume", "volume"], 0.18).clamp(0.0, 1.0);
    let duck_under_dialogue = input_bool(inputs, &["duck_under_dialogue", "duck"], true);

    let assets = parse_scene_assets(inputs, project_root, &metas)?;
    let mut warnings = Vec::new();

    // Auto-pick estimated when talking takes clearly overshoot S under handles.
    if !trim_explicit && matches!(trim_mode, TrimMode::Handles) && !assets.is_empty() {
        let mut talking = 0usize;
        let mut overshoot = 0usize;
        for asset in &assets {
            let meta = meta_by_id.get(&asset.id).copied();
            let speech = meta.map(|m| m.speech_mode.as_str()).unwrap_or("none");
            if is_talking_speech(speech) {
                talking += 1;
            }
            let h = meta.map(|m| m.head_s).unwrap_or(0.0);
            let t = meta.map(|m| m.tail_s).unwrap_or(0.0);
            let s = meta.map(|m| m.estimated_s).unwrap_or(0.0);
            if s <= 0.0 {
                continue;
            }
            let d = probe_duration_seconds(&asset.video.to_string_lossy())?;
            let handles_dur = (d - h - t).max(0.0);
            if handles_dur > s + 1.5 {
                overshoot += 1;
            }
        }
        let majority_talking = talking * 3 >= assets.len() * 2;
        if majority_talking && overshoot >= 2 {
            trim_mode = TrimMode::Estimated;
            warnings.push(
                "Auto trim:estimated — talking-shot takes overshoot estimated_screen_duration under handles (common with 5s/10s model buckets). Pass trim:\"handles\" to keep longer performance tails."
                    .into(),
            );
        }
    }

    let mut scenes = Vec::new();
    let mut sum_estimated = 0.0;

    for (idx, asset) in assets.iter().enumerate() {
        let meta = meta_by_id.get(&asset.id).copied();
        let head_s = meta.map(|m| m.head_s).unwrap_or(0.0);
        let tail_s = meta.map(|m| m.tail_s).unwrap_or(0.0);
        let estimated_s = meta.map(|m| m.estimated_s).unwrap_or(0.0);
        let speech_mode = meta
            .map(|m| m.speech_mode.clone())
            .unwrap_or_else(|| "none".into());
        let cues = meta.map(|m| m.cues.clone()).unwrap_or_default();
        let mode = asset.trim_override.unwrap_or(trim_mode);
        let d = probe_duration_seconds(&asset.video.to_string_lossy())?;
        let has_audio = has_audio_stream(&asset.video);
        let (trim_start, trim_duration) = match compute_trim(
            mode,
            d,
            head_s,
            tail_s,
            estimated_s,
            asset.custom_start,
            asset.custom_duration,
        ) {
            Ok(v) => v,
            Err(e) => {
                return Err(format!("Scene {}: {e}", asset.id));
            }
        };
        if estimated_s > 0.0 && trim_duration + 0.5 < estimated_s {
            warnings.push(format!(
                "Scene {}: trim duration {trim_duration:.2}s is shorter than estimated S={estimated_s:.1}s",
                asset.id
            ));
        }
        if speech_mode.contains("lipsync") || speech_mode.contains("audio_driven") {
            if !has_audio {
                if asset.audio.is_none() {
                    warnings.push(format!(
                        "Scene {}: talking/lipsync mode but video has no audio and no scene WAV — silent picture",
                        asset.id
                    ));
                } else {
                    warnings.push(format!(
                        "Scene {}: lipsync video has no audio stream — will lay scene WAV under picture",
                        asset.id
                    ));
                }
            }
        }
        let transition_to_next = if idx + 1 < assets.len() {
            let next_id = assets[idx + 1].id;
            match (meta, meta_by_id.get(&next_id).copied()) {
                (Some(a), Some(b)) => transition_between(a, b),
                _ => TransitionKind::Cut,
            }
        } else {
            TransitionKind::Cut
        };
        sum_estimated += if estimated_s > 0.0 {
            estimated_s
        } else {
            trim_duration
        };
        scenes.push(ScenePlan {
            id: asset.id,
            video: asset.video.clone(),
            audio: asset.audio.clone(),
            speech_mode,
            source_duration: d,
            head_s,
            tail_s,
            estimated_s,
            trim_start,
            trim_duration,
            transition_to_next,
            has_audio_stream: has_audio,
            cues,
        });
    }

    if scenes.is_empty() {
        return Err("assemble plan has zero scenes".into());
    }

    let fade_in_open = assets
        .first()
        .and_then(|a| meta_by_id.get(&a.id).copied())
        .is_some_and(|m| matches!(m.transition_in, TransitionKind::Fade));
    let fade_out_close = assets
        .last()
        .and_then(|a| meta_by_id.get(&a.id).copied())
        .is_some_and(|m| matches!(m.transition_out, TransitionKind::Fade));

    let planned_runtime: f64 = scenes.iter().map(|s| s.trim_duration).sum();
    if matches!(trim_mode, TrimMode::Handles)
        && sum_estimated > 0.0
        && (planned_runtime - sum_estimated).abs() > 2.0
    {
        warnings.push(format!(
            "Handle-trim runtime ~{planned_runtime:.1}s vs sum of estimates ~{sum_estimated:.1}s — pass trim:\"estimated\" for the tighter cut to S."
        ));
    }

    Ok(AssemblePlan {
        production_path,
        output_path,
        work_dir,
        trim_mode,
        caption_mode,
        caption_language,
        music_path,
        music_volume,
        duck_under_dialogue,
        overwrite,
        fade_in_open,
        fade_out_close,
        scenes,
        sum_estimated,
        warnings,
    })
}

fn format_timecode(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

fn write_srt(plan: &AssemblePlan, srt_path: &Path) -> Result<(), String> {
    let mut cursor = 0.0f64;
    let mut idx = 1u32;
    let mut body = String::new();
    for scene in &plan.scenes {
        let scene_start = cursor;
        let scene_end = cursor + scene.trim_duration;
        let cue_count = scene.cues.len().max(1);
        if scene.cues.is_empty() {
            cursor = scene_end;
            continue;
        }
        let mut local = 0.0f64;
        for cue in &scene.cues {
            let dur = cue
                .estimated_duration
                .unwrap_or(scene.trim_duration / cue_count as f64)
                .max(0.4);
            let start = (scene_start + local).min(scene_end - 0.05);
            let end = (start + dur).min(scene_end);
            body.push_str(&format!(
                "{idx}\n{} --> {}\n{}\n\n",
                format_timecode(start),
                format_timecode(end),
                cue.text
            ));
            idx += 1;
            local += dur;
        }
        cursor = scene_end;
        // Approximate transition overlap shrinkage for dissolve/fade
        if matches!(
            scene.transition_to_next,
            TransitionKind::Dissolve | TransitionKind::Fade
        ) {
            cursor = (cursor - 0.35).max(0.0);
        }
    }
    if let Some(parent) = srt_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create srt dir: {e}"))?;
    }
    fs::write(srt_path, body).map_err(|e| format!("Could not write srt: {e}"))?;
    Ok(())
}

fn normalize_vf() -> &'static str {
    // Keep talking-shot masters on a common 9:16 canvas without letterbox surprises.
    "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=24"
}

fn trim_scene(app: &HostCtx, scene: &ScenePlan, out: &Path) -> Result<(), String> {
    app.emit(
        "director-progress",
        format!(
            "Assemble: trim Scene {} ({:.2}s from {:.2}s)…",
            scene.id, scene.trim_duration, scene.trim_start
        ),
    );
    let speech = scene.speech_mode.to_ascii_lowercase();
    let needs_vo = (speech.contains("voiceover")
        || (!scene.has_audio_stream && scene.audio.is_some()))
        && scene.audio.is_some();
    let vf = normalize_vf();

    if needs_vo {
        let audio = scene.audio.as_ref().unwrap();
        // Trim picture + lay WAV aligned to trimmed start (WAV assumed unpadded / same handles).
        let args = vec![
            "-y".into(),
            "-ss".into(),
            format!("{:.6}", scene.trim_start),
            "-i".into(),
            crate::ffmpeg_path::ffmpeg_local_arg(&scene.video)?,
            "-ss".into(),
            format!("{:.6}", scene.trim_start),
            "-i".into(),
            crate::ffmpeg_path::ffmpeg_local_arg(audio)?,
            "-t".into(),
            format!("{:.6}", scene.trim_duration),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "1:a:0".into(),
            "-vf".into(),
            vf.into(),
            "-c:v".into(),
            "libx264".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-c:a".into(),
            "aac".into(),
            "-ar".into(),
            "48000".into(),
            "-shortest".into(),
            crate::ffmpeg_path::ffmpeg_local_arg(out)?,
        ];
        run_ffmpeg(&args, &format!("trim+vo scene {}", scene.id))?;
    } else {
        // All inputs must be declared before any output option: the silent
        // conformer's `-f lavfi -i anullsrc` previously trailed `-vf`, which
        // ffmpeg rejects as an input option applied after output filters.
        let mut args = vec![
            "-y".into(),
            "-ss".into(),
            format!("{:.6}", scene.trim_start),
            "-i".into(),
            crate::ffmpeg_path::ffmpeg_local_arg(&scene.video)?,
        ];
        if !scene.has_audio_stream {
            args.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "anullsrc=channel_layout=stereo:sample_rate=48000".into(),
            ]);
        }
        args.extend([
            "-t".into(),
            format!("{:.6}", scene.trim_duration),
            "-vf".into(),
            vf.into(),
            "-c:v".into(),
            "libx264".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ]);
        if scene.has_audio_stream {
            args.extend(["-c:a".into(), "aac".into(), "-ar".into(), "48000".into()]);
        } else {
            args.extend(["-shortest".into(), "-c:a".into(), "aac".into()]);
        }
        args.push(crate::ffmpeg_path::ffmpeg_local_arg(out)?);
        run_ffmpeg(&args, &format!("trim scene {}", scene.id))?;
    }
    if !out.is_file() {
        return Err(format!("Trim output missing: {}", out.display()));
    }
    Ok(())
}

fn apply_edge_fade(
    app: &HostCtx,
    clip: &Path,
    fade_in: bool,
    fade_out: bool,
    out: &Path,
) -> Result<(), String> {
    if !fade_in && !fade_out {
        fs::copy(clip, out).map_err(|e| format!("Could not copy clip for edge fade: {e}"))?;
        return Ok(());
    }
    let dur = probe_duration_seconds(&clip.to_string_lossy())?;
    let fade_d = 0.4f64;
    let mut vf = Vec::new();
    let mut af = Vec::new();
    if fade_in {
        vf.push(format!("fade=t=in:st=0:d={fade_d}"));
        af.push(format!("afade=t=in:st=0:d={fade_d}"));
    }
    if fade_out {
        let start = (dur - fade_d).max(0.0);
        vf.push(format!("fade=t=out:st={start}:d={fade_d}"));
        af.push(format!("afade=t=out:st={start}:d={fade_d}"));
    }
    app.emit(
        "director-progress",
        format!(
            "Assemble: edge fade ({})…",
            if fade_in && fade_out {
                "in+out"
            } else if fade_in {
                "in"
            } else {
                "out"
            }
        ),
    );
    let args = vec![
        "-y".into(),
        "-i".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(clip)?,
        "-vf".into(),
        vf.join(","),
        "-af".into(),
        af.join(","),
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-ar".into(),
        "48000".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(out)?,
    ];
    run_ffmpeg(&args, "edge fade")?;
    Ok(())
}

fn concat_cuts(app: &HostCtx, clips: &[PathBuf], out: &Path) -> Result<(), String> {
    app.emit(
        "director-progress",
        format!("Assemble: concat {} clips (hard cuts)…", clips.len()),
    );
    let list_path = out.parent().unwrap_or(Path::new(".")).join("concat.txt");
    let mut list = String::new();
    for clip in clips {
        list.push_str(&crate::ffmpeg_path::concat_list_line(clip)?);
    }
    fs::write(&list_path, list).map_err(|e| format!("Could not write concat list: {e}"))?;
    let args = vec![
        "-y".into(),
        "-f".into(),
        "concat".into(),
        // Absolute paths in a host-written list need -safe 0. Lines are
        // canonical paths with newlines and quotes already rejected.
        "-safe".into(),
        "0".into(),
        "-i".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(&list_path)?,
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-ar".into(),
        "48000".into(),
        "-movflags".into(),
        "+faststart".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(out)?,
    ];
    run_ffmpeg(&args, "concat")?;
    Ok(())
}

fn concat_with_xfades(
    app: &HostCtx,
    clips: &[PathBuf],
    transitions: &[TransitionKind],
    durations: &[f64],
    out: &Path,
) -> Result<(), String> {
    if clips.len() < 2 {
        return concat_cuts(app, clips, out);
    }
    app.emit(
        "director-progress",
        "Assemble: building timeline with fades/dissolves…",
    );
    let fade_dur = 0.35f64;
    let mut args = vec!["-y".into()];
    for clip in clips {
        args.push("-i".into());
        args.push(crate::ffmpeg_path::ffmpeg_local_arg(clip)?);
    }
    // Build video xfade + acrossfade chain
    let mut filter = String::new();
    let mut v_label = "0:v".to_string();
    let mut a_label = "0:a".to_string();
    let mut offset = durations[0];
    for i in 1..clips.len() {
        let kind = transitions
            .get(i - 1)
            .copied()
            .unwrap_or(TransitionKind::Cut);
        let use_fade = !matches!(kind, TransitionKind::Cut);
        let d = if use_fade { fade_dur } else { 0.01 };
        let next_v = format!("v{i}");
        let next_a = format!("a{i}");
        let xfade_offset = (offset - d).max(0.0);
        let transition = match kind {
            TransitionKind::Dissolve => "fade",
            TransitionKind::Fade => "fadeblack",
            TransitionKind::Cut => "fade",
        };
        if i == 1 {
            filter.push_str(&format!(
                "[{v_label}][{i}:v]xfade=transition={transition}:duration={d}:offset={xfade_offset}[{next_v}];\
[{a_label}][{i}:a]acrossfade=d={d}[{next_a}]"
            ));
        } else {
            filter.push_str(&format!(
                ";[{v_label}][{i}:v]xfade=transition={transition}:duration={d}:offset={xfade_offset}[{next_v}];\
[{a_label}][{i}:a]acrossfade=d={d}[{next_a}]"
            ));
        }
        v_label = next_v;
        a_label = next_a;
        offset = offset + durations[i] - d;
    }
    args.extend([
        "-filter_complex".into(),
        filter,
        "-map".into(),
        format!("[{v_label}]"),
        "-map".into(),
        format!("[{a_label}]"),
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-c:a".into(),
        "aac".into(),
        "-ar".into(),
        "48000".into(),
        "-movflags".into(),
        "+faststart".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(out)?,
    ]);
    run_ffmpeg(&args, "xfade timeline")?;
    Ok(())
}

fn mix_music(
    app: &HostCtx,
    dialogue: &Path,
    music: &Path,
    volume: f64,
    duck: bool,
    out: &Path,
) -> Result<(), String> {
    app.emit(
        "director-progress",
        format!("Assemble: mixing music bed (volume={volume:.2}, duck={duck})…"),
    );
    let duration = probe_duration_seconds(&dialogue.to_string_lossy())?;
    let music_vol = if duck { volume * 0.55 } else { volume };
    let filter = format!(
        "[1:a]volume={music_vol},atrim=0:{duration:.6},asetpts=PTS-STARTPTS[a1];\
[0:a][a1]amix=inputs=2:duration=first:dropout_transition=0[aout]"
    );
    let args = vec![
        "-y".into(),
        "-i".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(dialogue)?,
        "-stream_loop".into(),
        "-1".into(),
        "-i".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(music)?,
        "-filter_complex".into(),
        filter,
        "-map".into(),
        "0:v".into(),
        "-map".into(),
        "[aout]".into(),
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "aac".into(),
        "-ar".into(),
        "48000".into(),
        "-shortest".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(out)?,
    ];
    run_ffmpeg(&args, "music mix")?;
    Ok(())
}

fn mux_soft_subs(
    app: &HostCtx,
    video: &Path,
    srt: &Path,
    language: &str,
    out: &Path,
) -> Result<(), String> {
    app.emit("director-progress", "Assemble: muxing soft subtitles…");
    let args = vec![
        "-y".into(),
        "-i".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(video)?,
        "-i".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(srt)?,
        "-c:v".into(),
        "copy".into(),
        "-c:a".into(),
        "copy".into(),
        "-c:s".into(),
        "mov_text".into(),
        "-metadata:s:s:0".into(),
        format!("language={language}"),
        crate::ffmpeg_path::ffmpeg_local_arg(out)?,
    ];
    run_ffmpeg(&args, "soft subs")?;
    Ok(())
}

fn burn_subs(app: &HostCtx, video: &Path, srt: &Path, out: &Path) -> Result<(), String> {
    app.emit("director-progress", "Assemble: burning captions…");
    // Escape path for subtitles filter
    let srt_esc = srt
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'");
    let vf =
        format!("subtitles='{srt_esc}':force_style='Alignment=2,FontSize=16,Outline=2,Shadow=0'");
    let args = vec![
        "-y".into(),
        "-i".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(video)?,
        "-vf".into(),
        vf,
        "-c:a".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        crate::ffmpeg_path::ffmpeg_local_arg(out)?,
    ];
    run_ffmpeg(&args, "burned captions")?;
    Ok(())
}

fn execute_plan(app: &HostCtx, plan: &AssemblePlan) -> Result<String, String> {
    fs::create_dir_all(&plan.work_dir).map_err(|e| format!("Could not create work dir: {e}"))?;
    if let Some(parent) = plan.output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Could not create output dir: {e}"))?;
    }

    let mut trimmed = Vec::new();
    let mut transitions = Vec::new();
    let mut durations = Vec::new();
    let last_idx = plan.scenes.len().saturating_sub(1);
    for (idx, scene) in plan.scenes.iter().enumerate() {
        let raw = plan
            .work_dir
            .join(format!("scene_{:02}_trim.mp4", scene.id));
        trim_scene(app, scene, &raw)?;
        let fade_in = idx == 0 && plan.fade_in_open;
        let fade_out = idx == last_idx && plan.fade_out_close;
        let out = if fade_in || fade_out {
            let faded = plan
                .work_dir
                .join(format!("scene_{:02}_edge.mp4", scene.id));
            apply_edge_fade(app, &raw, fade_in, fade_out, &faded)?;
            faded
        } else {
            raw
        };
        trimmed.push(out);
        transitions.push(scene.transition_to_next);
        durations.push(scene.trim_duration);
    }

    let needs_xfade = transitions
        .iter()
        .any(|t| matches!(t, TransitionKind::Fade | TransitionKind::Dissolve));
    let dialogue_path = plan.work_dir.join("dialogue_timeline.mp4");
    if needs_xfade {
        concat_with_xfades(app, &trimmed, &transitions, &durations, &dialogue_path)?;
    } else {
        concat_cuts(app, &trimmed, &dialogue_path)?;
    }

    let mut current = dialogue_path;
    if let Some(ref music) = plan.music_path {
        let with_music = plan.work_dir.join("with_music.mp4");
        mix_music(
            app,
            &current,
            music,
            plan.music_volume,
            plan.duck_under_dialogue,
            &with_music,
        )?;
        current = with_music;
    }

    let srt_path = plan.output_path.with_extension("srt");
    if !matches!(plan.caption_mode, CaptionMode::None) {
        write_srt(plan, &srt_path)?;
    }

    match plan.caption_mode {
        CaptionMode::None => {
            fs::copy(&current, &plan.output_path)
                .map_err(|e| format!("Could not write master: {e}"))?;
        }
        CaptionMode::Soft => {
            mux_soft_subs(
                app,
                &current,
                &srt_path,
                &plan.caption_language,
                &plan.output_path,
            )?;
        }
        CaptionMode::Burned => {
            burn_subs(app, &current, &srt_path, &plan.output_path)?;
        }
        CaptionMode::Both => {
            let soft_out = plan.output_path.with_extension("soft.mp4");
            mux_soft_subs(app, &current, &srt_path, &plan.caption_language, &soft_out)?;
            burn_subs(app, &current, &srt_path, &plan.output_path)?;
        }
    }

    let master_dur = probe_duration_seconds(&plan.output_path.to_string_lossy())?;
    let (w, h) = probe_image_size(&plan.output_path.to_string_lossy()).unwrap_or((0, 0));
    let bytes = fs::metadata(&plan.output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let mut report = String::from("Assemble finished (host-run).\n");
    report.push_str(&format!(
        "- master: `{}` ({bytes} bytes, {master_dur:.2}s",
        plan.output_path.display()
    ));
    if w > 0 {
        report.push_str(&format!(", {w}×{h}"));
    }
    report.push_str(")\n");
    report.push_str(&format!(
        "- production: `{}`\n",
        plan.production_path.display()
    ));
    report.push_str(&format!("- work dir: `{}`\n", plan.work_dir.display()));
    report.push_str(&format!("- trim mode: `{}`\n", plan.trim_mode.as_str()));
    if plan.overwrite {
        report.push_str("- overwrite: approved\n");
    }
    report.push_str("- scenes:\n");
    for scene in &plan.scenes {
        report.push_str(&format!(
            "  - Scene {}: `{}` D={:.2}s H={:.1}/T={:.1}/S={:.1} → start={:.2} dur={:.2} ({}) → {}\n",
            scene.id,
            scene.video.display(),
            scene.source_duration,
            scene.head_s,
            scene.tail_s,
            scene.estimated_s,
            scene.trim_start,
            scene.trim_duration,
            scene.speech_mode,
            scene.transition_to_next.as_str()
        ));
    }
    if let Some(ref music) = plan.music_path {
        report.push_str(&format!(
            "- music: `{}` volume={:.2} duck={}\n",
            music.display(),
            plan.music_volume,
            plan.duck_under_dialogue
        ));
    } else {
        report.push_str("- music: none\n");
    }
    report.push_str(&format!("- captions: `{}`", plan.caption_mode.as_str()));
    if !matches!(plan.caption_mode, CaptionMode::None) {
        report.push_str(&format!(" (`{}`)", srt_path.display()));
    }
    report.push('\n');
    if !plan.warnings.is_empty() {
        report.push_str("- warnings:\n");
        for w in &plan.warnings {
            report.push_str(&format!("  - {w}\n"));
        }
    }
    report.push_str(
        "\nEscape hatch: open the work dir / master in Kdenlive for taste-heavy finishing.",
    );
    Ok(report)
}

pub fn approval_notes(inputs: &Map<String, Value>, project_root: Option<&str>) -> String {
    match build_plan(inputs, project_root) {
        Ok(plan) => {
            let planned: f64 = plan.scenes.iter().map(|s| s.trim_duration).sum();
            let mut body = String::from("### Assemble plan (host-validated)\n");
            body.push_str(&format!(
                "- production: `{}`\n",
                plan.production_path.display()
            ));
            body.push_str(&format!("- trim mode: `{}`\n", plan.trim_mode.as_str()));
            body.push_str(&format!("- planned runtime (post-trim): **{planned:.1}s**"));
            if plan.sum_estimated > 0.0 {
                body.push_str(&format!(" · sum of estimates: ~{:.1}s", plan.sum_estimated));
            }
            body.push('\n');
            body.push_str(&format!("- output: `{}`\n", plan.output_path.display()));
            body.push_str(&format!("- work dir: `{}`\n", plan.work_dir.display()));
            if plan.fade_in_open || plan.fade_out_close {
                body.push_str(&format!(
                    "- edge fades: in={} out={}\n",
                    plan.fade_in_open, plan.fade_out_close
                ));
            }
            if let Some(ref music) = plan.music_path {
                body.push_str(&format!(
                    "- music: `{}` @ {:.2} (duck={})\n",
                    music.display(),
                    plan.music_volume,
                    plan.duck_under_dialogue
                ));
            } else {
                body.push_str("- music: none\n");
            }
            body.push_str(&format!("- captions: `{}`\n", plan.caption_mode.as_str()));
            body.push_str("- scenes:\n");
            for scene in &plan.scenes {
                body.push_str(&format!(
                    "  - **Scene {}** `{}`\n    D={:.2}s · H={:.1} T={:.1} S={:.1} · trim start={:.2} dur={:.2} · {} · → {}\n",
                    scene.id,
                    scene.video.display(),
                    scene.source_duration,
                    scene.head_s,
                    scene.tail_s,
                    scene.estimated_s,
                    scene.trim_start,
                    scene.trim_duration,
                    scene.speech_mode,
                    scene.transition_to_next.as_str()
                ));
                if let Some(ref a) = scene.audio {
                    body.push_str(&format!("    audio: `{}`\n", a.display()));
                }
            }
            if !plan.warnings.is_empty() {
                body.push_str("- warnings:\n");
                for w in &plan.warnings {
                    body.push_str(&format!("  - {w}\n"));
                }
            }
            body.push_str(
                "\nLocal ffmpeg only (no provider spend). Use the Approve button to render this plan.",
            );
            body
        }
        Err(error) => format!(
            "### Assemble plan (validation failed)\n{error}\n\nFix the inputs and try again."
        ),
    }
}

pub fn run(
    app: &HostCtx,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<String, String> {
    let inputs = &validated.action.inputs;
    let plan = build_plan(inputs, project_root)?;
    app.emit(
        "director-progress",
        format!(
            "Assemble: rendering {} scenes → {}…",
            plan.scenes.len(),
            plan.output_path.display()
        ),
    );
    execute_plan(app, &plan)
}

/// Full host validation: paths, probes, trim windows. Used at action-validate time.
pub fn validate_plan(
    inputs: &Map<String, Value>,
    project_root: Option<&str>,
) -> Result<(), String> {
    build_plan(inputs, project_root).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_list_style_assemble_map() {
        let text = r#"
assemble:
  production: "/tmp/prod.md"
  output: "/tmp/Final/out.mp4"
  trim: estimated
  captions:
    mode: none
    language: en
  scenes:
    - id: 1
      video: "/tmp/Scenes/Scene1/SC1.mp4"
    - id: 2
      video: "/tmp/Scenes/Scene2/SC2.mp4"
"#;
        let map = parse_assemble_map_text(text).expect("parse");
        assert_eq!(
            map.get("production_path").and_then(|v| v.as_str()),
            Some("/tmp/prod.md")
        );
        assert_eq!(
            map.get("output_path").and_then(|v| v.as_str()),
            Some("/tmp/Final/out.mp4")
        );
        assert_eq!(map.get("trim").and_then(|v| v.as_str()), Some("estimated"));
        assert_eq!(
            map.get("caption_mode").and_then(|v| v.as_str()),
            Some("none")
        );
        let scenes = map.get("scenes").and_then(|v| v.as_array()).unwrap();
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].get("id").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            scenes[1].get("video").and_then(|v| v.as_str()),
            Some("/tmp/Scenes/Scene2/SC2.mp4")
        );
    }

    #[test]
    fn parses_map_style_scene_entries() {
        let text = r#"
assemble:
  production: /tmp/prod.md
  output: /tmp/out.mp4
  scenes:
    1: /tmp/SC1.mp4
    4: /tmp/SC4_retry.mp4
"#;
        let map = parse_assemble_map_text(text).expect("parse");
        let scenes = map.get("scenes").and_then(|v| v.as_array()).unwrap();
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].get("id").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            scenes[1].get("video").and_then(|v| v.as_str()),
            Some("/tmp/SC4_retry.mp4")
        );
    }

    #[test]
    fn merge_does_not_override_explicit_inputs() {
        let dir = std::env::temp_dir().join(format!("wb_assemble_map_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let map_path = dir.join("final_prompt.txt");
        fs::write(
            &map_path,
            r#"
assemble:
  production: /tmp/from_map_prod.md
  output: /tmp/from_map_out.mp4
  trim: estimated
  scenes:
    - id: 1
      video: /tmp/SC1.mp4
"#,
        )
        .unwrap();
        let mut inputs = Map::new();
        inputs.insert(
            "assemble_map".into(),
            json!(map_path.to_string_lossy().to_string()),
        );
        inputs.insert("trim".into(), json!("full"));
        merge_assemble_map_into_inputs(&mut inputs, Some(dir.to_str().unwrap())).expect("merge");
        assert_eq!(inputs.get("trim").and_then(|v| v.as_str()), Some("full"));
        assert_eq!(
            inputs.get("production_path").and_then(|v| v.as_str()),
            Some("/tmp/from_map_prod.md")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

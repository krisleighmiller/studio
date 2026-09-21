//! Shared helpers for generative media adapters.

use crate::atlas;
use crate::ctx::HostCtx;
use base64::Engine;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub fn emit_progress(app: Option<&HostCtx>, message: impl Into<String>) {
    if let Some(app) = app {
        app.emit("director-progress", message.into());
    }
}

pub fn param_str(params: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = params.get(*key).and_then(|v| v.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn param_u64(params: &Value, keys: &[&str], default: u64) -> u64 {
    for key in keys {
        if let Some(n) = params.get(*key).and_then(|v| v.as_u64()) {
            return n;
        }
        if let Some(s) = params.get(*key).and_then(|v| v.as_str()) {
            if let Ok(n) = s.trim().parse::<u64>() {
                return n;
            }
        }
    }
    default
}

pub fn resolve_path(raw: &str, project_root: Option<&str>) -> PathBuf {
    let path = PathBuf::from(raw.trim());
    if path.is_absolute() {
        path
    } else if let Some(root) = project_root {
        PathBuf::from(root).join(path)
    } else {
        path
    }
}

pub fn file_to_data_uri(raw: &str, project_root: Option<&str>) -> Result<String, String> {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("data:") {
        return Ok(raw.to_string());
    }
    let path = resolve_path(raw, project_root);
    if !path.is_file() {
        return Err(format!("Media path not found: {}", path.display()));
    }
    let bytes = fs::read(&path).map_err(|error| format!("Could not read file: {error}"))?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        _ => "image/png",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn write_bytes(dest: &str, project_root: Option<&str>, data: &[u8]) -> Result<String, String> {
    let path = resolve_path(dest, project_root);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent dirs: {error}"))?;
        }
    }
    fs::write(&path, data)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(serde_json::json!({
        "saved": true,
        "path": path.to_string_lossy(),
        "bytes": data.len(),
    })
    .to_string())
}

pub fn write_b64(dest: &str, project_root: Option<&str>, b64: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|error| format!("Could not decode base64: {error}"))?;
    write_bytes(dest, project_root, &bytes)
}

pub fn download_url(
    url: &str,
    dest: &str,
    project_root: Option<&str>,
    app: Option<&HostCtx>,
) -> Result<String, String> {
    atlas::download_atlas_media(url, dest, project_root, app)
}

pub fn http_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .user_agent("VideoWorkbench/0.1 (desktop; media-adapters)")
        .build()
        .map_err(|error| format!("Could not build HTTP client: {error}"))
}

pub fn first_url_in_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s)
            if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("data:") =>
        {
            Some(s.clone())
        }
        Value::Array(items) => items.iter().find_map(first_url_in_value),
        Value::Object(map) => {
            for key in ["url", "uri", "video", "image", "audio", "output", "file"] {
                if let Some(found) = map.get(key).and_then(first_url_in_value) {
                    return Some(found);
                }
            }
            for (_k, v) in map {
                if let Some(found) = first_url_in_value(v) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

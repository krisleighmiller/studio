//! Shared helpers for generative media adapters.

use crate::atlas;
use crate::ctx::HostCtx;
use base64::Engine;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
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

pub fn resolve_path(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    crate::project_path::resolve(raw, project_root, crate::project_path::PathIntent::Read)
        .map(|path| path.into_path_buf())
        .map_err(|error| error.to_string())
}

pub fn resolve_write_path(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    crate::project_path::resolve(raw, project_root, crate::project_path::PathIntent::Write)
        .map(|path| path.into_path_buf())
        .map_err(|error| error.to_string())
}

const MAX_MEDIA_BYTES: u64 = 80_000_000;

pub fn load_media(path: &Path) -> Result<(String, Vec<u8>), String> {
    if !path.is_file() {
        return Err(format!("Media path not found: {}", path.display()));
    }
    let len = fs::metadata(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?
        .len();
    if len > MAX_MEDIA_BYTES {
        return Err(format!(
            "{} is larger than {MAX_MEDIA_BYTES} bytes",
            path.display()
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let mime = sniff_media(path, &bytes)?;
    Ok((mime, bytes))
}

pub fn ensure_media_file(path: &Path) -> Result<String, String> {
    Ok(load_media(path)?.0)
}

fn sniff_media(path: &Path, bytes: &[u8]) -> Result<String, String> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        _ => {
            return Err(format!(
                "refusing to upload `{}`: extension is not an allowed image, audio, or video type",
                path.display()
            ))
        }
    };
    if !magic_matches(&ext, bytes) {
        return Err(format!(
            "refusing to upload `{}`: file contents do not match .{ext}",
            path.display()
        ));
    }
    Ok(mime.to_string())
}

fn magic_matches(ext: &str, bytes: &[u8]) -> bool {
    match ext {
        "png" => bytes.starts_with(b"\x89PNG"),
        "jpg" | "jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
        "wav" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE",
        "mp3" => bytes.starts_with(b"ID3")
            || (bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0),
        "mp4" | "mov" => bytes.len() >= 8 && &bytes[4..8] == b"ftyp",
        "webm" => bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]),
        _ => false,
    }
}

pub fn file_to_data_uri(raw: &str, project_root: Option<&str>) -> Result<String, String> {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("data:") {
        return Ok(raw.to_string());
    }
    let path = resolve_path(raw, project_root)?;
    let (mime, bytes) = load_media(&path)?;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn write_bytes(dest: &str, project_root: Option<&str>, data: &[u8]) -> Result<String, String> {
    let path = resolve_write_path(dest, project_root)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_extension_that_does_not_match_the_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.png");
        std::fs::write(&path, b"this is text").unwrap();
        let error = load_media(&path).unwrap_err();
        assert!(error.contains("do not match"), "{error}");
    }

    #[test]
    fn accepts_a_png_signature() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frame.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();
        let (mime, _) = load_media(&path).unwrap();
        assert_eq!(mime, "image/png");
    }
}

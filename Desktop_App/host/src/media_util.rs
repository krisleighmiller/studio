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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRole {
    Image,
    Audio,
    Video,
    AnyMedia,
}

impl MediaRole {
    fn label(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::AnyMedia => "media",
        }
    }

    fn max_bytes(self) -> u64 {
        match self {
            Self::Image => 25_000_000,
            Self::Audio | Self::Video | Self::AnyMedia => 80_000_000,
        }
    }

    fn accepts(self, mime: &str) -> bool {
        match self {
            Self::Image => mime.starts_with("image/"),
            Self::Audio => mime.starts_with("audio/"),
            Self::Video => mime.starts_with("video/"),
            Self::AnyMedia => {
                mime.starts_with("image/")
                    || mime.starts_with("audio/")
                    || mime.starts_with("video/")
            }
        }
    }
}

pub fn resolve_path(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    #[cfg(not(test))]
    {
        let _ = project_root;
        return crate::session::authorize(raw, crate::project_path::PathIntent::Read);
    }
    #[cfg(test)]
    {
        crate::project_path::resolve(raw, project_root, crate::project_path::PathIntent::Read)
            .map(|path| path.into_path_buf())
            .map_err(|error| error.to_string())
    }
}

pub fn peek_path(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    #[cfg(not(test))]
    {
        let _ = project_root;
        return crate::session::peek_authorize(raw, crate::project_path::PathIntent::Read);
    }
    #[cfg(test)]
    {
        crate::project_path::resolve(raw, project_root, crate::project_path::PathIntent::Read)
            .map(|path| path.into_path_buf())
            .map_err(|error| error.to_string())
    }
}

pub fn resolve_write_path(raw: &str, project_root: Option<&str>) -> Result<PathBuf, String> {
    #[cfg(not(test))]
    {
        let _ = project_root;
        return crate::session::authorize(raw, crate::project_path::PathIntent::Write);
    }
    #[cfg(test)]
    {
        crate::project_path::resolve(raw, project_root, crate::project_path::PathIntent::Write)
            .map(|path| path.into_path_buf())
            .map_err(|error| error.to_string())
    }
}

const MAX_MEDIA_BYTES: u64 = 80_000_000;

pub fn load_media(path: &Path) -> Result<(String, Vec<u8>), String> {
    load_media_as(path, MediaRole::AnyMedia)
}

pub fn load_media_as(path: &Path, role: MediaRole) -> Result<(String, Vec<u8>), String> {
    if !path.is_file() {
        return Err(format!("Media path not found: {}", path.display()));
    }
    let len = fs::metadata(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?
        .len();
    let limit = role.max_bytes().min(MAX_MEDIA_BYTES);
    if len > limit {
        return Err(format!(
            "{} is larger than the {} upload limit ({limit} bytes)",
            path.display(),
            role.label()
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let mime = sniff_media(path, &bytes)?;
    if !role.accepts(&mime) {
        return Err(format!(
            "refusing to upload `{}`: detected `{mime}` is not allowed for {} inputs",
            path.display(),
            role.label()
        ));
    }
    Ok((mime, bytes))
}

pub fn ensure_media_file(path: &Path) -> Result<String, String> {
    ensure_media_file_as(path, MediaRole::AnyMedia)
}

pub fn ensure_media_file_as(path: &Path, role: MediaRole) -> Result<String, String> {
    Ok(load_media_as(path, role)?.0)
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
        "bmp" => "image/bmp",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" | "mkv" => "video/webm",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
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

pub(crate) fn detected_image_extension(bytes: &[u8]) -> Option<&'static str> {
    ["png", "jpg", "webp", "gif", "bmp"]
        .into_iter()
        .find(|ext| magic_matches(ext, bytes))
}

fn magic_matches(ext: &str, bytes: &[u8]) -> bool {
    match ext {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "bmp" => bytes.starts_with(b"BM"),
        "wav" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE",
        "mp3" => {
            bytes.starts_with(b"ID3")
                || (bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0)
        }
        "mp4" | "mov" | "m4v" | "m4a" => bytes.len() >= 8 && &bytes[4..8] == b"ftyp",
        "webm" | "mkv" => bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]),
        "ogg" => bytes.starts_with(b"OggS"),
        "flac" => bytes.starts_with(b"fLaC"),
        "aac" => bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] & 0xF6 == 0xF0,
        _ => false,
    }
}

fn validate_data_uri(raw: &str, role: MediaRole) -> Result<(), String> {
    let (header, payload) = raw
        .split_once(',')
        .ok_or_else(|| "invalid data URI".to_string())?;
    let header = header
        .strip_prefix("data:")
        .ok_or_else(|| "invalid data URI".to_string())?;
    let mut fields = header.split(';');
    let mime = fields.next().unwrap_or("").trim().to_ascii_lowercase();
    if !fields.any(|field| field.eq_ignore_ascii_case("base64")) {
        return Err("data URI must use base64 encoding".into());
    }
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/mpeg" => "mp3",
        "audio/mp4" => "m4a",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/aac" => "aac",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "video/x-matroska" => "mkv",
        _ => return Err("data URI media type is not supported".into()),
    };
    if !role.accepts(&mime) {
        return Err(format!(
            "data URI media type `{mime}` is not allowed for {} inputs",
            role.label()
        ));
    }
    let encoded_limit = role
        .max_bytes()
        .saturating_mul(4)
        .div_ceil(3)
        .saturating_add(8);
    if payload.len() as u64 > encoded_limit {
        return Err(format!(
            "data URI exceeds the {} upload limit",
            role.label()
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| "data URI contains invalid base64".to_string())?;
    if bytes.len() as u64 > role.max_bytes() {
        return Err(format!(
            "data URI exceeds the {} upload limit",
            role.label()
        ));
    }
    if !magic_matches(ext, &bytes) {
        return Err("data URI bytes do not match their declared media type".into());
    }
    Ok(())
}

pub fn file_to_data_uri(raw: &str, project_root: Option<&str>) -> Result<String, String> {
    file_to_data_uri_as(raw, project_root, MediaRole::Image)
}

pub fn file_to_data_uri_as(
    raw: &str,
    project_root: Option<&str>,
    role: MediaRole,
) -> Result<String, String> {
    let raw = raw.trim();
    if raw.starts_with("http://") || raw.starts_with("https://") {
        return Ok(raw.to_string());
    }
    if raw.starts_with("data:") {
        validate_data_uri(raw, role)?;
        return Ok(raw.to_string());
    }
    let path = resolve_path(raw, project_root)?;
    let (mime, bytes) = load_media_as(&path, role)?;
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

    #[test]
    fn image_roles_reject_audio_and_video_files() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("voice.wav");
        std::fs::write(&audio, b"RIFF\x04\0\0\0WAVErest").unwrap();
        assert!(load_media_as(&audio, MediaRole::Image)
            .unwrap_err()
            .contains("not allowed for image"));

        let image = dir.path().join("still.png");
        std::fs::write(&image, b"\x89PNG\r\n\x1a\nrest").unwrap();
        assert!(load_media_as(&image, MediaRole::Video)
            .unwrap_err()
            .contains("not allowed for video"));
    }

    #[test]
    fn data_uris_are_mime_checked_and_size_limited() {
        let image = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nrest")
        );
        assert!(validate_data_uri(&image, MediaRole::Image).is_ok());
        assert!(validate_data_uri(&image, MediaRole::Audio).is_err());
        assert!(validate_data_uri("data:image/png,not-base64", MediaRole::Image).is_err());
    }
}

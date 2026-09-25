//! Project filesystem helpers.

use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryInfo {
    name: String,
    path: String,
    is_dir: bool,
}

fn path_from_str(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty() {
        return Err("Path is empty.".into());
    }
    Ok(path)
}

pub fn list_dir(path: String) -> Result<Vec<DirEntryInfo>, String> {
    let root = path_from_str(&path)?;
    if !root.is_dir() {
        return Err(format!("Not a directory: {}", root.display()));
    }

    let mut entries = Vec::new();
    let read = fs::read_dir(&root).map_err(|error| format!("Could not read directory: {error}"))?;

    for entry in read {
        let entry = entry.map_err(|error| format!("Could not read directory entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not read file type: {error}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        entries.push(DirEntryInfo {
            name,
            path: entry.path().to_string_lossy().to_string(),
            is_dir: file_type.is_dir(),
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

pub fn read_text_file(path: String) -> Result<String, String> {
    let path = path_from_str(&path)?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    fs::read_to_string(&path).map_err(|error| format!("Could not read file: {error}"))
}

pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    let path = path_from_str(&path)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent directories: {error}"))?;
        }
    }
    fs::write(&path, contents).map_err(|error| format!("Could not write file: {error}"))
}

pub fn create_file(path: String) -> Result<(), String> {
    let path = path_from_str(&path)?;
    if path.exists() {
        return Err(format!("Already exists: {}", path.display()));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent directories: {error}"))?;
        }
    }
    fs::write(&path, "").map_err(|error| format!("Could not create file: {error}"))
}

pub fn create_dir(path: String) -> Result<(), String> {
    let path = path_from_str(&path)?;
    if path.exists() {
        return Err(format!("Already exists: {}", path.display()));
    }
    fs::create_dir_all(&path).map_err(|error| format!("Could not create folder: {error}"))
}

fn is_inside(parent: &Path, child: &Path) -> bool {
    let parent = parent.components().collect::<Vec<_>>();
    let child = child.components().collect::<Vec<_>>();
    child.len() >= parent.len() && child[..parent.len()] == parent[..]
}

pub fn rename_path(from: String, to: String) -> Result<(), String> {
    let from = path_from_str(&from)?;
    let to = path_from_str(&to)?;
    if !from.exists() {
        return Err(format!("Not found: {}", from.display()));
    }
    if to.exists() {
        return Err(format!("Already exists: {}", to.display()));
    }
    if from.is_dir() && is_inside(&from, &to) {
        return Err("Cannot move a folder into itself.".into());
    }
    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent directories: {error}"))?;
        }
    }
    fs::rename(&from, &to).map_err(|error| format!("Could not move/rename: {error}"))
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| format!("Could not create folder: {error}"))?;
    let read = fs::read_dir(from).map_err(|error| format!("Could not read directory: {error}"))?;
    for entry in read {
        let entry = entry.map_err(|error| format!("Could not read directory entry: {error}"))?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not read file type: {error}"))?;
        if file_type.is_dir() {
            copy_dir_recursive(&src, &dest)?;
        } else {
            fs::copy(&src, &dest)
                .map_err(|error| format!("Could not copy {}: {error}", src.display()))?;
        }
    }
    Ok(())
}

pub fn copy_path(from: String, to: String) -> Result<(), String> {
    let from = path_from_str(&from)?;
    let to = path_from_str(&to)?;
    if !from.exists() {
        return Err(format!("Not found: {}", from.display()));
    }
    if to.exists() {
        return Err(format!("Already exists: {}", to.display()));
    }
    if from.is_dir() && is_inside(&from, &to) {
        return Err("Cannot copy a folder into itself.".into());
    }
    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent directories: {error}"))?;
        }
    }
    if from.is_dir() {
        copy_dir_recursive(&from, &to)
    } else {
        fs::copy(&from, &to)
            .map(|_| ())
            .map_err(|error| format!("Could not copy: {error}"))
    }
}

pub fn delete_path(path: String) -> Result<(), String> {
    let path = path_from_str(&path)?;
    let meta = fs::symlink_metadata(&path)
        .map_err(|error| format!("Not found: {} ({error})", path.display()))?;
    if meta.file_type().is_symlink() {
        // Remove the link itself; do not walk through to the target.
        return fs::remove_file(&path)
            .map_err(|error| format!("Could not delete symlink: {error}"));
    }
    if meta.is_dir() {
        // Prefer a clear error when a child is busy/read-only.
        match fs::remove_dir_all(&path) {
            Ok(()) => Ok(()),
            Err(error) => Err(format!(
                "Could not delete folder {}: {error}. Close any open files from this folder in the editor/media viewer and try again.",
                path.display()
            )),
        }
    } else {
        fs::remove_file(&path).map_err(|error| format!("Could not delete file: {error}"))
    }
}

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn is_probably_text(path: &Path) -> bool {
    const TEXT_EXTENSIONS: &[&str] = &[
        "md",
        "markdown",
        "txt",
        "yaml",
        "yml",
        "json",
        "toml",
        "csv",
        "ts",
        "tsx",
        "js",
        "jsx",
        "css",
        "html",
        "htm",
        "rs",
        "py",
        "sh",
        "xml",
        "svg",
        "env",
        "gitignore",
        "prompt",
    ];
    match extension_lower(path) {
        Some(ext) => TEXT_EXTENSIONS.iter().any(|allowed| *allowed == ext),
        None => {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            matches!(
                name,
                "prod.md" | "Makefile" | "Dockerfile" | "LICENSE" | "README"
            ) || !name.contains('.')
        }
    }
}

fn classify_path(path: &Path) -> &'static str {
    const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "avif", "ico"];
    const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mov", "mkv", "m4v"];
    const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "flac", "m4a", "aac", "opus"];

    if let Some(ext) = extension_lower(path) {
        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            return "image";
        }
        if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
            return "video";
        }
        if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            return "audio";
        }
    }
    if is_probably_text(path) {
        return "text";
    }
    "unsupported"
}

pub fn classify_file(path: String) -> Result<String, String> {
    let path = path_from_str(&path)?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    Ok(classify_path(&path).to_string())
}

pub fn can_open_in_editor(path: String) -> Result<bool, String> {
    let path = path_from_str(&path)?;
    Ok(path.is_file() && classify_path(&path) != "unsupported")
}

/// Return media file bytes as base64 (Electron may prefer a custom protocol instead).
pub fn read_media_bytes(path: String) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let path = path_from_str(&path)?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    let data = fs::read(&path).map_err(|error| format!("Could not read media file: {error}"))?;
    Ok(B64.encode(data))
}

/// Extract the selected out-frame (near last decoded frame) for continuity I2V.
/// Matches the Scene Generation specialist ffmpeg shape — not the first frame.
pub fn extract_out_frame(
    video_path: String,
    output_path: Option<String>,
) -> Result<String, String> {
    extract_frame_at(video_path, output_path, None)
}

/// Extract one frame. `at_seconds` uses an accurate seek; `None` stays near the end.
pub fn extract_frame_at(
    video_path: String,
    output_path: Option<String>,
    at_seconds: Option<f64>,
) -> Result<String, String> {
    let video = path_from_str(&video_path)?;
    if !video.is_file() {
        return Err(format!("Not a video file: {}", video.display()));
    }
    let out = if let Some(path) = output_path.filter(|s| !s.trim().is_empty()) {
        path_from_str(path.trim())?
    } else {
        let stem = video
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("frame");
        let parent = video
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| "Video path has no parent directory for .out-frame.png".to_string())?;
        parent.join(format!("{stem}.out-frame.png"))
    };

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(format!(
                "Output directory does not exist (will not create): {}",
                parent.display()
            ));
        }
    }

    let video_arg = video
        .to_str()
        .ok_or_else(|| "Video path is not valid UTF-8".to_string())?
        .to_string();
    let out_arg = out
        .to_str()
        .ok_or_else(|| "Output path is not valid UTF-8".to_string())?
        .to_string();
    let mut command = Command::new("ffmpeg");
    command.arg("-y");
    if let Some(seconds) = at_seconds {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("frame time must be a non-negative number of seconds".into());
        }
        // Decode up to the requested time so the still is that frame, not a nearby keyframe.
        command
            .arg("-i")
            .arg(&video_arg)
            .arg("-ss")
            .arg(format!("{seconds:.3}"));
    } else {
        // Specialist contract: last-frame fallback via -sseof -0.1 (never bare -frames:v 1).
        command
            .arg("-sseof")
            .arg("-0.1")
            .arg("-i")
            .arg(&video_arg);
    }
    let status = command
        .args(["-frames:v", "1", "-q:v", "2", &out_arg])
        .output()
        .map_err(|error| format!("Could not run ffmpeg (is it installed on PATH?): {error}"))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!(
            "ffmpeg failed extracting out-frame from {}: {stderr}",
            video.display()
        ));
    }
    if !out.is_file() {
        return Err(format!(
            "ffmpeg reported success but output missing: {}",
            out.display()
        ));
    }
    let bytes = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    let note = match at_seconds {
        Some(seconds) => format!(
            "Out-frame extracted at {seconds:.3}s. Upload this path as the start frame; do not treat it as a character reference."
        ),
        None => "Out-frame extracted near end of clip (-sseof -0.1). Upload this path as the start frame; do not treat it as a character reference.".into(),
    };
    Ok(json!({
        "ok": true,
        "video_path": video.to_string_lossy(),
        "output_path": out.to_string_lossy(),
        "frame_time_seconds": at_seconds,
        "bytes": bytes,
        "note": note,
    })
    .to_string())
}

/// Probe media duration in seconds via ffprobe (audio or video).
pub fn probe_duration_seconds(path: &str) -> Result<f64, String> {
    let path = path_from_str(path)?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            path.to_str()
                .ok_or_else(|| "Path is not valid UTF-8".to_string())?,
        ])
        .output()
        .map_err(|error| format!("Could not run ffprobe (is it on PATH?): {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed for {}: {stderr}", path.display()));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    text.parse::<f64>()
        .map_err(|_| format!("ffprobe returned non-numeric duration: {text}"))
}

/// Probe image/video stream width×height via ffprobe.
pub fn probe_image_size(path: &str) -> Result<(u32, u32), String> {
    let path = path_from_str(path)?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
            path.to_str()
                .ok_or_else(|| "Path is not valid UTF-8".to_string())?,
        ])
        .output()
        .map_err(|error| format!("Could not run ffprobe (is it on PATH?): {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ffprobe size failed for {}: {stderr}",
            path.display()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (w, h) = text
        .split_once('x')
        .ok_or_else(|| format!("ffprobe size parse failed: {text}"))?;
    let width = w
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("bad width: {w}"))?;
    let height = h
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("bad height: {h}"))?;
    Ok((width, height))
}

/// Pad leading silence (`adelay`) and optional trailing silence (`apad`) for talking-shot handles.
/// `head_ms` is total leading delay in milliseconds (handles + sync offset).
pub fn pad_audio_handles(
    input_path: &str,
    output_path: &str,
    head_ms: u64,
    tail_seconds: f64,
) -> Result<(), String> {
    let input = path_from_str(input_path)?;
    let output = path_from_str(output_path)?;
    if !input.is_file() {
        return Err(format!("Not an audio file: {}", input.display()));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent dirs: {error}"))?;
        }
    }
    let input_s = input
        .to_str()
        .ok_or_else(|| "Input path is not valid UTF-8".to_string())?;
    let output_s = output
        .to_str()
        .ok_or_else(|| "Output path is not valid UTF-8".to_string())?;

    let filter = if tail_seconds > 0.0 {
        format!("adelay={head_ms}|{head_ms},apad=pad_dur={tail_seconds}")
    } else {
        format!("adelay={head_ms}|{head_ms}")
    };

    let status = Command::new("ffmpeg")
        .args(["-y", "-i", input_s, "-af", &filter, output_s])
        .output()
        .map_err(|error| format!("Could not run ffmpeg (is it on PATH?): {error}"))?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!(
            "ffmpeg audio pad failed for {}: {stderr}",
            input.display()
        ));
    }
    if !output.is_file() {
        return Err(format!(
            "ffmpeg reported success but padded audio missing: {}",
            output.display()
        ));
    }
    Ok(())
}

/// Pad/letterbox a still into a 1080×1920 (9:16) talking still. Does not overwrite `input_path`.
pub fn ensure_talking_still_9x16(input_path: &str, output_path: &str) -> Result<(), String> {
    let input = path_from_str(input_path)?;
    let output = path_from_str(output_path)?;
    if !input.is_file() {
        return Err(format!("Not an image file: {}", input.display()));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create parent dirs: {error}"))?;
        }
    }
    let input_s = input
        .to_str()
        .ok_or_else(|| "Input path is not valid UTF-8".to_string())?;
    let output_s = output
        .to_str()
        .ok_or_else(|| "Output path is not valid UTF-8".to_string())?;
    let vf = "scale=1080:1080:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=0x2a241c";
    let status = Command::new("ffmpeg")
        .args(["-y", "-i", input_s, "-vf", vf, output_s])
        .output()
        .map_err(|error| format!("Could not run ffmpeg (is it on PATH?): {error}"))?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!(
            "ffmpeg 9:16 talking still failed for {}: {stderr}",
            input.display()
        ));
    }
    if !output.is_file() {
        return Err(format!(
            "ffmpeg reported success but talking still missing: {}",
            output.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod delete_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn deletes_nonempty_directory() {
        let root = std::env::temp_dir().join(format!("wb_del_{}", std::process::id()));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("a.txt"), b"hi").unwrap();
        delete_path(root.to_string_lossy().to_string()).unwrap();
        assert!(!root.exists());
    }
}

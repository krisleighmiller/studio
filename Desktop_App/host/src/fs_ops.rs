//! Project filesystem helpers.

use serde::Serialize;
use serde_json::json;
use std::fs;
use std::io::Write;
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

/// Generic file commands cannot bypass revision-locked workflow persistence.
/// Check the spelling (even when missing) and the resolved path (symlink aliases).
fn normalized_path(path: &Path) -> Result<PathBuf, String> {
    use std::path::Component;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err("path escapes filesystem root".into());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

fn resolved_path(path: &Path) -> Result<PathBuf, String> {
    let path = normalized_path(path)?;
    let mut ancestor = path.as_path();
    let mut missing = Vec::new();
    while fs::symlink_metadata(ancestor).is_err() {
        missing.push(ancestor.file_name().ok_or("invalid path")?.to_os_string());
        ancestor = ancestor.parent().ok_or("invalid path")?;
    }
    let mut result = ancestor
        .canonicalize()
        .map_err(|e| format!("path error: {e}"))?;
    for part in missing.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}

/// Reject the protected files or a directory operation carrying either file.
pub(crate) fn reject_workflow_mutation(path: &Path, recursive: bool) -> Result<(), String> {
    let Some(root) = crate::session::root() else {
        return Ok(());
    };
    let root = root
        .canonicalize()
        .map_err(|e| format!("project root: {e}"))?;
    let lexical = normalized_path(path)?;
    let resolved = resolved_path(path)?;
    for candidate in [&lexical, &resolved] {
        for protected in [
            root.join(crate::workflow::state::STATE_REL),
            root.join("Preproduction/.media-spend.lock"),
        ] {
            if candidate == &protected || (recursive && protected.starts_with(candidate)) {
                return Err(
                    "workflow state and its lock are managed by revision-locked persistence".into(),
                );
            }
        }
    }
    Ok(())
}

/// Recursive copy dereferences symlinked files, including nested aliases.
pub(crate) fn reject_workflow_copy_source(path: &Path) -> Result<(), String> {
    reject_workflow_mutation(path, path.is_dir())?;
    if path.is_dir()
        && !fs::symlink_metadata(path)
            .map_err(|e| format!("Could not inspect copy source: {e}"))?
            .file_type()
            .is_symlink()
    {
        for entry in fs::read_dir(path).map_err(|e| format!("Could not read directory: {e}"))? {
            let entry = entry.map_err(|e| format!("Could not read directory entry: {e}"))?;
            reject_workflow_copy_source(&entry.path())?;
        }
    }
    Ok(())
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

const MAX_TEXT_BYTES: u64 = 10_000_000;

pub fn read_text_file(path: String) -> Result<String, String> {
    let path = path_from_str(&path)?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    let len = fs::metadata(&path)
        .map_err(|error| format!("Could not read file: {error}"))?
        .len();
    if len > MAX_TEXT_BYTES {
        return Err("Text file is larger than 10 MB.".into());
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

fn validate_script(filename: &str, text: &str) -> Result<(), String> {
    if filename.is_empty()
        || filename != filename.trim()
        || filename.contains(['/', '\\', '\0', '\n', '\r', ':'])
        || Path::new(filename).file_name().and_then(|name| name.to_str()) != Some(filename)
    {
        return Err("Use a filename only, without directories or traversal.".into());
    }
    let extension = Path::new(filename).extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if !matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt") {
        return Err("Choose a .md, .markdown, or .txt script filename.".into());
    }
    if text.trim().is_empty() {
        return Err("Script text must not be empty.".into());
    }
    if crate::script::extract_scenes(text).is_empty() {
        return Err("Script must contain parseable Scene N headings.".into());
    }
    Ok(())
}

/// Save validated local script text without replacing any existing file.
pub fn save_project_script(root: &Path, filename: &str, text: &str) -> Result<String, String> {
    use crate::project_path::{relative_to_root, resolve, PathIntent};
    use std::io::Write;

    validate_script(filename, text)?;
    let root_string = root.to_str().ok_or("Invalid project root")?;
    let scripts = resolve("Scripts", Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?.into_path_buf();
    fs::create_dir_all(&scripts).map_err(|error| format!("Could not create Scripts: {error}"))?;
    // Recheck after creation; never accept a Scripts redirect, even inside the project.
    let scripts = resolve("Scripts", Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?.into_path_buf();
    let relative = format!("Scripts/{filename}");
    let target = resolve(&relative, Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?.into_path_buf();
    if target.parent() != Some(scripts.as_path()) || relative_to_root(root, &target)? != Path::new(&relative) {
        return Err("Script destination must be directly inside Scripts/.".into());
    }
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&target)
        .map_err(|error| format!("Could not save script (existing files are never replaced): {error}"))?;
    if let Err(error) = file.write_all(text.as_bytes()).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&target);
        return Err(format!("Could not write script: {error}"));
    }
    Ok(relative)
}

/// Save the exact UTF-8 notes text to the fixed project-root Notes.md file.
pub fn save_project_notes(root: &Path, text: &str) -> Result<String, String> {
    use crate::project_path::{relative_to_root, resolve, PathIntent};

    let root_string = root.to_str().ok_or("Invalid project root")?;
    let relative = Path::new("Notes.md");
    let target = resolve("Notes.md", Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?
        .into_path_buf();
    if relative_to_root(root, &target)? != relative {
        return Err("Notes destination must be the project-root Notes.md file.".into());
    }
    // Recheck the fixed target immediately before writing. A Notes.md symlink
    // must be refused rather than followed, even when it points back inside.
    let rechecked = resolve("Notes.md", Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?
        .into_path_buf();
    if rechecked != target || relative_to_root(root, &rechecked)? != relative {
        return Err("Notes destination changed before it could be saved.".into());
    }
    let temp_relative = Path::new("Notes.md.tmp");
    let temp = resolve("Notes.md.tmp", Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?
        .into_path_buf();
    if temp.parent() != target.parent() || relative_to_root(root, &temp)? != temp_relative {
        return Err("Notes temporary file must stay beside project-root Notes.md.".into());
    }
    write_notes_then_replace(&target, &temp, text, |from, to| {
        // Recheck at the replacement boundary so a redirected destination is
        // refused after the temporary file is safely on disk as well.
        let final_target = resolve("Notes.md", Some(root_string), PathIntent::Write)
            .map_err(|error| error.to_string())?
            .into_path_buf();
        if final_target != target || relative_to_root(root, &final_target)? != relative {
            return Err("Notes destination changed before it could be replaced.".into());
        }
        fs::rename(from, to).map_err(|error| format!("Could not replace Notes.md: {error}"))
    })?;
    Ok("Notes.md".into())
}

fn write_notes_then_replace<F>(
    target: &Path,
    temp: &Path,
    text: &str,
    replace: F,
) -> Result<(), String>
where
    F: FnOnce(&Path, &Path) -> Result<(), String>,
{
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(temp)
            .map_err(|error| format!("Could not write Notes.md temporary file: {error}"))?;
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not sync Notes.md temporary file: {error}"))?;
        drop(file);
        replace(temp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

/// Ingest a local script through the shared saver; never import workflow state.
pub fn ingest_script_file(root: &Path, source_path: &str, source_kind: &str) -> Result<String, String> {
    use crate::project_path::{resolve, PathIntent};

    let root_string = root.to_str().ok_or("Invalid project root")?;
    let source = match source_kind {
        "existing-project-file" => resolve(source_path, Some(root_string), PathIntent::Read)
            .map_err(|error| error.to_string())?.into_path_buf(),
        "external-import" => crate::session::authorize(source_path, PathIntent::Read)?,
        _ => return Err("Choose external-import or existing-project-file.".into()),
    };
    let filename = source.file_name().and_then(|name| name.to_str()).ok_or("Invalid script filename")?;
    let text = read_text_file(source.to_string_lossy().into_owned())?;
    validate_script(filename, &text)?;
    let scripts = resolve("Scripts", Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?.into_path_buf();
    if crate::project_path::relative_to_root(root, &scripts)? != Path::new("Scripts") {
        return Err("Script destination must be inside Scripts/.".into());
    }
    if source.starts_with(&scripts) {
        return Ok(crate::project_path::relative_to_root(root, &source)?.to_string_lossy().into_owned());
    }
    let relative = format!("Scripts/{filename}");
    let target = resolve(&relative, Some(root_string), PathIntent::Write)
        .map_err(|error| error.to_string())?.into_path_buf();
    if target.parent() != Some(scripts.as_path()) {
        return Err("Script destination must be directly inside Scripts/.".into());
    }
    if fs::read(&target).map(|bytes| bytes == text.as_bytes()).unwrap_or(false) {
        return Ok(relative);
    }
    save_project_script(root, filename, &text)
}

/// Acquire bytes only: reference selection and identity readiness belong to callers.
/// The returned project-relative asset path is stable across identical acquisitions.
pub fn ingest_character_reference_image(
    root: &Path,
    character_dir: &str,
    source_path: &str,
) -> Result<(String, bool), String> {
    use crate::project_path::{relative_to_root, resolve, PathIntent};
    use std::io::{Read, Write};
    use std::path::Component;
    use std::process::Stdio;

    let root = root.canonicalize().map_err(|e| format!("Project root: {e}"))?;
    let root_string = root.to_str().ok_or("Invalid project root")?;
    let requested = Path::new(character_dir);
    let relative = if requested.is_absolute() {
        requested.strip_prefix(&root).map_err(|_| "Character directory is outside the project")?
    } else {
        requested
    };
    let parts: Vec<_> = relative.components().collect();
    if parts.len() != 2 || parts[0] != Component::Normal("Characters".as_ref())
        || !matches!(parts[1], Component::Normal(_)) || character_dir.contains('\\') {
        return Err("Choose a character directory directly inside Characters/.".into());
    }
    let character = resolve(character_dir, Some(root_string), PathIntent::Write)
        .map_err(|e| e.to_string())?.into_path_buf();
    if relative_to_root(&root, &character)? != relative || !character.is_dir() {
        return Err("Character directory must exist and cannot redirect to another directory.".into());
    }
    let bucket_relative = relative.join("refs");
    let bucket_raw = bucket_relative.to_str().ok_or("Invalid character directory")?;
    let bucket = resolve(bucket_raw, Some(root_string), PathIntent::Write)
        .map_err(|e| e.to_string())?.into_path_buf();
    if relative_to_root(&root, &bucket)? != bucket_relative {
        return Err("Reference destination must be the character's refs directory.".into());
    }

    let source = crate::session::authorize(source_path, PathIntent::Read)?;
    if !source.is_file() {
        return Err("Reference image must be a readable file.".into());
    }
    let filename = source.file_name().and_then(|n| n.to_str()).ok_or("Invalid image filename")?;
    let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let format = match extension.as_str() {
        "png" => "png_pipe",
        "jpg" | "jpeg" => "jpeg_pipe",
        _ => return Err("Choose a PNG or JPEG reference image.".into()),
    };
    const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
    let mut bytes = Vec::new();
    fs::File::open(&source).map_err(|e| format!("Could not read image: {e}"))?
        .take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read image: {e}"))?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("Reference image exceeds 32 MiB.".into());
    }
    let signature_ok = if extension == "png" {
        bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.ends_with(b"\0\0\0\0IEND\xaeB`\x82")
    } else {
        bytes.starts_with(&[0xff, 0xd8, 0xff]) && bytes.ends_with(&[0xff, 0xd9])
    };
    if !signature_ok {
        return Err("Invalid or truncated PNG/JPEG image.".into());
    }
    // Decode the exact snapshot we copy, not a second read of a mutable source.
    // A forced image demuxer and pipe-only protocol cannot open remote/local links.
    let mut decoder = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-xerror", "-err_detect", "explode",
            "-protocol_whitelist", "pipe", "-f", format, "-i", "pipe:0",
            "-map", "0:v:0", "-f", "null", "-"])
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|e| format!("Could not validate image with ffmpeg: {e}"))?;
    let sent = decoder.stdin.take().ok_or("Missing image decoder input")?.write_all(&bytes);
    let decoded = decoder.wait().map_err(|e| format!("Could not validate image: {e}"))?;
    if sent.is_err() || !decoded.success() {
        return Err("Corrupt or undecodable reference image.".into());
    }

    fs::create_dir_all(&bucket).map_err(|e| format!("Could not create references: {e}"))?;
    let bucket = resolve(bucket_raw, Some(root_string), PathIntent::Write)
        .map_err(|e| e.to_string())?.into_path_buf();
    let asset_relative = bucket_relative.join(filename);
    let asset_raw = asset_relative.to_str().ok_or("Invalid image filename")?;
    let target = resolve(asset_raw, Some(root_string), PathIntent::Write)
        .map_err(|e| e.to_string())?.into_path_buf();
    if relative_to_root(&root, &bucket)? != bucket_relative
        || target.parent() != Some(bucket.as_path())
        || relative_to_root(&root, &target)? != asset_relative {
        return Err("Reference destination escaped the character's refs directory.".into());
    }
    if fs::read(&target).map(|existing| existing == bytes).unwrap_or(false) {
        return Ok((asset_raw.to_owned(), false));
    }
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&target)
        .map_err(|e| format!("Could not acquire reference (existing files are never replaced): {e}"))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&target);
        return Err(format!("Could not copy reference image: {error}"));
    }
    Ok((asset_raw.to_owned(), true))
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
const MAX_MEDIA_READ_BYTES: u64 = 80_000_000;

pub fn read_media_bytes(path: String) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
    let path = path_from_str(&path)?;
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    let len = fs::metadata(&path)
        .map_err(|error| format!("Could not read media file: {error}"))?
        .len();
    if len > MAX_MEDIA_READ_BYTES {
        return Err("Media file is larger than 80 MB.".into());
    }
    let data = fs::read(&path).map_err(|error| format!("Could not read media file: {error}"))?;
    Ok(B64.encode(data))
}

pub fn delete_scoped(path: String) -> Result<(), String> {
    let path = path_from_str(&path)?;
    let Some(root) = crate::session::root() else {
        return Err("Open a project first.".into());
    };
    let root_canon = root
        .canonicalize()
        .map_err(|error| format!("project root: {error}"))?;
    let meta = fs::symlink_metadata(&path)
        .map_err(|error| format!("Not found: {} ({error})", path.display()))?;
    if meta.file_type().is_symlink() {
        let parent = path.parent().unwrap_or(&path);
        let parent_canon = parent
            .canonicalize()
            .map_err(|error| format!("path error: {error}"))?;
        if parent_canon != root_canon && !parent_canon.starts_with(&root_canon) {
            return Err("Refusing to delete a file outside the project.".into());
        }
        return fs::remove_file(&path)
            .map_err(|error| format!("Could not delete symlink: {error}"));
    }
    let canon = path
        .canonicalize()
        .map_err(|error| format!("path error: {error}"))?;
    if canon == root_canon {
        return Err("Refusing to delete the project root.".into());
    }
    if canon != root_canon && !canon.starts_with(&root_canon) {
        return Err("Refusing to delete a file outside the project.".into());
    }
    if meta.is_file() {
        crate::project_path::move_to_trash(&root_canon, &canon).map(|_| ())
    } else if meta.is_dir() {
        let trash = root_canon.join(".workbench-trash");
        fs::create_dir_all(&trash).map_err(|error| format!("could not create trash: {error}"))?;
        let name = canon
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "folder".into());
        let dest = trash.join(format!(
            "{}-{name}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or(0)
        ));
        fs::rename(&canon, &dest).map_err(|error| format!("Could not trash folder: {error}"))
    } else {
        Err("Refusing to delete that path.".into())
    }
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

    let video_arg = crate::ffmpeg_path::ffmpeg_local_arg(&video)?;
    let out_arg = crate::ffmpeg_path::ffmpeg_local_arg(&out)?;
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
        command.arg("-sseof").arg("-0.1").arg("-i").arg(&video_arg);
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
    use std::{io::Write, time::{Duration, Instant}};
    use crate::project_path::{self, PathIntent};
    let root = crate::session::command_root(None)?;
    if !tail_seconds.is_finite() || !(0.0..=120.0).contains(&tail_seconds) || head_ms > 120_000 {
        return Err("Audio handles must be finite and at most 120 seconds".into());
    }
    let input = project_path::resolve(input_path, root.to_str(), PathIntent::Read).map_err(|e|e.to_string())?.into_path_buf();
    let output = project_path::resolve(output_path, root.to_str(), PathIntent::Write).map_err(|e|e.to_string())?.into_path_buf();
    let rel = project_path::relative_to_root(&root, &input)?.to_string_lossy().replace('\\', "/");
    let parts: Vec<_> = rel.split('/').collect();
    if parts.len()!=4 || parts[0]!="Scenes" || !parts[1].starts_with("Scene") || !parts[2].starts_with("Cut")
        || input.parent()!=output.parent() || !input.is_file()
        || !input.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        || !output.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        || fs::symlink_metadata(output_path).is_ok() || output.exists() {
        return Err("Padded WAV requires an unoccupied destination in the exact performance shot bucket".into());
    }
    let filter = format!("adelay={head_ms}|{head_ms},apad=pad_dur={tail_seconds}");
    let mut command = Command::new("ffmpeg");
    command.args(["-nostdin","-v","error","-xerror","-err_detect","explode","-protocol_whitelist","file","-format_whitelist","wav","-f","wav","-i"])
        .arg(&input).args(["-map","0:a:0","-vn","-map_metadata","-1","-af",&filter,"-c:a","pcm_s16le","-f","wav","pipe:1"]);
    let (mut wav, _) = crate::media_inspect::run_bounded(&mut command, Instant::now()+Duration::from_secs(60),128*1024*1024)?;
    crate::workflow::audio_edit::finalize_pcm_wav(&mut wav)?;
    // Revalidate before exclusive creation. FFmpeg never opens the destination,
    // so an occupied file or symlink cannot be overwritten through `-y`.
    let rechecked = project_path::resolve(output_path, root.to_str(), PathIntent::Write).map_err(|e|e.to_string())?.into_path_buf();
    if rechecked != output {return Err("Padded WAV destination changed".into());}
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&output)
        .map_err(|e|format!("Cannot reserve padded WAV without overwriting: {e}"))?;
    file.write_all(&wav).and_then(|_|file.sync_all()).map_err(|e|format!("Cannot save padded WAV: {e}"))
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
mod notes_tests {
    use super::*;

    #[test]
    fn notes_file_atomic_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir(&root).unwrap();
        let target = root.join("Notes.md");
        let temp = root.join("Notes.md.tmp");
        let original = b"prior notes remain intact\n";
        fs::write(&target, original).unwrap();

        let result = write_notes_then_replace(
            &target,
            &temp,
            "replacement must not be reported as saved\n",
            |_, _| Err("forced final replacement failure".into()),
        );

        let error = result.expect_err("failed replacement must not report success");
        assert_eq!(error, "forced final replacement failure");
        assert_eq!(fs::read(&target).unwrap(), original);
        assert!(!temp.exists(), "failed replacement must clean its temporary file");
    }
}

#[cfg(test)]
mod delete_tests {
    use super::*;
    use std::fs;

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

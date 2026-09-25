//! ffmpeg treats some path spellings as options or protocols. Local files go
//! in as canonical `file:` URLs with no newlines.

use std::path::Path;

pub fn ffmpeg_local_arg(path: &Path) -> Result<String, String> {
    let raw = path.to_string_lossy();
    if raw.contains('\n') || raw.contains('\r') || raw.contains('\0') {
        return Err(format!(
            "refusing an ffmpeg path that contains a newline: {}",
            path.display()
        ));
    }
    let absolute = if path.exists() {
        path.canonicalize()
            .map_err(|error| format!("ffmpeg path: {error}"))?
    } else if path.is_absolute() {
        path.to_path_buf()
    } else {
        return Err(format!(
            "ffmpeg path must exist or be absolute: {}",
            path.display()
        ));
    };
    let text = absolute.to_string_lossy();
    if text.starts_with('-') {
        return Err("refusing an ffmpeg path that starts with '-'".into());
    }
    Ok(format!("file:{text}"))
}

pub fn concat_list_line(path: &Path) -> Result<String, String> {
    let absolute = path
        .canonicalize()
        .map_err(|error| format!("concat input: {error}"))?;
    let text = absolute.to_string_lossy();
    if text.contains('\n') || text.contains('\r') || text.contains('\'') {
        return Err(format!(
            "refusing a concat path with a newline or quote: {}",
            absolute.display()
        ));
    }
    Ok(format!("file '{text}'\n"))
}

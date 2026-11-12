//! Local, non-destructive crop of a registered scene-look still.
use super::{import, look, state::{AssetRecord, AssetStatus}};
use crate::{media_inspect::run_bounded, project_path::{self, PathIntent}};
use serde_json::{json, Value};
use std::{fs::{self, OpenOptions}, io::Write, path::Path, process::Command, time::{Duration, Instant}};

fn positive(args: &Value, key: &str) -> Result<u32, String> {
    args[key].as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0)
        .ok_or_else(|| format!("Invalid {key}"))
}
fn nonnegative(args: &Value, key: &str) -> Result<u32, String> {
    args[key].as_u64().and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("Invalid {key}"))
}

pub fn crop(args: &Value, root: &Path) -> Result<Value, String> {
    let scene_id = positive(args, "sceneId")?;
    let slot_id = args["slotId"].as_str().filter(|s| !s.is_empty()).ok_or("Missing slotId")?;
    let source_id = args["assetId"].as_str().filter(|s| !s.is_empty()).ok_or("Missing assetId")?;
    let (x, y, width, height) = (nonnegative(args, "x")?, nonnegative(args, "y")?, positive(args, "width")?, positive(args, "height")?);
    let mut state = import::load_state(root)?;
    let slot = state.scene(scene_id).and_then(|s| s.look_board.as_ref())
        .and_then(|b| b.slots.iter().find(|s| s.id == slot_id)).ok_or("Look slot not found")?;
    if !slot.asset_ids.iter().any(|id| id == source_id) { return Err("Image is not on this look slot".into()); }
    let source = state.asset(source_id).filter(|a| a.scene_id == Some(scene_id) && a.role == slot.role && a.role.is_scene_look())
        .ok_or("Registered look image not found")?.clone();
    let ext = Path::new(&source.path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") { return Err("Crop supports PNG, JPEG or WebP stills".into()); }
    let root_str = root.to_string_lossy();
    let input = project_path::resolve(&source.path, Some(&root_str), PathIntent::Read).map_err(|e| e.to_string())?.into_path_buf();
    project_path::under_scene_bucket(root, &input, "references")?;
    let before = fs::read(&input).map_err(|e| e.to_string())?;
    if before.is_empty() || before.len() > 64 * 1024 * 1024 { return Err("Still must be 1–64 MiB".into()); }
    let format = match ext.as_str() {"png" => "png_pipe", "jpg" | "jpeg" => "jpeg_pipe", "webp" => "webp_pipe", _ => unreachable!()};
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut probe = Command::new("ffprobe");
    probe.args(["-v", "error", "-protocol_whitelist", "file", "-format_whitelist", format, "-f", format, "-show_entries", "stream=width,height,codec_type", "-of", "json"]).arg(&input);
    let (raw, _) = run_bounded(&mut probe, deadline, 64 * 1024)?;
    let parsed: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let stream = parsed["streams"].as_array().and_then(|s| s.first()).ok_or("No image stream")?;
    let image_width = stream["width"].as_u64().ok_or("Invalid image dimensions")?;
    let image_height = stream["height"].as_u64().ok_or("Invalid image dimensions")?;
    if image_width > 8192 || image_height > 8192 || image_width == 0 || image_height == 0
        || u64::from(x) + u64::from(width) > image_width || u64::from(y) + u64::from(height) > image_height
        || (x == 0 && y == 0 && u64::from(width) == image_width && u64::from(height) == image_height) {
        return Err("Crop must be a smaller rectangle inside an image of at most 8192 × 8192".into());
    }
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg.args(["-nostdin", "-v", "error", "-xerror", "-err_detect", "explode", "-protocol_whitelist", "file", "-format_whitelist", format, "-f", format, "-i"])
        .arg(&input).args(["-frames:v", "1", "-vf", &format!("crop={width}:{height}:{x}:{y}:exact=1"), "-threads", "1", "-f", "image2pipe", "-vcodec", "png", "pipe:1"]);
    let (bytes, _) = run_bounded(&mut ffmpeg, deadline, 128 * 1024 * 1024)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") { return Err("Crop produced no PNG".into()); }
    if fs::read(&input).map_err(|e| e.to_string())? != before { return Err("Source image changed during crop; reload and retry".into()); }
    // Reuse the look board's version allocator, but never trust its final 99+ fallback.
    let rel = look::next_look_rel(root, state.scene(scene_id).ok_or("Scene missing")?, source.role, "png");
    let dest = project_path::resolve(&rel, Some(&root_str), PathIntent::Write).map_err(|e| e.to_string())?.into_path_buf();
    project_path::under_scene_bucket(root, &dest, "references")?;
    if state.assets.iter().any(|a| a.path == rel) || state.jobs.iter().any(|j| j.output_path.as_deref() == Some(&rel)) { return Err("Look output version already reserved".into()); }
    fs::create_dir_all(dest.parent().ok_or("Invalid image destination")?).map_err(|e| e.to_string())?;
    // Recheck after folder creation: path policy rejects redirected/symlink buckets.
    let dest = project_path::resolve(&rel, Some(&root_str), PathIntent::Write).map_err(|e| e.to_string())?.into_path_buf();
    if project_path::relative_to_root(root, &dest)? != Path::new(&rel) { return Err("Look output redirected".into()); }
    let mut output = OpenOptions::new().write(true).create_new(true).open(&dest).map_err(|e| format!("Cannot reserve crop output: {e}"))?;
    if let Err(e) = output.write_all(&bytes).and_then(|_| output.sync_all()) {
        drop(output); let _ = fs::remove_file(&dest); return Err(format!("Cannot save crop: {e}"));
    }
    drop(output);
    let id = state.push_asset(AssetRecord { id: String::new(), path: rel.clone(), role: source.role, status: AssetStatus::Draft,
        scene_id: Some(scene_id), cut_id: None, character_id: None, source_job_id: None,
        parent_asset_id: Some(source_id.to_string()), notes: Some(format!("Local crop from {source_id}: {x},{y} {width}×{height}")) });
    let board = state.scenes.iter_mut().find(|s| s.id == scene_id).and_then(|s| s.look_board.as_mut()).ok_or("Look board vanished")?;
    board.slot_mut(slot_id).ok_or("Look slot vanished")?.asset_ids.push(id.clone());
    board.sync_asset_ids();
    let saved = match import::save_project(root, state) { Ok(saved) => saved, Err(e) => { let _ = fs::remove_file(&dest); return Err(e); } };
    Ok(json!({"state":saved.state,"report":saved.report,"editedLook":{"slotId":slot_id,"assetId":id,"sourceAssetId":source_id,"path":rel},"note":"Crop saved as a new draft. Review and explicitly approve it to replace the approved still."}))
}

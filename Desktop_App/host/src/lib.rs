//! Workbench host library: FS, settings, director, providers, voice.

pub mod assemble;
pub mod atlas;
pub mod chat;
pub mod ctx;
pub mod director;
pub mod director_actions;
pub mod elevenlabs;
pub mod specialists;
pub mod fal;
pub mod fs_ops;
pub mod gemini_images;
pub mod http_util;
pub mod kling;
pub mod luma;
pub mod media;
pub mod media_util;
pub mod openai_images;
pub mod prompts;
pub mod replicate;
pub mod runway;
pub mod settings;
pub mod stability;
pub mod sync_labs;
pub mod voice;

use crate::ctx::HostCtx;
use crate::voice::RecorderState;
use serde_json::{json, Value};

/// Dispatch one command by name. Arguments use the same camelCase shapes as the former Tauri invoke API.
pub fn dispatch(
    app: &HostCtx,
    recorder: &RecorderState,
    cmd: &str,
    args: &Value,
) -> Result<Value, String> {
    match cmd {
        "list_dir" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::list_dir(path))
        }
        "read_text_file" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::read_text_file(path))
        }
        "write_text_file" => {
            let path = arg_string(args, "path")?;
            let contents = arg_string(args, "contents")?;
            to_value(fs_ops::write_text_file(path, contents))
        }
        "create_file" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::create_file(path))
        }
        "create_dir" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::create_dir(path))
        }
        "rename_path" => {
            let from = arg_string(args, "from")?;
            let to = arg_string(args, "to")?;
            to_value(fs_ops::rename_path(from, to))
        }
        "copy_path" => {
            let from = arg_string(args, "from")?;
            let to = arg_string(args, "to")?;
            to_value(fs_ops::copy_path(from, to))
        }
        "delete_path" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::delete_path(path))
        }
        "classify_file" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::classify_file(path))
        }
        "can_open_in_editor" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::can_open_in_editor(path))
        }
        "read_media_bytes" => {
            let path = arg_string(args, "path")?;
            to_value(fs_ops::read_media_bytes(path))
        }
        "load_settings" => to_value(settings::load_settings(app.clone())),
        "save_settings" => {
            let settings = args
                .get("settings")
                .cloned()
                .ok_or_else(|| "Missing settings".to_string())?;
            to_value(settings::save_settings(app.clone(), settings))
        }
        "fetch_atlas_catalog" => {
            let api_key = arg_string(args, "apiKey")?;
            to_value(settings::fetch_atlas_catalog(api_key))
        }
        "fetch_api_catalog" => {
            let kind = arg_string(args, "kind")?;
            let api_key = opt_string(args, "apiKey").unwrap_or_default();
            let base_url = opt_string(args, "baseUrl").unwrap_or_default();
            to_value(settings::fetch_api_catalog(kind, api_key, base_url))
        }
        "director_turn" => {
            let request: director::DirectorTurnRequest = serde_json::from_value(
                args.get("request")
                    .cloned()
                    .ok_or_else(|| "Missing request".to_string())?,
            )
            .map_err(|error| format!("Invalid director request: {error}"))?;
            to_value(director::director_turn(app.clone(), request))
        }
        "elevenlabs_list_voices" => {
            let search = opt_string(args, "search");
            let voice_type = opt_string(args, "voiceType");
            let page_size = args.get("pageSize").and_then(|v| v.as_u64()).map(|v| v as u32);
            let next_page_token = opt_string(args, "nextPageToken");
            to_value(elevenlabs::elevenlabs_list_voices(
                app.clone(),
                search,
                voice_type,
                page_size,
                next_page_token,
            ))
        }
        "elevenlabs_tts_preview" => {
            let voice_id = arg_string(args, "voiceId")?;
            let text = arg_string(args, "text")?;
            to_value(elevenlabs::elevenlabs_tts_preview(
                app.clone(),
                voice_id,
                text,
            ))
        }
        "elevenlabs_design_voice" => {
            let voice_description = arg_string(args, "voiceDescription")?;
            let text = arg_string(args, "text")?;
            to_value(elevenlabs::elevenlabs_design_voice(
                app.clone(),
                voice_description,
                text,
            ))
        }
        "elevenlabs_create_designed_voice" => {
            let voice_name = arg_string(args, "voiceName")?;
            let voice_description = arg_string(args, "voiceDescription")?;
            let generated_voice_id = arg_string(args, "generatedVoiceId")?;
            to_value(elevenlabs::elevenlabs_create_designed_voice(
                app.clone(),
                voice_name,
                voice_description,
                generated_voice_id,
            ))
        }
        "elevenlabs_download_preview_url" => {
            let url = arg_string(args, "url")?;
            to_value(elevenlabs::elevenlabs_download_preview_url(url))
        }
        "voice_list_recording_sources" => to_value(voice::voice_list_recording_sources()),
        "voice_start_recording" => {
            let output_path = arg_string(args, "outputPath")?;
            let source_id = args
                .get("sourceId")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            to_value(voice::voice_start_recording(
                recorder,
                output_path,
                source_id,
            ))
        }
        "voice_stop_recording" => to_value(voice::voice_stop_recording(recorder)),
        "voice_recording_level" => to_value(voice::voice_recording_level(recorder)),
        "voice_save_recording" => {
            let request = args
                .get("request")
                .cloned()
                .ok_or_else(|| "Missing request".to_string())?;
            let request: voice::SaveRecordingRequest = serde_json::from_value(request)
                .map_err(|error| format!("Invalid save recording request: {error}"))?;
            to_value(voice::voice_save_recording(request))
        }
        "voice_save_library_voice" => {
            let request = args
                .get("request")
                .cloned()
                .ok_or_else(|| "Missing request".to_string())?;
            let request: voice::SaveLibraryVoiceRequest = serde_json::from_value(request)
                .map_err(|error| format!("Invalid save library voice request: {error}"))?;
            to_value(voice::voice_save_library_voice(request))
        }
        "voice_save_designed_voice" => {
            let request = args
                .get("request")
                .cloned()
                .ok_or_else(|| "Missing request".to_string())?;
            let request: voice::SaveDesignedVoiceRequest = serde_json::from_value(request)
                .map_err(|error| format!("Invalid save designed voice request: {error}"))?;
            to_value(voice::voice_save_designed_voice(app.clone(), request))
        }
        "ping" => Ok(json!("pong")),
        other => Err(format!("Unknown command: {other}")),
    }
}

fn arg_string(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Missing or invalid argument: {key}"))
}

fn opt_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn to_value<T: serde::Serialize>(result: Result<T, String>) -> Result<Value, String> {
    result.and_then(|value| {
        serde_json::to_value(value).map_err(|error| format!("Could not serialize result: {error}"))
    })
}

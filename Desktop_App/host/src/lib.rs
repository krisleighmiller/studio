//! Workbench host library: FS, settings, director, providers, voice.

pub mod assemble;
pub mod atlas;
pub mod chat;
pub mod ctx;
pub mod director;
pub mod director_actions;
pub mod elevenlabs;
pub mod fal;
pub mod ffmpeg_path;
pub mod fs_ops;
pub mod gemini_images;
pub mod http_util;
pub mod kling;
pub mod luma;
pub mod media;
pub mod media_util;
pub mod openai_images;
pub mod output_paths;
pub mod pending;
pub mod project_path;
pub mod prompts;
pub mod replicate;
pub mod runway;
pub mod scene_plan;
pub mod script;
pub mod secret_store;
pub mod secrets;
pub mod session;
pub mod settings;
pub mod specialists;
pub mod stability;
pub mod sync_labs;
pub mod text_writes;
pub mod voice;
pub mod workflow;

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
        "open_project" => {
            let path = arg_string(args, "path")?;
            pending::cancel_all();
            to_value(session::open(&path).map(|root| json!({ "root": root })))
        }
        "close_project" => {
            pending::cancel_all();
            session::close();
            Ok(json!({ "closed": true }))
        }
        "grant_path" => {
            let path = arg_string(args, "path")?;
            let intent = match arg_string(args, "intent")?.as_str() {
                "read" => project_path::PathIntent::Read,
                "write" => project_path::PathIntent::Write,
                _ => return Err("Dialog grants must declare read or write intent.".into()),
            };
            to_value(session::grant(&path, intent).map(|path| json!({ "path": path })))
        }
        "list_dir" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Read)?;
            to_value(fs_ops::list_dir(path))
        }
        "read_text_file" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Read)?;
            to_value(fs_ops::read_text_file(path))
        }
        "write_text_file" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Write)?;
            let contents = arg_string(args, "contents")?;
            to_value(fs_ops::write_text_file(path, contents))
        }
        "create_file" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Write)?;
            to_value(fs_ops::create_file(path))
        }
        "create_dir" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Write)?;
            to_value(fs_ops::create_dir(path))
        }
        "rename_path" => {
            let from = scoped_path(&arg_string(args, "from")?, project_path::PathIntent::Write)?;
            let to = scoped_path(&arg_string(args, "to")?, project_path::PathIntent::Write)?;
            to_value(fs_ops::rename_path(from, to))
        }
        "copy_path" => {
            let from = scoped_path(&arg_string(args, "from")?, project_path::PathIntent::Read)?;
            let to = scoped_path(&arg_string(args, "to")?, project_path::PathIntent::Write)?;
            to_value(fs_ops::copy_path(from, to))
        }
        "delete_path" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Write)?;
            to_value(fs_ops::delete_scoped(path))
        }
        "classify_file" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Read)?;
            to_value(fs_ops::classify_file(path))
        }
        "can_open_in_editor" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Read)?;
            to_value(fs_ops::can_open_in_editor(path))
        }
        "read_media_bytes" => {
            let path = scoped_path(&arg_string(args, "path")?, project_path::PathIntent::Read)?;
            to_value(fs_ops::read_media_bytes(path))
        }
        "load_settings" => {
            let mut value = settings::load_settings(app.clone())?;
            settings::redact_settings_for_renderer(&mut value);
            Ok(value)
        }
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
        "fetch_api_catalog" => to_value(settings::fetch_catalog_for_renderer(app, args)),
        "director_turn" => {
            let request: director::DirectorTurnRequest = serde_json::from_value(
                args.get("request")
                    .cloned()
                    .ok_or_else(|| "Missing request".to_string())?,
            )
            .map_err(|error| format!("Invalid director request: {error}"))?;
            to_value(director::director_turn(app.clone(), request))
        }
        "director_approve" => {
            let id = arg_string(args, "id")?;
            to_value(director::director_approve(app.clone(), id))
        }
        "elevenlabs_list_voices" => {
            let search = opt_string(args, "search");
            let voice_type = opt_string(args, "voiceType");
            let page_size = args
                .get("pageSize")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
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
        "voice_attach_existing" => {
            let request = args
                .get("request")
                .cloned()
                .ok_or_else(|| "Missing request".to_string())?;
            let request: voice::AttachExistingVoiceRequest = serde_json::from_value(request)
                .map_err(|error| format!("Invalid attach voice request: {error}"))?;
            to_value(voice::voice_attach_existing(request))
        }
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
        "workflow_import" => workflow::cmd_import(args),
        "workflow_status" => workflow::cmd_status(args),
        "workflow_save_character" => workflow::cmd_save_character(args),
        "workflow_draft_intent" => workflow::cmd_draft_intent(app, args),
        "workflow_save_intent" => workflow::cmd_save_intent(args),
        "workflow_approve_intent" => workflow::cmd_approve_intent(args),
        "workflow_set_strategy" => workflow::cmd_set_strategy(args),
        "workflow_save_profile" => workflow::cmd_save_profile(args),
        "workflow_set_budget" => workflow::cmd_set_budget(args),
        "workflow_set_requirement_status" => workflow::cmd_set_requirement_status(args),
        "workflow_add_requirement" => workflow::cmd_add_requirement(args),
        "workflow_backfill_sketches" => workflow::cmd_backfill_sketches(args),
        "workflow_draft_sketches" => workflow::cmd_draft_sketches(app, args),
        "workflow_update_sketch" => workflow::cmd_update_sketch(args),
        "workflow_save_sketch" => workflow::cmd_save_sketch(args),
        "workflow_set_animatic_duration" => workflow::cmd_set_animatic_duration(args),
        "workflow_reorder_animatic" => workflow::cmd_reorder_animatic(args),
        "workflow_reorder_cuts" => workflow::cmd_reorder_cuts(args),
        "workflow_approve_animatic" => workflow::cmd_approve_animatic(args),
        "workflow_attach_scratch_audio" => workflow::cmd_attach_scratch_audio(args),
        "workflow_clear_scratch_audio" => workflow::cmd_clear_scratch_audio(args),
        "workflow_set_shot_trim" => workflow::cmd_set_shot_trim(args),
        "workflow_set_shot_transition" => workflow::cmd_set_shot_transition(args),
        "workflow_clear_package_review" => workflow::cmd_clear_package_review(args),
        "workflow_draft_look" => workflow::cmd_draft_look(app, args),
        "workflow_update_look_slot" => workflow::cmd_update_look_slot(args),
        "workflow_attach_look" => workflow::cmd_attach_look(args),
        "workflow_approve_look" => workflow::cmd_approve_look(args),
        "workflow_reject_look" => workflow::cmd_reject_look(args),
        "workflow_delete_look" => workflow::cmd_delete_look(args),
        "workflow_generate_look" => workflow::cmd_generate_look(app, args),
        "workflow_generate_wardrobe_plate" => workflow::cmd_generate_wardrobe_plate(app, args),
        "workflow_delete_wardrobe_plate" => workflow::cmd_delete_wardrobe_plate(args),
        "workflow_draft_start_frame_prompt" => workflow::cmd_draft_start_frame_prompt(app, args),
        "workflow_save_start_frame_prompt" => workflow::cmd_save_start_frame_prompt(args),
        "workflow_save_production_prompt" => workflow::cmd_save_production_prompt(args),
        "workflow_refresh_production_handoff" => workflow::cmd_refresh_production_handoff(args),
        "workflow_sync_production_notes" => workflow::cmd_sync_production_notes(app, args),
        "workflow_prepare_render" => workflow::cmd_prepare_render(app, args),
        "workflow_set_visual_source" => workflow::cmd_set_visual_source(args),
        "workflow_bind_continuity_frame" => workflow::cmd_bind_continuity_frame(args),
        "workflow_set_take_status" => workflow::cmd_set_take_status(args),
        "workflow_delete_take" => workflow::cmd_delete_take(args),
        "workflow_prepare_assemble" => workflow::cmd_prepare_assemble_with_host(app, args),
        "workflow_add_alternate_shot" => workflow::cmd_add_alternate_shot(args),
        "workflow_mark_cut_version" => workflow::cmd_mark_cut_version(args),
        "workflow_set_cut_version_stage" => workflow::cmd_set_cut_version_stage(args),
        "workflow_lock_picture" => workflow::cmd_lock_picture(args),
        "workflow_set_post_status" => workflow::cmd_set_post_status(args),
        "workflow_set_editorial_path" => workflow::cmd_set_editorial_path(args),
        "workflow_check_deliverable" => workflow::cmd_check_deliverable(args),
        "workflow_generate_start_frame" => workflow::cmd_generate_start_frame(app, args),
        "workflow_edit_start_frame" => workflow::cmd_edit_start_frame(app, args),
        "workflow_attach_start_frame" => workflow::cmd_attach_start_frame(args),
        "workflow_approve_start_frame" => workflow::cmd_approve_start_frame(args),
        "workflow_reject_start_frame" => workflow::cmd_reject_start_frame(args),
        "workflow_delete_start_frame" => workflow::cmd_delete_start_frame(args),
        "director_cancel" => Ok(json!({ "cancelled": true })),
        "ping" => Ok(json!("pong")),
        other => Err(format!("Unknown command: {other}")),
    }
}

fn scoped_path(raw: &str, intent: project_path::PathIntent) -> Result<String, String> {
    session::authorize(raw, intent).map(|path| path.to_string_lossy().into_owned())
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

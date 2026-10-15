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
pub mod voice_spend;
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
            let path = scoped_mutation_path(
                &arg_string(args, "path")?,
                project_path::PathIntent::Write,
                false,
            )?;
            let contents = arg_string(args, "contents")?;
            to_value(fs_ops::write_text_file(path, contents))
        }
        "create_file" => {
            let path = scoped_mutation_path(
                &arg_string(args, "path")?,
                project_path::PathIntent::Write,
                false,
            )?;
            to_value(fs_ops::create_file(path))
        }
        "create_dir" => {
            let path = scoped_mutation_path(
                &arg_string(args, "path")?,
                project_path::PathIntent::Write,
                false,
            )?;
            // Creating a missing Preproduction directory is harmless; creating a
            // directory under either protected file is not.
            to_value(fs_ops::create_dir(path))
        }
        "rename_path" => {
            let from = scoped_mutation_path(
                &arg_string(args, "from")?,
                project_path::PathIntent::Write,
                true,
            )?;
            let to = scoped_mutation_path(
                &arg_string(args, "to")?,
                project_path::PathIntent::Write,
                true,
            )?;
            to_value(fs_ops::rename_path(from, to))
        }
        "copy_path" => {
            let from = scoped_path(&arg_string(args, "from")?, project_path::PathIntent::Read)?;
            let to = scoped_mutation_path(
                &arg_string(args, "to")?,
                project_path::PathIntent::Write,
                true,
            )?;
            fs_ops::reject_workflow_copy_source(std::path::Path::new(&from))?;
            to_value(fs_ops::copy_path(from, to))
        }
        "delete_path" => {
            let path = scoped_mutation_path(
                &arg_string(args, "path")?,
                project_path::PathIntent::Write,
                true,
            )?;
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
        "confirm_spend" => {
            let root = crate::session::command_root(opt_string(args, "projectRoot").as_deref())?;
            let provider = arg_string(args, "provider")?;
            let category = arg_string(args, "category")?;
            let operation = arg_string(args, "operation")?;
            let capability = arg_string(args, "capability")?;
            let model = arg_string(args, "model")?;
            let revision = arg_string(args, "ledgerRevision")?;
            let amount = args
                .get("amountMinor")
                .and_then(Value::as_i64)
                .ok_or("Missing quoted amountMinor")?;
            to_value(crate::workflow::spend::confirm_quoted(
                &root,
                &provider,
                &category,
                &operation,
                &capability,
                &model,
                &revision,
                amount,
            ))
        }
        "confirm_unbounded_spend" => {
            let root = crate::session::command_root(opt_string(args, "projectRoot").as_deref())?;
            let provider = arg_string(args, "provider")?;
            let category = arg_string(args, "category")?;
            let operation = arg_string(args, "operation")?;
            let model = arg_string(args, "model")?;
            let request_hash = arg_string(args, "requestHash")?;
            let revision = arg_string(args, "ledgerRevision")?;
            to_value(crate::workflow::spend::confirm_unbounded_scoped(
                &root,
                &provider,
                &category,
                &operation,
                &model,
                &request_hash,
                &revision,
            ))
        }
        "media_generate_confirmed" => {
            let root = arg_string(args, "projectRoot")?;
            let operation = arg_string(args, "operation")?;
            let token = arg_string(args, "token")?;
            let mut media_args = args
                .get("mediaArgs")
                .filter(|v| v.is_object())
                .cloned()
                .ok_or("Missing mediaArgs")?;
            // Never accept a caller-supplied spend token inside mediaArgs.
            media_args["spend_confirmation"] = json!(token);
            match media::run_media_tool(&operation, &media_args, Some(&root), app) {
                Some(result) => to_value(result),
                None => Err("Unsupported media operation".into()),
            }
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
        "elevenlabs_quote_voice_spend" => to_value(elevenlabs::elevenlabs_quote_voice_spend(
            arg_string(args, "projectRoot")?,
            arg_string(args, "operation")?,
            arg_string(args, "model")?,
            arg_string(args, "input")?,
        )),
        "elevenlabs_confirm_unbounded_voice_spend" => {
            to_value(elevenlabs::elevenlabs_confirm_unbounded_voice_spend(
                arg_string(args, "projectRoot")?,
                arg_string(args, "operation")?,
                arg_string(args, "model")?,
                arg_string(args, "input")?,
                arg_string(args, "requestHash")?,
                arg_string(args, "ledgerRevision")?,
            ))
        }
        "elevenlabs_confirm_voice_spend" => {
            let project_root = arg_string(args, "projectRoot")?;
            let operation = arg_string(args, "operation")?;
            let model = arg_string(args, "model")?;
            let input = arg_string(args, "input")?;
            let ceiling_minor = args
                .get("ceilingMinor")
                .and_then(|v| v.as_i64())
                .ok_or("Positive integer USD cents ceiling required")?;
            let evidence = arg_string(args, "evidence")?;
            let attested = args
                .get("attested")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            to_value(elevenlabs::elevenlabs_confirm_voice_spend(
                project_root,
                operation,
                model,
                input,
                ceiling_minor,
                evidence,
                attested,
            ))
        }
        "elevenlabs_tts_preview" => {
            let voice_id = arg_string(args, "voiceId")?;
            let text = arg_string(args, "text")?;
            to_value(elevenlabs::elevenlabs_tts_preview(
                app.clone(),
                voice_id,
                text,
                arg_string(args, "projectRoot")?,
                arg_string(args, "confirmation")?,
            ))
        }
        "elevenlabs_design_voice" => {
            let voice_description = arg_string(args, "voiceDescription")?;
            let text = arg_string(args, "text")?;
            to_value(elevenlabs::elevenlabs_design_voice(
                app.clone(),
                voice_description,
                text,
                arg_string(args, "projectRoot")?,
                arg_string(args, "confirmation")?,
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
                arg_string(args, "projectRoot")?,
                arg_string(args, "confirmation")?,
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
        "workflow_unsettled_spend" => workflow::cmd_unsettled_spend(args),
        "workflow_reconcile_spend" => workflow::cmd_reconcile_spend(args),
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

// Check the spelling before authorization resolves links. In particular, an
// existing workflow_state.json symlink must not be rewritten via its target.
fn scoped_mutation_path(
    raw: &str,
    intent: project_path::PathIntent,
    recursive: bool,
) -> Result<String, String> {
    if let Some(root) = session::root() {
        let path = std::path::Path::new(raw);
        let original = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        fs_ops::reject_workflow_mutation(&original, recursive)?;
    }
    let path = scoped_path(raw, intent)?;
    fs_ops::reject_workflow_mutation(std::path::Path::new(&path), recursive)?;
    Ok(path)
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

#[cfg(test)]
mod protected_workflow_tests {
    use super::*;
    use std::{fs, path::Path};

    fn command(
        app: &HostCtx,
        recorder: &RecorderState,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        dispatch(app, recorder, name, &args)
    }

    #[test]
    fn generic_commands_cannot_bypass_workflow_revision_or_reservation() {
        let _session = session::test_session_lock();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join("Preproduction")).unwrap();
        let app = HostCtx::new(dir.path().join("settings"));
        let recorder = RecorderState::new();
        session::open(root.to_str().unwrap()).unwrap();
        let state = root.join(workflow::state::STATE_REL);
        let lock = root.join("Preproduction/.media-spend.lock");
        let state_path = state.to_string_lossy().into_owned();
        let lock_path = lock.to_string_lossy().into_owned();
        let call = |name: &str, args: Value| command(&app, &recorder, name, args);
        let check = |name: &str, args: Value| {
            let error = match call(name, args.clone()) {
                Err(error) => error,
                Ok(value) => panic!("{name} unexpectedly succeeded: {args} => {value}"),
            };
            assert!(
                error.contains("workflow state")
                    || error.contains("outside")
                    || error.contains("symlink"),
                "{name}: {error}"
            );
        };
        // Missing state and lock names must be reserved too.
        for path in [&state_path, &lock_path] {
            check("write_text_file", json!({"path":path,"contents":"{}"}));
            check("create_file", json!({"path":path}));
            check("create_dir", json!({"path":path}));
        }
        let mut initial = workflow::state::ProjectState::new();
        initial.budget = Some(workflow::state::ProjectBudget {
            currency: "USD".into(),
            limit_minor: Some(100),
            warning_at_minor: None,
            hard_stop: true,
            policy: None,
        });
        workflow::import::persist_state(&root, &initial).unwrap();
        let original = fs::read(&state).unwrap();
        let alternate = root.join("Scripts/ordinary.txt");
        fs::create_dir_all(alternate.parent().unwrap()).unwrap();
        fs::write(&alternate, "safe").unwrap();
        for path in [&state_path, &lock_path] {
            check(
                "write_text_file",
                json!({"path":path,"contents":"tampered"}),
            );
            check("delete_path", json!({"path":path}));
            check("rename_path", json!({"from":path,"to":root.join("stolen")}));
            check("rename_path", json!({"from":alternate,"to":path}));
            check("copy_path", json!({"from":alternate,"to":path}));
            check("copy_path", json!({"from":path,"to":root.join("stolen")}));
        }
        for ancestor in [&root, &root.join("Preproduction")] {
            check("delete_path", json!({"path":ancestor}));
            check(
                "rename_path",
                json!({"from":ancestor,"to":root.join("moved")}),
            );
            check(
                "copy_path",
                json!({"from":ancestor,"to":root.join("copied")}),
            );
            check("rename_path", json!({"from":alternate,"to":ancestor}));
            check("copy_path", json!({"from":alternate,"to":ancestor}));
        }
        check(
            "write_text_file",
            json!({"path":root.join("Scripts/../Preproduction/workflow_state.json"),"contents":"bad"}),
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(root.join("Preproduction"), root.join("alias")).unwrap();
            symlink(&state, root.join("state-alias.json")).unwrap();
            check(
                "write_text_file",
                json!({"path":root.join("alias/workflow_state.json"),"contents":"bad"}),
            );
            check("delete_path", json!({"path":root.join("alias")}));
            check(
                "copy_path",
                json!({"from":root.join("state-alias.json"),"to":root.join("copied.json")}),
            );
            call(
                "copy_path",
                json!({"from":root.join("Scripts"),"to":root.join("copy-scripts")}),
            )
            .unwrap();
        }
        // A mock provider is invoked only after the reservation; rejected generic
        // writes must not change the running job, revision or active lock.
        workflow::spend::run_scoped_with_request(
            &root,
            "atlas-cloud",
            "media",
            "atlas_generate_image",
            "image.text_to_image",
            "model",
            None,
            None,
            None,
            Some(workflow::spend::VerifiedMaximum::synthetic_for_test(
                "USD", 7,
            )),
            || {
                let reserved_bytes = fs::read(&state).unwrap();
                let before = workflow::state::ProjectState::from_json(
                    &String::from_utf8(reserved_bytes.clone()).unwrap(),
                )
                .unwrap();
                assert_eq!(before.jobs.len(), 1);
                for path in [&state_path, &lock_path] {
                    check("write_text_file", json!({"path":path,"contents":"{}"}));
                    check("delete_path", json!({"path":path}));
                }
                check("delete_path", json!({"path":root.join("Preproduction")}));
                let after =
                    workflow::state::ProjectState::from_json(&fs::read_to_string(&state).unwrap())
                        .unwrap();
                assert_eq!(before.state_revision, after.state_revision);
                assert_eq!(before.jobs.len(), after.jobs.len());
                assert_eq!(fs::read(&state).unwrap(), reserved_bytes);
                Ok("mock sent".into())
            },
        )
        .unwrap();
        assert_ne!(fs::read(&state).unwrap(), original);
        assert_eq!(workflow::import::load_state(&root).unwrap().jobs.len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let backup = root.join("Scripts/state-backup.json");
            fs::rename(&state, &backup).unwrap();
            symlink(&alternate, &state).unwrap();
            check("write_text_file", json!({"path":state,"contents":"bad"}));
            check("delete_path", json!({"path":state}));
            assert_eq!(fs::read_to_string(&alternate).unwrap(), "safe");
            fs::remove_file(&state).unwrap();
            fs::rename(backup, &state).unwrap();
        }
        // Other paths, including files in Preproduction, remain writable.
        for path in [
            root.join("Preproduction/notes.txt"),
            root.join("Scripts/notes.txt"),
        ] {
            call("write_text_file", json!({"path":path,"contents":"ok"})).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "ok");
        }
        assert!(Path::new(&lock_path).exists());
        session::close();
    }
}

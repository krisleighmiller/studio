//! Standalone runner reusing the native TTS adapter, approval and spend gates.
use super::*;
pub(super) fn run(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
) -> Result<SpecialistResult, String> {
    let epoch = crate::pending::generation();
    let root = Path::new(project_root.ok_or("Speech needs an open project")?);
    let output_audio = crate::workflow::dialogue_audio::validate(root, &validated.action)?;
    let inputs = &validated.action.inputs;
    if let Some(prediction) = input_str(inputs, &["collect_prediction_id"]) {
        let result = media::run_media_tool(
            "atlas_collect_audio",
            &json!({"prediction_id":prediction,"path":output_audio,"wait_seconds":240}),
            project_root,
            app,
        )
        .ok_or("Audio collection unavailable")??;
        require_media_saved(&result, &output_audio.to_string_lossy())?;
        return Ok(SpecialistResult::done("Speech collection completed; audition the Draft WAV in Scene Look. No video was requested."));
    }
    if output_audio.exists() {
        return Err("Speech output is occupied; existing media preserved".into());
    }
    let lines = inputs
        .get("dialogue")
        .and_then(Value::as_array)
        .ok_or("Missing current dialogue")?;
    let cue_text = lines
        .iter()
        .filter_map(|l| l.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    let delivery = String::new();
    let clone_wav = input_str(inputs, &["voice_clone"]).map(str::to_owned);
    let profile_yaml = input_str(inputs, &["voice_profile"]).map(str::to_owned);
    if clone_wav.is_some() {
        let clone_path = resolve_under_project(clone_wav.as_deref().unwrap(), project_root)
            .map_err(|e| e.to_string())?;
        if !clone_path.is_file() {
            return Err(format!(
                "voice_clone WAV not found: {}",
                clone_path.display()
            ));
        }
        let audio_model = capability_model(context, "audio.tts_clone").ok_or_else(|| {
            "Set Settings → Audio → TTS (voice clone) for text_sync (e.g. bytedance/seed-audio-1.0)."
                .to_string()
        })?;
        app.emit(
            "director-progress",
            format!("Uploading clone WAV {}…", clone_path.display()),
        );
        let clone_url = upload_local(
            app,
            project_root,
            &clone_path.to_string_lossy(),
            crate::media_util::MediaRole::Audio,
        )?;
        let tts_text = build_seed_tts_text(&cue_text, &delivery);
        let params = json!({
            "text": tts_text,
            "prompt": tts_text,
            "references": [{ "audio_url": clone_url }],
            "audio_url": clone_url,
            "reference_audio": clone_url,
        });
        app.emit(
            "director-progress",
            format!(
                "Generating performance TTS with `{}` → {}…",
                audio_model.model_id,
                output_audio.display()
            ),
        );
        let api_id = if audio_model.api_id.trim().is_empty() {
            None
        } else {
            Some(audio_model.api_id.as_str())
        };
        let result = run_media_generate(
            app,
            project_root,
            "audio",
            &audio_model.model_id,
            api_id,
            params,
            &output_audio.to_string_lossy(),
        )?;
        require_media_saved(&result, &output_audio.to_string_lossy())?;
    } else {
        let profile_path = resolve_under_project(profile_yaml.as_deref().unwrap(), project_root)
            .map_err(|e| e.to_string())?;
        if !profile_path.is_file() {
            return Err(format!(
                "voice_profile YAML not found: {}",
                profile_path.display()
            ));
        }
        let (profile_model, voice_id, language, settings) =
            load_voice_profile_fields(&profile_path)?;
        let audio_model = capability_model(context, "audio.tts_profile")
            .ok_or("Set Settings → Audio → TTS (profile) before generating dialogue")?;
        if !profile_model.trim().is_empty() && profile_model != audio_model.model_id {
            return Err("The saved voice profile model differs from the approved Settings audio model. Choose matching settings and prepare fresh audio approval.".into());
        }

        let mut params = json!({ "text": cue_text });
        if !voice_id.is_empty() {
            params["voice"] = json!(voice_id);
            params["voice_id"] = json!(voice_id);
        }
        if !language.is_empty() {
            params["language"] = json!(language);
        }
        for (k, v) in settings {
            if !params.as_object().unwrap().contains_key(&k) {
                params[k] = v;
            }
        }
        if !delivery.is_empty() {
            params["prompt"] = json!(format!("{delivery}. Speak exactly: {cue_text}"));
        }
        app.emit(
            "director-progress",
            format!(
                "Generating profile TTS with `{}` → {}…",
                audio_model.model_id,
                output_audio.display()
            ),
        );
        let api_id = if audio_model.api_id.trim().is_empty() {
            None
        } else {
            Some(audio_model.api_id.as_str())
        };
        let result = run_media_generate(
            app,
            project_root,
            "audio",
            &audio_model.model_id,
            api_id,
            params,
            &output_audio.to_string_lossy(),
        )?;
        require_media_saved(&result, &output_audio.to_string_lossy())?;
    }
    crate::workflow::dialogue_audio::settle_guarded(root, &validated.action, &output_audio, || {
        if app.director_cancelled()
            || crate::session::root().as_deref() != Some(root)
            || crate::pending::generation() != epoch
        {
            return Err("Speech admission was stopped, superseded or its project changed; saved file retained".into());
        }
        if let Some(parent) = &app.workflow_parent { parent.valid()?; }
        Ok(())
    })?;
    Ok(SpecialistResult::done("Dialogue audio saved as Draft. Audition and select it in Scene Look; video requires separate preparation and approval."))
}

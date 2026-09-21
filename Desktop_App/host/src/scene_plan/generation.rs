//! What the generation route needs from a validated handoff. Replaces the
//! `yaml.get("target_duration_seconds")` JSON poking in the specialist.
use crate::scene_plan::extract::load_handoff_document;
use crate::scene_plan::handoff::{
    AssetRole, CastBinding, CutId, RenderLane, SceneHandoff, SpeechMode,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct RenderRequest {
    pub id: CutId,
    pub lane: RenderLane,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub aspect_ratio: String,
    /// Provider bucket chosen by the host (whole seconds).
    pub duration_seconds: u64,
    pub start_frame: Option<String>,
    pub character_reference: Option<String>,
    /// All reference image paths in handoff order (start frame first if any).
    pub reference_images: Vec<String>,
    pub speech_mode: SpeechMode,
    pub cast: Vec<CastBinding>,
}

impl RenderRequest {
    pub fn from_handoff(h: &SceneHandoff) -> Result<Self, String> {
        let derived = h.derived.as_ref().ok_or_else(|| {
            format!(
                "{} has no host-derived timing; it was never validated",
                h.id()
            )
        })?;
        if !derived.render_duration_seconds.is_finite() || derived.render_duration_seconds <= 0.0 {
            return Err(format!(
                "{} has an unusable render duration {}",
                h.id(),
                derived.render_duration_seconds
            ));
        }
        let start_frame = h
            .asset_with_role(AssetRole::StartFrame)
            .map(|a| a.path.clone());
        let character_reference = h
            .asset_with_role(AssetRole::CharacterReference)
            .map(|a| a.path.clone());
        let mut reference_images: Vec<String> = Vec::new();
        if let Some(s) = &start_frame {
            reference_images.push(s.clone());
        }
        for a in &h.reference_assets {
            if !reference_images.contains(&a.path) {
                reference_images.push(a.path.clone());
            }
        }
        Ok(RenderRequest {
            id: h.id(),
            lane: derived.render_lane,
            prompt: h.prompt.clone(),
            negative_prompt: h.negative_prompt.clone().filter(|n| !n.trim().is_empty()),
            aspect_ratio: h.generation_requirements.aspect_ratio.clone(),
            duration_seconds: derived.render_duration_seconds.round() as u64,
            start_frame,
            character_reference,
            reference_images,
            speech_mode: h.speech.mode.unwrap_or(SpeechMode::Silent),
            cast: h.cast.clone(),
        })
    }

    pub fn from_document(text: &str) -> Result<Self, String> {
        RenderRequest::from_handoff(&load_handoff_document(text)?)
    }

    /// Base provider params; the media router remaps aliases per model.
    pub fn base_params(&self) -> Value {
        let mut params = json!({ "prompt": self.prompt, "duration": self.duration_seconds, "aspect_ratio": self.aspect_ratio });
        if let Some(n) = &self.negative_prompt {
            params["negative_prompt"] = json!(n);
        }
        params
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::config::HostConfig;
    use crate::scene_plan::extract::parse_handoff_yaml;
    use crate::scene_plan::fixtures::{lipsync_scene_yaml, SAM_REF};
    use crate::scene_plan::render::sidecar_document;
    use crate::scene_plan::validate::validate_scene;

    #[test]
    fn render_request_comes_from_derived_timing_not_the_model() {
        let h = parse_handoff_yaml(&lipsync_scene_yaml(
            2,
            "This is a twelve word sentence that needs a reliable duration estimate.",
        ))
        .unwrap();
        assert!(RenderRequest::from_handoff(&h)
            .unwrap_err()
            .contains("never validated"));
        let v = validate_scene(h, &HostConfig::default(), None, None).unwrap();
        let req =
            RenderRequest::from_document(&sidecar_document(&v.handoff, &v.canonical_yaml)).unwrap();
        assert_eq!(req.id, CutId::new(1, 2));
        assert_eq!(req.lane, RenderLane::AudioDrivenI2v);
        assert_eq!(req.duration_seconds, 10);
        assert_eq!(req.aspect_ratio, "9:16");
        assert_eq!(req.character_reference.as_deref(), Some(SAM_REF));
        assert_eq!(req.reference_images, vec![SAM_REF.to_string()]);
        assert_eq!(req.base_params()["duration"], 10);
        assert_eq!(req.base_params()["aspect_ratio"], "9:16");
    }
}

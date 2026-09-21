use crate::scene_plan::handoff::RenderLane;
use crate::scene_plan::timing::{round2, DEFAULT_SPEAKING_RATE_WPM};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenderDurationPolicy {
    Buckets {
        seconds: Vec<f64>,
    },
    Continuous {
        min_seconds: f64,
        max_seconds: f64,
        step_seconds: f64,
    },
}

impl RenderDurationPolicy {
    pub fn buckets(seconds: &[f64]) -> Self {
        RenderDurationPolicy::Buckets {
            seconds: seconds.to_vec(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            RenderDurationPolicy::Buckets { seconds } => {
                if seconds.is_empty() {
                    return Err("render buckets must not be empty".into());
                }
                if seconds.iter().any(|s| !s.is_finite() || *s <= 0.0) {
                    return Err(format!(
                        "render buckets must be finite and > 0, got {seconds:?}"
                    ));
                }
                Ok(())
            }
            RenderDurationPolicy::Continuous {
                min_seconds,
                max_seconds,
                step_seconds,
            } => {
                if !(min_seconds.is_finite() && *min_seconds > 0.0) {
                    return Err(format!(
                        "continuous min_seconds must be > 0, got {min_seconds}"
                    ));
                }
                if !(max_seconds.is_finite() && max_seconds >= min_seconds) {
                    return Err(format!(
                        "continuous max_seconds must be >= min_seconds, got {max_seconds}"
                    ));
                }
                if !(step_seconds.is_finite() && *step_seconds > 0.0) {
                    return Err(format!(
                        "continuous step_seconds must be > 0, got {step_seconds}"
                    ));
                }
                Ok(())
            }
        }
    }

    pub fn choose(&self, minimum_source_seconds: f64) -> Result<f64, String> {
        match self {
            RenderDurationPolicy::Buckets { seconds } => {
                let mut sorted = seconds.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                sorted
                    .iter()
                    .copied()
                    .find(|b| *b + 1e-9 >= minimum_source_seconds)
                    .ok_or_else(|| {
                    format!(
                        "minimum source duration {:.2}s exceeds the largest provider render bucket {:.2}s (buckets: {:?})",
                        minimum_source_seconds,
                        sorted.last().copied().unwrap_or(0.0),
                        sorted
                    )
                })
            }
            RenderDurationPolicy::Continuous {
                min_seconds,
                max_seconds,
                step_seconds,
            } => {
                let chosen = if minimum_source_seconds <= *min_seconds {
                    *min_seconds
                } else {
                    let steps =
                        ((minimum_source_seconds - min_seconds) / step_seconds - 1e-9).ceil();
                    min_seconds + steps * step_seconds
                };
                if chosen > max_seconds + 1e-9 {
                    return Err(format!(
                        "minimum source duration {:.2}s exceeds the provider maximum render duration {:.2}s",
                        minimum_source_seconds, max_seconds
                    ));
                }
                Ok(round2(chosen))
            }
        }
    }

    /// The next larger legal duration after `current`, if the lane has one.
    pub fn next_above(&self, current: f64) -> Option<f64> {
        match self {
            RenderDurationPolicy::Buckets { seconds } => {
                let mut sorted = seconds.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                sorted.iter().copied().find(|b| *b > current + 1e-9)
            }
            RenderDurationPolicy::Continuous {
                max_seconds,
                step_seconds,
                ..
            } => {
                let next = round2(current + step_seconds);
                (next <= max_seconds + 1e-9).then_some(next)
            }
        }
    }
}

/// Duration buckets for a model family, matched by case-insensitive substring
/// of the model id. First match wins. These are *defaults*: verify against
/// the provider you actually route to and override in config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelBucketRule {
    pub model_contains: String,
    pub seconds: Vec<f64>,
}

fn rule(model_contains: &str, seconds: &[f64]) -> ModelBucketRule {
    ModelBucketRule {
        model_contains: model_contains.to_string(),
        seconds: seconds.to_vec(),
    }
}

pub fn default_model_bucket_rules() -> Vec<ModelBucketRule> {
    vec![
        rule("kling", &[5.0, 10.0]),
        rule("wan", &[5.0, 10.0]),
        rule("veo", &[4.0, 6.0, 8.0]),
        rule("seedance", &[5.0, 10.0]),
        rule("hailuo", &[6.0, 10.0]),
        rule("minimax", &[6.0, 10.0]),
        rule("luma", &[5.0, 9.0]),
        rule("ray-", &[5.0, 9.0]),
        rule("runway", &[5.0, 10.0]),
        rule("gen4", &[5.0, 10.0]),
        rule("gen3", &[5.0, 10.0]),
        rule("sora", &[4.0, 8.0, 12.0]),
        rule("pixverse", &[5.0, 8.0]),
        rule("vidu", &[4.0, 8.0]),
    ]
}

/// Model ids the host resolved per lane (from the Director's capability settings).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LaneModels {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_to_video: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_to_video: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_to_video: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_driven_i2v: Option<String>,
}

impl LaneModels {
    pub fn is_empty(&self) -> bool {
        self.text_to_video.is_none()
            && self.image_to_video.is_none()
            && self.reference_to_video.is_none()
            && self.audio_driven_i2v.is_none()
    }
    pub fn get(&self, lane: RenderLane) -> Option<&str> {
        match lane {
            RenderLane::TextToVideo => self.text_to_video.as_deref(),
            RenderLane::ImageToVideo => self.image_to_video.as_deref(),
            RenderLane::ReferenceToVideo => self.reference_to_video.as_deref(),
            RenderLane::AudioDrivenI2v => self.audio_driven_i2v.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostConfig {
    #[serde(default = "default_wpm")]
    pub speaking_rate_wpm: f64,
    #[serde(default = "default_buckets")]
    pub audio_driven_render: RenderDurationPolicy,
    #[serde(default = "default_buckets")]
    pub text_to_video_render: RenderDurationPolicy,
    #[serde(default = "default_buckets")]
    pub image_to_video_render: RenderDurationPolicy,
    #[serde(default = "default_buckets")]
    pub reference_to_video_render: RenderDurationPolicy,
    #[serde(default = "default_model_bucket_rules")]
    pub model_buckets: Vec<ModelBucketRule>,
    #[serde(default, skip_serializing_if = "LaneModels::is_empty")]
    pub lane_models: LaneModels,
    #[serde(default = "default_head_handle")]
    pub head_handle_seconds: f64,
    #[serde(default = "default_tail_handle")]
    pub tail_handle_seconds: f64,
    #[serde(default = "default_language")]
    pub default_language: String,
}

fn default_wpm() -> f64 {
    DEFAULT_SPEAKING_RATE_WPM
}
fn default_buckets() -> RenderDurationPolicy {
    RenderDurationPolicy::buckets(&[5.0, 10.0])
}
fn default_head_handle() -> f64 {
    0.5
}
fn default_tail_handle() -> f64 {
    0.5
}
fn default_language() -> String {
    "en".into()
}

impl Default for HostConfig {
    fn default() -> Self {
        HostConfig {
            speaking_rate_wpm: default_wpm(),
            audio_driven_render: default_buckets(),
            text_to_video_render: default_buckets(),
            image_to_video_render: default_buckets(),
            reference_to_video_render: default_buckets(),
            model_buckets: default_model_bucket_rules(),
            lane_models: LaneModels::default(),
            head_handle_seconds: default_head_handle(),
            tail_handle_seconds: default_tail_handle(),
            default_language: default_language(),
        }
    }
}

impl HostConfig {
    pub fn choose_render_duration(
        &self,
        lane: RenderLane,
        minimum_source_seconds: f64,
    ) -> Result<f64, String> {
        self.policy_for(lane).choose(minimum_source_seconds)
    }

    pub fn policy_for(&self, lane: RenderLane) -> &RenderDurationPolicy {
        match lane {
            RenderLane::TextToVideo => &self.text_to_video_render,
            RenderLane::ImageToVideo => &self.image_to_video_render,
            RenderLane::ReferenceToVideo => &self.reference_to_video_render,
            RenderLane::AudioDrivenI2v => &self.audio_driven_render,
        }
    }

    fn policy_mut(&mut self, lane: RenderLane) -> &mut RenderDurationPolicy {
        match lane {
            RenderLane::TextToVideo => &mut self.text_to_video_render,
            RenderLane::ImageToVideo => &mut self.image_to_video_render,
            RenderLane::ReferenceToVideo => &mut self.reference_to_video_render,
            RenderLane::AudioDrivenI2v => &mut self.audio_driven_render,
        }
    }

    /// Buckets for a concrete model id, if a rule matches.
    pub fn policy_for_model(&self, model_id: &str) -> Option<RenderDurationPolicy> {
        let needle = model_id.to_ascii_lowercase();
        self.model_buckets
            .iter()
            .find(|r| {
                !r.model_contains.is_empty()
                    && needle.contains(&r.model_contains.to_ascii_lowercase())
            })
            .map(|r| RenderDurationPolicy::buckets(&r.seconds))
    }

    /// Apply per-lane model ids: lanes whose model matches a bucket rule get
    /// that model's buckets; unknown models keep the lane default.
    pub fn with_lane_models(mut self, models: &LaneModels) -> Self {
        for lane in [
            RenderLane::TextToVideo,
            RenderLane::ImageToVideo,
            RenderLane::ReferenceToVideo,
            RenderLane::AudioDrivenI2v,
        ] {
            if let Some(model) = models.get(lane) {
                if let Some(policy) = self.policy_for_model(model) {
                    *self.policy_mut(lane) = policy;
                }
            }
        }
        self.lane_models = models.clone();
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.speaking_rate_wpm.is_finite() && self.speaking_rate_wpm > 0.0) {
            return Err(format!(
                "speaking_rate_wpm must be > 0, got {}",
                self.speaking_rate_wpm
            ));
        }
        self.audio_driven_render
            .validate()
            .map_err(|e| format!("audio_driven_render: {e}"))?;
        self.text_to_video_render
            .validate()
            .map_err(|e| format!("text_to_video_render: {e}"))?;
        self.image_to_video_render
            .validate()
            .map_err(|e| format!("image_to_video_render: {e}"))?;
        self.reference_to_video_render
            .validate()
            .map_err(|e| format!("reference_to_video_render: {e}"))?;
        for r in &self.model_buckets {
            RenderDurationPolicy::buckets(&r.seconds)
                .validate()
                .map_err(|e| format!("model_buckets[{}]: {e}", r.model_contains))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_pick_smallest_fitting_bucket() {
        let p = RenderDurationPolicy::buckets(&[10.0, 5.0]);
        assert_eq!(p.choose(3.0).unwrap(), 5.0);
        assert_eq!(p.choose(8.0).unwrap(), 10.0);
        assert_eq!(p.next_above(5.0), Some(10.0));
        assert_eq!(p.next_above(10.0), None);
        assert!(p
            .choose(10.01)
            .unwrap_err()
            .contains("exceeds the largest provider render bucket 10.00s"));
    }

    #[test]
    fn continuous_still_supported_for_explicit_config() {
        let p = RenderDurationPolicy::Continuous {
            min_seconds: 2.0,
            max_seconds: 15.0,
            step_seconds: 1.0,
        };
        assert_eq!(p.choose(8.2).unwrap(), 9.0);
        assert!(p.choose(15.5).is_err());
    }

    #[test]
    fn defaults_are_5_and_10_on_every_lane() {
        let c = HostConfig::default();
        c.validate().unwrap();
        for lane in [
            RenderLane::TextToVideo,
            RenderLane::ImageToVideo,
            RenderLane::ReferenceToVideo,
            RenderLane::AudioDrivenI2v,
        ] {
            assert_eq!(
                c.policy_for(lane),
                &RenderDurationPolicy::buckets(&[5.0, 10.0]),
                "{lane:?}"
            );
        }
        // 10 + 1 + 1 = 12 used to become a 12 s render; now it is rejected.
        assert!(c.policy_for(RenderLane::TextToVideo).choose(12.0).is_err());
    }

    #[test]
    fn lane_models_apply_known_buckets_and_leave_unknown_alone() {
        let models = LaneModels {
            text_to_video: Some("google/veo-3.1".into()),
            image_to_video: Some("Wan-AI/wan2.2-i2v".into()),
            reference_to_video: Some("vendor/mystery-model".into()),
            audio_driven_i2v: Some("alibaba/wan-2.5-s2v".into()),
        };
        let c = HostConfig::default().with_lane_models(&models);
        assert_eq!(
            c.policy_for(RenderLane::TextToVideo),
            &RenderDurationPolicy::buckets(&[4.0, 6.0, 8.0])
        );
        assert_eq!(
            c.policy_for(RenderLane::TextToVideo).choose(7.0).unwrap(),
            8.0
        );
        assert_eq!(
            c.policy_for(RenderLane::ImageToVideo),
            &RenderDurationPolicy::buckets(&[5.0, 10.0])
        );
        assert_eq!(
            c.policy_for(RenderLane::ReferenceToVideo),
            &RenderDurationPolicy::buckets(&[5.0, 10.0])
        );
        assert_eq!(
            c.lane_models.get(RenderLane::AudioDrivenI2v),
            Some("alibaba/wan-2.5-s2v")
        );
        assert!(
            c.policy_for_model("openai/sora-2")
                .unwrap()
                .choose(9.0)
                .unwrap()
                == 12.0
        );
        assert!(c.policy_for_model("nothing-known").is_none());
    }

    #[test]
    fn invalid_config_is_rejected_and_json_round_trips() {
        let mut c = HostConfig::default();
        c.reference_to_video_render = RenderDurationPolicy::buckets(&[]);
        assert!(c
            .validate()
            .unwrap_err()
            .contains("reference_to_video_render"));
        let c: HostConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c, HostConfig::default());
        let c: HostConfig = serde_json::from_str(
            r#"{"text_to_video_render":{"kind":"buckets","seconds":[4.0,8.0]}}"#,
        )
        .unwrap();
        assert_eq!(
            c.policy_for(RenderLane::TextToVideo).choose(5.0).unwrap(),
            8.0
        );
    }
}

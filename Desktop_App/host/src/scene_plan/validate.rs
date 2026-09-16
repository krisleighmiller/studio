//! Cross-field rules. Every rule lives here, not in prose.
use crate::scene_plan::assets::AssetRegistry;
use crate::scene_plan::config::HostConfig;
use crate::scene_plan::error::{Issue, IssueKind};
use crate::scene_plan::handoff::{
    label, AssetRole, CastBinding, CutId, DerivedTiming, GenerationPath, RenderLane, SceneHandoff,
    SourceMode, SpeechMode,
};
use crate::scene_plan::render::canonical_yaml;
use crate::scene_plan::timing::{cue_timing, normalized_words, round2, CueTiming, SceneTiming};
use regex::Regex;
use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::OnceLock;

fn aspect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{1,4}:\d{1,4}$").unwrap())
}

fn cue_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^S(\d+)C(\d+)_[A-Za-z0-9]+$").unwrap())
}

/// Cut id encoded in a cue id, if the id is well-formed (`S01C02_a`).
pub fn parse_cue_id(id: &str) -> Option<CutId> {
    let caps = cue_id_re().captures(id.trim())?;
    Some(CutId::new(caps[1].parse().ok()?, caps[2].parse().ok()?))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedScene {
    pub handoff: SceneHandoff,
    pub canonical_yaml: String,
    pub timing: SceneTiming,
    pub spoken_words: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidatedScene {
    pub fn id(&self) -> CutId {
        self.handoff.id()
    }
}

pub fn validate_scene(
    mut handoff: SceneHandoff,
    config: &HostConfig,
    expected_aspect_ratio: Option<&str>,
    registry: Option<&AssetRegistry>,
) -> Result<ValidatedScene, Vec<Issue>> {
    let id = handoff.id();
    let wpm = config.speaking_rate_wpm;
    let mut issues: Vec<Issue> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    macro_rules! fail {
        ($($arg:tt)*) => { issues.push(Issue::new(IssueKind::Validation, Some(id), format!($($arg)*))) };
    }
    macro_rules! not_ready {
        ($($arg:tt)*) => { issues.push(Issue::new(IssueKind::NotReady, Some(id), format!($($arg)*))) };
    }

    // --- identity / text -------------------------------------------------
    if handoff.source_scene == 0 {
        fail!("source_scene must be 1 or greater");
    }
    if handoff.cut == 0 {
        fail!("cut must be 1 or greater");
    }
    if handoff.provider.trim().is_empty() {
        fail!("provider must not be empty");
    }
    if handoff.prompt.trim().is_empty() {
        fail!("prompt must not be empty");
    }
    if handoff.continuity.group.trim().is_empty() {
        fail!("continuity.group must not be empty; use `standalone` for a cut that stands alone");
    }

    // --- editing durations ------------------------------------------------
    let screen = handoff.editing.estimated_screen_duration_seconds;
    let head = handoff.editing.head_handle_seconds;
    let tail = handoff.editing.tail_handle_seconds;
    if !(screen.is_finite() && screen > 0.0) {
        fail!("editing.estimated_screen_duration_seconds must be a finite number greater than 0, got {screen}");
    }
    for (name, value) in [("head_handle_seconds", head), ("tail_handle_seconds", tail)] {
        if !(value.is_finite() && value >= 0.0) {
            fail!("editing.{name} must be a finite number >= 0, got {value}");
        }
    }

    // --- aspect ratio -----------------------------------------------------
    let aspect = handoff.generation_requirements.aspect_ratio.trim();
    if !aspect_re().is_match(aspect) {
        fail!("generation_requirements.aspect_ratio must look like `9:16`, got `{aspect}`");
    } else if let Some(expected) = expected_aspect_ratio {
        if expected.trim() != aspect {
            fail!(
                "generation_requirements.aspect_ratio is `{aspect}` but this production is `{}`",
                expected.trim()
            );
        }
    }

    // --- speech -----------------------------------------------------------
    let mode = handoff.speech.mode;
    let cue_count = handoff.speech.cues.len();
    match (mode, cue_count) {
        (None, n) if n > 0 => not_ready!("speech.mode is missing but {n} cue(s) are present; set speech.mode to none, voice_over or source_video_lipsync"),
        (Some(SpeechMode::Silent), n) if n > 0 => fail!("speech.mode is none but {n} cue(s) are present; remove the cues or choose voice_over / source_video_lipsync"),
        (Some(m), 0) if m != SpeechMode::Silent => fail!("speech.mode is {} but speech.cues is empty", label(&m)),
        _ => {}
    }

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut cue_timings: Vec<CueTiming> = Vec::with_capacity(cue_count);
    let mut spoken_words: Vec<String> = Vec::new();
    let mut on_camera_speakers: BTreeSet<String> = BTreeSet::new();

    for (index, cue) in handoff.speech.cues.iter().enumerate() {
        let label_ = if cue.id.trim().is_empty() {
            format!("speech.cues[{index}]")
        } else {
            format!("cue `{}`", cue.id.trim())
        };
        match parse_cue_id(&cue.id) {
            Some(cid) if cid == id => {}
            Some(cid) => fail!("{label_} is numbered {} but sits in {} (cue ids must start with {})", cid.code(), id.code(), id.cue_prefix()),
            None => fail!("{label_} has a malformed id; expected {}<letters or digits> (source scene + cut + suffix)", id.cue_prefix()),
        }
        if !seen_ids.insert(cue.id.trim().to_string()) {
            fail!("{label_} has a duplicate id");
        }
        if cue.speaker.trim().is_empty() {
            fail!("{label_} has an empty speaker");
        }
        let words = normalized_words(&cue.text);
        if words.is_empty() {
            fail!("{label_} has no speakable words in `text`");
        }
        for (name, value) in [
            ("pause_before_seconds", cue.pause_before_seconds),
            ("pause_after_seconds", cue.pause_after_seconds),
        ] {
            if !(value.is_finite() && value >= 0.0) {
                fail!("{label_}.{name} must be a finite number >= 0, got {value}");
            }
        }
        let timing = cue_timing(cue, wpm);
        if let Some(est) = cue.estimated_duration_seconds {
            if !(est.is_finite() && est >= 0.0) {
                fail!("{label_}.estimated_duration_seconds must be a finite number >= 0 if present, got {est}");
            } else if (est - timing.speech_seconds).abs()
                > 0.5_f64.max(timing.speech_seconds * 0.25)
            {
                warnings.push(format!("{id} {label_}: model estimated {est:.2}s but {} words at {wpm} WPM take {:.2}s; host value used", timing.word_count, timing.speech_seconds));
            }
        }
        if cue.on_camera {
            on_camera_speakers.insert(cue.speaker.trim().to_string());
        }
        spoken_words.extend(words);
        cue_timings.push(timing);
    }

    let speech_total = round2(
        cue_timings
            .iter()
            .map(|t| t.total_seconds)
            .fold(0.0, |a, b| a + b),
    );
    let pauses_total: f64 = round2(
        cue_timings
            .iter()
            .map(|t| t.pause_seconds)
            .fold(0.0, |a, b| a + b),
    );
    let words_total: usize = cue_timings.iter().map(|t| t.word_count).sum();
    if screen.is_finite() && speech_total > screen + 1e-6 {
        fail!("speech needs {speech_total:.2}s ({words_total} words at {wpm} WPM plus {pauses_total:.2}s of pauses) but editing.estimated_screen_duration_seconds is {screen}; raise it to at least {speech_total:.2} or move cues to another cut");
    }

    // --- cast: speaker -> discovered character (host-owned) --------------
    let speakers = handoff.distinct_speakers();
    let mut cast: Vec<CastBinding> = Vec::with_capacity(speakers.len());
    for speaker in &speakers {
        let character = registry.and_then(|r| r.find_character(speaker));
        if registry.is_some() && character.is_none() {
            warnings.push(format!("{id}: speaker `{speaker}` matches no character under Characters/; treated as an unlisted voice"));
        }
        let voice = character.and_then(|c| c.voice.as_ref());
        cast.push(CastBinding {
            speaker: speaker.clone(),
            character: character
                .map(|c| c.name.clone())
                .unwrap_or_else(|| speaker.clone()),
            reference_image: character
                .and_then(|c| c.best_reference_image())
                .map(|p| p.to_string_lossy().to_string()),
            voice_provider: voice.map(|v| v.provider.clone()),
            voice_id: voice.map(|v| v.voice_id.clone()),
            voice_name: voice.and_then(|v| v.voice_name.clone()),
        });
    }

    // --- generation path --------------------------------------------------
    let lipsync = mode == Some(SpeechMode::SourceVideoLipsync);
    let requested_path = handoff.generation_requirements.generation_path;
    let resolved_path = match (lipsync, requested_path) {
        (true, None) => {
            not_ready!("source_video_lipsync requires generation_requirements.generation_path: audio_driven_i2v but it is missing; the host will not fall back to text_to_video");
            None
        }
        (true, Some(GenerationPath::PromptDriven)) => {
            fail!(
                "source_video_lipsync requires generation_path audio_driven_i2v, not prompt_driven"
            );
            Some(GenerationPath::PromptDriven)
        }
        (true, Some(GenerationPath::AudioDrivenI2v)) => Some(GenerationPath::AudioDrivenI2v),
        (false, None) => {
            warnings.push(format!(
                "{id}: generation_path not given for a non-lip-sync cut; recorded as prompt_driven"
            ));
            Some(GenerationPath::PromptDriven)
        }
        (false, Some(GenerationPath::AudioDrivenI2v)) => {
            fail!("generation_path audio_driven_i2v is only valid with speech.mode source_video_lipsync (speech.mode is {})", mode.map(|m| label(&m)).unwrap_or_else(|| "missing".to_string()));
            Some(GenerationPath::AudioDrivenI2v)
        }
        (false, Some(GenerationPath::PromptDriven)) => Some(GenerationPath::PromptDriven),
    };

    // --- source mode vs. assets -----------------------------------------
    let source_mode = handoff.generation_requirements.source_mode;
    let assets = &handoff.reference_assets;
    for (i, asset) in assets.iter().enumerate() {
        let path = asset.path.trim();
        if path.is_empty() {
            fail!("reference_assets[{i}] has an empty path");
        } else if registry.is_some() && !Path::new(path).is_file() {
            fail!("reference_assets[{i}] path `{path}` does not exist on disk; use one of the host-discovered asset paths");
        }
    }
    let start_frames = assets
        .iter()
        .filter(|a| a.role == AssetRole::StartFrame)
        .count();
    if start_frames > 1 {
        fail!("only one reference asset may have role start_frame, found {start_frames}");
    }
    match source_mode {
        SourceMode::TextToVideo => {
            if !assets.is_empty() {
                fail!("source_mode text_to_video cannot carry reference images, but {} reference_assets were given; switch to image_to_video / reference_to_video or remove them", assets.len());
            }
        }
        SourceMode::ReferenceToVideo => {
            if assets.is_empty() {
                fail!("source_mode reference_to_video requires at least one entry in reference_assets");
            }
        }
        SourceMode::ImageToVideo => {
            if start_frames == 0 {
                fail!(
                    "source_mode image_to_video requires a reference asset with role start_frame"
                );
            }
        }
    }

    // --- lip-sync rules ---------------------------------------------------
    if lipsync {
        if source_mode == SourceMode::TextToVideo {
            fail!("speech.mode source_video_lipsync cannot use source_mode text_to_video; the lip-sync pass needs a source performance from image_to_video or reference_to_video");
        }
        match on_camera_speakers.len() {
            0 => fail!("source_video_lipsync requires exactly one on-camera speaker, but every cue is off-camera"),
            1 => {}
            n => fail!(
                "source_video_lipsync can drive only one on-camera face per cut, but {n} on-camera speakers alternate here ({}); give each speaker their own cut or mark the others on_camera: false",
                on_camera_speakers.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        }
        let usable_image = assets.iter().any(|a| {
            matches!(
                a.role,
                AssetRole::StartFrame | AssetRole::CharacterReference
            ) && !a.path.trim().is_empty()
        });
        if !usable_image {
            not_ready!("source_video_lipsync needs a usable start_frame or character_reference image in reference_assets; none is present");
        }
        if let (Some(reg), 1) = (registry, on_camera_speakers.len()) {
            let speaker = on_camera_speakers.iter().next().expect("one speaker");
            match reg.find_character(speaker) {
                Some(c) if c.lipsync_ready() => {
                    let uses_own_shot = assets
                        .iter()
                        .any(|a| matches!(a.role, AssetRole::StartFrame | AssetRole::CharacterReference) && c.owns_image(&a.path));
                    if !uses_own_shot {
                        fail!(
                            "on-camera speaker `{speaker}` resolves to `{}` but none of the reference_assets is one of that character's saved shots; use `{}`",
                            c.name,
                            c.best_reference_image().map(|p| p.display().to_string()).unwrap_or_default()
                        );
                    }
                }
                Some(c) => not_ready!("on-camera speaker `{speaker}` resolves to `{}` which has no usable reference image on disk; generate its model sheet first or take the speaker off camera", c.name),
                None => not_ready!("on-camera speaker `{speaker}` matches no character under Characters/; lip-sync needs a discovered reference image"),
            }
        }
    }

    // --- voice-over with a visible lip-sync-ready speaker ------------------
    // Allowed: the line plays over a face that is not performing it. The host
    // cannot see the video, so it only asks that the prompt says so.
    if mode == Some(SpeechMode::VoiceOver) {
        let text = format!(
            "{} {} {}",
            handoff.prompt,
            handoff.negative_prompt.as_deref().unwrap_or(""),
            handoff.editing.cut_guidance.as_deref().unwrap_or("")
        )
        .to_lowercase();
        let acknowledged = [
            "lip",
            "mouth",
            "not speaking",
            "silent",
            "does not speak",
            "listening",
        ]
        .iter()
        .any(|k| text.contains(k));
        for speaker in &on_camera_speakers {
            let ready = registry
                .and_then(|r| r.find_character(speaker))
                .is_some_and(|c| c.lipsync_ready());
            if ready && !acknowledged {
                warnings.push(format!(
                    "{id}: `{speaker}` is on camera in a voice_over cut and has a reference image; if the line is heard over their face, say the face is not speaking in the prompt and add `lip movement` to negative_prompt — or make this a source_video_lipsync cut"
                ));
            }
        }
    }

    // --- render lane + provider bucket ------------------------------------
    let lane = RenderLane::for_request(
        resolved_path.unwrap_or(GenerationPath::PromptDriven),
        source_mode,
    );
    let minimum_source = screen + head + tail;
    let render_duration = if minimum_source.is_finite() {
        match config.policy_for(lane).choose(minimum_source) {
            Ok(seconds) => seconds,
            Err(e) => {
                fail!("{e} on the {} lane; shorten editing.estimated_screen_duration_seconds or the handles, or split the cut", label(&lane));
                f64::NAN
            }
        }
    } else {
        f64::NAN
    };

    if !issues.is_empty() {
        return Err(issues);
    }

    // --- normalize: the handoff becomes the single truth -------------------
    handoff.speech.mode = Some(mode.unwrap_or(SpeechMode::Silent));
    handoff.generation_requirements.generation_path = resolved_path;
    for (cue, timing) in handoff.speech.cues.iter_mut().zip(&cue_timings) {
        cue.estimated_duration_seconds = Some(timing.speech_seconds);
    }
    handoff.cast = cast;
    handoff.derived = Some(DerivedTiming {
        speaking_rate_wpm: wpm,
        speech_duration_seconds: speech_total,
        minimum_source_duration_seconds: round2(minimum_source),
        render_lane: lane,
        render_duration_seconds: render_duration,
    });

    let canonical = match canonical_yaml(&handoff) {
        Ok(yaml) => yaml,
        Err(e) => {
            return Err(vec![Issue::new(
                IssueKind::Validation,
                Some(id),
                format!("could not serialize canonical YAML: {e}"),
            )])
        }
    };
    if canonical.lines().any(|l| l.trim_start().starts_with("```")) {
        return Err(vec![Issue::new(IssueKind::Validation, Some(id), "a text field contains a line starting with ``` which would break the Markdown fence; rephrase it")]);
    }

    let timing = SceneTiming {
        cues: cue_timings,
        speech_total_seconds: speech_total,
        screen_seconds: screen,
        head_handle_seconds: head,
        tail_handle_seconds: tail,
        minimum_source_seconds: round2(minimum_source),
        render_duration_seconds: render_duration,
    };
    Ok(ValidatedScene {
        handoff,
        canonical_yaml: canonical,
        timing,
        spoken_words,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::error::retry_feedback;
    use crate::scene_plan::extract::parse_handoff_yaml;
    use crate::scene_plan::fixtures::{
        broll_scene_yaml, lipsync_cut_yaml, lipsync_scene_yaml, lipsync_scene_yaml_with_cues,
        voiceover_cut_yaml, write_fixture_project,
    };

    const TEXT: &str = "This is a twelve word sentence that needs a reliable duration estimate.";

    fn validate_yaml(yaml: &str) -> Result<ValidatedScene, Vec<Issue>> {
        let handoff =
            parse_handoff_yaml(yaml).unwrap_or_else(|e| panic!("fixture must parse: {e}\n{yaml}"));
        validate_scene(handoff, &HostConfig::default(), Some("9:16"), None)
    }

    fn validate_with(yaml: &str, registry: &AssetRegistry) -> Result<ValidatedScene, Vec<Issue>> {
        let handoff =
            parse_handoff_yaml(yaml).unwrap_or_else(|e| panic!("fixture must parse: {e}\n{yaml}"));
        validate_scene(
            handoff,
            &HostConfig::default(),
            Some("9:16"),
            Some(registry),
        )
    }

    fn assert_issue(
        result: Result<ValidatedScene, Vec<Issue>>,
        kind: IssueKind,
        needle: &str,
    ) -> Vec<Issue> {
        let issues = result.expect_err("expected rejection");
        assert!(
            issues
                .iter()
                .any(|i| i.kind == kind && i.message.contains(needle)),
            "no {kind:?} issue containing {needle:?}; got:\n{}",
            retry_feedback(&issues)
        );
        issues
    }

    fn expect_issue(yaml: &str, kind: IssueKind, needle: &str) -> Vec<Issue> {
        assert_issue(validate_yaml(yaml), kind, needle)
    }

    #[test]
    fn happy_lipsync_cut_recomputes_timing_and_picks_bucket() {
        let v = validate_yaml(&lipsync_scene_yaml(1, TEXT)).unwrap();
        assert_eq!(v.id(), CutId::new(1, 1));
        assert_eq!(
            v.handoff.speech.cues[0].estimated_duration_seconds,
            Some(4.36)
        );
        let d = v.handoff.derived.as_ref().unwrap();
        assert_eq!(d.minimum_source_duration_seconds, 8.0);
        assert_eq!(d.render_duration_seconds, 10.0);
        assert_eq!(d.render_lane, RenderLane::AudioDrivenI2v);
        assert_eq!(v.handoff.cast.len(), 1);
        assert_eq!(v.handoff.cast[0].character, "Sam");
        assert!(v.handoff.cast[0].reference_image.is_none());
    }

    #[test]
    fn canonical_round_trip_is_stable() {
        let first = validate_yaml(&lipsync_scene_yaml(1, TEXT)).unwrap();
        let reparsed = parse_handoff_yaml(&first.canonical_yaml).unwrap();
        let second = validate_scene(reparsed, &HostConfig::default(), Some("9:16"), None).unwrap();
        assert_eq!(first.handoff, second.handoff);
        assert_eq!(first.canonical_yaml, second.canonical_yaml);
    }

    #[test]
    fn text_to_video_cut_uses_buckets_not_continuous() {
        // 10 + 1 + 1 = 12 s minimum source: the v2 run rendered this as 12 s.
        let yaml = broll_scene_yaml(1).replace(
            "estimated_screen_duration_seconds: 4.0",
            "estimated_screen_duration_seconds: 10.0",
        );
        expect_issue(&yaml, IssueKind::Validation, "exceeds the largest provider render bucket 10.00s (buckets: [5.0, 10.0]) on the text_to_video lane");
        let v = validate_yaml(&broll_scene_yaml(1)).unwrap();
        assert_eq!(
            v.handoff.derived.as_ref().unwrap().render_duration_seconds,
            5.0
        );
        assert_eq!(
            v.handoff.derived.as_ref().unwrap().render_lane,
            RenderLane::TextToVideo
        );
    }

    #[test]
    fn speech_longer_than_screen_duration_fails_with_numbers() {
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six tw-seven tw-eight tw-nine thirty";
        let issues = expect_issue(
            &lipsync_scene_yaml(1, long),
            IssueKind::Validation,
            "speech needs 10.91s",
        );
        assert!(issues[0]
            .message
            .contains("estimated_screen_duration_seconds is 6"));
    }

    #[test]
    fn pauses_count_against_screen_duration() {
        let yaml = lipsync_scene_yaml(1, TEXT).replace("      on_camera: true\n", "      on_camera: true\n      pause_before_seconds: 1.0\n      pause_after_seconds: 0.7\n");
        expect_issue(&yaml, IssueKind::Validation, "plus 1.70s of pauses");
    }

    #[test]
    fn lipsync_path_and_mode_rules() {
        expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace(
                "source_mode: reference_to_video",
                "source_mode: text_to_video",
            ),
            IssueKind::Validation,
            "cannot use source_mode text_to_video",
        );
        expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace(
                "generation_path: audio_driven_i2v",
                "generation_path: prompt_driven",
            ),
            IssueKind::Validation,
            "requires generation_path audio_driven_i2v",
        );
        let issues = expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace("  generation_path: audio_driven_i2v\n", ""),
            IssueKind::NotReady,
            "will not fall back to text_to_video",
        );
        assert!(issues.iter().all(|i| i.kind == IssueKind::NotReady));
        expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace("  mode: source_video_lipsync\n", ""),
            IssueKind::NotReady,
            "speech.mode is missing",
        );
        expect_issue(
            &lipsync_scene_yaml(1, TEXT)
                .replace("role: character_reference", "role: style_reference"),
            IssueKind::NotReady,
            "needs a usable start_frame or character_reference image",
        );
        expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace("mode: source_video_lipsync", "mode: none"),
            IssueKind::Validation,
            "speech.mode is none but 1 cue(s)",
        );
        expect_issue(
            &broll_scene_yaml(1).replace("mode: none", "mode: voice_over"),
            IssueKind::Validation,
            "speech.cues is empty",
        );
        expect_issue(
            &broll_scene_yaml(1).replace(
                "generation_path: prompt_driven",
                "generation_path: audio_driven_i2v",
            ),
            IssueKind::Validation,
            "only valid with speech.mode source_video_lipsync",
        );
    }

    #[test]
    fn on_camera_speaker_rules() {
        expect_issue(
            &lipsync_scene_yaml_with_cues(
                1,
                &[
                    ("a", "Sam", "Hello there friend.", true),
                    ("b", "Alex", "Hi Sam good to see you.", true),
                ],
            ),
            IssueKind::Validation,
            "2 on-camera speakers alternate here (Alex, Sam)",
        );
        validate_yaml(&lipsync_scene_yaml_with_cues(
            1,
            &[
                ("a", "Sam", "Hello there friend.", true),
                ("b", "Alex", "Hi Sam.", false),
            ],
        ))
        .unwrap();
        expect_issue(
            &lipsync_scene_yaml_with_cues(1, &[("a", "Sam", "Hello there friend.", false)]),
            IssueKind::Validation,
            "every cue is off-camera",
        );
    }

    #[test]
    fn silent_cut_has_positive_zero_speech_duration() {
        let v = validate_yaml(&broll_scene_yaml(1)).unwrap();
        let d = v.handoff.derived.as_ref().unwrap();
        assert_eq!(d.speech_duration_seconds.to_bits(), 0.0f64.to_bits());
        assert!(
            v.canonical_yaml.contains("speech_duration_seconds: 0.0\n"),
            "{}",
            v.canonical_yaml
        );
        assert!(!v.canonical_yaml.contains("-0.0"));
    }

    #[test]
    fn source_mode_asset_rules() {
        expect_issue(
            &format!(
                "{}reference_assets:\n  - path: \"/x.png\"\n    role: style_reference\n",
                broll_scene_yaml(1)
            ),
            IssueKind::Validation,
            "text_to_video cannot carry reference images",
        );
        expect_issue(
            &broll_scene_yaml(1).replace(
                "source_mode: text_to_video",
                "source_mode: reference_to_video",
            ),
            IssueKind::Validation,
            "reference_to_video requires at least one",
        );
        let i2v = broll_scene_yaml(1)
            .replace("source_mode: text_to_video", "source_mode: image_to_video");
        expect_issue(
            &format!("{i2v}reference_assets:\n  - path: \"/x.png\"\n    role: style_reference\n"),
            IssueKind::Validation,
            "requires a reference asset with role start_frame",
        );
        let v = validate_yaml(&format!(
            "{i2v}reference_assets:\n  - path: \"/x.png\"\n    role: start_frame\n"
        ))
        .unwrap();
        assert_eq!(
            v.handoff.derived.as_ref().unwrap().render_lane,
            RenderLane::ImageToVideo
        );
    }

    #[test]
    fn aspect_ratio_must_match_production() {
        let handoff = parse_handoff_yaml(&lipsync_scene_yaml(1, TEXT)).unwrap();
        let issues =
            validate_scene(handoff, &HostConfig::default(), Some("16:9"), None).unwrap_err();
        assert!(issues[0]
            .message
            .contains("`9:16` but this production is `16:9`"));
        expect_issue(
            &lipsync_scene_yaml(1, TEXT)
                .replace("aspect_ratio: \"9:16\"", "aspect_ratio: vertical"),
            IssueKind::Validation,
            "must look like `9:16`",
        );
    }

    #[test]
    fn cue_ids_carry_source_scene_and_cut() {
        expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace("S01C01_a", "S01C02_a"),
            IssueKind::Validation,
            "numbered S01C02 but sits in S01C01",
        );
        expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace("S01C01_a", "S02C01_a"),
            IssueKind::Validation,
            "numbered S02C01 but sits in S01C01",
        );
        expect_issue(
            &lipsync_scene_yaml(1, TEXT).replace("S01C01_a", "S01_a"),
            IssueKind::Validation,
            "malformed id; expected S01C01_<letters or digits>",
        );
        expect_issue(
            &lipsync_scene_yaml_with_cues(
                1,
                &[("a", "Sam", "Hello.", true), ("a", "Sam", "Again.", true)],
            ),
            IssueKind::Validation,
            "duplicate id",
        );
        assert_eq!(parse_cue_id("S7C2_b"), Some(CutId::new(7, 2)));
        assert_eq!(parse_cue_id("S01_a"), None);
    }

    #[test]
    fn broll_defaults_and_bad_numbers() {
        let v =
            validate_yaml(&broll_scene_yaml(1).replace("  generation_path: prompt_driven\n", ""))
                .unwrap();
        assert_eq!(
            v.handoff.generation_requirements.generation_path,
            Some(GenerationPath::PromptDriven)
        );
        assert_eq!(v.handoff.speech.mode, Some(SpeechMode::Silent));
        assert!(v.handoff.cast.is_empty());
        let v = validate_yaml(&broll_scene_yaml(1).replace("speech:\n  mode: none\n", "")).unwrap();
        assert_eq!(v.handoff.speech.mode, Some(SpeechMode::Silent));
        expect_issue(
            &broll_scene_yaml(1).replace(
                "estimated_screen_duration_seconds: 4.0",
                "estimated_screen_duration_seconds: 0",
            ),
            IssueKind::Validation,
            "greater than 0",
        );
        expect_issue(
            &broll_scene_yaml(1).replace("head_handle_seconds: 0.5", "head_handle_seconds: -1"),
            IssueKind::Validation,
            "head_handle_seconds must be",
        );
        expect_issue(
            &broll_scene_yaml(1).replace("cut: 1", "cut: 0"),
            IssueKind::Validation,
            "cut must be 1 or greater",
        );
    }

    #[test]
    fn all_issues_are_collected_not_just_the_first() {
        let yaml = lipsync_scene_yaml(1, TEXT)
            .replace("S01C01_a", "S01C09_a")
            .replace(
                "source_mode: reference_to_video",
                "source_mode: text_to_video",
            )
            .replace(
                "generation_path: audio_driven_i2v",
                "generation_path: prompt_driven",
            );
        assert!(validate_yaml(&yaml).unwrap_err().len() >= 4);
    }

    #[test]
    fn fence_closing_line_in_text_is_rejected_but_inline_backticks_pass() {
        let bad = lipsync_scene_yaml(1, TEXT).replace(
            "  and settle again afterward. Locked-off camera with no zoom.",
            "  and settle again afterward.\n  ```\n  Locked-off camera.",
        );
        expect_issue(
            &bad,
            IssueKind::Validation,
            "would break the Markdown fence",
        );
        validate_yaml(&lipsync_scene_yaml(1, TEXT).replace(
            "Locked-off camera with no zoom.",
            "Locked-off camera; no ``` zoom.",
        ))
        .unwrap();
    }

    // ---- registry-aware rules --------------------------------------------

    #[test]
    fn lipsync_with_discovered_reference_binds_cast() {
        let dir = tempfile::tempdir().unwrap();
        let fx = write_fixture_project(dir.path());
        let reg = AssetRegistry::discover(dir.path());
        let yaml = lipsync_cut_yaml(
            1,
            1,
            &[("a", "Tom", "We shall be there before supper.", true)],
            &fx.tom_closeup.to_string_lossy(),
        );
        let v = validate_with(&yaml, &reg).unwrap();
        let cast = &v.handoff.cast[0];
        assert_eq!(cast.character, "Tom Vale");
        assert_eq!(
            cast.reference_image.as_deref(),
            Some(fx.tom_closeup.to_string_lossy().as_ref())
        );
        assert!(cast.voice_id.is_none());
        assert!(v
            .warnings
            .iter()
            .all(|w| !w.contains("matches no character")));

        let pip = lipsync_cut_yaml(
            1,
            2,
            &[("a", "Pip", "Will there be cocoa?", true)],
            &fx.pip_closeup.to_string_lossy(),
        );
        let v = validate_with(&pip, &reg).unwrap();
        assert_eq!(
            v.handoff.cast[0].voice_id.as_deref(),
            Some("MlxAbUenU34aJO4OFnwY")
        );
        assert_eq!(
            v.handoff.cast[0].voice_provider.as_deref(),
            Some("elevenlabs")
        );
    }

    #[test]
    fn reference_paths_must_exist_and_belong_to_the_speaker() {
        let dir = tempfile::tempdir().unwrap();
        let fx = write_fixture_project(dir.path());
        let reg = AssetRegistry::discover(dir.path());
        let ghost = lipsync_cut_yaml(
            1,
            1,
            &[("a", "Tom", "We shall be there before supper.", true)],
            "/project/Characters/Tom_9x16.png",
        );
        assert_issue(
            validate_with(&ghost, &reg),
            IssueKind::Validation,
            "does not exist on disk",
        );
        let wrong = lipsync_cut_yaml(
            1,
            1,
            &[("a", "Tom", "We shall be there before supper.", true)],
            &fx.nell_closeup.to_string_lossy(),
        );
        assert_issue(
            validate_with(&wrong, &reg),
            IssueKind::Validation,
            "none of the reference_assets is one of that character's saved shots",
        );
    }

    #[test]
    fn lipsync_for_unlisted_or_imageless_speakers_is_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        let fx = write_fixture_project(dir.path());
        let reg = AssetRegistry::discover(dir.path());
        let guard = lipsync_cut_yaml(
            1,
            1,
            &[("a", "Guard, outside", "Willowby Halt!", true)],
            &fx.tom_closeup.to_string_lossy(),
        );
        assert_issue(
            validate_with(&guard, &reg),
            IssueKind::NotReady,
            "matches no character under Characters/",
        );
        let marian = lipsync_cut_yaml(
            1,
            1,
            &[("a", "Aunt Marian", "Come in out of the rain.", true)],
            &fx.tom_closeup.to_string_lossy(),
        );
        assert_issue(
            validate_with(&marian, &reg),
            IssueKind::NotReady,
            "`Aunt Marian` which has no usable reference image",
        );
    }

    #[test]
    fn voice_over_with_visible_ready_speaker_is_a_choice_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        write_fixture_project(dir.path());
        let reg = AssetRegistry::discover(dir.path());
        let on_cam = voiceover_cut_yaml(
            1,
            1,
            &[("a", "Tom", "We shall be there before supper.", true)],
        );
        let v = validate_with(&on_cam, &reg).unwrap();
        assert_eq!(v.handoff.speech.mode, Some(SpeechMode::VoiceOver));
        assert!(
            v.warnings
                .iter()
                .any(|w| w.contains("is on camera in a voice_over cut and has a reference image")),
            "{:?}",
            v.warnings
        );
        let acknowledged = on_cam.replace(
            "prompt: |-
",
            "negative_prompt: \"lip movement, talking\"
prompt: |-
",
        );
        let v = validate_with(&acknowledged, &reg).unwrap();
        assert!(
            v.warnings
                .iter()
                .all(|w| !w.contains("voice_over cut and has")),
            "{:?}",
            v.warnings
        );
        let off_cam = voiceover_cut_yaml(
            1,
            1,
            &[
                ("a", "Tom", "We shall be there before supper.", false),
                ("b", "Guard", "Willowby Halt!", false),
            ],
        );
        assert!(validate_with(&off_cam, &reg).is_ok());
    }
}

//! Cross-field rules. Every rule lives here, not in prose.
use crate::scene_plan::assets::AssetRegistry;
use crate::scene_plan::config::HostConfig;
use crate::scene_plan::error::{Issue, IssueKind};
use crate::scene_plan::handoff::{
    label, CastBinding, CutId, DerivedTiming, GenerationPath, RenderLane, SceneHandoff, SourceMode,
    SpeechMode,
};
use crate::scene_plan::render::canonical_yaml;
use crate::scene_plan::resolve::{
    apply_duration, apply_generation, apply_host_defaults, assign_cue_ids, resolve_duration,
    resolve_generation,
};
use crate::scene_plan::timing::{cue_timing, normalized_words, round2, CueTiming, SceneTiming};
use regex::Regex;
use std::collections::{BTreeSet, HashSet};
use std::sync::OnceLock;

fn cue_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^S(\d+)C(\d+)_[A-Za-z0-9]+$").unwrap())
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
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

    apply_host_defaults(&mut handoff, config, expected_aspect_ratio);
    assign_cue_ids(id, &mut handoff.speech.cues);

    // --- identity / text -------------------------------------------------
    if handoff.source_scene == 0 {
        fail!("source_scene must be 1 or greater");
    }
    if handoff.cut == 0 {
        fail!("cut must be 1 or greater");
    }
    if handoff.prompt.trim().is_empty() {
        fail!("prompt must not be empty");
    }
    if handoff.continuity.group.trim().is_empty() {
        fail!("continuity.group must not be empty; use `standalone` for a cut that stands alone");
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
            voice_provider: voice.and_then(|v| nonempty(&v.provider)),
            voice_id: voice.and_then(|v| nonempty(&v.voice_id)),
            voice_name: voice.and_then(|v| v.voice_name.clone()),
            voice_sample: voice.and_then(|v| {
                v.sample_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            }),
        });
    }

    // --- generation: invert the old rejection matrix ----------------------
    let lipsync = mode == Some(SpeechMode::SourceVideoLipsync);
    if lipsync {
        match on_camera_speakers.len() {
            0 => fail!("source_video_lipsync requires exactly one on-camera speaker, but every cue is off-camera"),
            1 => {}
            n => fail!(
                "source_video_lipsync can drive only one on-camera face per cut, but {n} on-camera speakers alternate here ({}); give each speaker their own cut or mark the others on_camera: false",
                on_camera_speakers.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
        }
        if let (Some(reg), 1) = (registry, on_camera_speakers.len()) {
            let speaker = on_camera_speakers.iter().next().expect("one speaker");
            match reg.find_character(speaker) {
                Some(c) if c.lipsync_ready() => {}
                Some(c) => not_ready!("on-camera speaker `{speaker}` resolves to `{}` which has no usable reference image on disk; generate its model sheet first or take the speaker off camera", c.name),
                None => not_ready!("on-camera speaker `{speaker}` matches no character under Characters/; lip-sync needs a discovered reference image"),
            }
        }
    }

    let resolved_gen = resolve_generation(
        mode,
        &on_camera_speakers,
        &cast,
        registry,
        &handoff.reference_assets,
    );
    if lipsync
        && resolved_gen.source_mode == SourceMode::TextToVideo
        && on_camera_speakers.len() == 1
        && registry.is_none()
    {
        not_ready!("source_video_lipsync needs a usable start_frame or character_reference image; none is present");
    }
    apply_generation(&mut handoff, resolved_gen);

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

    // --- render lane + host-snapped duration ------------------------------
    let lane = RenderLane::for_request(
        handoff
            .generation_requirements
            .generation_path
            .unwrap_or(GenerationPath::PromptDriven),
        handoff.generation_requirements.source_mode,
    );
    let resolved_timing = match resolve_duration(speech_total, handoff.editing.pace, lane, config) {
        Ok(t) => Some(t),
        Err(e) => {
            fail!(
                "{e} on the {} lane ({words_total} words at {wpm} WPM plus {pauses_total:.2}s of pauses); split the cut",
                label(&lane)
            );
            None
        }
    };
    if let Some(t) = &resolved_timing {
        apply_duration(&mut handoff, t);
    }
    let screen = resolved_timing
        .as_ref()
        .map(|t| t.screen_seconds)
        .unwrap_or(f64::NAN);
    let head = resolved_timing
        .as_ref()
        .map(|t| t.head_handle_seconds)
        .unwrap_or(0.0);
    let tail = resolved_timing
        .as_ref()
        .map(|t| t.tail_handle_seconds)
        .unwrap_or(0.0);
    let minimum_source = resolved_timing
        .as_ref()
        .map(|t| t.minimum_source_seconds)
        .unwrap_or(f64::NAN);
    let render_duration = resolved_timing
        .as_ref()
        .map(|t| t.render_duration_seconds)
        .unwrap_or(f64::NAN);

    if !issues.is_empty() {
        return Err(issues);
    }

    // --- normalize: the handoff becomes the single truth -------------------
    handoff.speech.mode = Some(mode.unwrap_or(SpeechMode::Silent));
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
        slim_broll_yaml, slim_lipsync_yaml, voiceover_cut_yaml, write_fixture_project,
    };
    use crate::scene_plan::handoff::Pace;

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
        assert_eq!(d.minimum_source_duration_seconds, 6.36);
        assert_eq!(v.handoff.editing.estimated_screen_duration_seconds, 5.36);
        assert_eq!(d.render_duration_seconds, 10.0);
        assert_eq!(v.handoff.editing.pace, Pace::Natural);
        assert_eq!(v.handoff.provider, "atlascloud");
        assert_eq!(v.handoff.generation_requirements.aspect_ratio, "9:16");
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
        let v = validate_yaml(&broll_scene_yaml(1)).unwrap();
        assert_eq!(
            v.handoff.derived.as_ref().unwrap().render_duration_seconds,
            5.0
        );
        assert_eq!(
            v.handoff.derived.as_ref().unwrap().render_lane,
            RenderLane::TextToVideo
        );
        assert_eq!(v.handoff.editing.estimated_screen_duration_seconds, 4.0);
    }

    #[test]
    fn speech_longer_than_largest_bucket_fails_with_numbers() {
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six tw-seven tw-eight tw-nine thirty";
        let issues = expect_issue(
            &lipsync_scene_yaml(1, long),
            IssueKind::Validation,
            "exceeds the largest provider render bucket 10.00s",
        );
        assert!(issues[0].message.contains("plus 0.00s of pauses"));
        assert!(issues[0].message.contains("split the cut"));
    }

    #[test]
    fn pauses_count_against_the_bucket() {
        let yaml = lipsync_scene_yaml(1, TEXT).replace(
            "      on_camera: true\n",
            "      on_camera: true\n      pause_before_seconds: 4.0\n      pause_after_seconds: 3.0\n",
        );
        expect_issue(&yaml, IssueKind::Validation, "plus 7.00s of pauses");
    }

    #[test]
    fn host_overwrites_illegal_generation_fields() {
        let flipped = validate_yaml(&lipsync_scene_yaml(1, TEXT).replace(
            "source_mode: reference_to_video",
            "source_mode: text_to_video",
        ))
        .unwrap();
        assert_eq!(
            flipped.handoff.generation_requirements.source_mode,
            SourceMode::ReferenceToVideo
        );
        assert_eq!(
            flipped.handoff.generation_requirements.generation_path,
            Some(GenerationPath::AudioDrivenI2v)
        );
        let path = validate_yaml(&lipsync_scene_yaml(1, TEXT).replace(
            "generation_path: audio_driven_i2v",
            "generation_path: prompt_driven",
        ))
        .unwrap();
        assert_eq!(
            path.handoff.generation_requirements.generation_path,
            Some(GenerationPath::AudioDrivenI2v)
        );
        let missing_path = validate_yaml(
            &lipsync_scene_yaml(1, TEXT).replace("  generation_path: audio_driven_i2v\n", ""),
        )
        .unwrap();
        assert_eq!(
            missing_path.handoff.generation_requirements.generation_path,
            Some(GenerationPath::AudioDrivenI2v)
        );
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
        let broll = validate_yaml(&broll_scene_yaml(1).replace(
            "generation_path: prompt_driven",
            "generation_path: audio_driven_i2v",
        ))
        .unwrap();
        assert_eq!(
            broll.handoff.generation_requirements.generation_path,
            Some(GenerationPath::PromptDriven)
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
    fn host_resolves_source_mode_from_assets() {
        let with_style = validate_yaml(&format!(
            "{}reference_assets:\n  - path: \"/x.png\"\n    role: style_reference\n",
            broll_scene_yaml(1)
        ))
        .unwrap();
        assert_eq!(
            with_style.handoff.generation_requirements.source_mode,
            SourceMode::ReferenceToVideo
        );
        let no_assets = validate_yaml(&broll_scene_yaml(1).replace(
            "source_mode: text_to_video",
            "source_mode: reference_to_video",
        ))
        .unwrap();
        assert_eq!(
            no_assets.handoff.generation_requirements.source_mode,
            SourceMode::TextToVideo
        );
        let i2v = broll_scene_yaml(1)
            .replace("source_mode: text_to_video", "source_mode: image_to_video");
        let v = validate_yaml(&format!(
            "{i2v}reference_assets:\n  - path: \"/x.png\"\n    role: start_frame\n"
        ))
        .unwrap();
        assert_eq!(
            v.handoff.derived.as_ref().unwrap().render_lane,
            RenderLane::ImageToVideo
        );
        assert_eq!(
            v.handoff.generation_requirements.source_mode,
            SourceMode::ImageToVideo
        );
    }

    #[test]
    fn host_overwrites_aspect_ratio_from_production() {
        let handoff = parse_handoff_yaml(&lipsync_scene_yaml(1, TEXT)).unwrap();
        let v = validate_scene(handoff, &HostConfig::default(), Some("16:9"), None).unwrap();
        assert_eq!(v.handoff.generation_requirements.aspect_ratio, "16:9");
        let garbled = validate_yaml(
            &lipsync_scene_yaml(1, TEXT)
                .replace("aspect_ratio: \"9:16\"", "aspect_ratio: vertical"),
        )
        .unwrap();
        assert_eq!(garbled.handoff.generation_requirements.aspect_ratio, "9:16");
    }

    #[test]
    fn host_rewrites_cue_ids() {
        let v =
            validate_yaml(&lipsync_scene_yaml(1, TEXT).replace("S01C01_a", "S01C02_a")).unwrap();
        assert_eq!(v.handoff.speech.cues[0].id, "S01C01_a");
        let v =
            validate_yaml(&lipsync_scene_yaml(1, TEXT).replace("S01C01_a", "S02C01_a")).unwrap();
        assert_eq!(v.handoff.speech.cues[0].id, "S01C01_a");
        let v = validate_yaml(&lipsync_scene_yaml(1, TEXT).replace("S01C01_a", "S01_a")).unwrap();
        assert_eq!(v.handoff.speech.cues[0].id, "S01C01_a");
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
        let ignored_zero = validate_yaml(&broll_scene_yaml(1).replace(
            "estimated_screen_duration_seconds: 4.0",
            "estimated_screen_duration_seconds: 0",
        ))
        .unwrap();
        assert!(
            ignored_zero
                .handoff
                .editing
                .estimated_screen_duration_seconds
                > 0.0
        );
        let ignored_handle = validate_yaml(
            &broll_scene_yaml(1).replace("head_handle_seconds: 0.5", "head_handle_seconds: -1"),
        )
        .unwrap();
        assert_eq!(ignored_handle.handoff.editing.head_handle_seconds, 0.5);
        expect_issue(
            &broll_scene_yaml(1).replace("cut: 1", "cut: 0"),
            IssueKind::Validation,
            "cut must be 1 or greater",
        );
    }

    #[test]
    fn all_issues_are_collected_not_just_the_first() {
        let yaml = lipsync_scene_yaml(1, TEXT)
            .replace(
                "prompt: |-\n  Sam faces the camera and speaks naturally. Hold settled before speaking\n  and settle again afterward. Locked-off camera with no zoom.\n",
                "prompt: \"\"\n",
            )
            .replace("      speaker: Sam\n", "      speaker: \"\"\n")
            .replace("cut: 1", "cut: 0");
        assert!(validate_yaml(&yaml).unwrap_err().len() >= 3);
    }

    #[test]
    fn slim_yaml_is_enough_for_the_host() {
        let v = validate_yaml(&slim_broll_yaml(1)).unwrap();
        assert_eq!(v.handoff.provider, "atlascloud");
        assert_eq!(
            v.handoff.generation_requirements.generation_path,
            Some(GenerationPath::PromptDriven)
        );
        assert_eq!(
            v.handoff.generation_requirements.source_mode,
            SourceMode::TextToVideo
        );
        assert_eq!(v.handoff.generation_requirements.aspect_ratio, "9:16");
        assert_eq!(
            v.handoff.derived.as_ref().unwrap().render_duration_seconds,
            5.0
        );
        expect_issue(
            &slim_lipsync_yaml(1, TEXT),
            IssueKind::NotReady,
            "needs a usable start_frame or character_reference image",
        );
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
    fn host_attaches_the_speakers_reference_and_ignores_model_paths() {
        let dir = tempfile::tempdir().unwrap();
        let fx = write_fixture_project(dir.path());
        let reg = AssetRegistry::discover(dir.path());
        let slim = slim_lipsync_yaml(1, "We shall be there before supper.")
            .replace("speaker: Sam", "speaker: Tom");
        let v = validate_with(&slim, &reg).unwrap();
        assert_eq!(
            v.handoff.reference_assets[0].path,
            fx.tom_closeup.to_string_lossy()
        );
        assert_eq!(
            v.handoff.generation_requirements.source_mode,
            SourceMode::ReferenceToVideo
        );
        let ghost = lipsync_cut_yaml(
            1,
            1,
            &[("a", "Tom", "We shall be there before supper.", true)],
            "/project/Characters/Tom_9x16.png",
        );
        let v = validate_with(&ghost, &reg).unwrap();
        assert_eq!(
            v.handoff.reference_assets[0].path,
            fx.tom_closeup.to_string_lossy()
        );
        let wrong = lipsync_cut_yaml(
            1,
            1,
            &[("a", "Tom", "We shall be there before supper.", true)],
            &fx.nell_closeup.to_string_lossy(),
        );
        let v = validate_with(&wrong, &reg).unwrap();
        assert_eq!(
            v.handoff.reference_assets[0].path,
            fx.tom_closeup.to_string_lossy()
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

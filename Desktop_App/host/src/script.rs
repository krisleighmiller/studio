//! Host-owned script parsing. Scene headings, labelled beats, and dialogue
//! lists come from the markdown the studio already writes — not from the LLM.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const STAGE_DIRECTION_LABELS: &[&str] = &[
    "location",
    "action",
    "sound",
    "transition",
    "dialogue",
    "camera",
    "music",
    "note",
    "notes",
    "vfx",
    "sfx",
    "setting",
    "time",
    "mood",
    "props",
    "wardrobe",
    "lighting",
    "continuity",
    "style",
    "voiceover",
    "narration",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeatKind {
    Location,
    Action,
    Sound,
    Transition,
    Dialogue,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptBeat {
    pub kind: BeatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueLine {
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedScene {
    pub id: u32,
    pub title: String,
    pub heading: String,
    pub source_text: String,
    pub location: Option<String>,
    pub action: Vec<String>,
    pub sound: Vec<String>,
    pub transitions: Vec<String>,
    pub dialogue: Vec<DialogueLine>,
    pub beats: Vec<ScriptBeat>,
}

impl ParsedScene {
    pub fn dialogue_text(&self) -> String {
        self.dialogue
            .iter()
            .map(|d| d.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^\s{0,3}#{0,6}\s*scene\s+(\d+)\b.*$").unwrap())
}

fn scene_number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bscene\s+(\d+)\b").unwrap())
}

fn aspect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{1,4}:\d{1,4}\b").unwrap())
}

fn bullet_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*[-*+]\s+\*\*\s*([^*]+?)\s*:?\s*\*\*\s*:?\s*(.*?)\s*$").unwrap()
    })
}

fn heading_number(line: &str) -> Option<u32> {
    heading_re()
        .captures(line.trim_end())
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

/// First `scene N` mention in free text (briefs, filenames, chat).
pub fn requested_scene_number(text: &str) -> Option<u32> {
    scene_number_re()
        .captures(text)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

pub fn aspect_from_text(text: &str) -> Option<String> {
    aspect_re().find(text).map(|m| m.as_str().to_string())
}

pub fn section_title(section: &str) -> Option<String> {
    section.lines().find_map(|line| {
        let trimmed = line.trim().trim_start_matches('#').trim();
        trimmed
            .to_ascii_lowercase()
            .starts_with("scene ")
            .then(|| trimmed.to_string())
    })
}

/// One source-scene body, heading through the next scene heading.
pub fn extract_scene_section(text: &str, scene: u32) -> Option<String> {
    extract_scenes(text)
        .into_iter()
        .find(|s| s.id == scene)
        .map(|s| s.source_text)
}

pub fn extract_scenes(text: &str) -> Vec<ParsedScene> {
    let mut spans: Vec<(u32, usize)> = Vec::new();
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        if let Some(n) = heading_number(line) {
            spans.push((n, offset));
        }
        offset += line.len();
    }
    let mut out = Vec::new();
    for (i, (id, start)) in spans.iter().enumerate() {
        let end = spans.get(i + 1).map(|(_, s)| *s).unwrap_or(text.len());
        let source_text = text[*start..end].trim().to_string();
        if source_text.is_empty() {
            continue;
        }
        out.push(parse_scene_body(*id, source_text));
    }
    out
}

fn parse_scene_body(id: u32, source_text: String) -> ParsedScene {
    let heading = section_title(&source_text).unwrap_or_else(|| format!("Scene {id}"));
    let beats = extract_beats(&source_text);
    let mut location = None;
    let mut action = Vec::new();
    let mut sound = Vec::new();
    let mut transitions = Vec::new();
    let mut dialogue = Vec::new();
    for beat in &beats {
        match beat.kind {
            BeatKind::Location if location.is_none() => location = Some(beat.text.clone()),
            BeatKind::Location => action.push(beat.text.clone()),
            BeatKind::Action => action.push(beat.text.clone()),
            BeatKind::Sound => sound.push(beat.text.clone()),
            BeatKind::Transition => transitions.push(beat.text.clone()),
            BeatKind::Dialogue => dialogue.push(DialogueLine {
                speaker: beat.speaker.clone().unwrap_or_else(|| "Unknown".into()),
                text: beat.text.clone(),
            }),
            BeatKind::Other => {}
        }
    }
    ParsedScene {
        id,
        title: heading.clone(),
        heading,
        source_text,
        location,
        action,
        sound,
        transitions,
        dialogue,
        beats,
    }
}

pub fn extract_beats(section: &str) -> Vec<ScriptBeat> {
    let mut beats = Vec::new();
    let mut in_dialogue = false;
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(caps) = bullet_re().captures(line) {
            let label = caps[1].trim().trim_end_matches(':').trim().to_string();
            let label_l = label.to_ascii_lowercase();
            let text = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            if label_l == "dialogue" {
                in_dialogue = true;
                continue;
            }
            if let Some(kind) = beat_kind(&label_l) {
                in_dialogue = false;
                if !text.is_empty() {
                    beats.push(ScriptBeat {
                        kind,
                        speaker: None,
                        text: text.to_string(),
                    });
                }
                continue;
            }
            if in_dialogue && !text.is_empty() {
                beats.push(ScriptBeat {
                    kind: BeatKind::Dialogue,
                    speaker: Some(clean_speaker(&label)),
                    text: strip_quotes(text),
                });
            }
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with('-') || trimmed.starts_with('*') {
            in_dialogue = false;
            continue;
        }
        if in_dialogue {
            if let Some(last) = beats.last_mut().filter(|b| b.kind == BeatKind::Dialogue) {
                last.text.push(' ');
                last.text.push_str(trimmed);
            }
        }
    }
    beats
}

fn beat_kind(label: &str) -> Option<BeatKind> {
    match label {
        "location" | "setting" => Some(BeatKind::Location),
        "action" => Some(BeatKind::Action),
        "sound" | "sfx" => Some(BeatKind::Sound),
        "transition" => Some(BeatKind::Transition),
        other if STAGE_DIRECTION_LABELS.contains(&other) => Some(BeatKind::Other),
        _ => None,
    }
}

fn clean_speaker(raw: &str) -> String {
    raw.split(',')
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_end_matches(':')
        .to_string()
}

fn strip_quotes(text: &str) -> String {
    text.trim_matches(|c| matches!(c, '"' | '“' | '”'))
        .trim()
        .to_string()
}

/// Flattened dialogue words for coverage / scene_plan compatibility.
pub fn extract_dialogue_text(section: &str) -> String {
    extract_beats(section)
        .into_iter()
        .filter(|b| b.kind == BeatKind::Dialogue)
        .map(|b| b.text)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "Scene 1 — The last train\n\n\
- **Location:** A two-carriage night train approaching Willowby Halt.\n\
- **Action:** Rain streams down the carriage windows. Pip Vale, six, small and hungry, clutches his coat and peers at Tom’s purse.\n\
- **Dialogue:**\n\
- **Tom:** We shall be there before supper.\n\
- **Pip:** Will there be cocoa?\n\
- **Tom:** Aunt Marian promised clean sheets and hot cocoa.\n\
- **Pip:** And jam?\n\
- **Nell:** If Aunt Marian has any.\n\
- **Pip:** Then we must get there quickly.\n\
- **Sound:** Train wheels clatter. Rain strikes the glass. The hanging lamp creaks overhead.\n\
- **Action:** The train slows. Its brakes sigh. The children gather the parcel and prepare to leave.\n\
- **Dialogue:**\n\
- **Guard, outside:** Willowby Halt!\n\
- **Transition:** Match cut from the train’s warm yellow window to the red lantern burning in the rain.\n\n\
Scene 2 — The halt\n\n\
- **Dialogue:**\n\
- **Nell:** Here we are.\n";

    const EXPECTED_DIALOGUE: &str = "We shall be there before supper. Will there be cocoa? Aunt Marian promised clean sheets and hot cocoa. And jam? If Aunt Marian has any. Then we must get there quickly. Willowby Halt!";

    #[test]
    fn extracts_all_scenes_with_beats() {
        let scenes = extract_scenes(SCRIPT);
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].id, 1);
        assert_eq!(scenes[0].title, "Scene 1 — The last train");
        assert_eq!(
            scenes[0].location.as_deref(),
            Some("A two-carriage night train approaching Willowby Halt.")
        );
        assert_eq!(scenes[0].action.len(), 2);
        assert_eq!(scenes[0].sound.len(), 1);
        assert_eq!(scenes[0].transitions.len(), 1);
        assert_eq!(scenes[0].dialogue.len(), 7);
        assert_eq!(scenes[0].dialogue[6].speaker, "Guard");
        assert_eq!(scenes[0].dialogue[6].text, "Willowby Halt!");
        assert_eq!(scenes[0].dialogue_text(), EXPECTED_DIALOGUE);
        assert_eq!(scenes[1].dialogue_text(), "Here we are.");
    }

    #[test]
    fn hashed_headings_and_compat_helpers() {
        let md = format!("## Scenes\n\n### {SCRIPT}");
        assert!(extract_scene_section(&md, 1).unwrap().contains("Tom:"));
        assert_eq!(
            extract_dialogue_text(&extract_scene_section(&md, 1).unwrap()),
            EXPECTED_DIALOGUE
        );
        assert_eq!(
            section_title(&extract_scene_section(&md, 1).unwrap()).as_deref(),
            Some("Scene 1 — The last train")
        );
        assert_eq!(requested_scene_number("notes for Scene 3 please"), Some(3));
        assert_eq!(
            aspect_from_text("Format 9:16\nStyle storybook").as_deref(),
            Some("9:16")
        );
        assert!(extract_scenes("## Scenes\n\nNo numbered scene yet.").is_empty());
    }
}

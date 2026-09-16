//! Source-speech coverage. Cue text must quote the section's source speech
//! verbatim, in order, as a prefix of what is not yet covered. Coverage is a
//! word count into the normalized source, never a regex over the output.
use crate::scene_plan::error::{Issue, IssueKind};
use crate::scene_plan::timing::normalize_token;

struct Token {
    start: usize,
    normalized: String,
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut push = |start: usize, end: usize| {
        let normalized = normalize_token(&text[start..end]);
        if !normalized.is_empty() {
            out.push(Token { start, normalized });
        }
    };
    let mut start: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(s) = start.take() {
                push(s, i);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        push(s, text.len());
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub covered_word_count: usize,
    pub consumed_words: usize,
    pub remaining_text: String,
}

pub fn total_words(source: &str) -> usize {
    tokens(source).len()
}

fn remaining_from(toks: &[Token], source: &str, covered: usize) -> String {
    match toks.get(covered) {
        Some(t) => source[t.start..].trim().to_string(),
        None => String::new(),
    }
}

pub fn remaining_text(source: &str, covered: usize) -> String {
    remaining_from(&tokens(source), source, covered)
}

pub fn consume_prefix(
    source: &str,
    already_covered: usize,
    spoken: &[String],
) -> Result<Coverage, Issue> {
    let toks = tokens(source);
    let already = already_covered.min(toks.len());
    for (k, word) in spoken.iter().enumerate() {
        let pos = already + k;
        match toks.get(pos) {
            None => {
                return Err(Issue::new(
                    IssueKind::Coverage,
                    None,
                    format!(
                        "cue text runs past the end of this section's source speech: {} extra word(s) starting with `{}`; only plan speech that is in the script",
                        spoken.len() - k,
                        word
                    ),
                ))
            }
            Some(t) if &t.normalized == word => {}
            Some(t) => {
                let expected_ctx = toks[pos..(pos + 8).min(toks.len())].iter().map(|t| t.normalized.as_str()).collect::<Vec<_>>().join(" ");
                let got_ctx = spoken[k..(k + 8).min(spoken.len())].join(" ");
                return Err(Issue::new(
                    IssueKind::Coverage,
                    None,
                    format!(
                        "cue text diverges from the source speech at source word {pos} (0-based): expected `{}` but the cue says `{}`; source continues \"{expected_ctx} …\", cues continue \"{got_ctx} …\"; cue text must quote the script verbatim and in order",
                        t.normalized, word
                    ),
                ));
            }
        }
    }
    let covered = already + spoken.len();
    Ok(Coverage {
        covered_word_count: covered,
        consumed_words: spoken.len(),
        remaining_text: remaining_from(&toks, source, covered),
    })
}

/// Original text of source words `[start, end)`, punctuation preserved.
pub fn text_between(source: &str, start: usize, end: usize) -> String {
    let toks = tokens(source);
    if start >= end || start >= toks.len() {
        return String::new();
    }
    let from = toks[start].start;
    let to = toks.get(end).map(|t| t.start).unwrap_or(source.len());
    source[from..to].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_plan::timing::normalized_words;

    const SOURCE: &str = "This is a twelve word sentence that needs a reliable duration estimate. Then Sam turns and leaves the room.";

    #[test]
    fn prefix_consumption_tracks_remaining_text() {
        let first = normalized_words(
            "This is a twelve word sentence that needs a reliable duration estimate.",
        );
        let c = consume_prefix(SOURCE, 0, &first).unwrap();
        assert_eq!(c.covered_word_count, 12);
        assert_eq!(c.remaining_text, "Then Sam turns and leaves the room.");
        let second = normalized_words("Then Sam turns and leaves the room.");
        let c = consume_prefix(SOURCE, 12, &second).unwrap();
        assert_eq!(c.covered_word_count, 19);
        assert_eq!(c.remaining_text, "");
        assert_eq!(total_words(SOURCE), 19);
    }

    #[test]
    fn punctuation_and_case_differences_are_tolerated() {
        let spoken = normalized_words(
            "this is a TWELVE word sentence, that needs a reliable duration estimate",
        );
        assert_eq!(
            consume_prefix(SOURCE, 0, &spoken)
                .unwrap()
                .covered_word_count,
            12
        );
    }

    #[test]
    fn divergence_is_reported_with_position_and_context() {
        let spoken = normalized_words("This is a thirteen word sentence");
        let issue = consume_prefix(SOURCE, 0, &spoken).unwrap_err();
        assert_eq!(issue.kind, IssueKind::Coverage);
        assert!(
            issue.message.contains("at source word 3"),
            "{}",
            issue.message
        );
        assert!(
            issue
                .message
                .contains("expected `twelve` but the cue says `thirteen`"),
            "{}",
            issue.message
        );
    }

    #[test]
    fn skipping_words_is_a_divergence() {
        let spoken = normalized_words(
            "This is a twelve word sentence that needs reliable duration estimate.",
        );
        assert!(consume_prefix(SOURCE, 0, &spoken)
            .unwrap_err()
            .message
            .contains("expected `a` but the cue says `reliable`"));
    }

    #[test]
    fn overrun_is_reported() {
        let spoken = normalized_words("Then Sam turns and leaves the room. And more words.");
        let issue = consume_prefix(SOURCE, 12, &spoken).unwrap_err();
        assert!(
            issue
                .message
                .contains("3 extra word(s) starting with `and`"),
            "{}",
            issue.message
        );
    }

    #[test]
    fn text_between_returns_original_spelling() {
        assert_eq!(
            text_between(SOURCE, 12, 19),
            "Then Sam turns and leaves the room."
        );
        assert_eq!(text_between(SOURCE, 0, 2), "This is");
        assert_eq!(text_between(SOURCE, 5, 5), "");
        assert_eq!(text_between(SOURCE, 40, 50), "");
    }

    #[test]
    fn empty_spoken_is_a_noop_and_empty_source_is_fully_covered() {
        let c = consume_prefix(SOURCE, 5, &[]).unwrap();
        assert_eq!(c.covered_word_count, 5);
        assert_eq!(total_words("   "), 0);
        assert_eq!(remaining_text("", 0), "");
        assert_eq!(consume_prefix("", 0, &[]).unwrap().remaining_text, "");
    }
}

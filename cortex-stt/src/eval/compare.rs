//! Whether an output says what the reference says.
//!
//! Every cell nobody has ruled on is decided here, so the rule lives on
//! the server: one implementation scores the grid, the per-model summary
//! and the run list, and changing it changes all three at once.
//!
//! What it folds away was measured, not chosen. Over the 553 cells a
//! person had ruled on, folding width, case, whitespace and punctuation
//! reproduces 552 of those rulings and contradicts none. Chinese numerals
//! are deliberately left alone: 「百分之五十」 against `50%` is the one
//! pair the rulings themselves disagree on, so it stays a question for a
//! person rather than becoming a rule that overrules them.

/// A person's ruling if there is one, the comparison otherwise.
///
/// Stored rulings are only ever written by a person, so this is the one
/// place the two meet — and the direction never reverses: no comparison
/// result is written back over a ruling.
pub fn verdict(ruling: Option<bool>, matches_reference: bool) -> bool {
    ruling.unwrap_or(matches_reference)
}

pub fn matches_reference(output: &str, reference: &str) -> bool {
    let out = normalise(output);
    !out.is_empty() && out == normalise(reference)
}

/// Keep only what was heard as a word.
///
/// Punctuation goes wherever it sits, not just at the end: a model that
/// writes 「等一下，會下雨嗎？」 for 「等一下會下雨嗎」 heard the
/// utterance correctly and made a claim about its clauses, which is not
/// a transcription error. Spacing goes for the same reason — a whole
/// family of models writes 「打 開 入 口 燈」 and means 打開入口燈.
fn normalise(text: &str) -> String {
    text.chars()
        .map(fold_width)
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Fullwidth forms to their ASCII twins. The same model returns `50%` or
/// `５０％` depending on the language hint, and neither is an error.
fn fold_width(c: char) -> char {
    match c as u32 {
        n @ 0xFF01..=0xFF5E => char::from_u32(n - 0xFEE0).unwrap_or(c),
        0x3000 => ' ',
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each case here is a cell from the run of 2026-08-27 that a person
    /// ruled correct while the text differed from the reference.
    #[test]
    fn reproduces_the_rulings_it_was_measured_against() {
        assert!(matches_reference("打 開 入 口 燈", "打開入口燈"));
        assert!(matches_reference("Alexa。", "alexa"));
        assert!(matches_reference("等一下，會下雨嗎？", "等一下會下雨嗎"));
        assert!(matches_reference("今 天 天 氣", "今天天氣"));
    }

    #[test]
    fn full_and_half_width_are_the_same_utterance() {
        assert!(matches_reference("窗簾打開５０％", "窗簾打開50%"));
        assert!(matches_reference("ＡＢＣ", "abc"));
    }

    /// The pair the human rulings disagree on. A rule that folded it
    /// would overrule a person in three of the four cells it touches.
    #[test]
    fn chinese_numerals_stay_a_question_for_a_person() {
        assert!(!matches_reference("窗簾打開50%", "窗簾打開百分之五十"));
    }

    #[test]
    fn a_wrong_word_is_still_wrong() {
        assert!(!matches_reference("打開大燈", "打開入口燈"));
        assert!(!matches_reference("關閉入口燈", "打開入口燈"));
    }

    /// An empty transcript is the failure mode of a model that heard
    /// nothing; it must never match, whatever the reference is.
    #[test]
    fn nothing_is_never_a_match() {
        assert!(!matches_reference("", "打開入口燈"));
        assert!(!matches_reference("。。。", "打開入口燈"));
        assert!(!matches_reference("", ""));
    }

    #[test]
    fn a_ruling_outranks_the_comparison_in_both_directions() {
        assert!(verdict(Some(true), false));
        assert!(!verdict(Some(false), true));
        assert!(verdict(None, true));
        assert!(!verdict(None, false));
    }
}

//! One-line text for a card: whitespace folded, cut to a width in chars.

pub fn fold(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cuts `text` to `max` chars, the last one an ellipsis when it was longer.
pub fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_keeps_text_that_fits_and_marks_what_it_drops() {
        assert_eq!(cut("abc", 3), "abc");
        assert_eq!(cut("abcd", 3), "ab…");
        assert_eq!(cut("가나다라", 3), "가나…");
    }

    #[test]
    fn fold_joins_every_run_of_whitespace_with_one_space() {
        assert_eq!(fold("  a\n\tb  c "), "a b c");
    }
}

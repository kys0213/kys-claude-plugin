//! Text for a card: whitespace folded, cut to a width in chars, with what
//! the engine or a terminal put around a person's words taken out.

const REMINDER_OPEN: &str = "<system-reminder>";
const REMINDER_CLOSE: &str = "</system-reminder>";

pub fn fold(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Drops every `<system-reminder>` block; one left open (the record was cut
/// inside it) drops the rest of the text.
pub fn strip_reminders(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(open) = rest.find(REMINDER_OPEN) {
        out.push_str(&rest[..open]);
        let inner = &rest[open + REMINDER_OPEN.len()..];
        match inner.find(REMINDER_CLOSE) {
            Some(close) => rest = &inner[close + REMINDER_CLOSE.len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Drops ANSI escape sequences and control characters but line breaks.
pub fn strip_controls(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        match c {
            '\n' => out.push(c),
            '\t' => out.push(' '),
            c if !c.is_control() => out.push(c),
            _ => {}
        }
    }
    out
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
    fn strip_reminders_keeps_the_text_around_each_block() {
        assert_eq!(
            strip_reminders("a<system-reminder>x</system-reminder>b<system-reminder>y"),
            "ab"
        );
        assert_eq!(strip_reminders("plain"), "plain");
    }

    #[test]
    fn strip_controls_keeps_line_breaks() {
        assert_eq!(
            strip_controls("a\u{1b}[1;31mb\u{1b}[0m\tc\u{7}\nd"),
            "ab c\nd"
        );
    }

    #[test]
    fn fold_joins_every_run_of_whitespace_with_one_space() {
        assert_eq!(fold("  a\n\tb  c "), "a b c");
    }
}

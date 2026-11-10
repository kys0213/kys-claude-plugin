//! Which of a session's prompts a person wrote: one typed in the session, or
//! sent from the board, that is not an event the host wrapped in a tag.

use super::detail::clean;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptFact {
    pub text: String,
    /// `prompt.submit`'s `origin.kind`, or `kanban` for a request the board delivered.
    pub origin: String,
    pub at: u64,
}

const PERSON_ORIGINS: [&str; 4] = ["composer", "bridge", "sdk", "kanban"];

/// `<name ...>…`: how hosts hand a session events (`<task-notification>`,
/// `<side-sessions-event>`) through the same door as a typed prompt.
pub fn is_wrapped_event(text: &str) -> bool {
    let cleaned = clean(text);
    let Some(rest) = cleaned.trim_start().strip_prefix('<') else {
        return false;
    };
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    name_len > 0
        && rest.starts_with(|c: char| c.is_ascii_alphabetic())
        && rest[name_len..].starts_with(|c: char| c == '>' || c.is_whitespace())
}

fn is_person(prompt: &PromptFact) -> bool {
    PERSON_ORIGINS.contains(&prompt.origin.as_str()) && !is_wrapped_event(&prompt.text)
}

/// The newest prompt a person wrote; for a record from before prompts were
/// kept, its last prompt unless that is an event.
pub fn latest_person_prompt<'a>(
    prompts: &'a [PromptFact],
    last_prompt: Option<&'a str>,
) -> Option<&'a str> {
    if !prompts.is_empty() {
        return prompts
            .iter()
            .rev()
            .find(|p| is_person(p))
            .map(|p| p.text.as_str());
    }
    last_prompt.filter(|p| !is_wrapped_event(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_wrapped_text_is_an_event_and_prose_is_not() {
        assert!(is_wrapped_event("<task-notification>x</task-notification>"));
        assert!(is_wrapped_event("  <side-sessions-event kind=\"x\">cut"));
        assert!(!is_wrapped_event("Fix <b> tags"));
        assert!(!is_wrapped_event("< not a tag"));
        assert!(!is_wrapped_event("<3 thanks"));
    }
}

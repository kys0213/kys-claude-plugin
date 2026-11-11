//! Which of a session's prompts count as a person's, by the engine's
//! `origin.kind`. The ledger counts no `sdk` or board prompt as a person's
//! decision, though the board shows both as a person's prompt.

use super::detail::clean;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptFact {
    pub text: String,
    /// `prompt.submit`'s `origin.kind`, or `kanban` for a request the board delivered.
    pub origin: String,
    pub at: u64,
}

const SHOWN_AS_PERSON: [&str; 4] = ["composer", "bridge", "sdk", "kanban"];
/// The `origin.kind`s of what a person typed or sent.
pub const PERSON_ORIGINS: [&str; 2] = ["composer", "bridge"];

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

/// For display: sdk and kanban requests count, events wrapped in a tag do not.
fn shows_as_person_prompt(prompt: &PromptFact) -> bool {
    SHOWN_AS_PERSON.contains(&prompt.origin.as_str()) && !is_wrapped_event(&prompt.text)
}

/// Whether a prompt of this `origin.kind` answering an open decision is a
/// person's decision; a kind this version does not know is not.
pub fn decides_as_person(origin: &str) -> bool {
    PERSON_ORIGINS.contains(&origin)
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
            .find(|p| shows_as_person_prompt(p))
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

/// Prompt role classification and system message filtering.
///
/// Extracted from `projects.rs` to be shared by both the v2 legacy path
/// (`adapt_to_history_entries`) and the v3 indexing pipeline (`extract_prompts`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptRole {
    Human,
    System,
    Meta,
}

impl PromptRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            PromptRole::Human => "human",
            PromptRole::System => "system",
            PromptRole::Meta => "meta",
        }
    }
}

/// Classify a prompt's role after `strip_system_reminders` has been applied.
///
/// - `stripped_text`: the text *after* system-reminder blocks have been removed.
/// - `had_system_reminders`: whether the original text contained any `<system-reminder>` tags.
pub fn classify_prompt_role(stripped_text: &str, had_system_reminders: bool) -> PromptRole {
    let trimmed = stripped_text.trim();
    if trimmed.is_empty() {
        return PromptRole::Meta;
    }
    if is_system_meta_message(trimmed) {
        return PromptRole::System;
    }
    // Original text was entirely system-reminder tags with negligible leftovers
    if had_system_reminders && trimmed.len() < 5 {
        return PromptRole::System;
    }
    PromptRole::Human
}

/// Strip `<system-reminder>...</system-reminder>` blocks from user messages.
pub fn strip_system_reminders(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("<system-reminder>") {
        result.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</system-reminder>") {
            rest = &rest[start + end + "</system-reminder>".len()..];
        } else {
            // Unclosed tag - skip everything after it
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result.trim().to_string()
}

/// Detect messages that are system/meta noise rather than genuine user prompts.
pub fn is_system_meta_message(content: &str) -> bool {
    let trimmed = content.trim();
    let lower = trimmed.to_lowercase();

    // Basic meta filters
    if lower.starts_with("<local-command-")
        || lower.starts_with("<command-name>")
        || lower.contains("[request interrupted by user")
        || trimmed.len() < 3
    {
        return true;
    }

    // Skill/command expansion: starts with "# " and very long
    if trimmed.starts_with("# ") && trimmed.len() > 500 {
        return true;
    }

    // Mode activation prompts
    if lower.contains("[autopilot activated")
        || lower.contains("[ralph loop")
        || lower.contains("[ultrawork activated")
        || lower.contains("[ralplan activated")
        || lower.contains("[ecomode activated")
    {
        return true;
    }

    // Predominantly markdown table content (system docs)
    let line_count = trimmed.lines().count();
    if line_count > 5 {
        let table_lines = trimmed
            .lines()
            .filter(|l| l.trim().starts_with('|') && l.trim().ends_with('|'))
            .count();
        if table_lines as f64 / line_count as f64 > 0.5 {
            return true;
        }
    }

    // YAML frontmatter (command definitions)
    if trimmed.starts_with("---\n") && trimmed.contains("\n---\n") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_system_reminders_basic() {
        let input = "Hello <system-reminder>secret</system-reminder> world";
        assert_eq!(strip_system_reminders(input), "Hello  world");
    }

    #[test]
    fn test_strip_system_reminders_only_reminder() {
        let input = "<system-reminder>all system</system-reminder>";
        assert_eq!(strip_system_reminders(input), "");
    }

    #[test]
    fn test_strip_system_reminders_no_tags() {
        let input = "just a normal message";
        assert_eq!(strip_system_reminders(input), "just a normal message");
    }

    #[test]
    fn test_strip_system_reminders_unclosed() {
        let input = "before <system-reminder>unclosed";
        assert_eq!(strip_system_reminders(input), "before");
    }

    #[test]
    fn test_classify_human() {
        assert_eq!(
            classify_prompt_role("Fix the login bug", false),
            PromptRole::Human
        );
    }

    #[test]
    fn test_classify_meta_empty() {
        assert_eq!(classify_prompt_role("", false), PromptRole::Meta);
        assert_eq!(classify_prompt_role("   ", false), PromptRole::Meta);
    }

    #[test]
    fn test_classify_system_meta_message() {
        assert_eq!(
            classify_prompt_role("<local-command-foo>bar</local-command-foo>", false),
            PromptRole::System
        );
    }

    #[test]
    fn test_classify_system_leftover_after_strip() {
        // Original had system-reminders and only "ok" remained
        assert_eq!(classify_prompt_role("ok", true), PromptRole::System);
    }

    #[test]
    fn test_classify_human_with_reminders_stripped() {
        // Had system-reminders but substantial text remains
        assert_eq!(
            classify_prompt_role("Please review my code changes", true),
            PromptRole::Human
        );
    }

    #[test]
    fn test_is_system_meta_short() {
        assert!(is_system_meta_message("ab"));
        assert!(!is_system_meta_message("abc"));
    }

    #[test]
    fn test_is_system_meta_command() {
        assert!(is_system_meta_message("<command-name>foo</command-name>"));
        assert!(is_system_meta_message(
            "<local-command-run>test</local-command-run>"
        ));
    }

    #[test]
    fn test_is_system_meta_interrupted() {
        assert!(is_system_meta_message(
            "something [request interrupted by user] end"
        ));
    }

    #[test]
    fn test_is_system_meta_mode_activation() {
        assert!(is_system_meta_message("[autopilot activated] starting"));
        assert!(is_system_meta_message("[ralph loop iteration 3]"));
        assert!(is_system_meta_message("[ultrawork activated] go"));
    }

    #[test]
    fn test_is_system_meta_yaml_frontmatter() {
        let yaml = "---\nname: test\n---\ncontent here";
        assert!(is_system_meta_message(yaml));
    }

    #[test]
    fn test_is_system_meta_skill_expansion() {
        let long_heading = format!("# Some Skill\n{}", "x".repeat(500));
        assert!(is_system_meta_message(&long_heading));
    }

    #[test]
    fn test_prompt_role_as_str() {
        assert_eq!(PromptRole::Human.as_str(), "human");
        assert_eq!(PromptRole::System.as_str(), "system");
        assert_eq!(PromptRole::Meta.as_str(), "meta");
    }
}

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

/// Minimum character count for text remaining after system-reminder stripping
/// to be considered genuine human input. Below this threshold, leftovers like
/// "ok" or "y" are treated as system noise when the original contained
/// `<system-reminder>` tags.
const MIN_HUMAN_CHARS_AFTER_STRIP: usize = 5;

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
    if had_system_reminders && trimmed.chars().count() < MIN_HUMAN_CHARS_AFTER_STRIP {
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
///
/// Uses structural heuristics rather than enumerating specific tag names,
/// so new XML-based system messages (e.g., `<session-restore>`) are caught
/// automatically.
pub fn is_system_meta_message(content: &str) -> bool {
    let trimmed = content.trim();

    // Very short messages are not meaningful prompts
    if trimmed.chars().count() < 3 {
        return true;
    }

    // XML-like tags: all system-injected messages use <tag> format.
    // Real user prompts never start with '<'.
    if trimmed.starts_with('<') {
        return true;
    }

    // Bracket-wrapped system messages where the entire content is enclosed:
    // [Request interrupted by user], [autopilot activated], [ralph loop ...].
    // Excludes user prefixes like "[autodev] fix: ..." where text follows "] ".
    if trimmed.starts_with('[') {
        if let Some(close) = trimmed.find(']') {
            // Entire message is bracket-wrapped, or only whitespace follows
            let after_bracket = trimmed[close + 1..].trim();
            if after_bracket.is_empty() {
                return true;
            }
        }
    }

    // Hook feedback prefixes (case-insensitive, first 30 chars for safe slicing)
    let byte_end = trimmed
        .char_indices()
        .map(|(i, _)| i)
        .take(31)
        .last()
        .unwrap_or(trimmed.len());
    let prefix_lower = trimmed[..byte_end].to_lowercase();
    if prefix_lower.starts_with("stop hook feedback:")
        || prefix_lower.starts_with("base directory for this skill:")
    {
        return true;
    }

    // Skill/command expansion: starts with "# " and very long
    if trimmed.starts_with("# ") && trimmed.len() > 500 {
        return true;
    }

    // YAML frontmatter (command definitions)
    if trimmed.starts_with("---\n") && trimmed.contains("\n---\n") {
        return true;
    }

    // Predominantly markdown table content (system docs)
    let mut line_count = 0;
    let mut table_lines = 0;
    for line in trimmed.lines() {
        line_count += 1;
        let l = line.trim();
        if l.starts_with('|') && l.ends_with('|') {
            table_lines += 1;
        }
    }
    if line_count > 5 && table_lines as f64 / line_count as f64 > 0.5 {
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
        assert!(is_system_meta_message(
            "<command-message>git-utils:commit-and-pr</command-message>"
        ));
    }

    #[test]
    fn test_is_system_meta_task_notification() {
        assert!(is_system_meta_message(
            "<task-notification><task-id>abc123</task-id></task-notification>"
        ));
    }

    #[test]
    fn test_is_system_meta_teammate_message() {
        assert!(is_system_meta_message(
            "<teammate-message teammate_id=\"agent-report\" color=\"green\">done</teammate-message>"
        ));
    }

    #[test]
    fn test_is_system_meta_hook_feedback() {
        assert!(is_system_meta_message(
            "Stop hook feedback: [bash ./.claude/hooks/auto-commit-hook.sh]: something"
        ));
    }

    #[test]
    fn test_is_system_meta_interrupted() {
        assert!(is_system_meta_message(
            "[Request interrupted by user for tool use]"
        ));
        assert!(is_system_meta_message("[Request interrupted by user]"));
    }

    #[test]
    fn test_is_system_meta_mode_activation() {
        // In real data, mode activations arrive inside <system-reminder> tags.
        // After strip_system_reminders(), only the bracket-wrapped core remains.
        assert!(is_system_meta_message("[autopilot activated]"));
        assert!(is_system_meta_message("[ralph loop iteration 3]"));
        assert!(is_system_meta_message("[ultrawork activated]"));
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

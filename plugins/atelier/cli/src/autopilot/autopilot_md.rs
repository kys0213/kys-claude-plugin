//! Reader for `github-autopilot.local.md` frontmatter — resolves the PR base
//! branch (`work_branch` > `branch_strategy`). Single source of truth so the
//! creation side (branch-promoter) and any consumer agree (#776); the previous
//! bash duplicated this and miscomputed `draft-develop-main` → `develop`.

use std::path::Path;

const CONFIG_FILE: &str = "github-autopilot.local.md";

/// Resolve the expected PR base branch for an autopilot project directory.
/// A missing or unreadable config resolves to `main` (the `draft-main` default)
/// so callers can consume `$(...)` without error handling.
pub fn resolve_base_branch(project_dir: &Path) -> String {
    match std::fs::read_to_string(project_dir.join(CONFIG_FILE)) {
        Ok(content) => base_branch_from_content(&content),
        Err(_) => "main".to_string(),
    }
}

/// Pure resolution from the config text. `work_branch` (non-empty) wins;
/// otherwise `branch_strategy == "draft-develop-main"` → `develop`; else `main`.
///
/// Only the first YAML frontmatter block (between the opening and closing `---`)
/// is config; the markdown body below may legitimately mention these keys in
/// prose or fenced examples. Keys are matched at column 0, so a nested mapping
/// entry is not mistaken for a top-level field.
pub fn base_branch_from_content(content: &str) -> String {
    let mut work_branch: Option<String> = None;
    let mut branch_strategy: Option<String> = None;

    let mut in_frontmatter = false;
    for line in content.lines() {
        if line.trim_end() == "---" {
            if in_frontmatter {
                break; // closing fence — the rest of the file is body
            }
            in_frontmatter = true;
            continue;
        }
        if !in_frontmatter {
            continue;
        }
        if work_branch.is_none() {
            if let Some(v) = field_value(line, "work_branch") {
                work_branch = Some(v);
                continue;
            }
        }
        if branch_strategy.is_none() {
            if let Some(v) = field_value(line, "branch_strategy") {
                branch_strategy = Some(v);
            }
        }
    }

    if let Some(wb) = work_branch.filter(|v| !v.is_empty()) {
        return wb;
    }
    match branch_strategy.as_deref() {
        Some("draft-develop-main") => "develop".to_string(),
        _ => "main".to_string(),
    }
}

/// Extracts `key: value` — strips an inline `# comment`, surrounding quotes,
/// and whitespace. Returns `None` if the line isn't this key.
fn field_value(line: &str, key: &str) -> Option<String> {
    let rest = line
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix(':')?
        .trim();
    // A quoted value owns everything up to its closing quote, so a `#` inside
    // it (e.g. a branch name) is not mistaken for the start of an inline comment.
    if let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
        if let Some(end) = rest[1..].find(quote) {
            return Some(rest[1..=end].to_string());
        }
    }
    // Unquoted (or an unterminated quote): strip an inline `# comment`.
    let val = rest.split('#').next().unwrap_or("").trim();
    Some(val.trim_matches(['"', '\'']).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::base_branch_from_content as resolve;

    #[test]
    fn work_branch_set_wins_over_strategy() {
        let c = "---\nbranch_strategy: \"draft-develop-main\"\nwork_branch: \"alpha\"\n---";
        assert_eq!(resolve(c), "alpha");
    }

    #[test]
    fn empty_work_branch_falls_back_to_strategy() {
        let c = "---\nwork_branch: \"\"\nbranch_strategy: \"draft-develop-main\"\n---";
        assert_eq!(resolve(c), "develop");
    }

    #[test]
    fn draft_main_resolves_main() {
        let c = "---\nwork_branch: \"\"\nbranch_strategy: \"draft-main\"\n---";
        assert_eq!(resolve(c), "main");
    }

    #[test]
    fn missing_fields_default_main() {
        assert_eq!(resolve("# just a heading\n\nsome body"), "main");
    }

    #[test]
    fn inline_comment_and_unquoted_are_handled() {
        // The documented schema ships inline comments; values may be unquoted.
        let c = "---\nbranch_strategy: draft-develop-main   # draft-develop-main | draft-main\n---";
        assert_eq!(resolve(c), "develop");
    }

    #[test]
    fn first_occurrence_wins() {
        let c = "---\nwork_branch: \"alpha\"\nwork_branch: \"beta\"\n---";
        assert_eq!(resolve(c), "alpha");
    }

    #[test]
    fn hash_inside_quoted_value_is_preserved() {
        // A quoted value owns its `#`; only an unquoted trailing `#` is a comment.
        let c = "---\nwork_branch: \"feat#1\"\n---";
        assert_eq!(resolve(c), "feat#1");
    }

    #[test]
    fn body_after_frontmatter_does_not_leak() {
        // Key absent from frontmatter but mentioned in the markdown body (prose,
        // example, fenced block) must not be read as config.
        let c = "---\nbranch_strategy: \"draft-main\"\n---\n\n# Notes\nwork_branch: alpha\n";
        assert_eq!(resolve(c), "main");
    }

    #[test]
    fn indented_keys_are_not_matched() {
        // A nested mapping entry is not a top-level key.
        let c =
            "---\nsome_map:\n  work_branch: nested\nbranch_strategy: \"draft-develop-main\"\n---";
        assert_eq!(resolve(c), "develop");
    }
}

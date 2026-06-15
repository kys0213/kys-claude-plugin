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
pub fn base_branch_from_content(content: &str) -> String {
    let mut work_branch: Option<String> = None;
    let mut branch_strategy: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
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
    let rest = line.strip_prefix(key)?.trim_start().strip_prefix(':')?;
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
        let c = "branch_strategy: draft-develop-main   # draft-develop-main | draft-main";
        assert_eq!(resolve(c), "develop");
    }

    #[test]
    fn first_occurrence_wins() {
        let c = "work_branch: \"alpha\"\nwork_branch: \"beta\"";
        assert_eq!(resolve(c), "alpha");
    }
}

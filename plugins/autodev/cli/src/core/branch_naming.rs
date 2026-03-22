// Convention-based branch name generation.
//
// Generates branch names following the `branch-naming.md` convention:
// `<type>/<issue-number>-<short-description>`
//
// Type inference priority:
// 1. GitHub labels (e.g., `bug` -> `fix`, `enhancement` -> `feat`, `documentation` -> `docs`)
// 2. Issue title prefix (e.g., `feat:`, `fix:`, `docs:`)
// 3. Fallback to `feat`

/// Supported branch type prefixes.
const TYPES: &[&str] = &[
    "feat", "fix", "refactor", "docs", "chore", "test", "perf", "ci",
];

/// Label-to-type mapping.
const LABEL_MAPPINGS: &[(&str, &str)] = &[
    ("bug", "fix"),
    ("fix", "fix"),
    ("enhancement", "feat"),
    ("feature", "feat"),
    ("feat", "feat"),
    ("documentation", "docs"),
    ("docs", "docs"),
    ("refactor", "refactor"),
    ("refactoring", "refactor"),
    ("chore", "chore"),
    ("test", "test"),
    ("testing", "test"),
    ("perf", "perf"),
    ("performance", "perf"),
    ("ci", "ci"),
];

/// Generate a convention-based branch name from issue title and labels.
///
/// Returns `<type>/<issue-number>-<short-description>` (e.g., `feat/42-add-user-auth`).
pub fn generate_branch_name(issue_number: i64, title: &str, labels: &[String]) -> String {
    let branch_type = infer_type(title, labels);
    let description = to_kebab_description(title);
    format!("{branch_type}/{issue_number}-{description}")
}

/// Sanitize a branch name for use as a worktree directory name.
///
/// Replaces `/` with `-` to avoid nested directory creation.
/// Example: `feat/42-add-user-auth` → `feat-42-add-user-auth`
pub fn sanitize_for_directory(branch_name: &str) -> String {
    branch_name.replace('/', "-")
}

/// Infer branch type from labels first, then title prefix.
fn infer_type(title: &str, labels: &[String]) -> &'static str {
    // 1. Check labels
    for label in labels {
        let lower = label.to_lowercase();
        for &(pattern, branch_type) in LABEL_MAPPINGS {
            if lower == pattern || lower.contains(pattern) {
                return branch_type;
            }
        }
    }

    // 2. Check title prefix (e.g., "feat: ...", "feat(...): ...")
    let lower_title = title.to_lowercase();
    for &t in TYPES {
        if let Some(rest) = lower_title.strip_prefix(t) {
            if rest.starts_with(':') || rest.starts_with('(') {
                return t;
            }
        }
    }

    // 3. Fallback
    "feat"
}

/// Convert title to a kebab-case short description (max 5 words).
fn to_kebab_description(title: &str) -> String {
    // Strip conventional commit prefix if present (e.g., "feat: add user auth" → "add user auth")
    let stripped = strip_prefix(title);

    let words: Vec<String> = stripped
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(5)
        .map(String::from)
        .collect();

    if words.is_empty() {
        "task".to_string()
    } else {
        words.join("-")
    }
}

/// Strip conventional commit prefix from title.
///
/// Examples:
/// - `feat: add user auth` → `add user auth`
/// - `feat(scope): add user auth` → `add user auth`
/// - `plain title` → `plain title`
fn strip_prefix(title: &str) -> &str {
    let lower = title.to_lowercase();
    for &t in TYPES {
        if lower.starts_with(t) {
            let rest = &title[t.len()..];
            // "feat: description" → skip "feat: "
            if let Some(after_colon) = rest.strip_prefix(':') {
                return after_colon.trim_start();
            }
            // "feat(scope): description" → skip "feat(scope): "
            if rest.starts_with('(') {
                if let Some(paren_end) = rest.find("):") {
                    return rest[paren_end + 2..].trim_start();
                }
            }
        }
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── infer_type tests ───

    #[test]
    fn infer_type_from_bug_label() {
        let labels = vec!["bug".to_string()];
        assert_eq!(infer_type("some issue title", &labels), "fix");
    }

    #[test]
    fn infer_type_from_enhancement_label() {
        let labels = vec!["enhancement".to_string()];
        assert_eq!(infer_type("some issue title", &labels), "feat");
    }

    #[test]
    fn infer_type_from_docs_label() {
        let labels = vec!["documentation".to_string()];
        assert_eq!(infer_type("some issue title", &labels), "docs");
    }

    #[test]
    fn infer_type_from_title_prefix() {
        let labels: Vec<String> = vec![];
        assert_eq!(infer_type("fix: resolve login crash", &labels), "fix");
        assert_eq!(infer_type("docs: update readme", &labels), "docs");
        assert_eq!(
            infer_type("refactor(auth): simplify flow", &labels),
            "refactor"
        );
    }

    #[test]
    fn infer_type_fallback_to_feat() {
        let labels: Vec<String> = vec![];
        assert_eq!(infer_type("add user authentication", &labels), "feat");
    }

    #[test]
    fn label_takes_priority_over_title() {
        let labels = vec!["bug".to_string()];
        assert_eq!(infer_type("feat: add new feature", &labels), "fix");
    }

    #[test]
    fn label_partial_match() {
        let labels = vec!["type:bug".to_string()];
        assert_eq!(infer_type("some issue", &labels), "fix");
    }

    // ─── to_kebab_description tests ───

    #[test]
    fn kebab_basic() {
        assert_eq!(to_kebab_description("Add User Auth"), "add-user-auth");
    }

    #[test]
    fn kebab_strips_prefix() {
        assert_eq!(to_kebab_description("feat: add user auth"), "add-user-auth");
    }

    #[test]
    fn kebab_strips_scoped_prefix() {
        assert_eq!(
            to_kebab_description("feat(auth): add user auth"),
            "add-user-auth"
        );
    }

    #[test]
    fn kebab_limits_to_five_words() {
        assert_eq!(
            to_kebab_description("this is a very long title with too many words"),
            "this-is-a-very-long"
        );
    }

    #[test]
    fn kebab_removes_special_chars() {
        assert_eq!(
            to_kebab_description("fix: resolve `token` expiry [check]"),
            "resolve-token-expiry-check"
        );
    }

    #[test]
    fn kebab_empty_title() {
        assert_eq!(to_kebab_description(""), "task");
    }

    #[test]
    fn kebab_only_special_chars() {
        assert_eq!(to_kebab_description("!!!"), "task");
    }

    // ─── generate_branch_name tests ───

    #[test]
    fn generate_feat_branch() {
        let labels: Vec<String> = vec![];
        assert_eq!(
            generate_branch_name(42, "add JWT middleware", &labels),
            "feat/42-add-jwt-middleware"
        );
    }

    #[test]
    fn generate_fix_branch_from_label() {
        let labels = vec!["bug".to_string()];
        assert_eq!(
            generate_branch_name(45, "token expiry check", &labels),
            "fix/45-token-expiry-check"
        );
    }

    #[test]
    fn generate_docs_branch_from_title() {
        let labels: Vec<String> = vec![];
        assert_eq!(
            generate_branch_name(100, "docs: update CLI usage guide", &labels),
            "docs/100-update-cli-usage-guide"
        );
    }

    #[test]
    fn generate_refactor_branch() {
        let labels: Vec<String> = vec![];
        assert_eq!(
            generate_branch_name(48, "refactor: simplify auth flow", &labels),
            "refactor/48-simplify-auth-flow"
        );
    }

    // ─── sanitize_for_directory tests ───

    #[test]
    fn sanitize_replaces_slash() {
        assert_eq!(
            sanitize_for_directory("feat/42-add-user-auth"),
            "feat-42-add-user-auth"
        );
    }

    #[test]
    fn sanitize_no_slash() {
        assert_eq!(sanitize_for_directory("simple-name"), "simple-name");
    }
}

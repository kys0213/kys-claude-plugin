//! Issue dependency analysis and spec auto-linking.
//!
//! Provides pure functions for:
//! 1. Extracting target file/module paths from issue body text
//! 2. Detecting conflicts between issues based on overlapping paths
//! 3. Matching issues to specs by keyword and path overlap
//!
//! These are consumed by the collector to automate spec_issues linking
//! and sequential/parallel processing decisions.

use std::collections::{HashMap, HashSet};

use super::models::{QueuePhase, QueueType, Spec, SpecStatus};
use super::queue_item::QueueItem;
use super::state_queue::StateQueue;

/// Result of dependency analysis for a newly enqueued issue.
#[derive(Debug, Clone)]
pub struct DependencyAnalysis {
    /// Issue number being analyzed.
    pub issue_number: i64,
    /// File/module paths inferred from the issue body.
    pub inferred_paths: Vec<String>,
    /// work_ids of existing queue items that conflict (share paths).
    pub conflicting_work_ids: Vec<String>,
    /// spec_ids that match this issue (for auto-linking).
    pub matching_spec_ids: Vec<String>,
    /// Whether this issue should be processed sequentially (has conflicts).
    pub requires_sequential: bool,
}

/// Extract file/module paths from issue body text.
///
/// Heuristic-based extraction that looks for:
/// - Backtick-quoted paths (e.g., `src/foo/bar.rs`)
/// - Paths with file extensions mentioned in prose
/// - Module references like `mod::submod`
///
/// Returns deduplicated, sorted list of paths.
pub fn extract_paths_from_body(body: &str) -> Vec<String> {
    let mut paths = HashSet::new();

    // Extract backtick-quoted content that looks like file paths
    for segment in body.split('`') {
        // Every other segment (odd indices) is inside backticks in a simple split.
        // But since we can't track index cleanly, just check each segment.
        let trimmed = segment.trim();
        if looks_like_path(trimmed) {
            paths.insert(normalize_path(trimmed));
        }
    }

    // Extract paths from prose lines (lines containing path-like tokens)
    for line in body.lines() {
        for word in line.split_whitespace() {
            let cleaned = word.trim_matches(|c: char| c == ',' || c == '.' || c == ')' || c == '(');
            if looks_like_path(cleaned) && !is_url(cleaned) {
                paths.insert(normalize_path(cleaned));
            }
        }
    }

    let mut result: Vec<String> = paths.into_iter().collect();
    result.sort();
    result
}

/// Check if a string looks like a file/module path.
fn looks_like_path(s: &str) -> bool {
    if s.is_empty() || s.len() < 3 {
        return false;
    }

    // Must contain a slash or a known file extension
    let has_slash = s.contains('/');
    let has_extension = KNOWN_EXTENSIONS
        .iter()
        .any(|ext| s.ends_with(ext) && s.len() > ext.len());

    // Filter out things that are clearly not paths
    if s.contains(' ') || s.contains('\n') {
        return false;
    }

    // Reject pure numbers, URLs, labels
    if s.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }

    has_slash || has_extension
}

/// Known source file extensions.
const KNOWN_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".java", ".toml", ".yaml", ".yml", ".json",
    ".md", ".sql", ".sh", ".css", ".scss", ".html",
];

/// Check if a string looks like a URL.
fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://") || s.starts_with("git@")
}

/// Normalize a path by removing leading `./` and trailing slashes.
fn normalize_path(s: &str) -> String {
    let mut p = s.to_string();
    while p.starts_with("./") {
        p = p[2..].to_string();
    }
    while p.ends_with('/') {
        p.pop();
    }
    p
}

/// Check if two paths overlap (same path, parent-child, or shared directory).
///
/// For file paths: checks if they share a common directory prefix.
/// For module paths: checks if one contains the other.
fn paths_overlap(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }

    // Parent-child check
    let a_is_parent = b.starts_with(a) && b.as_bytes().get(a.len()) == Some(&b'/');
    let b_is_parent = a.starts_with(b) && a.as_bytes().get(b.len()) == Some(&b'/');
    if a_is_parent || b_is_parent {
        return true;
    }

    // Same immediate parent directory (e.g., "src/foo/a.rs" and "src/foo/b.rs")
    if let (Some(dir_a), Some(dir_b)) = (parent_dir(a), parent_dir(b)) {
        return dir_a == dir_b;
    }

    false
}

/// Get parent directory of a path.
fn parent_dir(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

/// Find existing queue items whose inferred paths conflict with the given paths.
///
/// Scans all active (non-done/skipped) items in the queue and compares
/// their issue bodies for overlapping file references.
pub fn find_conflicting_items(
    queue: &StateQueue<QueueItem>,
    new_paths: &[String],
    exclude_work_id: &str,
) -> Vec<String> {
    if new_paths.is_empty() {
        return Vec::new();
    }

    let mut conflicts = Vec::new();

    for phase in &[QueuePhase::Pending, QueuePhase::Ready, QueuePhase::Running] {
        for item in queue.iter(*phase) {
            if item.work_id == exclude_work_id {
                continue;
            }
            if item.queue_type != QueueType::Issue {
                continue;
            }

            let existing_paths = match item.body() {
                Some(body) => extract_paths_from_body(body),
                None => continue,
            };

            if has_path_overlap(new_paths, &existing_paths) {
                conflicts.push(item.work_id.clone());
            }
        }
    }

    conflicts
}

/// Check if two sets of paths have any overlap.
fn has_path_overlap(paths_a: &[String], paths_b: &[String]) -> bool {
    for a in paths_a {
        for b in paths_b {
            if paths_overlap(a, b) {
                return true;
            }
        }
    }
    false
}

/// Match an issue to relevant specs based on:
/// 1. Spec source_path overlapping with issue's inferred paths
/// 2. Spec title/body keywords appearing in issue title/body
///
/// Returns spec IDs that should be linked to this issue.
pub fn find_matching_specs(
    specs: &[Spec],
    issue_title: &str,
    issue_body: Option<&str>,
    inferred_paths: &[String],
) -> Vec<String> {
    let mut matches = Vec::new();
    let issue_text = format!(
        "{} {}",
        issue_title.to_lowercase(),
        issue_body.unwrap_or("").to_lowercase()
    );

    for spec in specs {
        if spec.status != SpecStatus::Active {
            continue;
        }

        // Check 1: source_path overlap
        if let Some(ref source_path) = spec.source_path {
            for path in inferred_paths {
                if paths_overlap(source_path, path) {
                    matches.push(spec.id.clone());
                    break;
                }
            }
            if matches.last().map(|id| id == &spec.id).unwrap_or(false) {
                continue; // already matched
            }
        }

        // Check 2: keyword matching (spec title words in issue text)
        if matches_by_keywords(&spec.title, &issue_text) {
            matches.push(spec.id.clone());
        }
    }

    matches
}

/// Check if significant keywords from spec title appear in the issue text.
///
/// Requires at least 2 non-trivial words from the spec title to match.
fn matches_by_keywords(spec_title: &str, issue_text: &str) -> bool {
    let spec_words: Vec<&str> = spec_title
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() >= 3 && !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();

    if spec_words.is_empty() {
        return false;
    }

    let match_count = spec_words
        .iter()
        .filter(|w| issue_text.contains(&w.to_lowercase()))
        .count();

    // Require at least 2 matching words, or all words if spec title is short
    let threshold = if spec_words.len() <= 2 {
        spec_words.len()
    } else {
        2
    };

    match_count >= threshold
}

/// Common stop words to exclude from keyword matching.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "into", "this", "that", "have", "has", "are", "was",
    "were", "been", "being", "not", "but", "all", "can", "will", "just", "should", "would",
    "could", "may", "add", "fix", "update", "new", "use",
];

/// Perform full dependency analysis for a newly enqueued issue.
///
/// This is the main entry point called by the collector after scanning.
pub fn analyze_issue_dependencies(
    queue: &StateQueue<QueueItem>,
    specs: &[Spec],
    issue: &QueueItem,
    already_linked: &HashMap<String, Vec<i64>>,
) -> DependencyAnalysis {
    let body = issue.body().unwrap_or("");
    let inferred_paths = extract_paths_from_body(body);

    let conflicting_work_ids = find_conflicting_items(queue, &inferred_paths, &issue.work_id);

    let matching_spec_ids: Vec<String> =
        find_matching_specs(specs, &issue.title, issue.body(), &inferred_paths)
            .into_iter()
            .filter(|spec_id| {
                // Skip if already linked
                !already_linked
                    .get(spec_id)
                    .map(|issues| issues.contains(&issue.github_number))
                    .unwrap_or(false)
            })
            .collect();

    let requires_sequential = !conflicting_work_ids.is_empty();

    DependencyAnalysis {
        issue_number: issue.github_number,
        inferred_paths,
        conflicting_work_ids,
        matching_spec_ids,
        requires_sequential,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::SpecStatus;
    use crate::core::phase::TaskKind;
    use crate::core::queue_item::testing::{test_issue, test_repo};
    use crate::core::queue_item::QueueItem;

    #[test]
    fn extract_backtick_paths() {
        let body = "Fix the bug in `src/core/collector.rs` and update `src/cli/mod.rs`";
        let paths = extract_paths_from_body(body);
        assert!(paths.contains(&"src/core/collector.rs".to_string()));
        assert!(paths.contains(&"src/cli/mod.rs".to_string()));
    }

    #[test]
    fn extract_prose_paths() {
        let body = "Changes needed in src/service/daemon/collectors/github.rs for the fix";
        let paths = extract_paths_from_body(body);
        assert!(paths.contains(&"src/service/daemon/collectors/github.rs".to_string()));
    }

    #[test]
    fn extract_ignores_urls() {
        let body = "See https://github.com/org/repo/blob/main/src/lib.rs for details";
        let paths = extract_paths_from_body(body);
        assert!(!paths.iter().any(|p| p.contains("https://")));
    }

    #[test]
    fn extract_normalizes_leading_dot_slash() {
        let body = "Update `./src/main.rs` file";
        let paths = extract_paths_from_body(body);
        assert!(paths.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn extract_deduplicates() {
        let body = "`src/lib.rs` is referenced twice: src/lib.rs";
        let paths = extract_paths_from_body(body);
        assert_eq!(paths.iter().filter(|p| *p == "src/lib.rs").count(), 1);
    }

    #[test]
    fn paths_overlap_same() {
        assert!(paths_overlap("src/core/mod.rs", "src/core/mod.rs"));
    }

    #[test]
    fn paths_overlap_parent_child() {
        assert!(paths_overlap("src/core", "src/core/mod.rs"));
        assert!(paths_overlap("src/core/mod.rs", "src/core"));
    }

    #[test]
    fn paths_overlap_same_directory() {
        assert!(paths_overlap("src/core/a.rs", "src/core/b.rs"));
    }

    #[test]
    fn paths_no_overlap() {
        assert!(!paths_overlap("src/core/a.rs", "src/cli/b.rs"));
    }

    #[test]
    fn find_conflicting_items_detects_overlap() {
        let mut queue: StateQueue<QueueItem> = StateQueue::new();
        let repo = test_repo();

        let existing = QueueItem::new_issue(
            &repo,
            1,
            TaskKind::Analyze,
            "Existing issue".into(),
            Some("Fix `src/core/collector.rs`".into()),
            vec![],
            "user".into(),
        );
        queue.push(QueuePhase::Pending, existing);

        let new_paths = vec!["src/core/collector.rs".to_string()];
        let conflicts = find_conflicting_items(&queue, &new_paths, "issue:org/repo:2");

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0], "issue:org/repo:1");
    }

    #[test]
    fn find_conflicting_items_excludes_self() {
        let mut queue: StateQueue<QueueItem> = StateQueue::new();
        let repo = test_repo();

        let item = QueueItem::new_issue(
            &repo,
            1,
            TaskKind::Analyze,
            "Issue".into(),
            Some("Fix `src/core/collector.rs`".into()),
            vec![],
            "user".into(),
        );
        queue.push(QueuePhase::Pending, item);

        let paths = vec!["src/core/collector.rs".to_string()];
        let conflicts = find_conflicting_items(&queue, &paths, "issue:org/repo:1");

        assert!(conflicts.is_empty());
    }

    #[test]
    fn find_conflicting_items_no_conflict() {
        let mut queue: StateQueue<QueueItem> = StateQueue::new();
        let repo = test_repo();

        let existing = QueueItem::new_issue(
            &repo,
            1,
            TaskKind::Analyze,
            "Existing".into(),
            Some("Fix `src/cli/mod.rs`".into()),
            vec![],
            "user".into(),
        );
        queue.push(QueuePhase::Pending, existing);

        let new_paths = vec!["src/service/daemon.rs".to_string()];
        let conflicts = find_conflicting_items(&queue, &new_paths, "issue:org/repo:2");

        assert!(conflicts.is_empty());
    }

    fn make_spec(id: &str, title: &str, source_path: Option<&str>) -> Spec {
        Spec {
            id: id.to_string(),
            repo_id: "r1".to_string(),
            title: title.to_string(),
            body: String::new(),
            status: SpecStatus::Active,
            source_path: source_path.map(|s| s.to_string()),
            test_commands: None,
            acceptance_criteria: None,
            priority: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn find_matching_specs_by_path() {
        let specs = vec![make_spec(
            "s1",
            "Collector refactor",
            Some("src/core/collector.rs"),
        )];

        let matches = find_matching_specs(
            &specs,
            "Fix collector bug",
            Some("Error in `src/core/collector.rs`"),
            &["src/core/collector.rs".to_string()],
        );

        assert_eq!(matches, vec!["s1"]);
    }

    #[test]
    fn find_matching_specs_by_keywords() {
        let specs = vec![make_spec("s1", "Issue dependency analysis", None)];

        let matches = find_matching_specs(
            &specs,
            "Implement issue dependency detection",
            Some("We need dependency analysis for issues"),
            &[],
        );

        assert_eq!(matches, vec!["s1"]);
    }

    #[test]
    fn find_matching_specs_skips_inactive() {
        let mut spec = make_spec("s1", "Collector refactor", Some("src/core/collector.rs"));
        spec.status = SpecStatus::Paused;

        let matches = find_matching_specs(
            &[spec],
            "Fix collector",
            Some("Error in `src/core/collector.rs`"),
            &["src/core/collector.rs".to_string()],
        );

        assert!(matches.is_empty());
    }

    #[test]
    fn analyze_issue_dependencies_full() {
        let mut queue: StateQueue<QueueItem> = StateQueue::new();
        let repo = test_repo();

        let existing = QueueItem::new_issue(
            &repo,
            1,
            TaskKind::Analyze,
            "Existing".into(),
            Some("Fix `src/core/collector.rs`".into()),
            vec![],
            "user".into(),
        );
        queue.push(QueuePhase::Pending, existing);

        let specs = vec![make_spec(
            "s1",
            "Collector improvement",
            Some("src/core/collector.rs"),
        )];

        let new_issue = QueueItem::new_issue(
            &repo,
            2,
            TaskKind::Analyze,
            "New collector issue".into(),
            Some("Bug in `src/core/collector.rs` line 42".into()),
            vec![],
            "user".into(),
        );

        let analysis = analyze_issue_dependencies(&queue, &specs, &new_issue, &HashMap::new());

        assert_eq!(analysis.issue_number, 2);
        assert!(analysis
            .inferred_paths
            .contains(&"src/core/collector.rs".to_string()));
        assert_eq!(analysis.conflicting_work_ids, vec!["issue:org/repo:1"]);
        assert_eq!(analysis.matching_spec_ids, vec!["s1"]);
        assert!(analysis.requires_sequential);
    }

    #[test]
    fn analyze_issue_dependencies_no_conflicts() {
        let queue: StateQueue<QueueItem> = StateQueue::new();
        let repo = test_repo();
        let specs = vec![make_spec("s1", "CLI improvement", Some("src/cli"))];

        let issue = QueueItem::new_issue(
            &repo,
            5,
            TaskKind::Analyze,
            "Fix service layer".into(),
            Some("Update `src/service/daemon.rs`".into()),
            vec![],
            "user".into(),
        );

        let analysis = analyze_issue_dependencies(&queue, &specs, &issue, &HashMap::new());

        assert!(!analysis.requires_sequential);
        assert!(analysis.conflicting_work_ids.is_empty());
        assert!(analysis.matching_spec_ids.is_empty());
    }

    #[test]
    fn analyze_skips_already_linked_specs() {
        let queue: StateQueue<QueueItem> = StateQueue::new();
        let repo = test_repo();
        let specs = vec![make_spec(
            "s1",
            "Collector refactor",
            Some("src/core/collector.rs"),
        )];

        let issue = QueueItem::new_issue(
            &repo,
            3,
            TaskKind::Analyze,
            "Collector fix".into(),
            Some("Fix `src/core/collector.rs`".into()),
            vec![],
            "user".into(),
        );

        let mut already_linked: HashMap<String, Vec<i64>> = HashMap::new();
        already_linked.insert("s1".to_string(), vec![3]);

        let analysis = analyze_issue_dependencies(&queue, &specs, &issue, &already_linked);

        assert!(analysis.matching_spec_ids.is_empty());
    }

    #[test]
    fn keyword_matching_requires_threshold() {
        // Single short word in spec title should not match
        let specs = vec![make_spec("s1", "Fix", None)];

        let matches = find_matching_specs(&specs, "Fix something", Some("body"), &[]);

        // "Fix" is in STOP_WORDS, so no match
        assert!(matches.is_empty());
    }

    #[test]
    fn keyword_matching_needs_multiple_words() {
        let specs = vec![make_spec("s1", "dependency analysis implementation", None)];

        // Only 1 keyword match should not be enough (threshold is 2)
        let matches =
            find_matching_specs(&specs, "unrelated dependency title", Some("nothing"), &[]);

        // "dependency" matches but only 1 word, threshold is 2
        assert!(matches.is_empty());
    }
}

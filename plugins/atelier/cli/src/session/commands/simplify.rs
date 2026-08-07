//! `session simplify-check` command — decides whether the Stop hook should
//! suggest `/simplify`, and renders the banner when it should.
//!
//! The old shell hook fired whenever the tree was dirty, so it announced work
//! it had no part in and repeated on every turn. The rule here is narrower:
//! speak once per session, only about files *this session* touched, and only
//! when at least one of them is not documentation or configuration.
//!
//! `decide` is pure — all repository and storage access is injected — so the
//! whole rule set is exercised in memory.

use crate::session::commands::baseline as baseline_command;
use crate::session::commands::SessionDeps;
use crate::session::core::baseline::{is_valid_session_id, Baseline};
use std::collections::BTreeSet;

/// How many paths the banner lists before collapsing the rest into a count.
pub const MAX_LISTED_FILES: usize = 10;

/// Extensions that carry no code to simplify. A session that only touched
/// these has nothing for `/simplify` to review.
const DOC_EXTENSIONS: &[&str] = &[
    "md", "mdx", "txt", "rst", "adoc", "json", "yaml", "yml", "toml", "ini", "lock",
];

/// Extensionless (or dot-prefixed) names in the same category.
const DOC_FILENAMES: &[&str] = &["LICENSE", ".gitignore"];

/// Everything the decision depends on. Assembled by the caller from the hook
/// payload, the stored baseline and the repository.
pub struct SimplifyInput {
    pub session_id: String,
    pub baseline: Option<Baseline>,
    /// Current `HEAD` — carried for diagnostics; the commit contribution is
    /// already resolved into `committed`.
    pub head: Option<String>,
    /// Paths currently dirty (`git status --porcelain`).
    pub dirty: BTreeSet<String>,
    /// Paths changed by commits made since the baseline `HEAD`.
    pub committed: BTreeSet<String>,
}

/// Why the hook stayed silent. Distinct variants so the reasoning is legible
/// in tests and future logging, even though all of them print nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SilentReason {
    /// No session id on stdin — session attribution is impossible.
    NoSessionId,
    /// Nothing recorded for this session, so every dirty file may predate it.
    NoBaseline,
    AlreadyNotified,
    /// The session added nothing beyond what was already there.
    NoSessionChanges,
    /// Only docs and config changed — nothing for `/simplify` to review.
    DocsOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimplifyDecision {
    Silent(SilentReason),
    Notify {
        /// Up to `MAX_LISTED_FILES` paths, sorted, for the banner.
        files: Vec<String>,
        /// Full count of session-changed files.
        total: usize,
    },
}

/// True for paths whose content `/simplify` has nothing to say about.
pub fn is_docs_or_config(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if DOC_FILENAMES.contains(&name) {
        return true;
    }
    // `rsplit_once` rather than `Path::extension` so dotfiles like
    // `.gitignore` are not mistaken for an extension-only name.
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            DOC_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
        }
        _ => false,
    }
}

/// The session's contribution: files dirty now that were not dirty at session
/// start, plus files the session committed.
fn session_changes(baseline: &Baseline, input: &SimplifyInput) -> BTreeSet<String> {
    input
        .dirty
        .difference(&baseline.dirty)
        .chain(input.committed.iter())
        .cloned()
        .collect()
}

/// Pure decision: same input, same answer.
pub fn decide(input: SimplifyInput) -> SimplifyDecision {
    if input.session_id.trim().is_empty() {
        return SimplifyDecision::Silent(SilentReason::NoSessionId);
    }
    let Some(baseline) = input.baseline.clone() else {
        return SimplifyDecision::Silent(SilentReason::NoBaseline);
    };
    if baseline.notified {
        return SimplifyDecision::Silent(SilentReason::AlreadyNotified);
    }

    let changes = session_changes(&baseline, &input);
    if changes.is_empty() {
        return SimplifyDecision::Silent(SilentReason::NoSessionChanges);
    }
    if changes.iter().all(|p| is_docs_or_config(p)) {
        return SimplifyDecision::Silent(SilentReason::DocsOnly);
    }

    SimplifyDecision::Notify {
        total: changes.len(),
        files: changes.into_iter().take(MAX_LISTED_FILES).collect(),
    }
}

/// Renders the suggestion banner. Divider and title are unchanged from the
/// shell hook; the count sentence now states what is actually counted.
pub fn render_banner(files: &[String], total: usize) -> String {
    const DIVIDER: &str = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";
    let mut listed: Vec<String> = files.iter().map(|f| format!("    {f}")).collect();
    if total > files.len() {
        listed.push(format!("    ... 외 {}개", total - files.len()));
    }
    format!(
        "\n{DIVIDER}\n[coding-style] /simplify 검토 제안\n{DIVIDER}\n\n\
         이번 세션에서 {total}개 파일을 변경했습니다.\n\
         (세션 시작 이후의 커밋 + 작업 트리 변경만 집계합니다)\n\
         작업을 마무리하기 전에 /simplify 를 실행하여\n\
         코드 재사용성, 품질, 효율성을 검토해 보세요.\n\n\
         \x20 변경 파일:\n{}\n\n{DIVIDER}\n",
        listed.join("\n")
    )
}

/// Gathers the decision inputs, decides, and records the session as notified
/// so the banner appears once. Returns the decision; printing is the CLI
/// edge's job (one place, so #725 has a single call site to change).
pub fn run(deps: &SessionDeps, session_id: &str) -> SimplifyDecision {
    if !is_valid_session_id(session_id) {
        return SimplifyDecision::Silent(SilentReason::NoSessionId);
    }
    let Some(baseline) = deps.store.load(session_id) else {
        // Self-healing: a session that started before the plugin was installed
        // has no baseline. Anchor from here and stay silent this turn.
        baseline_command::run(deps, session_id);
        return SimplifyDecision::Silent(SilentReason::NoBaseline);
    };

    let committed = baseline
        .head
        .as_deref()
        .map(|head| deps.repo.files_changed_since(head))
        .unwrap_or_default();
    let decision = decide(SimplifyInput {
        session_id: session_id.to_string(),
        baseline: Some(baseline.clone()),
        head: deps.repo.head(),
        dirty: deps.repo.dirty_files(),
        committed,
    });

    if matches!(decision, SimplifyDecision::Notify { .. }) {
        let mut notified = baseline;
        notified.mark_notified();
        // A failed write only costs a repeated banner — never a blocked Stop.
        let _ = deps.store.save(session_id, &notified);
    }
    decision
}

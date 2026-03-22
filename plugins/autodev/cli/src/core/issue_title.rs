//! Issue 타이틀 기반 자동 분류 모듈.
//!
//! `<type>: <설명>` 형식의 이슈 타이틀에서 type을 파싱하여
//! autodev의 `TaskKind`로 매핑한다.
//!
//! ## 매핑 규칙
//! - `bug:` → TaskKind::Analyze (fix 목적의 분석)
//! - `fix:` → TaskKind::Analyze (fix 목적의 분석)
//! - `feat:` → TaskKind::Implement (구현)
//! - `docs:` → TaskKind::Analyze (문서 분석)
//! - `refactor:` → TaskKind::Analyze (리팩토링 분석)
//! - `chore:` → TaskKind::Analyze (기본 분석)
//! - 파싱 실패 → TaskKind::Analyze (기본값)

use std::fmt;

use super::phase::TaskKind;

/// 이슈 타이틀에서 파싱된 type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueType {
    Bug,
    Feat,
    Fix,
    Refactor,
    Docs,
    Chore,
}

impl IssueType {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueType::Bug => "bug",
            IssueType::Feat => "feat",
            IssueType::Fix => "fix",
            IssueType::Refactor => "refactor",
            IssueType::Docs => "docs",
            IssueType::Chore => "chore",
        }
    }

    /// IssueType을 autodev TaskKind로 변환한다.
    ///
    /// - `bug`, `fix` → Analyze (수정 방안 분석 후 구현으로 이어짐)
    /// - `feat` → Implement (바로 구현 단계로 진입)
    /// - `docs`, `refactor`, `chore` → Analyze (분석 먼저)
    pub fn to_task_kind(self) -> TaskKind {
        match self {
            IssueType::Bug => TaskKind::Analyze,
            IssueType::Fix => TaskKind::Analyze,
            IssueType::Feat => TaskKind::Implement,
            IssueType::Docs => TaskKind::Analyze,
            IssueType::Refactor => TaskKind::Analyze,
            IssueType::Chore => TaskKind::Analyze,
        }
    }
}

impl fmt::Display for IssueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 이슈 타이틀에서 `<type>: <설명>` 패턴을 파싱한다.
///
/// 콜론 뒤 공백은 선택적이다. type은 대소문자 무시.
/// 파싱 실패 시 `None`을 반환한다.
pub fn parse_issue_type(title: &str) -> Option<IssueType> {
    let colon_pos = title.find(':')?;
    let type_str = title[..colon_pos].trim().to_lowercase();

    match type_str.as_str() {
        "bug" => Some(IssueType::Bug),
        "feat" => Some(IssueType::Feat),
        "fix" => Some(IssueType::Fix),
        "refactor" => Some(IssueType::Refactor),
        "docs" => Some(IssueType::Docs),
        "chore" => Some(IssueType::Chore),
        _ => None,
    }
}

/// 이슈 타이틀에서 TaskKind를 결정한다.
///
/// 타이틀이 `<type>: <설명>` 형식이면 type에 따라 TaskKind를 결정하고,
/// 파싱 실패 시 기본값 `TaskKind::Analyze`를 반환한다.
pub fn task_kind_from_title(title: &str) -> TaskKind {
    parse_issue_type(title)
        .map(|t| t.to_task_kind())
        .unwrap_or(TaskKind::Analyze)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── parse_issue_type tests ───

    #[test]
    fn parse_bug_type() {
        assert_eq!(
            parse_issue_type("bug: something broken"),
            Some(IssueType::Bug)
        );
    }

    #[test]
    fn parse_feat_type() {
        assert_eq!(
            parse_issue_type("feat: add new feature"),
            Some(IssueType::Feat)
        );
    }

    #[test]
    fn parse_fix_type() {
        assert_eq!(parse_issue_type("fix: resolve crash"), Some(IssueType::Fix));
    }

    #[test]
    fn parse_refactor_type() {
        assert_eq!(
            parse_issue_type("refactor: simplify logic"),
            Some(IssueType::Refactor)
        );
    }

    #[test]
    fn parse_docs_type() {
        assert_eq!(
            parse_issue_type("docs: update readme"),
            Some(IssueType::Docs)
        );
    }

    #[test]
    fn parse_chore_type() {
        assert_eq!(
            parse_issue_type("chore: update deps"),
            Some(IssueType::Chore)
        );
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(parse_issue_type("BUG: uppercase"), Some(IssueType::Bug));
        assert_eq!(parse_issue_type("Feat: mixed case"), Some(IssueType::Feat));
    }

    #[test]
    fn parse_no_space_after_colon() {
        assert_eq!(parse_issue_type("bug:no space"), Some(IssueType::Bug));
    }

    #[test]
    fn parse_extra_spaces() {
        assert_eq!(parse_issue_type("  bug  : spaced"), Some(IssueType::Bug));
    }

    #[test]
    fn parse_unknown_type_returns_none() {
        assert_eq!(parse_issue_type("unknown: something"), None);
    }

    #[test]
    fn parse_no_colon_returns_none() {
        assert_eq!(parse_issue_type("just a title"), None);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_issue_type(""), None);
    }

    // ─── task_kind_from_title tests ───

    #[test]
    fn bug_maps_to_analyze() {
        assert_eq!(
            task_kind_from_title("bug: crash on startup"),
            TaskKind::Analyze
        );
    }

    #[test]
    fn fix_maps_to_analyze() {
        assert_eq!(
            task_kind_from_title("fix: resolve path issue"),
            TaskKind::Analyze
        );
    }

    #[test]
    fn feat_maps_to_implement() {
        assert_eq!(
            task_kind_from_title("feat: add multi-LLM support"),
            TaskKind::Implement
        );
    }

    #[test]
    fn docs_maps_to_analyze() {
        assert_eq!(
            task_kind_from_title("docs: update architecture"),
            TaskKind::Analyze
        );
    }

    #[test]
    fn refactor_maps_to_analyze() {
        assert_eq!(
            task_kind_from_title("refactor: simplify config"),
            TaskKind::Analyze
        );
    }

    #[test]
    fn chore_maps_to_analyze() {
        assert_eq!(
            task_kind_from_title("chore: update deps"),
            TaskKind::Analyze
        );
    }

    #[test]
    fn unparseable_defaults_to_analyze() {
        assert_eq!(
            task_kind_from_title("just a regular title"),
            TaskKind::Analyze
        );
    }

    // ─── IssueType display ───

    #[test]
    fn issue_type_display() {
        assert_eq!(IssueType::Bug.to_string(), "bug");
        assert_eq!(IssueType::Feat.to_string(), "feat");
        assert_eq!(IssueType::Fix.to_string(), "fix");
        assert_eq!(IssueType::Refactor.to_string(), "refactor");
        assert_eq!(IssueType::Docs.to_string(), "docs");
        assert_eq!(IssueType::Chore.to_string(), "chore");
    }

    // ─── IssueType::to_task_kind exhaustive ───

    #[test]
    fn all_issue_types_have_task_kind_mapping() {
        // Ensure every variant maps to a valid TaskKind
        let types = [
            IssueType::Bug,
            IssueType::Feat,
            IssueType::Fix,
            IssueType::Refactor,
            IssueType::Docs,
            IssueType::Chore,
        ];
        for t in types {
            let _ = t.to_task_kind(); // should not panic
        }
    }
}

pub mod mock;
pub mod real;

use anyhow::Result;
use async_trait::async_trait;

pub use real::RealGh;

/// GitHub CLI (`gh`) 추상화
#[async_trait]
pub trait Gh: Send + Sync {
    /// `gh api repos/{repo}/{path} --jq {jq}` — 단일 필드 조회
    /// API 실패 시 None 반환 (best effort)
    async fn api_get_field(
        &self,
        repo_name: &str,
        path: &str,
        jq: &str,
        host: Option<&str>,
    ) -> Option<String>;

    /// `gh api repos/{repo}/{endpoint} --paginate` — 페이지네이션 조회
    /// params: [("state", "open"), ("sort", "updated"), ...]
    async fn api_paginate(
        &self,
        repo_name: &str,
        endpoint: &str,
        params: &[(&str, &str)],
        host: Option<&str>,
    ) -> Result<Vec<u8>>;

    /// `gh issue comment {number} --repo {repo} --body {body}`
    /// 실패해도 계속 진행 (best effort)
    async fn issue_comment(
        &self,
        repo_name: &str,
        number: i64,
        body: &str,
        host: Option<&str>,
    ) -> bool;

    /// `gh api repos/{repo}/issues/{number}/labels/{label} --method DELETE`
    /// 라벨 제거 (best effort)
    async fn label_remove(
        &self,
        repo_name: &str,
        number: i64,
        label: &str,
        host: Option<&str>,
    ) -> bool;

    /// `gh api repos/{repo}/issues/{number}/labels --method POST`
    /// 라벨 추가 (best effort)
    async fn label_add(
        &self,
        repo_name: &str,
        number: i64,
        label: &str,
        host: Option<&str>,
    ) -> bool;

    /// `gh api repos/{repo}/issues --method POST`
    /// 이슈 생성 (knowledge extraction daily report 등에 사용)
    async fn create_issue(
        &self,
        repo_name: &str,
        title: &str,
        body: &str,
        host: Option<&str>,
    ) -> bool;

    /// `gh api repos/{repo}/pulls --method POST`
    /// PR 생성 (knowledge suggestion PR 등에 사용)
    /// 성공 시 PR 번호를 반환, 실패 시 None
    async fn create_pr(
        &self,
        repo_name: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
        host: Option<&str>,
    ) -> Option<i64>;

    /// `gh pr review {number}` — PR 리뷰 제출
    /// event: `"APPROVE"` | `"REQUEST_CHANGES"` | `"COMMENT"`
    /// GitHub PR UI에서 Approved / Changes Requested 상태를 설정한다.
    async fn pr_review(
        &self,
        repo_name: &str,
        number: i64,
        event: &str,
        body: &str,
        host: Option<&str>,
    ) -> bool;
}

use crate::infrastructure::gh::Gh;

/// GitHub 상태 확인 + 댓글 관리 — Gh trait 주입받아 동작
pub struct Notifier<'a> {
    gh: &'a dyn Gh,
}

impl<'a> Notifier<'a> {
    pub fn new(gh: &'a dyn Gh) -> Self {
        Self { gh }
    }

    /// Issue가 아직 open 상태인지 확인 (API 실패 시 true — best effort)
    pub async fn is_issue_open(&self, repo_name: &str, number: i64, host: Option<&str>) -> bool {
        match self
            .gh
            .api_get_field(repo_name, &format!("issues/{number}"), ".state", host)
            .await
        {
            Some(state) => state == "open",
            None => true,
        }
    }

    /// PR이 리뷰 대상인지 확인 (open + APPROVED 리뷰 없음)
    pub async fn is_pr_reviewable(
        &self,
        repo_name: &str,
        number: i64,
        host: Option<&str>,
    ) -> bool {
        match self
            .gh
            .api_get_field(repo_name, &format!("pulls/{number}"), ".state", host)
            .await
        {
            Some(state) if state != "open" => return false,
            None => return true,
            _ => {}
        }

        let jq = r#"[.[] | select(.state == "APPROVED")] | length"#;
        match self
            .gh
            .api_get_field(
                repo_name,
                &format!("pulls/{number}/reviews"),
                jq,
                host,
            )
            .await
        {
            Some(count) => count.parse::<i64>().unwrap_or(0) == 0,
            None => true,
        }
    }

    /// PR이 머지 가능한 상태인지 확인 (open + not merged)
    pub async fn is_pr_mergeable(
        &self,
        repo_name: &str,
        number: i64,
        host: Option<&str>,
    ) -> bool {
        match self
            .gh
            .api_get_field(repo_name, &format!("pulls/{number}"), ".state", host)
            .await
        {
            Some(state) => state == "open",
            None => true,
        }
    }

    /// 이슈에 댓글 게시 (best effort)
    pub async fn post_issue_comment(
        &self,
        repo_name: &str,
        number: i64,
        body: &str,
        host: Option<&str>,
    ) -> bool {
        self.gh.issue_comment(repo_name, number, body, host).await
    }
}

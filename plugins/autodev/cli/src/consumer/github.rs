/// Consumer pre-flight: GitHub 상태를 확인하여 불필요한 처리 방지
/// API 실패 시 true 반환 (처리 계속 — best effort)

/// Issue가 아직 open 상태인지 확인
pub async fn is_issue_open(repo_name: &str, number: i64, gh_host: Option<&str>) -> bool {
    match gh_get_field(repo_name, &format!("issues/{number}"), ".state", gh_host).await {
        Some(state) => state == "open",
        None => true,
    }
}

/// PR이 리뷰 대상인지 확인 (open + APPROVED 리뷰 없음)
pub async fn is_pr_reviewable(repo_name: &str, number: i64, gh_host: Option<&str>) -> bool {
    match gh_get_field(repo_name, &format!("pulls/{number}"), ".state", gh_host).await {
        Some(state) if state != "open" => return false,
        None => return true,
        _ => {}
    }

    let jq = r#"[.[] | select(.state == "APPROVED")] | length"#;
    match gh_get_field(repo_name, &format!("pulls/{number}/reviews"), jq, gh_host).await {
        Some(count) => count.parse::<i64>().unwrap_or(0) == 0,
        None => true,
    }
}

/// PR이 머지 가능한 상태인지 확인 (open + not merged)
pub async fn is_pr_mergeable(repo_name: &str, number: i64, gh_host: Option<&str>) -> bool {
    match gh_get_field(repo_name, &format!("pulls/{number}"), ".state", gh_host).await {
        Some(state) => state == "open",
        None => true,
    }
}

/// 이슈에 댓글 게시 (best effort — 실패해도 계속 진행)
pub async fn post_issue_comment(repo_name: &str, number: i64, body: &str, gh_host: Option<&str>) -> bool {
    let mut args = vec![
        "issue".to_string(),
        "comment".to_string(),
        number.to_string(),
        "--repo".to_string(),
        repo_name.to_string(),
        "--body".to_string(),
        body.to_string(),
    ];

    if let Some(host) = gh_host {
        args.push("--hostname".to_string());
        args.push(host.to_string());
    }

    match tokio::process::Command::new("gh").args(&args).output().await {
        Ok(output) => {
            if !output.status.success() {
                tracing::warn!("gh issue comment failed for {repo_name}#{number}");
            }
            output.status.success()
        }
        Err(e) => {
            tracing::warn!("gh issue comment error: {e}");
            false
        }
    }
}

async fn gh_get_field(
    repo_name: &str,
    path: &str,
    jq: &str,
    gh_host: Option<&str>,
) -> Option<String> {
    let mut args = vec![
        "api".to_string(),
        format!("repos/{repo_name}/{path}"),
        "--jq".to_string(),
        jq.to_string(),
    ];

    if let Some(host) = gh_host {
        args.push("--hostname".to_string());
        args.push(host.to_string());
    }

    let output = tokio::process::Command::new("gh")
        .args(&args)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

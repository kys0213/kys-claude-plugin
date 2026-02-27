use anyhow::Result;
use serde::Deserialize;

use crate::domain::labels;
use crate::domain::repository::*;
use crate::infrastructure::gh::Gh;
use crate::queue::task_queues::{
    make_work_id, merge_phase, pr_phase, MergeItem, PrItem, TaskQueues,
};
use crate::queue::Database;

#[derive(Debug, Deserialize)]
struct GitHubPR {
    number: i64,
    title: String,
    #[allow(dead_code)]
    body: Option<String>,
    user: GitHubUser,
    head: GitHubBranch,
    base: GitHubBranch,
    updated_at: String,
    #[serde(default)]
    labels: Vec<GitHubLabel>,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GitHubBranch {
    #[serde(rename = "ref")]
    ref_name: String,
}

#[derive(Debug, Deserialize)]
struct GitHubLabel {
    name: String,
}

/// autodev: 라벨이 있는지 확인 (done/skip/wip)
fn has_autodev_label(pr_labels: &[GitHubLabel]) -> bool {
    pr_labels.iter().any(|l| l.name.starts_with("autodev:"))
}

/// GitHub PRs를 스캔하여 TaskQueues에 추가 + autodev:wip 라벨 설정
#[allow(clippy::too_many_arguments)]
pub async fn scan(
    db: &Database,
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    ignore_authors: &[String],
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    let since = db.cursor_get_last_seen(repo_id, "pulls")?;

    let mut params: Vec<(&str, &str)> = vec![
        ("state", "open"),
        ("sort", "updated"),
        ("direction", "desc"),
        ("per_page", "30"),
    ];

    let since_owned;
    if let Some(ref s) = since {
        since_owned = s.clone();
        params.push(("since", &since_owned));
    }

    let stdout = gh
        .api_paginate(repo_name, "pulls", &params, gh_host)
        .await?;

    let prs: Vec<GitHubPR> = serde_json::from_slice(&stdout)?;
    let mut latest_updated = since;

    for pr in &prs {
        if let Some(ref s) = latest_updated {
            if pr.updated_at <= *s {
                continue;
            }
        }

        if ignore_authors.contains(&pr.user.login) {
            continue;
        }

        // autodev: 라벨이 이미 있으면 skip (done/skip/wip 모두)
        if has_autodev_label(&pr.labels) {
            if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                latest_updated = Some(pr.updated_at.clone());
            }
            continue;
        }

        let work_id = make_work_id("pr", repo_name, pr.number);

        // 이미 큐에 있으면 skip (O(1) dedup)
        if queues.contains(&work_id) {
            if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
                latest_updated = Some(pr.updated_at.clone());
            }
            continue;
        }

        let item = PrItem {
            work_id,
            repo_id: repo_id.to_string(),
            repo_name: repo_name.to_string(),
            repo_url: repo_url.to_string(),
            github_number: pr.number,
            title: pr.title.clone(),
            head_branch: pr.head.ref_name.clone(),
            base_branch: pr.base.ref_name.clone(),
            review_comment: None,
            source_issue_number: None,
            review_iteration: 0,
            gh_host: gh_host.map(String::from),
        };

        // autodev:wip 라벨 추가 + 큐에 push
        gh.label_add(repo_name, pr.number, labels::WIP, gh_host)
            .await;
        queues.prs.push(pr_phase::PENDING, item);
        tracing::info!("queued PR #{}: {}", pr.number, pr.title);

        if latest_updated.as_ref().is_none_or(|l| pr.updated_at > *l) {
            latest_updated = Some(pr.updated_at.clone());
        }
    }

    if let Some(last_seen) = latest_updated {
        db.cursor_upsert(repo_id, "pulls", &last_seen)?;
    }

    Ok(())
}

/// autodev:done 라벨이 붙은 open PR 중 approved 상태인 것을 merge queue에 적재
///
/// PR review → done 후 다음 scan cycle에서 merge 대상으로 발견된다.
/// `auto_merge: true` 설정이 있는 레포에서만 호출되어야 한다.
pub async fn scan_merges(
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    // autodev:done 라벨이 붙은 open PR 조회
    // issues endpoint를 사용하면 label 필터링이 가능 (pulls endpoint는 불가)
    let params: Vec<(&str, &str)> = vec![
        ("state", "open"),
        ("labels", labels::DONE),
        ("per_page", "30"),
    ];

    let stdout = gh
        .api_paginate(repo_name, "issues", &params, gh_host)
        .await?;

    let items: Vec<serde_json::Value> = serde_json::from_slice(&stdout)?;

    for item in &items {
        // issues endpoint에서 PR만 필터 (pull_request 필드가 있으면 PR)
        if item.get("pull_request").is_none() {
            continue;
        }

        let number = match item["number"].as_i64() {
            Some(n) if n > 0 => n,
            _ => continue,
        };

        let merge_work_id = make_work_id("merge", repo_name, number);

        // 이미 merge queue에 있으면 skip
        if queues.contains(&merge_work_id) {
            continue;
        }

        // PR 상세 정보 조회 (head/base branch 필요)
        let pr_params: Vec<(&str, &str)> = vec![];
        let pr_data = gh
            .api_paginate(repo_name, &format!("pulls/{number}"), &pr_params, gh_host)
            .await;

        let (head_branch, base_branch, title) = match pr_data {
            Ok(data) => {
                let pr: serde_json::Value =
                    serde_json::from_slice(&data).unwrap_or(serde_json::Value::Null);
                (
                    pr["head"]["ref"].as_str().unwrap_or("").to_string(),
                    pr["base"]["ref"].as_str().unwrap_or("main").to_string(),
                    pr["title"].as_str().unwrap_or("").to_string(),
                )
            }
            Err(_) => continue,
        };

        let merge_item = MergeItem {
            work_id: merge_work_id,
            repo_id: repo_id.to_string(),
            repo_name: repo_name.to_string(),
            repo_url: repo_url.to_string(),
            pr_number: number,
            title,
            head_branch,
            base_branch,
            gh_host: gh_host.map(String::from),
        };

        // done → wip 라벨 전환 + merge queue push
        gh.label_remove(repo_name, number, labels::DONE, gh_host)
            .await;
        gh.label_add(repo_name, number, labels::WIP, gh_host).await;
        queues.merges.push(merge_phase::PENDING, merge_item);
        tracing::info!("queued merge PR #{number}");
    }

    Ok(())
}

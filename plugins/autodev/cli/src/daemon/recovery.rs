use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::Result;

use crate::config;
use crate::config::Env;
use crate::infrastructure::gh::Gh;
use crate::queue::models::{EnabledRepo, RepoIssue, RepoPull, ResolvedRepo};
use crate::queue::task_queues::{labels, make_work_id, TaskQueues};

/// gh_host 캐시 엔트리: 설정 파일의 mtime과 해석된 값을 보관
struct GhHostEntry {
    mtime: Option<SystemTime>,
    value: Option<String>,
}

static GH_HOST_CACHE: Mutex<Option<HashMap<String, GhHostEntry>>> = Mutex::new(None);

/// 설정 파일의 mtime을 조회 (파일 없으면 None)
fn config_mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Per-repo config에서 gh_host를 해석하는 내부 헬퍼.
///
/// 설정 파일의 mtime이 변경되지 않았으면 캐시된 값을 반환하여
/// 매 tick마다 불필요한 디스크 I/O를 회피한다.
fn resolve_gh_host(env: &dyn Env, repo_name: &str) -> Option<String> {
    let ws_path = config::workspaces_path(env).join(config::sanitize_repo_name(repo_name));
    let config_path = ws_path.join(".develop-workflow.yaml");
    let current_mtime = config_mtime(&config_path);

    // 캐시 조회: mtime 일치하면 재사용
    {
        let guard = GH_HOST_CACHE.lock().unwrap();
        if let Some(ref cache) = *guard {
            if let Some(entry) = cache.get(repo_name) {
                if entry.mtime == current_mtime {
                    return entry.value.clone();
                }
            }
        }
    }

    // 캐시 미스 또는 mtime 변경 → 디스크에서 로드
    let cfg = config::loader::load_merged(
        env,
        if ws_path.exists() {
            Some(ws_path.as_path())
        } else {
            None
        },
    );
    let value = cfg.consumer.gh_host;

    // 캐시 갱신
    {
        let mut guard = GH_HOST_CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(
            repo_name.to_string(),
            GhHostEntry {
                mtime: current_mtime,
                value: value.clone(),
            },
        );
    }

    value
}

/// EnabledRepo 목록을 ResolvedRepo 목록으로 변환하는 팩토리.
///
/// 각 레포의 per-repo config에서 gh_host를 해석하고,
/// GitHub API를 통해 open issues/pulls를 pre-fetch하여 value object에 내장한다.
/// mtime 기반 캐시를 내부적으로 사용하므로 tick마다 호출해도 디스크 I/O가 최소화된다.
pub(crate) async fn resolve_repos(
    repos: &[EnabledRepo],
    env: &dyn Env,
    gh: &dyn Gh,
) -> Vec<ResolvedRepo> {
    let mut resolved = Vec::with_capacity(repos.len());

    for r in repos {
        let gh_host = resolve_gh_host(env, &r.name);

        // Open issues pre-fetch (GitHub issues API에는 PR도 포함 — RepoIssue::from_json이 필터링)
        let issues = match gh
            .api_paginate(
                &r.name,
                "issues",
                &[("state", "open"), ("per_page", "100")],
                gh_host.as_deref(),
            )
            .await
        {
            Ok(data) => {
                let raw: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();
                raw.iter().filter_map(RepoIssue::from_json).collect()
            }
            Err(e) => {
                tracing::warn!("failed to fetch issues for {}: {e}", r.name);
                Vec::new()
            }
        };

        // Open pulls pre-fetch (full PR data: head/base branch 포함)
        let pulls = match gh
            .api_paginate(
                &r.name,
                "pulls",
                &[("state", "open"), ("per_page", "100")],
                gh_host.as_deref(),
            )
            .await
        {
            Ok(data) => {
                let raw: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();
                raw.iter().filter_map(RepoPull::from_json).collect()
            }
            Err(e) => {
                tracing::warn!("failed to fetch pulls for {}: {e}", r.name);
                Vec::new()
            }
        };

        resolved.push(ResolvedRepo {
            id: r.id.clone(),
            url: r.url.clone(),
            name: r.name.clone(),
            gh_host,
            issues,
            pulls,
        });
    }

    resolved
}

/// Orphan `autodev:wip` 라벨 정리
///
/// 크래시로 인해 `autodev:wip` 라벨이 남아있지만 메모리 큐에 없는 항목을 찾아
/// 라벨을 제거한다. 다음 scan에서 자연스럽게 재발견되어 재처리된다.
pub async fn recover_orphan_wip(
    repos: &[ResolvedRepo],
    gh: &dyn Gh,
    queues: &TaskQueues,
) -> Result<u64> {
    let mut recovered = 0u64;

    for repo in repos {
        let gh_host = repo.gh_host();

        // Issues with wip label
        for issue in repo.issues.iter().filter(|i| i.is_wip()) {
            let work_id = make_work_id("issue", &repo.name, issue.number);
            if !queues.contains(&work_id)
                && gh
                    .label_remove(&repo.name, issue.number, labels::WIP, gh_host)
                    .await
            {
                recovered += 1;
                tracing::info!(
                    "recovered orphan issue #{} in {} (removed autodev:wip)",
                    issue.number,
                    repo.name
                );
            }
        }

        // PRs with wip label
        for pull in repo.pulls.iter().filter(|p| p.is_wip()) {
            let work_id = make_work_id("pr", &repo.name, pull.number);
            if !queues.contains(&work_id)
                && gh
                    .label_remove(&repo.name, pull.number, labels::WIP, gh_host)
                    .await
            {
                recovered += 1;
                tracing::info!(
                    "recovered orphan pr #{} in {} (removed autodev:wip)",
                    pull.number,
                    repo.name
                );
            }
        }
    }

    Ok(recovered)
}

/// Orphan `autodev:implementing` 이슈 복구
///
/// 크래시로 인해 `autodev:implementing` 라벨이 남아있지만 연결된 PR이 이미
/// merged/closed인 이슈를 찾아 `autodev:done`으로 전이한다.
/// 연결 PR 마커(`<!-- autodev:pr-link:{N} -->`)가 없는 경우 implementing 라벨을 제거하여
/// 다음 scan에서 재시도하도록 한다.
pub async fn recover_orphan_implementing(
    repos: &[ResolvedRepo],
    gh: &dyn Gh,
    queues: &TaskQueues,
) -> Result<u64> {
    let mut recovered = 0u64;

    for repo in repos {
        let gh_host = repo.gh_host();

        for issue in repo.issues.iter().filter(|i| i.is_implementing()) {
            let work_id = make_work_id("issue", &repo.name, issue.number);
            if queues.contains(&work_id) {
                continue;
            }

            // 이슈 코멘트에서 pr-link 마커 추출
            match extract_pr_link_from_comments(gh, &repo.name, issue.number, gh_host).await {
                Some(pr_num) => {
                    // 연결 PR 상태 확인
                    let pr_state = get_pr_state(gh, &repo.name, pr_num, gh_host).await;
                    match pr_state.as_deref() {
                        Some("closed") | Some("merged") => {
                            gh.label_remove(
                                &repo.name,
                                issue.number,
                                labels::IMPLEMENTING,
                                gh_host,
                            )
                            .await;
                            gh.label_add(&repo.name, issue.number, labels::DONE, gh_host)
                                .await;
                            recovered += 1;
                            tracing::info!(
                                "recovered implementing issue #{} in {} (PR #{pr_num} {})",
                                issue.number,
                                repo.name,
                                pr_state.as_deref().unwrap_or("unknown")
                            );
                        }
                        _ => {
                            // PR이 아직 open → skip (PR pipeline이 처리)
                        }
                    }
                }
                None => {
                    // pr-link 마커 없음 → implementing 제거 (다음 scan에서 재시도)
                    gh.label_remove(&repo.name, issue.number, labels::IMPLEMENTING, gh_host)
                        .await;
                    recovered += 1;
                    tracing::info!(
                        "recovered orphan implementing issue #{} in {} (no pr-link marker)",
                        issue.number,
                        repo.name
                    );
                }
            }
        }
    }

    Ok(recovered)
}

/// 이슈 코멘트에서 `<!-- autodev:pr-link:{N} -->` 마커를 추출하여 PR 번호 반환
async fn extract_pr_link_from_comments(
    gh: &dyn Gh,
    repo_name: &str,
    number: i64,
    gh_host: Option<&str>,
) -> Option<i64> {
    let jq = r#"[.[] | select(.body | contains("<!-- autodev:pr-link:")) | .body] | last"#;
    let body = gh
        .api_get_field(repo_name, &format!("issues/{number}/comments"), jq, gh_host)
        .await?;
    // <!-- autodev:pr-link:42 --> 에서 42 추출
    let start = body.find("<!-- autodev:pr-link:")? + "<!-- autodev:pr-link:".len();
    let end = body[start..].find(" -->").map(|i| start + i)?;
    body[start..end].trim().parse().ok()
}

/// PR의 state를 조회 ("open", "closed", "merged" 등)
async fn get_pr_state(
    gh: &dyn Gh,
    repo_name: &str,
    pr_number: i64,
    gh_host: Option<&str>,
) -> Option<String> {
    // merged 여부를 먼저 확인
    let merged = gh
        .api_get_field(repo_name, &format!("pulls/{pr_number}"), ".merged", gh_host)
        .await;
    if merged.as_deref() == Some("true") {
        return Some("merged".to_string());
    }

    // state 필드 조회
    gh.api_get_field(repo_name, &format!("pulls/{pr_number}"), ".state", gh_host)
        .await
}

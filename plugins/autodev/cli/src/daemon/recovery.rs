use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::Result;

use crate::config;
use crate::config::Env;
use crate::infrastructure::gh::Gh;
use crate::queue::models::EnabledRepo;
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

/// Per-repo config에서 gh_host를 로드하는 헬퍼.
///
/// 설정 파일의 mtime이 변경되지 않았으면 캐시된 값을 반환하여
/// 매 tick마다 불필요한 디스크 I/O를 회피한다.
pub(crate) fn resolve_gh_host(env: &dyn Env, repo_name: &str) -> Option<String> {
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

/// Orphan `autodev:wip` 라벨 정리
///
/// 크래시로 인해 `autodev:wip` 라벨이 남아있지만 메모리 큐에 없는 항목을 찾아
/// 라벨을 제거한다. 다음 scan에서 자연스럽게 재발견되어 재처리된다.
pub async fn recover_orphan_wip(
    repos: &[EnabledRepo],
    gh: &dyn Gh,
    queues: &TaskQueues,
    env: &dyn Env,
) -> Result<u64> {
    let mut recovered = 0u64;

    for repo in repos {
        let repo_gh_host = resolve_gh_host(env, &repo.name);
        let gh_host = repo_gh_host.as_deref();

        let endpoint = "issues";
        let params = &[
            ("labels", labels::WIP),
            ("state", "open"),
            ("per_page", "100"),
        ];

        let data = match gh.api_paginate(&repo.name, endpoint, params, gh_host).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("recovery scan failed for {}: {e}", repo.name);
                continue;
            }
        };

        let items: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();

        for item in items {
            let number = match item["number"].as_i64() {
                Some(n) if n > 0 => n,
                _ => continue,
            };

            // GitHub issues API includes PRs — pull_request 필드 유무로 구분
            let is_pr = item.get("pull_request").is_some();

            let queue_type = if is_pr { "pr" } else { "issue" };
            let work_id = make_work_id(queue_type, &repo.name, number);

            if !queues.contains(&work_id)
                && gh
                    .label_remove(&repo.name, number, labels::WIP, gh_host)
                    .await
            {
                recovered += 1;
                tracing::info!(
                    "recovered orphan {queue_type} #{number} in {} (removed autodev:wip)",
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
    repos: &[EnabledRepo],
    gh: &dyn Gh,
    queues: &TaskQueues,
    env: &dyn Env,
) -> Result<u64> {
    let mut recovered = 0u64;

    for repo in repos {
        let repo_gh_host = resolve_gh_host(env, &repo.name);
        let gh_host = repo_gh_host.as_deref();

        let params = &[
            ("labels", labels::IMPLEMENTING),
            ("state", "open"),
            ("per_page", "100"),
        ];

        let data = match gh.api_paginate(&repo.name, "issues", params, gh_host).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("implementing recovery scan failed for {}: {e}", repo.name);
                continue;
            }
        };

        let items: Vec<serde_json::Value> = serde_json::from_slice(&data).unwrap_or_default();

        for item in items {
            if item.get("pull_request").is_some() {
                continue; // PR 제외
            }

            let number = match item["number"].as_i64() {
                Some(n) if n > 0 => n,
                _ => continue,
            };

            let work_id = make_work_id("issue", &repo.name, number);
            if queues.contains(&work_id) {
                continue; // 큐에 있으면 skip
            }

            // 이슈 코멘트에서 pr-link 마커 추출
            match extract_pr_link_from_comments(gh, &repo.name, number, gh_host).await {
                Some(pr_num) => {
                    // 연결 PR 상태 확인
                    let pr_state = get_pr_state(gh, &repo.name, pr_num, gh_host).await;
                    match pr_state.as_deref() {
                        Some("closed") | Some("merged") => {
                            gh.label_remove(&repo.name, number, labels::IMPLEMENTING, gh_host)
                                .await;
                            gh.label_add(&repo.name, number, labels::DONE, gh_host)
                                .await;
                            recovered += 1;
                            tracing::info!(
                                "recovered implementing issue #{number} in {} (PR #{pr_num} {})",
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
                    gh.label_remove(&repo.name, number, labels::IMPLEMENTING, gh_host)
                        .await;
                    recovered += 1;
                    tracing::info!(
                        "recovered orphan implementing issue #{number} in {} (no pr-link marker)",
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

use anyhow::Result;

use crate::active::ActiveItems;
use crate::infrastructure::gh::Gh;
use crate::queue::models::EnabledRepo;

/// Orphan `autodev:wip` 라벨 정리
///
/// 크래시로 인해 `autodev:wip` 라벨이 남아있지만 active 큐에 없는 항목을 찾아
/// 라벨을 제거한다. 다음 scan에서 자연스럽게 재발견되어 재처리된다.
pub async fn recover_orphan_wip(
    repos: &[EnabledRepo],
    gh: &dyn Gh,
    active: &ActiveItems,
    gh_host: Option<&str>,
) -> Result<u64> {
    let mut recovered = 0u64;

    for repo in repos {
        let endpoint = "issues";
        let params = &[
            ("labels", "autodev:wip"),
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

            let is_active = if is_pr {
                // PR은 review 또는 merge 두 가지 큐에서 처리될 수 있음
                active.contains("pr", &repo.id, number)
                    || active.contains("merge", &repo.id, number)
            } else {
                active.contains("issue", &repo.id, number)
            };

            if !is_active {
                let queue_type = if is_pr { "pr" } else { "issue" };
                if gh
                    .label_remove(&repo.name, number, "autodev:wip", gh_host)
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
    }

    Ok(recovered)
}

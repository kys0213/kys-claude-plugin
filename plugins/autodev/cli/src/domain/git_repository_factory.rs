use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::Result;

use crate::config;
use crate::config::Env;
use crate::domain::models::EnabledRepo;
use crate::domain::repository::RepoRepository;
use crate::infrastructure::gh::Gh;

use super::git_repository::{fetch_issues, fetch_pulls, GitRepository};

// ─── gh_host Cache ───

struct GhHostEntry {
    mtime: Option<SystemTime>,
    value: Option<String>,
}

static GH_HOST_CACHE: Mutex<Option<HashMap<String, GhHostEntry>>> = Mutex::new(None);

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
                    tracing::debug!(
                        "[config] resolve_gh_host({repo_name}): cache hit → {:?}",
                        entry.value
                    );
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
    tracing::debug!("[config] resolve_gh_host({repo_name}): loaded → {value:?}");

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

// ─── Factory ───

/// GitRepository 인스턴스를 조립하는 팩토리.
///
/// DB의 EnabledRepo + per-repo config(gh_host) + GitHub API(issues/pulls)를
/// 하나의 GitRepository aggregate로 조합한다.
pub struct GitRepositoryFactory;

impl GitRepositoryFactory {
    /// 단일 레포를 GitRepository로 조립한다.
    pub async fn create(repo: &EnabledRepo, env: &dyn Env, gh: &dyn Gh) -> GitRepository {
        let gh_host = resolve_gh_host(env, &repo.name);

        let issues = fetch_issues(gh, &repo.name, gh_host.as_deref()).await;
        let pulls = fetch_pulls(gh, &repo.name, gh_host.as_deref()).await;

        let mut git_repo = GitRepository::new(
            repo.id.clone(),
            repo.name.clone(),
            repo.url.clone(),
            gh_host,
        );
        git_repo.set_github_state(issues, pulls);
        git_repo
    }

    /// 모든 enabled repos를 일괄 생성한다.
    pub async fn create_all(
        db: &dyn RepoRepository,
        env: &dyn Env,
        gh: &dyn Gh,
    ) -> Result<HashMap<String, GitRepository>> {
        let repos = db.repo_find_enabled()?;
        let mut result = HashMap::with_capacity(repos.len());

        for repo in &repos {
            let git_repo = Self::create(repo, env, gh).await;
            result.insert(repo.name.clone(), git_repo);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::gh::mock::MockGh;

    fn mock_env() -> impl Env {
        struct TestEnv(tempfile::TempDir);
        impl Env for TestEnv {
            fn var(&self, key: &str) -> Result<String, std::env::VarError> {
                match key {
                    "AUTODEV_HOME" => Ok(self.0.path().to_string_lossy().into_owned()),
                    _ => Err(std::env::VarError::NotPresent),
                }
            }
        }
        TestEnv(tempfile::tempdir().unwrap())
    }

    #[tokio::test]
    async fn factory_create_builds_repository_with_github_state() {
        let gh = MockGh::new();

        // issues API 응답 설정
        let issues_json = serde_json::json!([
            {
                "number": 1,
                "title": "bug report",
                "body": "fix it",
                "user": {"login": "alice"},
                "labels": [{"name": "bug"}]
            },
            {
                "number": 2,
                "title": "feature PR",
                "body": null,
                "user": {"login": "bob"},
                "labels": [],
                "pull_request": {}
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "issues",
            serde_json::to_vec(&issues_json).unwrap(),
        );

        // pulls API 응답 설정
        let pulls_json = serde_json::json!([
            {
                "number": 10,
                "title": "fix bug",
                "body": "Closes #1",
                "user": {"login": "alice"},
                "labels": [{"name": "autodev:wip"}],
                "head": {"ref": "fix-bug"},
                "base": {"ref": "main"}
            }
        ]);
        gh.set_paginate(
            "org/repo",
            "pulls",
            serde_json::to_vec(&pulls_json).unwrap(),
        );

        let enabled = EnabledRepo {
            id: "repo-1".to_string(),
            url: "https://github.com/org/repo".to_string(),
            name: "org/repo".to_string(),
        };

        let env = mock_env();
        let repo = GitRepositoryFactory::create(&enabled, &env, &gh).await;

        assert_eq!(repo.id(), "repo-1");
        assert_eq!(repo.name(), "org/repo");

        // PR은 issues API에서 필터링됨
        assert_eq!(repo.issues().len(), 1);
        assert_eq!(repo.issues()[0].number, 1);

        assert_eq!(repo.pulls().len(), 1);
        assert_eq!(repo.pulls()[0].number, 10);
        assert!(repo.pulls()[0].is_wip());

        // 큐는 비어있음
        assert_eq!(repo.total_items(), 0);
    }

    #[tokio::test]
    async fn factory_create_handles_api_failure_gracefully() {
        let gh = MockGh::new();
        // API 응답을 설정하지 않으면 에러 반환 → 빈 벡터

        let enabled = EnabledRepo {
            id: "repo-1".to_string(),
            url: "https://github.com/org/repo".to_string(),
            name: "org/repo".to_string(),
        };

        let env = mock_env();
        let repo = GitRepositoryFactory::create(&enabled, &env, &gh).await;

        assert!(repo.issues().is_empty());
        assert!(repo.pulls().is_empty());
    }
}

//! SpecCompleter — Spec 큐 아이템의 완료 처리.
//!
//! 완료 동작:
//! 1. 스펙 상태를 Completed로 업데이트
//! 2. 관련 이슈/PR에 autodev:done 라벨 추가

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::completer::{CompleteResult, Completer};
use crate::core::labels;
use crate::core::models::SpecStatus;
use crate::core::queue_item::QueueItem;
use crate::core::repository::SpecRepository;
use crate::infra::gh::Gh;

/// Spec 완료 처리 구현체.
pub struct SpecCompleter {
    gh: Arc<dyn Gh>,
    spec_repo: Arc<dyn SpecRepository + Send + Sync>,
}

impl SpecCompleter {
    pub fn new(gh: Arc<dyn Gh>, spec_repo: Arc<dyn SpecRepository + Send + Sync>) -> Self {
        Self { gh, spec_repo }
    }
}

#[async_trait]
impl Completer for SpecCompleter {
    async fn complete(&self, item: &QueueItem) -> CompleteResult {
        let gh_host = item.gh_host.as_deref();
        let repo = &item.repo_name;

        // 1. 스펙 상태를 Completed로 전이
        // work_id에서 spec_id를 추출하지 못하면 에스컬레이션
        let spec_id = item.work_id.strip_prefix("spec:").and_then(|rest| {
            // work_id format: "spec:{repo_name}:{number}" — use repo_id for spec lookup
            rest.rsplit(':').next()
        });

        let spec_id = match spec_id {
            Some(id) => id.to_string(),
            None => {
                return CompleteResult::EscalationNeeded {
                    reason: format!("cannot extract spec_id from work_id: {}", item.work_id),
                };
            }
        };

        // 스펙 상태 업데이트
        if let Err(e) = self
            .spec_repo
            .spec_set_status(&spec_id, SpecStatus::Completed)
        {
            return CompleteResult::EscalationNeeded {
                reason: format!("failed to update spec status: {e}"),
            };
        }

        // 2. 관련 이슈 정리 — spec에 연결된 이슈들에 done 라벨 추가
        if let Ok(issues) = self.spec_repo.spec_issues(&spec_id) {
            for si in &issues {
                self.gh
                    .label_add(repo, si.issue_number, labels::DONE, gh_host)
                    .await;
                self.gh
                    .label_remove(repo, si.issue_number, labels::WIP, gh_host)
                    .await;
                self.gh.issue_close(repo, si.issue_number, gh_host).await;
            }
        }

        CompleteResult::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::*;
    use crate::core::phase::TaskKind;
    use crate::core::queue_item::{QueueItem, RepoRef};
    use crate::infra::gh::mock::MockGh;
    use anyhow::Result;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // ─── Mock SpecRepository ───

    struct MockSpecRepo {
        status_updates: Mutex<Vec<(String, SpecStatus)>>,
        issues: Mutex<Vec<SpecIssue>>,
    }

    impl MockSpecRepo {
        fn new(issues: Vec<SpecIssue>) -> Self {
            Self {
                status_updates: Mutex::new(Vec::new()),
                issues: Mutex::new(issues),
            }
        }
    }

    impl SpecRepository for MockSpecRepo {
        fn spec_add(&self, _: &NewSpec) -> Result<String> {
            Ok("s1".into())
        }
        fn spec_list(&self, _: Option<&str>) -> Result<Vec<Spec>> {
            Ok(vec![])
        }
        fn spec_show(&self, _: &str) -> Result<Option<Spec>> {
            Ok(None)
        }
        fn spec_update(&self, _: &str, _: &str, _: Option<&str>, _: Option<&str>) -> Result<()> {
            Ok(())
        }
        fn spec_set_status(&self, id: &str, status: SpecStatus) -> Result<()> {
            self.status_updates
                .lock()
                .unwrap()
                .push((id.to_string(), status));
            Ok(())
        }
        fn spec_issues(&self, _: &str) -> Result<Vec<SpecIssue>> {
            Ok(self.issues.lock().unwrap().clone())
        }
        fn spec_issues_all(&self) -> Result<HashMap<String, Vec<SpecIssue>>> {
            Ok(HashMap::new())
        }
        fn spec_issue_counts(&self) -> Result<HashMap<String, usize>> {
            Ok(HashMap::new())
        }
        fn spec_link_issue(&self, _: &str, _: i64) -> Result<()> {
            Ok(())
        }
        fn spec_unlink_issue(&self, _: &str, _: i64) -> Result<()> {
            Ok(())
        }
        fn spec_list_by_status(&self, _: SpecStatus) -> Result<Vec<Spec>> {
            Ok(vec![])
        }
        fn spec_set_priority(&self, _: &str, _: i32) -> Result<()> {
            Ok(())
        }
    }

    fn make_spec_item(spec_number: i64) -> QueueItem {
        let repo = RepoRef {
            id: "r1".into(),
            name: "org/repo".into(),
            url: "https://github.com/org/repo".into(),
            gh_host: None,
        };
        // Spec items use issue type internally but with spec work_id
        QueueItem::new_issue(
            &repo,
            spec_number,
            TaskKind::Analyze,
            "Spec task".into(),
            None,
            vec![],
            "autodev".into(),
        )
    }

    #[tokio::test]
    async fn complete_updates_spec_status_and_closes_issues() {
        let gh = Arc::new(MockGh::new());
        let spec_issues = vec![
            SpecIssue {
                spec_id: "s1".into(),
                issue_number: 10,
                created_at: "2024-01-01".into(),
            },
            SpecIssue {
                spec_id: "s1".into(),
                issue_number: 20,
                created_at: "2024-01-02".into(),
            },
        ];
        let spec_repo = Arc::new(MockSpecRepo::new(spec_issues));

        let completer = SpecCompleter::new(gh.clone(), spec_repo.clone());

        // Create item with spec-like work_id
        let mut item = make_spec_item(1);
        item.work_id = "spec:org/repo:s1".to_string();

        let result = completer.complete(&item).await;

        assert_eq!(result, CompleteResult::Completed);

        // 스펙 상태 업데이트 확인
        let updates = spec_repo.status_updates.lock().unwrap();
        assert!(updates
            .iter()
            .any(|(id, status)| id == "s1" && *status == SpecStatus::Completed));

        // 관련 이슈들에 done 라벨 추가 확인
        let added = gh.added_labels.lock().unwrap();
        assert!(added.iter().any(|(_, n, l)| *n == 10 && l == labels::DONE));
        assert!(added.iter().any(|(_, n, l)| *n == 20 && l == labels::DONE));

        // 관련 이슈들 close 확인
        let closed = gh.closed_issues.lock().unwrap();
        assert!(closed.iter().any(|(_, n)| *n == 10));
        assert!(closed.iter().any(|(_, n)| *n == 20));
    }

    #[tokio::test]
    async fn complete_with_no_linked_issues() {
        let gh = Arc::new(MockGh::new());
        let spec_repo = Arc::new(MockSpecRepo::new(vec![]));

        let completer = SpecCompleter::new(gh.clone(), spec_repo.clone());

        let mut item = make_spec_item(1);
        item.work_id = "spec:org/repo:s1".to_string();

        let result = completer.complete(&item).await;

        assert_eq!(result, CompleteResult::Completed);

        // 스펙 상태 업데이트는 수행됨
        let updates = spec_repo.status_updates.lock().unwrap();
        assert!(!updates.is_empty());

        // 이슈 close 없음
        let closed = gh.closed_issues.lock().unwrap();
        assert!(closed.is_empty());
    }
}

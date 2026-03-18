use async_trait::async_trait;
use serde::Serialize;

use super::models::{HitlEvent, NewHitlEvent};

/// 알림 이벤트 (HitlEvent의 알림용 뷰)
#[derive(Serialize)]
pub struct NotificationEvent {
    pub repo_name: String,
    pub severity: String,
    pub situation: String,
    pub context: String,
    pub options: Vec<String>,
    pub work_id: Option<String>,
    pub spec_id: Option<String>,
    pub url: Option<String>,
    /// HITL event ID (for reply-scanning markers in GitHub comments).
    pub hitl_id: Option<String>,
}

impl NotificationEvent {
    /// Create a notification for an expired HITL event.
    pub fn from_hitl_expired(event: &HitlEvent) -> Self {
        Self {
            repo_name: event.repo_id.clone(),
            severity: event.severity.to_string(),
            situation: format!("[EXPIRED] {}", event.situation),
            context: event.context.clone(),
            options: event.parsed_options(),
            work_id: event.work_id.clone(),
            spec_id: event.spec_id.clone(),
            url: None,
            hitl_id: Some(event.id.clone()),
        }
    }

    /// Create a notification for a newly created HITL event.
    pub fn from_hitl_created(event: &NewHitlEvent) -> Self {
        Self {
            repo_name: event.repo_id.clone(),
            severity: event.severity.to_string(),
            situation: format!("[HITL] {}", event.situation),
            context: event.context.clone(),
            options: event.options.clone(),
            work_id: event.work_id.clone(),
            spec_id: event.spec_id.clone(),
            url: None,
            hitl_id: None, // ID not yet assigned at creation time
        }
    }

    /// Create a notification for a failed task.
    pub fn from_task_failed(work_id: &str, repo_name: &str, error_message: &str) -> Self {
        Self {
            repo_name: repo_name.to_string(),
            severity: "high".to_string(),
            situation: format!("Task failed: {work_id}"),
            context: error_message.to_string(),
            options: vec!["Retry".to_string(), "Skip".to_string()],
            work_id: Some(work_id.to_string()),
            spec_id: None,
            url: None,
            hitl_id: None,
        }
    }
}

/// HITL 알림을 외부 채널로 전송하는 인터페이스 (OCP).
///
/// 새로운 알림 채널을 추가할 때 기존 코드를 수정하지 않고
/// 이 trait을 구현하는 새로운 struct만 추가하면 된다.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// 채널 이름 (로깅/에러 리포트용)
    fn channel_name(&self) -> &str;

    /// 알림 전송. 실패 시 anyhow::Error 반환.
    async fn notify(&self, event: &NotificationEvent) -> anyhow::Result<()>;
}

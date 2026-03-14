use async_trait::async_trait;

/// 알림 이벤트 (HitlEvent의 알림용 뷰)
pub struct NotificationEvent {
    pub repo_name: String,
    pub severity: String,
    pub situation: String,
    pub context: String,
    pub options: Vec<String>,
    pub work_id: Option<String>,
    pub spec_id: Option<String>,
    pub url: Option<String>,
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

use async_trait::async_trait;

use super::TaskOutput;

/// 개별 작업의 실행 계약.
///
/// 각 구현체는 전처리 → Agent 호출 → 후처리 패턴을 캡슐화한다.
/// TaskRunner가 이 trait을 사용하여 실행한다.
#[async_trait]
pub trait Task: Send {
    /// Task 실행 — 전처리, Agent 호출, 후처리를 내부에서 수행하고
    /// 큐 조작 명령 + 로그를 TaskOutput으로 반환한다.
    async fn run(&mut self) -> TaskOutput;
}

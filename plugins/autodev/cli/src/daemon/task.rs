//! Task trait과 관련 DTO 정의.
//!
//! 모든 pipeline 작업(분석, 구현, 리뷰, 개선, 머지)을 동일한 인터페이스로 추상화한다.
//! Task는 `before_invoke` → Agent 호출 → `after_invoke` 생명주기를 따르며,
//! TaskRunner가 이 생명주기를 실행한다.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;

use crate::domain::models::NewConsumerLog;
use crate::infrastructure::claude::SessionOptions;
use crate::queue::task_queues::PrItem;

// ─── Agent 요청/응답 DTO ───

/// Agent(Claude)에게 전달할 요청.
/// `before_invoke`가 성공하면 이 DTO를 반환한다.
#[derive(Debug)]
pub struct AgentRequest {
    /// 작업 디렉토리 (worktree 경로)
    pub working_dir: PathBuf,
    /// Agent에 보낼 프롬프트
    pub prompt: String,
    /// Claude 세션 옵션 (output_format, json_schema, system_prompt)
    pub session_opts: SessionOptions,
}

/// Agent(Claude) 호출 결과.
/// `after_invoke`에서 이 결과를 해석하여 `TaskResult`를 생성한다.
pub struct AgentResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl AgentResponse {
    /// Agent 호출 자체가 실패한 경우의 에러 응답 생성
    pub fn error(msg: impl ToString) -> Self {
        Self {
            exit_code: -1,
            stdout: String::new(),
            stderr: msg.to_string(),
            duration: Duration::ZERO,
        }
    }
}

// ─── Queue 조작 명령 ───

/// 큐 조작 명령 — TaskResult에 담겨 main loop에서 실행된다.
pub enum QueueOp {
    /// 현재 working phase에서 아이템 제거 (done/skip/error)
    Remove,
    /// PR을 특정 phase에 push
    PushPr {
        phase: &'static str,
        item: Box<PrItem>,
    },
}

// ─── Task 결과 DTO ───

/// Task 실행의 최종 결과.
/// main loop에서 큐 조작 + DB 로그 기록에 사용한다.
pub struct TaskResult {
    /// 처리된 아이템의 work_id
    pub work_id: String,
    /// 레포 이름 (InFlightTracker 카운터 감소에 사용)
    pub repo_name: String,
    /// 큐 조작 명령 목록 (main loop에서 순서대로 실행)
    pub queue_ops: Vec<QueueOp>,
    /// DB에 기록할 consumer log 목록
    pub logs: Vec<NewConsumerLog>,
    /// 최종 상태
    pub status: TaskStatus,
}

impl TaskResult {
    /// Preflight 실패 등으로 Agent 호출 없이 건너뛴 경우의 결과 생성
    pub fn skipped(work_id: String, repo_name: String, reason: SkipReason) -> Self {
        Self {
            work_id,
            repo_name,
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
            status: TaskStatus::Skipped(reason),
        }
    }
}

/// Task 실행 상태
pub enum TaskStatus {
    /// 정상 완료
    Completed,
    /// 건너뜀 (preflight 실패 등)
    Skipped(SkipReason),
    /// 실패
    Failed(String),
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Skipped(reason) => write!(f, "skipped: {reason}"),
            TaskStatus::Failed(msg) => write!(f, "failed: {msg}"),
        }
    }
}

/// 건너뛰기 사유
#[derive(Debug)]
pub enum SkipReason {
    /// Preflight 검사 실패 (이슈가 닫힘, PR이 머지됨 등)
    PreflightFailed(String),
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkipReason::PreflightFailed(msg) => write!(f, "preflight: {msg}"),
        }
    }
}

// ─── Task trait ───

/// 모든 pipeline 작업의 공통 인터페이스.
///
/// 생명주기:
/// 1. `before_invoke()` — preflight 검사 + worktree 준비 + AgentRequest 구성
/// 2. Agent 호출 (TaskRunner가 수행)
/// 3. `after_invoke()` — 결과 해석 + 라벨/코멘트 + QueueOp 생성
///
/// `before_invoke`가 `Err(SkipReason)`을 반환하면 Agent 호출 없이 건너뛴다.
#[async_trait]
pub trait Task: Send + Sync {
    /// 이 task의 work_id (e.g. "issue:org/repo:42")
    fn work_id(&self) -> &str;

    /// 이 task가 속한 레포 이름 (e.g. "org/repo")
    fn repo_name(&self) -> &str;

    /// Agent 호출 전 준비.
    /// Preflight 검사, worktree 생성, 프롬프트 구성을 수행한다.
    /// 실패 시 `Err(SkipReason)` 반환 → Agent 호출 생략.
    async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason>;

    /// Agent 호출 후 결과 처리.
    /// 응답 파싱, GitHub 라벨/코멘트, QueueOp 생성을 수행한다.
    async fn after_invoke(&mut self, response: AgentResponse) -> TaskResult;
}

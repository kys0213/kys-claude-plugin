//! TaskContext — Task 구현체에 주입되는 공유 의존성 번들.
//!
//! 개별 Task가 필요로 하는 인프라 의존성을 하나의 struct로 모아,
//! `#[allow(clippy::too_many_arguments)]` 없이 Task 생성자에 전달한다.

use std::sync::Arc;

use crate::components::workspace::WorkspaceOps;
use crate::config::ConfigLoader;
use crate::infrastructure::gh::Gh;

/// Task 구현체에 주입되는 공유 의존성 번들.
///
/// `Arc`로 감싸 여러 Task가 동시에 공유할 수 있다.
/// `Clone`이 가능하므로 각 Task 생성 시 저렴하게 복제할 수 있다.
#[derive(Clone)]
pub struct TaskContext {
    /// Workspace 관리 (clone, worktree 생성/삭제)
    pub workspace: Arc<dyn WorkspaceOps>,
    /// GitHub API 호출
    pub gh: Arc<dyn Gh>,
    /// 설정 로더
    pub config: Arc<dyn ConfigLoader>,
}

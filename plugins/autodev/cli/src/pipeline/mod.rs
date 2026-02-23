pub mod issue;
pub mod merge;
pub mod pr;

use anyhow::Result;

use crate::components::notifier::Notifier;
use crate::components::workspace::Workspace;
use crate::config::Env;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::task_queues::TaskQueues;
use crate::queue::Database;

/// 모든 큐 처리 — 각 phase를 독립적으로 실행
#[allow(clippy::too_many_arguments)]
pub async fn process_all(
    db: &Database,
    env: &dyn Env,
    workspace: &Workspace<'_>,
    notifier: &Notifier<'_>,
    gh: &dyn Gh,
    claude: &dyn Claude,
    sw: &dyn SuggestWorkflow,
    queues: &mut TaskQueues,
) -> Result<()> {
    // Issue: Phase 1 (분석) → Phase 2 (구현)
    issue::process_pending(db, env, workspace, notifier, gh, claude, queues).await?;
    issue::process_ready(db, env, workspace, gh, claude, sw, queues).await?;

    // PR: 리뷰 → 개선 → 재리뷰 사이클
    pr::process_pending(db, env, workspace, notifier, gh, claude, sw, queues).await?;
    pr::process_review_done(db, env, workspace, gh, claude, queues).await?;
    pr::process_improved(db, env, workspace, notifier, gh, claude, sw, queues).await?;

    // Merge 큐 처리
    merge::process_pending(db, env, workspace, notifier, gh, claude, queues).await?;

    Ok(())
}

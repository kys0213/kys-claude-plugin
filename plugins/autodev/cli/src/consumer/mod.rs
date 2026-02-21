pub mod github;
pub mod issue;
pub mod merge;
pub mod pr;

use anyhow::Result;

use crate::active::ActiveItems;
use crate::config::Env;
use crate::queue::Database;

/// 모든 큐 처리 — 각 phase를 독립적으로 실행
pub async fn process_all(db: &Database, env: &dyn Env, active: &mut ActiveItems) -> Result<()> {
    // Issue: Phase 1 (분석) → Phase 2 (구현)
    issue::process_pending(db, env, active).await?;
    issue::process_ready(db, env, active).await?;

    // PR 큐 처리
    pr::process_pending(db, env, active).await?;

    // Merge 큐 처리
    merge::process_pending(db, env, active).await?;

    Ok(())
}

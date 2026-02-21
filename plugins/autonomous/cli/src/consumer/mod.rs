pub mod github;
pub mod issue;
pub mod merge;
pub mod pr;

use anyhow::Result;

use crate::active::ActiveItems;
use crate::config::Env;
use crate::queue::Database;

/// 모든 큐의 pending 항목 처리
pub async fn process_all(db: &Database, env: &dyn Env, active: &mut ActiveItems) -> Result<()> {
    // Issue 큐 처리
    issue::process_pending(db, env, active).await?;

    // PR 큐 처리
    pr::process_pending(db, env, active).await?;

    // Merge 큐 처리
    merge::process_pending(db, env, active).await?;

    Ok(())
}

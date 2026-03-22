use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::infra::claude::Claude;
use crate::v5::core::runtime::RuntimeRegistry;
use crate::v5::core::workspace::WorkspaceConfig;
use crate::v5::infra::runtimes::claude::ClaudeRuntime;
use crate::v5::infra::sources::mock::MockDataSource;
use crate::v5::service::daemon::V5Daemon;
use crate::v5::service::worktree::MockWorktreeManager;

/// v5 daemon을 시작한다.
///
/// workspace.yaml에서 설정을 로드하고 V5Daemon을 구성하여 실행.
/// v4의 daemon::start()에 대응한다.
pub async fn start_v5(home: &Path, claude: Arc<dyn Claude>, config: WorkspaceConfig) -> Result<()> {
    // RuntimeRegistry 구성
    let mut registry = RuntimeRegistry::new(config.runtime.default.clone());
    let claude_runtime =
        ClaudeRuntime::new(Box::new(ClaudeRuntimeAdapter(Arc::clone(&claude))), None);
    registry.register(Arc::new(claude_runtime));

    // DataSource 구성 (현재는 MockDataSource; GitHub 연동 시 교체)
    let source = MockDataSource::new("github");

    // WorktreeManager 구성
    let worktree_dir = home.join("worktrees");
    std::fs::create_dir_all(&worktree_dir)?;
    let worktree_mgr = MockWorktreeManager::new(&worktree_dir);

    // Daemon 구성 및 실행
    let max_concurrent = config
        .sources
        .values()
        .next()
        .map(|s| s.concurrency)
        .unwrap_or(1)
        .max(1);

    let mut daemon = V5Daemon::with_home(
        config,
        vec![Box::new(source)],
        Arc::new(registry),
        Box::new(worktree_mgr),
        max_concurrent,
        home.to_path_buf(),
    );

    daemon.run(10).await;
    Ok(())
}

/// Arc<dyn Claude> → Box<dyn Claude> 어댑터.
/// ClaudeRuntime은 Box<dyn Claude>를 소유하는데, start_v5는 Arc를 받으므로.
struct ClaudeRuntimeAdapter(Arc<dyn Claude>);

#[async_trait::async_trait]
impl Claude for ClaudeRuntimeAdapter {
    async fn run_session(
        &self,
        cwd: &Path,
        prompt: &str,
        opts: &crate::infra::claude::SessionOptions,
    ) -> Result<crate::infra::claude::SessionResult> {
        self.0.run_session(cwd, prompt, opts).await
    }
}

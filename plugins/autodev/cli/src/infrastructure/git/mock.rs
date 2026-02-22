use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use super::Git;

/// 테스트용 Git 구현체 — 파일시스템만 조작, 실제 git 호출 없음
#[allow(dead_code)]
pub struct MockGit {
    /// clone 호출 시 실패시킬지 여부
    pub clone_should_fail: Mutex<bool>,
    /// worktree_add 호출 시 실패시킬지 여부
    pub worktree_should_fail: Mutex<bool>,
    /// pull 호출 시 성공/실패
    pub pull_result: Mutex<bool>,
    /// 호출 기록: (method, args_summary)
    pub calls: Mutex<Vec<(String, String)>>,
}

impl Default for MockGit {
    fn default() -> Self {
        Self {
            clone_should_fail: Mutex::new(false),
            worktree_should_fail: Mutex::new(false),
            pull_result: Mutex::new(true),
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[allow(dead_code)]
impl MockGit {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Git for MockGit {
    async fn clone(&self, url: &str, dest: &Path) -> Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(("clone".into(), format!("{url} → {}", dest.display())));

        if *self.clone_should_fail.lock().unwrap() {
            anyhow::bail!("mock: git clone failed");
        }

        // 실제 디렉토리 생성 (워크트리 로직이 존재 여부를 확인하므로)
        std::fs::create_dir_all(dest)?;
        Ok(())
    }

    async fn pull_ff_only(&self, repo_dir: &Path) -> Result<bool> {
        self.calls
            .lock()
            .unwrap()
            .push(("pull".into(), repo_dir.display().to_string()));

        Ok(*self.pull_result.lock().unwrap())
    }

    async fn worktree_add(&self, base_dir: &Path, dest: &Path, branch: Option<&str>) -> Result<()> {
        self.calls.lock().unwrap().push((
            "worktree_add".into(),
            format!(
                "{} → {} (branch: {:?})",
                base_dir.display(),
                dest.display(),
                branch
            ),
        ));

        if *self.worktree_should_fail.lock().unwrap() {
            anyhow::bail!("mock: git worktree add failed");
        }

        std::fs::create_dir_all(dest)?;
        Ok(())
    }

    async fn worktree_remove(&self, base_dir: &Path, worktree: &Path) -> Result<()> {
        self.calls.lock().unwrap().push((
            "worktree_remove".into(),
            format!("{} → {}", base_dir.display(), worktree.display()),
        ));

        if worktree.exists() {
            std::fs::remove_dir_all(worktree)?;
        }
        Ok(())
    }
}

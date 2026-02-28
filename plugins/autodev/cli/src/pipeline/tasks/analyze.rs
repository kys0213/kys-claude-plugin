use std::sync::Arc;

use async_trait::async_trait;

use crate::config::Env;
use crate::infrastructure::agent::Agent;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::pipeline::task::Task;
use crate::pipeline::TaskOutput;
use crate::queue::task_queues::IssueItem;

/// Issue 분석 Task — `analyze_one` 로직을 캡슐화
pub struct AnalyzeTask {
    item: IssueItem,
    env: Arc<dyn Env>,
    gh: Arc<dyn Gh>,
    git: Arc<dyn Git>,
    agent: Arc<dyn Agent>,
}

impl AnalyzeTask {
    pub fn new(
        item: IssueItem,
        env: Arc<dyn Env>,
        gh: Arc<dyn Gh>,
        git: Arc<dyn Git>,
        agent: Arc<dyn Agent>,
    ) -> Self {
        Self {
            item,
            env,
            gh,
            git,
            agent,
        }
    }
}

#[async_trait]
impl Task for AnalyzeTask {
    async fn run(&mut self) -> TaskOutput {
        crate::pipeline::issue::analyze_one(
            self.item.clone(),
            &*self.env,
            &*self.gh,
            &*self.git,
            &*self.agent,
        )
        .await
    }
}

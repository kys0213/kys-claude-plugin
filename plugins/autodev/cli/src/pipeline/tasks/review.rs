use std::sync::Arc;

use async_trait::async_trait;

use crate::config::Env;
use crate::infrastructure::agent::Agent;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::pipeline::task::Task;
use crate::pipeline::TaskOutput;
use crate::queue::task_queues::PrItem;

/// PR 리뷰 Task — `review_one` 로직을 캡슐화
pub struct ReviewTask {
    item: PrItem,
    env: Arc<dyn Env>,
    gh: Arc<dyn Gh>,
    git: Arc<dyn Git>,
    agent: Arc<dyn Agent>,
    sw: Arc<dyn SuggestWorkflow>,
}

impl ReviewTask {
    pub fn new(
        item: PrItem,
        env: Arc<dyn Env>,
        gh: Arc<dyn Gh>,
        git: Arc<dyn Git>,
        agent: Arc<dyn Agent>,
        sw: Arc<dyn SuggestWorkflow>,
    ) -> Self {
        Self {
            item,
            env,
            gh,
            git,
            agent,
            sw,
        }
    }
}

#[async_trait]
impl Task for ReviewTask {
    async fn run(&mut self) -> TaskOutput {
        crate::pipeline::pr::review_one(
            self.item.clone(),
            &*self.env,
            &*self.gh,
            &*self.git,
            &*self.agent,
            &*self.sw,
        )
        .await
    }
}

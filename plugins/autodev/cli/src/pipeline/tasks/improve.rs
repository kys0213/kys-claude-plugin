use std::sync::Arc;

use async_trait::async_trait;

use crate::config::Env;
use crate::infrastructure::agent::Agent;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::pipeline::task::Task;
use crate::pipeline::TaskOutput;
use crate::queue::task_queues::PrItem;

/// PR 개선 Task — `improve_one` 로직을 캡슐화
pub struct ImproveTask {
    item: PrItem,
    env: Arc<dyn Env>,
    gh: Arc<dyn Gh>,
    git: Arc<dyn Git>,
    agent: Arc<dyn Agent>,
}

impl ImproveTask {
    pub fn new(
        item: PrItem,
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
impl Task for ImproveTask {
    async fn run(&mut self) -> TaskOutput {
        crate::pipeline::pr::improve_one(
            self.item.clone(),
            &*self.env,
            &*self.gh,
            &*self.git,
            &*self.agent,
        )
        .await
    }
}

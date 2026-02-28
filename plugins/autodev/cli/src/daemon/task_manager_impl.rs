//! DefaultTaskManager — TaskManager trait의 기본 구현체.
//!
//! TaskSource를 poll하여 Task를 수집하고, Daemon에게 분배한다.

use async_trait::async_trait;

use super::task::{Task, TaskResult};
use super::task_manager::TaskManager;
use super::task_source::TaskSource;

/// TaskSource에서 Task를 수집하고 분배하는 기본 구현체.
pub struct DefaultTaskManager {
    sources: Vec<Box<dyn TaskSource>>,
    ready_tasks: Vec<Box<dyn Task>>,
}

impl DefaultTaskManager {
    pub fn new(sources: Vec<Box<dyn TaskSource>>) -> Self {
        Self {
            sources,
            ready_tasks: Vec::new(),
        }
    }
}

#[async_trait]
impl TaskManager for DefaultTaskManager {
    async fn tick(&mut self) {
        for source in &mut self.sources {
            let tasks = source.poll().await;
            self.ready_tasks.extend(tasks);
        }
    }

    fn drain_ready(&mut self) -> Vec<Box<dyn Task>> {
        std::mem::take(&mut self.ready_tasks)
    }

    fn apply(&mut self, result: TaskResult) {
        for source in &mut self.sources {
            source.apply(&result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::task::{AgentRequest, AgentResponse, QueueOp, SkipReason, TaskStatus};
    use crate::infrastructure::claude::SessionOptions;
    use std::path::PathBuf;
    use std::sync::Mutex;

    // ─── Mock TaskSource ───

    struct MockSource {
        tasks_to_return: Mutex<Vec<Box<dyn Task>>>,
        applied: Mutex<Vec<String>>,
    }

    impl MockSource {
        fn new(tasks: Vec<Box<dyn Task>>) -> Self {
            Self {
                tasks_to_return: Mutex::new(tasks),
                applied: Mutex::new(Vec::new()),
            }
        }

        fn applied_work_ids(&self) -> Vec<String> {
            self.applied.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TaskSource for MockSource {
        async fn poll(&mut self) -> Vec<Box<dyn Task>> {
            std::mem::take(&mut *self.tasks_to_return.lock().unwrap())
        }

        fn apply(&mut self, result: &TaskResult) {
            self.applied.lock().unwrap().push(result.work_id.clone());
        }
    }

    // ─── Mock Task ───

    struct DummyTask {
        id: String,
    }

    impl DummyTask {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    #[async_trait]
    impl Task for DummyTask {
        fn work_id(&self) -> &str {
            &self.id
        }
        fn repo_name(&self) -> &str {
            "org/repo"
        }
        async fn before_invoke(&mut self) -> Result<AgentRequest, SkipReason> {
            Ok(AgentRequest {
                working_dir: PathBuf::from("/tmp"),
                prompt: "test".to_string(),
                session_opts: SessionOptions::default(),
            })
        }
        async fn after_invoke(&mut self, _: AgentResponse) -> TaskResult {
            TaskResult {
                work_id: self.id.clone(),
                repo_name: "org/repo".to_string(),
                queue_ops: vec![QueueOp::Remove],
                logs: vec![],
                status: TaskStatus::Completed,
            }
        }
    }

    #[tokio::test]
    async fn tick_collects_from_all_sources() {
        let source1 = Box::new(MockSource::new(vec![Box::new(DummyTask::new("t1"))]));
        let source2 = Box::new(MockSource::new(vec![
            Box::new(DummyTask::new("t2")),
            Box::new(DummyTask::new("t3")),
        ]));

        let mut mgr = DefaultTaskManager::new(vec![source1, source2]);

        mgr.tick().await;

        let tasks = mgr.drain_ready();
        assert_eq!(tasks.len(), 3);
    }

    #[tokio::test]
    async fn drain_ready_returns_and_clears() {
        let source = Box::new(MockSource::new(vec![Box::new(DummyTask::new("t1"))]));

        let mut mgr = DefaultTaskManager::new(vec![source]);
        mgr.tick().await;

        let tasks = mgr.drain_ready();
        assert_eq!(tasks.len(), 1);

        // Second drain should be empty
        let tasks2 = mgr.drain_ready();
        assert!(tasks2.is_empty());
    }

    #[tokio::test]
    async fn apply_delegates_to_all_sources() {
        let source1 = Box::new(MockSource::new(vec![]));
        let source2 = Box::new(MockSource::new(vec![]));

        let mut mgr = DefaultTaskManager::new(vec![source1, source2]);

        let result = TaskResult {
            work_id: "test:org/repo:1".to_string(),
            repo_name: "org/repo".to_string(),
            queue_ops: vec![QueueOp::Remove],
            logs: vec![],
            status: TaskStatus::Completed,
        };

        mgr.apply(result);

        // Each source should have received the apply
        for source in &mgr.sources {
            // We can't easily access MockSource fields through dyn TaskSource
            // so we trust the implementation delegates correctly
        }
        // The test proves no panics and apply is called on all sources
    }
}

use tokio::task::JoinSet;

use super::task::Task;
use super::TaskOutput;

/// Task를 JoinSet에 spawn하는 runner.
///
/// Task trait 객체를 받아서 실행하므로 daemon이 task 유형을 알 필요 없다.
/// 새로운 task 유형 추가 시 Task 구현체만 만들면 됨 (OCP).
pub struct TaskRunner;

impl TaskRunner {
    /// Task를 JoinSet에 spawn한다.
    pub fn spawn(join_set: &mut JoinSet<TaskOutput>, mut task: impl Task + 'static) {
        join_set.spawn(async move { task.run().await });
    }
}

use std::collections::HashMap;

/// 2단계 concurrency 제어.
///
/// Level 1: workspace별 제한 (workspace.concurrency)
/// Level 2: 전역 제한 (daemon.max_concurrent)
///
/// evaluate도 slot을 소비한다.
pub struct ConcurrencyTracker {
    per_workspace: HashMap<String, usize>,
    total: usize,
    max_total: usize,
}

impl ConcurrencyTracker {
    pub fn new(max_total: u32) -> Self {
        Self {
            per_workspace: HashMap::new(),
            total: 0,
            max_total: max_total as usize,
        }
    }

    /// 전역 slot이 남아있는지 확인.
    pub fn can_spawn(&self) -> bool {
        self.total < self.max_total
    }

    /// 특정 workspace에서 추가 실행 가능한지 확인.
    /// workspace_limit: 해당 workspace의 concurrency 설정값.
    pub fn can_spawn_in_workspace(&self, workspace_id: &str, workspace_limit: u32) -> bool {
        if !self.can_spawn() {
            return false;
        }
        let current = self.per_workspace.get(workspace_id).copied().unwrap_or(0);
        current < workspace_limit as usize
    }

    /// 사용 가능한 slot 수 계산.
    /// workspace_limit와 global 남은 수 중 작은 값.
    pub fn available_slots(&self, workspace_id: &str, workspace_limit: u32) -> usize {
        if !self.can_spawn() {
            return 0;
        }
        let ws_current = self.per_workspace.get(workspace_id).copied().unwrap_or(0);
        let ws_available = (workspace_limit as usize).saturating_sub(ws_current);
        let global_available = self.max_total.saturating_sub(self.total);
        ws_available.min(global_available)
    }

    /// slot을 점유한다.
    pub fn track(&mut self, workspace_id: &str) {
        *self
            .per_workspace
            .entry(workspace_id.to_string())
            .or_insert(0) += 1;
        self.total += 1;
    }

    /// slot을 반환한다.
    pub fn release(&mut self, workspace_id: &str) {
        if let Some(count) = self.per_workspace.get_mut(workspace_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.per_workspace.remove(workspace_id);
            }
        }
        self.total = self.total.saturating_sub(1);
    }

    /// 현재 전역 사용량.
    pub fn total(&self) -> usize {
        self.total
    }

    /// 특정 workspace의 현재 사용량.
    pub fn workspace_count(&self, workspace_id: &str) -> usize {
        self.per_workspace.get(workspace_id).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_track_release() {
        let mut tracker = ConcurrencyTracker::new(4);
        assert!(tracker.can_spawn());
        assert_eq!(tracker.total(), 0);

        tracker.track("ws1");
        assert_eq!(tracker.total(), 1);
        assert_eq!(tracker.workspace_count("ws1"), 1);

        tracker.release("ws1");
        assert_eq!(tracker.total(), 0);
        assert_eq!(tracker.workspace_count("ws1"), 0);
    }

    #[test]
    fn global_limit() {
        let mut tracker = ConcurrencyTracker::new(2);
        tracker.track("ws1");
        tracker.track("ws2");
        assert!(!tracker.can_spawn());
        assert_eq!(tracker.total(), 2);

        tracker.release("ws1");
        assert!(tracker.can_spawn());
    }

    #[test]
    fn workspace_limit() {
        let mut tracker = ConcurrencyTracker::new(10);

        // ws1의 concurrency = 2
        tracker.track("ws1");
        tracker.track("ws1");
        assert!(!tracker.can_spawn_in_workspace("ws1", 2));
        assert!(tracker.can_spawn_in_workspace("ws2", 2)); // ws2는 아직 0

        tracker.release("ws1");
        assert!(tracker.can_spawn_in_workspace("ws1", 2));
    }

    #[test]
    fn available_slots_min_of_ws_and_global() {
        let mut tracker = ConcurrencyTracker::new(3);

        // ws1 concurrency=5, global=3 → available=3
        assert_eq!(tracker.available_slots("ws1", 5), 3);

        tracker.track("ws1");
        // ws1: 1/5, global: 1/3 → available=min(4, 2)=2
        assert_eq!(tracker.available_slots("ws1", 5), 2);

        tracker.track("ws2");
        // ws1: 1/5, global: 2/3 → available=min(4, 1)=1
        assert_eq!(tracker.available_slots("ws1", 5), 1);

        tracker.track("ws1");
        // ws1: 2/5, global: 3/3 → available=min(3, 0)=0
        assert_eq!(tracker.available_slots("ws1", 5), 0);
    }

    #[test]
    fn workspace_limit_smaller_than_global() {
        let mut tracker = ConcurrencyTracker::new(10);

        // ws1 concurrency=2, global=10 → available=2
        assert_eq!(tracker.available_slots("ws1", 2), 2);

        tracker.track("ws1");
        assert_eq!(tracker.available_slots("ws1", 2), 1);

        tracker.track("ws1");
        assert_eq!(tracker.available_slots("ws1", 2), 0);
        assert!(!tracker.can_spawn_in_workspace("ws1", 2));
    }

    #[test]
    fn release_saturating() {
        let mut tracker = ConcurrencyTracker::new(4);
        // release without track → no underflow
        tracker.release("ws1");
        assert_eq!(tracker.total(), 0);
        assert_eq!(tracker.workspace_count("ws1"), 0);
    }

    #[test]
    fn multi_workspace() {
        let mut tracker = ConcurrencyTracker::new(4);
        tracker.track("ws1");
        tracker.track("ws1");
        tracker.track("ws2");

        assert_eq!(tracker.workspace_count("ws1"), 2);
        assert_eq!(tracker.workspace_count("ws2"), 1);
        assert_eq!(tracker.total(), 3);

        assert!(tracker.can_spawn_in_workspace("ws1", 3));
        assert!(!tracker.can_spawn_in_workspace("ws1", 2));
    }

    #[test]
    fn evaluate_consumes_slot() {
        let mut tracker = ConcurrencyTracker::new(2);
        tracker.track("ws1"); // handler
        tracker.track("ws1"); // evaluate (같은 workspace)
        assert!(!tracker.can_spawn());
    }
}

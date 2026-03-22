use std::collections::{HashMap, VecDeque};

use super::models::QueuePhase;

/// 아이템의 고유 ID를 반환하는 trait
pub trait HasWorkId {
    fn work_id(&self) -> &str;
}

/// 상태별 큐: consumer가 특정 상태의 아이템을 pop하여 처리
///
/// 각 상태(phase)마다 독립된 VecDeque를 유지하며,
/// dedup index로 O(1) 중복 체크와 상태 조회를 지원한다.
pub struct StateQueue<T: HasWorkId> {
    queues: HashMap<QueuePhase, VecDeque<T>>,
    /// work_id → current phase (O(1) lookup)
    index: HashMap<String, QueuePhase>,
}

impl<T: HasWorkId> Default for StateQueue<T> {
    fn default() -> Self {
        Self {
            queues: HashMap::new(),
            index: HashMap::new(),
        }
    }
}

impl<T: HasWorkId> StateQueue<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// 특정 상태의 큐에 아이템을 push한다.
    /// 이미 같은 work_id가 존재하면 무시한다 (dedup).
    pub fn push(&mut self, phase: QueuePhase, item: T) -> bool {
        let id = item.work_id().to_string();
        if self.index.contains_key(&id) {
            return false;
        }
        self.index.insert(id, phase);
        self.queues.entry(phase).or_default().push_back(item);
        true
    }

    /// 특정 상태의 큐에서 아이템을 하나 꺼낸다 (FIFO).
    pub fn pop(&mut self, phase: QueuePhase) -> Option<T> {
        let queue = self.queues.get_mut(&phase)?;
        let item = queue.pop_front()?;
        self.index.remove(item.work_id());
        Some(item)
    }

    /// 아이템을 from 상태에서 to 상태로 전이한다.
    /// 성공 시 true, 해당 아이템이 from 상태에 없으면 false.
    pub fn transit(&mut self, id: &str, from: QueuePhase, to: QueuePhase) -> bool {
        match self.index.get(id) {
            Some(current) if *current == from => {}
            _ => return false,
        }

        let item = self.remove_from_queue(from, id);
        match item {
            Some(item) => {
                self.index.insert(id.to_string(), to);
                self.queues.entry(to).or_default().push_back(item);
                true
            }
            None => false,
        }
    }

    /// work_id로 아이템을 완전히 제거한다 (done/skip 시).
    pub fn remove(&mut self, id: &str) -> Option<T> {
        let phase = self.index.remove(id)?;
        self.remove_from_queue(phase, id)
    }

    /// 해당 work_id가 큐에 존재하는지 확인한다.
    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// 주어진 prefix로 시작하는 work_id가 큐에 존재하는지 확인한다.
    ///
    /// source_id 기반 dedup에 사용: 같은 외부 엔티티의 어떤 상태라도 큐에 있으면 true.
    pub fn contains_by_prefix(&self, prefix: &str) -> bool {
        self.index.keys().any(|k| k.starts_with(prefix))
    }

    /// 해당 work_id의 현재 상태를 반환한다.
    pub fn phase_of(&self, id: &str) -> Option<QueuePhase> {
        self.index.get(id).copied()
    }

    /// 특정 상태의 큐 깊이를 반환한다.
    pub fn len(&self, phase: QueuePhase) -> usize {
        self.queues.get(&phase).map_or(0, |q| q.len())
    }

    /// from 상태에서 최대 limit개를 pop → to 상태로 push하고 반환한다.
    pub fn drain_to(&mut self, from: QueuePhase, to: QueuePhase, limit: usize) -> Vec<T>
    where
        T: Clone,
    {
        let mut result = Vec::new();
        for _ in 0..limit {
            let Some(item) = self.pop(from) else { break };
            self.push(to, item.clone());
            result.push(item);
        }
        result
    }

    /// from 상태에서 predicate를 만족하는 아이템만 최대 limit개 pop → to 상태로 push.
    ///
    /// predicate를 만족하지 않는 아이템은 from 상태에 그대로 남는다.
    pub fn drain_to_filtered<F>(
        &mut self,
        from: QueuePhase,
        to: QueuePhase,
        limit: usize,
        predicate: F,
    ) -> Vec<T>
    where
        T: Clone,
        F: Fn(&T) -> bool,
    {
        let mut result = Vec::new();
        let mut skipped = Vec::new();

        while result.len() < limit {
            let Some(item) = self.pop(from) else { break };
            if predicate(&item) {
                self.push(to, item.clone());
                result.push(item);
            } else {
                skipped.push(item);
            }
        }

        // 건너뛴 아이템을 다시 from 큐에 넣는다 (순서 유지를 위해 앞에 삽입)
        for item in skipped.into_iter().rev() {
            let id = item.work_id().to_string();
            self.index.insert(id, from);
            self.queues.entry(from).or_default().push_front(item);
        }

        result
    }

    /// 전체 아이템 수를 반환한다.
    pub fn total(&self) -> usize {
        self.index.len()
    }

    /// 특정 상태의 모든 아이템을 참조로 반환한다.
    pub fn iter(&self, phase: QueuePhase) -> impl Iterator<Item = &T> {
        self.queues.get(&phase).into_iter().flat_map(|q| q.iter())
    }

    /// 전체 아이템을 (phase, &item) 형태로 순회한다.
    pub fn iter_all(&self) -> impl Iterator<Item = (QueuePhase, &T)> {
        self.queues
            .iter()
            .flat_map(|(phase, queue)| queue.iter().map(move |item| (*phase, item)))
    }

    /// 내부 큐에서 work_id로 아이템을 제거 (선형 탐색)
    fn remove_from_queue(&mut self, phase: QueuePhase, id: &str) -> Option<T> {
        let queue = self.queues.get_mut(&phase)?;
        let pos = queue.iter().position(|item| item.work_id() == id)?;
        queue.remove(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestItem {
        id: String,
        value: i32,
    }

    impl HasWorkId for TestItem {
        fn work_id(&self) -> &str {
            &self.id
        }
    }

    fn item(id: &str, value: i32) -> TestItem {
        TestItem {
            id: id.to_string(),
            value,
        }
    }

    #[test]
    fn push_and_pop() {
        let mut q = StateQueue::new();
        assert!(q.push(QueuePhase::Pending, item("a", 1)));
        assert!(q.push(QueuePhase::Pending, item("b", 2)));

        assert_eq!(q.len(QueuePhase::Pending), 2);
        assert_eq!(q.total(), 2);

        let a = q.pop(QueuePhase::Pending).unwrap();
        assert_eq!(a.id, "a");
        assert_eq!(q.len(QueuePhase::Pending), 1);
        assert_eq!(q.total(), 1);

        let b = q.pop(QueuePhase::Pending).unwrap();
        assert_eq!(b.id, "b");
        assert_eq!(q.len(QueuePhase::Pending), 0);
        assert_eq!(q.total(), 0);
    }

    #[test]
    fn pop_empty_returns_none() {
        let mut q: StateQueue<TestItem> = StateQueue::new();
        assert!(q.pop(QueuePhase::Pending).is_none());
        assert!(q.pop(QueuePhase::Running).is_none());
    }

    #[test]
    fn dedup_prevents_duplicate_push() {
        let mut q = StateQueue::new();
        assert!(q.push(QueuePhase::Pending, item("a", 1)));
        assert!(!q.push(QueuePhase::Pending, item("a", 99)));
        assert!(!q.push(QueuePhase::Ready, item("a", 99)));

        assert_eq!(q.total(), 1);
        let a = q.pop(QueuePhase::Pending).unwrap();
        assert_eq!(a.value, 1);
    }

    #[test]
    fn transit_moves_item_between_phases() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));

        assert!(q.transit("a", QueuePhase::Pending, QueuePhase::Running));
        assert_eq!(q.len(QueuePhase::Pending), 0);
        assert_eq!(q.len(QueuePhase::Running), 1);
        assert_eq!(q.phase_of("a"), Some(QueuePhase::Running));

        let a = q.pop(QueuePhase::Running).unwrap();
        assert_eq!(a.value, 1);
    }

    #[test]
    fn transit_wrong_from_phase_returns_false() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));

        assert!(!q.transit("a", QueuePhase::Ready, QueuePhase::Running));
        assert_eq!(q.phase_of("a"), Some(QueuePhase::Pending));
    }

    #[test]
    fn transit_nonexistent_id_returns_false() {
        let mut q: StateQueue<TestItem> = StateQueue::new();
        assert!(!q.transit("x", QueuePhase::Pending, QueuePhase::Ready));
    }

    #[test]
    fn remove_item() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));
        q.push(QueuePhase::Pending, item("b", 2));

        let a = q.remove("a").unwrap();
        assert_eq!(a.value, 1);
        assert!(!q.contains("a"));
        assert_eq!(q.total(), 1);

        assert!(q.remove("x").is_none());
    }

    #[test]
    fn contains_and_phase_of() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));

        assert!(q.contains("a"));
        assert!(!q.contains("b"));
        assert_eq!(q.phase_of("a"), Some(QueuePhase::Pending));
        assert_eq!(q.phase_of("b"), None);
    }

    #[test]
    fn iter_returns_items_in_phase() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));
        q.push(QueuePhase::Pending, item("b", 2));
        q.push(QueuePhase::Ready, item("c", 3));

        let pending: Vec<&str> = q.iter(QueuePhase::Pending).map(|i| i.work_id()).collect();
        assert_eq!(pending, vec!["a", "b"]);

        let ready: Vec<&str> = q.iter(QueuePhase::Ready).map(|i| i.work_id()).collect();
        assert_eq!(ready, vec!["c"]);
    }

    #[test]
    fn iter_all_returns_all_items_with_phase() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));
        q.push(QueuePhase::Ready, item("b", 2));

        let mut all: Vec<(QueuePhase, &str)> =
            q.iter_all().map(|(p, i)| (p, i.work_id())).collect();
        all.sort_by_key(|(_, id)| *id);
        assert_eq!(
            all,
            vec![(QueuePhase::Pending, "a"), (QueuePhase::Ready, "b")]
        );
    }

    #[test]
    fn push_after_remove_allows_reinsert() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));
        q.remove("a");

        assert!(q.push(QueuePhase::Pending, item("a", 99)));
        let a = q.pop(QueuePhase::Pending).unwrap();
        assert_eq!(a.value, 99);
    }

    #[test]
    fn fifo_order_preserved() {
        let mut q = StateQueue::new();
        for i in 0..5 {
            q.push(QueuePhase::Pending, item(&format!("item-{i}"), i));
        }
        for i in 0..5 {
            let it = q.pop(QueuePhase::Pending).unwrap();
            assert_eq!(it.value, i);
        }
    }

    #[test]
    fn remove_middle_item_preserves_order() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));
        q.push(QueuePhase::Pending, item("b", 2));
        q.push(QueuePhase::Pending, item("c", 3));

        q.remove("b");

        let first = q.pop(QueuePhase::Pending).unwrap();
        assert_eq!(first.id, "a");
        let second = q.pop(QueuePhase::Pending).unwrap();
        assert_eq!(second.id, "c");
    }

    // ─── drain_to_filtered tests ───

    #[test]
    fn drain_to_filtered_selects_by_predicate() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));
        q.push(QueuePhase::Pending, item("b", 2));
        q.push(QueuePhase::Pending, item("c", 3));

        let drained = q.drain_to_filtered(QueuePhase::Pending, QueuePhase::Running, 10, |i| {
            i.value > 1
        });

        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, "b");
        assert_eq!(drained[1].id, "c");

        // "a" should remain in Pending
        assert_eq!(q.len(QueuePhase::Pending), 1);
        assert_eq!(q.len(QueuePhase::Running), 2);
    }

    #[test]
    fn drain_to_filtered_respects_limit() {
        let mut q = StateQueue::new();
        for i in 0..5 {
            q.push(QueuePhase::Pending, item(&format!("i{i}"), i));
        }

        let drained = q.drain_to_filtered(QueuePhase::Pending, QueuePhase::Running, 2, |_| true);

        assert_eq!(drained.len(), 2);
        assert_eq!(q.len(QueuePhase::Pending), 3);
        assert_eq!(q.len(QueuePhase::Running), 2);
    }

    #[test]
    fn drain_to_filtered_leaves_non_matching() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("a", 1));
        q.push(QueuePhase::Pending, item("b", 2));
        q.push(QueuePhase::Pending, item("c", 3));

        let drained = q.drain_to_filtered(QueuePhase::Pending, QueuePhase::Running, 10, |i| {
            i.id == "b"
        });

        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "b");

        // a and c remain in Pending, in order
        let remaining: Vec<&str> = q.iter(QueuePhase::Pending).map(|i| i.work_id()).collect();
        assert_eq!(remaining, vec!["a", "c"]);
    }

    #[test]
    fn drain_to_filtered_preserves_skipped_order() {
        let mut q = StateQueue::new();
        q.push(QueuePhase::Pending, item("x", 0));
        q.push(QueuePhase::Pending, item("a", 1));
        q.push(QueuePhase::Pending, item("y", 0));
        q.push(QueuePhase::Pending, item("b", 2));

        let drained = q.drain_to_filtered(QueuePhase::Pending, QueuePhase::Running, 10, |i| {
            i.value > 0
        });

        assert_eq!(drained.len(), 2);
        // Skipped items should retain original order
        let remaining: Vec<&str> = q.iter(QueuePhase::Pending).map(|i| i.work_id()).collect();
        assert_eq!(remaining, vec!["x", "y"]);
    }
}

use std::collections::{HashMap, VecDeque};

/// 아이템의 고유 ID를 반환하는 trait
pub trait HasWorkId {
    fn work_id(&self) -> &str;
}

/// 상태별 큐: consumer가 특정 상태의 아이템을 pop하여 처리
///
/// 각 상태(phase)마다 독립된 VecDeque를 유지하며,
/// dedup index로 O(1) 중복 체크와 상태 조회를 지원한다.
pub struct StateQueue<T: HasWorkId> {
    queues: HashMap<String, VecDeque<T>>,
    /// work_id → current state (O(1) lookup)
    index: HashMap<String, String>,
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
    pub fn push(&mut self, state: &str, item: T) -> bool {
        let id = item.work_id().to_string();
        if self.index.contains_key(&id) {
            return false;
        }
        self.index.insert(id, state.to_string());
        self.queues
            .entry(state.to_string())
            .or_default()
            .push_back(item);
        true
    }

    /// 특정 상태의 큐에서 아이템을 하나 꺼낸다 (FIFO).
    pub fn pop(&mut self, state: &str) -> Option<T> {
        let queue = self.queues.get_mut(state)?;
        let item = queue.pop_front()?;
        self.index.remove(item.work_id());
        Some(item)
    }

    /// 아이템을 from 상태에서 to 상태로 전이한다.
    /// 성공 시 true, 해당 아이템이 from 상태에 없으면 false.
    pub fn transit(&mut self, id: &str, from: &str, to: &str) -> bool {
        match self.index.get(id) {
            Some(current) if current == from => {}
            _ => return false,
        }

        let item = self.remove_from_queue(from, id);
        match item {
            Some(item) => {
                self.index.insert(id.to_string(), to.to_string());
                self.queues
                    .entry(to.to_string())
                    .or_default()
                    .push_back(item);
                true
            }
            None => false,
        }
    }

    /// work_id로 아이템을 완전히 제거한다 (done/skip 시).
    pub fn remove(&mut self, id: &str) -> Option<T> {
        let state = self.index.remove(id)?;
        self.remove_from_queue(&state, id)
    }

    /// 해당 work_id가 큐에 존재하는지 확인한다.
    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    /// 해당 work_id의 현재 상태를 반환한다.
    pub fn state_of(&self, id: &str) -> Option<&str> {
        self.index.get(id).map(|s| s.as_str())
    }

    /// 특정 상태의 큐 깊이를 반환한다.
    pub fn len(&self, state: &str) -> usize {
        self.queues.get(state).map_or(0, |q| q.len())
    }

    /// 전체 아이템 수를 반환한다.
    pub fn total(&self) -> usize {
        self.index.len()
    }

    /// 특정 상태의 모든 아이템을 참조로 반환한다.
    pub fn iter(&self, state: &str) -> impl Iterator<Item = &T> {
        self.queues.get(state).into_iter().flat_map(|q| q.iter())
    }

    /// 전체 아이템을 (state, &item) 형태로 순회한다.
    pub fn iter_all(&self) -> impl Iterator<Item = (&str, &T)> {
        self.queues
            .iter()
            .flat_map(|(state, queue)| queue.iter().map(move |item| (state.as_str(), item)))
    }

    /// 내부 큐에서 work_id로 아이템을 제거 (선형 탐색)
    fn remove_from_queue(&mut self, state: &str, id: &str) -> Option<T> {
        let queue = self.queues.get_mut(state)?;
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
        assert!(q.push("Pending", item("a", 1)));
        assert!(q.push("Pending", item("b", 2)));

        assert_eq!(q.len("Pending"), 2);
        assert_eq!(q.total(), 2);

        let a = q.pop("Pending").unwrap();
        assert_eq!(a.id, "a");
        assert_eq!(q.len("Pending"), 1);
        assert_eq!(q.total(), 1);

        let b = q.pop("Pending").unwrap();
        assert_eq!(b.id, "b");
        assert_eq!(q.len("Pending"), 0);
        assert_eq!(q.total(), 0);
    }

    #[test]
    fn pop_empty_returns_none() {
        let mut q: StateQueue<TestItem> = StateQueue::new();
        assert!(q.pop("Pending").is_none());
        assert!(q.pop("NonExistent").is_none());
    }

    #[test]
    fn dedup_prevents_duplicate_push() {
        let mut q = StateQueue::new();
        assert!(q.push("Pending", item("a", 1)));
        assert!(!q.push("Pending", item("a", 99)));
        assert!(!q.push("Ready", item("a", 99)));

        assert_eq!(q.total(), 1);
        let a = q.pop("Pending").unwrap();
        assert_eq!(a.value, 1);
    }

    #[test]
    fn transit_moves_item_between_states() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));

        assert!(q.transit("a", "Pending", "Analyzing"));
        assert_eq!(q.len("Pending"), 0);
        assert_eq!(q.len("Analyzing"), 1);
        assert_eq!(q.state_of("a"), Some("Analyzing"));

        let a = q.pop("Analyzing").unwrap();
        assert_eq!(a.value, 1);
    }

    #[test]
    fn transit_wrong_from_state_returns_false() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));

        assert!(!q.transit("a", "Ready", "Analyzing"));
        assert_eq!(q.state_of("a"), Some("Pending"));
    }

    #[test]
    fn transit_nonexistent_id_returns_false() {
        let mut q: StateQueue<TestItem> = StateQueue::new();
        assert!(!q.transit("x", "Pending", "Ready"));
    }

    #[test]
    fn remove_item() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));
        q.push("Pending", item("b", 2));

        let a = q.remove("a").unwrap();
        assert_eq!(a.value, 1);
        assert!(!q.contains("a"));
        assert_eq!(q.total(), 1);

        assert!(q.remove("x").is_none());
    }

    #[test]
    fn contains_and_state_of() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));

        assert!(q.contains("a"));
        assert!(!q.contains("b"));
        assert_eq!(q.state_of("a"), Some("Pending"));
        assert_eq!(q.state_of("b"), None);
    }

    #[test]
    fn iter_returns_items_in_state() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));
        q.push("Pending", item("b", 2));
        q.push("Ready", item("c", 3));

        let pending: Vec<&str> = q.iter("Pending").map(|i| i.work_id()).collect();
        assert_eq!(pending, vec!["a", "b"]);

        let ready: Vec<&str> = q.iter("Ready").map(|i| i.work_id()).collect();
        assert_eq!(ready, vec!["c"]);
    }

    #[test]
    fn iter_all_returns_all_items_with_state() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));
        q.push("Ready", item("b", 2));

        let mut all: Vec<(&str, &str)> = q.iter_all().map(|(s, i)| (s, i.work_id())).collect();
        all.sort();
        assert_eq!(all, vec![("Pending", "a"), ("Ready", "b")]);
    }

    #[test]
    fn push_after_remove_allows_reinsert() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));
        q.remove("a");

        assert!(q.push("Pending", item("a", 99)));
        let a = q.pop("Pending").unwrap();
        assert_eq!(a.value, 99);
    }

    #[test]
    fn fifo_order_preserved() {
        let mut q = StateQueue::new();
        for i in 0..5 {
            q.push("Pending", item(&format!("item-{i}"), i));
        }
        for i in 0..5 {
            let it = q.pop("Pending").unwrap();
            assert_eq!(it.value, i);
        }
    }

    #[test]
    fn remove_middle_item_preserves_order() {
        let mut q = StateQueue::new();
        q.push("Pending", item("a", 1));
        q.push("Pending", item("b", 2));
        q.push("Pending", item("c", 3));

        q.remove("b");

        let first = q.pop("Pending").unwrap();
        assert_eq!(first.id, "a");
        let second = q.pop("Pending").unwrap();
        assert_eq!(second.id, "c");
    }
}

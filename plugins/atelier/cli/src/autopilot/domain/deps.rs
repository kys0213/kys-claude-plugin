use std::collections::{BTreeMap, BTreeSet};

use super::task_id::TaskId;

#[derive(Debug, Clone)]
pub struct TaskGraph {
    edges: BTreeMap<TaskId, BTreeSet<TaskId>>,
    reverse: BTreeMap<TaskId, BTreeSet<TaskId>>,
    nodes: BTreeSet<TaskId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleError {
    pub cycle: Vec<TaskId>,
}

impl TaskGraph {
    pub fn build<I>(deps: I) -> Self
    where
        I: IntoIterator<Item = (TaskId, TaskId)>,
    {
        let mut g = TaskGraph {
            edges: BTreeMap::new(),
            reverse: BTreeMap::new(),
            nodes: BTreeSet::new(),
        };
        for (task, depends_on) in deps {
            g.nodes.insert(task.clone());
            g.nodes.insert(depends_on.clone());
            g.edges
                .entry(task.clone())
                .or_default()
                .insert(depends_on.clone());
            g.reverse.entry(depends_on).or_default().insert(task);
        }
        g
    }

    pub fn detect_cycle(&self) -> Option<Vec<TaskId>> {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Color {
            White,
            Gray,
            Black,
        }
        let mut color: BTreeMap<TaskId, Color> = self
            .nodes
            .iter()
            .map(|n| (n.clone(), Color::White))
            .collect();
        let mut path: Vec<TaskId> = Vec::new();

        fn dfs(
            node: &TaskId,
            edges: &BTreeMap<TaskId, BTreeSet<TaskId>>,
            color: &mut BTreeMap<TaskId, Color>,
            path: &mut Vec<TaskId>,
        ) -> Option<Vec<TaskId>> {
            color.insert(node.clone(), Color::Gray);
            path.push(node.clone());
            if let Some(adj) = edges.get(node) {
                for next in adj {
                    match color.get(next).copied().unwrap_or(Color::White) {
                        Color::White => {
                            if let Some(c) = dfs(next, edges, color, path) {
                                return Some(c);
                            }
                        }
                        Color::Gray => {
                            let mut cycle: Vec<TaskId> =
                                path.iter().skip_while(|n| *n != next).cloned().collect();
                            if cycle.is_empty() {
                                cycle.push(next.clone());
                            }
                            return Some(cycle);
                        }
                        Color::Black => {}
                    }
                }
            }
            path.pop();
            color.insert(node.clone(), Color::Black);
            None
        }

        for n in self.nodes.iter() {
            if color.get(n).copied().unwrap_or(Color::White) == Color::White {
                if let Some(c) = dfs(n, &self.edges, &mut color, &mut path) {
                    return Some(c);
                }
                path.clear();
            }
        }
        None
    }

    pub fn topological_order(&self) -> Result<Vec<TaskId>, CycleError> {
        if let Some(cycle) = self.detect_cycle() {
            return Err(CycleError { cycle });
        }
        let mut indegree: BTreeMap<TaskId, usize> =
            self.nodes.iter().map(|n| (n.clone(), 0usize)).collect();
        for (_task, deps) in &self.edges {
            for dep in deps {
                *indegree.entry(dep.clone()).or_insert(0) += 0;
                let entry = indegree.entry(_task.clone()).or_insert(0);
                *entry += 1;
            }
        }

        let mut ready: Vec<TaskId> = indegree
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(k, _)| k.clone())
            .collect();
        let mut order: Vec<TaskId> = Vec::with_capacity(self.nodes.len());
        while let Some(n) = ready.pop() {
            order.push(n.clone());
            if let Some(parents) = self.reverse.get(&n) {
                for parent in parents {
                    if let Some(d) = indegree.get_mut(parent) {
                        *d -= 1;
                        if *d == 0 {
                            ready.push(parent.clone());
                        }
                    }
                }
            }
        }
        Ok(order)
    }

    pub fn dependents_of(&self, id: &TaskId) -> Vec<&TaskId> {
        self.reverse
            .get(id)
            .map(|s| s.iter().collect())
            .unwrap_or_default()
    }

    pub fn dependencies_of(&self, id: &TaskId) -> Vec<&TaskId> {
        self.edges
            .get(id)
            .map(|s| s.iter().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> TaskId {
        TaskId::from_raw(s)
    }

    #[test]
    fn graph_with_no_cycle_passes() {
        let g = TaskGraph::build([(id("B"), id("A")), (id("C"), id("B"))]);
        assert!(g.detect_cycle().is_none());
    }

    #[test]
    fn graph_with_self_loop_fails() {
        let g = TaskGraph::build([(id("A"), id("A"))]);
        let cycle = g.detect_cycle().expect("cycle expected");
        assert_eq!(cycle, vec![id("A")]);
    }

    #[test]
    fn graph_with_2cycle_fails() {
        let g = TaskGraph::build([(id("A"), id("B")), (id("B"), id("A"))]);
        assert!(g.detect_cycle().is_some());
    }

    #[test]
    fn dependents_of_finds_immediate_children() {
        let g = TaskGraph::build([(id("B"), id("A")), (id("C"), id("B"))]);
        assert_eq!(g.dependents_of(&id("A")), vec![&id("B")]);
        assert_eq!(g.dependents_of(&id("B")), vec![&id("C")]);
    }

    #[test]
    fn topological_order_respects_constraints() {
        let g = TaskGraph::build([(id("B"), id("A")), (id("C"), id("B"))]);
        let order = g.topological_order().unwrap();
        let pos = |s: &str| order.iter().position(|i| i.as_str() == s).unwrap();
        assert!(pos("A") < pos("B"));
        assert!(pos("B") < pos("C"));
    }
}

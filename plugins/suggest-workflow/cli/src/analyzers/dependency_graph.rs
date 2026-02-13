use std::collections::{HashMap, HashSet};
use crate::types::{
    SessionEntry, DependencyGraphResult, DependencyNode, DependencyEdge,
    ToolCycle, CriticalPath,
};
use crate::parsers::extract_tool_sequence;
use crate::analyzers::tool_classifier::classify_tool;
use crate::analyzers::tuning::TuningConfig;

/// Build a tool dependency graph with node/edge metrics, cycle detection,
/// and critical path analysis.
///
/// Work units are time-windowed subsequences (same logic as workflow.rs).
/// "Commit-reachable" is measured by whether a work unit contains a `Bash:git` step.
pub fn build_dependency_graph(
    sessions: &[(String, Vec<SessionEntry>)],
    top: usize,
    tuning: &TuningConfig,
) -> DependencyGraphResult {
    let time_window_ms = tuning.time_window_minutes as i64 * 60 * 1000;

    // --- Phase 1: Extract work units ---
    let mut work_units: Vec<Vec<String>> = Vec::new();

    for (_id, entries) in sessions {
        let tool_uses = extract_tool_sequence(entries);
        let classified: Vec<String> = tool_uses
            .iter()
            .map(|t| classify_tool(&t.name, t.input.as_ref()).classified_name)
            .collect();

        // Split into work units by time gap
        let mut current_unit: Vec<String> = Vec::new();
        for (i, name) in classified.iter().enumerate() {
            current_unit.push(name.clone());

            if i < tool_uses.len() - 1 {
                if let (Some(curr_ts), Some(next_ts)) =
                    (tool_uses[i].timestamp, tool_uses[i + 1].timestamp)
                {
                    if next_ts - curr_ts > time_window_ms {
                        if current_unit.len() >= 2 {
                            work_units.push(current_unit.clone());
                        }
                        current_unit.clear();
                    }
                }
            }
        }
        if current_unit.len() >= 2 {
            work_units.push(current_unit);
        }
    }

    if work_units.is_empty() {
        return DependencyGraphResult {
            nodes: Vec::new(),
            edges: Vec::new(),
            cycles: Vec::new(),
            critical_paths: Vec::new(),
            total_transitions: 0,
            unique_tools: 0,
        };
    }

    // --- Phase 2: Compute edge and node metrics ---
    let mut edge_counts: HashMap<(String, String), usize> = HashMap::new();
    let mut tool_uses_count: HashMap<String, usize> = HashMap::new();
    let mut tool_positions: HashMap<String, Vec<f64>> = HashMap::new();
    let mut entry_counts: HashMap<String, usize> = HashMap::new();
    let mut terminal_counts: HashMap<String, usize> = HashMap::new();
    let mut total_transitions: usize = 0;

    // Per-edge: track which work units contain this edge and whether they have git
    let mut edge_work_unit_indices: HashMap<(String, String), Vec<usize>> = HashMap::new();
    // Per-edge: steps remaining to end of work unit when this edge occurs
    let mut edge_steps_to_end: HashMap<(String, String), Vec<f64>> = HashMap::new();

    // Which work units contain Bash:git
    let commit_units: Vec<bool> = work_units
        .iter()
        .map(|unit| unit.iter().any(|t| t == "Bash:git"))
        .collect();

    for (unit_idx, unit) in work_units.iter().enumerate() {
        let unit_len = unit.len();

        // Entry and terminal tools
        if let Some(first) = unit.first() {
            *entry_counts.entry(first.clone()).or_insert(0) += 1;
        }
        if let Some(last) = unit.last() {
            *terminal_counts.entry(last.clone()).or_insert(0) += 1;
        }

        // Per-tool position and usage
        for (pos, tool) in unit.iter().enumerate() {
            *tool_uses_count.entry(tool.clone()).or_insert(0) += 1;
            let normalized_pos = if unit_len > 1 {
                pos as f64 / (unit_len - 1) as f64
            } else {
                0.5
            };
            tool_positions
                .entry(tool.clone())
                .or_default()
                .push(normalized_pos);
        }

        // Edges
        for (i, pair) in unit.windows(2).enumerate() {
            let key = (pair[0].clone(), pair[1].clone());
            *edge_counts.entry(key.clone()).or_insert(0) += 1;
            total_transitions += 1;

            edge_work_unit_indices
                .entry(key.clone())
                .or_default()
                .push(unit_idx);

            let steps_remaining = (unit_len - 2 - i) as f64;
            edge_steps_to_end
                .entry(key)
                .or_default()
                .push(steps_remaining);
        }
    }

    let total_work_units = work_units.len() as f64;

    // Unique tool set
    let all_tools: HashSet<&str> = tool_uses_count.keys().map(|s| s.as_str()).collect();
    let unique_tools = all_tools.len();

    // From-totals for forward probability, to-totals for reverse probability
    let from_totals: HashMap<&str, usize> = {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for ((from, _), &count) in &edge_counts {
            *m.entry(from.as_str()).or_insert(0) += count;
        }
        m
    };
    let to_totals: HashMap<&str, usize> = {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for ((_, to), &count) in &edge_counts {
            *m.entry(to.as_str()).or_insert(0) += count;
        }
        m
    };

    // Fanout / fanin
    let mut fanout: HashMap<&str, HashSet<&str>> = HashMap::new();
    let mut fanin: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (from, to) in edge_counts.keys() {
        fanout.entry(from.as_str()).or_default().insert(to.as_str());
        fanin.entry(to.as_str()).or_default().insert(from.as_str());
    }

    // --- Build nodes ---
    let mut nodes: Vec<DependencyNode> = all_tools
        .iter()
        .map(|&tool| {
            let uses = tool_uses_count.get(tool).copied().unwrap_or(0);
            let positions = tool_positions.get(tool);
            let avg_pos = positions
                .map(|ps| ps.iter().sum::<f64>() / ps.len() as f64)
                .unwrap_or(0.5);
            let entry = entry_counts.get(tool).copied().unwrap_or(0) as f64;
            let terminal = terminal_counts.get(tool).copied().unwrap_or(0) as f64;

            DependencyNode {
                tool: tool.to_string(),
                total_uses: uses,
                fanout: fanout.get(tool).map(|s| s.len()).unwrap_or(0),
                fanin: fanin.get(tool).map(|s| s.len()).unwrap_or(0),
                avg_position: round2(avg_pos),
                terminal_rate: round2(terminal / total_work_units),
                entry_rate: round2(entry / total_work_units),
            }
        })
        .collect();
    nodes.sort_by(|a, b| b.total_uses.cmp(&a.total_uses));

    // --- Build edges ---
    let mut edges: Vec<DependencyEdge> = edge_counts
        .iter()
        .map(|((from, to), &count)| {
            let from_total = *from_totals.get(from.as_str()).unwrap_or(&1) as f64;
            let to_total = *to_totals.get(to.as_str()).unwrap_or(&1) as f64;

            // Commit-reachable rate
            let unit_indices = edge_work_unit_indices
                .get(&(from.clone(), to.clone()))
                .cloned()
                .unwrap_or_default();
            let commit_count = unit_indices
                .iter()
                .filter(|&&idx| commit_units.get(idx).copied().unwrap_or(false))
                .count();
            let commit_rate = if unit_indices.is_empty() {
                0.0
            } else {
                commit_count as f64 / unit_indices.len() as f64
            };

            // Avg steps to end
            let steps = edge_steps_to_end
                .get(&(from.clone(), to.clone()))
                .cloned()
                .unwrap_or_default();
            let avg_steps = if steps.is_empty() {
                0.0
            } else {
                steps.iter().sum::<f64>() / steps.len() as f64
            };

            DependencyEdge {
                from: from.clone(),
                to: to.clone(),
                count,
                probability: round2(count as f64 / from_total),
                reverse_probability: round2(count as f64 / to_total),
                commit_reachable_rate: round2(commit_rate),
                avg_steps_to_end: round2(avg_steps),
            }
        })
        .collect();
    edges.sort_by(|a, b| b.count.cmp(&a.count));

    // --- Phase 3: Cycle detection (Tarjan's SCC) ---
    let cycles = detect_cycles(&edge_counts, &work_units);

    // --- Phase 4: Critical paths ---
    let critical_paths = find_critical_paths(&work_units, &commit_units, top);

    DependencyGraphResult {
        nodes,
        edges,
        cycles,
        critical_paths,
        total_transitions,
        unique_tools,
    }
}

/// Detect cycles using Tarjan's strongly connected components algorithm.
/// Then verify each SCC cycle actually appears as a contiguous subsequence in real data.
fn detect_cycles(
    edge_counts: &HashMap<(String, String), usize>,
    work_units: &[Vec<String>],
) -> Vec<ToolCycle> {
    // Build adjacency list
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut all_nodes: HashSet<&str> = HashSet::new();
    for (from, to) in edge_counts.keys() {
        adj.entry(from.as_str()).or_default().push(to.as_str());
        all_nodes.insert(from.as_str());
        all_nodes.insert(to.as_str());
    }

    // Tarjan's SCC
    let sccs = tarjan_scc(&all_nodes, &adj);

    // Only keep SCCs with size >= 2 (self-loops are handled by repetition.rs)
    let mut cycles: Vec<ToolCycle> = Vec::new();

    for scc in &sccs {
        if scc.len() < 2 {
            continue;
        }

        let scc_set: HashSet<&str> = scc.iter().map(|s| s.as_str()).collect();

        // Find actual occurrences of this cycle pattern in work units
        let (occurrence_count, avg_iterations) =
            count_cycle_occurrences(&scc_set, work_units);

        if occurrence_count > 0 {
            let mut tools: Vec<String> = scc.clone();
            tools.sort();
            cycles.push(ToolCycle {
                tools,
                occurrence_count,
                avg_iterations: round2(avg_iterations),
            });
        }
    }

    cycles.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));
    cycles
}

/// Tarjan's SCC algorithm.
fn tarjan_scc<'a>(
    nodes: &HashSet<&'a str>,
    adj: &HashMap<&'a str, Vec<&'a str>>,
) -> Vec<Vec<String>> {
    struct State<'a> {
        index_counter: usize,
        stack: Vec<&'a str>,
        on_stack: HashSet<&'a str>,
        index: HashMap<&'a str, usize>,
        lowlink: HashMap<&'a str, usize>,
        result: Vec<Vec<String>>,
    }

    fn strongconnect<'a>(
        v: &'a str,
        adj: &HashMap<&'a str, Vec<&'a str>>,
        state: &mut State<'a>,
    ) {
        state.index.insert(v, state.index_counter);
        state.lowlink.insert(v, state.index_counter);
        state.index_counter += 1;
        state.stack.push(v);
        state.on_stack.insert(v);

        if let Some(neighbors) = adj.get(v) {
            for &w in neighbors {
                if !state.index.contains_key(w) {
                    strongconnect(w, adj, state);
                    let w_low = state.lowlink[w];
                    let v_low = state.lowlink[v];
                    if w_low < v_low {
                        state.lowlink.insert(v, w_low);
                    }
                } else if state.on_stack.contains(w) {
                    let w_idx = state.index[w];
                    let v_low = state.lowlink[v];
                    if w_idx < v_low {
                        state.lowlink.insert(v, w_idx);
                    }
                }
            }
        }

        if state.lowlink[v] == state.index[v] {
            let mut component = Vec::new();
            loop {
                let w = state.stack.pop().unwrap();
                state.on_stack.remove(w);
                component.push(w.to_string());
                if w == v {
                    break;
                }
            }
            state.result.push(component);
        }
    }

    let mut state = State {
        index_counter: 0,
        stack: Vec::new(),
        on_stack: HashSet::new(),
        index: HashMap::new(),
        lowlink: HashMap::new(),
        result: Vec::new(),
    };

    let mut sorted_nodes: Vec<&&str> = nodes.iter().collect();
    sorted_nodes.sort();

    for &&node in &sorted_nodes {
        if !state.index.contains_key(node) {
            strongconnect(node, adj, &mut state);
        }
    }

    state.result
}

/// Count how many times a cycle (SCC members) appears as a contiguous rotation
/// in the actual work unit data. Also compute average iterations.
fn count_cycle_occurrences(
    scc_set: &HashSet<&str>,
    work_units: &[Vec<String>],
) -> (usize, f64) {
    let mut total_occurrences: usize = 0;
    let mut total_iterations: f64 = 0.0;
    let scc_len = scc_set.len();

    for unit in work_units {
        if unit.len() < scc_len * 2 {
            continue;
        }

        // Slide a window of scc_len through the work unit,
        // looking for runs where all tools in the window belong to the SCC.
        let mut i = 0;
        while i + scc_len <= unit.len() {
            let window: HashSet<&str> = unit[i..i + scc_len]
                .iter()
                .map(|s| s.as_str())
                .collect();

            if window.is_subset(scc_set) && window.len() == scc_len {
                // Found one occurrence of the cycle pattern, count consecutive repetitions
                let pattern = &unit[i..i + scc_len];
                let mut reps = 1;
                let mut j = i + scc_len;
                while j + scc_len <= unit.len() && unit[j..j + scc_len] == *pattern {
                    reps += 1;
                    j += scc_len;
                }
                total_occurrences += 1;
                total_iterations += reps as f64;
                i = j;
            } else {
                i += 1;
            }
        }
    }

    let avg_iter = if total_occurrences > 0 {
        total_iterations / total_occurrences as f64
    } else {
        0.0
    };

    (total_occurrences, avg_iter)
}

/// Find the most common complete work-unit paths and their commit rates.
fn find_critical_paths(
    work_units: &[Vec<String>],
    commit_units: &[bool],
    top: usize,
) -> Vec<CriticalPath> {
    // Deduplicate work-unit paths into (path, [unit_indices])
    let mut path_map: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
    for (idx, unit) in work_units.iter().enumerate() {
        // Truncate long paths to keep analysis tractable
        let key: Vec<String> = if unit.len() > 8 {
            unit[..8].to_vec()
        } else {
            unit.clone()
        };
        path_map.entry(key).or_default().push(idx);
    }

    let mut paths: Vec<CriticalPath> = path_map
        .into_iter()
        .filter(|(_, indices)| indices.len() >= 2) // only report paths seen >= 2 times
        .map(|(path, indices)| {
            let commit_count = indices
                .iter()
                .filter(|&&idx| commit_units.get(idx).copied().unwrap_or(false))
                .count();
            let commit_rate = commit_count as f64 / indices.len() as f64;
            CriticalPath {
                path,
                frequency: indices.len(),
                commit_rate: round2(commit_rate),
            }
        })
        .collect();

    paths.sort_by(|a, b| b.frequency.cmp(&a.frequency));
    paths.truncate(top);
    paths
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

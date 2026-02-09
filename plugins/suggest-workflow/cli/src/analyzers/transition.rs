use std::collections::HashMap;
use crate::types::{SessionEntry, TransitionMatrixResult, TransitionEntry};
use crate::parsers::extract_tool_sequence;
use crate::analyzers::tool_classifier::classify_tool;

/// Build a tool transition matrix from sessions.
/// Pure counting: no rules, no classification beyond tool name mapping.
pub fn build_transition_matrix(
    sessions: &[(String, Vec<SessionEntry>)],
) -> TransitionMatrixResult {
    let mut matrix: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut total_transitions: usize = 0;

    for (_id, entries) in sessions {
        let tool_uses = extract_tool_sequence(entries);
        let classified: Vec<String> = tool_uses
            .iter()
            .map(|t| classify_tool(&t.name, t.input.as_ref()).classified_name)
            .collect();

        for pair in classified.windows(2) {
            *matrix
                .entry(pair[0].clone())
                .or_default()
                .entry(pair[1].clone())
                .or_default() += 1;
            total_transitions += 1;
        }
    }

    // Compute probabilities
    let from_totals: HashMap<String, usize> = matrix
        .iter()
        .map(|(from, to_map)| {
            let total: usize = to_map.values().sum();
            (from.clone(), total)
        })
        .collect();

    let unique_tools = {
        let mut tools: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (from, to_map) in &matrix {
            tools.insert(from.as_str());
            for to in to_map.keys() {
                tools.insert(to.as_str());
            }
        }
        tools.len()
    };

    let mut transitions: Vec<TransitionEntry> = Vec::new();
    for (from, to_map) in &matrix {
        let from_total = from_totals[from] as f64;
        for (to, &count) in to_map {
            transitions.push(TransitionEntry {
                from: from.clone(),
                to: to.clone(),
                count,
                probability: if from_total > 0.0 { count as f64 / from_total } else { 0.0 },
            });
        }
    }

    // Sort by count descending for readability
    transitions.sort_by(|a, b| b.count.cmp(&a.count));

    TransitionMatrixResult {
        transitions,
        total_transitions,
        unique_tools,
    }
}

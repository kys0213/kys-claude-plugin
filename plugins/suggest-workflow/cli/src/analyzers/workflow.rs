use std::collections::HashMap;
use crate::types::{SessionEntry, ToolSequence, WorkflowAnalysisResult};
use crate::parsers::extract_tool_sequence;
use crate::analyzers::tool_classifier::classify_tool;

const DEFAULT_TIME_WINDOW: i64 = 5 * 60 * 1000; // 5 minutes in milliseconds

/// Extract tool sequences from session entries
pub fn extract_tool_sequences(
    entries: &[SessionEntry],
    min_length: usize,
    max_length: usize,
) -> Vec<Vec<String>> {
    let tool_uses = extract_tool_sequence(entries);

    if tool_uses.len() < min_length {
        return Vec::new();
    }

    // Group into work units based on time windows
    let mut work_units: Vec<Vec<String>> = Vec::new();
    let mut current_unit: Vec<String> = Vec::new();

    for (i, tool) in tool_uses.iter().enumerate() {
        let classified = classify_tool(&tool.name, None);
        current_unit.push(classified.classified_name);

        if i < tool_uses.len() - 1 {
            if let (Some(curr_ts), Some(next_ts)) = (tool.timestamp, tool_uses[i + 1].timestamp) {
                let time_diff = next_ts - curr_ts;
                if time_diff > DEFAULT_TIME_WINDOW {
                    if current_unit.len() >= min_length {
                        work_units.push(current_unit.clone());
                    }
                    current_unit.clear();
                }
            }
        }
    }

    if current_unit.len() >= min_length {
        work_units.push(current_unit);
    }

    // Extract sequences using sliding window
    let mut sequences = Vec::new();
    for unit in work_units {
        for len in min_length..=max_length.min(unit.len()) {
            for start in 0..=(unit.len() - len) {
                sequences.push(unit[start..start + len].to_vec());
            }
        }
    }

    sequences
}

/// Find common sequences across sessions
fn find_common_sequences(
    sequences: &[Vec<String>],
    session_ids: &[String],
    min_occurrence: usize,
) -> Vec<ToolSequence> {
    let mut sequence_map: HashMap<String, (usize, Vec<String>)> = HashMap::new();

    for (seq, session_id) in sequences.iter().zip(session_ids.iter()) {
        let key = seq.join("->");
        let entry = sequence_map.entry(key).or_insert((0, Vec::new()));
        entry.0 += 1;
        if !entry.1.contains(session_id) {
            entry.1.push(session_id.clone());
        }
    }

    let mut results: Vec<ToolSequence> = sequence_map
        .into_iter()
        .filter(|(_, (count, _))| *count >= min_occurrence)
        .map(|(key, (count, sessions))| ToolSequence {
            tools: key.split("->").map(String::from).collect(),
            count,
            sessions,
        })
        .collect();

    results.sort_by(|a, b| b.count.cmp(&a.count));
    results
}

/// Analyze workflows across multiple sessions
pub fn analyze_workflows(
    sessions: &[(String, Vec<SessionEntry>)],
    threshold: usize,
    top: usize,
    min_length: usize,
    max_length: usize,
) -> WorkflowAnalysisResult {
    if sessions.is_empty() {
        return WorkflowAnalysisResult {
            total_sequences: 0,
            unique_sequences: 0,
            top_sequences: Vec::new(),
            tool_usage_stats: Vec::new(),
        };
    }

    let mut all_sequences = Vec::new();
    let mut all_session_ids = Vec::new();
    let mut tool_usage: HashMap<String, usize> = HashMap::new();

    for (session_id, entries) in sessions {
        let sequences = extract_tool_sequences(entries, min_length, max_length);

        for seq in sequences {
            all_sequences.push(seq);
            all_session_ids.push(session_id.clone());
        }

        // Count individual tool usage
        let tool_uses = extract_tool_sequence(entries);
        for tool in tool_uses {
            let classified = classify_tool(&tool.name, None);
            *tool_usage.entry(classified.classified_name).or_insert(0) += 1;
        }
    }

    let common_sequences = find_common_sequences(&all_sequences, &all_session_ids, threshold);
    let top_sequences = common_sequences.into_iter().take(top).collect();

    let unique_sequences = all_sequences
        .iter()
        .map(|seq| seq.join("->"))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let mut tool_usage_vec: Vec<(String, usize)> = tool_usage.into_iter().collect();
    tool_usage_vec.sort_by(|a, b| b.1.cmp(&a.1));

    WorkflowAnalysisResult {
        total_sequences: all_sequences.len(),
        unique_sequences,
        top_sequences,
        tool_usage_stats: tool_usage_vec,
    }
}

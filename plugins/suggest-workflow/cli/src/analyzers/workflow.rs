use std::collections::{HashMap, HashSet};
use crate::types::{SessionEntry, ToolUse, ToolSequence, WorkflowAnalysisResult};
use crate::parsers::extract_tool_sequence;
use crate::analyzers::tool_classifier::classify_tool;

const DEFAULT_TIME_WINDOW: i64 = 5 * 60 * 1000; // 5 minutes in milliseconds

/// Classify tool uses into named sequences, passing input for Bash sub-classification
fn classify_tool_uses(tool_uses: &[ToolUse]) -> Vec<String> {
    tool_uses
        .iter()
        .map(|t| classify_tool(&t.name, t.input.as_ref()).classified_name)
        .collect()
}

/// Extract tool sequences from pre-classified tool names with time-based grouping
fn extract_sequences_from_classified(
    tool_uses: &[ToolUse],
    classified_names: &[String],
    min_length: usize,
    max_length: usize,
) -> Vec<Vec<String>> {
    if classified_names.len() < min_length {
        return Vec::new();
    }

    // Group into work units based on time windows
    let mut work_units: Vec<Vec<String>> = Vec::new();
    let mut current_unit: Vec<String> = Vec::new();

    for (i, name) in classified_names.iter().enumerate() {
        current_unit.push(name.clone());

        if i < tool_uses.len() - 1 {
            if let (Some(curr_ts), Some(next_ts)) = (tool_uses[i].timestamp, tool_uses[i + 1].timestamp) {
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
    session_indices: &[usize],
    session_names: &[String],
    min_occurrence: usize,
) -> Vec<ToolSequence> {
    let mut sequence_map: HashMap<String, (usize, HashSet<usize>)> = HashMap::new();

    for (seq, &session_idx) in sequences.iter().zip(session_indices.iter()) {
        let key = seq.join("\x1F");
        let entry = sequence_map.entry(key).or_insert_with(|| (0, HashSet::new()));
        entry.0 += 1;
        entry.1.insert(session_idx);
    }

    let mut results: Vec<ToolSequence> = sequence_map
        .into_iter()
        .filter(|(_, (count, _))| *count >= min_occurrence)
        .map(|(key, (count, session_idxs))| ToolSequence {
            tools: key.split('\x1F').map(String::from).collect(),
            count,
            sessions: session_idxs
                .into_iter()
                .map(|idx| session_names[idx].clone())
                .collect(),
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

    let session_names: Vec<String> = sessions.iter().map(|(id, _)| id.clone()).collect();
    let mut all_sequences = Vec::new();
    let mut all_session_indices = Vec::new();
    let mut tool_usage: HashMap<String, usize> = HashMap::new();

    for (session_idx, (_session_id, entries)) in sessions.iter().enumerate() {
        // Extract tool uses once per session (fixes P1: double extraction)
        let tool_uses = extract_tool_sequence(entries);
        let classified_names = classify_tool_uses(&tool_uses);

        // Count individual tool usage from the same classified result
        for name in &classified_names {
            *tool_usage.entry(name.clone()).or_insert(0) += 1;
        }

        // Extract sequences from already-classified names
        let sequences = extract_sequences_from_classified(
            &tool_uses, &classified_names, min_length, max_length,
        );

        for seq in sequences {
            all_sequences.push(seq);
            all_session_indices.push(session_idx);
        }
    }

    let common_sequences = find_common_sequences(
        &all_sequences, &all_session_indices, &session_names, threshold,
    );
    let top_sequences = common_sequences.into_iter().take(top).collect();

    let unique_sequences = all_sequences
        .iter()
        .map(|seq| seq.join("\x1F"))
        .collect::<HashSet<_>>()
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

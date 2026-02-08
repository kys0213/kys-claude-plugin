use std::collections::HashMap;
use crate::types::{
    SessionEntry, RepetitionResult, FileEditOutlier,
    ToolLoop, SessionRepetitionStats,
};
use crate::parsers::extract_tool_sequence;
use crate::analyzers::tool_classifier::classify_tool;

/// Detect repetition patterns and statistical outliers across sessions.
/// Uses mean ± σ from the data itself — no hardcoded thresholds.
pub fn analyze_repetition(
    sessions: &[(String, Vec<SessionEntry>)],
) -> RepetitionResult {
    let mut all_file_edits: Vec<(String, String, usize)> = Vec::new(); // (session_id, file, count)
    let mut all_loops: Vec<ToolLoop> = Vec::new();
    let mut session_stats: Vec<SessionRepetitionStats> = Vec::new();

    for (session_id, entries) in sessions {
        let tool_uses = extract_tool_sequence(entries);
        let classified: Vec<String> = tool_uses
            .iter()
            .map(|t| classify_tool(&t.name, t.input.as_ref()).classified_name)
            .collect();

        // Per-file edit counts
        let mut file_counts: HashMap<String, usize> = HashMap::new();
        for tool in &tool_uses {
            if matches!(tool.name.as_str(), "Edit" | "Write" | "NotebookEdit") {
                if let Some(input) = &tool.input {
                    let path = input
                        .get("file_path")
                        .or_else(|| input.get("notebook_path"))
                        .and_then(|v| v.as_str());
                    if let Some(p) = path {
                        *file_counts.entry(p.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
        for (file, count) in &file_counts {
            all_file_edits.push((session_id.clone(), file.clone(), *count));
        }

        // Detect consecutive loops: find repeated subsequences
        detect_loops(&classified, session_id, &mut all_loops);

        // Session-level repetition stats
        let total_tool_uses = classified.len();
        let unique_tool_uses = {
            let set: std::collections::HashSet<&str> = classified.iter().map(|s| s.as_str()).collect();
            set.len()
        };

        let (max_consecutive, most_repeated) = max_consecutive_same(&classified);

        session_stats.push(SessionRepetitionStats {
            session_id: session_id.clone(),
            total_tool_uses,
            unique_tool_uses,
            max_consecutive_same_tool: max_consecutive,
            most_repeated_tool: most_repeated,
        });
    }

    // Statistical outlier detection for file edits (mean + 2σ)
    let edit_counts: Vec<f64> = all_file_edits.iter().map(|(_, _, c)| *c as f64).collect();
    let (mean, std_dev) = mean_stddev(&edit_counts);

    let file_edit_outliers: Vec<FileEditOutlier> = if std_dev > 0.0 {
        all_file_edits
            .iter()
            .filter_map(|(session_id, file, count)| {
                let z = (*count as f64 - mean) / std_dev;
                if z >= 2.0 {
                    Some(FileEditOutlier {
                        file: file.clone(),
                        edit_count: *count,
                        session_id: session_id.clone(),
                        z_score: (z * 100.0).round() / 100.0,
                    })
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    RepetitionResult {
        file_edit_outliers,
        tool_loops: all_loops,
        session_stats,
        global_mean_edits_per_file: (mean * 100.0).round() / 100.0,
        global_std_dev: (std_dev * 100.0).round() / 100.0,
    }
}

/// Detect repeating subsequences of length 2-3 in tool sequences.
fn detect_loops(classified: &[String], session_id: &str, loops: &mut Vec<ToolLoop>) {
    for seq_len in 2..=3 {
        if classified.len() < seq_len * 2 {
            continue;
        }
        let mut i = 0;
        while i + seq_len <= classified.len() {
            let pattern = &classified[i..i + seq_len];
            let mut repeat_count = 1;
            let mut j = i + seq_len;
            while j + seq_len <= classified.len() && classified[j..j + seq_len] == *pattern {
                repeat_count += 1;
                j += seq_len;
            }
            if repeat_count >= 2 {
                loops.push(ToolLoop {
                    sequence: pattern.to_vec(),
                    repeat_count,
                    session_id: session_id.to_string(),
                });
                i = j; // skip past the repeated section
            } else {
                i += 1;
            }
        }
    }
}

/// Find max consecutive identical tool and which tool it was.
fn max_consecutive_same(tools: &[String]) -> (usize, Option<String>) {
    if tools.is_empty() {
        return (0, None);
    }
    let mut max_count = 1;
    let mut max_tool = tools[0].clone();
    let mut current_count = 1;

    for i in 1..tools.len() {
        if tools[i] == tools[i - 1] {
            current_count += 1;
            if current_count > max_count {
                max_count = current_count;
                max_tool = tools[i].clone();
            }
        } else {
            current_count = 1;
        }
    }

    (max_count, Some(max_tool))
}

fn mean_stddev(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    (mean, variance.sqrt())
}

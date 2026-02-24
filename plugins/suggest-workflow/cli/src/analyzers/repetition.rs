use crate::analyzers::tool_classifier::classify_tool;
use crate::analyzers::tuning::TuningConfig;
use crate::parsers::extract_tool_sequence;
use crate::types::{
    FileEditOutlier, RepetitionResult, SessionEntry, SessionRepetitionStats, ToolLoop,
};
use std::collections::HashMap;

/// Detect repetition patterns and statistical outliers across sessions.
/// Uses mean ± σ from the data itself — thresholds driven by TuningConfig.
pub fn analyze_repetition(
    sessions: &[(String, Vec<SessionEntry>)],
    tuning: &TuningConfig,
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
        detect_loops(
            &classified,
            session_id,
            &mut all_loops,
            tuning.loop_max_seq_length,
            tuning.loop_min_repeats,
        );

        // Session-level repetition stats
        let total_tool_uses = classified.len();
        let unique_tool_uses = {
            let set: std::collections::HashSet<&str> =
                classified.iter().map(|s| s.as_str()).collect();
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

    // Statistical outlier detection for file edits with Benjamini-Hochberg FDR correction.
    // BH controls the false discovery rate when testing many files simultaneously:
    // without correction, z >= 2.0 on 1000 files yields ~50 false positives;
    // BH keeps the expected proportion of false discoveries ≤ α.
    let edit_counts: Vec<f64> = all_file_edits.iter().map(|(_, _, c)| *c as f64).collect();
    let (mean, std_dev) = mean_stddev(&edit_counts);

    let file_edit_outliers: Vec<FileEditOutlier> = if std_dev > 0.0 {
        // Step 1: compute z-scores and p-values for all candidates
        let mut candidates: Vec<(usize, f64, f64)> = all_file_edits // (index, z, p)
            .iter()
            .enumerate()
            .filter_map(|(i, (_, _, count))| {
                let z = (*count as f64 - mean) / std_dev;
                if z > 0.0 {
                    let p = 1.0 - normal_cdf_approx(z);
                    Some((i, z, p))
                } else {
                    None
                }
            })
            .collect();

        // Step 2: BH procedure — derive α from z_score_threshold
        let alpha = 1.0 - normal_cdf_approx(tuning.z_score_threshold);
        let significant = benjamini_hochberg(&mut candidates, alpha);

        // Step 3: build outlier results from significant indices
        significant
            .iter()
            .map(|&idx| {
                let (session_id, file, count) = &all_file_edits[idx];
                let z = (*count as f64 - mean) / std_dev;
                FileEditOutlier {
                    file: file.clone(),
                    edit_count: *count,
                    session_id: session_id.clone(),
                    z_score: (z * 100.0).round() / 100.0,
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

/// Detect repeating subsequences in tool sequences.
fn detect_loops(
    classified: &[String],
    session_id: &str,
    loops: &mut Vec<ToolLoop>,
    max_seq_length: usize,
    min_repeats: usize,
) {
    for seq_len in 2..=max_seq_length {
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
            if repeat_count >= min_repeats {
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

/// Approximate standard normal CDF using Abramowitz & Stegun formula 26.2.17.
/// Absolute error < 7.5×10⁻⁸ for all z.
fn normal_cdf_approx(z: f64) -> f64 {
    if z < -8.0 {
        return 0.0;
    }
    if z > 8.0 {
        return 1.0;
    }
    let t = 1.0 / (1.0 + 0.2316419 * z.abs());
    let pdf = 0.398_942_280_401_432_7 * (-z * z / 2.0).exp(); // 1/√(2π) × e^(-z²/2)
    let poly = t
        * (0.319_381_530
            + t * (-0.356_563_782
                + t * (1.781_477_937 + t * (-1.821_255_978 + t * 1.330_274_429))));
    if z >= 0.0 {
        1.0 - pdf * poly
    } else {
        pdf * poly
    }
}

/// Benjamini-Hochberg procedure for controlling False Discovery Rate.
/// Input: `candidates` = [(original_index, z_score, p_value)].
/// Returns the original indices of items that survive correction at level `alpha`.
fn benjamini_hochberg(candidates: &mut [(usize, f64, f64)], alpha: f64) -> Vec<usize> {
    if candidates.is_empty() || alpha <= 0.0 {
        return Vec::new();
    }

    let m = candidates.len();

    // Sort by p-value ascending
    candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    // Find largest rank k where p(k) ≤ (k/m) × α
    let mut max_k = 0;
    for (rank_0, &(_, _, p)) in candidates.iter().enumerate() {
        let rank = rank_0 + 1; // 1-indexed
        let bh_threshold = (rank as f64 / m as f64) * alpha;
        if p <= bh_threshold {
            max_k = rank;
        }
    }

    // All items with rank ≤ max_k are significant
    candidates
        .iter()
        .take(max_k)
        .map(|&(idx, _, _)| idx)
        .collect()
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

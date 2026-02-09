use std::collections::{BTreeSet, HashMap, HashSet};
use crate::types::{
    SessionEntry, FileAnalysisResult, FileHotspot, CoChangeGroup,
};
use crate::parsers::extract_tool_sequence;
use crate::analyzers::tool_classifier::classify_tool;

/// Analyze file-level patterns: hotspots, co-change groups, tool correlations.
/// Pure counting — no heuristic rules.
pub fn analyze_files(
    sessions: &[(String, Vec<SessionEntry>)],
    top_n: usize,
) -> FileAnalysisResult {
    // Per-file stats across all sessions
    let mut file_edit_count: HashMap<String, usize> = HashMap::new();
    let mut file_session_count: HashMap<String, HashSet<String>> = HashMap::new();
    let mut file_tool_map: HashMap<String, HashMap<String, usize>> = HashMap::new();

    // Per-session file sets (for co-change detection)
    let mut session_file_sets: Vec<BTreeSet<String>> = Vec::new();

    for (session_id, entries) in sessions {
        let tool_uses = extract_tool_sequence(entries);
        let mut session_files: BTreeSet<String> = BTreeSet::new();

        for tool in &tool_uses {
            let classified = classify_tool(&tool.name, tool.input.as_ref());

            // Track file edits
            if matches!(tool.name.as_str(), "Edit" | "Write" | "NotebookEdit" | "Read") {
                if let Some(input) = &tool.input {
                    let path = input
                        .get("file_path")
                        .or_else(|| input.get("notebook_path"))
                        .and_then(|v| v.as_str());
                    if let Some(p) = path {
                        if matches!(tool.name.as_str(), "Edit" | "Write" | "NotebookEdit") {
                            *file_edit_count.entry(p.to_string()).or_insert(0) += 1;
                            session_files.insert(p.to_string());
                        }
                        file_session_count
                            .entry(p.to_string())
                            .or_default()
                            .insert(session_id.clone());
                        *file_tool_map
                            .entry(p.to_string())
                            .or_default()
                            .entry(classified.classified_name.clone())
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        if !session_files.is_empty() {
            session_file_sets.push(session_files);
        }
    }

    // Build hot files list
    let mut hot_files: Vec<FileHotspot> = file_edit_count
        .iter()
        .map(|(path, &edit_count)| {
            let session_count = file_session_count
                .get(path)
                .map(|s| s.len())
                .unwrap_or(0);

            let mut tools_used: Vec<(String, usize)> = file_tool_map
                .get(path)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
                .unwrap_or_default();
            tools_used.sort_by(|a, b| b.1.cmp(&a.1));

            FileHotspot {
                path: path.clone(),
                edit_count,
                session_count,
                tools_used,
            }
        })
        .collect();

    hot_files.sort_by(|a, b| b.edit_count.cmp(&a.edit_count));
    hot_files.truncate(top_n);

    // Co-change detection: files that appear together across multiple sessions
    let co_change_groups = detect_co_changes(&session_file_sets);

    FileAnalysisResult {
        hot_files,
        co_change_groups,
    }
}

/// Detect co-change groups: file pairs that frequently appear in the same session.
/// Uses pairwise counting — no clustering rules.
fn detect_co_changes(session_file_sets: &[BTreeSet<String>]) -> Vec<CoChangeGroup> {
    let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();

    for file_set in session_file_sets {
        let files: Vec<&String> = file_set.iter().collect();
        // Only process sessions with manageable file counts
        if files.len() > 50 || files.len() < 2 {
            continue;
        }
        for i in 0..files.len() {
            for j in (i + 1)..files.len() {
                let key = (files[i].clone(), files[j].clone());
                *pair_counts.entry(key).or_insert(0) += 1;
            }
        }
    }

    // Filter: keep pairs that co-occur in at least 2 sessions
    let min_co_occurrence = 2;
    let mut groups: Vec<CoChangeGroup> = pair_counts
        .into_iter()
        .filter(|(_, count)| *count >= min_co_occurrence)
        .map(|((a, b), count)| CoChangeGroup {
            files: vec![a, b],
            co_occurrence_count: count,
        })
        .collect();

    groups.sort_by(|a, b| b.co_occurrence_count.cmp(&a.co_occurrence_count));
    groups.truncate(20); // Keep top 20 co-change groups

    groups
}

use crate::parsers::extract_tool_sequence;
use crate::types::{SessionEntry, SessionLink, SessionLinkResult};
use std::collections::{BTreeSet, HashSet};

/// Link sessions that share edited files and are temporally close.
/// Pure structural computation: file overlap (Jaccard) + time proximity.
pub fn link_sessions(sessions: &[(String, Vec<SessionEntry>)]) -> SessionLinkResult {
    // Extract per-session: file set + timestamp range
    let mut session_data: Vec<SessionFileData> = Vec::new();

    for (session_id, entries) in sessions {
        let tool_uses = extract_tool_sequence(entries);
        let mut files: BTreeSet<String> = BTreeSet::new();
        let mut first_ts: Option<i64> = None;
        let mut last_ts: Option<i64> = None;

        for tool in &tool_uses {
            if matches!(tool.name.as_str(), "Edit" | "Write" | "NotebookEdit") {
                if let Some(input) = &tool.input {
                    let path = input
                        .get("file_path")
                        .or_else(|| input.get("notebook_path"))
                        .and_then(|v| v.as_str());
                    if let Some(p) = path {
                        files.insert(p.to_string());
                    }
                }
            }
            if let Some(ts) = tool.timestamp {
                if first_ts.is_none() || ts < first_ts.unwrap() {
                    first_ts = Some(ts);
                }
                if last_ts.is_none() || ts > last_ts.unwrap() {
                    last_ts = Some(ts);
                }
            }
        }

        if !files.is_empty() {
            session_data.push(SessionFileData {
                session_id: session_id.clone(),
                files,
                first_timestamp: first_ts,
                last_timestamp: last_ts,
            });
        }
    }

    // Pairwise comparison: Jaccard file overlap
    let mut links: Vec<SessionLink> = Vec::new();

    for i in 0..session_data.len() {
        for j in (i + 1)..session_data.len() {
            let a = &session_data[i];
            let b = &session_data[j];

            let set_a: HashSet<&String> = a.files.iter().collect();
            let set_b: HashSet<&String> = b.files.iter().collect();

            let intersection = set_a.intersection(&set_b).count();
            if intersection == 0 {
                continue;
            }

            let union = set_a.union(&set_b).count();
            let overlap_ratio = intersection as f64 / union as f64;

            // Time gap between sessions (end of earlier → start of later)
            let time_gap = match (
                a.last_timestamp,
                b.first_timestamp,
                b.last_timestamp,
                a.first_timestamp,
            ) {
                (Some(a_last), Some(b_first), _, _) if b_first >= a_last => {
                    Some((b_first - a_last) / (60 * 1000)) // minutes
                }
                (_, _, Some(b_last), Some(a_first)) if a_first >= b_last => {
                    Some((a_first - b_last) / (60 * 1000))
                }
                _ => None,
            };

            let shared_files: Vec<String> =
                set_a.intersection(&set_b).map(|s| (*s).clone()).collect();

            links.push(SessionLink {
                session_a: a.session_id.clone(),
                session_b: b.session_id.clone(),
                shared_files,
                file_overlap_ratio: (overlap_ratio * 100.0).round() / 100.0,
                time_gap_minutes: time_gap,
            });
        }
    }

    // Sort by overlap ratio descending
    links.sort_by(|a, b| {
        b.file_overlap_ratio
            .partial_cmp(&a.file_overlap_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Keep top linked pairs
    links.truncate(20);

    SessionLinkResult { links }
}

struct SessionFileData {
    session_id: String,
    files: BTreeSet<String>,
    first_timestamp: Option<i64>,
    last_timestamp: Option<i64>,
}

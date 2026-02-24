use crate::analyzers::tool_classifier::classify_tool;
use crate::analyzers::tuning::TuningConfig;
use crate::parsers::extract_tool_sequence;
use crate::types::{HistoryEntry, SessionEntry, ToolTrend, TrendResult, WeeklyBucket};
use chrono::{DateTime, Datelike, IsoWeek, Utc};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Aggregate statistics by ISO week. Pure temporal grouping — no rules.
pub fn analyze_trends(
    sessions: &[(String, Vec<SessionEntry>)],
    history_entries: &[HistoryEntry],
    tuning: &TuningConfig,
) -> TrendResult {
    // Build per-session file sets for weekly file counting
    let mut session_files: HashMap<String, HashSet<String>> = HashMap::new();
    let mut session_tools: HashMap<String, Vec<String>> = HashMap::new();
    let mut session_timestamps: HashMap<String, i64> = HashMap::new();

    for (session_id, entries) in sessions {
        let tool_uses = extract_tool_sequence(entries);
        let classified: Vec<String> = tool_uses
            .iter()
            .map(|t| classify_tool(&t.name, t.input.as_ref()).classified_name)
            .collect();

        let mut files: HashSet<String> = HashSet::new();
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
        }

        // Use first tool timestamp as session timestamp
        let ts = tool_uses.first().and_then(|t| t.timestamp).unwrap_or(0);

        session_files.insert(session_id.clone(), files);
        session_tools.insert(session_id.clone(), classified);
        session_timestamps.insert(session_id.clone(), ts);
    }

    // Group prompts by ISO week
    let mut weekly_prompts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in history_entries {
        let week_key = timestamp_to_week_key(entry.timestamp);
        *weekly_prompts.entry(week_key).or_insert(0) += 1;
    }

    // Group sessions by ISO week
    let mut weekly_sessions: BTreeMap<String, usize> = BTreeMap::new();
    let mut weekly_tools: BTreeMap<String, HashMap<String, usize>> = BTreeMap::new();
    let mut weekly_files: BTreeMap<String, HashSet<String>> = BTreeMap::new();

    for (session_id, &ts) in &session_timestamps {
        let week_key = timestamp_to_week_key(ts);
        *weekly_sessions.entry(week_key.clone()).or_insert(0) += 1;

        if let Some(tools) = session_tools.get(session_id) {
            let tool_map = weekly_tools.entry(week_key.clone()).or_default();
            for tool in tools {
                *tool_map.entry(tool.clone()).or_insert(0) += 1;
            }
        }

        if let Some(files) = session_files.get(session_id) {
            weekly_files
                .entry(week_key)
                .or_default()
                .extend(files.iter().cloned());
        }
    }

    // Build weekly buckets (union of all week keys)
    let all_weeks: BTreeMap<String, ()> = weekly_prompts
        .keys()
        .chain(weekly_sessions.keys())
        .map(|k| (k.clone(), ()))
        .collect();

    let weeks: Vec<WeeklyBucket> = all_weeks
        .keys()
        .map(|week| {
            let mut tool_counts: Vec<(String, usize)> = weekly_tools
                .get(week)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
                .unwrap_or_default();
            tool_counts.sort_by(|a, b| b.1.cmp(&a.1));

            WeeklyBucket {
                week: week.clone(),
                prompt_count: *weekly_prompts.get(week).unwrap_or(&0),
                session_count: *weekly_sessions.get(week).unwrap_or(&0),
                tool_counts,
                unique_files_edited: weekly_files.get(week).map(|f| f.len()).unwrap_or(0),
            }
        })
        .collect();

    // Compute tool trends (linear regression slope + R² over weeks)
    let tool_trends = compute_tool_trends(&weeks, tuning.min_trend_r_squared);

    TrendResult { weeks, tool_trends }
}

/// Compute per-tool weekly counts with linear regression slope and R².
/// Trends with R² below `min_r_squared` have their slope zeroed out to avoid
/// reporting noise as a meaningful trend.
fn compute_tool_trends(weeks: &[WeeklyBucket], min_r_squared: f64) -> Vec<ToolTrend> {
    if weeks.len() < 2 {
        return Vec::new();
    }

    // Collect all tool names
    let mut all_tools: HashSet<String> = HashSet::new();
    for week in weeks {
        for (tool, _) in &week.tool_counts {
            all_tools.insert(tool.clone());
        }
    }

    let n = weeks.len() as f64;
    let mut trends: Vec<ToolTrend> = Vec::new();

    for tool in all_tools {
        let weekly_counts: Vec<usize> = weeks
            .iter()
            .map(|w| {
                w.tool_counts
                    .iter()
                    .find(|(t, _)| t == &tool)
                    .map(|(_, c)| *c)
                    .unwrap_or(0)
            })
            .collect();

        // Linear regression: y = a + b*x
        let x_mean = (n - 1.0) / 2.0;
        let y_mean = weekly_counts.iter().sum::<usize>() as f64 / n;

        let mut ss_xy = 0.0; // Σ(xi - x̄)(yi - ȳ)
        let mut ss_xx = 0.0; // Σ(xi - x̄)²
        let mut ss_tot = 0.0; // Σ(yi - ȳ)²  (total sum of squares)
        for (i, &count) in weekly_counts.iter().enumerate() {
            let x_diff = i as f64 - x_mean;
            let y_diff = count as f64 - y_mean;
            ss_xy += x_diff * y_diff;
            ss_xx += x_diff * x_diff;
            ss_tot += y_diff * y_diff;
        }

        let slope = if ss_xx > 0.0 { ss_xy / ss_xx } else { 0.0 };

        // R² = 1 - SS_res / SS_tot
        // SS_res = SS_tot - slope² × SS_xx  (algebraic identity)
        let r_squared = if ss_tot > 0.0 {
            let ss_res = ss_tot - slope * slope * ss_xx;
            (1.0 - ss_res / ss_tot).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // If the fit is too weak, zero the slope so it's not mistaken for a real trend
        let effective_slope = if r_squared >= min_r_squared {
            slope
        } else {
            0.0
        };

        trends.push(ToolTrend {
            tool,
            weekly_counts,
            trend_slope: (effective_slope * 100.0).round() / 100.0,
            r_squared: (r_squared * 1000.0).round() / 1000.0,
        });
    }

    // Sort by absolute slope descending (most changing tools first)
    trends.sort_by(|a, b| {
        b.trend_slope
            .abs()
            .partial_cmp(&a.trend_slope.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    trends
}

fn timestamp_to_week_key(timestamp_ms: i64) -> String {
    DateTime::from_timestamp_millis(timestamp_ms)
        .map(|dt: DateTime<Utc>| {
            let iso_week: IsoWeek = dt.iso_week();
            format!("{}-W{:02}", iso_week.year(), iso_week.week())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

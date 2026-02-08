use std::collections::HashMap;
use chrono::{DateTime, Utc};
use crate::types::{HistoryEntry, PromptAnalysisResult, PromptFrequency};
use crate::analyzers::suffix_miner::SuffixMiner;

pub fn analyze_prompts(
    entries: &[HistoryEntry],
    decay: bool,
    half_life_days: f64,
) -> PromptAnalysisResult {
    if entries.is_empty() {
        return PromptAnalysisResult {
            total: 0,
            unique: 0,
            start_date: None,
            end_date: None,
            top_prompts: Vec::new(),
        };
    }

    let total = entries.len();
    let half_life_ms = half_life_days * 24.0 * 60.0 * 60.0 * 1000.0;
    let reference_time = Utc::now().timestamp_millis();

    // Mine suffixes from prompt corpus for normalization
    let prompt_texts: Vec<&str> = entries.iter().map(|e| e.display.as_str()).collect();
    let suffix_miner = SuffixMiner::default();
    let discovered_suffixes = suffix_miner.mine(&prompt_texts);

    let mut prompt_map: HashMap<String, (usize, f64, i64, String)> = HashMap::new();
    let mut earliest = i64::MAX;
    let mut latest = i64::MIN;

    for entry in entries {
        // Normalize using suffix mining (same as tacit analysis)
        let normalized = suffix_miner.normalize(&entry.display, &discovered_suffixes);
        let key = normalized.content.trim().to_lowercase();

        let age_ms = reference_time - entry.timestamp;
        let decay_weight = if decay {
            (0.5_f64).powf(age_ms.max(0) as f64 / half_life_ms)
        } else {
            1.0
        };

        let data = prompt_map.entry(key).or_insert((0, 0.0, entry.timestamp, entry.display.clone()));
        data.0 += 1;
        data.1 += decay_weight;
        data.2 = data.2.max(entry.timestamp);

        earliest = earliest.min(entry.timestamp);
        latest = latest.max(entry.timestamp);
    }

    let unique = prompt_map.len();

    let mut top_prompts: Vec<PromptFrequency> = prompt_map
        .into_iter()
        .map(|(_, (count, weighted_count, last_used, display))| {
            let datetime = DateTime::from_timestamp_millis(last_used)
                .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap());

            PromptFrequency {
                prompt: display,
                count,
                weighted_count,
                last_used: datetime.to_rfc3339(),
            }
        })
        .collect();

    if decay {
        top_prompts.sort_by(|a, b| {
            b.weighted_count
                .partial_cmp(&a.weighted_count)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.count.cmp(&a.count))
        });
    } else {
        top_prompts.sort_by(|a, b| b.count.cmp(&a.count));
    }

    let start_date = if earliest != i64::MAX {
        DateTime::from_timestamp_millis(earliest)
            .map(|dt| dt.to_rfc3339())
    } else {
        None
    };

    let end_date = if latest != i64::MIN {
        DateTime::from_timestamp_millis(latest)
            .map(|dt| dt.to_rfc3339())
    } else {
        None
    };

    PromptAnalysisResult {
        total,
        unique,
        start_date,
        end_date,
        top_prompts,
    }
}

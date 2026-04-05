use anyhow::Result;

use super::analysis::{AnalysisContext, AnalysisOutcome, DiffAnalysis};
use super::EXIT_STAGNATION;
use crate::cmd::simhash;

/// Minimum number of history entries to attempt pattern detection.
const MIN_HISTORY_LEN: usize = 2;

/// Hamming distance threshold: entries within this distance are considered "similar".
const SIMILARITY_THRESHOLD: u32 = 5;

/// How many similar entries needed to flag stagnation.
const STAGNATION_COUNT: usize = 2;

/// Detects stagnation patterns by comparing simhash history in LoopState.
///
/// If recent output hashes show repeated similar patterns, overrides
/// the exit code to EXIT_STAGNATION and includes candidate details.
pub struct StagnationAnalysis;

fn no_stagnation() -> AnalysisOutcome {
    AnalysisOutcome {
        exit_override: None,
        extra_fields: serde_json::json!({}),
    }
}

impl DiffAnalysis for StagnationAnalysis {
    fn analyze(&self, ctx: &AnalysisContext) -> Result<AnalysisOutcome> {
        let history = &ctx.state.output_history;

        if history.len() < MIN_HISTORY_LEN {
            return Ok(no_stagnation());
        }

        let latest = &history[history.len() - 1];
        let latest_hash = match simhash::parse_simhash(&latest.simhash) {
            Some(h) => h,
            None => return Ok(no_stagnation()),
        };

        let mut candidates = Vec::new();
        for entry in &history[..history.len() - 1] {
            if let Some(h) = simhash::parse_simhash(&entry.simhash) {
                let distance = simhash::hamming_distance(latest_hash, h);
                candidates.push(serde_json::json!({
                    "simhash": simhash::format_simhash(h),
                    "distance": distance,
                    "category": entry.category,
                    "timestamp": entry.timestamp,
                }));
            }
        }

        candidates.sort_by_key(|c| c["distance"].as_u64().unwrap_or(64));

        let similar_count = candidates
            .iter()
            .filter(|c| c["distance"].as_u64().unwrap_or(64) <= SIMILARITY_THRESHOLD as u64)
            .count();

        if similar_count >= STAGNATION_COUNT {
            Ok(AnalysisOutcome {
                exit_override: Some(EXIT_STAGNATION),
                extra_fields: serde_json::json!({
                    "stagnation": {
                        "detected": true,
                        "current_simhash": simhash::format_simhash(latest_hash),
                        "similar_count": similar_count,
                        "candidates": candidates,
                    }
                }),
            })
        } else {
            Ok(no_stagnation())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::check::state::{LoopState, OutputCategory, OutputEntry};

    fn make_ctx(history: Vec<OutputEntry>) -> (Vec<String>, LoopState) {
        let state = LoopState {
            hash: "abc1234".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            output_history: history,
        };
        (vec!["src/lib.rs".to_string()], state)
    }

    #[test]
    fn no_stagnation_with_empty_history() {
        let (files, state) = make_ctx(vec![]);
        let ctx = AnalysisContext {
            loop_name: "gap-watch",
            changed_files: &files,
            spec_files: &[],
            code_files: &files,
            state: &state,
        };
        let result = StagnationAnalysis.analyze(&ctx).unwrap();
        assert!(result.exit_override.is_none());
    }

    #[test]
    fn no_stagnation_with_different_hashes() {
        let (files, state) = make_ctx(vec![
            OutputEntry {
                simhash: "0x0000000000000001".to_string(),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
            OutputEntry {
                simhash: "0xFFFFFFFFFFFFFFFF".to_string(),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-02T00:00:00Z".to_string(),
            },
        ]);
        let ctx = AnalysisContext {
            loop_name: "gap-watch",
            changed_files: &files,
            spec_files: &[],
            code_files: &files,
            state: &state,
        };
        let result = StagnationAnalysis.analyze(&ctx).unwrap();
        assert!(result.exit_override.is_none());
    }

    #[test]
    fn detects_stagnation_with_similar_hashes() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        let similar = base ^ 0x01; // 1 bit different
        let (files, state) = make_ctx(vec![
            OutputEntry {
                simhash: simhash::format_simhash(base),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
            OutputEntry {
                simhash: simhash::format_simhash(similar),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-02T00:00:00Z".to_string(),
            },
            OutputEntry {
                simhash: simhash::format_simhash(base),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-03T00:00:00Z".to_string(),
            },
        ]);
        let ctx = AnalysisContext {
            loop_name: "gap-watch",
            changed_files: &files,
            spec_files: &[],
            code_files: &files,
            state: &state,
        };
        let result = StagnationAnalysis.analyze(&ctx).unwrap();
        assert_eq!(result.exit_override, Some(EXIT_STAGNATION));
        assert!(result.extra_fields["stagnation"]["detected"]
            .as_bool()
            .unwrap());
    }
}

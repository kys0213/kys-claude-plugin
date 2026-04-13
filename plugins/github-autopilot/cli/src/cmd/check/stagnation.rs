use anyhow::Result;
use serde::Serialize;

use super::analysis::{AnalysisContext, AnalysisOutcome, DiffAnalysis};
use super::EXIT_STAGNATION;
use crate::cmd::check::state::OutputEntry;
use crate::cmd::simhash;

/// Default minimum number of history entries to attempt pattern detection.
const DEFAULT_MIN_HISTORY_LEN: usize = 2;

/// Default hamming distance threshold: entries within this distance are considered "similar".
pub const DEFAULT_SIMILARITY_THRESHOLD: u32 = 5;

/// Default number of similar entries needed to flag stagnation.
const DEFAULT_STAGNATION_COUNT: usize = 2;

/// Classified stagnation pattern types (priority order).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// Same hash repeating (distance ≤ 3 for all pairs)
    Spinning,
    /// A↔B alternating pattern (even/odd entries cluster separately)
    Oscillation,
    /// All entries similar but no improvement trend
    NoDrift,
    /// Distances gradually decreasing but still within threshold
    DiminishingReturns,
}

impl PatternType {
    /// Deterministic persona recommendation per pattern.
    pub fn recommended_persona(self) -> &'static str {
        match self {
            PatternType::Spinning => "hacker",
            PatternType::Oscillation => "architect",
            PatternType::NoDrift => "researcher",
            PatternType::DiminishingReturns => "simplifier",
        }
    }
}

/// Detects stagnation patterns by comparing simhash history in LoopState.
///
/// If recent output hashes show repeated similar patterns, overrides
/// the exit code to EXIT_STAGNATION and includes candidate details.
///
/// Thresholds are configurable for tuning with real gap report data (#578).
pub struct StagnationAnalysis {
    pub similarity_threshold: u32,
    pub stagnation_count: usize,
    pub min_history_len: usize,
}

impl Default for StagnationAnalysis {
    fn default() -> Self {
        Self {
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            stagnation_count: DEFAULT_STAGNATION_COUNT,
            min_history_len: DEFAULT_MIN_HISTORY_LEN,
        }
    }
}

/// Classify the stagnation pattern from a history of simhashes.
/// Returns None if no stagnation pattern is detected.
///
/// Priority: Spinning > Oscillation > NoDrift > DiminishingReturns
pub fn classify_pattern(
    history: &[OutputEntry],
    latest_hash: u64,
    threshold: u32,
) -> Option<PatternType> {
    let distances: Vec<u32> = history[..history.len() - 1]
        .iter()
        .filter_map(|e| simhash::parse_simhash(&e.simhash))
        .map(|h| simhash::hamming_distance(latest_hash, h))
        .collect();

    if distances.is_empty() {
        return None;
    }

    // Spinning: all distances very small (≤ 3)
    let spinning_threshold = 3.min(threshold);
    if distances.len() >= 2 && distances.iter().all(|&d| d <= spinning_threshold) {
        return Some(PatternType::Spinning);
    }

    // Oscillation: parse full hash sequence, check even/odd clustering
    if distances.len() >= 3 {
        let hashes: Vec<u64> = history
            .iter()
            .filter_map(|e| simhash::parse_simhash(&e.simhash))
            .collect();
        if hashes.len() >= 4 {
            let even_similar = (0..hashes.len() - 2)
                .filter(|&i| simhash::hamming_distance(hashes[i], hashes[i + 2]) <= threshold)
                .count();
            let odd_different = (0..hashes.len() - 1)
                .filter(|&i| simhash::hamming_distance(hashes[i], hashes[i + 1]) > threshold)
                .count();
            // At least 2 even-skip matches and most adjacent pairs are different
            if even_similar >= 2 && odd_different >= hashes.len() / 2 {
                return Some(PatternType::Oscillation);
            }
        }
    }

    // NoDrift: all within threshold, no clear improvement trend
    if distances.len() >= 2 && distances.iter().all(|&d| d <= threshold) {
        // Check if NOT diminishing (no clear downward trend)
        let is_diminishing = distances.windows(2).all(|w| w[1] <= w[0]);
        if !is_diminishing || distances.first() == distances.last() {
            return Some(PatternType::NoDrift);
        }
    }

    // DiminishingReturns: distances are decreasing and all within threshold
    if distances.len() >= 2 && distances.iter().all(|&d| d <= threshold) {
        let is_diminishing = distances.windows(2).all(|w| w[1] <= w[0]);
        if is_diminishing && distances.first() != distances.last() {
            return Some(PatternType::DiminishingReturns);
        }
    }

    None
}

fn no_stagnation() -> AnalysisOutcome {
    AnalysisOutcome {
        exit_override: None,
        extra_fields: serde_json::json!({}),
    }
}

impl DiffAnalysis for StagnationAnalysis {
    fn analyze(&self, ctx: &AnalysisContext) -> Result<AnalysisOutcome> {
        let history = &ctx.state.output_history;

        if history.len() < self.min_history_len {
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
            .filter(|c| c["distance"].as_u64().unwrap_or(64) <= self.similarity_threshold as u64)
            .count();

        if similar_count >= self.stagnation_count {
            let pattern = classify_pattern(history, latest_hash, self.similarity_threshold);
            let persona = pattern.map(|p| p.recommended_persona());

            Ok(AnalysisOutcome {
                exit_override: Some(EXIT_STAGNATION),
                extra_fields: serde_json::json!({
                    "stagnation": {
                        "detected": true,
                        "pattern_type": pattern,
                        "recommended_persona": persona,
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
            ..Default::default()
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
        let result = StagnationAnalysis::default().analyze(&ctx).unwrap();
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
        let result = StagnationAnalysis::default().analyze(&ctx).unwrap();
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
        let result = StagnationAnalysis::default().analyze(&ctx).unwrap();
        assert_eq!(result.exit_override, Some(EXIT_STAGNATION));
        assert!(result.extra_fields["stagnation"]["detected"]
            .as_bool()
            .unwrap());
    }

    #[test]
    fn stricter_threshold_detects_more() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        let slightly_different = base ^ 0x07; // 3 bits different
        let (files, state) = make_ctx(vec![
            OutputEntry {
                simhash: simhash::format_simhash(base),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
            OutputEntry {
                simhash: simhash::format_simhash(slightly_different),
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

        // Default threshold=5 detects (3 bits < 5)
        let result = StagnationAnalysis::default().analyze(&ctx).unwrap();
        assert_eq!(result.exit_override, Some(EXIT_STAGNATION));

        // Tight threshold=2 does NOT detect (3 bits > 2)
        let tight = StagnationAnalysis {
            similarity_threshold: 2,
            ..Default::default()
        };
        let result = tight.analyze(&ctx).unwrap();
        assert!(result.exit_override.is_none());
    }

    #[test]
    fn higher_stagnation_count_requires_more_matches() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        let (files, state) = make_ctx(vec![
            OutputEntry {
                simhash: simhash::format_simhash(base),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
            OutputEntry {
                simhash: simhash::format_simhash(base ^ 0x01),
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

        // Default count=2 detects (2 similar entries)
        let result = StagnationAnalysis::default().analyze(&ctx).unwrap();
        assert_eq!(result.exit_override, Some(EXIT_STAGNATION));

        // Require count=5 → not enough matches
        let high_count = StagnationAnalysis {
            stagnation_count: 5,
            ..Default::default()
        };
        let result = high_count.analyze(&ctx).unwrap();
        assert!(result.exit_override.is_none());
    }

    #[test]
    fn min_history_len_gates_detection() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        // 3 entries: latest compares against 2 previous, both similar → stagnation
        let (files, state) = make_ctx(vec![
            OutputEntry {
                simhash: simhash::format_simhash(base),
                category: OutputCategory::GapAnalysis,
                timestamp: "2026-01-01T00:00:00Z".to_string(),
            },
            OutputEntry {
                simhash: simhash::format_simhash(base ^ 0x01),
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

        // Default min_history=2, 3 entries → detection proceeds
        let result = StagnationAnalysis::default().analyze(&ctx).unwrap();
        assert_eq!(result.exit_override, Some(EXIT_STAGNATION));

        // Require min_history=5 → not enough history, skips detection
        let high_min = StagnationAnalysis {
            min_history_len: 5,
            ..Default::default()
        };
        let result = high_min.analyze(&ctx).unwrap();
        assert!(result.exit_override.is_none());
    }

    fn entry(hash: u64) -> OutputEntry {
        OutputEntry {
            simhash: simhash::format_simhash(hash),
            category: OutputCategory::GapAnalysis,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn classifies_spinning_pattern() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        let history = vec![
            entry(base),
            entry(base ^ 0x01),
            entry(base),
            entry(base ^ 0x01),
        ];
        let pattern = classify_pattern(&history, base ^ 0x01, 5);
        assert_eq!(pattern, Some(PatternType::Spinning));
        assert_eq!(PatternType::Spinning.recommended_persona(), "hacker");
    }

    #[test]
    fn classifies_oscillation_pattern() {
        let a: u64 = 0xA3F2B81C4D5E6F1B;
        let b: u64 = 0x1234567890ABCDEF; // very different from a
                                         // A, B, A, B → even indices similar, odd pairs different
        let history = vec![entry(a), entry(b), entry(a), entry(b)];
        let pattern = classify_pattern(&history, b, 5);
        assert_eq!(pattern, Some(PatternType::Oscillation));
        assert_eq!(PatternType::Oscillation.recommended_persona(), "architect");
    }

    #[test]
    fn classifies_no_drift_pattern() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        // All within threshold but not spinning (distance > 3 but ≤ 5)
        let history = vec![
            entry(base ^ 0x0F), // 4 bits from latest
            entry(base ^ 0x17), // ~4 bits from latest
            entry(base ^ 0x0F), // 4 bits from latest
            entry(base),        // latest
        ];
        let pattern = classify_pattern(&history, base, 5);
        assert_eq!(pattern, Some(PatternType::NoDrift));
        assert_eq!(PatternType::NoDrift.recommended_persona(), "researcher");
    }

    #[test]
    fn classifies_diminishing_returns() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        // Distances decreasing: 5, 3, 1
        let history = vec![
            entry(base ^ 0x1F), // 5 bits
            entry(base ^ 0x07), // 3 bits
            entry(base ^ 0x01), // 1 bit
            entry(base),        // latest
        ];
        let pattern = classify_pattern(&history, base, 5);
        assert_eq!(pattern, Some(PatternType::DiminishingReturns));
        assert_eq!(
            PatternType::DiminishingReturns.recommended_persona(),
            "simplifier"
        );
    }

    #[test]
    fn no_pattern_when_insufficient_history() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        let history = vec![entry(base)];
        let pattern = classify_pattern(&history, base, 5);
        assert_eq!(pattern, None);
    }

    #[test]
    fn stagnation_output_includes_pattern_and_persona() {
        let base: u64 = 0xA3F2B81C4D5E6F1B;
        let (files, state) = make_ctx(vec![entry(base), entry(base ^ 0x01), entry(base)]);
        let ctx = AnalysisContext {
            loop_name: "gap-watch",
            changed_files: &files,
            spec_files: &[],
            code_files: &files,
            state: &state,
        };
        let result = StagnationAnalysis::default().analyze(&ctx).unwrap();
        assert_eq!(result.exit_override, Some(EXIT_STAGNATION));

        let stag = &result.extra_fields["stagnation"];
        assert!(stag["pattern_type"].is_string());
        assert!(stag["recommended_persona"].is_string());
    }
}

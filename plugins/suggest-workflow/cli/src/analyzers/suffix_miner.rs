use std::collections::HashMap;

/// A suffix discovered from corpus analysis
#[derive(Debug, Clone)]
pub struct DiscoveredSuffix {
    pub text: String,
}

/// A prompt after suffix normalization
#[derive(Debug, Clone)]
pub struct NormalizedPrompt {
    /// Suffix-stripped content
    pub content: String,
}

/// Cold-start fallback suffixes for small corpora
const FALLBACK_SUFFIXES: &[&str] = &["해줘", "해주세요", "하세요", "해", "줘"];

/// Mines frequent suffixes from a collection of prompts
pub struct SuffixMiner {
    min_n: usize,      // min char n-gram length (default: 2)
    max_n: usize,      // max char n-gram length (default: 10)
    min_freq_pct: f64, // min frequency ratio (default: 0.02)
}

impl SuffixMiner {
    pub fn new(min_n: usize, max_n: usize, min_freq_pct: f64) -> Self {
        Self {
            min_n,
            max_n,
            min_freq_pct,
        }
    }

    /// Mine frequent suffixes from prompts.
    /// Uses char-based n-gram extraction (byte-safe for Korean).
    /// Longer suffixes get priority via greedy longest match.
    /// Counts are exclusive: a prompt matching "해주세요" won't also count for "해줘".
    pub fn mine(&self, prompts: &[&str]) -> Vec<DiscoveredSuffix> {
        let n = prompts.len();
        if n == 0 {
            return Vec::new();
        }

        // Step 1: Extract all char n-grams from suffix positions
        let mut suffix_counts: HashMap<String, usize> = HashMap::new();
        for prompt in prompts {
            let trimmed = prompt.trim();
            let chars: Vec<char> = trimmed.chars().collect();
            let len = chars.len();
            let mut seen_for_prompt: Vec<String> = Vec::new();
            for ngram_len in self.min_n..=self.max_n.min(len) {
                let suffix: String = chars[len - ngram_len..].iter().collect();
                seen_for_prompt.push(suffix);
            }
            for suffix in &seen_for_prompt {
                *suffix_counts.entry(suffix.clone()).or_insert(0) += 1;
            }
        }

        // Step 2: Filter by min_support = max(3, ceil(N * min_freq_pct))
        let min_support = (3_usize).max((n as f64 * self.min_freq_pct).ceil() as usize);
        let mut candidates: Vec<(String, usize)> = suffix_counts
            .into_iter()
            .filter(|(_, count)| *count >= min_support)
            .collect();

        // Step 3: Sort by length descending (longest first for greedy matching)
        candidates.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()));

        // Step 4: Exclusive counting - re-count with greedy longest match
        let mut exclusive_counts: HashMap<String, usize> = HashMap::new();
        for suffix_text in candidates.iter().map(|(t, _)| t.clone()) {
            exclusive_counts.insert(suffix_text, 0);
        }

        let candidate_texts: Vec<String> = candidates.iter().map(|(t, _)| t.clone()).collect();

        for prompt in prompts {
            let trimmed = prompt.trim();
            for suffix_text in &candidate_texts {
                if trimmed.ends_with(suffix_text.as_str()) {
                    *exclusive_counts.get_mut(suffix_text).unwrap() += 1;
                    break;
                }
            }
        }

        // Step 5: Build result, filter again by min_support after exclusive counting
        let mut result: Vec<DiscoveredSuffix> = exclusive_counts
            .into_iter()
            .filter(|(_, count)| *count >= min_support)
            .map(|(text, _)| DiscoveredSuffix { text })
            .collect();

        // Sort by length descending for normalize() longest-match
        result.sort_by(|a, b| b.text.chars().count().cmp(&a.text.chars().count()));

        // Step 6: Cold-start fallback - augment with defaults if corpus is small
        if n < 30 {
            for fallback in FALLBACK_SUFFIXES {
                if !result.iter().any(|s| s.text == *fallback) {
                    result.push(DiscoveredSuffix {
                        text: fallback.to_string(),
                    });
                }
            }
            result.sort_by(|a, b| b.text.chars().count().cmp(&a.text.chars().count()));
        }

        result
    }

    /// Normalize text by stripping the longest matching discovered suffix.
    pub fn normalize(&self, text: &str, suffixes: &[DiscoveredSuffix]) -> NormalizedPrompt {
        let trimmed = text.trim();
        for suffix in suffixes {
            if let Some(stripped) = trimmed.strip_suffix(suffix.text.as_str()) {
                let content = stripped.trim_end().to_string();
                if !content.is_empty() {
                    return NormalizedPrompt { content };
                }
            }
        }
        NormalizedPrompt {
            content: trimmed.to_string(),
        }
    }
}

impl Default for SuffixMiner {
    fn default() -> Self {
        Self::new(2, 10, 0.02)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_suffix_mining() {
        let prompts = vec![
            "타입을 명시해줘",
            "에러를 처리해줘",
            "const로 통일해줘",
            "커밋해줘",
            "테스트 작성해줘",
        ];
        let miner = SuffixMiner::default();
        let suffixes = miner.mine(&prompts);
        assert!(
            suffixes.iter().any(|s| s.text == "해줘"),
            "Should discover '해줘' suffix"
        );
    }

    #[test]
    fn test_longest_match_priority() {
        let prompts = vec![
            "타입을 명시해주세요",
            "에러를 처리해주세요",
            "const로 통일해주세요",
            "커밋해줘",
            "테스트 작성해줘",
            "lint 실행해줘",
        ];
        let miner = SuffixMiner::new(2, 10, 0.02);
        let suffixes = miner.mine(&prompts);
        if suffixes.len() >= 2 {
            assert!(
                suffixes[0].text.chars().count() >= suffixes[1].text.chars().count(),
                "Suffixes should be sorted by length descending"
            );
        }
    }

    #[test]
    fn test_normalization() {
        let miner = SuffixMiner::default();
        let suffixes = vec![
            DiscoveredSuffix {
                text: "해주세요".to_string(),
            },
            DiscoveredSuffix {
                text: "해줘".to_string(),
            },
            DiscoveredSuffix {
                text: "해".to_string(),
            },
        ];

        let r1 = miner.normalize("타입을 명시해줘", &suffixes);
        assert_eq!(r1.content, "타입을 명시");

        let r2 = miner.normalize("타입을 명시해주세요", &suffixes);
        assert_eq!(r2.content, "타입을 명시");
    }

    #[test]
    fn test_cold_start_fallback() {
        let prompts = vec!["hello world", "foo bar"];
        let miner = SuffixMiner::default();
        let suffixes = miner.mine(&prompts);
        assert!(
            suffixes.iter().any(|s| s.text == "해줘"),
            "Should have fallback '해줘'"
        );
        assert!(
            suffixes.iter().any(|s| s.text == "해주세요"),
            "Should have fallback '해주세요'"
        );
    }

    #[test]
    fn test_empty_input() {
        let miner = SuffixMiner::default();
        let suffixes = miner.mine(&[]);
        assert!(suffixes.is_empty());
    }

    #[test]
    fn test_byte_safety_korean() {
        let prompts = vec![
            "한글 테스트해줘",
            "유니코드 안전해주세요",
            "바이트 경계 확인해",
            "멀티바이트 문자열 처리해줘",
        ];
        let miner = SuffixMiner::default();
        let suffixes = miner.mine(&prompts);
        for prompt in &prompts {
            let _ = miner.normalize(prompt, &suffixes);
        }
    }
}

use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use crate::types::{HistoryEntry, TacitPattern, TacitAnalysisResult};
use crate::analyzers::bm25::BM25Ranker;
use crate::analyzers::suffix_miner::SuffixMiner;
use crate::analyzers::depth::DepthConfig;
use crate::analyzers::query_decomposer::decompose_query;
use crate::tokenizer::KoreanTokenizer;

// --- Type seed keywords (minimal hardcoding) ---

const TYPE_SEEDS: &[(&str, &[&str])] = &[
    ("directive",  &["항상", "반드시", "무조건", "절대", "꼭", "always", "must", "never"]),
    ("convention", &["컨벤션", "규칙", "스타일", "포맷", "convention", "standard"]),
    ("correction", &["말고", "대신", "아니라", "아니야", "틀렸", "instead"]),
    ("preference", &["좋아", "선호", "나아", "prefer", "better"]),
];

const TYPE_BOOST: &[(&str, f64)] = &[
    ("directive", 0.10),
    ("convention", 0.08),
    ("correction", 0.06),
    ("preference", 0.05),
];

// --- Stopwords & tokenizer (kept from original) ---

static STOPWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    ["응", "네", "좋아", "그래", "알겠어", "해줘", "해", "하자", "고마워", "감사", "ok", "yes"]
        .iter()
        .copied()
        .collect()
});

static KOREAN_TOKENIZER: Lazy<Option<KoreanTokenizer>> = Lazy::new(|| {
    KoreanTokenizer::new().ok()
});

/// Minimum character length for a prompt to be considered meaningful
const MIN_PROMPT_LENGTH: usize = 5;

/// Stopword-only or confirmation prompts to filter out
static CONFIRMATION_PROMPTS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "응", "네", "좋아", "그래", "알겠어", "해줘", "해", "하자", "고마워", "감사",
        "ok", "yes", "y", "sure", "thanks", "ㅇ", "ㅇㅇ", "넵",
    ]
    .iter()
    .copied()
    .collect()
});

// --- Internal types ---

#[derive(Debug, Clone)]
struct ClusterEntry {
    original: String,
    normalized_content: String,
    timestamp: i64,
}

// --- Boundary matching for seed keywords ---

/// Check if seed appears at a word boundary in text.
/// For Korean: at least one side must be whitespace/punctuation/string boundary.
/// This avoids false positives from substring matches.
fn contains_at_boundary(text: &str, seed: &str) -> bool {
    let text_lower = text.to_lowercase();
    let seed_lower = seed.to_lowercase();
    let mut search_from = 0;
    while let Some(pos) = text_lower[search_from..].find(&seed_lower) {
        let abs_pos = search_from + pos;
        let before_ok = abs_pos == 0 || text_lower[..abs_pos]
            .ends_with(|c: char| c.is_whitespace() || c.is_ascii_punctuation());
        let after_pos = abs_pos + seed_lower.len();
        let after_ok = after_pos >= text_lower.len() || text_lower[after_pos..]
            .starts_with(|c: char| c.is_whitespace() || c.is_ascii_punctuation());
        if before_ok || after_ok {
            return true;
        }
        // Move past this occurrence
        search_from = abs_pos + seed_lower.len();
        if search_from >= text_lower.len() {
            break;
        }
    }
    false
}

/// Classify text type by seed keywords. Priority: directive > convention > correction > preference.
/// Returns "general" if no seed matches.
fn classify_type(text: &str) -> &'static str {
    for (type_name, seeds) in TYPE_SEEDS {
        if seeds.iter().any(|seed| contains_at_boundary(text, seed)) {
            return type_name;
        }
    }
    "general"
}

fn get_type_boost(pattern_type: &str) -> f64 {
    TYPE_BOOST
        .iter()
        .find(|(t, _)| *t == pattern_type)
        .map(|(_, b)| *b)
        .unwrap_or(0.0)
}

// --- Tokenization (kept from original) ---

fn tokenize(text: &str) -> Vec<String> {
    if let Some(ref tokenizer) = *KOREAN_TOKENIZER {
        let tokens = tokenizer.tokenize(text);
        if !tokens.is_empty() {
            return tokens
                .into_iter()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty() && !STOPWORDS.contains(s.as_str()))
                .collect();
        }
    }

    text.split_whitespace()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && !STOPWORDS.contains(s.as_str()))
        .collect()
}

// --- Char bigram similarity ---

fn char_bigrams(s: &str) -> HashSet<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return HashSet::new();
    }
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}

fn char_bigram_similarity(a: &str, b: &str) -> f64 {
    let bigrams_a = char_bigrams(a);
    let bigrams_b = char_bigrams(b);

    if bigrams_a.is_empty() && bigrams_b.is_empty() {
        return if a == b { 1.0 } else { 0.0 };
    }

    let intersection = bigrams_a.intersection(&bigrams_b).count();
    let union = bigrams_a.union(&bigrams_b).count();

    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

// --- Clustering ---

/// Cluster normalized texts using:
/// Phase 1: Exact match grouping (O(n))
/// Phase 2: Jaccard similarity on char bigrams (O(k²), k = unique normalized, max 500)
/// Short strings (< 4 chars) skip Phase 2 bigram comparison.
fn cluster_normalized(
    entries: &[ClusterEntry],
    similarity_threshold: f64,
) -> Vec<Vec<ClusterEntry>> {
    // Phase 1: Group by exact normalized content
    let mut exact_groups: HashMap<String, Vec<ClusterEntry>> = HashMap::new();
    for entry in entries {
        let key = entry.normalized_content.trim().to_lowercase();
        exact_groups.entry(key).or_default().push(entry.clone());
    }

    // Collect representatives sorted by group size descending for stable results
    let mut representatives: Vec<(String, Vec<ClusterEntry>)> = exact_groups.into_iter().collect();
    representatives.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    // Truncate to max_k=500 if needed (drop lowest frequency groups)
    if representatives.len() > 500 {
        representatives.truncate(500);
    }

    // Phase 2: Merge similar groups via char bigram Jaccard
    let mut clusters: Vec<(String, Vec<ClusterEntry>)> = Vec::new();

    for (repr_text, group) in representatives {
        let repr_chars_count = repr_text.chars().count();
        let mut merged = false;

        // Short strings (< 4 chars): only exact match (already grouped in Phase 1)
        if repr_chars_count >= 4 {
            for (cluster_repr, cluster_entries) in clusters.iter_mut() {
                if cluster_repr.chars().count() < 4 {
                    continue;
                }
                let sim = char_bigram_similarity(&repr_text, cluster_repr);
                if sim >= similarity_threshold {
                    cluster_entries.extend(group.clone());
                    merged = true;
                    break;
                }
            }
        }

        if !merged {
            clusters.push((repr_text, group));
        }
    }

    clusters.into_iter().map(|(_, entries)| entries).collect()
}

// --- Scoring ---

fn calculate_consistency(timestamps: &[i64]) -> f64 {
    if timestamps.len() < 2 {
        return 0.0;
    }

    let mut sorted = timestamps.to_vec();
    sorted.sort();

    let intervals: Vec<f64> = sorted
        .windows(2)
        .map(|w| (w[1] - w[0]) as f64)
        .collect();

    if intervals.is_empty() {
        return 0.0;
    }

    let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }

    let variance = intervals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / intervals.len() as f64;
    let cv = variance.sqrt() / mean;
    1.0 / (1.0 + cv)
}

fn calculate_confidence(
    count: usize,
    total_prompts: usize,
    timestamps: &[i64],
    bm25_score: f64,
    pattern_type: &str,
) -> f64 {
    if count == 0 || total_prompts == 0 {
        return 0.0;
    }

    let frequency_score = (count as f64 / total_prompts as f64).min(1.0);
    let consistency_score = calculate_consistency(timestamps);
    let normalized_bm25 = 1.0 / (1.0 + (-bm25_score / 5.0).exp());

    let base = (frequency_score * 0.4) + (consistency_score * 0.2) + (normalized_bm25 * 0.4);
    let type_boost = get_type_boost(pattern_type);

    (base + type_boost).min(1.0) // Always clamp to [0, 1]
}

// --- Prompt filtering ---

fn is_meaningful_prompt(prompt: &str) -> bool {
    let trimmed = prompt.trim();
    if trimmed.chars().count() < MIN_PROMPT_LENGTH {
        return false;
    }
    if CONFIRMATION_PROMPTS.contains(trimmed) {
        return false;
    }
    if trimmed.starts_with('<') {
        return false;
    }
    if trimmed.starts_with("This session is being continued") {
        return false;
    }
    true
}

// --- Main analysis function ---

/// Data-driven tacit knowledge analysis pipeline with multi-query BM25:
/// 1. Filter meaningful prompts
/// 2. Mine frequent suffixes from corpus
/// 3. Normalize prompts (strip suffixes)
/// 4. Build BM25 ranker and decompose queries per DepthConfig
/// 5. Cluster normalized texts via char bigram similarity
/// 6. Score clusters with multi-query BM25 + frequency + consistency
/// 7. Label types via seed keywords
/// 8. Rank by confidence
pub fn analyze_tacit_knowledge(
    entries: &[HistoryEntry],
    threshold: usize,
    top_n: usize,
    depth_config: &DepthConfig,
) -> TacitAnalysisResult {
    if entries.is_empty() {
        return TacitAnalysisResult {
            total: 0,
            patterns: Vec::new(),
        };
    }

    // Step 1: Filter meaningful prompts
    let meaningful: Vec<&HistoryEntry> = entries
        .iter()
        .filter(|e| is_meaningful_prompt(&e.display))
        .collect();

    if meaningful.is_empty() {
        return TacitAnalysisResult {
            total: entries.len(),
            patterns: Vec::new(),
        };
    }

    // Step 2: Mine suffixes from corpus
    let prompt_texts: Vec<&str> = meaningful.iter().map(|e| e.display.as_str()).collect();
    let suffix_miner = SuffixMiner::default();
    let discovered_suffixes = suffix_miner.mine(&prompt_texts);

    // Step 3: Normalize all prompts
    let cluster_entries: Vec<ClusterEntry> = meaningful
        .iter()
        .map(|e| {
            let normalized = suffix_miner.normalize(&e.display, &discovered_suffixes);
            ClusterEntry {
                original: e.display.clone(),
                normalized_content: normalized.content,
                timestamp: e.timestamp,
            }
        })
        .collect();

    // Step 4: Build BM25 ranker from ORIGINAL texts (pre-normalization)
    let all_documents: Vec<Vec<String>> = meaningful
        .iter()
        .map(|e| tokenize(&e.display))
        .collect();
    let bm25_ranker = BM25Ranker::new(&all_documents, 1.5, 0.75);

    // Step 5: Cluster normalized texts (using depth-driven similarity threshold)
    let clusters = cluster_normalized(&cluster_entries, depth_config.similarity_threshold);

    // Step 6: Score and rank clusters with multi-query BM25
    let mut patterns = Vec::new();
    for cluster in clusters {
        if cluster.len() < threshold {
            continue;
        }

        // Use first entry's normalized content as representative
        let representative = cluster
            .first()
            .map(|e| e.normalized_content.clone())
            .unwrap_or_default();

        if representative.trim().is_empty() {
            continue;
        }

        // Multi-query BM25: decompose representative and score all sub-queries
        let decomposed = decompose_query(&representative, depth_config, &bm25_ranker);
        let bm25_score = if decomposed.original.is_empty() {
            0.0
        } else if decomposed.is_decomposed() {
            bm25_ranker.score_multi_query(
                &decomposed.all_queries(),
                depth_config.multi_query_strategy,
            )
        } else {
            bm25_ranker.score_query(&decomposed.original)
        };

        // Classify type using original prompts (seed matching on full text)
        let mut type_counts: HashMap<&str, usize> = HashMap::new();
        for entry in &cluster {
            let t = classify_type(&entry.original);
            *type_counts.entry(t).or_insert(0) += 1;
        }
        let dominant_type = type_counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(t, _)| t)
            .unwrap_or("general");

        // Calculate confidence with type boost and clamping
        let timestamps: Vec<i64> = cluster.iter().map(|e| e.timestamp).collect();
        let confidence = calculate_confidence(
            cluster.len(),
            entries.len(),
            &timestamps,
            bm25_score,
            dominant_type,
        );

        // Collect unique examples (up to 5)
        let examples: Vec<String> = cluster
            .iter()
            .map(|e| e.original.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .take(5)
            .collect();

        // Truncate representative for display
        let display_pattern = if representative.chars().count() > 80 {
            let s: String = representative.chars().take(77).collect();
            format!("{}...", s)
        } else {
            representative.clone()
        };

        patterns.push(TacitPattern {
            pattern_type: dominant_type.to_string(),
            pattern: display_pattern,
            normalized: representative.trim().to_lowercase(),
            examples,
            count: cluster.len(),
            bm25_score: (bm25_score * 100.0).round() / 100.0,
            confidence: (confidence * 100.0).round() / 100.0,
        });
    }

    // Sort by confidence desc, then count desc
    patterns.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.count.cmp(&a.count))
    });

    TacitAnalysisResult {
        total: entries.len(),
        patterns: patterns.into_iter().take(top_n).collect(),
    }
}

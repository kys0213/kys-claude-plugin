use std::sync::LazyLock;
use std::collections::{BTreeSet, HashMap, HashSet};
use crate::types::{HistoryEntry, TacitPattern, TacitAnalysisResult};
use crate::analyzers::bm25::BM25Ranker;
use crate::analyzers::suffix_miner::SuffixMiner;
use crate::analyzers::depth::DepthConfig;
use crate::analyzers::query_decomposer::decompose_query;
use crate::analyzers::stopwords::StopwordSet;
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

// --- Tokenizer ---

static KOREAN_TOKENIZER: LazyLock<Option<KoreanTokenizer>> = LazyLock::new(|| {
    KoreanTokenizer::new().ok()
});

/// Minimum character length for a prompt to be considered meaningful
const MIN_PROMPT_LENGTH: usize = 5;

// --- Internal types ---

#[derive(Debug, Clone)]
struct ClusterEntry {
    original: String,
    normalized_content: String,
    timestamp: i64,
}

// --- Boundary matching for seed keywords ---

/// Check if a character is a word boundary (whitespace or any Unicode punctuation).
fn is_boundary_char(c: char) -> bool {
    c.is_whitespace() || c.is_ascii_punctuation() || unicode_punctuation(c)
}

/// Check Unicode General_Category for punctuation beyond ASCII.
fn unicode_punctuation(c: char) -> bool {
    matches!(c,
        '\u{2000}'..='\u{206F}' |  // General Punctuation (…·†‡, hyphens, dashes, bullets)
        '\u{3000}'..='\u{303F}' |  // CJK Symbols and Punctuation (。、「」etc.)
        '\u{FE30}'..='\u{FE4F}' |  // CJK Compatibility Forms
        '\u{FF01}'..='\u{FF0F}' |  // Fullwidth punctuation (！～／)
        '\u{FF1A}'..='\u{FF20}' |  // Fullwidth colon to @
        '\u{FF3B}'..='\u{FF40}' |  // Fullwidth brackets
        '\u{FF5B}'..='\u{FF65}'    // Fullwidth braces, halfwidth forms
    )
}

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
            .ends_with(is_boundary_char);
        let after_pos = abs_pos + seed_lower.len();
        let after_ok = after_pos >= text_lower.len() || text_lower[after_pos..]
            .starts_with(is_boundary_char);
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

pub fn tokenize(text: &str, stopwords: &StopwordSet) -> Vec<String> {
    if let Some(ref tokenizer) = *KOREAN_TOKENIZER {
        let tokens = tokenizer.tokenize(text);
        if !tokens.is_empty() {
            return tokens
                .into_iter()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty() && !stopwords.contains(s.as_str()))
                .collect();
        }
    }

    text.split_whitespace()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && !stopwords.contains(s.as_str()))
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

fn bigram_similarity_precomputed(
    bigrams_a: &HashSet<(char, char)>,
    bigrams_b: &HashSet<(char, char)>,
) -> f64 {
    if bigrams_a.is_empty() && bigrams_b.is_empty() {
        return 0.0;
    }

    let intersection = bigrams_a.intersection(bigrams_b).count();
    let union = bigrams_a.union(bigrams_b).count();

    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

// --- Clustering ---

/// Cluster normalized texts using:
/// Phase 1: Exact match grouping (O(n))
/// Phase 2: Jaccard similarity on precomputed char bigrams (O(k²), k = unique normalized, max 500)
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

    // Precompute bigrams for all representatives (fixes P2: repeated bigram computation)
    let precomputed_bigrams: Vec<HashSet<(char, char)>> = representatives
        .iter()
        .map(|(text, _)| char_bigrams(text))
        .collect();

    // Phase 2: Merge similar groups via precomputed char bigram Jaccard (best-match)
    let mut clusters: Vec<(usize, Vec<ClusterEntry>)> = Vec::new(); // (original_index, entries)

    for (i, (repr_text, group)) in representatives.into_iter().enumerate() {
        let repr_chars_count = repr_text.chars().count();
        let mut best_match: Option<(usize, f64)> = None; // (cluster_position, similarity)

        // Short strings (< 4 chars): only exact match (already grouped in Phase 1)
        if repr_chars_count >= 4 {
            for (pos, (cluster_idx, _)) in clusters.iter().enumerate() {
                if precomputed_bigrams[*cluster_idx].is_empty() {
                    continue;
                }
                let sim = bigram_similarity_precomputed(
                    &precomputed_bigrams[i],
                    &precomputed_bigrams[*cluster_idx],
                );
                if sim >= similarity_threshold {
                    if best_match.map_or(true, |(_, best_sim)| sim > best_sim) {
                        best_match = Some((pos, sim));
                    }
                }
            }
        }

        if let Some((pos, _)) = best_match {
            clusters[pos].1.extend(group);
        } else {
            clusters.push((i, group));
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

/// Calculate recency score based on the most recent timestamp.
/// Uses exponential decay: score = exp(-age_days / half_life_days)
fn calculate_recency(timestamps: &[i64], now_ms: i64, half_life_days: f64) -> f64 {
    if timestamps.is_empty() || half_life_days <= 0.0 {
        return 0.0;
    }
    let most_recent = timestamps.iter().cloned().max().unwrap_or(0);
    let age_ms = (now_ms - most_recent).max(0) as f64;
    let age_days = age_ms / (1000.0 * 60.0 * 60.0 * 24.0);
    // Exponential decay: half_life_days controls how fast old patterns lose relevance
    (-age_days * (2.0_f64.ln()) / half_life_days).exp()
}

fn calculate_confidence(
    count: usize,
    meaningful_count: usize,
    timestamps: &[i64],
    bm25_score: f64,
    pattern_type: &str,
    decay: bool,
    now_ms: i64,
    decay_half_life_days: f64,
) -> f64 {
    if count == 0 || meaningful_count == 0 {
        return 0.0;
    }

    // B5 fix: use meaningful prompt count as denominator instead of total entries
    let frequency_score = (count as f64 / meaningful_count as f64).min(1.0);
    let consistency_score = calculate_consistency(timestamps);
    let normalized_bm25 = 1.0 / (1.0 + (-bm25_score / 5.0).exp());

    let base = if decay {
        let recency = calculate_recency(timestamps, now_ms, decay_half_life_days);
        // With decay: frequency 30%, consistency 15%, BM25 35%, recency 20%
        (frequency_score * 0.30) + (consistency_score * 0.15) + (normalized_bm25 * 0.35) + (recency * 0.20)
    } else {
        (frequency_score * 0.4) + (consistency_score * 0.2) + (normalized_bm25 * 0.4)
    };

    let type_boost = get_type_boost(pattern_type);

    (base + type_boost).min(1.0) // Always clamp to [0, 1]
}

// --- Prompt filtering ---

fn is_meaningful_prompt(prompt: &str, stopwords: &StopwordSet) -> bool {
    let trimmed = prompt.trim();
    if trimmed.chars().count() < MIN_PROMPT_LENGTH {
        return false;
    }
    if stopwords.contains(trimmed) {
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
    decay: bool,
    decay_half_life_days: f64,
    stopwords: &StopwordSet,
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
        .filter(|e| is_meaningful_prompt(&e.display, stopwords))
        .collect();

    if meaningful.is_empty() {
        return TacitAnalysisResult {
            total: entries.len(),
            patterns: Vec::new(),
        };
    }

    let meaningful_count = meaningful.len();

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
        .map(|e| tokenize(&e.display, stopwords))
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

        // Use most frequent normalized content as representative
        let representative = {
            let mut freq: HashMap<&str, usize> = HashMap::new();
            for entry in &cluster {
                *freq.entry(entry.normalized_content.as_str()).or_insert(0) += 1;
            }
            freq.into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(text, _)| text.to_string())
                .unwrap_or_default()
        };

        if representative.trim().is_empty() {
            continue;
        }

        // Multi-query BM25: decompose representative and score against corpus
        let decomposed = decompose_query(&representative, depth_config, &bm25_ranker, stopwords);
        let bm25_score = if decomposed.original.is_empty() {
            0.0
        } else if decomposed.is_decomposed() {
            // Score each sub-query against corpus and combine
            let queries = decomposed.all_queries();
            let scores: Vec<f64> = queries
                .iter()
                .filter(|q| !q.is_empty())
                .map(|q| bm25_ranker.score_against_corpus(q))
                .collect();
            if scores.is_empty() {
                0.0
            } else {
                match depth_config.multi_query_strategy {
                    crate::analyzers::bm25::MultiQueryStrategy::Max => {
                        scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
                    }
                    crate::analyzers::bm25::MultiQueryStrategy::Avg => {
                        scores.iter().sum::<f64>() / scores.len() as f64
                    }
                    crate::analyzers::bm25::MultiQueryStrategy::WeightedAvg => {
                        let mut total_weight = 0.0;
                        let mut weighted_sum = 0.0;
                        for (i, &score) in scores.iter().enumerate() {
                            let weight = 0.6_f64.powi(i as i32);
                            weighted_sum += score * weight;
                            total_weight += weight;
                        }
                        if total_weight > 0.0 { weighted_sum / total_weight } else { 0.0 }
                    }
                }
            }
        } else {
            bm25_ranker.score_against_corpus(&decomposed.original)
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

        // B5 fix: pass meaningful_count instead of entries.len()
        let timestamps: Vec<i64> = cluster.iter().map(|e| e.timestamp).collect();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let confidence = calculate_confidence(
            cluster.len(),
            meaningful_count,
            &timestamps,
            bm25_score,
            dominant_type,
            decay,
            now_ms,
            decay_half_life_days,
        );

        // B4 fix: use BTreeSet for deterministic example ordering
        let examples: Vec<String> = cluster
            .iter()
            .map(|e| e.original.clone())
            .collect::<BTreeSet<_>>()
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

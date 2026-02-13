use std::collections::{HashMap, HashSet};

/// Strategy for combining scores from multiple queries
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MultiQueryStrategy {
    /// Use the highest score among all sub-queries
    Max,
    /// Average all sub-query scores
    Avg,
    /// Weighted average: first query gets base weight, additional queries get decaying weights
    WeightedAvg,
}

pub struct BM25Ranker {
    k1: f64,
    b: f64,
    avg_dl: f64,
    idf: HashMap<String, f64>,
    documents: Vec<Vec<String>>,
}

#[allow(dead_code)]
impl BM25Ranker {
    pub fn new(documents: &[Vec<String>], k1: f64, b: f64) -> Self {
        let doc_count = documents.len();
        let avg_dl = if doc_count > 0 {
            documents.iter().map(|d| d.len()).sum::<usize>() as f64 / doc_count as f64
        } else {
            0.0
        };

        // Calculate document frequency
        let mut df: HashMap<String, usize> = HashMap::new();
        for doc in documents {
            let unique: HashSet<_> = doc.iter().collect();
            for term in unique {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
        }

        // Calculate IDF
        let idf: HashMap<String, f64> = df
            .into_iter()
            .map(|(term, freq)| {
                let idf_val = ((doc_count as f64 - freq as f64 + 0.5) / (freq as f64 + 0.5) + 1.0).ln();
                (term, idf_val)
            })
            .collect();

        Self {
            k1,
            b,
            avg_dl,
            idf,
            documents: documents.to_vec(),
        }
    }

    pub fn score_query(&self, query_tokens: &[String]) -> f64 {
        if query_tokens.is_empty() {
            return 0.0;
        }

        let dl = query_tokens.len() as f64;
        let mut score = 0.0;

        // Count term frequency in query
        let mut tf: HashMap<String, usize> = HashMap::new();
        for term in query_tokens {
            *tf.entry(term.clone()).or_insert(0) += 1;
        }

        for (term, freq) in tf {
            if let Some(&idf) = self.idf.get(&term) {
                let numerator = freq as f64 * (self.k1 + 1.0);
                let denominator = freq as f64 + self.k1 * (1.0 - self.b + self.b * (dl / self.avg_dl));
                score += idf * (numerator / denominator);
            }
        }

        score
    }

    /// Score multiple queries and combine using the given strategy.
    /// Each element in `queries` is a separate set of tokens (a sub-query).
    /// Empty queries are filtered out before scoring.
    /// `decay_factor` controls the weight decay for WeightedAvg (weight = decay^i).
    pub fn score_multi_query(
        &self,
        queries: &[Vec<String>],
        strategy: MultiQueryStrategy,
        decay_factor: f64,
    ) -> f64 {
        let scores: Vec<f64> = queries
            .iter()
            .filter(|q| !q.is_empty())
            .map(|q| self.score_query(q))
            .collect();

        if scores.is_empty() {
            return 0.0;
        }

        match strategy {
            MultiQueryStrategy::Max => {
                scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            }
            MultiQueryStrategy::Avg => {
                scores.iter().sum::<f64>() / scores.len() as f64
            }
            MultiQueryStrategy::WeightedAvg => {
                // First (original) query gets weight 1.0, subsequent queries decay
                let mut total_weight = 0.0;
                let mut weighted_sum = 0.0;
                for (i, &score) in scores.iter().enumerate() {
                    let weight = decay_factor.powi(i as i32);
                    weighted_sum += score * weight;
                    total_weight += weight;
                }
                if total_weight > 0.0 {
                    weighted_sum / total_weight
                } else {
                    0.0
                }
            }
        }
    }

    /// Score query terms against a specific document.
    /// Unlike `score_query()` (self-scoring), this measures how relevant
    /// a query is to an independent document in the corpus.
    pub fn score_against_document(&self, query_tokens: &[String], document: &[String]) -> f64 {
        if query_tokens.is_empty() || document.is_empty() {
            return 0.0;
        }

        let dl = document.len() as f64;

        // Count term frequency in document
        let mut doc_tf: HashMap<&String, usize> = HashMap::new();
        for term in document {
            *doc_tf.entry(term).or_insert(0) += 1;
        }

        let mut score = 0.0;
        // Deduplicate query terms for scoring
        let query_unique: HashSet<&String> = query_tokens.iter().collect();
        for term in query_unique {
            if let (Some(&idf), Some(&freq)) = (self.idf.get(term), doc_tf.get(term)) {
                let numerator = freq as f64 * (self.k1 + 1.0);
                let denominator = freq as f64 + self.k1 * (1.0 - self.b + self.b * (dl / self.avg_dl));
                score += idf * (numerator / denominator);
            }
        }

        score
    }

    /// Score query against all documents in the corpus and return the max score.
    /// This measures how well the query matches the most relevant document,
    /// providing a corpus-relative relevance score instead of self-scoring.
    pub fn score_against_corpus(&self, query_tokens: &[String]) -> f64 {
        self.documents
            .iter()
            .map(|doc| self.score_against_document(query_tokens, doc))
            .fold(0.0_f64, f64::max)
    }

    /// Extract high-IDF tokens from a query — the most discriminative terms.
    /// Returns tokens sorted by IDF descending, limited to `top_k`.
    /// This enables "reverse refinement": distilling a long prompt into
    /// the terms that carry the most information for BM25 scoring.
    pub fn extract_high_idf_tokens(&self, tokens: &[String], top_k: usize) -> Vec<String> {
        if tokens.is_empty() || top_k == 0 {
            return Vec::new();
        }

        let mut token_idf: Vec<(String, f64)> = tokens
            .iter()
            .filter_map(|t| {
                self.idf.get(t).map(|&idf| (t.clone(), idf))
            })
            .collect();

        // Deduplicate
        token_idf.sort_by(|a, b| a.0.cmp(&b.0));
        token_idf.dedup_by(|a, b| a.0 == b.0);

        // Sort by IDF descending
        token_idf.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        token_idf.into_iter().take(top_k).map(|(t, _)| t).collect()
    }
}

use std::collections::{HashMap, HashSet};

pub struct BM25Ranker {
    k1: f64,
    b: f64,
    avg_dl: f64,
    idf: HashMap<String, f64>,
    doc_count: usize,
}

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
            doc_count,
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
}

use crate::analyzers::bm25::BM25Ranker;
use crate::analyzers::depth::DepthConfig;
use crate::analyzers::stopwords::StopwordSet;
use crate::analyzers::tacit::tokenize;
use crate::tokenizer::KoreanTokenizer;
use std::sync::LazyLock;
use std::collections::HashSet;

static KOREAN_TOKENIZER: LazyLock<Option<KoreanTokenizer>> = LazyLock::new(|| {
    KoreanTokenizer::new().ok()
});

/// Sentence/clause delimiters for Korean text
const CLAUSE_DELIMITERS: &[&str] = &[
    ". ", ".\n", "! ", "!\n", "? ", "?\n",
    " 그리고 ", " 그런데 ", " 하지만 ", " 또한 ",
    " and ", " but ", " also ",
];

/// Single-char delimiters that work at boundaries
const CHAR_DELIMITERS: &[char] = &['\n'];

/// Result of decomposing a prompt into multiple sub-queries
#[derive(Debug, Clone)]
pub struct DecomposedQuery {
    /// The original full query tokens
    pub original: Vec<String>,
    /// Sub-queries generated from decomposition
    pub sub_queries: Vec<Vec<String>>,
}

/// Extract nouns from text using the Korean tokenizer
fn extract_nouns(text: &str, stopwords: &StopwordSet) -> Vec<String> {
    if let Some(ref tokenizer) = *KOREAN_TOKENIZER {
        let nouns = tokenizer.extract_nouns(text);
        if !nouns.is_empty() {
            return nouns
                .into_iter()
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty() && !stopwords.contains(s.as_str()))
                .collect();
        }
    }

    // Fallback: same as tokenize (no POS info available)
    tokenize(text, stopwords)
}

/// Split text into clauses/sentences
fn split_into_clauses(text: &str) -> Vec<String> {
    let mut segments = vec![text.to_string()];

    // Split by multi-char delimiters first
    for delim in CLAUSE_DELIMITERS {
        let mut new_segments = Vec::new();
        for seg in &segments {
            for part in seg.split(delim) {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    new_segments.push(trimmed.to_string());
                }
            }
        }
        segments = new_segments;
    }

    // Split by single-char delimiters
    for &delim in CHAR_DELIMITERS {
        let mut new_segments = Vec::new();
        for seg in &segments {
            for part in seg.split(delim) {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    new_segments.push(trimmed.to_string());
                }
            }
        }
        segments = new_segments;
    }

    segments
}

/// Decompose a text prompt into multiple sub-queries based on depth config.
///
/// Generates:
/// 1. Original full query (always)
/// 2. Sentence/clause-split sub-queries (if tokens > sentence_split_min_tokens)
/// 3. High-IDF token sub-query (reverse refinement)
/// 4. Noun-only sub-query (if noun_extraction enabled)
///
/// Total sub-queries are capped at `config.max_sub_queries`.
pub fn decompose_query(
    text: &str,
    config: &DepthConfig,
    bm25_ranker: &BM25Ranker,
    stopwords: &StopwordSet,
) -> DecomposedQuery {
    let original_tokens = tokenize(text, stopwords);

    if original_tokens.is_empty() {
        return DecomposedQuery {
            original: original_tokens,
            sub_queries: Vec::new(),
        };
    }

    let mut sub_queries: Vec<Vec<String>> = Vec::new();
    let remaining = config.max_sub_queries;

    // 1. Sentence/clause splitting (only if long enough)
    if original_tokens.len() >= config.sentence_split_min_tokens {
        let clauses = split_into_clauses(text);
        if clauses.len() > 1 {
            for clause in &clauses {
                if sub_queries.len() >= remaining {
                    break;
                }
                let clause_tokens = tokenize(clause, stopwords);
                if !clause_tokens.is_empty() && clause_tokens != original_tokens {
                    sub_queries.push(clause_tokens);
                }
            }
        }
    }

    // 2. High-IDF reverse refinement
    if sub_queries.len() < remaining && config.idf_top_k > 0 {
        let high_idf = bm25_ranker.extract_high_idf_tokens(&original_tokens, config.idf_top_k);
        if !high_idf.is_empty() && high_idf != original_tokens {
            sub_queries.push(high_idf);
        }
    }

    // 3. Noun extraction query
    if sub_queries.len() < remaining && config.noun_extraction {
        let nouns = extract_nouns(text, stopwords);
        if !nouns.is_empty() && nouns != original_tokens {
            // Deduplicate against existing sub-queries
            let nouns_set: HashSet<_> = nouns.iter().collect();
            let is_duplicate = sub_queries.iter().any(|sq| {
                let sq_set: HashSet<_> = sq.iter().collect();
                sq_set == nouns_set
            });
            if !is_duplicate {
                sub_queries.push(nouns);
            }
        }
    }

    // Cap at max_sub_queries
    sub_queries.truncate(config.max_sub_queries);

    DecomposedQuery {
        original: original_tokens,
        sub_queries,
    }
}

/// Convenience: build all query variants (original + sub-queries) as a flat list
/// suitable for `BM25Ranker::score_multi_query`.
impl DecomposedQuery {
    pub fn all_queries(&self) -> Vec<Vec<String>> {
        let mut all = vec![self.original.clone()];
        all.extend(self.sub_queries.clone());
        all
    }

    /// True if decomposition produced additional sub-queries beyond the original
    pub fn is_decomposed(&self) -> bool {
        !self.sub_queries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::depth::AnalysisDepth;

    fn test_stopwords() -> StopwordSet {
        StopwordSet::builtin()
    }

    fn make_ranker(docs: &[&str]) -> BM25Ranker {
        let sw = test_stopwords();
        let tokenized: Vec<Vec<String>> = docs.iter().map(|d| tokenize(d, &sw)).collect();
        BM25Ranker::new(&tokenized, 1.5, 0.75)
    }

    #[test]
    fn test_split_into_clauses_korean() {
        let text = "타입을 명시하고 그리고 에러 처리도 해줘";
        let clauses = split_into_clauses(text);
        assert!(clauses.len() >= 2, "Should split on '그리고': {:?}", clauses);
    }

    #[test]
    fn test_split_into_clauses_newline() {
        let text = "타입을 명시해줘\n에러도 처리해줘";
        let clauses = split_into_clauses(text);
        assert_eq!(clauses.len(), 2);
    }

    #[test]
    fn test_split_into_clauses_no_split() {
        let text = "타입을 명시해줘";
        let clauses = split_into_clauses(text);
        assert_eq!(clauses.len(), 1);
    }

    #[test]
    fn test_decompose_short_text_no_split() {
        let ranker = make_ranker(&["타입 명시", "에러 처리"]);
        let config = AnalysisDepth::Normal.resolve();
        let result = decompose_query("타입 명시", &config, &ranker, &test_stopwords());
        // Short text: no sentence splitting, might still get IDF/noun queries
        assert!(!result.original.is_empty());
    }

    #[test]
    fn test_decompose_long_text_splits() {
        let docs = &[
            "타입을 명시해줘",
            "에러를 처리해줘",
            "테스트를 작성해줘",
            "항상 타입을 명시하고 그리고 에러 핸들링도 꼭 해줘 그리고 테스트도 작성해줘",
        ];
        let ranker = make_ranker(docs);
        let config = AnalysisDepth::Wide.resolve();
        let result = decompose_query(
            "항상 타입을 명시하고 그리고 에러 핸들링도 꼭 해줘 그리고 테스트도 작성해줘",
            &config,
            &ranker,
            &test_stopwords(),
        );
        assert!(result.is_decomposed(), "Long text should produce sub-queries");
        assert!(result.all_queries().len() > 1);
    }

    #[test]
    fn test_decompose_narrow_fewer_queries() {
        let docs = &[
            "타입을 명시해줘",
            "에러를 처리해줘",
            "항상 타입을 명시하고 그리고 에러도 처리하고 그리고 테스트도 작성해줘",
        ];
        let ranker = make_ranker(docs);

        let sw = test_stopwords();
        let narrow = decompose_query(
            "항상 타입을 명시하고 그리고 에러도 처리하고 그리고 테스트도 작성해줘",
            &AnalysisDepth::Narrow.resolve(),
            &ranker,
            &sw,
        );
        let wide = decompose_query(
            "항상 타입을 명시하고 그리고 에러도 처리하고 그리고 테스트도 작성해줘",
            &AnalysisDepth::Wide.resolve(),
            &ranker,
            &sw,
        );

        assert!(
            narrow.all_queries().len() <= wide.all_queries().len(),
            "Narrow should produce fewer or equal queries than wide"
        );
    }

    #[test]
    fn test_decompose_empty() {
        let ranker = make_ranker(&["hello"]);
        let config = AnalysisDepth::Normal.resolve();
        let result = decompose_query("", &config, &ranker, &test_stopwords());
        assert!(result.original.is_empty());
        assert!(result.sub_queries.is_empty());
    }

    #[test]
    fn test_all_queries_includes_original() {
        let ranker = make_ranker(&["타입 명시"]);
        let config = AnalysisDepth::Normal.resolve();
        let result = decompose_query("타입을 명시해줘", &config, &ranker, &test_stopwords());
        let all = result.all_queries();
        assert!(!all.is_empty());
        assert_eq!(all[0], result.original, "First query should be original");
    }
}

use crate::analyzers::bm25::MultiQueryStrategy;

/// Analysis depth preset — controls how aggressively queries are decomposed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalysisDepth {
    /// Conservative: fewer sub-queries, faster, high-precision
    Narrow,
    /// Balanced default
    Normal,
    /// Aggressive: maximum decomposition, more patterns discovered
    Wide,
}

/// Resolved parameters from an AnalysisDepth preset
#[derive(Debug, Clone)]
pub struct DepthConfig {
    /// Minimum token count before sentence splitting kicks in
    pub sentence_split_min_tokens: usize,
    /// Number of high-IDF tokens to extract for reverse refinement
    pub idf_top_k: usize,
    /// Maximum number of sub-queries to generate
    pub max_sub_queries: usize,
    /// Whether to generate a noun-only sub-query
    pub noun_extraction: bool,
    /// Similarity threshold for clustering
    pub similarity_threshold: f64,
    /// Multi-query score combination strategy
    pub multi_query_strategy: MultiQueryStrategy,
}

impl AnalysisDepth {
    pub fn resolve(&self) -> DepthConfig {
        match self {
            AnalysisDepth::Narrow => DepthConfig {
                sentence_split_min_tokens: 12,
                idf_top_k: 3,
                max_sub_queries: 2,
                noun_extraction: false,
                similarity_threshold: 0.8,
                multi_query_strategy: MultiQueryStrategy::Max,
            },
            AnalysisDepth::Normal => DepthConfig {
                sentence_split_min_tokens: 8,
                idf_top_k: 5,
                max_sub_queries: 4,
                noun_extraction: true,
                similarity_threshold: 0.7,
                multi_query_strategy: MultiQueryStrategy::WeightedAvg,
            },
            AnalysisDepth::Wide => DepthConfig {
                sentence_split_min_tokens: 5,
                idf_top_k: 8,
                max_sub_queries: 8,
                noun_extraction: true,
                similarity_threshold: 0.5,
                multi_query_strategy: MultiQueryStrategy::Avg,
            },
        }
    }
}

impl std::fmt::Display for AnalysisDepth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisDepth::Narrow => write!(f, "narrow"),
            AnalysisDepth::Normal => write!(f, "normal"),
            AnalysisDepth::Wide => write!(f, "wide"),
        }
    }
}

impl std::str::FromStr for AnalysisDepth {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "narrow" => Ok(AnalysisDepth::Narrow),
            "normal" => Ok(AnalysisDepth::Normal),
            "wide" => Ok(AnalysisDepth::Wide),
            _ => Err(format!(
                "invalid depth '{}': expected narrow, normal, or wide",
                s
            )),
        }
    }
}

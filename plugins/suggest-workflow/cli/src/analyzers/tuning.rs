use serde::{Deserialize, Serialize};
use std::path::Path;

/// All tunable numeric parameters extracted from hardcoded magic numbers.
/// Load from a JSON file via `--tuning <path>`, or use `--tuning-defaults`
/// to print the default template.  Individual CLI flags override file values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningConfig {
    // -- BM25 hyperparameters --
    /// Term-frequency saturation (higher → raw TF matters more)
    pub bm25_k1: f64,
    /// Document-length normalization (0 = ignore length, 1 = full normalization)
    pub bm25_b: f64,

    // -- Workflow sequence extraction --
    /// Time gap (minutes) that splits a session into separate work-units
    pub time_window_minutes: u64,
    /// Minimum tool-sequence length to extract
    pub min_seq_length: usize,
    /// Maximum tool-sequence length to extract
    pub max_seq_length: usize,

    // -- Temporal decay --
    /// Half-life in days for recency weighting
    pub decay_half_life_days: f64,

    // -- Confidence scoring weights (no-decay mode, should sum to 1.0) --
    /// Weight for frequency component
    pub weight_frequency: f64,
    /// Weight for consistency component
    pub weight_consistency: f64,
    /// Weight for BM25 relevance component
    pub weight_bm25: f64,

    // -- Confidence scoring weights (decay mode, should sum to 1.0) --
    /// Weight for frequency component (decay mode)
    pub decay_weight_frequency: f64,
    /// Weight for consistency component (decay mode)
    pub decay_weight_consistency: f64,
    /// Weight for BM25 relevance component (decay mode)
    pub decay_weight_bm25: f64,
    /// Weight for recency component (decay mode)
    pub decay_weight_recency: f64,

    // -- BM25 normalization --
    /// Sigmoid divisor for mapping raw BM25 scores to [0,1]
    pub bm25_sigmoid_k: f64,
    /// Decay factor for weighted-average multi-query strategy (weight = decay^i)
    pub multi_query_decay: f64,

    // -- Clustering --
    /// Maximum number of unique normalized prompts to cluster
    pub max_clusters: usize,

    // -- Repetition / outlier detection --
    /// Z-score threshold for file-edit outlier detection (also sets BH FDR alpha)
    pub z_score_threshold: f64,
    /// Maximum subsequence length for loop detection
    pub loop_max_seq_length: usize,
    /// Minimum consecutive repetitions to report a loop
    pub loop_min_repeats: usize,

    // -- Trend analysis --
    /// Minimum R² to report a tool trend as meaningful (0.0–1.0)
    pub min_trend_r_squared: f64,
}

impl Default for TuningConfig {
    fn default() -> Self {
        Self {
            bm25_k1: 1.5,
            bm25_b: 0.75,

            time_window_minutes: 5,
            min_seq_length: 2,
            max_seq_length: 5,

            decay_half_life_days: 14.0,

            weight_frequency: 0.40,
            weight_consistency: 0.20,
            weight_bm25: 0.40,

            decay_weight_frequency: 0.30,
            decay_weight_consistency: 0.15,
            decay_weight_bm25: 0.35,
            decay_weight_recency: 0.20,

            bm25_sigmoid_k: 5.0,
            multi_query_decay: 0.6,

            max_clusters: 500,

            z_score_threshold: 2.0,
            loop_max_seq_length: 3,
            loop_min_repeats: 2,

            min_trend_r_squared: 0.3,
        }
    }
}

impl TuningConfig {
    /// Load from a JSON file, falling back to defaults for any missing field.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read tuning file '{}': {}", path.display(), e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("invalid tuning JSON '{}': {}", path.display(), e))
    }

    /// Print default config as pretty JSON (for --tuning-defaults).
    pub fn print_defaults() {
        let json = serde_json::to_string_pretty(&Self::default()).expect("serialize defaults");
        println!("{}", json);
    }

    /// Apply individual CLI overrides on top of the current config.
    pub fn apply_overrides(
        &mut self,
        bm25_k1: Option<f64>,
        bm25_b: Option<f64>,
        time_window: Option<u64>,
        decay_half_life: Option<f64>,
        z_threshold: Option<f64>,
    ) {
        if let Some(v) = bm25_k1 {
            self.bm25_k1 = v;
        }
        if let Some(v) = bm25_b {
            self.bm25_b = v;
        }
        if let Some(v) = time_window {
            self.time_window_minutes = v;
        }
        if let Some(v) = decay_half_life {
            self.decay_half_life_days = v;
        }
        if let Some(v) = z_threshold {
            self.z_score_threshold = v;
        }
    }
}

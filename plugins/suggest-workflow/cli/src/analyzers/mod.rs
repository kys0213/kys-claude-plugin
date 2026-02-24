pub mod bm25;
pub mod depth;
pub mod prompt;
pub mod query_decomposer;
pub mod stopwords;
pub mod suffix_miner;
pub mod tacit;
pub mod tool_classifier;
pub mod tuning;
pub mod workflow;

// Statistical analyzers (rule-free)
pub mod dependency_graph;
pub mod file_analysis;
pub mod repetition;
pub mod session_link;
pub mod transition;
pub mod trend;

pub use depth::{AnalysisDepth, DepthConfig};
pub use prompt::analyze_prompts;
pub use stopwords::StopwordSet;
pub use tacit::analyze_tacit_knowledge;
pub use tuning::TuningConfig;
pub use workflow::analyze_workflows;

pub use dependency_graph::build_dependency_graph;
pub use file_analysis::analyze_files;
pub use repetition::analyze_repetition;
pub use session_link::link_sessions;
pub use transition::build_transition_matrix;
pub use trend::analyze_trends;

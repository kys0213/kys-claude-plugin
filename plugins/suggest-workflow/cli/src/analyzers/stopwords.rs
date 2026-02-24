use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

/// Built-in default noise words (fallback when no config file exists)
const BUILTIN_NOISE_WORDS: &[&str] = &[
    "응",
    "네",
    "좋아",
    "그래",
    "알겠어",
    "해줘",
    "해",
    "하자",
    "고마워",
    "감사",
    "ok",
    "yes",
    "y",
    "sure",
    "thanks",
    "ㅇ",
    "ㅇㅇ",
    "넵",
];

/// Config file format for project-specific stopwords.
/// Extra fields in the JSON file are ignored via deny_unknown_fields=false (serde default).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopwordsConfig {
    words: Vec<String>,
}

/// Resolved stopwords set — merged from built-in defaults, config file, and CLI args
#[derive(Debug, Clone)]
pub struct StopwordSet {
    words: HashSet<String>,
}

impl StopwordSet {
    /// Build from defaults + config file + CLI extra words
    pub fn load(extra_words: &[String]) -> Self {
        let mut words: HashSet<String> =
            BUILTIN_NOISE_WORDS.iter().map(|s| s.to_string()).collect();

        // Load from config file
        if let Some(config) = load_config_file() {
            for word in config.words {
                words.insert(word);
            }
        }

        // Add CLI extra words
        for word in extra_words {
            words.insert(word.clone());
        }

        Self { words }
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(word)
    }
}

/// Default config file path: ~/.claude/suggest-workflow/stopwords.json
fn config_file_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join(".claude")
            .join("suggest-workflow")
            .join("stopwords.json"),
    )
}

/// Load stopwords config from file, returns None if file doesn't exist or is invalid
fn load_config_file() -> Option<StopwordsConfig> {
    let path = config_file_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

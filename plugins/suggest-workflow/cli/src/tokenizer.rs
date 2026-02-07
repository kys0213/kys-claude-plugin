//! Korean tokenization module
//!
//! Uses lindera for morphological analysis when available,
//! falls back to rule-based tokenization otherwise.

use anyhow::Result;

#[cfg(feature = "lindera-korean")]
use lindera::tokenizer::Tokenizer;
#[cfg(feature = "lindera-korean")]
use lindera::segmenter::Segmenter;
#[cfg(feature = "lindera-korean")]
use lindera::dictionary::{DictionaryKind, load_embedded_dictionary};
#[cfg(feature = "lindera-korean")]
use lindera::mode::Mode;

/// Korean tokenizer with optional lindera support
pub struct KoreanTokenizer {
    #[cfg(feature = "lindera-korean")]
    tokenizer: Tokenizer,
}

impl KoreanTokenizer {
    /// Create a new Korean tokenizer
    #[cfg(feature = "lindera-korean")]
    pub fn new() -> Result<Self> {
        let dictionary = load_embedded_dictionary(DictionaryKind::KoDic)?;
        let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
        let tokenizer = Tokenizer::new(segmenter);
        Ok(Self { tokenizer })
    }

    #[cfg(not(feature = "lindera-korean"))]
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Tokenize text into words
    #[cfg(feature = "lindera-korean")]
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        self.tokenizer
            .tokenize(text)
            .unwrap_or_default()
            .iter()
            .map(|t| t.surface.to_string())
            .collect()
    }

    #[cfg(not(feature = "lindera-korean"))]
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        rule_based_tokenize(text)
    }

    /// Extract only nouns (NNG, NNP, etc.)
    /// Falls back to all tokens if POS details are unavailable
    #[cfg(feature = "lindera-korean")]
    pub fn extract_nouns(&self, text: &str) -> Vec<String> {
        let tokens = self.tokenizer.tokenize(text).unwrap_or_default();

        // Try POS-based filtering first
        let nouns: Vec<String> = tokens
            .iter()
            .filter(|t| {
                t.details
                    .as_ref()
                    .and_then(|d| d.first())
                    .map(|pos| pos.starts_with("NN"))
                    .unwrap_or(false)
            })
            .map(|t| t.surface.to_string())
            .collect();

        // If no details available, return all surface tokens
        if nouns.is_empty() {
            tokens.iter().map(|t| t.surface.to_string()).collect()
        } else {
            nouns
        }
    }

    #[cfg(not(feature = "lindera-korean"))]
    pub fn extract_nouns(&self, text: &str) -> Vec<String> {
        rule_based_tokenize(text)
    }
}

impl Default for KoreanTokenizer {
    fn default() -> Self {
        Self::new().expect("Failed to create tokenizer")
    }
}

/// Rule-based Korean tokenization (fallback)
/// Splits on whitespace and removes common Korean particles
#[cfg(not(feature = "lindera-korean"))]
fn rule_based_tokenize(text: &str) -> Vec<String> {
    const PARTICLES: &[&str] = &[
        "은", "는", "이", "가", "을", "를", "에", "의", "로", "으로",
        "와", "과", "도", "만", "까지", "부터", "에서", "한테", "께",
        "처럼", "같이", "보다", "라고", "고", "면", "서", "니까",
        "지만", "어서", "아서", "려고", "게", "도록",
    ];

    let mut tokens = Vec::new();

    for word in text.split_whitespace() {
        if word.is_empty() {
            continue;
        }

        let mut processed = word.to_string();
        for particle in PARTICLES {
            if let Some(stripped) = processed.strip_suffix(particle) {
                if !stripped.is_empty() {
                    processed = stripped.to_string();
                    break;
                }
            }
        }

        if !processed.is_empty() {
            tokens.push(processed);
        }
    }

    if tokens.is_empty() {
        text.split_whitespace()
            .map(|s| s.to_string())
            .collect()
    } else {
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_creation() {
        let tokenizer = KoreanTokenizer::new();
        assert!(tokenizer.is_ok());
    }

    #[test]
    fn test_basic_tokenization() {
        let tokenizer = KoreanTokenizer::new().unwrap();
        let tokens = tokenizer.tokenize("항상 타입을 명시해줘");
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_mixed_text() {
        let tokenizer = KoreanTokenizer::new().unwrap();
        let tokens = tokenizer.tokenize("conventional commit으로 커밋해줘");
        assert!(!tokens.is_empty());
    }
}

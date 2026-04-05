//! 64-bit weighted simhash for locality-sensitive hashing.
//!
//! Similar texts produce similar hashes. The hamming distance between
//! two simhashes approximates the dissimilarity of the original texts.

/// Compute a 64-bit weighted simhash from pre-weighted tokens.
pub fn weighted_simhash(tokens: &[(String, u32)]) -> u64 {
    let mut v = [0i32; 64];
    for (token, weight) in tokens {
        let hash = fnv1a_64(token);
        let w = *weight as i32;
        for (i, slot) in v.iter_mut().enumerate() {
            if (hash >> i) & 1 == 1 {
                *slot += w;
            } else {
                *slot -= w;
            }
        }
    }
    let mut result: u64 = 0;
    for (i, val) in v.iter().enumerate() {
        if *val > 0 {
            result |= 1 << i;
        }
    }
    result
}

/// Hamming distance between two 64-bit simhashes.
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Format a simhash as a hex string with 0x prefix.
pub fn format_simhash(hash: u64) -> String {
    format!("0x{hash:016X}")
}

/// Parse a hex simhash string (with or without 0x prefix).
pub fn parse_simhash(s: &str) -> Option<u64> {
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(hex, 16).ok()
}

/// Tokenize text into weighted (token, weight) pairs.
///
/// Strategy:
/// - Filter stopwords and markdown syntax
/// - Generate 2-shingles (word pairs) for order sensitivity
/// - Apply weight ×3 to implementation keywords, file paths, function names
/// - Apply weight ×1 to everything else
pub fn tokenize_weighted(text: &str) -> Vec<(String, u32)> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '/' && c != '.'))
        .filter(|w| !w.is_empty() && !is_stopword(w) && !is_markdown_syntax(w))
        .collect();

    let mut tokens = Vec::new();

    // 2-shingles
    for window in words.windows(2) {
        let shingle = format!("{} {}", window[0].to_lowercase(), window[1].to_lowercase());
        let weight = classify_weight(&shingle);
        tokens.push((shingle, weight));
    }

    // Single words
    for word in &words {
        let w = word.to_lowercase();
        let weight = classify_weight(&w);
        tokens.push((w, weight));
    }

    tokens
}

/// FNV-1a 64-bit hash — fast, no external deps.
fn fnv1a_64(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn is_stopword(w: &str) -> bool {
    matches!(
        w.to_lowercase().as_str(),
        "the" | "a" | "an" | "is" | "are" | "was" | "were" | "be" | "been"
            | "being" | "have" | "has" | "had" | "do" | "does" | "did"
            | "will" | "would" | "could" | "should" | "may" | "might"
            | "shall" | "can" | "to" | "of" | "in" | "for" | "on" | "with"
            | "at" | "by" | "from" | "as" | "into" | "through" | "and"
            | "or" | "but" | "not" | "no" | "if" | "then" | "else"
            | "this" | "that" | "it" | "its"
            // Korean stopwords common in gap reports
            | "및" | "등" | "위" | "것" | "수" | "더" | "또" | "각"
    )
}

fn is_markdown_syntax(w: &str) -> bool {
    w.starts_with('#')
        || w == "-"
        || w == "*"
        || w == ">"
        || w == "|"
        || w == "```"
        || w == "---"
        || w == "✅"
        || w == "⚠️"
        || w == "❌"
}

fn classify_weight(token: &str) -> u32 {
    if is_high_weight(token) {
        3
    } else {
        1
    }
}

fn is_high_weight(token: &str) -> bool {
    // File path patterns
    if token.contains('/')
        && (token.ends_with(".rs")
            || token.ends_with(".ts")
            || token.ends_with(".go")
            || token.ends_with(".py"))
    {
        return true;
    }
    // Function name patterns (snake_case or contains ::)
    if token.contains('_')
        && token
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
        && token.len() > 3
    {
        return true;
    }
    if token.contains("::") {
        return true;
    }
    // Architecture keywords
    matches!(
        token,
        "middleware"
            | "handler"
            | "interceptor"
            | "adapter"
            | "controller"
            | "service"
            | "repository"
            | "trait"
            | "interface"
            | "impl"
            | "struct"
            | "enum"
            | "module"
            | "factory"
            | "strategy"
            | "provider"
            | "resolver"
            | "validator"
            | "parser"
            | "builder"
            | "router"
            | "dispatcher"
            | "listener"
            | "observer"
            | "proxy"
            | "decorator"
            | "wrapper"
            | "client"
            | "server"
            | "endpoint"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_produces_same_hash() {
        let text = "JWT token refresh middleware 추가 필요";
        let a = weighted_simhash(&tokenize_weighted(text));
        let b = weighted_simhash(&tokenize_weighted(text));
        assert_eq!(a, b);
    }

    #[test]
    fn similar_text_produces_small_distance() {
        let a = weighted_simhash(&tokenize_weighted("middleware 레이어에 rate limiter 추가"));
        let b = weighted_simhash(&tokenize_weighted(
            "middleware layer rate limiter 추가 필요",
        ));
        let dist = hamming_distance(a, b);
        assert!(dist <= 10, "expected small distance, got {dist}");
    }

    #[test]
    fn different_approach_produces_large_distance() {
        let a = weighted_simhash(&tokenize_weighted(
            "middleware 레이어에 rate limiter handler 추가",
        ));
        let b = weighted_simhash(&tokenize_weighted(
            "interceptor 패턴으로 request throttling validator 구현",
        ));
        let dist = hamming_distance(a, b);
        assert!(dist >= 8, "expected large distance, got {dist}");
    }

    #[test]
    fn weight_amplifies_key_terms() {
        // Two texts differing only in a high-weight term should have larger
        // distance than two texts differing in a low-weight term.
        let base = "add rate limiting logic";
        let high_diff_a = "add middleware rate limiting logic";
        let high_diff_b = "add interceptor rate limiting logic";
        let low_diff_a = "add some rate limiting logic";
        let low_diff_b = "add new rate limiting logic";

        let high_dist = hamming_distance(
            weighted_simhash(&tokenize_weighted(high_diff_a)),
            weighted_simhash(&tokenize_weighted(high_diff_b)),
        );
        let low_dist = hamming_distance(
            weighted_simhash(&tokenize_weighted(low_diff_a)),
            weighted_simhash(&tokenize_weighted(low_diff_b)),
        );

        // High-weight term changes should produce >= distance than low-weight
        assert!(
            high_dist >= low_dist,
            "high_dist={high_dist} should be >= low_dist={low_dist} (base: {base})"
        );
    }

    #[test]
    fn empty_text_produces_zero_hash() {
        let hash = weighted_simhash(&tokenize_weighted(""));
        assert_eq!(hash, 0);
    }

    #[test]
    fn format_and_parse_roundtrip() {
        let original: u64 = 0xA3F2B81C4D5E6F1B;
        let formatted = format_simhash(original);
        assert_eq!(formatted, "0xA3F2B81C4D5E6F1B");
        assert_eq!(parse_simhash(&formatted), Some(original));
    }

    #[test]
    fn parse_without_prefix() {
        assert_eq!(parse_simhash("A3F2B81C4D5E6F1B"), Some(0xA3F2B81C4D5E6F1B));
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(parse_simhash("not_hex"), None);
    }

    #[test]
    fn hamming_distance_identical() {
        assert_eq!(hamming_distance(0xFFFF, 0xFFFF), 0);
    }

    #[test]
    fn hamming_distance_one_bit() {
        assert_eq!(hamming_distance(0b1000, 0b0000), 1);
    }

    #[test]
    fn file_path_gets_high_weight() {
        let tokens = tokenize_weighted("src/auth/handler.rs implements login");
        // Single-word token (not shingle) should get high weight
        let path_token = tokens.iter().find(|(t, _)| *t == "src/auth/handler.rs");
        assert!(path_token.is_some());
        assert_eq!(path_token.unwrap().1, 3);
    }
}

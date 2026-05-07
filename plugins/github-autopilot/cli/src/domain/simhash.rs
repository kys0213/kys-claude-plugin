//! Domain-level simhash + Jaccard primitives for ledger-based stagnation
//! detection.
//!
//! These are the deterministic transforms specified in
//! `plans/ledger-stagnation-redesign.md` §3.3:
//!
//! - [`derive_simhash`] computes a 64-bit weighted simhash over token
//!   shingles (split on whitespace + punctuation). Each token is hashed with
//!   SHA-256 truncated to 64 bits, contributing a per-bit weighted vote
//!   towards the final signature.
//! - [`hamming_distance`] returns the bit difference between two simhashes.
//! - [`jaccard_similarity`] returns the path-set Jaccard ratio
//!   `|A ∩ B| / |A ∪ B|`.
//!
//! These three primitives are intentionally pure functions — same input
//! always produces the same output (per CLAUDE.md "책임 경계"). The
//! existing `cmd::simhash` module remains the implementation used by
//! GitHub-issue-fingerprint flows; this domain module exists so the
//! ledger-based stagnation pipeline (CLI primitives + scenario tests) can
//! depend on a stable, domain-owned API instead of reaching into the
//! command layer.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

/// Compute a 64-bit weighted simhash for `text` using token shingles.
///
/// Algorithm (per spec §3.3):
/// 1. Split `text` on whitespace + punctuation → tokens.
/// 2. Build 1-gram + 2-gram (shingle) tokens; each contributes weight 1.
/// 3. Hash each token with SHA-256, truncate to the first 64 bits.
/// 4. For every bit position 0..64, accumulate `+weight` if the bit is set
///    in the token hash, else `-weight`.
/// 5. Final signature: bit `i` = 1 if accumulator > 0 else 0.
///
/// Empty input yields `0` (no votes → all bits remain 0).
pub fn derive_simhash(text: &str) -> u64 {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return 0;
    }

    let mut votes = [0i32; 64];
    for token in &tokens {
        let h = sha256_first_u64(token);
        for (i, slot) in votes.iter_mut().enumerate() {
            if (h >> i) & 1 == 1 {
                *slot += 1;
            } else {
                *slot -= 1;
            }
        }
    }

    let mut signature: u64 = 0;
    for (i, v) in votes.iter().enumerate() {
        if *v > 0 {
            signature |= 1u64 << i;
        }
    }
    signature
}

/// XOR + popcount: bit-difference between two 64-bit simhash signatures.
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Jaccard similarity for two path sets: `|A ∩ B| / |A ∪ B|`.
///
/// - Returns `1.0` when both inputs are empty (vacuous match — both
///   describe the same "no specific area" state). Callers comparing against
///   `--min-jaccard` thresholds get a deterministic answer for the
///   degenerate case.
/// - Duplicates within either input are de-duplicated via [`BTreeSet`].
pub fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let sa: BTreeSet<&str> = a.iter().map(String::as_str).collect();
    let sb: BTreeSet<&str> = b.iter().map(String::as_str).collect();
    let intersection = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        return 0.0;
    }
    intersection / union
}

fn tokenize(text: &str) -> Vec<String> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '/' && c != '.')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();

    if words.is_empty() {
        return Vec::new();
    }

    let mut tokens: Vec<String> = Vec::with_capacity(words.len() * 2);
    // 1-grams
    tokens.extend(words.iter().cloned());
    // 2-grams (shingles for order sensitivity)
    for w in words.windows(2) {
        tokens.push(format!("{} {}", w[0], w[1]));
    }
    tokens
}

fn sha256_first_u64(s: &str) -> u64 {
    let digest = Sha256::digest(s.as_bytes());
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_produces_same_hash() {
        let a = derive_simhash("middleware refactor for rate limiting");
        let b = derive_simhash("middleware refactor for rate limiting");
        assert_eq!(a, b);
    }

    #[test]
    fn similar_text_produces_small_distance() {
        let a = derive_simhash("middleware refactor rate limiter handler");
        let b = derive_simhash("middleware refactor rate limiter handler tweak");
        let dist = hamming_distance(a, b);
        assert!(dist <= 12, "expected small distance, got {dist}");
    }

    #[test]
    fn different_text_produces_large_distance() {
        let a = derive_simhash("add middleware rate limiting handler");
        let b = derive_simhash("rewrite database transaction isolation strategy");
        let dist = hamming_distance(a, b);
        assert!(dist >= 16, "expected large distance, got {dist}");
    }

    #[test]
    fn empty_input_returns_zero() {
        assert_eq!(derive_simhash(""), 0);
        assert_eq!(derive_simhash("   \t \n"), 0);
    }

    #[test]
    fn hamming_distance_basics() {
        assert_eq!(hamming_distance(0, 0), 0);
        assert_eq!(hamming_distance(0xFFFF_FFFF_FFFF_FFFF, 0), 64);
        assert_eq!(hamming_distance(0b1010, 0b0101), 4);
    }

    #[test]
    fn jaccard_full_overlap() {
        let a = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let b = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let a = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        let b = vec!["src/b.rs".to_string(), "src/c.rs".to_string()];
        // intersection = {b.rs}, union = {a.rs, b.rs, c.rs}, ratio = 1/3.
        let ratio = jaccard_similarity(&a, &b);
        assert!((ratio - 1.0 / 3.0).abs() < 1e-9, "got {ratio}");
    }

    #[test]
    fn jaccard_disjoint() {
        let a = vec!["x".to_string()];
        let b = vec!["y".to_string()];
        assert_eq!(jaccard_similarity(&a, &b), 0.0);
    }

    #[test]
    fn jaccard_both_empty_is_one() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![];
        assert_eq!(jaccard_similarity(&a, &b), 1.0);
    }

    #[test]
    fn jaccard_one_empty_is_zero() {
        let a = vec!["x".to_string()];
        let b: Vec<String> = vec![];
        assert_eq!(jaccard_similarity(&a, &b), 0.0);
    }

    #[test]
    fn jaccard_dedupes_within_inputs() {
        let a = vec!["x".to_string(), "x".to_string(), "y".to_string()];
        let b = vec!["y".to_string(), "y".to_string(), "z".to_string()];
        // intersection = {y}, union = {x, y, z}, ratio = 1/3.
        let ratio = jaccard_similarity(&a, &b);
        assert!((ratio - 1.0 / 3.0).abs() < 1e-9, "got {ratio}");
    }
}

//! How many of the nonces the pane leaves in the store every session shares
//! are kept.

const MAX_NONCES: usize = 32;

/// The keys to delete of `keys`, given oldest first: all but the newest
/// `MAX_NONCES`.
pub fn past_cap(keys: &[String]) -> &[String] {
    &keys[..keys.len().saturating_sub(MAX_NONCES)]
}

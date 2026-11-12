//! How many of the nonces the pane leaves in the store every session shares
//! are kept.

const NONCE_PREFIX: &str = "kanban:nonce:";
const MAX_NONCES: usize = 32;

/// The nonce keys to delete of the store's `keys`, given oldest first: all
/// but the newest `MAX_NONCES`; other keys are never named.
pub fn past_cap(keys: &[String]) -> Vec<&str> {
    let nonces: Vec<&str> = keys
        .iter()
        .map(String::as_str)
        .filter(|k| k.starts_with(NONCE_PREFIX))
        .collect();
    let excess = nonces.len().saturating_sub(MAX_NONCES);
    nonces[..excess].to_vec()
}

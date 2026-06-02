pub const READY: &str = "ready";
pub const WIP: &str = "wip";
pub const CI_FAILURE: &str = "ci-failure";
pub const AUTO: &str = "auto";

pub fn with_prefix(prefix: &str, suffix: &str) -> String {
    format!("{prefix}{suffix}")
}

/// Check if any label starts with the given prefix.
pub fn has_prefixed_label(labels: &[serde_json::Value], prefix: &str) -> bool {
    labels.iter().any(|l| {
        l["name"]
            .as_str()
            .is_some_and(|name| name.starts_with(prefix))
    })
}

/// Check if a specific prefixed label exists.
pub fn has_label(labels: &[serde_json::Value], prefix: &str, suffix: &str) -> bool {
    let target = with_prefix(prefix, suffix);
    labels
        .iter()
        .any(|l| l["name"].as_str().is_some_and(|name| name == target))
}

/// Check if a specific label (exact match) exists.
pub fn has_exact_label(labels: &[serde_json::Value], label: &str) -> bool {
    labels
        .iter()
        .any(|l| l["name"].as_str().is_some_and(|name| name == label))
}

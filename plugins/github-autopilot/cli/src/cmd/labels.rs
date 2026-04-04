pub const READY: &str = "ready";
pub const WIP: &str = "wip";
pub const CI_FAILURE: &str = "ci-failure";
pub const AUTO: &str = "auto";

pub fn with_prefix(prefix: &str, suffix: &str) -> String {
    format!("{prefix}{suffix}")
}

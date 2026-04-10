use crate::cmd::watch::WatchEvent;
use crate::git::GitOps;

/// Detect new commits on a remote branch by comparing SHA after fetch.
///
/// Returns `None` if unchanged or if fetch/resolve fails (transient).
pub fn detect_push(
    git: &dyn GitOps,
    remote: &str,
    branch: &str,
    last_sha: &str,
) -> Option<WatchEvent> {
    if git.fetch_remote(remote, branch).is_err() {
        return None;
    }

    let refname = format!("{remote}/{branch}");
    let current = match git.rev_parse_ref(&refname) {
        Ok(sha) => sha,
        Err(_) => return None,
    };

    if current == last_sha {
        return None;
    }

    let count = git.rev_list_count(last_sha, &current).unwrap_or(0);

    Some(WatchEvent::MainUpdated {
        before: last_sha.to_string(),
        after: current,
        count,
    })
}

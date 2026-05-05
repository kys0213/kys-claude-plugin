use anyhow::Result;
use std::sync::Arc;

use crate::gh::{self, GhOps};

/// Check the autopilot pipeline state.
///
/// Exit codes:
/// - `0` idle: `ready + wip + prs == 0`
/// - `3` at-capacity: `--max-parallel <N>` supplied AND `wip >= N` (skip new
///   dispatch — capacity is full, in-flight cycle still running)
/// - `1` active: pipeline has work in flight but capacity remains, OR
///   `--max-parallel` not supplied (caller hasn't opted into capacity check)
///
/// `max_parallel` is `None` when the caller did not pass `--max-parallel`,
/// in which case the at-capacity branch is disabled and behavior matches
/// the original `idle/active` contract.
pub fn idle(client: Arc<dyn GhOps>, label_prefix: &str, max_parallel: Option<u64>) -> Result<i32> {
    use super::labels;
    let ready_label = labels::with_prefix(label_prefix, labels::READY);
    let wip_label = labels::with_prefix(label_prefix, labels::WIP);
    let auto_label = labels::with_prefix(label_prefix, labels::AUTO);

    let results = gh::run_parallel(vec![
        Box::new({
            let gh = Arc::clone(&client);
            let label = ready_label.clone();
            move || {
                gh::count_items(
                    gh.as_ref(),
                    &[
                        "issue", "list", "--label", &label, "--state", "open", "--json", "number",
                    ],
                )
            }
        }),
        Box::new({
            let gh = Arc::clone(&client);
            let label = wip_label.clone();
            move || {
                gh::count_items(
                    gh.as_ref(),
                    &[
                        "issue", "list", "--label", &label, "--state", "open", "--json", "number",
                    ],
                )
            }
        }),
        Box::new({
            let gh = Arc::clone(&client);
            let label = auto_label.clone();
            move || {
                gh::count_items(
                    gh.as_ref(),
                    &[
                        "pr", "list", "--label", &label, "--state", "open", "--json", "number",
                    ],
                )
            }
        }),
    ]);

    let ready = *results[0].as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
    let wip = *results[1].as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
    let prs = *results[2].as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;

    let is_idle = ready + wip + prs == 0;
    // at-capacity is only meaningful when caller specified the cap. Without
    // --max-parallel we don't know what "capacity" means, so we fall back to
    // the original idle/active contract.
    let at_capacity = match max_parallel {
        Some(n) if !is_idle => wip >= n,
        _ => false,
    };

    let mut out = serde_json::json!({
        "idle": is_idle,
        "ready": ready,
        "wip": wip,
        "prs": prs,
    });
    if let Some(n) = max_parallel {
        out["max_parallel"] = serde_json::json!(n);
        out["at_capacity"] = serde_json::json!(at_capacity);
    }
    println!("{out}");

    if is_idle {
        Ok(0)
    } else if at_capacity {
        Ok(3)
    } else {
        Ok(1)
    }
}

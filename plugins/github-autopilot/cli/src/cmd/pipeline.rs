use anyhow::Result;

use crate::gh;

/// Check if the autopilot pipeline is idle (no active issues or PRs).
/// Returns exit code: 0 = idle, 1 = active.
pub fn idle(label_prefix: &str) -> Result<i32> {
    use super::labels;
    let ready_label = labels::with_prefix(label_prefix, labels::READY);
    let wip_label = labels::with_prefix(label_prefix, labels::WIP);
    let auto_label = labels::with_prefix(label_prefix, labels::AUTO);

    let results = gh::run_parallel(vec![
        Box::new({
            let label = ready_label.clone();
            move || {
                count_items(&[
                    "issue", "list", "--label", &label, "--state", "open", "--json", "number",
                ])
            }
        }),
        Box::new({
            let label = wip_label.clone();
            move || {
                count_items(&[
                    "issue", "list", "--label", &label, "--state", "open", "--json", "number",
                ])
            }
        }),
        Box::new({
            let label = auto_label.clone();
            move || {
                count_items(&[
                    "pr", "list", "--label", &label, "--state", "open", "--json", "number",
                ])
            }
        }),
    ]);

    let ready = results[0].as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
    let wip = results[1].as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
    let prs = results[2].as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;

    let is_idle = ready + wip + prs == 0;

    let out = serde_json::json!({
        "idle": is_idle,
        "ready": ready,
        "wip": wip,
        "prs": prs,
    });
    println!("{out}");

    if is_idle {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn count_items(args: &[&str]) -> Result<u64> {
    let items = gh::list_json(args)?;
    Ok(items.len() as u64)
}

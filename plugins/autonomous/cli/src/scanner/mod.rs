pub mod issues;
pub mod pulls;

use anyhow::Result;

use crate::queue::Database;

/// 등록된 모든 레포를 스캔
pub async fn scan_all(db: &Database) -> Result<()> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT r.id, r.url, r.name, c.scan_targets, c.scan_interval_secs, c.filter_labels, c.ignore_authors \
         FROM repositories r JOIN repo_configs c ON r.id = c.repo_id \
         WHERE r.enabled = 1",
    )?;

    let repos: Vec<(String, String, String, String, i64, Option<String>, String)> =
        stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    for (repo_id, _url, name, targets, interval_secs, filter_labels, ignore_authors) in repos {
        // 마지막 스캔 시각 확인
        let should_scan = should_scan_repo(db, &repo_id, interval_secs)?;
        if !should_scan {
            continue;
        }

        let targets: Vec<String> = serde_json::from_str(&targets)?;
        let ignore: Vec<String> = serde_json::from_str(&ignore_authors)?;
        let labels: Option<Vec<String>> =
            filter_labels.and_then(|l| serde_json::from_str(&l).ok());

        tracing::info!("scanning {name}...");

        for target in &targets {
            match target.as_str() {
                "issues" => {
                    if let Err(e) = issues::scan(db, &repo_id, &name, &ignore, &labels).await {
                        tracing::error!("issue scan error for {name}: {e}");
                    }
                }
                "pulls" => {
                    if let Err(e) = pulls::scan(db, &repo_id, &name, &ignore).await {
                        tracing::error!("PR scan error for {name}: {e}");
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn should_scan_repo(db: &Database, repo_id: &str, interval_secs: i64) -> Result<bool> {
    let conn = db.conn();
    let last_scan: Option<String> = conn
        .query_row(
            "SELECT MAX(last_scan) FROM scan_cursors WHERE repo_id = ?1",
            rusqlite::params![repo_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    if let Some(last) = last_scan {
        if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(&last) {
            let elapsed = chrono::Utc::now().signed_duration_since(last_time);
            return Ok(elapsed.num_seconds() >= interval_secs);
        }
    }

    Ok(true) // 스캔 이력 없으면 즉시 스캔
}

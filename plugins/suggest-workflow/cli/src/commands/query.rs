use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::db::repository::{QueryParams, QueryRepository};

pub fn run(
    repo: &dyn QueryRepository,
    perspective: Option<&str>,
    sql_file: Option<&Path>,
    params: QueryParams,
) -> Result<()> {
    let result = match (perspective, sql_file) {
        // --sql-file takes priority
        (_, Some(path)) => {
            let sql = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("failed to read SQL file '{}': {}", path.display(), e))?;
            let trimmed = sql.trim();
            if trimmed.is_empty() {
                anyhow::bail!("SQL file is empty: {}", path.display());
            }
            repo.execute_sql(trimmed)?
        }
        // Named perspective
        (Some(name), None) => repo.query(name, &params)?,
        // Neither
        (None, None) => anyhow::bail!("--perspective or --sql-file required"),
    };

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub fn list(repo: &dyn QueryRepository) -> Result<()> {
    let perspectives = repo.list_perspectives()?;

    for p in &perspectives {
        eprintln!("  {} — {}", p.name, p.description);
        for param in &p.params {
            let required_tag = if param.required { " (required)" } else { "" };
            let default_tag = match &param.default {
                Some(d) => format!(" [default: {}]", d),
                None => String::new(),
            };
            eprintln!(
                "    --param {}  {:?}{}{}  {}",
                param.name, param.param_type, required_tag, default_tag, param.description
            );
        }
        eprintln!();
    }

    Ok(())
}

pub fn parse_params(raw: &[String]) -> Result<QueryParams> {
    let mut map = HashMap::new();
    for item in raw {
        let (key, value) = item
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --param format '{}': expected key=value", item))?;
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}

//! The ledger commands: record what a mod saw of one answer, list entries,
//! prune old ones. Each answers one JSON value.

use crate::kanban::commands::ledger_store::LedgerStore;
use crate::kanban::commands::payload::{parse_bind_facts, parse_ledger_facts};
use crate::kanban::core::allowlist::RealPaths;
use crate::kanban::core::ledger::{
    entries, month_of, permission_call, prune, select, DecidedBy, Entry, EntryKind, EntryVia,
    ListQuery, Pruned, DEFAULT_LIMIT, MAX_AGE_MS, MAX_BYTES, MAX_LIMIT,
};
use serde_json::{json, Value};

pub struct KanbanLedger<'a> {
    pub store: &'a dyn LedgerStore,
    pub paths: &'a dyn RealPaths,
}

/// `ledger list`'s arguments as given.
#[derive(Debug, Default, clap::Args)]
pub struct ListArgs {
    /// Kept from this time on, unix milliseconds
    #[arg(long)]
    pub since: Option<u64>,
    /// Kept up to this time, unix milliseconds
    #[arg(long)]
    pub until: Option<u64>,
    /// question, permission or decision (repeatable)
    #[arg(long = "kind")]
    pub kinds: Vec<String>,
    #[arg(long)]
    pub repo: Option<String>,
    #[arg(long)]
    pub session: Option<String>,
    /// human or proxy
    #[arg(long)]
    pub decided_by: Option<String>,
    /// session, pane, terminal or agent
    #[arg(long)]
    pub via: Option<String>,
    #[arg(long)]
    pub key: Option<String>,
    /// Matched in what was asked and answered, in any case
    #[arg(long, allow_hyphen_values = true)]
    pub search: Option<String>,
    #[arg(long)]
    pub target_session: Option<String>,
    #[arg(long)]
    pub target_wait: Option<String>,
    /// At most this many (default 50, up to 500)
    #[arg(long)]
    pub limit: Option<usize>,
}

fn answer(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|e| json!({ "error": e }))
}

fn parse_one<T>(
    what: &str,
    raw: &str,
    parse: fn(&str) -> Option<T>,
    known: &str,
) -> Result<T, String> {
    parse(raw).ok_or_else(|| format!("{what} 은 {known} 중 하나예요 (받은 값: {raw})"))
}

impl ListArgs {
    fn query(self) -> Result<ListQuery, String> {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(format!(
                "--limit 은 1~{MAX_LIMIT} 이에요 (받은 값: {limit})"
            ));
        }
        Ok(ListQuery {
            since: self.since,
            until: self.until,
            kinds: self
                .kinds
                .iter()
                .map(|k| {
                    parse_one(
                        "--kind",
                        k,
                        EntryKind::parse,
                        "question·permission·decision",
                    )
                })
                .collect::<Result<_, _>>()?,
            repo: self.repo,
            session: self.session,
            decided_by: self
                .decided_by
                .map(|d| parse_one("--decided-by", &d, DecidedBy::parse, "human·proxy"))
                .transpose()?,
            via: self
                .via
                .map(|v| parse_one("--via", &v, EntryVia::parse, "session·pane·terminal·agent"))
                .transpose()?,
            key: self.key,
            search: self.search,
            target_session: self.target_session,
            target_wait: self.target_wait,
            limit,
        })
    }
}

impl KanbanLedger<'_> {
    pub fn record(&self, raw: &str) -> Value {
        answer((|| {
            let facts = parse_ledger_facts(raw)?;
            let kept = entries(&facts, self.paths);
            if !kept.is_empty() {
                let lines: Vec<String> = kept.iter().map(Entry::to_line).collect();
                self.store.append(&month_of(facts.at), &lines)?;
            }
            Ok(json!({ "recorded": kept.iter().map(|e| e.id.as_str()).collect::<Vec<_>>() }))
        })())
    }

    pub fn bind(raw: &str) -> Value {
        answer(
            parse_bind_facts(raw)
                .map(|(tool, calls)| json!({ "call": permission_call(&tool, &calls) })),
        )
    }

    pub fn list(&self, args: ListArgs) -> Value {
        answer((|| {
            let query = args.query()?;
            let read: Vec<Entry> = self
                .store
                .read()?
                .iter()
                .flat_map(|month| &month.lines)
                .filter_map(|line| Entry::parse(line))
                .collect();
            Ok(json!({ "entries": select(read, &query) }))
        })())
    }

    pub fn prune(&self, now: u64) -> Value {
        answer((|| {
            let mut pruned = Pruned::default();
            self.store.rewrite(&mut |months| {
                let (kept, dropped) = prune(months, now, MAX_AGE_MS, MAX_BYTES);
                pruned = dropped;
                kept
            })?;
            Ok(json!({ "pruned": { "entries": pruned.entries, "files": pruned.files } }))
        })())
    }
}

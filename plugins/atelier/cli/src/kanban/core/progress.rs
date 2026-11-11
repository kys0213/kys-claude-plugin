//! The progress line of a card: the to-do count and the item in progress,
//! and while working the tool the session is running.

use super::text::{cut, fold};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStatus {
    Pending,
    InProgress,
    Completed,
    Deleted,
    /// A status this version does not know: counted, never done.
    Other,
}

impl ItemStatus {
    pub fn parse(raw: &str) -> ItemStatus {
        match raw {
            "pending" => ItemStatus::Pending,
            "in_progress" => ItemStatus::InProgress,
            "completed" => ItemStatus::Completed,
            "deleted" => ItemStatus::Deleted,
            _ => ItemStatus::Other,
        }
    }
}

/// One entry of the Task list (`subject`, maybe `active`) or of TodoWrite
/// (`active` only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItem {
    pub status: ItemStatus,
    pub subject: Option<String>,
    pub active: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub tool: String,
    /// The tool input's string fields, by name.
    pub input: Vec<(String, String)>,
    pub at: u64,
}

const CUT_CHARS: usize = 40;

/// Tried in order; the first non-blank one names what the tool works on.
const TARGET_FIELDS: [&str; 11] = [
    "description",
    "file_path",
    "notebook_path",
    "pattern",
    "url",
    "query",
    "skill",
    "command",
    "subject",
    "prompt",
    "path",
];

const PATH_FIELDS: [&str; 3] = ["file_path", "notebook_path", "path"];

pub struct ProgressFacts<'a> {
    pub tasks: &'a [WorkItem],
    pub todos: &'a [WorkItem],
    pub activity: Option<&'a Activity>,
    /// What runs in the background once the turn ended.
    pub background: Option<&'a str>,
}

pub fn progress_line(facts: &ProgressFacts) -> Option<String> {
    let parts: Vec<String> = [
        todo_part(facts.tasks, facts.todos),
        facts.activity.map(activity_part),
        facts.background.map(str::to_string),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The to-do list a session keeps: its Task list when it has one, else its
/// TodoWrite list, without deleted items.
pub fn live_items<'a>(tasks: &'a [WorkItem], todos: &'a [WorkItem]) -> Vec<&'a WorkItem> {
    let items = if tasks.is_empty() { todos } else { tasks };
    items
        .iter()
        .filter(|i| i.status != ItemStatus::Deleted)
        .collect()
}

/// The words an item is shown by: what it is doing while in progress, else
/// what it is.
pub fn item_text(item: &WorkItem) -> Option<String> {
    let text = match item.status {
        ItemStatus::InProgress => item.active.as_deref().or(item.subject.as_deref()),
        _ => item.subject.as_deref().or(item.active.as_deref()),
    };
    text.map(fold).filter(|t| !t.is_empty())
}

/// The first of `fields` the input holds non-blank, with its name.
pub fn first_field<'a>(
    input: &[(String, String)],
    fields: &[&'a str],
) -> Option<(&'a str, String)> {
    fields.iter().find_map(|field| {
        input
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| fold(value))
            .filter(|v| !v.is_empty())
            .map(|v| (*field, v))
    })
}

fn todo_part(tasks: &[WorkItem], todos: &[WorkItem]) -> Option<String> {
    let live = live_items(tasks, todos);
    if live.is_empty() {
        return None;
    }
    let done = live
        .iter()
        .filter(|i| i.status == ItemStatus::Completed)
        .count();
    let count = format!("할 일 {done}/{}", live.len());
    let current = live
        .iter()
        .find(|i| i.status == ItemStatus::InProgress)
        .and_then(|i| item_text(i));
    Some(match current {
        Some(current) => format!("{count} · {}", cut(&current, CUT_CHARS)),
        None => count,
    })
}

fn activity_part(activity: &Activity) -> String {
    format!("▸ {}", tool_line(activity))
}

/// `<tool>: <what it works on>`, the target cut to fit a card line.
pub fn tool_line(activity: &Activity) -> String {
    let target = first_field(&activity.input, &TARGET_FIELDS).map(|(field, value)| {
        if PATH_FIELDS.contains(&field) {
            file_name(&value)
        } else {
            value
        }
    });
    match target {
        Some(target) => format!("{}: {}", activity.tool, cut(&target, CUT_CHARS)),
        None => activity.tool.clone(),
    }
}

fn file_name(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

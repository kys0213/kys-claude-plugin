//! Session facts → kanban board. Pure: the caller supplies `now`, the
//! thresholds and every fact, so one snapshot always yields one board.

use super::duration::age_label;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub detail: Option<String>,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFact {
    pub session_id: String,
    pub cwd: String,
    pub repo: Option<String>,
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub started_at: u64,
    pub heartbeat_at: u64,
    pub ended_reason: Option<String>,
    /// Oldest first.
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardParams {
    pub now: u64,
    pub stale_after: u64,
    pub expire_after: u64,
    pub self_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    NeedsInput,
    Working,
    Idle,
    Stale,
}

impl Column {
    pub const ALL: [Column; 4] = [
        Column::NeedsInput,
        Column::Working,
        Column::Idle,
        Column::Stale,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Column::NeedsInput => "needs_input",
            Column::Working => "working",
            Column::Idle => "idle",
            Column::Stale => "stale",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Column::NeedsInput => "입력 대기",
            Column::Working => "작업 중",
            Column::Idle => "쉬는 중",
            Column::Stale => "응답 없음",
        }
    }

    fn longest_waiting_first(self) -> bool {
        matches!(self, Column::NeedsInput | Column::Working)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub session_id: String,
    pub label: String,
    pub subtitle: Option<String>,
    pub reason: Option<String>,
    pub age: String,
    pub is_self: bool,
}

impl Card {
    pub fn headline(&self) -> String {
        let marker = if self.is_self { "▶ " } else { "" };
        match &self.reason {
            Some(reason) => format!("{marker}{} · {reason} · {}", self.label, self.age),
            None => format!("{marker}{} · {}", self.label, self.age),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub columns: Vec<(Column, Vec<Card>)>,
    pub expired: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WaitReason {
    /// The tool asking, when the event names it.
    Permission(Option<String>),
    Question,
    Input,
}

impl WaitReason {
    fn label(&self) -> String {
        match self {
            WaitReason::Permission(Some(tool)) => format!("권한 요청: {tool}"),
            WaitReason::Permission(None) => "권한 요청".to_string(),
            WaitReason::Question => "질문".to_string(),
            WaitReason::Input => "입력 요청".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    NeedsInput(WaitReason),
    Working,
    Idle,
}

enum Signal {
    Set(State),
    /// The engine drew its spinner again. No event marks a person answering a
    /// permission dialog, but the spinner is hidden while one is open, so its
    /// return after a wait for input is the answer. It also draws after a
    /// turn ends, so it means nothing after any other state.
    Resumed,
    Nothing,
}

fn signal_of(event: &Event) -> Signal {
    let needs = |reason: WaitReason| Signal::Set(State::NeedsInput(reason));
    match (event.name.as_str(), event.detail.as_deref()) {
        ("prompt.submit" | "turn.start", _) => Signal::Set(State::Working),
        ("classic.PostToolUse" | "classic.PostToolUseFailure", _) => Signal::Set(State::Working),
        ("turn.complete" | "session.start", _) => Signal::Set(State::Idle),
        ("classic.PermissionRequest", Some("AskUserQuestion")) => needs(WaitReason::Question),
        ("classic.PermissionRequest", tool) => {
            needs(WaitReason::Permission(tool.map(str::to_string)))
        }
        ("classic.Notification", Some("permission_prompt")) => needs(WaitReason::Permission(None)),
        ("classic.Notification", Some("elicitation_dialog")) => needs(WaitReason::Input),
        ("classic.Notification", Some("idle_prompt")) => Signal::Set(State::Idle),
        ("ui.spinner", _) => Signal::Resumed,
        _ => Signal::Nothing,
    }
}

fn same_kind(a: &State, b: &State) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// A `permission_prompt` notification follows the request it repeats, so a
/// repeated state keeps its first event and that event's reason.
fn current_state(fact: &SessionFact) -> (State, u64) {
    let mut current: Option<(State, u64)> = None;
    for event in &fact.events {
        let waiting = matches!(current, Some((State::NeedsInput(_), _)));
        match signal_of(event) {
            Signal::Set(state)
                if current
                    .as_ref()
                    .is_some_and(|(cur, _)| same_kind(cur, &state)) => {}
            Signal::Set(state) => current = Some((state, event.at)),
            Signal::Resumed if waiting => current = Some((State::Working, event.at)),
            Signal::Resumed | Signal::Nothing => {}
        }
    }
    current.unwrap_or((State::Idle, fact.started_at))
}

fn label_of(fact: &SessionFact) -> String {
    let repo_name = fact
        .repo
        .as_deref()
        .and_then(|r| r.rsplit('/').next())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(name) = repo_name {
        return name.to_string();
    }
    fact.cwd
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("/")
        .to_string()
}

const SUBTITLE_CHARS: usize = 60;

fn subtitle_of(fact: &SessionFact) -> Option<String> {
    let folded = [&fact.title, &fact.first_prompt]
        .into_iter()
        .flatten()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|s| !s.is_empty())?;
    if folded.chars().count() <= SUBTITLE_CHARS {
        return Some(folded);
    }
    let mut cut: String = folded.chars().take(SUBTITLE_CHARS - 1).collect();
    cut.push('…');
    Some(cut)
}

struct Placed {
    column: Column,
    sort_at: u64,
    card: Card,
}

fn place(fact: &SessionFact, params: &BoardParams) -> Placed {
    let silent_for = params.now.saturating_sub(fact.heartbeat_at);
    let (column, reason, since) = if silent_for > params.stale_after {
        (Column::Stale, None, fact.heartbeat_at)
    } else {
        match current_state(fact) {
            (State::NeedsInput(reason), at) => (Column::NeedsInput, Some(reason.label()), at),
            (State::Working, at) => (Column::Working, None, at),
            (State::Idle, at) => (Column::Idle, None, at),
        }
    };
    Placed {
        column,
        sort_at: since,
        card: Card {
            session_id: fact.session_id.clone(),
            label: label_of(fact),
            subtitle: subtitle_of(fact),
            reason,
            age: age_label(params.now.saturating_sub(since)),
            is_self: params.self_id.as_deref() == Some(fact.session_id.as_str()),
        },
    }
}

fn is_expired(fact: &SessionFact, params: &BoardParams) -> bool {
    fact.ended_reason.is_some()
        || params.now.saturating_sub(fact.heartbeat_at) > params.expire_after
}

pub fn build(sessions: &[SessionFact], params: &BoardParams) -> Board {
    let mut expired = Vec::new();
    let mut placed = Vec::new();
    for fact in sessions {
        if is_expired(fact, params) {
            expired.push(fact.session_id.clone());
        } else {
            placed.push(place(fact, params));
        }
    }
    let columns = Column::ALL
        .into_iter()
        .map(|column| {
            let mut in_column: Vec<&Placed> =
                placed.iter().filter(|p| p.column == column).collect();
            in_column.sort_by(|a, b| {
                let by_time = if column.longest_waiting_first() {
                    a.sort_at.cmp(&b.sort_at)
                } else {
                    b.sort_at.cmp(&a.sort_at)
                };
                by_time.then_with(|| a.card.session_id.cmp(&b.card.session_id))
            });
            (
                column,
                in_column.into_iter().map(|p| p.card.clone()).collect(),
            )
        })
        .collect();
    Board { columns, expired }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str, detail: Option<&str>, at: u64) -> Event {
        Event {
            name: name.to_string(),
            detail: detail.map(str::to_string),
            at,
        }
    }

    fn fact(events: Vec<Event>) -> SessionFact {
        SessionFact {
            session_id: "s".to_string(),
            cwd: "/w/p".to_string(),
            repo: None,
            title: None,
            first_prompt: None,
            started_at: 1,
            heartbeat_at: 100,
            ended_reason: None,
            events,
        }
    }

    #[test]
    fn repeated_needs_input_keeps_the_first_reason_and_time() {
        let f = fact(vec![
            event("classic.PostToolUse", None, 10),
            event("classic.PermissionRequest", Some("Bash"), 20),
            event("classic.Notification", Some("permission_prompt"), 26),
        ]);
        assert_eq!(
            current_state(&f),
            (
                State::NeedsInput(WaitReason::Permission(Some("Bash".to_string()))),
                20
            )
        );
    }

    #[test]
    fn needs_input_after_working_is_not_merged_across_the_working_event() {
        let f = fact(vec![
            event("classic.PermissionRequest", Some("Bash"), 10),
            event("classic.PostToolUse", None, 20),
            event("classic.PermissionRequest", Some("Edit"), 30),
        ]);
        assert_eq!(
            current_state(&f),
            (
                State::NeedsInput(WaitReason::Permission(Some("Edit".to_string()))),
                30
            )
        );
    }

    #[test]
    fn no_state_event_falls_back_to_started_at() {
        assert_eq!(current_state(&fact(vec![])), (State::Idle, 1));
    }
}

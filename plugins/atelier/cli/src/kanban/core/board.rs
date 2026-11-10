//! Session facts → kanban board. Pure: the caller supplies `now`, the
//! thresholds and every fact, so one snapshot always yields one board.

use super::detail::{self, Decision, Delivery, Detail, DetailFacts, Phase};
use super::duration::age_label;
use super::progress::{progress_line, Activity, ProgressFacts, WorkItem};
use super::question::Ask;
use super::text::{cut, fold};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub detail: Option<String>,
    pub at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFact {
    pub session_id: String,
    pub cwd: String,
    pub repo: Option<String>,
    /// The main working tree of the session's repository.
    pub repo_root: Option<String>,
    pub remote: Option<String>,
    pub permission_mode: Option<String>,
    pub title: Option<String>,
    pub first_prompt: Option<String>,
    pub last_prompt: Option<String>,
    pub last_answer: Option<String>,
    /// The reply just before the session started waiting mid-turn.
    pub pending_context: Option<String>,
    /// The main loop's running turn; absent between turns.
    pub turn_id: Option<String>,
    pub started_at: u64,
    pub heartbeat_at: u64,
    pub ended_reason: Option<String>,
    /// Oldest first.
    pub events: Vec<Event>,
    /// The tool the main loop runs now; cleared when a turn ends.
    pub activity: Option<Activity>,
    /// The main loop's latest tool calls this turn.
    pub recent: Vec<Activity>,
    /// The permission request the session raised last, until its tool ends.
    pub permission: Option<Activity>,
    /// The decision the last turn ended on, until the next turn starts.
    pub decision: Option<Decision>,
    pub inbox: Option<Delivery>,
    pub tasks: Vec<WorkItem>,
    pub todos: Vec<WorkItem>,
    /// AskUserQuestion calls the mod has open.
    pub asks: Vec<Ask>,
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

/// What the board offers to do with a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Answer,
    /// Reply with one of the options the turn ended on.
    Decide,
    Send,
    Stop,
}

impl Action {
    pub fn id(self) -> &'static str {
        match self {
            Action::Answer => "answer",
            Action::Decide => "decide",
            Action::Send => "send",
            Action::Stop => "stop",
        }
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
    pub progress: Option<String>,
    pub last_line: Option<String>,
    pub cwd: String,
    pub repo_root: Option<String>,
    pub permission_mode: Option<String>,
    pub turn_id: Option<String>,
    pub actions: Vec<Action>,
    pub detail: Detail,
    /// Open questions, oldest first; empty unless the session waits on one.
    pub asks: Vec<Ask>,
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
    Decision,
    Input,
}

impl WaitReason {
    fn label(&self) -> String {
        match self {
            WaitReason::Permission(Some(tool)) => format!("권한 요청: {tool}"),
            WaitReason::Permission(None) => "권한 요청".to_string(),
            WaitReason::Question => "질문".to_string(),
            WaitReason::Decision => "결정 요청".to_string(),
            WaitReason::Input => "입력 요청".to_string(),
        }
    }

    fn phase(&self) -> Phase {
        match self {
            WaitReason::Permission(_) => Phase::Permission,
            WaitReason::Question => Phase::Question,
            WaitReason::Decision => Phase::Decision,
            WaitReason::Input => Phase::Input,
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
    /// permission dialog, but the terminal hides the spinner while one is
    /// open, so its return after a wait for input is the answer. It also draws
    /// after a turn ends, so it means nothing after any other state.
    Resumed,
    Nothing,
}

const ASK_TOOL: &str = "AskUserQuestion";

fn signal_of(event: &Event) -> Signal {
    let needs = |reason: WaitReason| Signal::Set(State::NeedsInput(reason));
    match (event.name.as_str(), event.detail.as_deref()) {
        ("prompt.submit" | "turn.start" | "kanban.answer", _) => Signal::Set(State::Working),
        ("classic.PostToolUse" | "classic.PostToolUseFailure", _) => Signal::Set(State::Working),
        ("turn.complete" | "session.start", _) => Signal::Set(State::Idle),
        ("classic.PermissionRequest", Some(ASK_TOOL)) => needs(WaitReason::Question),
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

fn moves_past_a_question(event: &Event) -> bool {
    match (event.name.as_str(), event.detail.as_deref()) {
        ("classic.PermissionRequest", Some(ASK_TOOL)) => false,
        ("classic.PermissionRequest", _) => true,
        (name, _) => matches!(
            name,
            "prompt.submit"
                | "turn.complete"
                | "session.start"
                | "kanban.answer"
                | "classic.PostToolUse"
                | "classic.PostToolUseFailure"
        ),
    }
}

fn open_asks(fact: &SessionFact) -> Vec<Ask> {
    let mut asks: Vec<Ask> = fact
        .asks
        .iter()
        .filter(|ask| {
            !fact
                .events
                .iter()
                .any(|e| e.at > ask.at && moves_past_a_question(e))
        })
        .cloned()
        .collect();
    asks.sort_by(|a, b| {
        a.at.cmp(&b.at)
            .then_with(|| a.tool_use_id.cmp(&b.tool_use_id))
    });
    asks
}

fn same_kind(a: &State, b: &State) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// A `permission_prompt` notification follows the request it repeats, so a
/// repeated state keeps its first event and that event's reason.
fn event_state(fact: &SessionFact) -> (State, u64) {
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

/// An open question waits whatever the events say, as some surfaces draw the
/// spinner under a dialog; a decision left by the last turn waits once the
/// turn is over.
fn current_state(fact: &SessionFact, asks: &[Ask]) -> (State, u64) {
    if let Some(first) = asks.first() {
        return (State::NeedsInput(WaitReason::Question), first.at);
    }
    match event_state(fact) {
        (State::Idle, at) if fact.decision.is_some() => {
            (State::NeedsInput(WaitReason::Decision), at)
        }
        other => other,
    }
}

fn last_path_part(path: &str) -> Option<&str> {
    path.trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .map(|s| s.trim_end_matches(".git"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn label_of(fact: &SessionFact) -> String {
    [&fact.repo, &fact.remote, &fact.repo_root]
        .into_iter()
        .flatten()
        .find_map(|s| last_path_part(s))
        .or_else(|| last_path_part(&fact.cwd))
        .unwrap_or("/")
        .to_string()
}

const SUBTITLE_CHARS: usize = 60;

fn subtitle_of(fact: &SessionFact) -> Option<String> {
    let folded = [&fact.title, &fact.first_prompt, &fact.last_prompt]
        .into_iter()
        .flatten()
        .map(|s| fold(&detail::clean(s)))
        .find(|s| !s.is_empty())?;
    Some(cut(&folded, SUBTITLE_CHARS))
}

fn actions_of(state: Option<&State>, fact: &SessionFact) -> Vec<Action> {
    let stop = fact.turn_id.as_ref().map(|_| Action::Stop);
    match state {
        None => Vec::new(),
        Some(State::NeedsInput(WaitReason::Question)) => {
            [Some(Action::Answer), stop].into_iter().flatten().collect()
        }
        Some(State::NeedsInput(WaitReason::Decision)) => vec![Action::Decide, Action::Send],
        Some(State::NeedsInput(_) | State::Working) => stop.into_iter().collect(),
        Some(State::Idle) => vec![Action::Send],
    }
}

fn phase_of(state: Option<&State>) -> Phase {
    match state {
        None => Phase::Stale,
        Some(State::NeedsInput(reason)) => reason.phase(),
        Some(State::Working) => Phase::Working,
        Some(State::Idle) => Phase::Idle,
    }
}

struct Placed {
    column: Column,
    sort_at: u64,
    card: Card,
}

fn place(fact: &SessionFact, params: &BoardParams) -> Placed {
    let silent_for = params.now.saturating_sub(fact.heartbeat_at);
    let asks = open_asks(fact);
    let state = (silent_for <= params.stale_after).then(|| current_state(fact, &asks));
    let (column, reason, since) = match &state {
        None => (Column::Stale, None, fact.heartbeat_at),
        Some((State::NeedsInput(reason), at)) => (Column::NeedsInput, Some(reason.label()), *at),
        Some((State::Working, at)) => (Column::Working, None, *at),
        Some((State::Idle, at)) => (Column::Idle, None, *at),
    };
    let state = state.map(|(s, _)| s);
    let progress = match column {
        Column::Stale => None,
        _ => progress_line(&ProgressFacts {
            tasks: &fact.tasks,
            todos: &fact.todos,
            activity: fact.activity.as_ref().filter(|_| column == Column::Working),
        }),
    };
    let detail = detail::build(
        phase_of(state.as_ref()),
        &DetailFacts {
            last_prompt: fact.last_prompt.as_deref(),
            last_answer: fact.last_answer.as_deref(),
            pending_context: fact.pending_context.as_deref(),
            permission: fact.permission.as_ref(),
            decision: fact.decision.as_ref(),
            tasks: &fact.tasks,
            todos: &fact.todos,
            recent: &fact.recent,
            inbox: fact.inbox.as_ref(),
        },
        params.now,
    );
    let is_question = matches!(state, Some(State::NeedsInput(WaitReason::Question)));
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
            progress,
            last_line: match column {
                Column::Idle => detail::last_line(fact.last_answer.as_deref()),
                _ => None,
            },
            cwd: fact.cwd.clone(),
            repo_root: fact.repo_root.clone(),
            permission_mode: fact.permission_mode.clone(),
            turn_id: fact.turn_id.clone(),
            actions: actions_of(state.as_ref(), fact),
            detail,
            asks: if is_question { asks } else { Vec::new() },
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
            started_at: 1,
            heartbeat_at: 100,
            events,
            ..SessionFact::default()
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
            event_state(&f),
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
            event_state(&f),
            (
                State::NeedsInput(WaitReason::Permission(Some("Edit".to_string()))),
                30
            )
        );
    }

    #[test]
    fn no_state_event_falls_back_to_started_at() {
        assert_eq!(event_state(&fact(vec![])), (State::Idle, 1));
    }

    #[test]
    fn the_label_part_of_a_remote_or_path_drops_the_git_suffix() {
        assert_eq!(last_path_part("git@h:o/r.git"), Some("r"));
        assert_eq!(last_path_part("git@h:r.git"), Some("r"));
        assert_eq!(last_path_part("/w/p/"), Some("p"));
        assert_eq!(last_path_part("/"), None);
    }
}

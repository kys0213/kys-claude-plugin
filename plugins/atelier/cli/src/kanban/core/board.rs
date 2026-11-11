//! Session facts → kanban board. Pure: the caller supplies `now`, the
//! thresholds and every fact, so one snapshot always yields one board.

use super::background::{self, AgentFact, Left};
use super::detail::{self, Decision, Delivery, Detail, DetailFacts, Phase};
use super::duration::age_label;
use super::progress::{progress_line, Activity, ProgressFacts, WorkItem};
use super::prompt::{is_wrapped_event, latest_person_prompt, PromptFact};
use super::question::Ask;
use super::text::{cut, fold};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub detail: Option<String>,
    pub at: u64,
    /// Raised by a subagent's or teammate's loop, not the main loop.
    pub by_agent: bool,
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
    /// Oldest first.
    pub prompts: Vec<PromptFact>,
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
    /// The id the mod takes a kanban answer for that request by; absent when
    /// the mod cannot take one.
    pub permission_id: Option<String>,
    /// The length each of `permission`'s input fields had before the mod cut
    /// it, in UTF-16 code units; absent when the record has none.
    pub permission_input_len: Option<Vec<(String, u64)>>,
    /// The decision the last turn ended on, until the next turn starts.
    pub decision: Option<Decision>,
    pub inbox: Option<Delivery>,
    pub tasks: Vec<WorkItem>,
    pub todos: Vec<WorkItem>,
    /// AskUserQuestion calls the mod has open.
    pub asks: Vec<Ask>,
    /// The session's agent loops as the mod last listed them.
    pub agents: Vec<AgentFact>,
    /// When a person turned this session into the delegate; absent while it is not one.
    pub delegate_since: Option<u64>,
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

    pub fn heading(self, count: usize) -> String {
        format!("{} {count}", self.title())
    }

    /// Every id must name a column; an empty list names none.
    pub fn parse_all(ids: &[String]) -> Result<Vec<Column>, String> {
        ids.iter()
            .map(|id| {
                Column::ALL
                    .into_iter()
                    .find(|column| column.id() == id)
                    .ok_or_else(|| format!("모르는 컬럼이에요: {id}"))
            })
            .collect()
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
    /// Allow or deny the permission request.
    Permit,
    Send,
    Stop,
}

impl Action {
    pub fn id(self) -> &'static str {
        match self {
            Action::Answer => "answer",
            Action::Decide => "decide",
            Action::Permit => "permit",
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
    pub permission_id: Option<String>,
    pub actions: Vec<Action>,
    pub detail: Detail,
    /// Open questions, oldest first; empty unless the session waits on one.
    pub asks: Vec<Ask>,
    /// What a session in the needs-input column waits on.
    pub waiting: Option<Waiting>,
    pub delegate_since: Option<u64>,
    /// How the last kanban request this session took went.
    pub inbox: Option<Delivery>,
}

/// The permission request a session waits on, with the lengths its input had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionAsk {
    pub id: Option<String>,
    pub request: Activity,
    pub input_len: Option<Vec<(String, u64)>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitOn {
    /// The oldest open question.
    Question(Ask),
    Permission(PermissionAsk),
    Decision(Decision),
    Input,
}

/// The facts of one wait as recorded: what it is on, since when, the person's
/// latest prompt and the reply just before it, uncut by the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub on: WaitOn,
    pub since: u64,
    pub request: Option<String>,
    pub context: Option<String>,
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

impl Board {
    /// Keeps the named columns in the board's order; `expired` stays whole.
    /// An empty list keeps every column.
    pub fn only(mut self, columns: &[Column]) -> Board {
        if !columns.is_empty() {
            self.columns.retain(|(column, _)| columns.contains(column));
        }
        self
    }
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
    /// The main loop's turn ended with work still running in the background.
    Background {
        left: Left,
        ended_at: u64,
    },
    Idle,
}

impl State {
    fn is_busy(&self) -> bool {
        matches!(self, State::Working | State::Background { .. })
    }
}

enum Signal {
    Set(State),
    /// The engine drew its spinner again. No event marks a person answering a
    /// permission dialog, but the terminal hides the spinner while one is
    /// open, so its return after a wait for input is the answer. It also draws
    /// after a turn ends, so it means nothing after any other state.
    Resumed,
    TurnEnded(Left),
    IdlePrompt,
    Nothing,
}

const ASK_TOOL: &str = "AskUserQuestion";

fn signal_of(event: &Event) -> Signal {
    let needs = |reason: WaitReason| Signal::Set(State::NeedsInput(reason));
    match (event.name.as_str(), event.detail.as_deref()) {
        ("prompt.submit" | "turn.start" | "kanban.answer", _) => Signal::Set(State::Working),
        ("classic.PostToolUse" | "classic.PostToolUseFailure", _) => Signal::Set(State::Working),
        ("turn.complete", left) => Signal::TurnEnded(background::left_by(left)),
        ("session.start", _) => Signal::Set(State::Idle),
        ("classic.PermissionRequest", Some(ASK_TOOL)) => needs(WaitReason::Question),
        ("classic.PermissionRequest", tool) => {
            needs(WaitReason::Permission(tool.map(str::to_string)))
        }
        ("classic.Notification", Some("permission_prompt")) => needs(WaitReason::Permission(None)),
        ("classic.Notification", Some("elicitation_dialog")) => needs(WaitReason::Input),
        ("classic.Notification", Some("idle_prompt")) => Signal::IdlePrompt,
        ("ui.spinner", _) => Signal::Resumed,
        _ => Signal::Nothing,
    }
}

/// A question's own result, an answer from the board, or a turn boundary; a
/// prompt queued while the dialog is open or another tool's events are not.
fn moves_past_a_question(event: &Event) -> bool {
    match (event.name.as_str(), event.detail.as_deref()) {
        ("classic.PostToolUse" | "classic.PostToolUseFailure", detail) => detail == Some(ASK_TOOL),
        (name, _) => matches!(
            name,
            "turn.start" | "turn.complete" | "session.start" | "kanban.answer"
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

fn after_turn(left: Left, ended_at: u64) -> State {
    match left {
        Left::Nothing => State::Idle,
        left => State::Background { left, ended_at },
    }
}

/// A `permission_prompt` notification follows the request it repeats, so a
/// repeated state keeps its first event and that event's reason. Work that
/// goes on across turn ends keeps the time it began.
fn advance(current: Option<(State, u64)>, next: State, at: u64) -> (State, u64) {
    match current {
        Some((cur, since)) if cur.is_busy() && next.is_busy() => (next, since),
        Some((cur, since)) if same_kind(&cur, &next) => (cur, since),
        _ => (next, at),
    }
}

/// The state the events reach. A wait an agent's loop raises holds the state
/// before it, and the end of that wait returns there: the agent's own tools
/// say nothing about the main loop.
struct Tracker {
    current: Option<(State, u64)>,
    held: Option<(State, u64)>,
    started_at: u64,
}

impl Tracker {
    fn new(started_at: u64) -> Tracker {
        Tracker {
            current: None,
            held: None,
            started_at,
        }
    }

    fn state(&self) -> Option<&State> {
        self.current.as_ref().map(|(state, _)| state)
    }

    fn move_to(&mut self, next: State, at: u64) {
        self.current = Some(advance(self.current.take(), next, at));
    }

    fn hold_for(&mut self, wait: WaitReason, at: u64) {
        if !matches!(self.state(), Some(State::NeedsInput(_))) {
            self.held = Some(
                self.current
                    .clone()
                    .unwrap_or((State::Idle, self.started_at)),
            );
        }
        self.move_to(State::NeedsInput(wait), at);
    }

    /// Whether a held state was there to return to.
    fn release(&mut self) -> bool {
        match self.held.take() {
            Some(back) => {
                self.current = Some(back);
                true
            }
            None => false,
        }
    }

    fn on_agent(&mut self, event: &Event) {
        match signal_of(event) {
            Signal::Set(State::NeedsInput(wait)) => self.hold_for(wait, event.at),
            Signal::Set(State::Working) => {
                self.release();
            }
            _ => {}
        }
    }

    fn on_main(&mut self, event: &Event) {
        let resumable =
            matches!(self.state(), Some(State::NeedsInput(r)) if *r != WaitReason::Question);
        let backgrounded = matches!(self.state(), Some(State::Background { .. }));
        let next = match signal_of(event) {
            Signal::Set(state) => Some(state),
            Signal::TurnEnded(left) => Some(after_turn(left, event.at)),
            Signal::IdlePrompt if !backgrounded => Some(State::Idle),
            Signal::Resumed if resumable && self.release() => None,
            Signal::Resumed if resumable => Some(State::Working),
            Signal::IdlePrompt | Signal::Resumed | Signal::Nothing => None,
        };
        if let Some(next) = next {
            if !matches!(next, State::NeedsInput(_)) {
                self.held = None;
            }
            self.move_to(next, event.at);
        }
    }

    fn finish(self) -> (State, u64) {
        self.current.unwrap_or((State::Idle, self.started_at))
    }
}

fn event_state(fact: &SessionFact) -> (State, u64) {
    let mut tracker = Tracker::new(fact.started_at);
    for event in &fact.events {
        if event.by_agent {
            tracker.on_agent(event);
        } else {
            tracker.on_main(event);
        }
    }
    tracker.finish()
}

/// An open question waits whatever the events say, as some surfaces draw the
/// spinner under a dialog; a decision left by the last turn waits once the
/// turn is over. Agents work on without the main loop hearing of it, so their
/// last listing settles a turn end that left teammates or listed nothing.
fn current_state(fact: &SessionFact, asks: &[Ask]) -> (State, u64) {
    if let Some(first) = asks.first() {
        return (State::NeedsInput(WaitReason::Question), first.at);
    }
    let agents_work = background::any_agent_working(&fact.agents);
    match event_state(fact) {
        (State::Idle, at) | (State::Background { ended_at: at, .. }, _)
            if fact.decision.is_some() =>
        {
            (State::NeedsInput(WaitReason::Decision), at)
        }
        (
            State::Background {
                left: Left::Team,
                ended_at,
            },
            _,
        ) if !agents_work => (State::Idle, ended_at),
        (State::Idle, at) if agents_work => (
            State::Background {
                left: Left::Team,
                ended_at: at,
            },
            at,
        ),
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
    let folded = [fact.title.as_deref(), fact.first_prompt.as_deref()]
        .into_iter()
        .flatten()
        .filter(|p| !is_wrapped_event(p))
        .chain(person_prompt(fact))
        .map(|s| fold(&detail::clean(s)))
        .find(|s| !s.is_empty())?;
    Some(cut(&folded, SUBTITLE_CHARS))
}

fn person_prompt(fact: &SessionFact) -> Option<&str> {
    latest_person_prompt(&fact.prompts, fact.last_prompt.as_deref())
}

fn actions_of(state: Option<&State>, fact: &SessionFact) -> Vec<Action> {
    let stop = fact.turn_id.as_ref().map(|_| Action::Stop);
    match state {
        None => Vec::new(),
        Some(State::NeedsInput(WaitReason::Question)) => {
            [Some(Action::Answer), stop].into_iter().flatten().collect()
        }
        Some(State::NeedsInput(WaitReason::Decision)) => vec![Action::Decide, Action::Send],
        Some(State::NeedsInput(WaitReason::Permission(_))) => {
            let permit = fact.permission_id.as_ref().map(|_| Action::Permit);
            [permit, stop].into_iter().flatten().collect()
        }
        Some(State::NeedsInput(_) | State::Working | State::Background { .. }) => {
            stop.into_iter().collect()
        }
        Some(State::Idle) => vec![Action::Send],
    }
}

fn phase_of(state: Option<&State>) -> Phase {
    match state {
        None => Phase::Stale,
        Some(State::NeedsInput(reason)) => reason.phase(),
        Some(State::Working | State::Background { .. }) => Phase::Working,
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
        Some((State::Working | State::Background { .. }, at)) => (Column::Working, None, *at),
        Some((State::Idle, at)) => (Column::Idle, None, *at),
    };
    let state = state.map(|(s, _)| s);
    let background = matches!(state, Some(State::Background { .. }))
        .then(|| background::background_line(last_turn_end(fact), &fact.agents));
    let progress = match column {
        Column::Stale => None,
        _ => progress_line(&ProgressFacts {
            tasks: &fact.tasks,
            todos: &fact.todos,
            activity: fact
                .activity
                .as_ref()
                .filter(|_| matches!(state, Some(State::Working))),
            background: background.as_deref(),
        }),
    };
    let detail = detail::build(
        phase_of(state.as_ref()),
        &DetailFacts {
            last_prompt: person_prompt(fact),
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
    let waiting = match &state {
        Some(State::NeedsInput(reason)) => wait_on(reason, fact, &asks).map(|on| Waiting {
            on,
            since,
            request: person_prompt(fact).map(str::to_string),
            context: fact.pending_context.clone(),
        }),
        _ => None,
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
            progress,
            last_line: match column {
                Column::Idle => detail::last_line(fact.last_answer.as_deref()),
                _ => None,
            },
            cwd: fact.cwd.clone(),
            repo_root: fact.repo_root.clone(),
            permission_mode: fact.permission_mode.clone(),
            turn_id: fact.turn_id.clone(),
            permission_id: match state {
                Some(State::NeedsInput(WaitReason::Permission(_))) => fact.permission_id.clone(),
                _ => None,
            },
            actions: actions_of(state.as_ref(), fact),
            detail,
            asks: if is_question { asks } else { Vec::new() },
            waiting,
            delegate_since: fact.delegate_since,
            inbox: fact.inbox.clone(),
        },
    }
}

/// `None` when the record lacks what the wait is on: a permission request
/// whose event came without a recorded request, or a decision state without
/// its decision.
fn wait_on(reason: &WaitReason, fact: &SessionFact, asks: &[Ask]) -> Option<WaitOn> {
    match reason {
        WaitReason::Question => asks.first().cloned().map(WaitOn::Question),
        WaitReason::Permission(_) => fact.permission.as_ref().map(|request| {
            WaitOn::Permission(PermissionAsk {
                id: fact.permission_id.clone(),
                request: request.clone(),
                input_len: fact.permission_input_len.clone(),
            })
        }),
        WaitReason::Decision => fact.decision.clone().map(WaitOn::Decision),
        WaitReason::Input => Some(WaitOn::Input),
    }
}

fn last_turn_end(fact: &SessionFact) -> Option<&str> {
    fact.events
        .iter()
        .rev()
        .find(|e| e.name == "turn.complete")
        .and_then(|e| e.detail.as_deref())
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
            by_agent: false,
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

//! Work a session keeps running in the background after its main loop's turn
//! ends: what the turn left running, whether its agents still work, and the
//! line a card shows for it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Pending,
    Running,
    Waiting,
    Idle,
    Completed,
    Failed,
    Killed,
    /// A status this version does not know: never counted as working.
    Other,
}

impl AgentStatus {
    pub fn parse(raw: &str) -> AgentStatus {
        match raw {
            "pending" => AgentStatus::Pending,
            "running" => AgentStatus::Running,
            "waiting" => AgentStatus::Waiting,
            "idle" => AgentStatus::Idle,
            "completed" => AgentStatus::Completed,
            "failed" => AgentStatus::Failed,
            "killed" => AgentStatus::Killed,
            _ => AgentStatus::Other,
        }
    }

    fn is_working(self) -> bool {
        matches!(
            self,
            AgentStatus::Pending | AgentStatus::Running | AgentStatus::Waiting
        )
    }
}

/// One of the session's agent loops as the mod last listed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFact {
    pub status: AgentStatus,
    pub teammate: bool,
}

/// What a turn's end left running, read from the background task types its
/// `turn.complete` event lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    Nothing,
    /// Teammates alone: the engine lists a live teammate whether it works or
    /// waits for a message, so only the agent list tells.
    Team,
    Work,
}

const WORK_TYPES: [&str; 3] = ["subagent", "workflow", "cloud session"];
const TEAMMATE_TYPE: &str = "teammate";

fn task_types(detail: Option<&str>) -> impl Iterator<Item = &str> {
    detail
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

pub fn left_by(detail: Option<&str>) -> Left {
    let types: Vec<&str> = task_types(detail).collect();
    if types.iter().any(|t| WORK_TYPES.contains(t)) {
        Left::Work
    } else if types.contains(&TEAMMATE_TYPE) {
        Left::Team
    } else {
        Left::Nothing
    }
}

pub fn any_agent_working(agents: &[AgentFact]) -> bool {
    agents.iter().any(|a| a.status.is_working())
}

pub fn background_line(detail: Option<&str>, agents: &[AgentFact]) -> String {
    let working = |teammate: bool| {
        agents
            .iter()
            .filter(|a| a.teammate == teammate && a.status.is_working())
            .count()
    };
    let tasks = |kind: &str| task_types(detail).filter(|t| *t == kind).count();
    let parts: Vec<String> = [
        ("에이전트", working(false)),
        ("팀원", working(true)),
        ("워크플로", tasks("workflow")),
        ("원격 세션", tasks("cloud session")),
    ]
    .into_iter()
    .filter(|(_, n)| *n > 0)
    .map(|(name, n)| format!("{name} {n}"))
    .collect();
    if parts.is_empty() {
        "▸ 백그라운드 작업".to_string()
    } else {
        format!("▸ 백그라운드: {}", parts.join(" · "))
    }
}

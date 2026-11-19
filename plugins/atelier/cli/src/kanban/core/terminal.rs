//! Which herdr-core terminal a session runs in, judged from the environment
//! its mod read at session start.

/// The session process's `HC_TERMINAL_ID`, `HERDR_ENV` and
/// `CLAUDE_CODE_SESSION_KIND`, as recorded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalEnv {
    pub hc_terminal_id: Option<String>,
    pub herdr_env: Option<String>,
    pub session_kind: Option<String>,
}

impl TerminalEnv {
    /// `HC_TERMINAL_ID` passes down to every process a terminal starts, so a
    /// `claude --bg` session and a Claude inside a herdr window carry the
    /// address of the terminal that started them: only an interactive session
    /// outside a herdr window has the terminal its own.
    pub fn hc_terminal(&self) -> Option<String> {
        if self.herdr_env.is_some() || self.session_kind.as_deref() == Some("bg") {
            return None;
        }
        self.hc_terminal_id.clone().filter(|id| !id.is_empty())
    }
}

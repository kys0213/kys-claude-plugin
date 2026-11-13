use crate::kanban::commands::ledger::ListArgs;
use crate::kanban::commands::requests::Proof;
use crate::kanban::core::duration::parse_millis;
use crate::kanban::core::request::Sender;
use clap::{Parser, Subcommand};

use super::now_millis;

#[derive(Parser)]
#[command(
    name = "kanban",
    version,
    about = "Session kanban board for the hooks module's pane"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Session facts on stdin → columns of cards and the ids to drop
    Board {
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// Heartbeat silence after which a session is shown as stale (e.g. 2m)
        #[arg(long, value_parser = parse_millis)]
        stale_after: u64,
        /// Heartbeat silence after which a session is dropped (e.g. 24h)
        #[arg(long, value_parser = parse_millis)]
        expire_after: u64,
        /// The pane's width in columns, which decides the board's layout
        #[arg(long)]
        width: u32,
        /// The calling session's id, marked `is_self`
        #[arg(long = "self")]
        self_id: Option<String>,
        /// Keeps only this column (repeatable): needs_input, working, idle, stale
        #[arg(long = "column")]
        columns: Vec<String>,
    },
    /// The board of the sessions' record copies, read without the pane
    Status {
        /// Keeps only this column (repeatable): needs_input, working, idle, stale
        #[arg(long = "column")]
        columns: Vec<String>,
        /// Lists only this session
        #[arg(long)]
        session: Option<String>,
        /// Current time, unix milliseconds; the system clock when absent
        #[arg(long)]
        now: Option<u64>,
        /// `json` or `text`
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// Where a session's record copy is
    RecordPath {
        #[arg(long)]
        session: String,
    },
    /// Removes a session's record copy and inbox
    Forget {
        #[arg(long)]
        session: String,
    },
    /// An AskUserQuestion call's questions on stdin → them rewritten over the
    /// wait their first line names, and that wait
    RelayFrame {
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// The asking session; only the delegate session frames
        #[arg(long)]
        session: String,
    },
    /// `/kanban <args>` as the mod saw it run → whether to toggle the pane, or what to tell the person
    Command {
        #[arg(long)]
        session: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// The run's `origin.kind`
        #[arg(long)]
        origin: String,
        /// Everything typed after `/kanban`
        #[arg(long, allow_hyphen_values = true)]
        args: String,
    },
    /// The delegate session's wakes and daily reports
    Delegate {
        #[command(subcommand)]
        command: DelegateCommandArgs,
    },
    /// An open question and the picks made on the board → one answer per question
    Answer,
    /// The Bash tool's input on stdin → why a model may not run it, if it may not
    GuardBash,
    /// The plugin store's keys on stdin, oldest first → the nonce keys past the cap to delete
    PruneNonces,
    /// The decisions taken for this machine's waits
    Ledger {
        #[command(subcommand)]
        command: LedgerCommand,
    },
    #[command(flatten)]
    Request(RequestCommand),
}

#[derive(Subcommand)]
pub enum DelegateCommandArgs {
    /// The records and the delegate's state on stdin → what to say now, and how
    Wake {
        #[arg(long)]
        session: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// The local time of day the daily report falls due, `HH:MM`
        #[arg(long)]
        report_at: String,
        /// The local clock's minutes east of UTC
        #[arg(long, allow_hyphen_values = true)]
        utc_offset: i64,
    },
    /// Leaves the daily report given at `now`
    Report {
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
    },
    /// A prompt entering the delegate session → whether the wake's context rides on it
    Attach {
        /// The prompt's `origin.kind`
        #[arg(long, allow_hyphen_values = true)]
        origin: String,
        /// The prompt runs a slash command
        #[arg(long)]
        slash_command: bool,
        /// The prompt is a meta prompt
        #[arg(long)]
        meta: bool,
    },
}

#[derive(Subcommand)]
pub enum LedgerCommand {
    /// What a session saw of one answer it took, on stdin → its entries
    Record,
    /// A permission request's tool and the calls in flight, on stdin → the call it belongs to
    Bind,
    /// Entries, newest first
    List(Box<ListArgs>),
    /// How the entries `list` keeps for a period were decided, counted
    Summary {
        /// Counted from this time on, unix milliseconds
        #[arg(long)]
        since: u64,
        /// Counted up to this time, unix milliseconds
        #[arg(long)]
        until: Option<u64>,
    },
    /// Whether a decision key's precedents let the main agent answer for it
    Match {
        #[arg(long)]
        key: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
    },
    /// Turns a proxy answer over, which turns its decision key off
    Revoke {
        /// The proxy answer's ledger entry id
        #[arg(long)]
        entry: String,
        #[arg(long, allow_hyphen_values = true)]
        reason: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
    },
    /// Each kind's mode and shadow comparisons, and the keys turned off
    Stats {
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
    },
    /// Drops entries older than 180 days, then the oldest past 50 MB
    Prune {
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
    },
}

#[derive(Subcommand)]
pub enum RequestCommand {
    /// Leaves a prompt in a session's inbox
    Send {
        #[arg(long)]
        session: String,
        /// `pane` for a person on the board, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// Names the sending session in an agent prompt's header
        #[arg(long)]
        from_label: Option<String>,
        /// The pane's one-time token, carried to the taking session as given
        #[arg(long)]
        nonce: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        text: String,
    },
    /// Leaves the answer to one permission request for the session that asked
    Permit {
        #[arg(long)]
        session: String,
        /// The permission request's id, as the board's card names it
        #[arg(long)]
        request: String,
        /// `allow` or `deny`
        #[arg(long)]
        decision: String,
        /// `pane` for a person on the board or a terminal, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// The pane's one-time token, carried to the answering session as given
        #[arg(long)]
        nonce: Option<String>,
        #[command(flatten)]
        proof: ProofArgs,
    },
    /// Leaves the answers to one open question for the session that asked
    Reply {
        #[arg(long)]
        session: String,
        /// The question's tool_use_id, as the board's wait names it
        #[arg(long)]
        ask: String,
        /// One per question, in the questions' order
        #[arg(long = "answer", allow_hyphen_values = true)]
        answers: Vec<String>,
        /// `pane` for a person on the board or a terminal, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// The pane's one-time token, carried to the answering session as given
        #[arg(long)]
        nonce: Option<String>,
        #[command(flatten)]
        proof: ProofArgs,
    },
    /// Leaves the reply to the decision a session's last turn ended on
    Decide {
        #[arg(long)]
        session: String,
        /// The decision's id, as the board's wait names it
        #[arg(long)]
        at: String,
        #[arg(long, allow_hyphen_values = true)]
        text: String,
        /// `pane` for a person on the board or a terminal, `agent` for a model
        #[arg(long, value_parser = parse_sender)]
        from: Sender,
        /// The pane's one-time token, carried to the answering session as given
        #[arg(long)]
        nonce: Option<String>,
        #[command(flatten)]
        proof: ProofArgs,
    },
    /// Leaves a stop for one turn in a session's inbox
    Stop {
        #[arg(long)]
        session: String,
        #[arg(long)]
        turn: String,
    },
    /// A session's own inbox
    Inbox {
        #[command(subcommand)]
        command: InboxCommand,
    },
    /// Starts a background session at a repository's main working tree
    Spawn {
        #[arg(long)]
        cwd: String,
        #[arg(long, allow_hyphen_values = true)]
        text: String,
        /// Runs the session in a new git worktree of this name
        #[arg(long, allow_hyphen_values = true)]
        worktree: Option<String>,
        #[arg(long)]
        from_label: Option<String>,
    },
}

#[derive(clap::Args)]
pub struct ProofArgs {
    /// `<question>:<ledger entry id>` of a person's decision the answer
    /// follows (repeatable; `1` for a permission request)
    #[arg(long = "precedent")]
    precedents: Vec<String>,
    /// Why the precedents fit, up to 200 characters
    #[arg(long, allow_hyphen_values = true)]
    reason: Option<String>,
    /// The ledger entry id of the answer a person gave when asked, as the
    /// wait's `relays` names it
    #[arg(long)]
    relay: Option<String>,
    /// When the precedents or the relay are judged, unix milliseconds; the system clock when absent
    #[arg(long)]
    now: Option<u64>,
}

impl ProofArgs {
    pub(super) fn proof(&self) -> Proof<'_> {
        Proof {
            precedents: &self.precedents,
            reason: self.reason.as_deref(),
            relay: self.relay.as_deref(),
            now: self.now.unwrap_or_else(now_millis),
        }
    }
}

#[derive(Subcommand)]
pub enum InboxCommand {
    /// Where the inbox is
    Dir {
        #[arg(long)]
        session: String,
    },
    /// Claims every request and says which to act on
    Take {
        #[arg(long)]
        session: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// Age after which a request is dropped (e.g. 60s)
        #[arg(long, value_parser = parse_millis)]
        ttl: u64,
        /// The turn the session runs now; absent while it waits for a prompt
        #[arg(long)]
        turn: Option<String>,
        /// Reads the open questions, the decision and the nonces the pane
        /// left on stdin; without it stdin is left unread, the session waits
        /// on none and every answer comes through the terminal
        #[arg(long)]
        waits_stdin: bool,
    },
    /// Waits a little for the answer to one permission request
    WaitPermit {
        #[arg(long)]
        session: String,
        #[arg(long)]
        request: String,
        /// Current time, unix milliseconds
        #[arg(long)]
        now: u64,
        /// Age after which an answer is dropped (e.g. 60s)
        #[arg(long, value_parser = parse_millis)]
        ttl: u64,
        /// How long to wait for it (e.g. 2s)
        #[arg(long = "for", value_parser = parse_millis)]
        wait: u64,
        /// Reads the nonces the pane left on stdin; without it stdin is left
        /// unread and the answer comes through the terminal
        #[arg(long, conflicts_with = "wait_stdin")]
        nonces_stdin: bool,
        /// Reads the nonces and the request the session waits on, its input
        /// whole, on stdin; only then is a model's answer judged
        #[arg(long)]
        wait_stdin: bool,
    },
    /// Removes the inbox and what is left in it
    Clear {
        #[arg(long)]
        session: String,
    },
}

fn parse_sender(raw: &str) -> Result<Sender, String> {
    Sender::parse(raw).ok_or_else(|| format!("pane 또는 agent 예요 (받은 값: {raw})"))
}

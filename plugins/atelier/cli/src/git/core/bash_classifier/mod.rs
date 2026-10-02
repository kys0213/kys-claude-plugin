//! Pure Bash command classifier. Turns a command line into the facts a guard
//! needs — hook-bypass flags, commits, file writes, opaque executions — and
//! the directory each fact is anchored to. No git, filesystem or environment
//! access: the cwd and HOME arrive through `ClassifyInput`.

mod analyzer;
mod args;
mod files;
mod git;
mod interpreters;
mod lexer;
mod wrappers;

use std::path::{Path, PathBuf};

use analyzer::Analyzer;
use args::normalize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifyInput {
    pub command: String,
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BashAnalysis {
    pub bypass: Vec<BypassHit>,
    pub hits: Vec<Hit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BypassHit {
    pub rule: BypassRule,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassRule {
    NoVerifyFlag,
    CommitShortNoVerify,
    HooksPathConfig,
    HooksPathEnv,
    HookSkipEnv,
    /// A write into a repository's `.git/hooks` directory.
    HooksDirWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub kind: HitKind,
    pub anchor: Anchor,
    pub program: String,
    /// The branch this effect lands on when an earlier `&&`-chained command in
    /// the same line switched branches.
    pub on_branch: Option<BranchSwitch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchSwitch {
    /// Where the switching git ran; the switch applies to that repository.
    pub repo: Anchor,
    pub branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitKind {
    Commit,
    Write(WriteRule),
    Opaque(OpaqueCause),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteRule {
    Redirect,
    InPlaceEdit,
    FileOp,
    CopyDest,
    DdOutput,
    Download,
    FindDelete,
    Patch,
    Extract,
    GitWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpaqueCause {
    Interpreter(String),
    ScriptFile(String),
    Eval,
    DynamicCommand,
    Substitution,
    Wrapper(Wrapper),
    /// The command line has an unterminated quote or substitution; the rest
    /// was read as if it closed at the end of input.
    Unparsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrapper {
    Env,
    Sudo,
    Nohup,
    Timeout,
    Time,
    Command,
    Xargs,
    FindExec,
    ShellC,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// A normalized path that need not exist. The effect lands at or below it,
    /// so consumers resolve it by walking up its ancestors.
    Path(PathBuf),
    Unresolved {
        literal_prefix: Option<PathBuf>,
        raw: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexError {
    UnterminatedQuote,
    UnterminatedSubstitution,
}

pub fn classify(input: &ClassifyInput) -> BashAnalysis {
    let cwd = Anchor::Path(normalize(&input.cwd));
    let mut analyzer = Analyzer::new(input.home.as_deref());
    if analyzer
        .run_script(&input.command, &cwd, &Vec::new(), 0, false)
        .is_ok()
    {
        return analyzer.out;
    }
    let mut analyzer = Analyzer::new(input.home.as_deref());
    let _ = analyzer.run_script(&input.command, &cwd, &Vec::new(), 0, true);
    let program = input.command.split_whitespace().next().unwrap_or("");
    analyzer.opaque(OpaqueCause::Unparsed, program, &cwd);
    analyzer.out
}

/// Whether `path` lies in a `.git/hooks` directory.
pub fn is_hooks_path(path: &Path) -> bool {
    let parts: Vec<_> = path.components().collect();
    parts
        .windows(2)
        .any(|w| w[0].as_os_str() == ".git" && w[1].as_os_str() == "hooks")
}

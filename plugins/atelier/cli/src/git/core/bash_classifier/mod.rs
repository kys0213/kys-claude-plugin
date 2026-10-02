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

use std::path::PathBuf;

use analyzer::Analyzer;
use args::{normalize, Place};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub kind: HitKind,
    pub anchor: Anchor,
    pub program: String,
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
    /// The command line could not be tokenized at all.
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
pub enum LexError {
    UnterminatedQuote,
    UnterminatedSubstitution,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LexError::UnterminatedQuote => f.write_str("unterminated quote"),
            LexError::UnterminatedSubstitution => f.write_str("unterminated substitution"),
        }
    }
}

impl std::error::Error for LexError {}

pub fn classify(input: &ClassifyInput) -> Result<BashAnalysis, LexError> {
    let mut analyzer = Analyzer {
        home: input.home.as_deref(),
        out: BashAnalysis::default(),
    };
    let cwd = Place::Known(normalize(&input.cwd));
    analyzer.run_script(&input.command, &cwd, &Vec::new(), 0)?;
    Ok(analyzer.out)
}

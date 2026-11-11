//! Which permission requests only read inside the session's repository: the
//! file-reading tools on paths in it, and shell commands whose every word is
//! an allowed command, option or argument count. Anything not listed is
//! refused.

use crate::git::core::bash_classifier::lexer::{lex, Tok};
use crate::shared::path::normalize;
use std::path::{Path, PathBuf};

/// Resolves a path to the one the file system would open.
pub trait RealPaths {
    fn real(&self, path: &Path) -> PathBuf;
}

/// Claude Code's file tools expand a leading `~` to a home directory, which is
/// not known here.
fn is_from_home(path: &str) -> bool {
    path.starts_with('~')
}

/// Where a session's paths are judged from: its repository's main working
/// tree and the directory its commands run in.
pub struct Scope<'a> {
    pub root: Option<&'a str>,
    pub cwd: &'a str,
    pub paths: &'a dyn RealPaths,
}

impl Scope<'_> {
    fn absolute(&self, arg: &str) -> Option<(PathBuf, &Path)> {
        let root = Path::new(self.root?);
        let target = Path::new(self.cwd).join(arg);
        (root.is_absolute() && target.is_absolute()).then_some((target, root))
    }

    /// Whether `arg`, relative to the working directory, resolves inside the
    /// repository. Without a repository nothing is inside, and neither is a
    /// path from a home directory.
    pub fn contains(&self, arg: &str) -> bool {
        !is_from_home(arg)
            && self.absolute(arg).is_some_and(|(target, root)| {
                self.paths.real(&target).starts_with(self.paths.real(root))
            })
    }

    /// `arg` relative to the repository root, `.` for the root itself; `None`
    /// unless it is inside.
    pub fn relative(&self, arg: &str) -> Option<String> {
        if !self.contains(arg) {
            return None;
        }
        let (target, root) = self.absolute(arg)?;
        let rel = normalize(&target)
            .strip_prefix(normalize(root))
            .ok()?
            .to_string_lossy()
            .into_owned();
        Some(if rel.is_empty() { ".".to_string() } else { rel })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Compound,
    Expansion,
    /// The first word, option or argument no rule allows.
    NotAllowlisted(String),
    OutsideRepo,
}

fn push(list: &mut Vec<Refusal>, refusal: Refusal) {
    if !list.contains(&refusal) {
        list.push(refusal);
    }
}

/// A command line split into the words of each simple command, without
/// expanding anything, and why it is not one plain command.
#[derive(Debug, Default)]
pub struct ShellLine {
    pub commands: Vec<Vec<String>>,
    pub refusals: Vec<Refusal>,
}

impl ShellLine {
    pub fn is_simple(&self) -> bool {
        self.refusals.is_empty() && self.commands.len() == 1
    }
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        let mut chars = name.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// Split with the git guard's lexer, and stricter than the shell where that
/// lexer cannot tell: a `$` or backtick is an expansion even inside single
/// quotes, a leading `NAME=…` is one even when quoted, and so is any word with
/// `=~` or `:~`. A `\r` makes the line compound, since the lexer splits words
/// at it and the shell does not. A `#` comment is dropped, as the shell does.
pub fn shell_line(command: &str) -> ShellLine {
    let mut line = ShellLine::default();
    if command.contains(['$', '`']) {
        push(&mut line.refusals, Refusal::Expansion);
    }
    let toks = match lex(command, false) {
        Ok(toks) if !command.contains('\r') => toks,
        _ => {
            push(&mut line.refusals, Refusal::Compound);
            lex(command, true).unwrap_or_default()
        }
    };
    let mut argv: Vec<String> = Vec::new();
    for tok in toks {
        match tok {
            Tok::Word(word) => {
                let expands = word.dyn_at.is_some()
                    || word.tilde
                    || (argv.is_empty() && is_assignment(&word.text))
                    || word.text.contains("=~")
                    || word.text.contains(":~");
                if expands {
                    push(&mut line.refusals, Refusal::Expansion);
                }
                argv.push(word.text);
            }
            Tok::Sep(_) | Tok::Redir(_) => {
                push(&mut line.refusals, Refusal::Compound);
                if !argv.is_empty() {
                    line.commands.push(std::mem::take(&mut argv));
                }
            }
        }
    }
    if !argv.is_empty() {
        line.commands.push(argv);
    }
    line
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arg {
    Number,
    Text,
    Path,
}

/// Option names as written: `-x` short (value attached or next), `--x` long
/// (value after `=` only).
struct Spec {
    flags: &'static [&'static str],
    valued: &'static [(&'static str, Arg)],
    /// Short options whose value must be attached (`-U3`).
    attached: &'static [(&'static str, Arg)],
    /// Accepts `-<N>`.
    dash_number: bool,
    shape: Shape,
}

#[derive(Clone, Copy)]
enum Shape {
    Paths {
        min: usize,
        max: usize,
    },
    /// A pattern unless `-e` (or rg's `--files`) gives it, then paths.
    Pattern {
        required: bool,
        max_paths: usize,
    },
    /// A jq filter, then paths.
    Filter {
        max_paths: usize,
    },
    /// Revisions, then paths after `--`.
    Revs {
        max_revs: usize,
        max_paths: usize,
    },
}

const NONE: &[(&str, Arg)] = &[];

const fn paths(min: usize, max: usize) -> Shape {
    Shape::Paths { min, max }
}

const fn spec(
    flags: &'static [&'static str],
    valued: &'static [(&'static str, Arg)],
    shape: Shape,
) -> Spec {
    Spec {
        flags,
        valued,
        attached: NONE,
        dash_number: false,
        shape,
    }
}

const LS: Spec = spec(
    &[
        "-l", "-a", "-A", "-h", "-R", "-1", "-t", "-r", "-S", "-d", "-F",
    ],
    NONE,
    paths(0, 5),
);
const CAT: Spec = spec(
    &["-n", "-b", "-s", "-A", "-E", "-T", "-v"],
    NONE,
    paths(1, 5),
);
const HEAD: Spec = spec(
    &["-q", "-v"],
    &[("-n", Arg::Number), ("-c", Arg::Number)],
    paths(1, 5),
);
const WC: Spec = spec(&["-l", "-w", "-c", "-m", "-L"], NONE, paths(1, 5));
const PWD: Spec = spec(&[], NONE, paths(0, 0));
const STAT: Spec = spec(&["-L"], NONE, paths(1, 5));
const FILE: Spec = spec(&["-b", "-i", "-L"], NONE, paths(1, 5));
const DU: Spec = spec(
    &["-s", "-h", "-a", "-c"],
    &[("-d", Arg::Number), ("--max-depth", Arg::Number)],
    paths(0, 5),
);
const TREE: Spec = spec(
    &["-a", "-d", "-f", "--noreport"],
    &[("-L", Arg::Number), ("-I", Arg::Text)],
    paths(0, 1),
);
// `-R` is left out: it follows links inside the tree to files outside it.
const GREP: Spec = spec(
    &[
        "-r", "-n", "-i", "-l", "-L", "-c", "-v", "-w", "-x", "-E", "-F", "-H", "-h", "-o", "-s",
        "-q",
    ],
    &[
        ("-A", Arg::Number),
        ("-B", Arg::Number),
        ("-C", Arg::Number),
        ("-e", Arg::Text),
        ("--include", Arg::Text),
        ("--exclude", Arg::Text),
        ("--exclude-dir", Arg::Text),
    ],
    Shape::Pattern {
        required: true,
        max_paths: 5,
    },
);
const RG: Spec = spec(
    &[
        "-n",
        "-i",
        "-S",
        "-l",
        "-c",
        "-v",
        "-w",
        "-F",
        "-o",
        "-u",
        "--hidden",
        "--no-ignore",
        "--files",
    ],
    &[
        ("-e", Arg::Text),
        ("-g", Arg::Text),
        ("-t", Arg::Text),
        ("-T", Arg::Text),
        ("-A", Arg::Number),
        ("-B", Arg::Number),
        ("-C", Arg::Number),
    ],
    Shape::Pattern {
        required: false,
        max_paths: 5,
    },
);
const JQ: Spec = spec(
    &["-r", "-c", "-S", "-e"],
    NONE,
    Shape::Filter { max_paths: 5 },
);
const SORT: Spec = spec(
    &["-n", "-r", "-u", "-f", "-h", "-V"],
    &[("-k", Arg::Text), ("-t", Arg::Text)],
    paths(0, 5),
);
const UNIQ: Spec = spec(&["-c", "-d", "-u", "-i"], NONE, paths(0, 1));

const GIT_STATUS: Spec = Spec {
    attached: &[("-u", Arg::Text)],
    ..spec(
        &["-s", "-b", "--short", "--branch", "--porcelain"],
        NONE,
        paths(0, 5),
    )
};
const GIT_LOG: Spec = Spec {
    dash_number: true,
    ..spec(
        &[
            "--oneline",
            "--graph",
            "--decorate",
            "--stat",
            "--name-only",
            "--name-status",
            "--no-color",
            "--all",
            "-p",
        ],
        &[
            ("-n", Arg::Number),
            ("--format", Arg::Text),
            ("--pretty", Arg::Text),
            ("--since", Arg::Text),
            ("--until", Arg::Text),
            ("--author", Arg::Text),
            ("--grep", Arg::Text),
        ],
        Shape::Revs {
            max_revs: 2,
            max_paths: 5,
        },
    )
};
const GIT_DIFF: Spec = Spec {
    attached: &[("-U", Arg::Number)],
    ..spec(
        &[
            "--stat",
            "--name-only",
            "--name-status",
            "--cached",
            "--staged",
            "--no-color",
            "-w",
        ],
        NONE,
        Shape::Revs {
            max_revs: 2,
            max_paths: 5,
        },
    )
};
const GIT_SHOW: Spec = spec(
    &[
        "--stat",
        "--name-only",
        "--name-status",
        "--oneline",
        "--no-color",
    ],
    &[("--format", Arg::Text), ("--pretty", Arg::Text)],
    Shape::Revs {
        max_revs: 3,
        max_paths: 5,
    },
);
const GIT_REV_PARSE: Spec = spec(
    &[
        "--show-toplevel",
        "--git-dir",
        "--abbrev-ref",
        "--short",
        "--verify",
    ],
    NONE,
    Shape::Revs {
        max_revs: 2,
        max_paths: 0,
    },
);
const GIT_LS_FILES: Spec = spec(
    &[
        "-c",
        "-m",
        "-o",
        "-d",
        "-z",
        "--others",
        "--exclude-standard",
    ],
    NONE,
    paths(0, 5),
);
const GIT_BRANCH: Spec = spec(
    &["--list", "-a", "-r", "-v", "--show-current"],
    NONE,
    paths(0, 0),
);

fn command_spec(name: &str) -> Option<&'static Spec> {
    Some(match name {
        "ls" => &LS,
        "cat" => &CAT,
        "head" | "tail" => &HEAD,
        "wc" => &WC,
        "pwd" => &PWD,
        "stat" => &STAT,
        "file" => &FILE,
        "du" => &DU,
        "tree" => &TREE,
        "grep" => &GREP,
        "rg" => &RG,
        "jq" => &JQ,
        "sort" => &SORT,
        "uniq" => &UNIQ,
        _ => return None,
    })
}

fn git_spec(sub: &str) -> Option<&'static Spec> {
    Some(match sub {
        "status" => &GIT_STATUS,
        "log" => &GIT_LOG,
        "diff" => &GIT_DIFF,
        "show" => &GIT_SHOW,
        "rev-parse" => &GIT_REV_PARSE,
        "ls-files" => &GIT_LS_FILES,
        "branch" => &GIT_BRANCH,
        _ => return None,
    })
}

fn is_number(value: &str) -> bool {
    let digits = value.trim_start_matches(['+', '-']);
    let numeric = digits.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    !numeric.is_empty()
        && numeric.chars().all(|c| c.is_ascii_digit())
        && digits.len() - numeric.len() <= 2
}

struct Reads {
    paths: Vec<String>,
}

type Checked = Result<Reads, String>;

struct Parsed {
    /// Positionals before `--`, then after it.
    before: Vec<String>,
    after: Vec<String>,
    given: Vec<&'static str>,
    paths: Vec<String>,
}

fn value_ok(kind: Arg, value: &str, paths: &mut Vec<String>) -> bool {
    match kind {
        Arg::Number => is_number(value),
        Arg::Text => true,
        Arg::Path => {
            paths.push(value.to_string());
            true
        }
    }
}

fn find_option(list: &[(&'static str, Arg)], name: &str) -> Option<(&'static str, Arg)> {
    list.iter().copied().find(|(n, _)| *n == name)
}

fn parse(spec: &Spec, args: &[String]) -> Result<Parsed, String> {
    let mut out = Parsed {
        before: Vec::new(),
        after: Vec::new(),
        given: Vec::new(),
        paths: Vec::new(),
    };
    let mut rest = args.iter();
    let mut ended = false;
    while let Some(arg) = rest.next() {
        if ended {
            out.after.push(arg.clone());
        } else if arg == "--" {
            ended = true;
        } else if let Some(long) = arg.strip_prefix("--") {
            let (name, value) = match long.split_once('=') {
                Some((name, value)) => (format!("--{name}"), Some(value)),
                None => (arg.clone(), None),
            };
            match (value, find_option(spec.valued, &name)) {
                (None, _) if spec.flags.contains(&name.as_str()) => {
                    out.given
                        .extend(spec.flags.iter().copied().find(|f| *f == name));
                }
                (Some(value), Some((known, kind))) if value_ok(kind, value, &mut out.paths) => {
                    out.given.push(known);
                }
                _ => return Err(name),
            }
        } else if arg.len() > 1 && arg.starts_with('-') {
            if spec.dash_number && arg[1..].chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let shorts: Vec<char> = arg[1..].chars().collect();
            let mut i = 0;
            while i < shorts.len() {
                let name = format!("-{}", shorts[i]);
                let tail: String = shorts[i + 1..].iter().collect();
                if let Some(flag) = spec.flags.iter().find(|f| **f == name) {
                    out.given.push(flag);
                    i += 1;
                } else if let Some((known, kind)) = find_option(spec.valued, &name) {
                    let value = if tail.is_empty() {
                        rest.next().cloned().ok_or_else(|| name.clone())?
                    } else {
                        tail
                    };
                    if !value_ok(kind, &value, &mut out.paths) {
                        return Err(name);
                    }
                    out.given.push(known);
                    break;
                } else if let Some((known, kind)) = find_option(spec.attached, &name) {
                    if tail.is_empty() || !value_ok(kind, &tail, &mut out.paths) {
                        return Err(name);
                    }
                    out.given.push(known);
                    break;
                } else {
                    return Err(name);
                }
            }
        } else {
            out.before.push(arg.clone());
        }
    }
    Ok(out)
}

fn at_most(list: Vec<String>, max: usize) -> Result<Vec<String>, String> {
    match list.get(max) {
        Some(extra) => Err(extra.clone()),
        None => Ok(list),
    }
}

fn check_spec(name: &str, spec: &Spec, args: &[String]) -> Checked {
    let parsed = parse(spec, args)?;
    let mut paths = parsed.paths;
    let given_pattern = parsed.given.iter().any(|g| *g == "-e" || *g == "--files");
    if let Shape::Revs {
        max_revs,
        max_paths,
    } = spec.shape
    {
        paths.extend(at_most(parsed.before, max_revs)?);
        paths.extend(at_most(parsed.after, max_paths)?);
        return Ok(Reads { paths });
    }
    let positionals: Vec<String> = parsed.before.into_iter().chain(parsed.after).collect();
    match spec.shape {
        Shape::Paths { min, max } => {
            if positionals.len() < min {
                return Err(name.to_string());
            }
            paths.extend(at_most(positionals, max)?);
        }
        Shape::Pattern {
            required,
            max_paths,
        } => {
            let mut rest = positionals.into_iter();
            if !given_pattern && rest.next().is_none() && required {
                return Err(name.to_string());
            }
            paths.extend(at_most(rest.collect(), max_paths)?);
        }
        Shape::Filter { max_paths } => {
            let mut rest = positionals.into_iter();
            let filter = rest.next().ok_or_else(|| name.to_string())?;
            check_jq_filter(&filter)?;
            paths.extend(at_most(rest.collect(), max_paths)?);
        }
        Shape::Revs { .. } => {}
    }
    Ok(Reads { paths })
}

/// The names a jq filter may call: keywords, literals and builtins that only
/// compute on their input.
const JQ_NAMES: &[&str] = &[
    "if",
    "then",
    "elif",
    "else",
    "end",
    "and",
    "or",
    "not",
    "try",
    "catch",
    "true",
    "false",
    "null",
    "length",
    "utf8bytelength",
    "keys",
    "keys_unsorted",
    "values",
    "has",
    "in",
    "map",
    "map_values",
    "path",
    "paths",
    "leaf_paths",
    "del",
    "to_entries",
    "from_entries",
    "with_entries",
    "select",
    "empty",
    "error",
    "add",
    "any",
    "all",
    "flatten",
    "range",
    "floor",
    "ceil",
    "round",
    "sqrt",
    "fabs",
    "abs",
    "tostring",
    "tonumber",
    "type",
    "sort",
    "sort_by",
    "group_by",
    "min",
    "max",
    "min_by",
    "max_by",
    "unique",
    "unique_by",
    "reverse",
    "contains",
    "inside",
    "startswith",
    "endswith",
    "ltrimstr",
    "rtrimstr",
    "trim",
    "ltrim",
    "rtrim",
    "explode",
    "implode",
    "split",
    "join",
    "ascii_downcase",
    "ascii_upcase",
    "test",
    "match",
    "capture",
    "scan",
    "splits",
    "sub",
    "gsub",
    "recurse",
    "tojson",
    "fromjson",
    "first",
    "last",
    "nth",
    "limit",
    "until",
    "while",
    "repeat",
    "indices",
    "index",
    "rindex",
    "isempty",
    "objects",
    "arrays",
    "strings",
    "numbers",
    "booleans",
    "nulls",
    "iterables",
    "scalars",
    "getpath",
    "tostream",
    "fromstream",
    "truncate_stream",
    "transpose",
    "walk",
    "todate",
    "fromdate",
];

/// Every bare name in a jq filter must be one of `JQ_NAMES`; a field (`.name`),
/// a variable (`$name`) or a format (`@name`) is not a call. A `#` outside a
/// string is refused: jq reads it as a comment, which can hide a quote that
/// would desync this scan, and jq 1.7.1 continues a comment past a `\` newline.
fn check_jq_filter(filter: &str) -> Result<(), String> {
    let chars: Vec<char> = filter.chars().collect();
    // Paren depth inside each open `\(` interpolation, innermost last.
    let mut interpolations: Vec<usize> = Vec::new();
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            match c {
                '"' => in_string = false,
                '\\' if chars.get(i + 1) == Some(&'(') => {
                    interpolations.push(0);
                    in_string = false;
                    i += 1;
                }
                '\\' => i += 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '#' => return Err("#".to_string()),
            '(' => {
                if let Some(depth) = interpolations.last_mut() {
                    *depth += 1;
                }
            }
            ')' => match interpolations.last_mut() {
                Some(0) => {
                    interpolations.pop();
                    in_string = true;
                }
                Some(depth) => *depth -= 1,
                None => {}
            },
            c if c.is_ascii_alphanumeric() || c == '_' => {
                let start = i;
                while chars
                    .get(i + 1)
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_')
                {
                    i += 1;
                }
                let word: String = chars[start..=i].iter().collect();
                let called = !c.is_ascii_digit()
                    && !matches!(
                        start.checked_sub(1).map(|p| chars[p]),
                        Some('.' | '$' | '@')
                    );
                if called && !JQ_NAMES.contains(&word.as_str()) {
                    return Err(word);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

const FIND_FLAGS: [&str; 13] = [
    "-empty", "-print", "-print0", "-ls", "-prune", "-not", "!", "-o", "-a", "-or", "-and", "(",
    ")",
];

fn find_valued(name: &str) -> Option<Arg> {
    match name {
        "-name" | "-iname" | "-path" | "-ipath" | "-type" | "-mtime" | "-mmin" | "-size" => {
            Some(Arg::Text)
        }
        "-maxdepth" | "-mindepth" => Some(Arg::Number),
        "-newer" => Some(Arg::Path),
        _ => None,
    }
}

fn check_find(args: &[String]) -> Checked {
    let starts = args
        .iter()
        .take_while(|a| !a.starts_with('-') && !FIND_FLAGS.contains(&a.as_str()))
        .count();
    let mut paths = at_most(args[..starts].to_vec(), 3)?;
    let mut rest = args[starts..].iter();
    while let Some(arg) = rest.next() {
        if FIND_FLAGS.contains(&arg.as_str()) {
            continue;
        }
        let kind = find_valued(arg).ok_or_else(|| arg.clone())?;
        let value = rest.next().ok_or_else(|| arg.clone())?;
        if !value_ok(kind, value, &mut paths) {
            return Err(arg.clone());
        }
    }
    Ok(Reads { paths })
}

fn check_git(args: &[String]) -> Checked {
    let sub = args.first().ok_or_else(|| "git".to_string())?;
    let spec = git_spec(sub).ok_or_else(|| sub.clone())?;
    check_spec(sub, spec, &args[1..])
}

/// `Err` names the first word no rule allows.
fn check_command(argv: &[String]) -> Checked {
    let (name, args) = argv.split_first().ok_or_else(String::new)?;
    match name.as_str() {
        "git" => check_git(args),
        "find" => check_find(args),
        other => match command_spec(other) {
            Some(spec) => check_spec(other, spec, args),
            None => Err(other.to_string()),
        },
    }
}

fn check_bash(command: &str, scope: &Scope) -> Vec<Refusal> {
    let line = shell_line(command);
    let mut refusals = line.refusals;
    if line.commands.is_empty() {
        push(&mut refusals, Refusal::NotAllowlisted("Bash".to_string()));
    }
    if !scope.contains("") {
        push(&mut refusals, Refusal::OutsideRepo);
    }
    for argv in &line.commands {
        match check_command(argv) {
            Ok(reads) => {
                if reads.paths.iter().any(|p| !scope.contains(p)) {
                    push(&mut refusals, Refusal::OutsideRepo);
                }
            }
            Err(word) => push(&mut refusals, Refusal::NotAllowlisted(word)),
        }
    }
    refusals
}

fn glob_base(pattern: &str) -> &str {
    let literal = pattern
        .find(['*', '?', '[', '{'])
        .map_or(pattern, |at| &pattern[..at]);
    match literal.rfind('/') {
        Some(0) => "/",
        Some(at) => &literal[..at],
        None if literal.len() == pattern.len() => pattern,
        None => "",
    }
}

pub fn field<'a>(input: &'a [(String, String)], name: &str) -> Option<&'a str> {
    input
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

/// Why a permission request for `tool` with `input` is not a read inside the
/// repository; empty when it is.
pub fn check_permission(tool: &str, input: &[(String, String)], scope: &Scope) -> Vec<Refusal> {
    let not_listed = || vec![Refusal::NotAllowlisted(tool.to_string())];
    let inside = |paths: &[String]| -> Vec<Refusal> {
        if paths.iter().all(|p| scope.contains(p)) {
            Vec::new()
        } else {
            vec![Refusal::OutsideRepo]
        }
    };
    match tool {
        "Bash" => match field(input, "command") {
            Some(command) => check_bash(command, scope),
            None => not_listed(),
        },
        "Read" => match field(input, "file_path") {
            Some(path) => inside(&[path.to_string()]),
            None => not_listed(),
        },
        "Glob" => {
            let base = field(input, "path").unwrap_or("");
            let pattern = field(input, "pattern").unwrap_or("");
            let climbs = pattern.split(['/', '{', ',', '}']).any(|part| part == "..");
            if climbs || is_from_home(pattern) {
                return vec![Refusal::OutsideRepo];
            }
            let under = Path::new(base).join(glob_base(pattern));
            inside(&[base.to_string(), under.to_string_lossy().into_owned()])
        }
        "Grep" => inside(&[field(input, "path").unwrap_or("").to_string()]),
        _ => not_listed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn git_revisions_are_read_as_paths_too() {
        let reads = check_command(&argv(&["git", "diff", "/etc/a", "/etc/b"])).expect("allowed");
        assert_eq!(reads.paths, argv(&["/etc/a", "/etc/b"]));
    }

    #[test]
    fn git_log_takes_a_dash_count_and_paths_after_the_separator() {
        let reads = check_command(&argv(&[
            "git",
            "log",
            "-5",
            "--oneline",
            "main",
            "--",
            "src",
        ]))
        .expect("allowed");
        assert_eq!(reads.paths, argv(&["main", "src"]));
    }

    #[test]
    fn a_grep_pattern_given_by_option_leaves_every_positional_a_path() {
        let reads = check_command(&argv(&["grep", "-e", "x", "a", "b"])).expect("allowed");
        assert_eq!(reads.paths, argv(&["a", "b"]));
        assert_eq!(
            check_command(&argv(&["grep"])).err(),
            Some("grep".to_string())
        );
    }

    #[test]
    fn numbers_take_a_sign_and_a_short_unit() {
        assert!(is_number("5"));
        assert!(is_number("+5"));
        assert!(is_number("-5"));
        assert!(is_number("1K"));
        assert!(!is_number("x"));
        assert!(!is_number("/etc"));
    }

    #[test]
    fn a_glob_base_is_the_directory_before_the_first_wildcard() {
        assert_eq!(glob_base("../**/*.rs"), "..");
        assert_eq!(glob_base("src/*.rs"), "src");
        assert_eq!(glob_base("*.rs"), "");
        assert_eq!(glob_base("/etc/*"), "/etc");
        assert_eq!(glob_base("/*"), "/");
    }

    struct Lexical;

    impl RealPaths for Lexical {
        fn real(&self, path: &Path) -> PathBuf {
            normalize(path)
        }
    }

    fn words(command: &str) -> Vec<Vec<String>> {
        shell_line(command).commands
    }

    #[test]
    fn quotes_and_escapes_join_into_one_word() {
        assert_eq!(
            words(r#"grep -e 'a b' "c\"d" e\ f"#),
            vec![vec!["grep", "-e", "a b", "c\"d", "e f"]]
        );
        assert!(shell_line("grep 'a|b' x").is_simple());
    }

    #[test]
    fn operators_split_commands_and_mark_the_line_compound() {
        let line = shell_line("cat a && rm b");
        assert_eq!(line.commands, vec![vec!["cat", "a"], vec!["rm", "b"]]);
        assert_eq!(line.refusals, vec![Refusal::Compound]);
        for command in ["ls 'a", "ls a\rb", "ls\nrm a", "cat <<E\nx\nE"] {
            assert!(
                shell_line(command).refusals.contains(&Refusal::Compound),
                "{command:?}"
            );
        }
    }

    #[test]
    fn expansions_are_refused_wherever_the_shell_might_expand() {
        for command in [
            "cat '$x'",
            "cat ~/a",
            "cat ~",
            "cat a=~/b",
            "ls *.rs",
            "cat {a,b}",
            "cat {1..3}",
            "X=1 ls",
            "X='1' ls",
            "'X=1' ls",
        ] {
            assert_eq!(
                shell_line(command).refusals,
                vec![Refusal::Expansion],
                "{command}"
            );
        }
    }

    #[test]
    fn literal_tildes_braces_and_quoted_globs_are_words() {
        for command in [
            "git log HEAD~3",
            "git show HEAD@{1}",
            "ls '*'",
            "cat \"~/a\"",
            "ls a # ; rm b",
        ] {
            assert!(shell_line(command).is_simple(), "{command}");
        }
    }

    #[test]
    fn a_jq_filter_calls_only_listed_names() {
        assert_eq!(check_jq_filter(".a | keys"), Ok(()));
        assert_eq!(check_jq_filter(".env | .input"), Ok(()));
        assert_eq!(check_jq_filter("\"input \\(.a)\""), Ok(()));
        assert_eq!(check_jq_filter("\"\\(env)\""), Err("env".to_string()));
        assert_eq!(
            check_jq_filter("\"\\(\"\\(input)\")\""),
            Err("input".to_string())
        );
        assert_eq!(check_jq_filter("def f: 1; f"), Err("def".to_string()));
    }

    #[test]
    fn a_scope_without_a_repository_contains_nothing() {
        let scope = Scope {
            root: None,
            cwd: "/w",
            paths: &Lexical,
        };
        assert!(!scope.contains("a"));
        let scope = Scope {
            root: Some("/w"),
            cwd: "/w/sub",
            paths: &Lexical,
        };
        assert_eq!(scope.relative("a/b"), Some("sub/a/b".to_string()));
        assert_eq!(scope.relative(".."), Some(".".to_string()));
        assert_eq!(scope.relative("../.."), None);
    }
}

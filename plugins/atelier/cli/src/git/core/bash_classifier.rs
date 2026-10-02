//! Pure Bash command classifier. Turns a command line into the facts a guard
//! needs — hook-bypass flags, commits, file writes, opaque executions — and
//! the directory each fact is anchored to. No git, filesystem or environment
//! access: the cwd and HOME arrive through `ClassifyInput`.

use std::path::{Component, Path, PathBuf};

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

const MAX_WRAPPER_DEPTH: usize = 2;

const RESERVED: &[&str] = &[
    "{", "}", "!", "if", "then", "else", "elif", "fi", "do", "done", "while", "until",
];

const HOOK_SUBCOMMANDS: &[&str] = &[
    "commit",
    "push",
    "merge",
    "am",
    "rebase",
    "pull",
    "cherry-pick",
    "revert",
];

const GIT_VALUED_LONG: &[&str] = &[
    "message",
    "file",
    "author",
    "date",
    "reuse-message",
    "reedit-message",
    "fixup",
    "squash",
    "template",
    "cleanup",
    "trailer",
    "pathspec-from-file",
    "repo",
    "receive-pack",
    "exec",
    "push-option",
    "strategy",
    "strategy-option",
    "onto",
];

type Env = Vec<(String, String)>;

// ---- words and tokens ---------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Word {
    text: String,
    quoted: bool,
    /// Byte index in `text` of the first expansion or glob character.
    dyn_at: Option<usize>,
    tilde: bool,
    /// Bodies of the `$(…)` and backtick substitutions inside the word.
    subs: Vec<String>,
}

impl Word {
    fn literal(text: &str) -> Self {
        Word {
            text: text.to_string(),
            ..Word::default()
        }
    }

    fn stdin_placeholder() -> Self {
        Word {
            text: "<stdin>".to_string(),
            dyn_at: Some(0),
            ..Word::default()
        }
    }

    fn push_lit(&mut self, c: char) {
        self.text.push(c);
    }

    fn push_dyn(&mut self, c: char) {
        if self.dyn_at.is_none() {
            self.dyn_at = Some(self.text.len());
        }
        self.text.push(c);
    }

    fn is(&self, s: &str) -> bool {
        self.text == s
    }

    fn is_flag(&self) -> bool {
        self.text.len() > 1 && self.text.starts_with('-')
    }

    fn slice_from(&self, offset: usize) -> Word {
        Word {
            text: self.text[offset..].to_string(),
            quoted: self.quoted,
            dyn_at: self.dyn_at.and_then(|i| i.checked_sub(offset)),
            tilde: false,
            subs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sep {
    Semi,
    And,
    Or,
    Pipe,
    Amp,
    Newline,
    LParen,
    RParen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RedirOp {
    Out,
    In,
    HereString,
    HereDoc,
    Dup,
}

impl RedirOp {
    fn takes_operand(self) -> bool {
        matches!(self, RedirOp::Out | RedirOp::In | RedirOp::HereString)
    }
}

#[derive(Debug, Clone)]
enum Tok {
    Word(Word),
    Sep(Sep),
    Redir(RedirOp),
}

// ---- lexer ----------------------------------------------------------------

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    toks: Vec<Tok>,
    heredocs: Vec<(String, bool)>,
}

fn lex(src: &str) -> Result<Vec<Tok>, LexError> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        toks: Vec::new(),
        heredocs: Vec::new(),
    };
    while let Some(c) = lx.peek() {
        match c {
            ' ' | '\t' | '\r' => lx.pos += 1,
            '\\' if lx.peek_at(1) == Some('\n') => lx.pos += 2,
            '\n' => {
                lx.pos += 1;
                lx.toks.push(Tok::Sep(Sep::Newline));
                lx.skip_heredoc_bodies();
            }
            '#' => {
                while lx.peek().is_some_and(|c| c != '\n') {
                    lx.pos += 1;
                }
            }
            ';' => {
                lx.pos += 1;
                if lx.peek() == Some(';') {
                    lx.pos += 1;
                }
                lx.toks.push(Tok::Sep(Sep::Semi));
            }
            '&' => lx.ampersand(),
            '|' => lx.pipe(),
            '(' => {
                lx.pos += 1;
                lx.toks.push(Tok::Sep(Sep::LParen));
            }
            ')' => {
                lx.pos += 1;
                lx.toks.push(Tok::Sep(Sep::RParen));
            }
            '<' | '>' => lx.redirect()?,
            _ => {
                let word = lx.read_word()?;
                lx.push_word(word);
            }
        }
    }
    Ok(lx.toks)
}

fn is_word_end(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
    )
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn at(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(i, c)| self.peek_at(i) == Some(c))
    }

    fn push_word(&mut self, word: Word) {
        if word.text.is_empty() && !word.quoted {
            return;
        }
        let is_fd_prefix = !word.quoted
            && word.text.chars().all(|c| c.is_ascii_digit())
            && matches!(self.peek(), Some('<' | '>'));
        if !is_fd_prefix {
            self.toks.push(Tok::Word(word));
        }
    }

    fn ampersand(&mut self) {
        match self.peek_at(1) {
            Some('>') => {
                self.pos += 2;
                if self.peek() == Some('>') {
                    self.pos += 1;
                }
                self.toks.push(Tok::Redir(RedirOp::Out));
            }
            Some('&') => {
                self.pos += 2;
                self.toks.push(Tok::Sep(Sep::And));
            }
            _ => {
                self.pos += 1;
                self.toks.push(Tok::Sep(Sep::Amp));
            }
        }
    }

    fn pipe(&mut self) {
        match self.peek_at(1) {
            Some('|') => {
                self.pos += 2;
                self.toks.push(Tok::Sep(Sep::Or));
            }
            Some('&') => {
                self.pos += 2;
                self.toks.push(Tok::Sep(Sep::Pipe));
            }
            _ => {
                self.pos += 1;
                self.toks.push(Tok::Sep(Sep::Pipe));
            }
        }
    }

    /// Consumes `&` plus an fd number or `-` when present. Returns false and
    /// leaves the position after the `&` when the target is a file name.
    fn dup_target(&mut self) -> bool {
        self.pos += 1;
        if !matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
            return false;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
            self.pos += 1;
        }
        true
    }

    fn redirect(&mut self) -> Result<(), LexError> {
        let c = self.chars[self.pos];
        if self.peek_at(1) == Some('(') {
            let mut word = Word::default();
            word.push_dyn(c);
            self.pos += 1;
            self.scan_parens(&mut word)?;
            self.toks.push(Tok::Word(word));
            return Ok(());
        }
        if c == '>' {
            self.pos += 1;
            match self.peek() {
                Some('>' | '|') => self.pos += 1,
                Some('&') => {
                    if self.dup_target() {
                        self.toks.push(Tok::Redir(RedirOp::Dup));
                        return Ok(());
                    }
                }
                _ => {}
            }
            self.toks.push(Tok::Redir(RedirOp::Out));
        } else if self.at("<<<") {
            self.pos += 3;
            self.toks.push(Tok::Redir(RedirOp::HereString));
        } else if self.at("<<") {
            self.pos += 2;
            let strip_tabs = self.peek() == Some('-');
            if strip_tabs {
                self.pos += 1;
            }
            self.toks.push(Tok::Redir(RedirOp::HereDoc));
            self.read_heredoc_delimiter(strip_tabs)?;
        } else if self.at("<&") {
            self.pos += 1;
            if self.dup_target() {
                self.toks.push(Tok::Redir(RedirOp::Dup));
            } else {
                self.toks.push(Tok::Redir(RedirOp::In));
            }
        } else if self.at("<>") {
            self.pos += 2;
            self.toks.push(Tok::Redir(RedirOp::Out));
        } else {
            self.pos += 1;
            self.toks.push(Tok::Redir(RedirOp::In));
        }
        Ok(())
    }

    fn read_heredoc_delimiter(&mut self, strip_tabs: bool) -> Result<(), LexError> {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.pos += 1;
        }
        if self.peek().is_none_or(is_word_end) {
            return Ok(());
        }
        let word = self.read_word()?;
        self.heredocs.push((word.text, strip_tabs));
        Ok(())
    }

    fn skip_heredoc_bodies(&mut self) {
        for (delimiter, strip_tabs) in std::mem::take(&mut self.heredocs) {
            while self.pos < self.chars.len() {
                let start = self.pos;
                while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                    self.pos += 1;
                }
                let line: String = self.chars[start..self.pos].iter().collect();
                if self.pos < self.chars.len() {
                    self.pos += 1;
                }
                let line = if strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                if line.trim_end_matches('\r') == delimiter {
                    break;
                }
            }
        }
    }

    fn read_word(&mut self) -> Result<Word, LexError> {
        let mut w = Word::default();
        while let Some(c) = self.peek() {
            match c {
                c if is_word_end(c) => break,
                '\\' => {
                    self.pos += 1;
                    match self.next() {
                        None | Some('\n') => {}
                        Some(escaped) => {
                            w.push_lit(escaped);
                            w.quoted = true;
                        }
                    }
                }
                '\'' => {
                    self.pos += 1;
                    w.quoted = true;
                    loop {
                        match self.next() {
                            None => return Err(LexError::UnterminatedQuote),
                            Some('\'') => break,
                            Some(ch) => w.push_lit(ch),
                        }
                    }
                }
                '"' => {
                    self.pos += 1;
                    w.quoted = true;
                    self.read_double_quoted(&mut w)?;
                }
                '$' => self.read_dollar(&mut w, false)?,
                '`' => self.read_backtick(&mut w)?,
                '*' | '?' | '[' => {
                    w.push_dyn(c);
                    self.pos += 1;
                }
                '{' => {
                    if self.brace_expands() {
                        w.push_dyn(c);
                    } else {
                        w.push_lit(c);
                    }
                    self.pos += 1;
                }
                '~' if w.text.is_empty() && !w.quoted => {
                    w.tilde = true;
                    w.push_lit(c);
                    self.pos += 1;
                }
                _ => {
                    w.push_lit(c);
                    self.pos += 1;
                }
            }
        }
        Ok(w)
    }

    fn brace_expands(&self) -> bool {
        let mut saw_separator = false;
        let mut j = self.pos + 1;
        while let Some(&c) = self.chars.get(j) {
            if c == '}' {
                return saw_separator;
            }
            if c.is_whitespace() || is_word_end(c) {
                return false;
            }
            if c == ',' || (c == '.' && self.chars.get(j + 1) == Some(&'.')) {
                saw_separator = true;
            }
            j += 1;
        }
        false
    }

    fn read_double_quoted(&mut self, w: &mut Word) -> Result<(), LexError> {
        loop {
            match self.peek() {
                None => return Err(LexError::UnterminatedQuote),
                Some('"') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.next() {
                        None => return Err(LexError::UnterminatedQuote),
                        Some('\n') => {}
                        Some(n @ ('$' | '`' | '"' | '\\')) => w.push_lit(n),
                        Some(n) => {
                            w.push_lit('\\');
                            w.push_lit(n);
                        }
                    }
                }
                Some('$') => self.read_dollar(w, true)?,
                Some('`') => self.read_backtick(w)?,
                Some(c) => {
                    w.push_lit(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn read_dollar(&mut self, w: &mut Word, in_double_quotes: bool) -> Result<(), LexError> {
        match self.peek_at(1) {
            Some('(') => {
                let arithmetic = self.peek_at(2) == Some('(');
                w.push_dyn('$');
                self.pos += 1;
                let open = w.text.len();
                self.scan_parens(w)?;
                if !arithmetic {
                    let body = w.text[open + 1..w.text.len() - 1].to_string();
                    w.subs.push(body);
                }
                Ok(())
            }
            Some('{') => {
                w.push_dyn('$');
                self.pos += 1;
                loop {
                    match self.next() {
                        None => return Err(LexError::UnterminatedSubstitution),
                        Some(c) => {
                            w.text.push(c);
                            if c == '}' {
                                return Ok(());
                            }
                        }
                    }
                }
            }
            Some('\'') if !in_double_quotes => {
                self.pos += 2;
                w.quoted = true;
                self.read_ansi_c(w)
            }
            Some(n) if n.is_alphanumeric() || "_?$@*#!-".contains(n) => {
                w.push_dyn('$');
                self.pos += 1;
                Ok(())
            }
            _ => {
                w.push_lit('$');
                self.pos += 1;
                Ok(())
            }
        }
    }

    /// Copies a balanced `( … )` group verbatim, starting at the `(`.
    fn scan_parens(&mut self, w: &mut Word) -> Result<(), LexError> {
        let mut depth = 0usize;
        let mut pending_heredocs: Vec<(String, bool)> = Vec::new();
        loop {
            let Some(c) = self.next() else {
                return Err(LexError::UnterminatedSubstitution);
            };
            w.text.push(c);
            match c {
                '<' if self.peek() == Some('<') && self.peek_at(1) != Some('<') => {
                    self.copy_heredoc_opener(w, &mut pending_heredocs);
                }
                '\n' if !pending_heredocs.is_empty() => {
                    for (delimiter, strip_tabs) in std::mem::take(&mut pending_heredocs) {
                        self.copy_heredoc_body(w, &delimiter, strip_tabs);
                    }
                }
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                '\\' => {
                    if let Some(n) = self.next() {
                        w.text.push(n);
                    }
                }
                '\'' | '"' => loop {
                    match self.next() {
                        None => return Err(LexError::UnterminatedSubstitution),
                        Some(q) => {
                            w.text.push(q);
                            if q == '\\' && c == '"' {
                                if let Some(n) = self.next() {
                                    w.text.push(n);
                                }
                            } else if q == c {
                                break;
                            }
                        }
                    }
                },
                _ => {}
            }
        }
    }

    /// Copies the rest of a `<<[-]DELIM` opener (the first `<` is already in
    /// `w`) and records its delimiter so the body can be copied unscanned.
    fn copy_heredoc_opener(&mut self, w: &mut Word, pending: &mut Vec<(String, bool)>) {
        w.text.push('<');
        self.pos += 1;
        let strip_tabs = self.peek() == Some('-');
        if strip_tabs {
            w.text.push('-');
            self.pos += 1;
        }
        while let Some(c @ (' ' | '\t')) = self.peek() {
            w.text.push(c);
            self.pos += 1;
        }
        let mut delimiter = String::new();
        while let Some(c) = self.peek().filter(|c| !is_word_end(*c)) {
            w.text.push(c);
            self.pos += 1;
            if !matches!(c, '\'' | '"' | '\\') {
                delimiter.push(c);
            }
        }
        if !delimiter.is_empty() {
            pending.push((delimiter, strip_tabs));
        }
    }

    /// Copies heredoc body lines verbatim through the delimiter line, so quotes
    /// and parentheses inside the body never reach the paren scanner.
    fn copy_heredoc_body(&mut self, w: &mut Word, delimiter: &str, strip_tabs: bool) {
        while self.pos < self.chars.len() {
            let start = self.pos;
            while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                self.pos += 1;
            }
            let line: String = self.chars[start..self.pos].iter().collect();
            w.text.push_str(&line);
            if self.pos < self.chars.len() {
                self.pos += 1;
                w.text.push('\n');
            }
            let line = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line.as_str()
            };
            if line.trim_end_matches('\r') == delimiter {
                break;
            }
        }
    }

    fn read_backtick(&mut self, w: &mut Word) -> Result<(), LexError> {
        w.push_dyn('`');
        self.pos += 1;
        let open = w.text.len();
        loop {
            let Some(c) = self.next() else {
                return Err(LexError::UnterminatedSubstitution);
            };
            w.text.push(c);
            if c == '\\' {
                if let Some(n) = self.next() {
                    w.text.push(n);
                }
            } else if c == '`' {
                w.subs
                    .push(unescape_backtick(&w.text[open..w.text.len() - 1]));
                return Ok(());
            }
        }
    }

    fn read_ansi_c(&mut self, w: &mut Word) -> Result<(), LexError> {
        loop {
            match self.next() {
                None => return Err(LexError::UnterminatedQuote),
                Some('\'') => return Ok(()),
                Some('\\') => {
                    let Some(e) = self.next() else {
                        return Err(LexError::UnterminatedQuote);
                    };
                    match e {
                        'n' => w.push_lit('\n'),
                        't' => w.push_lit('\t'),
                        'r' => w.push_lit('\r'),
                        'a' => w.push_lit('\x07'),
                        'b' => w.push_lit('\x08'),
                        'e' | 'E' => w.push_lit('\x1b'),
                        'f' => w.push_lit('\x0c'),
                        'v' => w.push_lit('\x0b'),
                        'x' => {
                            let value = self.read_digits(2, 16);
                            match value.and_then(char::from_u32) {
                                Some(ch) => w.push_lit(ch),
                                None => w.push_lit('x'),
                            }
                        }
                        '0'..='7' => {
                            self.pos -= 1;
                            match self.read_digits(3, 8).and_then(char::from_u32) {
                                Some(ch) => w.push_lit(ch),
                                None => w.push_lit(e),
                            }
                        }
                        other => w.push_lit(other),
                    }
                }
                Some(ch) => w.push_lit(ch),
            }
        }
    }

    fn read_digits(&mut self, max: usize, radix: u32) -> Option<u32> {
        let mut value = 0u32;
        let mut count = 0;
        while count < max {
            let Some(d) = self.peek().and_then(|c| c.to_digit(radix)) else {
                break;
            };
            value = value * radix + d;
            self.pos += 1;
            count += 1;
        }
        (count > 0).then_some(value)
    }
}

// ---- places and path resolution ------------------------------------------

#[derive(Debug, Clone)]
enum Place {
    Known(PathBuf),
    Dynamic {
        literal_prefix: Option<PathBuf>,
        raw: String,
    },
}

impl Place {
    fn unknown(raw: &str) -> Place {
        Place::Dynamic {
            literal_prefix: None,
            raw: raw.to_string(),
        }
    }

    fn to_anchor(&self) -> Anchor {
        match self {
            Place::Known(p) => Anchor::Path(p.clone()),
            Place::Dynamic {
                literal_prefix,
                raw,
            } => Anchor::Unresolved {
                literal_prefix: literal_prefix.clone(),
                raw: raw.clone(),
            },
        }
    }
}

fn unescape_backtick(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('\\', Some('`' | '\\' | '$')) => {
                out.extend(chars.next());
            }
            _ => out.push(c),
        }
    }
    out
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn is_device(path: &Path) -> bool {
    path.starts_with("/dev/fd")
        || matches!(
            path.to_str(),
            Some("/dev/null" | "/dev/stdin" | "/dev/stdout" | "/dev/stderr" | "/dev/tty")
        )
}

fn program_name(w: &Word) -> &str {
    w.text.rsplit('/').next().unwrap_or("")
}

fn assignment(w: &Word) -> Option<(String, String)> {
    let (name, value) = w.text.split_once('=')?;
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

/// State a segment leaves for the following segments of the same shell.
enum Effect {
    Cd(Place),
    Env(Vec<(String, String)>),
}

fn set_env(env: &mut Env, (name, value): (String, String)) {
    env.retain(|(n, _)| *n != name);
    env.push((name, value));
}

fn assignments(args: &[Word]) -> Vec<(String, String)> {
    args.iter().filter_map(assignment).collect()
}

fn exports(args: &[Word]) -> bool {
    args.iter()
        .take_while(|w| w.is_flag())
        .any(|w| w.text[1..].contains('x'))
}

fn split_eq(s: &str) -> (&str, bool) {
    match s.split_once('=') {
        Some((name, _)) => (name, true),
        None => (s, false),
    }
}

/// Value of a short option inside a bundle: the attached rest, else the next
/// argument (reported as one extra consumed word).
fn short_value(w: &Word, after: usize, next: Option<&Word>) -> (Option<Word>, usize) {
    if after < w.text.len() {
        (Some(w.slice_from(after)), 0)
    } else {
        (next.cloned(), 1)
    }
}

#[derive(Default)]
struct Parsed {
    positionals: Vec<Word>,
    opts: Vec<(String, Option<Word>)>,
}

impl Parsed {
    fn value(&self, names: &[&str]) -> Option<&Word> {
        self.opts
            .iter()
            .find(|(n, _)| names.contains(&n.as_str()))
            .and_then(|(_, v)| v.as_ref())
    }

    fn values(&self, names: &[&str]) -> Vec<&Word> {
        self.opts
            .iter()
            .filter(|(n, _)| names.contains(&n.as_str()))
            .filter_map(|(_, v)| v.as_ref())
            .collect()
    }

    fn has(&self, name: &str) -> bool {
        self.opts.iter().any(|(n, _)| n == name)
    }
}

fn parse_args(args: &[Word], short_valued: &str, long_valued: &[&str]) -> Parsed {
    let mut parsed = Parsed::default();
    let mut i = 0;
    let mut options_ended = false;
    while i < args.len() {
        let w = &args[i];
        i += 1;
        if options_ended || !w.is_flag() {
            parsed.positionals.push(w.clone());
            continue;
        }
        if w.is("--") {
            options_ended = true;
            continue;
        }
        if let Some(long) = w.text.strip_prefix("--") {
            match long.split_once('=') {
                Some((name, _)) => parsed
                    .opts
                    .push((name.to_string(), Some(w.slice_from(2 + name.len() + 1)))),
                None if long_valued.contains(&long) => {
                    parsed.opts.push((long.to_string(), args.get(i).cloned()));
                    i += 1;
                }
                None => parsed.opts.push((long.to_string(), None)),
            }
            continue;
        }
        for (ci, ch) in w.text[1..].char_indices() {
            if short_valued.contains(ch) {
                let (value, extra) = short_value(w, 1 + ci + ch.len_utf8(), args.get(i));
                parsed.opts.push((ch.to_string(), value));
                i += extra;
                break;
            }
            parsed.opts.push((ch.to_string(), None));
        }
    }
    parsed
}

// ---- analyzer -----------------------------------------------------------------

#[derive(Default)]
struct Segment {
    words: Vec<Word>,
    redirs: Vec<(RedirOp, Option<Word>)>,
}

impl Segment {
    fn push_word(&mut self, w: Word) {
        if let Some((op, target)) = self.redirs.last_mut() {
            if op.takes_operand() && target.is_none() {
                *target = Some(w);
                return;
            }
        }
        self.words.push(w);
    }
}

struct Analyzer<'a> {
    home: Option<&'a Path>,
    out: BashAnalysis,
}

impl Analyzer<'_> {
    fn run_script(
        &mut self,
        src: &str,
        cwd: &Place,
        env: &Env,
        depth: usize,
    ) -> Result<(), LexError> {
        let toks = lex(src)?;
        let mut cwd = cwd.clone();
        let mut env = env.clone();
        let mut saved: Vec<(Place, Env)> = Vec::new();
        let mut seg = Segment::default();
        let mut after_pipe = false;
        for tok in toks {
            match tok {
                Tok::Word(w) => seg.push_word(w),
                Tok::Redir(op) => seg.redirs.push((op, None)),
                Tok::Sep(sep) => {
                    let before_pipe = sep == Sep::Pipe;
                    let propagate = !after_pipe && !before_pipe && sep != Sep::Amp;
                    self.flush(&mut seg, &mut cwd, &mut env, depth, propagate);
                    match sep {
                        Sep::LParen => saved.push((cwd.clone(), env.clone())),
                        Sep::RParen => {
                            if let Some((outer_cwd, outer_env)) = saved.pop() {
                                cwd = outer_cwd;
                                env = outer_env;
                            }
                        }
                        _ => {}
                    }
                    after_pipe = before_pipe;
                }
            }
        }
        self.flush(&mut seg, &mut cwd, &mut env, depth, !after_pipe);
        Ok(())
    }

    fn flush(
        &mut self,
        seg: &mut Segment,
        cwd: &mut Place,
        env: &mut Env,
        depth: usize,
        propagate: bool,
    ) {
        let segment = std::mem::take(seg);
        if segment.words.is_empty() && segment.redirs.is_empty() {
            return;
        }
        let effect = self.segment(&segment, cwd, env, depth);
        if !propagate {
            return;
        }
        match effect {
            Some(Effect::Cd(next)) => *cwd = next,
            Some(Effect::Env(assigned)) => {
                for kv in assigned {
                    set_env(env, kv);
                }
            }
            None => {}
        }
    }

    fn scan_substitutions(&mut self, seg: &Segment, cwd: &Place, env: &Env, depth: usize) {
        let targets = seg.redirs.iter().filter_map(|(_, t)| t.as_ref());
        for word in seg.words.iter().chain(targets) {
            for body in &word.subs {
                if depth >= MAX_WRAPPER_DEPTH || self.run_script(body, cwd, env, depth + 1).is_err()
                {
                    self.opaque(OpaqueCause::Substitution, "$(", cwd);
                }
            }
        }
    }

    /// Returns the state change the segment leaves behind for later segments
    /// of the same shell.
    fn segment(&mut self, seg: &Segment, cwd: &Place, env: &Env, depth: usize) -> Option<Effect> {
        self.scan_substitutions(seg, cwd, env, depth);
        for (op, target) in &seg.redirs {
            if let (RedirOp::Out, Some(target)) = (op, target) {
                self.write_target(WriteRule::Redirect, ">", target, cwd);
            }
        }
        let mut env = env.clone();
        let mut assigned: Vec<(String, String)> = Vec::new();
        let mut words: &[Word] = &seg.words;
        while let Some(first) = words.first() {
            if !first.quoted && RESERVED.contains(&first.text.as_str()) {
                words = &words[1..];
            } else if let Some(kv) = assignment(first) {
                set_env(&mut env, kv.clone());
                assigned.push(kv);
                words = &words[1..];
            } else {
                break;
            }
        }
        let Some(first) = words.first() else {
            return (!assigned.is_empty()).then_some(Effect::Env(assigned));
        };
        match program_name(first) {
            "cd" => Some(Effect::Cd(self.cd_target(&words[1..], cwd))),
            name @ ("pushd" | "popd") => Some(Effect::Cd(Place::unknown(name))),
            "export" => Some(Effect::Env(assignments(&words[1..]))),
            "declare" | "typeset" if exports(&words[1..]) => {
                Some(Effect::Env(assignments(&words[1..])))
            }
            _ => {
                self.command(words, &env, cwd, depth);
                None
            }
        }
    }

    fn cd_target(&self, args: &[Word], cwd: &Place) -> Place {
        let mut rest = args;
        while let Some(w) = rest.first() {
            if w.is("--") {
                rest = &rest[1..];
                break;
            }
            if !w.is_flag() {
                break;
            }
            rest = &rest[1..];
        }
        match rest.first() {
            None => match self.home {
                Some(h) => Place::Known(normalize(h)),
                None => Place::unknown("~"),
            },
            Some(w) if w.is("-") => Place::unknown("-"),
            Some(w) => self.resolve(cwd, w),
        }
    }

    fn expand_tilde(&self, w: &Word) -> Option<(String, Option<usize>)> {
        if !w.tilde {
            return Some((w.text.clone(), w.dyn_at));
        }
        let rest = &w.text[1..];
        if !(rest.is_empty() || rest.starts_with('/')) {
            return None;
        }
        let home = self.home?.to_string_lossy().into_owned();
        let shift = home.len();
        Some((format!("{home}{rest}"), w.dyn_at.map(|i| i - 1 + shift)))
    }

    fn resolve(&self, base: &Place, w: &Word) -> Place {
        let Some((text, dyn_at)) = self.expand_tilde(w) else {
            return Place::unknown(&w.text);
        };
        let is_abs = text.starts_with('/');
        let Some(idx) = dyn_at else {
            return match base {
                _ if is_abs => Place::Known(normalize(Path::new(&text))),
                Place::Known(b) => Place::Known(normalize(&b.join(&text))),
                Place::Dynamic {
                    literal_prefix,
                    raw,
                } => Place::Dynamic {
                    literal_prefix: literal_prefix.clone(),
                    raw: format!("{raw}/{text}"),
                },
            };
        };
        let head = &text[..idx];
        let dir_part = match head.rfind('/') {
            Some(0) => "/",
            Some(p) => &head[..p],
            None => "",
        };
        if is_abs {
            return Place::Dynamic {
                literal_prefix: Some(normalize(Path::new(dir_part))),
                raw: text,
            };
        }
        if idx == 0 && matches!(text[idx..].chars().next(), Some('$' | '`' | '<' | '>')) {
            return Place::unknown(&text);
        }
        let (prefix_base, raw) = match base {
            Place::Known(b) => (Some(b.clone()), text.clone()),
            Place::Dynamic {
                literal_prefix,
                raw: base_raw,
            } => (literal_prefix.clone(), format!("{base_raw}/{text}")),
        };
        Place::Dynamic {
            literal_prefix: prefix_base.map(|p| normalize(&p.join(dir_part))),
            raw,
        }
    }

    // ---- hit emission ----

    fn push_hit(&mut self, kind: HitKind, program: &str, anchor: Anchor) {
        self.out.hits.push(Hit {
            kind,
            anchor,
            program: program.to_string(),
        });
    }

    fn push_bypass(&mut self, rule: BypassRule, token: &str) {
        self.out.bypass.push(BypassHit {
            rule,
            token: token.to_string(),
        });
    }

    fn opaque(&mut self, cause: OpaqueCause, program: &str, cwd: &Place) {
        self.push_hit(HitKind::Opaque(cause), program, cwd.to_anchor());
    }

    fn write_target(&mut self, rule: WriteRule, program: &str, target: &Word, cwd: &Place) {
        if target.text.is_empty() {
            return;
        }
        let place = self.resolve(cwd, target);
        if matches!(&place, Place::Known(p) if is_device(p)) {
            return;
        }
        self.push_hit(HitKind::Write(rule), program, place.to_anchor());
    }

    fn write_place(&mut self, rule: WriteRule, program: &str, place: &Place) {
        self.push_hit(HitKind::Write(rule), program, place.to_anchor());
    }

    // ---- command dispatch ----

    fn command(&mut self, words: &[Word], env: &Env, cwd: &Place, depth: usize) {
        let Some(prog) = words.first() else { return };
        let args = &words[1..];
        if prog.dyn_at == Some(0) && prog.text.starts_with(['$', '`']) {
            self.opaque(OpaqueCause::DynamicCommand, &prog.text, cwd);
            return;
        }
        let name = program_name(prog);
        match name {
            "env" => self.env_cmd(args, env, cwd, depth),
            "sudo" => self.sudo_cmd(args, env, cwd, depth),
            "nohup" => {
                if self.can_unwrap(Wrapper::Nohup, name, cwd, depth) {
                    self.inner(args, env, cwd, depth);
                }
            }
            "time" => self.time_cmd(args, env, cwd, depth),
            "command" => self.command_cmd(args, env, cwd, depth),
            "timeout" => self.timeout_cmd(args, env, cwd, depth),
            "xargs" => self.xargs_cmd(args, env, cwd, depth),
            "find" => self.find_cmd(args, env, cwd, depth),
            "bash" | "sh" | "zsh" | "dash" | "ksh" => self.shell_cmd(name, args, env, cwd, depth),
            "eval" => self.opaque(OpaqueCause::Eval, name, cwd),
            "source" | "." => {
                let file = args.first().map_or("", |w| w.text.as_str());
                self.opaque(OpaqueCause::ScriptFile(file.to_string()), name, cwd);
            }
            "git" => self.git_cmd(args, env, cwd),
            "sed" => self.sed_cmd(args, cwd),
            "awk" | "gawk" => self.awk_cmd(args, cwd),
            "perl" => self.perl_cmd(args, cwd),
            "rm" | "rmdir" | "touch" | "mkdir" | "truncate" | "tee" => {
                self.file_ops(name, args, cwd)
            }
            "mv" => self.mv_cmd(args, cwd),
            "cp" | "install" | "ln" => self.copy_like(name, args, cwd),
            "dd" => self.dd_cmd(args, cwd),
            "patch" => self.patch_cmd(args, cwd),
            "tar" => self.tar_cmd(args, cwd),
            "unzip" => self.unzip_cmd(args, cwd),
            "curl" => self.curl_cmd(args, cwd),
            "wget" => self.wget_cmd(args, cwd),
            "rsync" => self.rsync_cmd(args, cwd),
            _ if is_interpreter(name) => self.interpreter(name, args, cwd),
            _ if prog.text.contains('/') && !prog.text.starts_with('/') => {
                self.opaque(OpaqueCause::ScriptFile(prog.text.clone()), name, cwd)
            }
            _ => {}
        }
    }

    // ---- wrappers ----

    fn can_unwrap(&mut self, wrapper: Wrapper, name: &str, cwd: &Place, depth: usize) -> bool {
        if depth >= MAX_WRAPPER_DEPTH {
            self.opaque(OpaqueCause::Wrapper(wrapper), name, cwd);
            return false;
        }
        true
    }

    fn inner(&mut self, rest: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !rest.is_empty() {
            self.command(rest, env, cwd, depth + 1);
        }
    }

    fn env_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !self.can_unwrap(Wrapper::Env, "env", cwd, depth) {
            return;
        }
        let mut env = env.clone();
        let mut inner_cwd = cwd.clone();
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            if let Some(kv) = assignment(w) {
                env.push(kv);
                i += 1;
                continue;
            }
            if !w.is_flag() {
                break;
            }
            match w.text.as_str() {
                "-i" | "-0" | "-v" | "--ignore-environment" | "--null" | "--debug" => i += 1,
                "-u" | "--unset" => i += 2,
                "-C" | "--chdir" => {
                    let Some(dir) = args.get(i + 1) else {
                        self.opaque(OpaqueCause::Wrapper(Wrapper::Env), "env", cwd);
                        return;
                    };
                    inner_cwd = self.resolve(&inner_cwd, dir);
                    i += 2;
                }
                "--" => {
                    i += 1;
                    break;
                }
                t if t.starts_with("--chdir=") => {
                    inner_cwd = self.resolve(&inner_cwd, &w.slice_from(8));
                    i += 1;
                }
                t if t.starts_with("--unset=") || (t.starts_with("-u") && !t.starts_with("--")) => {
                    i += 1
                }
                _ => {
                    self.opaque(OpaqueCause::Wrapper(Wrapper::Env), "env", cwd);
                    return;
                }
            }
        }
        self.inner(&args[i.min(args.len())..], &env, &inner_cwd, depth);
    }

    fn sudo_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !self.can_unwrap(Wrapper::Sudo, "sudo", cwd, depth) {
            return;
        }
        let mut env = env.clone();
        let mut inner_cwd = cwd.clone();
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            if let Some(kv) = assignment(w) {
                env.push(kv);
                i += 1;
                continue;
            }
            if !w.is_flag() {
                break;
            }
            i += 1;
            if w.is("--") {
                break;
            }
            if let Some(long) = w.text.strip_prefix("--") {
                let (name, attached) = split_eq(long);
                match name {
                    "shell" | "login" => {
                        self.opaque(OpaqueCause::Wrapper(Wrapper::Sudo), "sudo", cwd);
                        return;
                    }
                    "chdir" => {
                        let dir = if attached {
                            Some(w.slice_from(2 + name.len() + 1))
                        } else {
                            i += 1;
                            args.get(i - 1).cloned()
                        };
                        if let Some(dir) = dir {
                            inner_cwd = self.resolve(&inner_cwd, &dir);
                        }
                    }
                    "user" | "group" | "host" | "prompt" | "role" | "type" | "other-user"
                    | "close-from" | "command-timeout" => {
                        if !attached {
                            i += 1;
                        }
                    }
                    _ => {}
                }
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    's' | 'i' => {
                        self.opaque(OpaqueCause::Wrapper(Wrapper::Sudo), "sudo", cwd);
                        return;
                    }
                    'D' | 'u' | 'g' | 'C' | 'h' | 'p' | 'r' | 't' | 'U' | 'T' => {
                        let (value, extra) = short_value(w, after, args.get(i));
                        let Some(value) = value else {
                            self.opaque(OpaqueCause::Wrapper(Wrapper::Sudo), "sudo", cwd);
                            return;
                        };
                        if ch == 'D' {
                            inner_cwd = self.resolve(&inner_cwd, &value);
                        }
                        i += extra;
                        break;
                    }
                    _ => {}
                }
            }
        }
        self.inner(&args[i.min(args.len())..], &env, &inner_cwd, depth);
    }

    fn time_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !self.can_unwrap(Wrapper::Time, "time", cwd, depth) {
            return;
        }
        let mut i = 0;
        while let Some(w) = args.get(i) {
            if !w.is_flag() {
                break;
            }
            i += if matches!(w.text.as_str(), "-f" | "-o") {
                2
            } else {
                1
            };
        }
        self.inner(&args[i.min(args.len())..], env, cwd, depth);
    }

    fn command_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !self.can_unwrap(Wrapper::Command, "command", cwd, depth) {
            return;
        }
        let mut i = 0;
        while let Some(w) = args.get(i) {
            if !w.is_flag() {
                break;
            }
            if w.text.contains(['v', 'V']) {
                return;
            }
            i += 1;
        }
        self.inner(&args[i..], env, cwd, depth);
    }

    fn timeout_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !self.can_unwrap(Wrapper::Timeout, "timeout", cwd, depth) {
            return;
        }
        let mut i = 0;
        while let Some(w) = args.get(i) {
            if !w.is_flag() {
                break;
            }
            if w.is("--") {
                i += 1;
                break;
            }
            i += match w.text.as_str() {
                "-s" | "-k" | "--signal" | "--kill-after" => 2,
                "-v" | "--verbose" | "--foreground" | "--preserve-status" => 1,
                t if t.starts_with("--signal=") || t.starts_with("--kill-after=") => 1,
                t if t.starts_with("-s") || t.starts_with("-k") => 1,
                _ => {
                    self.opaque(OpaqueCause::Wrapper(Wrapper::Timeout), "timeout", cwd);
                    return;
                }
            };
        }
        let Some(duration) = args.get(i) else { return };
        if !is_duration(&duration.text) {
            self.opaque(OpaqueCause::Wrapper(Wrapper::Timeout), "timeout", cwd);
            return;
        }
        self.inner(&args[i + 1..], env, cwd, depth);
    }

    fn xargs_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !self.can_unwrap(Wrapper::Xargs, "xargs", cwd, depth) {
            return;
        }
        let mut replace: Option<String> = None;
        let mut i = 0;
        while let Some(w) = args.get(i) {
            if !w.is_flag() {
                break;
            }
            i += 1;
            if w.is("--") {
                break;
            }
            if let Some(long) = w.text.strip_prefix("--") {
                let (name, attached) = split_eq(long);
                match name {
                    "replace" => {
                        let marker = if attached {
                            &long[name.len() + 1..]
                        } else {
                            "{}"
                        };
                        replace = Some(marker.to_string());
                    }
                    "max-args" | "max-procs" | "max-lines" | "max-chars" | "delimiter"
                    | "arg-file" | "eof" | "process-slot-var" => {
                        if !attached {
                            i += 1;
                        }
                    }
                    _ => {}
                }
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'I' | 'n' | 'L' | 'P' | 's' | 'd' | 'E' | 'a' | 'J' | 'S' => {
                        let (value, extra) = short_value(w, after, args.get(i));
                        if ch == 'I' {
                            replace = value.map(|v| v.text);
                        }
                        i += extra;
                        break;
                    }
                    'i' => {
                        let marker = if after < w.text.len() {
                            &w.text[after..]
                        } else {
                            "{}"
                        };
                        replace = Some(marker.to_string());
                        break;
                    }
                    'l' | 'e' => break,
                    _ => {}
                }
            }
        }
        let rest = &args[i.min(args.len())..];
        if rest.is_empty() {
            return;
        }
        let command: Vec<Word> = match replace.as_deref() {
            Some(marker) if !marker.is_empty() => rest
                .iter()
                .map(|w| substitute_stdin_marker(w, marker))
                .collect(),
            Some(_) => rest.to_vec(),
            None => {
                let mut command = rest.to_vec();
                command.push(Word::stdin_placeholder());
                command
            }
        };
        self.command(&command, env, cwd, depth + 1);
    }

    fn find_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        let start = args
            .iter()
            .position(|w| !(w.is("-H") || w.is("-L") || w.is("-P")))
            .unwrap_or(args.len());
        let split = args[start..]
            .iter()
            .position(|w| w.is_flag() || w.is("(") || w.is("!") || w.is(","))
            .map_or(args.len(), |p| start + p);
        let mut roots = args[start..split].to_vec();
        if roots.is_empty() {
            roots.push(Word::literal("."));
        }
        let expr = &args[split..];
        let mut i = 0;
        while i < expr.len() {
            let w = &expr[i];
            i += 1;
            if w.is("-delete") {
                for root in &roots {
                    self.write_target(WriteRule::FindDelete, "find", root, cwd);
                }
                continue;
            }
            if !matches!(w.text.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir") {
                continue;
            }
            let Some(len) = expr[i..]
                .iter()
                .position(|x| x.text == ";" || x.text == "+")
            else {
                self.opaque(OpaqueCause::Wrapper(Wrapper::FindExec), "find", cwd);
                break;
            };
            let command = &expr[i..i + len];
            i += len + 1;
            if !self.can_unwrap(Wrapper::FindExec, "find", cwd, depth) {
                continue;
            }
            for root in &roots {
                let substituted: Vec<Word> = command
                    .iter()
                    .map(|c| substitute_find_placeholder(c, root))
                    .collect();
                self.command(&substituted, env, cwd, depth + 1);
            }
        }
    }

    fn shell_cmd(&mut self, name: &str, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
        let mut i = 0;
        let mut script_at = None;
        while let Some(w) = args.get(i) {
            if !w.is_flag() || w.is("--") {
                break;
            }
            if w.text.starts_with("--") {
                i += if matches!(w.text.as_str(), "--rcfile" | "--init-file") {
                    2
                } else {
                    1
                };
                continue;
            }
            if w.text == "-o" {
                i += 2;
                continue;
            }
            if w.text[1..].contains('c') {
                script_at = Some(i + 1);
                break;
            }
            i += 1;
        }
        let Some(at) = script_at else {
            self.interpreter(name, args, cwd);
            return;
        };
        let Some(script) = args.get(at) else {
            self.opaque(OpaqueCause::Wrapper(Wrapper::ShellC), name, cwd);
            return;
        };
        if depth >= MAX_WRAPPER_DEPTH || self.run_script(&script.text, cwd, env, depth + 1).is_err()
        {
            self.opaque(OpaqueCause::Wrapper(Wrapper::ShellC), name, cwd);
        }
    }

    // ---- git ----

    fn git_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place) {
        let mut dir = cwd.clone();
        let mut i = 0;
        let mut sub = None;
        while i < args.len() {
            let w = &args[i];
            if !w.is_flag() {
                sub = Some(i);
                break;
            }
            i += match w.text.as_str() {
                "-C" => {
                    if let Some(d) = args.get(i + 1) {
                        dir = self.resolve(&dir, d);
                    }
                    2
                }
                "-c" | "--config-env" => {
                    if let Some(c) = args.get(i + 1) {
                        self.check_hooks_path_setting(&c.text);
                    }
                    2
                }
                "--git-dir" | "--work-tree" | "--namespace" | "--super-prefix" => 2,
                t => {
                    if let Some(setting) = t.strip_prefix("--config-env=") {
                        self.check_hooks_path_setting(setting);
                    }
                    1
                }
            };
        }
        let Some(at) = sub else { return };
        let name = args[at].text.as_str();
        let rest = &args[at + 1..];
        let program = format!("git {name}");
        if HOOK_SUBCOMMANDS.contains(&name) {
            self.check_hook_env(env);
        }
        if matches!(name, "commit" | "push" | "merge" | "am" | "rebase") {
            self.scan_hook_flags(name, rest);
        }
        match name {
            "commit" => self.push_hit(HitKind::Commit, &program, dir.to_anchor()),
            "apply" => {
                let read_only = rest.iter().any(|w| {
                    w.is("--check") || w.is("--stat") || w.is("--numstat") || w.is("--summary")
                });
                if !read_only {
                    self.write_place(WriteRule::GitWrite, &program, &dir);
                }
            }
            "am" | "rm" | "mv" => self.write_place(WriteRule::GitWrite, &program, &dir),
            "stash" => {
                let action = rest.iter().find(|w| !w.is_flag());
                if action.is_some_and(|w| matches!(w.text.as_str(), "pop" | "apply")) {
                    self.write_place(WriteRule::GitWrite, &program, &dir);
                }
            }
            "config" => self.check_config_command(rest),
            _ => {}
        }
    }

    fn check_hooks_path_setting(&mut self, setting: &str) {
        let key = setting.split('=').next().unwrap_or("");
        if key.eq_ignore_ascii_case("core.hookspath") {
            self.push_bypass(BypassRule::HooksPathConfig, setting);
        }
    }

    fn check_hook_env(&mut self, env: &Env) {
        for (name, value) in env {
            let token = format!("{name}={value}");
            if name == "HUSKY" && value == "0" || name == "SKIP" && !value.is_empty() {
                self.push_bypass(BypassRule::HookSkipEnv, &token);
            } else if name == "GIT_CONFIG_PARAMETERS" && contains_hooks_path(value)
                || name.starts_with("GIT_CONFIG_KEY_")
                    && value.eq_ignore_ascii_case("core.hookspath")
            {
                self.push_bypass(BypassRule::HooksPathEnv, &token);
            }
        }
    }

    fn scan_hook_flags(&mut self, sub: &str, rest: &[Word]) {
        let short_valued = match sub {
            "commit" => "mFCct",
            "merge" => "mFsX",
            "push" => "o",
            "rebase" => "xsXC",
            _ => "",
        };
        let mut i = 0;
        while i < rest.len() {
            let w = &rest[i];
            i += 1;
            if w.is("--") {
                break;
            }
            if !w.is_flag() {
                continue;
            }
            if let Some(long) = w.text.strip_prefix("--") {
                let (name, attached) = split_eq(long);
                if name.len() >= 7 && "no-verify".starts_with(name) {
                    self.push_bypass(BypassRule::NoVerifyFlag, &w.text);
                } else if !attached && GIT_VALUED_LONG.contains(&name) {
                    i += 1;
                }
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                if sub == "commit" && ch == 'n' {
                    self.push_bypass(BypassRule::CommitShortNoVerify, &w.text);
                }
                let after = 1 + ci + ch.len_utf8();
                if short_valued.contains(ch) {
                    i += short_value(w, after, rest.get(i)).1;
                    break;
                }
                if sub == "commit" && matches!(ch, 'u' | 'S') {
                    break;
                }
            }
        }
    }

    fn check_config_command(&mut self, rest: &[Word]) {
        let mut positionals: Vec<&Word> = Vec::new();
        let mut read = false;
        let mut unset = false;
        let mut i = 0;
        while i < rest.len() {
            let w = &rest[i];
            i += 1;
            if !w.is_flag() {
                positionals.push(w);
                continue;
            }
            match w.text.as_str() {
                "--get" | "--get-all" | "--get-regexp" | "--get-urlmatch" | "--get-color"
                | "--get-colorbool" | "-l" | "--list" => read = true,
                "--unset" | "--unset-all" => unset = true,
                "--file" | "-f" | "--blob" | "--default" | "--type" | "-t" | "--comment" => i += 1,
                _ => {}
            }
        }
        if read {
            return;
        }
        let (key, writes) = match positionals.as_slice() {
            [verb, key, rest @ ..] if verb.text == "set" => (*key, !rest.is_empty()),
            [verb, key, ..] if verb.text == "unset" => (*key, true),
            [verb, ..] if matches!(verb.text.as_str(), "get" | "list") => return,
            [key, rest @ ..] => (*key, !rest.is_empty()),
            [] => return,
        };
        if key.text.eq_ignore_ascii_case("core.hookspath") && (writes || unset) {
            self.push_bypass(BypassRule::HooksPathConfig, &key.text);
        }
    }

    // ---- in-place editors ----

    fn sed_cmd(&mut self, args: &[Word], cwd: &Place) {
        let mut in_place = false;
        let mut has_script = false;
        let mut positionals: Vec<&Word> = Vec::new();
        let mut options_ended = false;
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            if options_ended || !w.is_flag() {
                positionals.push(w);
                continue;
            }
            if w.is("--") {
                options_ended = true;
                continue;
            }
            if let Some(long) = w.text.strip_prefix("--") {
                let (name, attached) = split_eq(long);
                match name {
                    "in-place" => in_place = true,
                    "expression" | "file" => {
                        has_script = true;
                        i += usize::from(!attached);
                    }
                    "line-length" => i += usize::from(!attached),
                    _ => {}
                }
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'i' => {
                        in_place = true;
                        let bsd_suffix = w.text.len() == 2
                            && args.get(i).is_some_and(|n| n.quoted && n.text.is_empty());
                        i += usize::from(bsd_suffix);
                        break;
                    }
                    'e' | 'f' => {
                        has_script = true;
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    'l' => {
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    _ => {}
                }
            }
        }
        if !in_place {
            return;
        }
        let files = if has_script {
            &positionals[..]
        } else {
            positionals.get(1..).unwrap_or(&[])
        };
        for file in files {
            self.write_target(WriteRule::InPlaceEdit, "sed", file, cwd);
        }
    }

    fn awk_cmd(&mut self, args: &[Word], cwd: &Place) {
        let mut in_place = false;
        let mut has_program = false;
        let mut positionals: Vec<&Word> = Vec::new();
        let mut options_ended = false;
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            if options_ended || !w.is_flag() {
                positionals.push(w);
                continue;
            }
            if w.is("--") {
                options_ended = true;
                continue;
            }
            if let Some(long) = w.text.strip_prefix("--") {
                let (name, attached) = split_eq(long);
                let value = if attached {
                    Some(w.slice_from(2 + name.len() + 1))
                } else {
                    None
                };
                match name {
                    "include" => {
                        let value = value.or_else(|| args.get(i).cloned());
                        in_place |= value.is_some_and(|v| v.text == "inplace");
                        i += usize::from(!attached);
                    }
                    "file" | "source" | "exec" => {
                        has_program = true;
                        i += usize::from(!attached);
                    }
                    "assign" | "field-separator" => i += usize::from(!attached),
                    _ => {}
                }
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'i' => {
                        let (value, extra) = short_value(w, after, args.get(i));
                        in_place |= value.is_some_and(|v| v.text == "inplace");
                        i += extra;
                        break;
                    }
                    'f' | 'e' | 'E' => {
                        has_program = true;
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    'v' | 'F' => {
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    _ => {}
                }
            }
        }
        if !in_place {
            return;
        }
        let files = if has_program {
            &positionals[..]
        } else {
            positionals.get(1..).unwrap_or(&[])
        };
        for file in files.iter().filter(|f| assignment(f).is_none()) {
            self.write_target(WriteRule::InPlaceEdit, "awk", file, cwd);
        }
    }

    fn perl_cmd(&mut self, args: &[Word], cwd: &Place) {
        let mut in_place = false;
        let mut has_code = false;
        let mut positionals: Vec<&Word> = Vec::new();
        let mut options_ended = false;
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            if options_ended || !w.is_flag() {
                positionals.push(w);
                options_ended = true;
                continue;
            }
            if w.is("--") {
                options_ended = true;
                continue;
            }
            if w.text.starts_with("--") {
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'i' => {
                        in_place = true;
                        break;
                    }
                    'e' | 'E' => {
                        has_code = true;
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    'I' | 'M' | 'm' | 'x' | 'F' | 'C' | 'D' | 'd' => break,
                    _ => {}
                }
            }
        }
        let files = if has_code {
            &positionals[..]
        } else {
            positionals.get(1..).unwrap_or(&[])
        };
        if in_place && !files.is_empty() {
            for file in files {
                self.write_target(WriteRule::InPlaceEdit, "perl", file, cwd);
            }
        } else {
            self.interpreter("perl", args, cwd);
        }
    }

    // ---- file operations ----

    fn file_ops(&mut self, name: &str, args: &[Word], cwd: &Place) {
        let (short_valued, long_valued): (&str, &[&str]) = match name {
            "touch" => ("drt", &["date", "reference"]),
            "mkdir" => ("m", &["mode"]),
            "truncate" => ("sr", &["size", "reference"]),
            _ => ("", &[]),
        };
        let parsed = parse_args(args, short_valued, long_valued);
        for w in &parsed.positionals {
            self.write_target(WriteRule::FileOp, name, w, cwd);
        }
    }

    fn mv_cmd(&mut self, args: &[Word], cwd: &Place) {
        let parsed = parse_args(args, "tS", &["target-directory", "suffix"]);
        for w in &parsed.positionals {
            self.write_target(WriteRule::FileOp, "mv", w, cwd);
        }
        if let Some(dest) = parsed.value(&["t", "target-directory"]) {
            self.write_target(WriteRule::FileOp, "mv", dest, cwd);
        }
    }

    fn copy_like(&mut self, name: &str, args: &[Word], cwd: &Place) {
        let (short_valued, long_valued): (&str, &[&str]) = if name == "install" {
            (
                "tmgoS",
                &["target-directory", "mode", "group", "owner", "suffix"],
            )
        } else {
            ("tS", &["target-directory", "suffix"])
        };
        let parsed = parse_args(args, short_valued, long_valued);
        if name == "install" && parsed.has("d") {
            for w in &parsed.positionals {
                self.write_target(WriteRule::FileOp, name, w, cwd);
            }
            return;
        }
        if let Some(dest) = parsed.value(&["t", "target-directory"]) {
            self.write_target(WriteRule::CopyDest, name, dest, cwd);
            return;
        }
        match parsed.positionals.as_slice() {
            [] => {}
            [_] if name == "ln" => self.write_place(WriteRule::CopyDest, name, cwd),
            [_] => {}
            [.., last] => self.write_target(WriteRule::CopyDest, name, last, cwd),
        }
    }

    fn dd_cmd(&mut self, args: &[Word], cwd: &Place) {
        for w in args {
            if w.text.starts_with("of=") {
                self.write_target(WriteRule::DdOutput, "dd", &w.slice_from(3), cwd);
            }
        }
    }

    fn patch_cmd(&mut self, args: &[Word], cwd: &Place) {
        let parsed = parse_args(
            args,
            "dioprBVYzFDg",
            &[
                "directory",
                "input",
                "output",
                "strip",
                "reject-file",
                "backup-prefix",
                "basename-prefix",
                "suffix",
                "version-control",
                "fuzz",
                "ifdef",
                "get",
                "reject-format",
            ],
        );
        let base = match parsed.value(&["d", "directory"]) {
            Some(dir) => self.resolve(cwd, dir),
            None => cwd.clone(),
        };
        if let Some(output) = parsed.value(&["o", "output"]) {
            self.write_target(WriteRule::Patch, "patch", output, &base);
        } else if let Some(file) = parsed.positionals.first() {
            self.write_target(WriteRule::Patch, "patch", file, &base);
        } else {
            self.write_place(WriteRule::Patch, "patch", &base);
        }
    }

    fn tar_cmd(&mut self, args: &[Word], cwd: &Place) {
        let mut extract = false;
        let mut dir = cwd.clone();
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            let first = i == 0;
            i += 1;
            if let Some(long) = w.text.strip_prefix("--") {
                if long.is_empty() {
                    break;
                }
                let (name, attached) = split_eq(long);
                match name {
                    "extract" | "get" => extract = true,
                    "directory" => {
                        let value = if attached {
                            Some(w.slice_from(2 + name.len() + 1))
                        } else {
                            i += 1;
                            args.get(i - 1).cloned()
                        };
                        if let Some(value) = value {
                            dir = self.resolve(&dir, &value);
                        }
                    }
                    "file"
                    | "files-from"
                    | "exclude-from"
                    | "use-compress-program"
                    | "transform"
                    | "exclude"
                    | "to-command"
                    | "suffix"
                    | "index-file" => {
                        i += usize::from(!attached);
                    }
                    _ => {}
                }
                continue;
            }
            let letters = if w.text.starts_with('-') {
                &w.text[1..]
            } else if first && w.text.chars().all(|c| c.is_ascii_alphabetic()) {
                &w.text[..]
            } else {
                continue;
            };
            let offset = w.text.len() - letters.len();
            for (ci, ch) in letters.char_indices() {
                let after = offset + ci + ch.len_utf8();
                match ch {
                    'x' => extract = true,
                    'C' => {
                        let (value, extra) = short_value(w, after, args.get(i));
                        if let Some(value) = value {
                            dir = self.resolve(&dir, &value);
                        }
                        i += extra;
                        break;
                    }
                    'f' | 'T' | 'X' | 'I' | 'H' | 'b' | 'L' | 'N' | 'V' | 'g' => {
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    _ => {}
                }
            }
        }
        if extract {
            self.write_place(WriteRule::Extract, "tar", &dir);
        }
    }

    fn unzip_cmd(&mut self, args: &[Word], cwd: &Place) {
        let mut dir = cwd.clone();
        let mut listing = false;
        let mut operands = 0;
        let mut i = 0;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            if !w.is_flag() {
                operands += 1;
                continue;
            }
            if w.text.starts_with("--") {
                continue;
            }
            for (ci, ch) in w.text[1..].char_indices() {
                let after = 1 + ci + ch.len_utf8();
                match ch {
                    'd' => {
                        let (value, extra) = short_value(w, after, args.get(i));
                        if let Some(value) = value {
                            dir = self.resolve(&dir, &value);
                        }
                        i += extra;
                        break;
                    }
                    'P' | 'O' | 'I' => {
                        i += short_value(w, after, args.get(i)).1;
                        break;
                    }
                    'l' | 't' | 'p' | 'v' | 'Z' => listing = true,
                    _ => {}
                }
            }
        }
        if !listing && operands > 0 {
            self.write_place(WriteRule::Extract, "unzip", &dir);
        }
    }

    fn curl_cmd(&mut self, args: &[Word], cwd: &Place) {
        let parsed = parse_args(
            args,
            "AbcCdDeEFHKmoPQrtTuUwxXyYz",
            &["output", "output-dir"],
        );
        let base = match parsed.value(&["output-dir"]) {
            Some(dir) => self.resolve(cwd, dir),
            None => cwd.clone(),
        };
        for target in parsed.values(&["o", "output"]) {
            if !target.is("-") {
                self.write_target(WriteRule::Download, "curl", target, &base);
            }
        }
    }

    fn wget_cmd(&mut self, args: &[Word], cwd: &Place) {
        let parsed = parse_args(args, "OPoaeitTwQ", &["output-document", "directory-prefix"]);
        for target in parsed.values(&["O", "output-document"]) {
            if !target.is("-") {
                self.write_target(WriteRule::Download, "wget", target, cwd);
            }
        }
        for dir in parsed.values(&["P", "directory-prefix"]) {
            self.write_target(WriteRule::Download, "wget", dir, cwd);
        }
    }

    fn rsync_cmd(&mut self, args: &[Word], cwd: &Place) {
        let parsed = parse_args(
            args,
            "eBfMT",
            &[
                "rsh",
                "rsync-path",
                "filter",
                "exclude",
                "exclude-from",
                "include",
                "include-from",
                "files-from",
                "partial-dir",
                "backup-dir",
                "temp-dir",
                "log-file",
                "bwlimit",
                "port",
                "timeout",
                "max-size",
                "min-size",
                "compare-dest",
                "copy-dest",
                "link-dest",
                "suffix",
                "chmod",
                "chown",
                "usermap",
                "groupmap",
                "block-size",
                "compress-level",
                "skip-compress",
                "out-format",
                "info",
                "debug",
            ],
        );
        if parsed.has("n") || parsed.has("dry-run") || parsed.has("list-only") {
            return;
        }
        if let [_, .., dest] = parsed.positionals.as_slice() {
            if !is_remote_spec(&dest.text) {
                self.write_target(WriteRule::CopyDest, "rsync", dest, cwd);
            }
        }
    }

    // ---- opaque executions ----

    fn interpreter(&mut self, name: &str, args: &[Word], cwd: &Place) {
        if !is_trusted_invocation(name, args) {
            self.opaque(OpaqueCause::Interpreter(name.to_string()), name, cwd);
        }
    }
}

fn is_remote_spec(text: &str) -> bool {
    text.split_once(':')
        .is_some_and(|(host, _)| !host.contains('/'))
}

fn contains_hooks_path(value: &str) -> bool {
    value.to_ascii_lowercase().contains("core.hookspath")
}

fn is_duration(text: &str) -> bool {
    let digits = text.strip_suffix(['s', 'm', 'h', 'd']).unwrap_or(text);
    !digits.is_empty()
        && digits.chars().any(|c| c.is_ascii_digit())
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn substitute_find_placeholder(word: &Word, root: &Word) -> Word {
    if !word.text.contains("{}") {
        return word.clone();
    }
    Word {
        text: format!("{}/*", root.text),
        quoted: root.quoted,
        dyn_at: root.dyn_at.or(Some(root.text.len() + 1)),
        tilde: root.tilde,
        subs: Vec::new(),
    }
}

fn substitute_stdin_marker(word: &Word, marker: &str) -> Word {
    let Some(at) = word.text.find(marker) else {
        return word.clone();
    };
    Word {
        text: word.text.replace(marker, "<stdin>"),
        dyn_at: Some(word.dyn_at.map_or(at, |d| d.min(at))),
        ..word.clone()
    }
}

fn is_interpreter(name: &str) -> bool {
    matches!(
        name,
        "node"
            | "nodejs"
            | "deno"
            | "bun"
            | "ruby"
            | "perl"
            | "php"
            | "bash"
            | "sh"
            | "zsh"
            | "dash"
            | "ksh"
    ) || name
        .strip_prefix("python")
        .is_some_and(|version| version.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

fn is_trusted_invocation(name: &str, args: &[Word]) -> bool {
    if let [only] = args {
        if matches!(
            only.text.as_str(),
            "--version" | "-v" | "-V" | "--help" | "-h"
        ) && !only.quoted
        {
            return true;
        }
    }
    let leading_flags = || args.iter().take_while(|w| w.is_flag());
    match name {
        "node" | "nodejs" => leading_flags().any(|w| w.is("--test")),
        "bun" | "deno" => args
            .iter()
            .find(|w| !w.is_flag())
            .is_some_and(|w| w.text == "test"),
        _ if name.starts_with("python") => {
            for (idx, w) in leading_flags().enumerate() {
                let module = if w.is("-m") {
                    args.get(idx + 1).map(|m| m.text.as_str())
                } else {
                    w.text.strip_prefix("-m").filter(|_| !w.quoted)
                };
                if let Some(module) = module {
                    return matches!(module, "pytest" | "unittest");
                }
            }
            false
        }
        _ => false,
    }
}

use std::path::Path;

use super::args::{assignment, is_device, normalize, program_name};
use super::interpreters::{is_interpreter, is_shell};
use super::lexer::{lex, RedirOp, Sep, Tok, Word};
use super::{
    is_hooks_path, Anchor, BashAnalysis, BranchSwitch, BypassHit, BypassRule, Hit, HitKind,
    LexError, OpaqueCause, Wrapper, WriteRule,
};

pub(super) const MAX_WRAPPER_DEPTH: usize = 2;

const RESERVED: &[&str] = &[
    "{", "}", "!", "if", "then", "else", "elif", "fi", "do", "done", "while", "until",
];

pub(super) type Env = Vec<(String, String)>;

/// State a segment leaves for the following segments of the same shell.
enum Effect {
    Cd(Anchor),
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

pub(super) struct Analyzer<'a> {
    pub(super) home: Option<&'a Path>,
    pub(super) out: BashAnalysis,
    /// A branch switch whose `&&` chain the current segment belongs to.
    pub(super) switch: Option<BranchSwitch>,
    /// A branch switch made by the segment being analyzed; `Some(None)` when
    /// the branch it lands on can't be named.
    pub(super) pending_switch: Option<Option<BranchSwitch>>,
}

/// Shell state carried from one segment of a script to the next.
struct Shell {
    cwd: Anchor,
    env: Env,
    frames: Vec<Frame>,
    /// The cwd the current and-or list started in.
    list_cwd: Anchor,
    /// A `cd` in this list ran only if an earlier command succeeded.
    list_conditional: bool,
    /// The cwd a `cd` set before a `||`, kept for a following `exit`.
    or_saved: Option<Anchor>,
    /// `or_saved` carried into the brace group after the `||`, with the frame
    /// depth inside that group.
    or_group: Option<(Anchor, usize)>,
    /// The next segment starts an and-or list.
    head: bool,
    /// The next brace group is a function body that may never run.
    func_pending: bool,
}

enum Frame {
    Subshell(Anchor, Env),
    Brace(Option<(Anchor, Env)>),
}

impl Shell {
    fn new(cwd: &Anchor, env: &Env) -> Shell {
        Shell {
            cwd: cwd.clone(),
            env: env.clone(),
            frames: Vec::new(),
            list_cwd: cwd.clone(),
            list_conditional: false,
            or_saved: None,
            or_group: None,
            head: true,
            func_pending: false,
        }
    }

    fn end_list(&mut self) {
        if self.list_conditional && self.cwd != self.list_cwd {
            self.cwd = Anchor::unknown("cd");
        }
        self.list_cwd = self.cwd.clone();
        self.list_conditional = false;
        self.or_saved = None;
        self.head = true;
    }

    fn separator(&mut self, sep: Sep) {
        match sep {
            Sep::And | Sep::Pipe => self.head = false,
            Sep::Or => {
                if self.cwd != self.list_cwd {
                    self.or_saved = Some(std::mem::replace(&mut self.cwd, Anchor::unknown("cd")));
                }
                self.head = false;
            }
            Sep::Seq | Sep::Amp => self.end_list(),
            Sep::LParen => {
                self.func_pending = false;
                self.frames
                    .push(Frame::Subshell(self.cwd.clone(), self.env.clone()));
                self.end_list();
            }
            Sep::RParen => {
                self.end_list();
                while let Some(frame) = self.frames.pop() {
                    if let Frame::Subshell(cwd, env) = frame {
                        self.cwd = cwd;
                        self.env = env;
                        break;
                    }
                }
                self.list_cwd = self.cwd.clone();
            }
        }
    }

    /// Strips the brace-group and function-definition words that open a
    /// segment, entering or leaving their frames.
    fn enter_groups<'w>(&mut self, mut words: &'w [Word]) -> &'w [Word] {
        loop {
            match words.first() {
                Some(w) if w.is("function") && !w.quoted && words.len() >= 2 => {
                    self.func_pending = true;
                    words = &words[2..];
                }
                Some(w) if w.is("{") && !w.quoted => {
                    let saved = std::mem::take(&mut self.func_pending)
                        .then(|| (self.cwd.clone(), self.env.clone()));
                    self.frames.push(Frame::Brace(saved));
                    words = &words[1..];
                }
                Some(w) if w.is("}") && !w.quoted => {
                    if let Some(Frame::Brace(saved)) = self.frames.last() {
                        if let Some((cwd, env)) = saved.clone() {
                            self.cwd = cwd;
                            self.env = env;
                            self.list_cwd = self.cwd.clone();
                        }
                        self.frames.pop();
                    }
                    words = &words[1..];
                }
                _ => return words,
            }
        }
    }
}

/// Whether the first unresolved component of `raw`, the one right below the
/// literal prefix, could expand to `hooks` or a later `..` climbs back to it.
fn may_name_hooks_dir(raw: &str) -> bool {
    let mut parts = raw.split('/');
    let first = parts.find(|c| c.contains(['*', '?', '[', '{', '$', '`']));
    let climbs_back = parts.any(|c| c == "..");
    climbs_back || first.is_none_or(|c| glob_may_match(c.as_bytes(), b"hooks"))
}

/// `*`/`?` glob match; a bracket expression or expansion matches anything.
fn glob_may_match(pattern: &[u8], name: &[u8]) -> bool {
    match pattern.first() {
        None => name.is_empty(),
        Some(b'[' | b'$' | b'`' | b'{') => true,
        Some(b'*') => (0..=name.len()).any(|i| glob_may_match(&pattern[1..], &name[i..])),
        Some(b'?') => !name.is_empty() && glob_may_match(&pattern[1..], &name[1..]),
        Some(c) => name.first() == Some(c) && glob_may_match(&pattern[1..], &name[1..]),
    }
}

fn starts_exit(seg: &Segment) -> bool {
    seg.words
        .first()
        .is_some_and(|w| matches!(w.text.as_str(), "exit" | "return"))
}

/// `name` or `function name` right before `()`: a function definition.
fn is_function_head(seg: &Segment) -> bool {
    if !seg.redirs.is_empty() {
        return false;
    }
    match seg.words.as_slice() {
        [name] => !name.quoted && name.dyn_at.is_none(),
        [kw, _] => kw.is("function") && !kw.quoted,
        _ => false,
    }
}

impl Analyzer<'_> {
    pub(super) fn new(home: Option<&Path>) -> Analyzer<'_> {
        Analyzer {
            home,
            out: BashAnalysis::default(),
            switch: None,
            pending_switch: None,
        }
    }

    pub(super) fn run_script(
        &mut self,
        src: &str,
        cwd: &Anchor,
        env: &Env,
        depth: usize,
        lenient: bool,
    ) -> Result<(), LexError> {
        let toks = lex(src, lenient)?;
        let outer_switch = self.switch.clone();
        let mut sh = Shell::new(cwd, env);
        let mut seg = Segment::default();
        let mut after_pipe = false;
        let mut i = 0;
        while i < toks.len() {
            match &toks[i] {
                Tok::Word(w) => seg.push_word(w.clone()),
                Tok::Redir(op) => seg.redirs.push((*op, None)),
                Tok::Sep(Sep::LParen)
                    if is_function_head(&seg)
                        && matches!(toks.get(i + 1), Some(Tok::Sep(Sep::RParen))) =>
                {
                    seg = Segment::default();
                    sh.func_pending = true;
                    i += 1;
                }
                Tok::Sep(sep) => {
                    let sep = *sep;
                    let before_pipe = sep == Sep::Pipe;
                    let propagate = !after_pipe && !before_pipe && sep != Sep::Amp;
                    self.flush(&mut seg, &mut sh, depth, propagate);
                    self.carry_switch(sep == Sep::And);
                    sh.separator(sep);
                    after_pipe = before_pipe;
                }
            }
            i += 1;
        }
        self.flush(&mut seg, &mut sh, depth, !after_pipe);
        self.pending_switch = None;
        self.switch = outer_switch;
        Ok(())
    }

    /// A switch made by the segment just analyzed covers the rest of its `&&`
    /// chain; any other separator ends the chain and its switch.
    fn carry_switch(&mut self, chained: bool) {
        let made = self.pending_switch.take();
        if chained {
            if let Some(made) = made {
                self.switch = made;
            }
        } else {
            self.switch = None;
        }
    }

    fn flush(&mut self, seg: &mut Segment, sh: &mut Shell, depth: usize, propagate: bool) {
        let mut segment = std::mem::take(seg);
        let opens_group = segment
            .words
            .first()
            .is_some_and(|w| w.is("{") && !w.quoted);
        segment.words = sh.enter_groups(&segment.words).to_vec();
        let head = std::mem::replace(&mut sh.head, false);
        if sh
            .or_group
            .as_ref()
            .is_some_and(|(_, d)| sh.frames.len() < *d)
        {
            sh.or_group = None;
        }
        let mut group_head = false;
        if let Some(saved) = sh.or_saved.take() {
            if opens_group {
                sh.or_group = Some((saved, sh.frames.len()));
                group_head = true;
            } else if starts_exit(&segment) {
                sh.cwd = saved;
                sh.list_cwd = sh.cwd.clone();
            }
        }
        if (head || group_head) && starts_exit(&segment) {
            if let Some((saved, _)) = sh.or_group.take() {
                sh.cwd = saved;
                sh.list_cwd = sh.cwd.clone();
            }
        }
        if segment.words.is_empty() && segment.redirs.is_empty() {
            return;
        }
        let effect = self.segment(&segment, &sh.cwd, &sh.env, depth);
        if !propagate {
            return;
        }
        match effect {
            Some(Effect::Cd(next)) => {
                sh.cwd = next;
                sh.list_conditional |= !head;
            }
            Some(Effect::Env(assigned)) => {
                for kv in assigned {
                    set_env(&mut sh.env, kv);
                }
            }
            None => {}
        }
    }

    fn scan_substitutions(&mut self, seg: &Segment, cwd: &Anchor, env: &Env, depth: usize) {
        let targets = seg.redirs.iter().filter_map(|(_, t)| t.as_ref());
        for word in seg.words.iter().chain(targets) {
            for body in &word.subs {
                self.run_nested(body, cwd, env, depth, OpaqueCause::Substitution, "$(");
            }
        }
    }

    /// Runs `src` one level deeper; past the depth limit or on a lex failure
    /// the nested script counts as one opaque execution.
    pub(super) fn run_nested(
        &mut self,
        src: &str,
        cwd: &Anchor,
        env: &Env,
        depth: usize,
        cause: OpaqueCause,
        program: &str,
    ) {
        if depth >= MAX_WRAPPER_DEPTH || self.run_script(src, cwd, env, depth + 1, false).is_err() {
            self.opaque(cause, program, cwd);
        }
    }

    /// Returns the state change the segment leaves behind for later segments
    /// of the same shell.
    fn segment(&mut self, seg: &Segment, cwd: &Anchor, env: &Env, depth: usize) -> Option<Effect> {
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
            name @ ("pushd" | "popd") => Some(Effect::Cd(Anchor::unknown(name))),
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

    fn cd_target(&self, args: &[Word], cwd: &Anchor) -> Anchor {
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
                Some(h) => Anchor::Path(normalize(h)),
                None => Anchor::unknown("~"),
            },
            Some(w) if w.is("-") => Anchor::unknown("-"),
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

    pub(super) fn resolve(&self, base: &Anchor, w: &Word) -> Anchor {
        let Some((text, dyn_at)) = self.expand_tilde(w) else {
            return Anchor::unknown(&w.text);
        };
        let is_abs = text.starts_with('/');
        let Some(idx) = dyn_at else {
            return match base {
                _ if is_abs => Anchor::Path(normalize(Path::new(&text))),
                Anchor::Path(b) => Anchor::Path(normalize(&b.join(&text))),
                Anchor::Unresolved {
                    literal_prefix,
                    raw,
                } => Anchor::Unresolved {
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
            return Anchor::Unresolved {
                literal_prefix: Some(normalize(Path::new(dir_part))),
                raw: text,
            };
        }
        if idx == 0 && matches!(text[idx..].chars().next(), Some('$' | '`' | '<' | '>')) {
            return Anchor::unknown(&text);
        }
        let (prefix_base, raw) = match base {
            Anchor::Path(b) => (Some(b.clone()), text.clone()),
            Anchor::Unresolved {
                literal_prefix,
                raw: base_raw,
            } => (literal_prefix.clone(), format!("{base_raw}/{text}")),
        };
        Anchor::Unresolved {
            literal_prefix: prefix_base.map(|p| normalize(&p.join(dir_part))),
            raw,
        }
    }

    // ---- hit emission ----

    pub(super) fn push_hit(&mut self, kind: HitKind, program: &str, anchor: Anchor) {
        if matches!(kind, HitKind::Write(_)) {
            let place = match &anchor {
                Anchor::Path(p) => Some(p).filter(|p| is_hooks_path(p)),
                Anchor::Unresolved {
                    literal_prefix,
                    raw,
                } => literal_prefix
                    .as_ref()
                    .filter(|p| is_hooks_path(p) || p.ends_with(".git") && may_name_hooks_dir(raw)),
            };
            if let Some(p) = place {
                let token = p.display().to_string();
                self.push_bypass(BypassRule::HooksDirWrite, &token);
            }
        }
        self.out.hits.push(Hit {
            kind,
            anchor,
            program: program.to_string(),
            on_branch: self.switch.clone(),
        });
    }

    pub(super) fn push_bypass(&mut self, rule: BypassRule, token: &str) {
        self.out.bypass.push(BypassHit {
            rule,
            token: token.to_string(),
        });
    }

    pub(super) fn opaque(&mut self, cause: OpaqueCause, program: &str, cwd: &Anchor) {
        self.push_hit(HitKind::Opaque(cause), program, cwd.clone());
    }

    pub(super) fn write_target(
        &mut self,
        rule: WriteRule,
        program: &str,
        target: &Word,
        cwd: &Anchor,
    ) {
        if target.text.is_empty() {
            return;
        }
        let place = self.resolve(cwd, target);
        if matches!(&place, Anchor::Path(p) if is_device(p)) {
            return;
        }
        self.push_hit(HitKind::Write(rule), program, place);
    }

    pub(super) fn write_place(&mut self, rule: WriteRule, program: &str, place: &Anchor) {
        self.push_hit(HitKind::Write(rule), program, place.clone());
    }

    // ---- command dispatch ----

    pub(super) fn command(&mut self, words: &[Word], env: &Env, cwd: &Anchor, depth: usize) {
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
                    self.command(args, env, cwd, depth + 1);
                }
            }
            "time" => self.time_cmd(args, env, cwd, depth),
            "command" => self.command_cmd(args, env, cwd, depth),
            "timeout" => self.timeout_cmd(args, env, cwd, depth),
            "xargs" => self.xargs_cmd(args, env, cwd, depth),
            "find" => self.find_cmd(args, env, cwd, depth),
            _ if is_shell(name) => self.shell_cmd(name, args, env, cwd, depth),
            "eval" => self.opaque(OpaqueCause::Eval, name, cwd),
            "builtin" => self.command(args, env, cwd, depth),
            "exec" => self.exec_cmd(args, env, cwd, depth),
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
            "chmod" => self.chmod_cmd(args, cwd),
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
}

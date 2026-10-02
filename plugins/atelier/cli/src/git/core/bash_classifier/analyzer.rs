use std::path::Path;

use super::args::{assignment, is_device, normalize, program_name, Place};
use super::interpreters::is_interpreter;
use super::lexer::{lex, RedirOp, Sep, Tok, Word};
use super::{
    Anchor, BashAnalysis, BypassHit, BypassRule, Hit, HitKind, LexError, OpaqueCause, Wrapper,
    WriteRule,
};

pub(super) const MAX_WRAPPER_DEPTH: usize = 2;

const RESERVED: &[&str] = &[
    "{", "}", "!", "if", "then", "else", "elif", "fi", "do", "done", "while", "until",
];

pub(super) type Env = Vec<(String, String)>;

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
}

impl Analyzer<'_> {
    pub(super) fn run_script(
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

    pub(super) fn resolve(&self, base: &Place, w: &Word) -> Place {
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

    pub(super) fn push_hit(&mut self, kind: HitKind, program: &str, anchor: Anchor) {
        self.out.hits.push(Hit {
            kind,
            anchor,
            program: program.to_string(),
        });
    }

    pub(super) fn push_bypass(&mut self, rule: BypassRule, token: &str) {
        self.out.bypass.push(BypassHit {
            rule,
            token: token.to_string(),
        });
    }

    pub(super) fn opaque(&mut self, cause: OpaqueCause, program: &str, cwd: &Place) {
        self.push_hit(HitKind::Opaque(cause), program, cwd.to_anchor());
    }

    pub(super) fn write_target(
        &mut self,
        rule: WriteRule,
        program: &str,
        target: &Word,
        cwd: &Place,
    ) {
        if target.text.is_empty() {
            return;
        }
        let place = self.resolve(cwd, target);
        if matches!(&place, Place::Known(p) if is_device(p)) {
            return;
        }
        self.push_hit(HitKind::Write(rule), program, place.to_anchor());
    }

    pub(super) fn write_place(&mut self, rule: WriteRule, program: &str, place: &Place) {
        self.push_hit(HitKind::Write(rule), program, place.to_anchor());
    }

    // ---- command dispatch ----

    pub(super) fn command(&mut self, words: &[Word], env: &Env, cwd: &Place, depth: usize) {
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
}

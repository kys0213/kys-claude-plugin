use super::analyzer::{Analyzer, Env, MAX_WRAPPER_DEPTH};
use super::args::{assignment, short_value, split_eq, Place};
use super::lexer::Word;
use super::{OpaqueCause, Wrapper, WriteRule};

impl Analyzer<'_> {
    pub(super) fn can_unwrap(
        &mut self,
        wrapper: Wrapper,
        name: &str,
        cwd: &Place,
        depth: usize,
    ) -> bool {
        if depth >= MAX_WRAPPER_DEPTH {
            self.opaque(OpaqueCause::Wrapper(wrapper), name, cwd);
            return false;
        }
        true
    }

    pub(super) fn inner(&mut self, rest: &[Word], env: &Env, cwd: &Place, depth: usize) {
        if !rest.is_empty() {
            self.command(rest, env, cwd, depth + 1);
        }
    }

    pub(super) fn env_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
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

    pub(super) fn sudo_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
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

    pub(super) fn time_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
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

    pub(super) fn command_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
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

    pub(super) fn timeout_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
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

    pub(super) fn xargs_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
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

    pub(super) fn find_cmd(&mut self, args: &[Word], env: &Env, cwd: &Place, depth: usize) {
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

    pub(super) fn shell_cmd(
        &mut self,
        name: &str,
        args: &[Word],
        env: &Env,
        cwd: &Place,
        depth: usize,
    ) {
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

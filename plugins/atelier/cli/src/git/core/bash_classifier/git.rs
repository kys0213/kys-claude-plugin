use super::analyzer::{Analyzer, Env};
use super::args::{short_value, split_eq};
use super::lexer::Word;
use super::{Anchor, BypassRule, HitKind, WriteRule};

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

impl Analyzer<'_> {
    pub(super) fn git_cmd(&mut self, args: &[Word], env: &Env, cwd: &Anchor) {
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
            "commit" => self.push_hit(HitKind::Commit, &program, dir.clone()),
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
}

fn contains_hooks_path(value: &str) -> bool {
    value.to_ascii_lowercase().contains("core.hookspath")
}

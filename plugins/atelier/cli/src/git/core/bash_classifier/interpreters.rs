use super::analyzer::Analyzer;
use super::lexer::Word;
use super::{Anchor, OpaqueCause};

impl Analyzer<'_> {
    pub(super) fn interpreter(&mut self, name: &str, args: &[Word], cwd: &Anchor) {
        if !is_trusted_invocation(name, args) {
            self.opaque(OpaqueCause::Interpreter(name.to_string()), name, cwd);
        }
    }
}

pub(crate) fn is_shell(name: &str) -> bool {
    matches!(name, "bash" | "sh" | "zsh" | "dash" | "ksh")
}

/// Where a shell invocation's script comes from.
pub(crate) enum ShellScript<'a> {
    /// The word after `-c`.
    Inline(&'a Word),
    /// `-c` with nothing after it.
    Missing,
    /// No `-c`: a script file or stdin.
    Input,
}

/// Reads a shell's arguments up to its first operand.
pub(crate) fn shell_script(args: &[Word]) -> ShellScript<'_> {
    let mut i = 0;
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
            return args
                .get(i + 1)
                .map_or(ShellScript::Missing, ShellScript::Inline);
        }
        i += 1;
    }
    ShellScript::Input
}

pub(super) fn is_interpreter(name: &str) -> bool {
    matches!(
        name,
        "node" | "nodejs" | "deno" | "bun" | "ruby" | "perl" | "php"
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

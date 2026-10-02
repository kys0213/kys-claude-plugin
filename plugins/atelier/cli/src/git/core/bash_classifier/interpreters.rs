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

pub(super) fn is_shell(name: &str) -> bool {
    matches!(name, "bash" | "sh" | "zsh" | "dash" | "ksh")
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

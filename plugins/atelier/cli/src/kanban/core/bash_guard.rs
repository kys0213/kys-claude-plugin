//! Which kanban calls a model's shell command may not make: the ones that
//! speak as a person on the board, and the ones only the hooks module makes.
//! Judged on the words the shell would split, without expanding anything.

use super::allowlist::shell_line;

const KANBAN: &str = "kanban";
const AS_PERSON: &[&str] = &["reply", "permit", "decide", "send"];
const MOD_ONLY: &[&[&str]] = &[&["forget"], &["relay-frame"], &["ledger", "record"]];

/// Why the command may not run, or `None` when it may.
pub fn refusal(command: &str) -> Option<String> {
    shell_line(command)
        .commands
        .iter()
        .find_map(|argv| kanban_args(argv).and_then(judge))
}

/// The words after `kanban` in one simple command.
fn kanban_args(argv: &[String]) -> Option<&[String]> {
    let at = argv.iter().skip(1).position(|word| word == KANBAN)? + 1;
    Some(&argv[at + 1..])
}

fn judge(args: &[String]) -> Option<String> {
    let sub = args.first()?.as_str();
    let mod_only = MOD_ONLY.iter().find(|path| {
        args.iter()
            .map(String::as_str)
            .take(path.len())
            .eq(path.iter().copied())
    });
    if let Some(path) = mod_only {
        return Some(format!(
            "`atelier kanban {}` 은 모드만 불러요 — 셸에서 부르지 않아요",
            path.join(" ")
        ));
    }
    (AS_PERSON.contains(&sub) && names_pane(args)).then(|| {
        format!(
            "`atelier kanban {sub} --from pane` 은 사람이 칸반 pane·터미널에서 답할 때만 써요 — 모델은 `--from agent` 로 보내요"
        )
    })
}

fn names_pane(args: &[String]) -> bool {
    args.iter().enumerate().any(|(i, word)| {
        word == "--from=pane" || (word == "--from" && args.get(i + 1).is_some_and(|v| v == "pane"))
    })
}

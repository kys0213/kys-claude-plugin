//! Which kanban calls a model's shell command may not make: the ones that
//! speak as a person on the board, and the ones only the hooks module makes.
//! Judged on the words the shell would split, without expanding variables;
//! the scripts of command substitutions and of nested shells are judged the
//! same way.

use crate::git::core::bash_classifier::interpreters::{is_shell, shell_script, ShellScript};
use crate::git::core::bash_classifier::lexer::{
    lex_with_heredocs, HeredocBody, RedirOp, Tok, Word,
};

const KANBAN: &str = "kanban";
const AS_PERSON: &[&str] = &["reply", "permit", "decide", "send"];
const MOD_ONLY: &[&[&str]] = &[&["forget"], &["relay-frame"], &["ledger", "record"]];
/// Scripts nested deeper than this are refused unread.
const MAX_DEPTH: usize = 3;

/// Why the command may not run, or `None` when it may.
pub fn refusal(command: &str) -> Option<String> {
    judge_script(command, 0)
}

fn judge_script(script: &str, depth: usize) -> Option<String> {
    if depth > MAX_DEPTH {
        return Some(format!(
            "셸 안의 셸이 {MAX_DEPTH}겹을 넘어 읽지 않았어요 — 칸반 명령은 바로 부르세요"
        ));
    }
    let Ok((toks, bodies)) = lex_with_heredocs(script, false) else {
        return Some(
            "닫히지 않은 따옴표나 명령 치환이 있어 셸이 나누는 대로 읽지 못했어요".to_string(),
        );
    };
    simple_commands(toks, &bodies)
        .iter()
        .find_map(|command| judge_command(command, depth))
}

#[derive(Default)]
struct Simple {
    argv: Vec<Word>,
    /// Redirection targets, here-strings and heredoc substitution carriers.
    operands: Vec<Word>,
    /// What a here-string or heredoc gives the command on stdin.
    input: Vec<String>,
}

fn simple_commands(toks: Vec<Tok>, bodies: &[HeredocBody]) -> Vec<Simple> {
    let mut commands = vec![Simple::default()];
    let mut operand_of = None;
    for (at, tok) in toks.into_iter().enumerate() {
        let Some(current) = commands.last_mut() else {
            break;
        };
        match tok {
            Tok::Sep(_) => {
                operand_of = None;
                commands.push(Simple::default());
            }
            Tok::Redir(RedirOp::HereDoc) => current.input.extend(
                bodies
                    .iter()
                    .filter(|body| body.at == at)
                    .map(|body| body.text.clone()),
            ),
            Tok::Redir(op) => operand_of = op.takes_operand().then_some(op),
            Tok::Word(word) => match operand_of.take() {
                Some(op) => {
                    if op == RedirOp::HereString {
                        current.input.push(word.text.clone());
                    }
                    current.operands.push(word);
                }
                None => current.argv.push(word),
            },
        }
    }
    commands
}

fn judge_command(command: &Simple, depth: usize) -> Option<String> {
    let argv: Vec<String> = command.argv.iter().map(|w| w.text.clone()).collect();
    kanban_args(&argv).and_then(judge).or_else(|| {
        nested_scripts(command)
            .iter()
            .find_map(|script| judge_script(script, depth + 1))
    })
}

/// The scripts the shell would run for this command beyond its own words.
fn nested_scripts(command: &Simple) -> Vec<String> {
    let substitutions = command
        .argv
        .iter()
        .chain(&command.operands)
        .flat_map(|word| word.subs.iter().cloned());
    let shell = command
        .argv
        .iter()
        .position(|word| word.text.rsplit('/').next().is_some_and(is_shell));
    let shell_scripts = match shell.map(|at| shell_script(&command.argv[at + 1..])) {
        Some(ShellScript::Inline(script)) => vec![script.text.clone()],
        Some(ShellScript::Input) => command.input.clone(),
        Some(ShellScript::Missing) | None => Vec::new(),
    };
    substitutions.chain(shell_scripts).collect()
}

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

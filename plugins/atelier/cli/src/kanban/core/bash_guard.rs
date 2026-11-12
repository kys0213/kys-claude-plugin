//! Which kanban calls a model's shell command may not make: the ones that
//! speak as a person on the board, and the ones only the hooks module makes.
//! Judged on the words the shell would split, without expanding variables.
//! What a shell would run besides — command and process substitutions, `-c`
//! scripts, a shell's heredoc or here-string, the words piped into a shell
//! and a process substitution a shell reads as its script — is judged the
//! same way.

use crate::git::core::bash_classifier::interpreters::{is_shell, shell_script, ShellScript};
use crate::git::core::bash_classifier::lexer::{
    lex_with_heredocs, HeredocBody, RedirOp, Sep, Tok, Word,
};

const KANBAN: &str = "kanban";
const AS_PERSON: &[&str] = &["reply", "permit", "decide", "send"];
const MOD_ONLY: &[&[&str]] = &[
    &["forget"],
    &["relay-frame"],
    &["command"],
    &["delegate", "report"],
    &["ledger", "record"],
    &["ledger", "bind"],
    &["ledger", "prune"],
    &["inbox", "take"],
    &["inbox", "wait-permit"],
    &["inbox", "dir"],
    &["inbox", "clear"],
];
const SCRIPT_READERS: &[&str] = &["source", "."];
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
    let commands = match parse(script) {
        Ok(commands) => commands,
        Err(reason) => return Some(reason),
    };
    commands
        .iter()
        .enumerate()
        .find_map(|(at, command)| judge_command(command, piped_into(&commands, at), depth))
}

fn parse(script: &str) -> Result<Vec<Simple>, String> {
    let (toks, bodies) = lex_with_heredocs(script, false).map_err(|_| {
        "닫히지 않은 따옴표나 명령 치환이 있어 셸이 나누는 대로 읽지 못했어요".to_string()
    })?;
    Ok(simple_commands(toks, &bodies))
}

#[derive(Default)]
struct Simple {
    argv: Vec<Word>,
    /// Redirection targets, here-strings and heredoc substitution carriers.
    operands: Vec<Word>,
    /// What a here-string or heredoc gives the command on stdin.
    input: Vec<String>,
    after_pipe: bool,
}

fn simple_commands(toks: Vec<Tok>, bodies: &[HeredocBody]) -> Vec<Simple> {
    let mut commands = vec![Simple::default()];
    let mut operand_of = None;
    for (at, tok) in toks.into_iter().enumerate() {
        let Some(current) = commands.last_mut() else {
            break;
        };
        match tok {
            Tok::Sep(sep) => {
                operand_of = None;
                commands.push(Simple {
                    after_pipe: sep == Sep::Pipe,
                    ..Simple::default()
                });
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

fn piped_into(commands: &[Simple], at: usize) -> &[Simple] {
    let start = commands[..=at]
        .iter()
        .rposition(|command| !command.after_pipe)
        .unwrap_or(0);
    &commands[start..at]
}

fn judge_command(command: &Simple, piped: &[Simple], depth: usize) -> Option<String> {
    let argv: Vec<String> = command.argv.iter().map(|w| w.text.clone()).collect();
    if let Some(reason) = kanban_args(&argv).and_then(judge) {
        return Some(reason);
    }
    match scripts_run_by(command, piped) {
        Ok(scripts) => scripts
            .iter()
            .find_map(|script| judge_script(script, depth + 1)),
        Err(reason) => Some(reason),
    }
}

fn scripts_run_by(command: &Simple, piped: &[Simple]) -> Result<Vec<String>, String> {
    let words = || command.argv.iter().chain(&command.operands);
    let mut scripts: Vec<String> = words()
        .flat_map(|word| word.subs.iter().cloned())
        .chain(words().filter_map(process_body).map(str::to_string))
        .collect();
    let shell = command
        .argv
        .iter()
        .position(|word| runs_shell(program(word)));
    let script_reader = command
        .argv
        .first()
        .is_some_and(|word| SCRIPT_READERS.contains(&program(word)));
    match shell.map(|at| shell_script(&command.argv[at + 1..])) {
        Some(ShellScript::Inline(script)) => scripts.push(script.text.clone()),
        Some(ShellScript::Input) => {
            scripts.extend(command.input.iter().cloned());
            scripts.extend(output_of(piped));
            scripts.extend(process_outputs(command)?);
        }
        Some(ShellScript::Missing) => {}
        None if script_reader => scripts.extend(process_outputs(command)?),
        None => {}
    }
    Ok(scripts)
}

fn program(word: &Word) -> &str {
    word.text.rsplit('/').next().unwrap_or("")
}

fn runs_shell(name: &str) -> bool {
    // fish stays out of the shared `is_shell`: the git classifier would then
    // read fish scripts and stdin as bash ones.
    is_shell(name) || name == "fish"
}

fn process_body(word: &Word) -> Option<&str> {
    if word.quoted || word.dyn_at != Some(0) {
        return None;
    }
    word.text
        .strip_prefix("<(")
        .or_else(|| word.text.strip_prefix(">("))
        .and_then(|rest| rest.strip_suffix(')'))
}

fn process_outputs(command: &Simple) -> Result<Vec<String>, String> {
    let mut scripts = Vec::new();
    for body in command.argv.iter().filter_map(process_body) {
        scripts.extend(output_of(&parse(body)?));
    }
    Ok(scripts)
}

fn output_of(producers: &[Simple]) -> Vec<String> {
    producers
        .iter()
        .flat_map(|producer| {
            let words: Vec<String> = producer
                .argv
                .iter()
                .skip(1)
                .map(|w| w.text.clone())
                .collect();
            let joined = words.join(" ");
            words
                .into_iter()
                .chain([joined])
                .chain(producer.input.iter().cloned())
        })
        .collect()
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

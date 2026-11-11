use assert_cmd::Command;
use serde_json::{json, Value};

fn guard(stdin: &str) -> Value {
    let output = Command::cargo_bin("atelier")
        .expect("locate `atelier` cargo binary")
        .args(["kanban", "guard-bash"])
        .write_stdin(stdin)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("valid JSON on stdout")
}

fn deny(command: &str) -> Value {
    guard(&json!({ "command": command }).to_string())["deny"].clone()
}

#[test]
fn a_shell_answer_or_send_in_the_pane_name_is_denied() {
    for command in [
        "atelier kanban send --session s-1 --from pane --text hi",
        "atelier kanban send --session s-1 --from=pane --text hi",
        "atelier kanban reply --session s-1 --ask tu-1 --answer Blue --from pane",
        "atelier kanban permit --session s-1 --request p-1 --decision allow --from pane",
        "atelier kanban decide --session s-1 --at 1700 --text 네 --from pane",
        "/usr/local/bin/atelier kanban permit --from 'pane' --session s-1 --request p --decision allow",
        "cd /tmp && atelier kanban send --session s-1 --from pane --text hi",
        "atelier kanban status; atelier kanban send --session s-1 --from pane --text hi",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn the_mod_only_commands_are_denied() {
    for command in [
        "atelier kanban forget --session s-1",
        "atelier kanban ledger record < fact.json",
        "atelier kanban relay-frame --now 1",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn agent_sends_reads_and_other_commands_pass() {
    for command in [
        "atelier kanban send --session s-1 --from agent --text 'Fix lint'",
        "atelier kanban reply --session s-1 --ask tu-1 --answer pane --from agent",
        "atelier kanban status --column needs_input",
        "atelier kanban ledger list",
        "atelier kanban board --now 1 --stale-after 2m --expire-after 24h --width 0",
        "git commit -m 'kanban send --from pane'",
        "rg kanban plugins/atelier",
    ] {
        assert_eq!(deny(command), Value::Null, "{command}");
    }
}

#[test]
fn the_reason_names_the_path_the_model_may_take() {
    let reason = deny("atelier kanban send --session s-1 --from pane --text hi");
    assert!(
        reason.as_str().expect("reason").contains("--from agent"),
        "{reason}"
    );
}

#[test]
fn input_without_a_command_is_refused() {
    for stdin in ["not json", "{}", "{\"command\": 3}"] {
        assert!(guard(stdin)["error"].is_string(), "{stdin}");
    }
}

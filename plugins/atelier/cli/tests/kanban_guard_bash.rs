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
        "atelier kanban send > /dev/null --session s-1 --from pane --text hi",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn a_pane_answer_inside_a_substitution_or_a_nested_shell_is_denied() {
    for command in [
        "$(atelier kanban send --session s-1 --from pane --text hi)",
        "x=$(atelier kanban send --session s-1 --from pane --text hi)",
        "`atelier kanban send --session s-1 --from pane --text hi`",
        "echo \"$(atelier kanban permit --session s-1 --request p-1 --decision allow --from pane)\"",
        "echo \"$(echo $(atelier kanban send --session s-1 --from pane --text hi))\"",
        "sh -c 'atelier kanban send --session s-1 --from pane --text hi'",
        "bash -c \"atelier kanban reply --session s-1 --ask tu-1 --answer Blue --from pane\"",
        "zsh -c \"atelier kanban decide --session s-1 --at 1700 --text 네 --from pane\"",
        "/bin/bash -lc 'cd /tmp && atelier kanban send --session s-1 --from pane --text hi'",
        "env bash -c 'atelier kanban forget --session s-1'",
        "bash -c \"sh -c 'atelier kanban send --session s-1 --from pane --text hi'\"",
        "bash <<EOF\natelier kanban decide --session s-1 --at 1700 --text 네 --from pane\nEOF",
        "sh <<'EOF'\natelier kanban send --session s-1 --from pane --text hi\nEOF",
        "cat <<EOF\n$(atelier kanban send --session s-1 --from pane --text hi)\nEOF",
        "bash <<< 'atelier kanban send --session s-1 --from pane --text hi'",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn a_pane_answer_in_a_fish_script_is_denied() {
    for command in [
        "fish -c 'atelier kanban send --session s-1 --from pane --text hi'",
        "fish <<EOF\natelier kanban send --session s-1 --from pane --text hi\nEOF",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn a_pane_answer_piped_into_a_shell_is_denied() {
    for command in [
        "echo 'atelier kanban send --session s-1 --from pane --text hi' | bash",
        "printf '%s\\n' 'atelier kanban send --session s-1 --from pane --text hi' | sh",
        "echo 'atelier kanban send --session s-1 --from pane --text hi' | bash -s",
        "echo atelier kanban send --session s-1 --from pane --text hi | zsh",
        "echo 'atelier kanban forget --session s-1' | tee log | dash",
        "echo 'atelier kanban send --session s-1 --from pane --text hi' | fish",
        "cat <<EOF | bash\natelier kanban send --session s-1 --from pane --text hi\nEOF",
        "cat <<< 'atelier kanban send --session s-1 --from pane --text hi' | /bin/ksh",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn words_piped_into_a_shell_that_do_not_parse_are_denied() {
    let command = "echo \"atelier kanban send --session s-1 --text 'unclosed\" | bash";

    assert!(deny(command).is_string(), "{command}");
}

#[test]
fn a_pane_answer_inside_a_process_substitution_is_denied() {
    for command in [
        "bash <(echo 'atelier kanban send --session s-1 --from pane --text hi')",
        "source <(echo 'atelier kanban send --session s-1 --from pane --text hi')",
        ". <(echo 'atelier kanban send --session s-1 --from pane --text hi')",
        "diff <(atelier kanban send --session s-1 --from pane --text hi) x",
        "echo hi | tee >(atelier kanban forget --session s-1)",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn text_naming_the_kanban_that_no_shell_runs_passes() {
    for command in [
        "git commit -m \"fix kanban send --from pane wording\"",
        "grep -r \"kanban\" src",
        "echo hi | bash",
        "echo 'atelier kanban status' | bash",
        "echo 'atelier kanban send --session s-1 --from pane --text hi' | grep kanban",
        "bash -c 'atelier kanban status'",
        "atelier kanban send --session s-1 --from agent --text hi",
        "echo 'atelier kanban send --session s-1 --from agent --text hi' | bash",
        "diff <(atelier kanban status) <(atelier kanban ledger list)",
        "bash <(echo 'atelier kanban status')",
    ] {
        assert_eq!(deny(command), Value::Null, "{command}");
    }
}

#[test]
fn shells_nested_past_the_depth_limit_or_left_unclosed_are_denied() {
    for command in [
        "echo $(echo $(echo $(echo $(atelier kanban status))))",
        "atelier kanban send --session s-1 --from agent --text 'unclosed",
        "echo $(atelier kanban status",
    ] {
        assert!(deny(command).is_string(), "{command}");
    }
}

#[test]
fn agent_sends_inside_a_substitution_or_a_nested_shell_pass() {
    for command in [
        "out=$(atelier kanban send --session s-1 --from agent --text hi)",
        "bash -c 'atelier kanban status --column needs_input'",
        "echo \"$(atelier kanban ledger list)\"",
        "echo $(echo $(echo $(atelier kanban status)))",
        "bash <<EOF\natelier kanban status\nEOF",
        "cat <<'EOF'\n$(atelier kanban send --session s-1 --from pane --text hi)\nEOF",
    ] {
        assert_eq!(deny(command), Value::Null, "{command}");
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
fn the_inbox_commands_only_the_mode_runs_are_denied() {
    for command in [
        "atelier kanban inbox take --session s-1 --now 1 --ttl 60s",
        "atelier kanban inbox take --session s-1 --now 1 --ttl 60s --waits-stdin < waits.json",
        "atelier kanban inbox wait-permit --session s-1 --request p-1 --now 1 --ttl 60s --for 2s",
        "atelier kanban inbox dir --session s-1",
        "atelier kanban inbox clear --session s-1",
        "bash -c 'atelier kanban inbox take --session s-1 --now 1 --ttl 60s'",
    ] {
        let reason = deny(command);
        assert!(
            reason.as_str().is_some_and(|r| r.contains("모드만")),
            "{command} → {reason}"
        );
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

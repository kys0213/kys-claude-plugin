//! Black-box tests for channel config resolution: precedence (env override →
//! project → global home config), the exec-declaration schema, and
//! missing/malformed config resolving to no channels (no-op), never an error.

mod notify_support;

use atelier::notify::config::{resolve_channels, ChannelSpec, DEFAULT_TIMEOUT_SECS, ENV_CONFIG};
use notify_support::{env, fs};

#[test]
fn parses_exec_declaration_with_defaults() {
    let file = r#"{"channels":[
        {"name":"slack","exec":["curl","--data","@-","https://hooks.slack.com/x"],"stdin":"{\"text\": {text#json}}"},
        {"exec":["notify-send","{title}","{body}"],"timeoutSeconds":2,"events":["notification"]}
    ]}"#;
    let channels = resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
    );

    assert_eq!(
        channels,
        vec![
            ChannelSpec {
                name: "slack".to_string(),
                exec: vec![
                    "curl".to_string(),
                    "--data".to_string(),
                    "@-".to_string(),
                    "https://hooks.slack.com/x".to_string(),
                ],
                stdin: Some("{\"text\": {text#json}}".to_string()),
                timeout_secs: DEFAULT_TIMEOUT_SECS,
                events: None,
            },
            ChannelSpec {
                // name defaults to the command
                name: "notify-send".to_string(),
                exec: vec![
                    "notify-send".to_string(),
                    "{title}".to_string(),
                    "{body}".to_string(),
                ],
                stdin: None,
                timeout_secs: 2,
                events: Some(vec!["notification".to_string()]),
            },
        ]
    );
}

#[test]
fn entries_without_exec_are_skipped() {
    let file = r#"{"channels":[
        {"name":"broken"},
        {"name":"empty","exec":[]},
        {"name":"ok","exec":["true"]}
    ]}"#;
    let channels = resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", file)]),
        "/proj",
    );
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].name, "ok");
}

#[test]
fn env_config_path_overrides_file_lookup() {
    let over = r#"{"channels":[{"exec":["from-env-path"]}]}"#;
    let proj = r#"{"channels":[{"exec":["from-project"]}]}"#;
    let channels = resolve_channels(
        &env(&[(ENV_CONFIG, "/custom/notify.json")]),
        &fs(&[
            ("/custom/notify.json", over),
            ("/proj/.claude/atelier-notify.json", proj),
        ]),
        "/proj",
    );
    assert_eq!(channels[0].name, "from-env-path");

    // Env path supports ~/ expansion.
    let channels = resolve_channels(
        &env(&[(ENV_CONFIG, "~/notify.json"), ("HOME", "/home/u")]),
        &fs(&[("/home/u/notify.json", over)]),
        "/proj",
    );
    assert_eq!(channels[0].name, "from-env-path");

    // Explicit override pointing at a missing file → no channels (no-op),
    // NOT a fallback to project/global config.
    let channels = resolve_channels(
        &env(&[(ENV_CONFIG, "/missing.json")]),
        &fs(&[("/proj/.claude/atelier-notify.json", proj)]),
        "/proj",
    );
    assert!(channels.is_empty());
}

#[test]
fn falls_back_to_global_home_config_when_project_config_missing() {
    let global = r#"{"channels":[{"exec":["from-global"]}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[("/home/u/.claude/atelier-notify.json", global)]),
        "/proj",
    );
    assert_eq!(channels[0].name, "from-global");

    // Project config wins over the global one when both exist.
    let project = r#"{"channels":[{"exec":["from-project"]}]}"#;
    let channels = resolve_channels(
        &env(&[("HOME", "/home/u")]),
        &fs(&[
            ("/proj/.claude/atelier-notify.json", project),
            ("/home/u/.claude/atelier-notify.json", global),
        ]),
        "/proj",
    );
    assert_eq!(channels[0].name, "from-project");
}

#[test]
fn missing_and_malformed_config_resolve_to_no_channels() {
    assert!(resolve_channels(&env(&[]), &fs(&[]), "/proj").is_empty());
    assert!(resolve_channels(
        &env(&[]),
        &fs(&[("/proj/.claude/atelier-notify.json", "{ broken")]),
        "/proj",
    )
    .is_empty());
}

#[test]
fn events_filter_matches_kinds() {
    let spec = ChannelSpec {
        name: "n".to_string(),
        exec: vec!["true".to_string()],
        stdin: None,
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        events: Some(vec!["notification".to_string()]),
    };
    assert!(spec.accepts("notification"));
    assert!(!spec.accepts("ask_user_question"));

    let all = ChannelSpec {
        events: None,
        ..spec.clone()
    };
    assert!(all.accepts("ask_user_question"));
}

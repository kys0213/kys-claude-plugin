use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

mod kanban_support;
use kanban_support::{home_deref, now_ms, stdout_json, Home};

struct World {
    home: Home,
    bin: TempDir,
}

home_deref!(World);

impl World {
    fn new() -> World {
        World {
            home: Home::default(),
            bin: TempDir::new().expect("bin dir"),
        }
    }

    fn kanban(&self, args: &[&str]) -> Value {
        let path = format!(
            "{}:{}",
            self.bin.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        stdout_json(self.command().args(args).env("PATH", path))
    }

    fn send_pane(&self, text: &str) -> Value {
        self.kanban(&["send", "--session", "s-1", "--from", "pane", "--text", text])
    }

    fn files(&self, session: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.inbox(session))
            .map(|dir| {
                dir.map(|e| e.expect("entry").file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn take(&self, session: &str, turn: Option<&str>) -> Value {
        self.home.take(session, now_ms(), "60s", turn, None)
    }

    /// A fake `claude` that records its argv and cwd, then prints `out`.
    fn fake_claude(&self, out: &str) -> PathBuf {
        let log = self.bin.path().join("claude.log");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$@\" > '{}'\nprintf '%s' '{}'\n",
            log.display(),
            out
        );
        let path = self.bin.path().join("claude");
        fs::write(&path, script).expect("write fake claude");
        let mut perms = fs::metadata(&path).expect("stat").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        fs::set_permissions(&path, perms).expect("chmod");
        log
    }
}

fn age(path: &Path, by: Duration) {
    let file = fs::File::options().write(true).open(path).expect("open");
    file.set_modified(SystemTime::now() - by)
        .expect("set mtime");
}

#[test]
fn inbox_dir_is_under_the_claude_config_dir() {
    let w = World::new();
    let out = w.kanban(&["inbox", "dir", "--session", "s-1"]);
    assert_eq!(out["dir"], json!(w.inbox("s-1").display().to_string()));
}

#[test]
fn a_sent_prompt_is_queued_as_one_finished_file() {
    let w = World::new();
    let out = w.send_pane("Run the tests");
    assert_eq!(out["queued"]["session_id"], "s-1");
    assert_eq!(out["queued"]["kind"], "prompt");
    let files = w.files("s-1");
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with(".json"), "{files:?}");
}

#[test]
fn taking_delivers_a_pane_prompt_as_typed_and_removes_it() {
    let w = World::new();
    w.send_pane("Run the tests");
    let out = w.take("s-1", None);
    assert_eq!(
        out["actions"],
        json!([{ "kind": "prompt", "text": "Run the tests", "body": "Run the tests", "from": "pane" }])
    );
    assert_eq!(out["results"][0]["status"], "delivered");
    assert!(w.files("s-1").is_empty());
    assert_eq!(w.take("s-1", None)["actions"], json!([]));
}

#[test]
fn an_agent_prompt_is_delivered_under_a_header_naming_where_it_came_from() {
    let w = World::new();
    w.kanban(&[
        "send",
        "--session",
        "s-1",
        "--from",
        "agent",
        "--from-label",
        "orchestra",
        "--text",
        "Fix lint",
    ]);
    let action = &w.take("s-1", None)["actions"][0];
    assert_eq!(
        action["text"],
        "[칸반 요청 · orchestra 세션의 오케스트레이터]\n\nFix lint"
    );
    assert_eq!(action["body"], "Fix lint");
    assert_eq!(action["from"], "agent");
}

#[test]
fn a_prompt_arriving_while_a_turn_runs_is_refused_not_queued() {
    let w = World::new();
    w.send_pane("Later");
    let out = w.take("s-1", Some("t-1"));
    assert_eq!(out["actions"], json!([]));
    assert_eq!(out["results"][0]["status"], "busy");
    assert!(w.files("s-1").is_empty());
}

#[test]
fn only_the_first_of_several_prompts_is_delivered() {
    let w = World::new();
    w.send_pane("One");
    w.send_pane("Two");
    let out = w.take("s-1", None);
    assert_eq!(out["actions"].as_array().expect("actions").len(), 1);
    assert_eq!(out["actions"][0]["body"], "One");
    assert_eq!(out["results"][1]["status"], "busy");
}

#[test]
fn a_prompt_older_than_the_ttl_is_dropped() {
    let w = World::new();
    w.send_pane("Old");
    let file = w.inbox("s-1").join(&w.files("s-1")[0]);
    age(&file, Duration::from_secs(120));
    let out = w.take("s-1", None);
    assert_eq!(out["actions"], json!([]));
    assert_eq!(out["results"][0]["status"], "expired");
}

#[test]
fn a_stop_reaches_only_the_turn_it_was_meant_for() {
    let w = World::new();
    w.kanban(&["stop", "--session", "s-1", "--turn", "t-1"]);
    let out = w.take("s-1", Some("t-1"));
    assert_eq!(
        out["actions"],
        json!([{ "kind": "stop", "turn_id": "t-1" }])
    );
    assert_eq!(out["results"][0]["status"], "delivered");

    w.kanban(&["stop", "--session", "s-1", "--turn", "t-1"]);
    let out = w.take("s-1", Some("t-2"));
    assert_eq!(out["actions"], json!([]));
    assert_eq!(out["results"][0]["status"], "stale");
}

#[test]
fn a_stop_taken_while_no_turn_runs_is_stale() {
    let w = World::new();
    w.kanban(&["stop", "--session", "s-1", "--turn", "t-1"]);

    let out = w.take("s-1", None);

    assert_eq!(out["actions"], json!([]));
    assert_eq!(out["results"][0]["status"], "stale");
}

#[test]
fn an_unreadable_request_is_dropped_as_invalid() {
    let w = World::new();
    w.leave_raw("s-1", "1-1.json", "not json");
    let out = w.take("s-1", None);
    assert_eq!(out["results"][0]["status"], "invalid");
    assert!(w.files("s-1").is_empty());
}

#[test]
fn taking_an_empty_or_missing_inbox_gives_nothing() {
    let w = World::new();
    assert_eq!(
        w.take("never", None),
        json!({ "actions": [], "results": [], "used_nonces": [] })
    );
}

#[test]
fn clearing_removes_a_session_inbox() {
    let w = World::new();
    w.send_pane("x");
    assert_eq!(
        w.kanban(&["inbox", "clear", "--session", "s-1"]),
        json!({ "cleared": "s-1" })
    );
    assert!(!w.inbox("s-1").exists());
}

#[test]
fn requests_are_refused_for_bad_ids_and_empty_text() {
    let w = World::new();
    for args in [
        vec![
            "send",
            "--session",
            "../escape",
            "--from",
            "pane",
            "--text",
            "x",
        ],
        vec![
            "send",
            "--session",
            "s-1",
            "--from",
            "pane",
            "--text",
            "   ",
        ],
        vec!["stop", "--session", "s/1", "--turn", "t"],
        vec!["inbox", "dir", "--session", ""],
    ] {
        let out = w.kanban(&args);
        assert!(out["error"].is_string(), "{args:?} → {out}");
    }
    assert!(!w.config().join("atelier").exists());
}

#[test]
fn a_sender_other_than_pane_or_agent_is_refused_before_anything_is_written() {
    let w = World::new();
    for args in [
        vec!["send", "--session", "s-1", "--text", "x"],
        vec![
            "reply",
            "--session",
            "s-1",
            "--ask",
            "tu-1",
            "--answer",
            "a",
        ],
        vec!["decide", "--session", "s-1", "--at", "1700", "--text", "네"],
        vec![
            "permit",
            "--session",
            "s-1",
            "--request",
            "p-1",
            "--decision",
            "allow",
        ],
    ] {
        let output = w
            .command()
            .args(&args)
            .args(["--from", "robot"])
            .output()
            .expect("run atelier");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("pane 또는 agent"),
            "{args:?}"
        );
    }
    assert!(!w.config().join("atelier").exists());
}

fn git_repo() -> TempDir {
    let dir = TempDir::new().expect("repo dir");
    let ok = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git init")
        .success();
    assert!(ok);
    dir
}

#[test]
fn spawn_starts_a_background_session_at_a_repository_root_under_a_header() {
    let w = World::new();
    let log = w.fake_claude("backgrounded · 6d652fd9\n  claude agents   list sessions\n");
    let repo = git_repo();
    let root = fs::canonicalize(repo.path()).expect("canonical");
    let root = root.display().to_string();
    let out = w.kanban(&[
        "spawn",
        "--cwd",
        &root,
        "--text",
        "Add a health check",
        "--worktree",
        "health-1",
        "--from-label",
        "orchestra",
    ]);
    assert_eq!(
        out,
        json!({ "spawned": { "short_id": "6d652fd9", "cwd": root, "worktree": "health-1" } })
    );
    let argv: Vec<String> = fs::read_to_string(log)
        .expect("claude log")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(argv[0], root);
    assert_eq!(argv[1..4], ["--bg", "--worktree", "health-1"]);
    assert_eq!(
        argv[4..].join("\n"),
        "[칸반 요청 · orchestra 세션의 오케스트레이터]\n\nAdd a health check"
    );
}

#[test]
fn spawn_refuses_anything_but_a_main_working_tree_root() {
    let w = World::new();
    w.fake_claude("backgrounded · 1\n");
    let repo = git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir_all(&sub).expect("sub");
    let plain = TempDir::new().expect("plain");
    for cwd in [
        sub.display().to_string(),
        plain.path().display().to_string(),
        "relative/dir".to_string(),
    ] {
        let out = w.kanban(&["spawn", "--cwd", &cwd, "--text", "x"]);
        assert!(out["error"].is_string(), "{cwd} → {out}");
    }
    assert!(!w.bin.path().join("claude.log").exists());
}

#[test]
fn spawn_refuses_a_worktree_name_that_is_not_a_plain_slug() {
    let w = World::new();
    w.fake_claude("backgrounded · 1\n");
    let repo = git_repo();
    let root = repo.path().display().to_string();
    for name in ["--dangerously-skip-permissions", "Up", "a/b", ""] {
        let out = w.kanban(&["spawn", "--cwd", &root, "--text", "x", "--worktree", name]);
        assert!(out["error"].is_string(), "{name} → {out}");
    }
}

#[test]
fn spawn_reports_a_launcher_that_did_not_background_a_session() {
    let w = World::new();
    w.fake_claude("error: not logged in");
    let repo = git_repo();
    let out = w.kanban(&[
        "spawn",
        "--cwd",
        &repo.path().display().to_string(),
        "--text",
        "x",
    ]);
    assert!(
        out["error"]
            .as_str()
            .expect("error")
            .contains("not logged in"),
        "{out}"
    );
}

#[test]
fn a_permit_lands_beside_the_requests_and_is_taken_by_its_permission() {
    let w = World::new();
    w.kanban(&[
        "permit",
        "--session",
        "s-1",
        "--request",
        "t-1-5",
        "--decision",
        "deny",
        "--from",
        "pane",
    ]);
    assert!(w.inbox("s-1").join("permits").join("t-1-5.json").exists());
    assert_eq!(
        w.take("s-1", None),
        json!({ "actions": [], "results": [], "used_nonces": [] })
    );

    let out = w.wait_permit("s-1", "t-1-5", now_ms(), "60s", None);
    assert_eq!(
        out,
        json!({ "decision": "deny", "from": "pane", "via": "terminal", "used_nonces": [] })
    );
    assert!(!w.inbox("s-1").join("permits").join("t-1-5.json").exists());
}

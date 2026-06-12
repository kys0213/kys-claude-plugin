//! `autopilot hook` — PreToolUse guards ported from the bash hooks (#776).
//!
//! Per `.claude/rules/tool-layer-boundary.md`, the deterministic logic that
//! used to live in `hooks/guard-pr-base.sh` and `hooks/protect-stagnation.sh`
//! belongs in CLI subcommands: stdin carries the PreToolUse JSON payload,
//! args/env carry configuration, and exit code 2 signals a block (reason on
//! stderr). The CLI edge in `run.rs` only reads stdin/env and maps the
//! returned [`HookDecision`] to the process exit code.

use std::io::Write as _;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;
use serde_json::Value;

use crate::autopilot::cmd::check::stagnation::{
    StagnationConfig, StagnationService, EXIT_ESCALATE, EXIT_STAGNATION,
};
use crate::autopilot::domain::TaskId;
use crate::autopilot::fs::FsOps;
use crate::autopilot::ports::task_store::TaskStore;

/// PreToolUse hook payload fields the autopilot guards consume. Like
/// `git::commands::guard::HookPayload`, `parse` is swallow-all: any JSON
/// failure yields all-`None` so a malformed payload can never block a tool
/// call by accident.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookToolPayload {
    /// `tool_name` from the payload (e.g. `Bash`,
    /// `mcp__github__create_pull_request`).
    pub tool_name: Option<String>,
    /// `tool_input.command` (Bash calls).
    pub command: Option<String>,
    /// `tool_input.base` (MCP create_pull_request calls).
    pub pr_base: Option<String>,
}

impl HookToolPayload {
    pub fn parse(raw: &str) -> HookToolPayload {
        match serde_json::from_str::<Value>(raw) {
            Ok(v) => HookToolPayload {
                tool_name: v["tool_name"].as_str().map(|s| s.to_string()),
                command: v["tool_input"]["command"].as_str().map(|s| s.to_string()),
                pr_base: v["tool_input"]["base"].as_str().map(|s| s.to_string()),
            },
            Err(_) => HookToolPayload::default(),
        }
    }
}

/// Allow/block outcome of a hook guard. Mirrors the PreToolUse contract:
/// exit 0 lets the tool call through, exit 2 blocks it (reason on stderr).
#[derive(Debug, Clone)]
pub struct HookDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl HookDecision {
    fn allow() -> Self {
        Self {
            allowed: true,
            reason: None,
        }
    }

    fn block(reason: String) -> Self {
        Self {
            allowed: false,
            reason: Some(reason),
        }
    }

    pub fn exit_code(&self) -> i32 {
        if self.allowed {
            0
        } else {
            2
        }
    }
}

// ── guard-pr-base ──────────────────────────────────────────────────────

/// Autopilot markdown config fields the PR-base guard consumes, parsed from
/// the `github-autopilot.local.md` YAML frontmatter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PrBaseConfig {
    work_branch: Option<String>,
    branch_strategy: Option<String>,
}

/// Parses `work_branch` / `branch_strategy` from the frontmatter block
/// (between the first and second `---` lines), mirroring the awk parser in
/// the original bash hook: quotes are stripped, empty values count as unset,
/// and keys outside the frontmatter are ignored.
fn parse_pr_base_config(content: &str) -> PrBaseConfig {
    let mut cfg = PrBaseConfig::default();
    let mut fences = 0;
    for line in content.lines() {
        if line.trim_end() == "---" {
            fences += 1;
            if fences >= 2 {
                break;
            }
            continue;
        }
        if fences != 1 {
            continue;
        }
        for (key, slot) in [
            ("work_branch:", &mut cfg.work_branch),
            ("branch_strategy:", &mut cfg.branch_strategy),
        ] {
            if let Some(raw) = line.strip_prefix(key) {
                let val = raw.trim().trim_matches(|c| c == '"' || c == '\'').trim();
                if !val.is_empty() {
                    *slot = Some(val.to_string());
                }
            }
        }
    }
    cfg
}

/// Resolves the expected PR base from the config. Priority: `work_branch` >
/// `branch_strategy`. An unrecognized non-empty strategy is an explicit
/// error (#758) — the bash hook silently treated typos like
/// `draft-develup-main` as `draft-main`, masking the misconfiguration.
fn expected_base(cfg: &PrBaseConfig) -> Result<String, String> {
    if let Some(wb) = &cfg.work_branch {
        return Ok(wb.clone());
    }
    match cfg.branch_strategy.as_deref() {
        Some("draft-develop-main") => Ok("develop".to_string()),
        Some("draft-main") | None => Ok("main".to_string()),
        Some(other) => Err(format!(
            "BLOCKED: unrecognized branch_strategy '{other}' in github-autopilot.local.md\n  supported: draft-main, draft-develop-main\n설정 오타를 수정하세요 — 'main'으로의 silent fallback은 비활성화되었습니다."
        )),
    }
}

static GH_PR_CREATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgh\s+pr\s+create\b").unwrap());
static PR_BASE_FLAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"--base[= ]\s*['"]?([^\s'"]+)"#).unwrap());

/// Extracts the base branch the tool call is about to use, or `None` when
/// the call doesn't create a PR with an explicit base (→ allow).
fn extract_actual_base(payload: &HookToolPayload) -> Option<String> {
    match payload.tool_name.as_deref() {
        Some("mcp__github__create_pull_request") => payload.pr_base.clone(),
        Some("Bash") => {
            let cmd = payload.command.as_deref()?;
            if !GH_PR_CREATE.is_match(cmd) {
                return None;
            }
            PR_BASE_FLAG
                .captures(cmd)
                .map(|c| c.get(1).unwrap().as_str().to_string())
        }
        _ => None,
    }
}

/// PR-base guard: blocks PR creation whose base branch deviates from the
/// autopilot config. Missing config means "not an autopilot project" →
/// allow. Config problems (#758) only surface when the call actually
/// creates a PR with an explicit base, matching the bash hook's scope.
pub fn guard_pr_base(
    fs: &dyn FsOps,
    project_dir: &Path,
    config_filename: &str,
    payload: &HookToolPayload,
) -> HookDecision {
    let actual = match extract_actual_base(payload) {
        Some(base) => base,
        None => return HookDecision::allow(),
    };

    let config_path = project_dir.join(config_filename);
    let content = match fs.read_file(&config_path) {
        Ok(c) => c,
        Err(_) => return HookDecision::allow(),
    };

    let expected = match expected_base(&parse_pr_base_config(&content)) {
        Ok(b) => b,
        Err(reason) => return HookDecision::block(reason),
    };

    if actual != expected {
        return HookDecision::block(format!(
            "BLOCKED: PR base branch mismatch\n  expected: {expected} (from {config_filename})\n  actual:   {actual}\n\n{config_filename}의 work_branch 또는 branch_strategy 설정을 확인하세요."
        ));
    }
    HookDecision::allow()
}

// ── protect-stagnation ─────────────────────────────────────────────────

static CLAIM_CMD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[\s;&|])autopilot\s+task\s+claim(?:\s|$)").unwrap());
static CLAIM_POSITIONAL_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"autopilot\s+task\s+claim\s+([a-f0-9]+)\b").unwrap());
static CLAIM_TASK_FLAG_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--task\s*[= ]\s*([a-f0-9]+)\b").unwrap());

/// Extracts the task id from an `autopilot task claim` command, or `None`
/// when the command is not a claim or carries no task id (the current claim
/// surface is `task claim --epic <name>`, which selects the next ready task
/// server-side — there is no id to pre-check, so the guard passes).
pub fn extract_claim_task_id(command: Option<&str>) -> Option<String> {
    let cmd = command?;
    if !CLAIM_CMD.is_match(cmd) {
        return None;
    }
    CLAIM_POSITIONAL_ID
        .captures(cmd)
        .or_else(|| CLAIM_TASK_FLAG_ID.captures(cmd))
        .map(|c| c.get(1).unwrap().as_str().to_string())
}

/// Runs the ledger stagnation check for `task_id` and converts the banded
/// result into a hook decision:
///
/// - exit 0 (ok) → allow
/// - exit 4 (stagnation) → block with the redirect prompt (spec §3.11)
/// - exit 5 (escalate) → block with a human-review prompt
///
/// The bash hook attempted `autopilot task escalate <id> --reason ...` on
/// the escalate band, but that call never matched the CLI surface
/// (`task escalate` requires `--issue <N>`) and always fell through to its
/// warning path — so the port records no event and instructs the operator
/// to escalate manually, preserving the de-facto behavior.
///
/// Errors (e.g. unknown task id) propagate; the CLI edge maps them to
/// allow, matching the bash hook's best-effort `*) exit 0` arm.
pub fn protect_stagnation_check(
    store: &dyn TaskStore,
    config: &StagnationConfig,
    task_id: &str,
) -> Result<HookDecision> {
    let svc = StagnationService::new(store);
    let mut buf: Vec<u8> = Vec::new();
    let exit = svc.check(&TaskId::from_raw(task_id), config, &mut buf)?;
    let report: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);

    match exit {
        EXIT_STAGNATION => Ok(HookDecision::block(redirect_prompt(task_id, &report))),
        EXIT_ESCALATE => Ok(HookDecision::block(escalate_prompt(task_id))),
        _ => Ok(HookDecision::allow()),
    }
}

/// Formats the stagnation redirect prompt from the check report, mirroring
/// the jq template in the bash hook.
fn redirect_prompt(task_id: &str, report: &Value) -> String {
    let mut out = Vec::new();
    let similar = report["similar_tasks"].as_array().map_or(0, |a| a.len());
    let _ = writeln!(out, "[STAGNATION DETECTED] task {task_id}");
    let _ = writeln!(
        out,
        "This task's territory is exhausted — {similar} similar tasks have failed before:"
    );
    if let Some(paths) = non_empty_str_list(&report["pattern"]["shared_paths"]) {
        let _ = writeln!(out, "  - same paths: {}", paths.join(", "));
    }
    if let Some(cats) = non_empty_str_list(&report["pattern"]["common_failure_categories"]) {
        let _ = writeln!(out, "  - same failure category: {}", cats.join(", "));
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "DO NOT proceed with the same approach. Try one of:");
    let _ = writeln!(out, "  1. Different file area entirely");
    let persona = report["recommended_persona"].as_str();
    match persona {
        Some(p) => {
            let _ = writeln!(
                out,
                "  2. Persona shift: \"{p}\" — challenge the underlying assumption"
            );
            let _ = writeln!(out);
            let _ = writeln!(out, "Recommended persona: {p}");
        }
        None => {
            let _ = writeln!(out, "  2. Different approach to the same problem");
        }
    }
    String::from_utf8_lossy(&out).trim_end().to_string()
}

fn escalate_prompt(task_id: &str) -> String {
    format!(
        "[STAGNATION ESCALATED] task {task_id}\n이 영역은 자동 retry 한도(N>=5)를 넘었습니다 — 사람의 검토가 필요합니다.\nDO NOT retry. HITL 이슈 생성 후 'atelier autopilot task escalate {task_id} --issue <N>' 으로 기록하세요."
    )
}

fn non_empty_str_list(v: &Value) -> Option<Vec<&str>> {
    let items: Vec<&str> = v.as_array()?.iter().filter_map(|s| s.as_str()).collect();
    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

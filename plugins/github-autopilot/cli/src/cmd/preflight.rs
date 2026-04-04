use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::fs::FsOps;
use crate::gh::GhOps;
use crate::git::GitOps;

#[derive(Serialize)]
struct CheckResult {
    check: String,
    status: String,
    detail: String,
}

impl CheckResult {
    fn pass(check: &str, detail: &str) -> Self {
        Self {
            check: check.to_string(),
            status: "PASS".to_string(),
            detail: detail.to_string(),
        }
    }
    fn warn(check: &str, detail: &str) -> Self {
        Self {
            check: check.to_string(),
            status: "WARN".to_string(),
            detail: detail.to_string(),
        }
    }
    fn fail(check: &str, detail: &str) -> Self {
        Self {
            check: check.to_string(),
            status: "FAIL".to_string(),
            detail: detail.to_string(),
        }
    }
}

/// Run all preflight checks. Returns exit code 0 (no FAIL) or 1 (any FAIL).
pub fn run(
    gh: &dyn GhOps,
    git: &dyn GitOps,
    fs: &dyn FsOps,
    config_path: &str,
    repo_root: &Path,
) -> Result<i32> {
    let mut results = Vec::new();

    // A-1. CLAUDE.md existence and content
    check_claude_md(fs, repo_root, &mut results);

    // A-2. Rules coverage
    check_rules_coverage(fs, repo_root, &mut results);

    // B-1. gh auth
    check_gh_auth(gh, &mut results);

    // B-2. Guard PR base hook
    check_guard_hook(fs, repo_root, &mut results);

    // B-3. Quality gate command
    check_quality_gate(fs, config_path, &mut results);

    // B-4. Git remote
    check_git_remote(git, &mut results);

    // C. Spec files existence
    check_spec_files(fs, config_path, repo_root, &mut results);

    let has_fail = results.iter().any(|r| r.status == "FAIL");
    println!("{}", serde_json::to_string(&results)?);

    if has_fail {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn check_claude_md(fs: &dyn FsOps, repo_root: &Path, results: &mut Vec<CheckResult>) {
    let path = repo_root.join("CLAUDE.md");
    if !fs.file_exists(&path) {
        results.push(CheckResult::fail("CLAUDE.md", "CLAUDE.md not found"));
        return;
    }

    let content = match fs.read_file(&path) {
        Ok(c) => c,
        Err(_) => {
            results.push(CheckResult::fail("CLAUDE.md", "failed to read CLAUDE.md"));
            return;
        }
    };

    let has_tree = content.contains("├")
        || content.contains("└")
        || content.contains("directory")
        || content.contains("structure");
    let has_build = ["cargo", "npm", "go ", "make", "pytest", "jest", "gradle"]
        .iter()
        .any(|kw| content.contains(kw));
    let has_convention = ["stack", "convention", "principle", "컨벤션", "원칙"]
        .iter()
        .any(|kw| content.contains(kw));

    let mut missing = Vec::new();
    if !has_tree {
        missing.push("file tree");
    }
    if !has_build {
        missing.push("build/test commands");
    }
    if !has_convention {
        missing.push("conventions");
    }

    if missing.is_empty() {
        results.push(CheckResult::pass(
            "CLAUDE.md",
            "file tree, build commands, conventions present",
        ));
    } else if missing.len() < 3 {
        results.push(CheckResult::warn(
            "CLAUDE.md",
            &format!("missing: {}", missing.join(", ")),
        ));
    } else {
        results.push(CheckResult::fail(
            "CLAUDE.md",
            &format!("missing all: {}", missing.join(", ")),
        ));
    }
}

fn check_rules_coverage(fs: &dyn FsOps, repo_root: &Path, results: &mut Vec<CheckResult>) {
    let rules_dir = repo_root.join(".claude/rules");
    let rule_files = match fs.list_files(&rules_dir, "md") {
        Ok(f) => f,
        Err(_) => {
            results.push(CheckResult::warn(
                "Rules coverage",
                ".claude/rules/ directory not found",
            ));
            return;
        }
    };

    if rule_files.is_empty() {
        results.push(CheckResult::warn("Rules coverage", "no rule files found"));
        return;
    }

    // Count rules with paths frontmatter as a simple coverage proxy
    let mut rules_with_paths = 0;
    for file in &rule_files {
        if let Ok(content) = fs.read_file(file) {
            if content.contains("paths:") {
                rules_with_paths += 1;
            }
        }
    }

    results.push(CheckResult::pass(
        "Rules coverage",
        &format!(
            "{} rules found ({} with paths)",
            rule_files.len(),
            rules_with_paths
        ),
    ));
}

fn check_gh_auth(gh: &dyn GhOps, results: &mut Vec<CheckResult>) {
    match gh.run(&["auth", "status"]) {
        Ok(_) => results.push(CheckResult::pass("gh auth", "authenticated")),
        Err(_) => results.push(CheckResult::fail("gh auth", "gh auth login required")),
    }
}

fn check_guard_hook(fs: &dyn FsOps, repo_root: &Path, results: &mut Vec<CheckResult>) {
    let settings = repo_root.join(".claude/settings.local.json");
    if !fs.file_exists(&settings) {
        results.push(CheckResult::warn("Hooks", "settings.local.json not found"));
        return;
    }

    match fs.read_file(&settings) {
        Ok(content) if content.contains("guard-pr-base") => {
            results.push(CheckResult::pass("Hooks", "guard-pr-base registered"));
        }
        _ => {
            results.push(CheckResult::warn("Hooks", "guard-pr-base hook not found"));
        }
    }
}

fn check_quality_gate(fs: &dyn FsOps, config_path: &str, results: &mut Vec<CheckResult>) {
    let path = PathBuf::from(config_path);
    if !fs.file_exists(&path) {
        results.push(CheckResult::warn(
            "Quality Gate",
            "config file not found, auto-detect",
        ));
        return;
    }

    match fs.read_file(&path) {
        Ok(content) => {
            let qg = content
                .lines()
                .find(|l| l.starts_with("quality_gate_command:"))
                .and_then(|l| {
                    let val = l.trim_start_matches("quality_gate_command:").trim();
                    let val = val.trim_matches('"').trim_matches('\'').trim();
                    if val.is_empty() {
                        None
                    } else {
                        Some(val.to_string())
                    }
                });

            match qg {
                None => results.push(CheckResult::pass("Quality Gate", "auto-detect")),
                Some(cmd) => results.push(CheckResult::pass("Quality Gate", &cmd)),
            }
        }
        Err(_) => {
            results.push(CheckResult::warn("Quality Gate", "failed to read config"));
        }
    }
}

fn check_git_remote(git: &dyn GitOps, results: &mut Vec<CheckResult>) {
    match git.remote_url("origin") {
        Ok(url) => results.push(CheckResult::pass("Git Remote", &url)),
        Err(_) => results.push(CheckResult::fail("Git Remote", "origin remote not found")),
    }
}

fn check_spec_files(
    fs: &dyn FsOps,
    config_path: &str,
    repo_root: &Path,
    results: &mut Vec<CheckResult>,
) {
    let path = PathBuf::from(config_path);
    if !fs.file_exists(&path) {
        results.push(CheckResult::warn("Spec files", "config not found"));
        return;
    }

    let content = match fs.read_file(&path) {
        Ok(c) => c,
        Err(_) => {
            results.push(CheckResult::warn("Spec files", "failed to read config"));
            return;
        }
    };

    // Parse spec_paths from YAML-like config
    let spec_paths: Vec<String> = parse_spec_paths(&content);

    if spec_paths.is_empty() {
        results.push(CheckResult::warn("Spec files", "spec_paths not configured"));
        return;
    }

    let mut total = 0;
    for sp in &spec_paths {
        let dir = repo_root.join(sp);
        if let Ok(files) = fs.list_files(&dir, "md") {
            total += files.len();
        }
    }

    if total > 0 {
        results.push(CheckResult::pass(
            "Spec files",
            &format!("{total} spec files found"),
        ));
    } else {
        results.push(CheckResult::fail(
            "Spec files",
            &format!("no .md files in spec_paths ({})", spec_paths.join(", ")),
        ));
    }
}

/// Simple parser for spec_paths YAML list.
fn parse_spec_paths(content: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_spec = false;

    for line in content.lines() {
        if line.starts_with("spec_paths:") {
            in_spec = true;
            continue;
        }
        if in_spec {
            let trimmed = line.trim();
            if trimmed.starts_with('-') {
                let val = trimmed
                    .trim_start_matches('-')
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .trim();
                if !val.is_empty() {
                    paths.push(val.to_string());
                }
            } else if !trimmed.is_empty() {
                break;
            }
        }
    }
    paths
}

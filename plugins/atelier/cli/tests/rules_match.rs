use assert_cmd::Command;
use atelier::rules::commands::matching::MatchCommand;
use atelier::rules::core::matcher::{match_files, FileMatch, MatchReport};
use atelier::rules::core::rule::Rule;
use atelier::rules::core::source::{ChangedFiles, RuleFile, RuleFiles};
use std::collections::BTreeMap;
use std::path::Path;

fn rule(path: &str, content: &str) -> Rule {
    Rule::parse(path, content).expect("valid rule")
}

fn files(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

fn rules_of<'a>(report: &'a MatchReport, file: &str) -> &'a [String] {
    report
        .files
        .iter()
        .find(|f| f.path == file)
        .map(|f| f.rules.as_slice())
        .unwrap_or_else(|| panic!("{file} not in matched files: {:?}", report.files))
}

const CORE: &str = "---\npaths:\n  - \"**/core/**/*.rs\"\n  - \"**/models.rs\"\n---\n\n# Core\n";
const COMMENTS: &str = "---\npaths:\n  - \"**/*.rs\"\n  - \"**/*.ts\"\n---\n# Comments\n";
const ALWAYS: &str = "# Always loaded\n\nno frontmatter\n";

#[test]
fn file_gets_every_rule_whose_paths_match() {
    let rules = [rule("r/core.md", CORE), rule("r/comments.md", COMMENTS)];
    let report = match_files(&rules, &files(&["src/git/core/guard.rs"]));
    assert_eq!(
        rules_of(&report, "src/git/core/guard.rs"),
        ["r/comments.md", "r/core.md"]
    );
}

#[test]
fn file_without_matching_rule_is_reported_unmatched() {
    let rules = [rule("r/core.md", CORE)];
    let report = match_files(&rules, &files(&["README.md", "src/core/a.rs"]));
    assert_eq!(report.unmatched, ["README.md"]);
    assert_eq!(report.files.len(), 1);
}

#[test]
fn rule_without_paths_is_listed_once_as_always_not_per_file() {
    let rules = [rule("r/always.md", ALWAYS), rule("r/core.md", CORE)];
    let report = match_files(&rules, &files(&["README.md"]));
    assert_eq!(report.always_rules, ["r/always.md"]);
    assert_eq!(report.unmatched, ["README.md"]);
}

#[test]
fn frontmatter_without_paths_key_is_always() {
    let r = rule("r/x.md", "---\ndescription: hello\n---\n# X\n");
    let report = match_files(&[r], &files(&["a.rs"]));
    assert_eq!(report.always_rules, ["r/x.md"]);
}

#[test]
fn double_star_prefix_matches_root_level_files() {
    let report = match_files(&[rule("r/c.md", COMMENTS)], &files(&["main.rs"]));
    assert_eq!(rules_of(&report, "main.rs"), ["r/c.md"]);
}

#[test]
fn single_star_does_not_cross_directories() {
    let r = rule("r/md.md", "---\npaths:\n  - \"*.md\"\n---\n");
    let report = match_files(&[r], &files(&["README.md", "docs/guide.md"]));
    assert_eq!(rules_of(&report, "README.md"), ["r/md.md"]);
    assert_eq!(report.unmatched, ["docs/guide.md"]);
}

#[test]
fn brace_alternatives_match_each_extension() {
    let r = rule("r/ts.md", "---\npaths:\n  - \"src/**/*.{ts,tsx}\"\n---\n");
    let report = match_files(&[r], &files(&["src/a.ts", "src/ui/b.tsx", "src/c.js"]));
    assert_eq!(report.files.len(), 2);
    assert_eq!(report.unmatched, ["src/c.js"]);
}

#[test]
fn inline_array_and_scalar_paths_are_accepted() {
    let inline = rule("r/i.md", "---\npaths: [\"*.go\", 'cmd/**']\n---\n");
    let scalar = rule("r/s.md", "---\npaths: \"**/*.sh\"\n---\n");
    let report = match_files(&[inline, scalar], &files(&["main.go", "cmd/x/y", "a/b.sh"]));
    assert_eq!(rules_of(&report, "main.go"), ["r/i.md"]);
    assert_eq!(rules_of(&report, "cmd/x/y"), ["r/i.md"]);
    assert_eq!(rules_of(&report, "a/b.sh"), ["r/s.md"]);
}

#[test]
fn output_order_is_deterministic_regardless_of_input_order() {
    let a = [rule("r/core.md", CORE), rule("r/comments.md", COMMENTS)];
    let b = [rule("r/comments.md", COMMENTS), rule("r/core.md", CORE)];
    let input = files(&["z/core/a.rs", "a.ts", "README.md"]);
    let mut reversed = input.clone();
    reversed.reverse();
    assert_eq!(match_files(&a, &input), match_files(&b, &reversed));
    assert_eq!(
        match_files(&a, &input).files,
        vec![
            FileMatch {
                path: "a.ts".into(),
                rules: vec!["r/comments.md".into()]
            },
            FileMatch {
                path: "z/core/a.rs".into(),
                rules: vec!["r/comments.md".into(), "r/core.md".into()]
            },
        ]
    );
}

#[test]
fn unterminated_frontmatter_is_rejected_with_rule_path() {
    let err = Rule::parse("r/bad.md", "---\npaths:\n  - \"*.rs\"\n# no close\n").unwrap_err();
    assert!(err.contains("r/bad.md"), "{err}");
}

#[test]
fn empty_paths_list_is_rejected() {
    let err = Rule::parse("r/empty.md", "---\npaths:\n---\n").unwrap_err();
    assert!(err.contains("r/empty.md"), "{err}");
}

#[test]
fn invalid_glob_is_rejected() {
    let err = Rule::parse("r/glob.md", "---\npaths:\n  - \"src/{a,b\"\n---\n").unwrap_err();
    assert!(err.contains("r/glob.md"), "{err}");
}

struct FakeChanges(Result<Vec<String>, String>);

impl ChangedFiles for FakeChanges {
    fn changed(&self, _base: &str, _head: &str) -> Result<Vec<String>, String> {
        self.0.clone()
    }
}

struct FakeRules(BTreeMap<String, Vec<RuleFile>>);

impl RuleFiles for FakeRules {
    fn list(&self, dir: &str) -> Result<Vec<RuleFile>, String> {
        self.0
            .get(dir)
            .cloned()
            .ok_or_else(|| format!("rules dir not found: {dir}"))
    }
}

fn rule_file(path: &str, content: &str) -> RuleFile {
    RuleFile {
        path: path.into(),
        content: content.into(),
    }
}

#[test]
fn command_merges_rules_from_every_dir() {
    let changes = FakeChanges(Ok(files(&["src/core/a.rs", "plans/p.md"])));
    let dirs = FakeRules(BTreeMap::from([
        ("proj".to_string(), vec![rule_file("proj/core.md", CORE)]),
        (
            "user".to_string(),
            vec![rule_file(
                "user/plan.md",
                "---\npaths:\n  - \"plans/**/*.md\"\n---\n",
            )],
        ),
    ]));
    let report = MatchCommand::new(&changes, &dirs)
        .run("main", "HEAD", &files(&["proj", "user"]))
        .unwrap();
    assert_eq!(rules_of(&report, "src/core/a.rs"), ["proj/core.md"]);
    assert_eq!(rules_of(&report, "plans/p.md"), ["user/plan.md"]);
}

#[test]
fn command_fails_on_missing_rules_dir() {
    let changes = FakeChanges(Ok(files(&["a.rs"])));
    let dirs = FakeRules(BTreeMap::new());
    let err = MatchCommand::new(&changes, &dirs)
        .run("main", "HEAD", &files(&["nope"]))
        .unwrap_err();
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn command_fails_when_diff_fails() {
    let changes = FakeChanges(Err("bad revision 'nope'".into()));
    let dirs = FakeRules(BTreeMap::from([("proj".to_string(), vec![])]));
    let err = MatchCommand::new(&changes, &dirs)
        .run("nope", "HEAD", &files(&["proj"]))
        .unwrap_err();
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn command_fails_on_broken_rule_file() {
    let changes = FakeChanges(Ok(files(&["a.rs"])));
    let dirs = FakeRules(BTreeMap::from([(
        "proj".to_string(),
        vec![rule_file("proj/bad.md", "---\npaths:\n")],
    )]));
    let err = MatchCommand::new(&changes, &dirs)
        .run("main", "HEAD", &files(&["proj"]))
        .unwrap_err();
    assert!(err.contains("proj/bad.md"), "{err}");
}

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn repo_with_branch() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    write(dir, ".claude/rules/core.md", CORE);
    write(dir, ".claude/rules/nested/always.md", ALWAYS);
    write(dir, "src/old.rs", "fn old() {}\n");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
    let base = git(dir, &["rev-parse", "HEAD"]);
    write(dir, "src/core/new.rs", "fn new() {}\n");
    write(dir, "README.md", "hi\n");
    git(dir, &["rm", "-q", "src/old.rs"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "change"]);
    (tmp, base)
}

#[test]
fn cli_reports_changed_files_against_base() {
    let (tmp, base) = repo_with_branch();
    let out = atelier()
        .current_dir(tmp.path())
        .args([
            "rules",
            "match",
            "--base",
            &base,
            "--rules-dir",
            ".claude/rules",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).expect("json stdout");
    assert_eq!(
        v,
        serde_json::json!({
            "files": [{ "path": "src/core/new.rs", "rules": [".claude/rules/core.md"] }],
            "always_rules": [".claude/rules/nested/always.md"],
            "unmatched": ["README.md"]
        })
    );
}

#[test]
fn cli_exits_2_on_missing_rules_dir() {
    let (tmp, base) = repo_with_branch();
    atelier()
        .current_dir(tmp.path())
        .args([
            "rules",
            "match",
            "--base",
            &base,
            "--rules-dir",
            "no/such/dir",
        ])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("no/such/dir"));
}

#[test]
fn cli_exits_2_on_unknown_base() {
    let (tmp, _) = repo_with_branch();
    atelier()
        .current_dir(tmp.path())
        .args([
            "rules",
            "match",
            "--base",
            "no-such-ref",
            "--rules-dir",
            ".claude/rules",
        ])
        .assert()
        .code(2);
}

#[test]
fn cli_requires_base_and_rules_dir() {
    atelier().args(["rules", "match"]).assert().code(2);
}

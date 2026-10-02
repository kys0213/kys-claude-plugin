//! Default-branch guard — port of `git-utils/src/core/guard.ts`. Decides
//! whether a write/commit on a protected branch is allowed, judging each effect
//! by the repository it lands in. `GuardService` takes the project's
//! `GitService` and a `GitServiceFactory` for every other repository by
//! injection, so it is unit-testable with mock gits.

use crate::git::core::bash_classifier::{
    classify, Anchor, BypassHit, BypassRule, ClassifyInput, Hit, HitKind, OpaqueCause,
};
use crate::git::core::git::{GitService, GitServiceFactory};
use crate::git::types::{
    DefaultBranchSource, GuardInput, GuardOutput, GuardTarget, GuardVerdict, OpaqueExecPolicy,
    ProtectionRule,
};
use regex::Regex;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

static GIT_COMMIT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgit\b.*\bcommit\b").unwrap());

/// Branch-creation command shown in the block message when the caller passes no
/// `--create-branch-script`. The default lives with the guard (the code that
/// renders it), not the CLI router that merely forwards the flag.
pub const DEFAULT_CREATE_BRANCH_SCRIPT: &str = "git switch -c";

/// Replaces single-/double-quoted segments with a space so quoted text
/// arguments can't false-positive the commit matcher — on a protected branch,
/// `gh issue create --body "... git commit ..."` must not be treated as a
/// commit (#754). Trade-off: a commit nested entirely inside quotes
/// (`bash -c "git commit"`) is no longer matched; the guard is a guard-rail,
/// not an escape-proof sandbox.
fn strip_quoted(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match c {
            // An escaped char never opens/closes a quote; keep it verbatim so
            // `echo \"git commit\"` stays conservative (still matches).
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '\'' => {
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                }
                out.push(' ');
            }
            '"' => {
                while let Some(q) = chars.next() {
                    if q == '\\' {
                        chars.next();
                    } else if q == '"' {
                        break;
                    }
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Lexically collapses `.`/`..` in `path` (relative to `base`) without touching
/// the filesystem. A relative `path` is anchored at `base` rather than the
/// process cwd — the guard runs as a PreToolUse hook whose cwd may differ from
/// the project (worktree / subagent contexts), so resolving against cwd
/// mis-judges relative `file_path`s (#780).
fn resolve_against(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    let mut out = if path.is_absolute() {
        PathBuf::new()
    } else {
        base.to_path_buf()
    };
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolves `project_dir` itself, anchoring a relative project dir at the
/// process cwd (the project dir is the anchor, so there is no better base).
fn resolve_project_dir(project_dir: &str) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    resolve_against(&cwd, project_dir)
}

/// Port of TS `isInsideProjectDir`: true when `file_path` is the project dir
/// itself or strictly inside it (no `..` escape, not a sibling prefix match).
/// Relative `file_path`s are resolved against `project_dir` (#780).
pub fn is_inside_project_dir(file_path: &str, project_dir: &str) -> bool {
    inside_project_dir(&resolve_project_dir(project_dir), file_path)
}

fn inside_project_dir(project: &Path, file_path: &str) -> bool {
    let file = resolve_against(project, file_path);
    match file.strip_prefix(project) {
        Ok(rel) => {
            // rel == '' (same dir) or a normal relative descendant.
            rel.as_os_str().is_empty() || !rel.starts_with("..")
        }
        Err(_) => false,
    }
}

/// Port of TS `isInsideAnyGitRepo`: walks up from the file's directory looking
/// for a `.git` entry, skipping non-existent leading directories first.
/// Relative `file_path`s are resolved against `project_dir`, not the process
/// cwd, so the walk starts inside the project (#780).
pub fn is_inside_any_git_repo(file_path: &str, project_dir: &str) -> bool {
    inside_any_git_repo(&resolve_project_dir(project_dir), file_path)
}

fn inside_any_git_repo(project: &Path, file_path: &str) -> bool {
    let resolved = resolve_against(project, file_path);
    find_repo_root(&resolved.parent().map(PathBuf::from).unwrap_or(resolved)).is_some()
}
/// Nearest ancestor of `path` (itself included) that holds a `.git` entry — a
/// directory for a main checkout, a file for a linked worktree. A `path` that
/// does not exist yet is first lifted to its nearest existing ancestor.
pub fn find_repo_root(path: &Path) -> Option<PathBuf> {
    let fs_root = Path::new("/");
    let mut dir = path.to_path_buf();

    while dir != fs_root && !dir.exists() {
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    while dir != fs_root {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    None
}

fn is_under(base: &Path, path: &Path) -> bool {
    path.strip_prefix(base)
        .is_ok_and(|rel| !rel.starts_with(".."))
}

/// The git common directory of the repository checked out at `root`, read from
/// plain files: `.git/` itself for a main checkout, the `commondir` (or the
/// `<common>/worktrees/<name>` layout) behind a linked worktree's `.git` file.
fn common_git_dir(root: &Path) -> Option<PathBuf> {
    let canonical = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(canonical(dot_git));
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let gitdir = text
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:"))?
        .trim();
    let gitdir = resolve_against(root, gitdir);
    let common = match std::fs::read_to_string(gitdir.join("commondir")) {
        Ok(rel) => resolve_against(&gitdir, rel.trim()),
        Err(_) => {
            let worktrees = gitdir.parent()?;
            if worktrees.file_name()? != "worktrees" {
                return None;
            }
            worktrees.parent()?.to_path_buf()
        }
    };
    Some(canonical(common))
}

/// Whether `root` and `project_root` are checkouts of one repository.
fn same_repository(root: &Path, project_root: Option<&Path>) -> bool {
    let Some(project_root) = project_root else {
        return false;
    };
    match (common_git_dir(root), common_git_dir(project_root)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

pub trait GuardService {
    fn check(&self, input: &GuardInput) -> GuardOutput;
}

pub struct RealGuardService<'a> {
    git: &'a dyn GitService,
    factory: &'a dyn GitServiceFactory,
}

/// Constructs a guard service. `git` judges the project's own repository;
/// `factory` supplies a git anchored at any other repository a command lands in.
pub fn create_guard_service<'a>(
    git: &'a dyn GitService,
    factory: &'a dyn GitServiceFactory,
) -> RealGuardService<'a> {
    RealGuardService { git, factory }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Commit,
    Modify,
    MaybeModify,
}

/// One effect of the tool call, reduced to what the branch gate needs.
struct Touch {
    /// Where the effect lands; `None` means "somewhere in the project".
    place: Option<PathBuf>,
    action: Action,
    subject: String,
}

impl Touch {
    fn from_hit(hit: &Hit) -> Touch {
        let (place, shown, certain) = match &hit.anchor {
            Anchor::Path(p) => (Some(p.clone()), p.display().to_string(), true),
            Anchor::Unresolved {
                literal_prefix,
                raw,
            } => (literal_prefix.clone(), raw.clone(), false),
        };
        let (action, program) = match (&hit.kind, certain) {
            (HitKind::Commit, true) => (Action::Commit, "git commit".to_string()),
            (HitKind::Commit, false) => (Action::MaybeModify, "git commit".to_string()),
            (HitKind::Write(_), true) => (Action::Modify, hit.program.clone()),
            _ => (Action::MaybeModify, hit.program.clone()),
        };
        Touch {
            place,
            action,
            subject: format!("Bash `{program}` → {shown}"),
        }
    }

    fn from_write_tool(resolved: Option<PathBuf>, project_dir: &str) -> Touch {
        let shown = resolved
            .as_ref()
            .map_or_else(|| project_dir.to_string(), |p| p.display().to_string());
        Touch {
            place: resolved,
            action: Action::Modify,
            subject: format!("Write → {shown}"),
        }
    }

    fn is_definite(&self) -> bool {
        self.action != Action::MaybeModify
    }
}

/// The tool call reduced to its effects, plus the context the message shows.
struct Scene {
    touches: Vec<Touch>,
    /// Shell cwd when it differs from the project directory.
    cwd: Option<String>,
    nothing_in_scope: &'static str,
}

#[derive(Clone, PartialEq, Eq)]
enum Scope {
    Project,
    Root(PathBuf),
}

fn scope_of(touch: &Touch, project: &Path, project_root: Option<&Path>) -> Option<Scope> {
    let Some(place) = &touch.place else {
        return Some(Scope::Project);
    };
    match find_repo_root(place) {
        Some(root) if Some(root.as_path()) == project_root => Some(Scope::Project),
        Some(root) => Some(Scope::Root(root)),
        None if is_under(project, place) => Some(Scope::Project),
        None => None,
    }
}

/// Everything one root's verdict needs besides the git it asks.
struct Case<'a> {
    input: &'a GuardInput,
    scene: &'a Scene,
    script: &'a str,
}

fn output(
    verdict: GuardVerdict,
    reason: Option<String>,
    root: &Path,
    branches: Option<(&str, &str)>,
    rule: Option<ProtectionRule>,
) -> GuardOutput {
    GuardOutput {
        verdict,
        reason,
        current_branch: branches.map(|(current, _)| current.to_string()),
        default_branch: branches.map(|(_, default)| default.to_string()),
        rule,
        repo_root: Some(root.display().to_string()),
    }
}

fn rank(verdict: GuardVerdict) -> u8 {
    match verdict {
        GuardVerdict::Allow => 0,
        GuardVerdict::Ask => 1,
        GuardVerdict::Block => 2,
    }
}

fn protection_rule(
    branch: &str,
    default_branch: &str,
    source: DefaultBranchSource,
    extras: Option<&[String]>,
) -> Option<ProtectionRule> {
    if branch == default_branch {
        Some(ProtectionRule::DefaultBranch { source })
    } else if branch == "develop" {
        Some(ProtectionRule::Develop)
    } else if extras.is_some_and(|extras| extras.iter().any(|b| b == branch)) {
        Some(ProtectionRule::Extra)
    } else {
        None
    }
}

impl GuardService for RealGuardService<'_> {
    fn check(&self, input: &GuardInput) -> GuardOutput {
        let project = resolve_project_dir(&input.project_dir);
        let scene = match scene_of(input, &project) {
            Ok(scene) => scene,
            Err(done) => return done,
        };
        let script = match input.create_branch_script.trim() {
            "" => DEFAULT_CREATE_BRANCH_SCRIPT,
            s => s,
        };
        let case = Case {
            input,
            scene: &scene,
            script,
        };

        let project_root = find_repo_root(&project);
        let mut groups: Vec<(Scope, Vec<&Touch>)> = Vec::new();
        for touch in &scene.touches {
            let Some(scope) = scope_of(touch, &project, project_root.as_deref()) else {
                continue;
            };
            match groups.iter_mut().find(|(s, _)| *s == scope) {
                Some((_, touches)) => touches.push(touch),
                None => groups.push((scope, vec![touch])),
            }
        }

        let mut strongest: Option<GuardOutput> = None;
        for (scope, touches) in &groups {
            let out = match scope {
                Scope::Project => {
                    let root = project_root.as_deref().unwrap_or(&project);
                    self.judge_repo(
                        &case,
                        self.git,
                        root,
                        input.default_branch.as_deref(),
                        touches,
                    )
                }
                Scope::Root(root) => {
                    let git = self.factory.at(root);
                    let pin = same_repository(root, project_root.as_deref())
                        .then_some(input.default_branch.as_deref())
                        .flatten();
                    self.judge_repo(&case, git.as_ref(), root, pin, touches)
                }
            };
            if out.verdict == GuardVerdict::Block {
                return out;
            }
            strongest = match strongest {
                Some(current) if rank(current.verdict) >= rank(out.verdict) => Some(current),
                _ => Some(out),
            };
        }
        strongest.unwrap_or_else(|| pass(Some(scene.nothing_in_scope)))
    }
}

fn pass(reason: Option<&str>) -> GuardOutput {
    GuardOutput {
        verdict: GuardVerdict::Allow,
        reason: reason.map(str::to_string),
        current_branch: None,
        default_branch: None,
        rule: None,
        repo_root: None,
    }
}

fn scene_of(input: &GuardInput, project: &Path) -> Result<Scene, GuardOutput> {
    match &input.target {
        GuardTarget::Write { file_path } => Ok(Scene {
            touches: vec![Touch::from_write_tool(
                file_path.as_deref().map(|f| resolve_against(project, f)),
                &input.project_dir,
            )],
            cwd: None,
            nothing_in_scope: "file is outside any git repository",
        }),
        GuardTarget::Commit { command, cwd } => {
            let Some(command) = command.as_deref().filter(|c| !c.is_empty()) else {
                return Err(pass(Some("no command to inspect")));
            };
            let cwd = cwd
                .as_deref()
                .map_or_else(|| project.to_path_buf(), |c| resolve_against(project, c));
            let analysis = classify(&ClassifyInput {
                command: command.to_string(),
                cwd: cwd.clone(),
                home: input.home.as_deref().map(PathBuf::from),
            });
            let hits = match analysis {
                Ok(analysis) => {
                    if let Some(bypass) = analysis.bypass.first() {
                        return Err(GuardOutput {
                            reason: Some(bypass_reason(bypass)),
                            verdict: GuardVerdict::Block,
                            ..pass(None)
                        });
                    }
                    analysis.hits
                }
                Err(_) => unparsed_command_hits(command, &cwd),
            };
            Ok(Scene {
                touches: hits.iter().map(Touch::from_hit).collect(),
                cwd: (cwd != project).then(|| cwd.display().to_string()),
                nothing_in_scope: "no repository-changing command",
            })
        }
    }
}

impl RealGuardService<'_> {
    /// Judges `touches` against the repository rooted at `root`. `pin` is the
    /// project's `--default-branch`, passed only where it applies.
    fn judge_repo(
        &self,
        case: &Case,
        git: &dyn GitService,
        root: &Path,
        pin: Option<&str>,
        touches: &[&Touch],
    ) -> GuardOutput {
        let allow = |reason: &str| {
            output(
                GuardVerdict::Allow,
                Some(reason.to_string()),
                root,
                None,
                None,
            )
        };

        if !git.is_inside_work_tree() {
            return allow("not a git repository");
        }

        // An empty/whitespace pin (a setup that detected nothing and recorded a
        // bare `--default-branch`) counts as absence, so detection still
        // protects the true default instead of silently protecting nothing.
        let (default_branch, source) = match pin.map(str::trim) {
            Some(b) if !b.is_empty() => (b.to_string(), DefaultBranchSource::Pinned),
            // Read-only detection: the guard must not mutate repo state on
            // every tool invocation.
            _ => match git.detect_default_branch() {
                Ok(b) => (b, DefaultBranchSource::Detected),
                Err(_) => return allow("could not detect default branch"),
            },
        };

        // The state snapshot also carries the current branch, so the special
        // state, detached and branch checks cost one git round-trip.
        let state = git.get_special_state();
        if state.rebase || state.merge {
            return allow("special git state (rebase/merge)");
        }
        if state.detached() {
            return allow("detached HEAD");
        }
        let branch = state.current_branch;

        let Some(rule) = protection_rule(
            &branch,
            &default_branch,
            source,
            case.input.protected_branches.as_deref(),
        ) else {
            return output(
                GuardVerdict::Allow,
                None,
                root,
                Some((&branch, &default_branch)),
                None,
            );
        };

        let decided = |verdict, reason: Option<String>| {
            output(
                verdict,
                reason,
                root,
                Some((&branch, &default_branch)),
                Some(rule),
            )
        };
        let render = |touch: &Touch, notice: Notice| {
            render_branch_report(&BranchReport {
                branch: &branch,
                rule,
                root: &root.display().to_string(),
                project_dir: &case.input.project_dir,
                cwd: case.scene.cwd.as_deref(),
                action: touch.action,
                subject: &touch.subject,
                script: case.script,
                notice,
            })
        };

        if let Some(touch) = touches.iter().find(|t| t.is_definite()) {
            return decided(GuardVerdict::Block, Some(render(touch, Notice::None)));
        }
        let Some(touch) = touches.first() else {
            return allow("no repository-changing command");
        };
        match case.input.opaque_exec {
            OpaqueExecPolicy::Allow => decided(
                GuardVerdict::Allow,
                Some("opaque execution allowed by --opaque-exec".to_string()),
            ),
            OpaqueExecPolicy::Block => decided(
                GuardVerdict::Block,
                Some(render(touch, Notice::OpaqueBlocked)),
            ),
            OpaqueExecPolicy::Ask => {
                decided(GuardVerdict::Ask, Some(render(touch, Notice::OpaqueAsk)))
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Notice {
    None,
    OpaqueAsk,
    OpaqueBlocked,
}

struct BranchReport<'a> {
    branch: &'a str,
    rule: ProtectionRule,
    root: &'a str,
    project_dir: &'a str,
    cwd: Option<&'a str>,
    action: Action,
    subject: &'a str,
    script: &'a str,
    notice: Notice,
}

fn rule_label(rule: ProtectionRule, branch: &str) -> String {
    match rule {
        ProtectionRule::DefaultBranch {
            source: DefaultBranchSource::Pinned,
        } => format!("기본 브랜치 보호(--default-branch {branch})"),
        ProtectionRule::DefaultBranch {
            source: DefaultBranchSource::Detected,
        } => format!("기본 브랜치 보호(자동 감지: {branch})"),
        ProtectionRule::Develop => "develop 고정 보호".to_string(),
        ProtectionRule::Extra => "추가 보호(--protected-branches)".to_string(),
    }
}

/// The one Branch Guard message, shared by the write tool and Bash.
fn render_branch_report(r: &BranchReport) -> String {
    let what = match r.action {
        Action::Commit => "커밋하려 합니다",
        Action::Modify => "파일을 수정하려 합니다",
        Action::MaybeModify => "파일을 수정할 수 있는 명령을 실행하려 합니다",
    };
    let location = match r.cwd {
        Some(cwd) => format!("프로젝트: {}, 명령 cwd: {cwd}", r.project_dir),
        None => format!("프로젝트: {}", r.project_dir),
    };
    let mut lines = vec![
        format!(
            "[Branch Guard] 보호 브랜치({})에서 {what} — {}",
            r.branch, r.subject
        ),
        format!(
            "- 현재 브랜치: {} / 대상 저장소: {} ({location})",
            r.branch, r.root
        ),
        format!("- 발동 규칙: {}", rule_label(r.rule, r.branch)),
    ];
    match r.notice {
        Notice::None => {}
        Notice::OpaqueAsk => lines.push(
            "스크립트/대상 내부를 확인할 수 없어 확인을 요청합니다. 저장소 파일을 바꾸지 않는다면 승인하세요."
                .to_string(),
        ),
        Notice::OpaqueBlocked => lines.push(
            "스크립트/대상 내부를 확인할 수 없어 차단합니다(--opaque-exec block).".to_string(),
        ),
    }
    lines.push(format!(
        "가드 오작동을 의심하기 전에 `git -C {} branch --show-current` 로 브랜치를 먼저 확인하세요.",
        r.root
    ));
    lines.push("해소: 새 브랜치를 만든 뒤 다시 시도하세요:".to_string());
    lines.push(format!("  {} <branch-name>", r.script));
    lines.join("\n")
}

fn bypass_reason(hit: &BypassHit) -> String {
    let rule = match hit.rule {
        BypassRule::NoVerifyFlag => "--no-verify",
        BypassRule::CommitShortNoVerify => "commit -n",
        BypassRule::HooksPathConfig => "core.hooksPath",
        BypassRule::HooksPathEnv => "core.hooksPath env",
        BypassRule::HookSkipEnv => "hook skip env",
    };
    let detail = if rule == hit.token {
        rule.to_string()
    } else {
        format!("{rule}: {}", hit.token)
    };
    format!(
        "[Hook Guard] git hook 우회({detail})는 프로젝트 정책상 금지입니다(브랜치 무관). hook 실패 원인을 수정한 뒤 다시 실행하세요."
    )
}

/// Hits for a command the lexer could not read: the plain regex commit check
/// plus an opaque hit, so a protected branch never ends in a silent allow.
fn unparsed_command_hits(command: &str, cwd: &Path) -> Vec<Hit> {
    let anchor = Anchor::Path(cwd.to_path_buf());
    let mut hits = Vec::new();
    if GIT_COMMIT_PATTERN.is_match(&strip_quoted(command)) {
        hits.push(Hit {
            kind: HitKind::Commit,
            anchor: anchor.clone(),
            program: "git".to_string(),
        });
    }
    hits.push(Hit {
        kind: HitKind::Opaque(OpaqueCause::Unparsed),
        anchor,
        program: command.split_whitespace().next().unwrap_or("").to_string(),
    });
    hits
}

//! Default-branch guard — port of `git-utils/src/core/guard.ts`. Decides
//! whether a write/commit on a protected branch is allowed, judging each effect
//! by the repository it lands in. `GuardService` takes the project's
//! `GitService` and a `GitServiceFactory` for every other repository by
//! injection, so it is unit-testable with mock gits.

mod message;
mod repo_layout;

pub use repo_layout::{find_repo_root, is_inside_any_git_repo, is_inside_project_dir};

use crate::git::core::bash_classifier::{
    classify, Anchor, ClassifyInput, Hit, HitKind, OpaqueCause,
};
use crate::git::core::git::{GitService, GitServiceFactory};
use crate::git::types::{
    DefaultBranchSource, GuardInput, GuardOutput, GuardTarget, GuardVerdict, OpaqueExecPolicy,
    ProtectionRule,
};
use message::{bypass_reason, render_branch_report, BranchReport, Notice};
use regex::Regex;
use repo_layout::{is_under, resolve_against, resolve_project_dir, same_repository};
use std::path::{Path, PathBuf};
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
pub(super) enum Action {
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

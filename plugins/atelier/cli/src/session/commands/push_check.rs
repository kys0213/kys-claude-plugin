//! `session push-check` command — decides whether the Stop hook should block
//! on unpushed work, and renders the block reason when it should.
//!
//! The rule: a branch with an **open PR** whose local commits are ahead of its
//! upstream is work a reviewer cannot see. Ending the session there leaves the
//! PR silently stale, so Stop is blocked with an explanation instead.
//!
//! The rule order is a cost contract, not an optimisation: the cheapest
//! deterministic read (one `rev-list`) ends the overwhelmingly common Stop, and
//! `gh` is consulted only once a block is otherwise certain.

use crate::git::core::git::GitService;
use crate::git::core::github::OpenPr;

/// Everything the decision reads from the outside world. Both traits belong to
/// the git subsystem, so "is this branch ahead" and "is a PR open" are answered
/// in the same place for the guard and for this hook.
pub struct PushCheckDeps<'a> {
    pub git: &'a dyn GitService,
    pub open_pr: &'a dyn OpenPr,
}

/// Why the hook stayed silent. One variant per rule so the reasoning is
/// legible in tests and future logging, even though all of them print nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SilentReason {
    /// Claude is already continuing *because* a Stop hook blocked. Blocking
    /// again is the documented infinite loop.
    StopHookActive,
    /// The drift against `@{upstream}` could not be read at all: no upstream
    /// configured, outside a work tree, an empty repo, a detached HEAD.
    /// Unknown is not zero drift, so the check disarms rather than guesses.
    DivergenceUnknown,
    /// Upstream already has every local commit.
    NothingToPush,
    /// Mid-rebase, mid-merge or detached HEAD — "push this branch" is not
    /// meaningful advice in any of them.
    SpecialState,
    /// On the default branch, which never carries an open PR of its own.
    DefaultBranch,
    /// Nothing is open for this branch, so nobody is waiting on the commits.
    NoOpenPr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushCheckDecision {
    Silent(SilentReason),
    Block {
        branch: String,
        pr_number: i64,
        /// Commits present locally but not on the upstream.
        ahead: u32,
        /// Commits present on the upstream but not locally.
        behind: u32,
    },
}

/// Runs the check against its injected dependencies, short-circuiting on the
/// first rule that answers. Printing is the CLI edge's job, so this stays a
/// pure decision the tests can assert on.
pub fn run(deps: &PushCheckDeps, stop_hook_active: bool) -> PushCheckDecision {
    use SilentReason as S;
    let silent = PushCheckDecision::Silent;

    if stop_hook_active {
        return silent(S::StopHookActive);
    }
    let Some(divergence) = deps.git.upstream_divergence() else {
        return silent(S::DivergenceUnknown);
    };
    if divergence.ahead == 0 {
        return silent(S::NothingToPush);
    }

    let state = deps.git.get_special_state();
    if state.rebase || state.merge || state.detached() {
        return silent(S::SpecialState);
    }
    // Detection failure is not a reason to stop: the decisive gate below is
    // "an open PR exists", and the default branch cannot have one for itself,
    // so carrying on costs at most one `gh` call and never a false block.
    if deps
        .git
        .detect_default_branch()
        .is_ok_and(|default| default == state.current_branch)
    {
        return silent(S::DefaultBranch);
    }

    let Some(pr_number) = deps.open_pr.open_pr_number() else {
        return silent(S::NoOpenPr);
    };

    PushCheckDecision::Block {
        branch: state.current_branch,
        pr_number,
        ahead: divergence.ahead,
        behind: divergence.behind,
    }
}

/// Renders the block reason Claude is shown: the situation, then a pointer to
/// the policy that decides what to do about it. The policy itself is stated
/// once, in the git skill, and is never restated here.
fn render_reason(branch: &str, pr_number: i64, ahead: u32, behind: u32) -> String {
    let behind_line = if behind > 0 {
        format!("  원격 전용 커밋: {behind}개\n")
    } else {
        String::new()
    };
    let next_step = if behind > 0 {
        "원격이 앞서 있습니다 — rebase 여부는 git skill §열린 PR 최신화 원칙 에 따라 판단한 뒤 push 하세요."
    } else {
        "`git push` 로 원격을 최신화한 뒤 종료하세요."
    };
    format!(
        "[push-check] 열린 PR 이 있는 브랜치에 push 되지 않은 커밋이 있습니다.\n\n\
         \x20 브랜치: {branch}\n\
         \x20 PR: #{pr_number}\n\
         \x20 로컬 전용 커밋: {ahead}개\n\
         {behind_line}\n\
         {next_step}\n\n\
         push 가 거부되면 재시도하지 말고 git skill §push 권한 거부 시 사용자 위임 을 따르세요."
    )
}

/// Serialises a block into the Stop hook's JSON contract. Built through
/// `serde_json` rather than string concatenation so a reason containing a
/// quote or a newline can never produce invalid JSON.
pub fn render_block_json(decision: &PushCheckDecision) -> Option<String> {
    let PushCheckDecision::Block {
        branch,
        pr_number,
        ahead,
        behind,
    } = decision
    else {
        return None;
    };
    let payload = serde_json::json!({
        "decision": "block",
        "reason": render_reason(branch, *pr_number, *ahead, *behind),
    });
    Some(payload.to_string())
}

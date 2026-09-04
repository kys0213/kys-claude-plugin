//! `session push-check` command — decides whether the Stop hook should block
//! on unpushed work, and renders the block reason when it should.
//!
//! The rule: a branch with an **open PR** whose local commits are ahead of its
//! upstream is work a reviewer cannot see. Ending the session there leaves the
//! PR silently stale, so Stop is blocked with an explanation instead. The hook
//! never pushes — it detects and reports; pushing is the agent's act, under the
//! git skill's force-push and permission-denied policies.
//!
//! Ordering is part of the contract, not an optimisation: every local check
//! comes first and `gh` is consulted only once a block is otherwise certain, so
//! the overwhelmingly common Stop (nothing to push) costs no network call.

use crate::session::core::branch_sync::BranchSyncReader;
use crate::session::core::open_pr::OpenPrReader;

/// Everything the decision reads from the outside world. Injected as traits so
/// the whole rule set — including *which* reads happen — is exercised in
/// memory.
pub struct PushCheckDeps<'a> {
    pub branch: &'a dyn BranchSyncReader,
    pub open_pr: &'a dyn OpenPrReader,
}

/// The decision's input: the one payload fact it depends on, plus the readers
/// it consults lazily. The readers are borrowed rather than pre-read because
/// the order in which they are consulted *is* the network contract.
pub struct PushCheckInput<'a> {
    /// `stop_hook_active` from the hook payload: this Stop is already the
    /// continuation of a blocked one.
    pub stop_hook_active: bool,
    pub deps: &'a PushCheckDeps<'a>,
}

/// Why the hook stayed silent. One variant per rule so the reasoning is
/// legible in tests and future logging, even though all of them print nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SilentReason {
    /// Claude is already continuing *because* a Stop hook blocked. Blocking
    /// again is the documented infinite loop.
    StopHookActive,
    /// The project directory is not inside a git work tree.
    OutsideWorkTree,
    /// Mid-rebase, mid-merge or detached HEAD — "push this branch" is not
    /// meaningful advice in any of them.
    SpecialState,
    /// On the default branch, which never carries an open PR of its own.
    DefaultBranch,
    /// No upstream configured — the branch was never pushed, so there is no
    /// PR either, and "ahead" has nothing to be ahead of.
    NoUpstream,
    /// Upstream already has every local commit.
    NothingToPush,
    /// The `gh` lookup failed. Safe mode, exactly as the PR guard treats it:
    /// an unanswerable forge never blocks the user.
    PrLookupFailed,
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

/// Decides in the documented order, short-circuiting on the first rule that
/// answers. Deterministic for a given set of readers.
pub fn decide(input: &PushCheckInput) -> PushCheckDecision {
    use SilentReason as S;
    let silent = PushCheckDecision::Silent;

    if input.stop_hook_active {
        return silent(S::StopHookActive);
    }
    let deps = input.deps;
    if !deps.branch.is_inside_work_tree() {
        return silent(S::OutsideWorkTree);
    }

    let state = deps.branch.special_state();
    if state.rebase || state.merge || state.detached() {
        return silent(S::SpecialState);
    }
    // Detection failure is not a reason to stop: the decisive gate below is
    // "an open PR exists", and the default branch cannot have one for itself,
    // so carrying on costs at most one `gh` call and never a false block.
    if deps
        .branch
        .default_branch()
        .is_some_and(|default| default == state.current_branch)
    {
        return silent(S::DefaultBranch);
    }

    let Some((behind, ahead)) = deps.branch.upstream_divergence() else {
        return silent(S::NoUpstream);
    };
    if ahead == 0 {
        return silent(S::NothingToPush);
    }

    // First and only network read — reached only when everything else already
    // points at a block.
    let Ok(open_pr) = deps.open_pr.open_pr_number() else {
        return silent(S::PrLookupFailed);
    };
    let Some(pr_number) = open_pr else {
        return silent(S::NoOpenPr);
    };

    PushCheckDecision::Block {
        branch: state.current_branch,
        pr_number,
        ahead,
        behind,
    }
}

/// Renders the block reason Claude is shown. Tone follows the PR guard's:
/// state the situation, then the exact next step.
pub fn render_reason(branch: &str, pr_number: i64, ahead: u32, behind: u32) -> String {
    let mut lines = vec![
        "[push-check] 열린 PR 이 있는 브랜치에 push 되지 않은 커밋이 있습니다.".to_string(),
        String::new(),
        format!("  브랜치: {branch}"),
        format!("  PR: #{pr_number}"),
        format!("  로컬 전용 커밋: {ahead}개"),
    ];
    if behind > 0 {
        lines.push(format!("  원격 전용 커밋: {behind}개"));
    }
    lines.push(String::new());
    lines.push(if behind > 0 {
        "원격이 앞서 있습니다. rebase 여부를 git skill 의 force-push 정책\
         (`--force-with-lease` 는 이미 push 된 브랜치를 rebase 한 직후에만)에 따라 \
         판단한 뒤 push 하세요."
            .to_string()
    } else {
        "`git push` 로 원격을 최신화한 뒤 종료하세요.".to_string()
    });
    lines.push(String::new());
    lines.push(
        "push 가 permission denied 되면 같은 명령을 재시도하지 말고 \
         실행할 정확한 명령(브랜치·remote 포함)을 사용자에게 위임하세요."
            .to_string(),
    );
    lines.join("\n")
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

/// Runs the check against its injected dependencies. Printing is the CLI
/// edge's job, so this stays a pure decision the tests can assert on.
pub fn run(deps: &PushCheckDeps, stop_hook_active: bool) -> PushCheckDecision {
    decide(&PushCheckInput {
        stop_hook_active,
        deps,
    })
}

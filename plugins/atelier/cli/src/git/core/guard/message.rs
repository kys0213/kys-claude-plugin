use super::Action;
use crate::git::core::bash_classifier::{BypassHit, BypassRule};
use crate::git::types::{DefaultBranchSource, ProtectionRule};

#[derive(Clone, Copy)]
pub(super) enum Notice {
    None,
    OpaqueAsk,
    OpaqueBlocked,
}

pub(super) struct BranchReport<'a> {
    pub(super) branch: &'a str,
    pub(super) rule: ProtectionRule,
    pub(super) root: &'a str,
    pub(super) project_dir: &'a str,
    pub(super) cwd: Option<&'a str>,
    pub(super) action: Action,
    pub(super) subject: &'a str,
    pub(super) script: &'a str,
    pub(super) notice: Notice,
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
pub(super) fn render_branch_report(r: &BranchReport) -> String {
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

pub(super) fn bypass_reason(hit: &BypassHit) -> String {
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

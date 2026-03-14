use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Returns the claw-workspace path under the given autodev home.
pub fn claw_workspace_path(home: &Path) -> PathBuf {
    home.join("claw-workspace")
}

/// Initialize the global claw-workspace with default structure.
///
/// Creates `<home>/claw-workspace/` with CLAUDE.md, rules, commands, and skills.
/// Idempotent: existing files are not overwritten.
pub fn claw_init(home: &Path) -> Result<()> {
    let ws = claw_workspace_path(home);

    // Create directory structure
    let dirs = [
        ws.join(".claude/rules"),
        ws.join("commands"),
        ws.join("skills/decompose"),
        ws.join("skills/prioritize"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create directory: {}", dir.display()))?;
    }

    // Write default files (only if they don't exist)
    let files: &[(&str, &str)] = &[
        ("CLAUDE.md", DEFAULT_CLAUDE_MD),
        (".claude/rules/scheduling.md", DEFAULT_SCHEDULING_MD),
        (".claude/rules/branch-naming.md", DEFAULT_BRANCH_NAMING_MD),
        (".claude/rules/review-policy.md", DEFAULT_REVIEW_POLICY_MD),
        ("commands/status.md", DEFAULT_STATUS_MD),
        ("commands/board.md", DEFAULT_BOARD_MD),
        ("commands/hitl.md", DEFAULT_HITL_MD),
        ("skills/decompose/SKILL.md", DEFAULT_DECOMPOSE_SKILL_MD),
        ("skills/prioritize/SKILL.md", DEFAULT_PRIORITIZE_SKILL_MD),
    ];

    for (rel_path, content) in files {
        let path = ws.join(rel_path);
        if !path.exists() {
            std::fs::write(&path, content)
                .with_context(|| format!("failed to write: {}", path.display()))?;
        }
    }

    println!("Claw workspace initialized: {}", ws.display());

    Ok(())
}

/// Initialize a per-repo claw override directory.
///
/// Creates `<home>/workspaces/<org-repo>/claw/` with empty override structure.
pub fn claw_init_repo(home: &Path, repo_name: &str) -> Result<()> {
    let sanitized = crate::core::config::sanitize_repo_name(repo_name);
    let repo_claw = home.join("workspaces").join(&sanitized).join("claw");

    let dirs = [
        repo_claw.join(".claude/rules"),
        repo_claw.join("commands"),
        repo_claw.join("skills"),
    ];

    for dir in &dirs {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create directory: {}", dir.display()))?;
    }

    println!(
        "Per-repo claw override initialized: {}",
        repo_claw.display()
    );

    Ok(())
}

/// List applied rule files from global claw-workspace and optionally per-repo overrides.
///
/// Returns a list of rule file paths (relative display).
pub fn claw_rules(home: &Path, repo: Option<&str>) -> Result<Vec<String>> {
    let ws = claw_workspace_path(home);
    let global_rules_dir = ws.join(".claude/rules");

    if !ws.exists() {
        anyhow::bail!("Claw workspace not initialized. Run 'autodev claw init' first.");
    }

    let mut rules = Vec::new();

    // Collect global rules
    if global_rules_dir.is_dir() {
        collect_rule_files(&global_rules_dir, "[global]", &mut rules)?;
    }

    // Collect per-repo override rules if requested
    if let Some(repo_name) = repo {
        let sanitized = crate::core::config::sanitize_repo_name(repo_name);
        let repo_rules_dir = home
            .join("workspaces")
            .join(&sanitized)
            .join("claw/.claude/rules");

        if !home
            .join("workspaces")
            .join(&sanitized)
            .join("claw")
            .exists()
        {
            anyhow::bail!(
                "Per-repo claw override not initialized for '{repo_name}'. Run 'autodev claw init --repo {repo_name}' first."
            );
        }

        if repo_rules_dir.is_dir() {
            collect_rule_files(&repo_rules_dir, &format!("[{repo_name}]"), &mut rules)?;
        }
    }

    Ok(rules)
}

fn collect_rule_files(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        out.push(format!("{prefix} {name}"));
    }

    Ok(())
}

// ─── Default content ───

const DEFAULT_CLAUDE_MD: &str = r#"# Claw 판단 원칙

## 역할
나는 Claw, 자율 개발 에이전트의 스케줄러다.
큐 상태를 보고 어떤 작업을 진행할지 판단한다.

## 핵심 원칙
1. 독립적인 이슈는 병렬 진행한다
2. 같은 파일을 수정하는 이슈는 순차 처리한다
3. 리뷰가 3회 반복되면 HITL을 요청한다
4. 스펙의 acceptance criteria를 항상 참조한다
5. gap을 발견하면 즉시 이슈를 생성한다

## 도구
- `autodev queue list --json` — 큐 상태 확인
- `autodev spec list --json` — 스펙 목록
- `autodev hitl list --json` — HITL 대기 목록
- `autodev decisions list --json` — 판단 이력
"#;

const DEFAULT_SCHEDULING_MD: &str = r#"# 스케줄링 정책

## 우선순위 결정
1. HITL 대기 중인 작업이 있으면 사용자에게 알린다
2. 블로커가 없는 작업을 우선 진행한다
3. 같은 파일을 수정하는 작업은 순차 처리한다
4. 리소스 제한 내에서 최대 병렬성을 유지한다

## 큐 상태 전이
- `pending` → `in_progress` → `review` → `done`
- `review`에서 3회 반복 시 → `hitl_required`
"#;

const DEFAULT_BRANCH_NAMING_MD: &str = r#"# 브랜치 네이밍

## 형식
```
<type>/<short-description>
```

## 타입
- `feat/` — 새 기능
- `fix/` — 버그 수정
- `refactor/` — 리팩토링
- `docs/` — 문서

## 예시
- `feat/add-user-auth`
- `fix/null-pointer-check`
- `refactor/extract-service`
"#;

const DEFAULT_REVIEW_POLICY_MD: &str = r#"# 리뷰 정책

## 자동 리뷰 기준
1. 모든 테스트가 통과해야 한다
2. lint/format 검사를 통과해야 한다
3. 변경 범위가 스펙의 acceptance criteria에 부합해야 한다

## HITL 에스컬레이션
- 리뷰 반복 3회 초과 시 HITL 요청
- 아키텍처 변경이 포함된 경우 HITL 요청
- 보안 관련 변경이 포함된 경우 HITL 요청
"#;

const DEFAULT_STATUS_MD: &str = r#"# /status 커맨드

현재 Claw 세션의 상태를 요약합니다.

## 출력 항목
- 활성 작업 수
- 대기 중인 HITL 이벤트
- 최근 완료된 작업
- 에러/블로커 현황

## 실행
```
autodev queue list --json
autodev hitl list --json
```
"#;

const DEFAULT_BOARD_MD: &str = r#"# /board 커맨드

칸반 보드 형태로 현재 작업 상태를 표시합니다.

## 컬럼
- Backlog: 대기 중인 작업
- In Progress: 진행 중인 작업
- Review: 리뷰 중인 작업
- Done: 완료된 작업

## 데이터 소스
```
autodev queue list --json
autodev spec list --json
```
"#;

const DEFAULT_HITL_MD: &str = r#"# /hitl 커맨드

Human-in-the-Loop 이벤트를 관리합니다.

## 기능
- 대기 중인 HITL 이벤트 목록 표시
- 이벤트 상세 정보 조회
- 이벤트 응답 (선택지 또는 메시지)

## 실행
```
autodev hitl list --json
autodev hitl show <id>
autodev hitl respond <id> --choice <n>
```
"#;

const DEFAULT_DECOMPOSE_SKILL_MD: &str = r#"# 스펙 분해 스킬

## 목적
큰 스펙을 구현 가능한 단위로 분해한다.

## 입력
- 스펙 본문 (body)
- acceptance criteria

## 출력
- 분해된 서브태스크 목록
- 각 서브태스크의 예상 범위
- 의존 관계 그래프

## 프로세스
1. 스펙의 요구사항을 파악한다
2. 독립적으로 구현 가능한 단위로 분리한다
3. 의존 관계를 식별한다
4. 구현 순서를 제안한다
"#;

const DEFAULT_PRIORITIZE_SKILL_MD: &str = r#"# 우선순위 판단 스킬

## 목적
대기 중인 작업의 우선순위를 판단한다.

## 기준
1. 블로커 여부 (다른 작업을 막고 있는가)
2. 의존성 (선행 작업이 완료되었는가)
3. 파일 충돌 (같은 파일을 수정하는 작업이 진행 중인가)
4. 비즈니스 우선순위 (스펙에 명시된 우선순위)

## 출력
- 정렬된 작업 목록
- 각 작업의 우선순위 근거
"#;

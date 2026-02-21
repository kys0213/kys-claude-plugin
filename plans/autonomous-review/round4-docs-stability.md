# Round 4: 문서 동기화 및 안정성

> **Type**: `docs(autonomous)` + `fix(autonomous)`
> **Priority**: P2
> **Depends on**: Round 1, 2, 3 (코드 변경 완료 후 문서 동기화)

## 요약

DESIGN.md를 현행 코드와 동기화하고, 리소스 관리(worktree, git pull) 안정성을 개선합니다.

---

## 작업 항목

### #1 이름/버전 불일치 수정

**파일**: `DESIGN.md`

**현재 불일치**:

| 항목 | DESIGN.md | Cargo.toml | plugin.json |
|------|-----------|------------|-------------|
| package name | `autonomous` | `autodev` | `autonomous` |
| binary name | `autonomous` | `autodev` | — |
| version | `0.1.0` | `0.2.3` | `0.2.5` |

**변경 방향**:
- DESIGN.md 섹션 3 (Cargo.toml) — `name = "autodev"` 로 수정
- DESIGN.md 전반 — 바이너리 이름을 `autodev`로 통일하거나, 현재 `ensure-binary.sh`가 `autonomous`로 설치하는 구조를 명시
- 버전 번호는 DESIGN.md에서 제거하고 "Cargo.toml / plugin.json 참조"로 변경 (매번 수동 동기화 방지)
- **추가 검토**: Cargo.toml(`0.2.3`) vs plugin.json(`0.2.5`) 불일치 → 어느 쪽이 정본인지 확인 후 통일

---

### #15 repo_configs 테이블 → YAML 전환 반영

**파일**: `DESIGN.md` 섹션 4 (SQLite 스키마)

**현재 문제**:

DESIGN.md에 `repo_configs` 테이블이 정의되어 있지만, 실제 코드에서는:
- `schema.rs`에 `repo_configs` 테이블 없음
- `config/loader.rs`에서 YAML 파일 기반 설정 (`~/.develop-workflow.yaml` + 레포별 오버라이드)
- `config/models.rs`에 `WorkflowConfig` 구조체로 대체

**변경 방향**:
- DESIGN.md 섹션 4에서 `repo_configs` 테이블 정의 제거
- 새 섹션 추가: "설정 관리 (YAML 기반)"
  - 글로벌 설정: `~/.develop-workflow.yaml`
  - 레포별 오버라이드: `<worktree>/.develop-workflow.yaml`
  - 딥머지 전략 설명
  - `WorkflowConfig` 스키마 참조
- 섹션 6 (`/auto-setup`) — 설정 저장 방식을 YAML로 업데이트

---

### #10 git pull 실패 처리 강화

**파일**: `cli/src/workspace/mod.rs`

**현재 문제**:
```rust
if !status.success() {
    tracing::warn!("git pull failed for {repo_name}, continuing with existing state");
}
```

오래된 코드 기반으로 작업하면 이슈 분석/PR 리뷰 결과가 부정확해질 수 있습니다.

**변경 방향**:

```rust
if !status.success() {
    // fetch + reset으로 재시도
    let fetch = tokio::process::Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(&base)
        .status()
        .await?;

    if fetch.success() {
        let default_branch = detect_default_branch(&base).await.unwrap_or("main".into());
        let reset = tokio::process::Command::new("git")
            .args(["reset", "--hard", &format!("origin/{default_branch}")])
            .current_dir(&base)
            .status()
            .await?;

        if !reset.success() {
            tracing::error!("git reset failed for {repo_name}");
            anyhow::bail!("failed to update repository {repo_name}");
        }
    } else {
        tracing::error!("git fetch failed for {repo_name}");
        anyhow::bail!("failed to fetch repository {repo_name}");
    }
}
```

**대안 (보수적)**:
- warn → error 레벨만 상향
- consumer에서 pull 실패 시 해당 아이템을 `failed` 처리하여 오래된 코드로 작업하는 것을 방지

---

### #16 worktree 미정리 해결

**파일**: `cli/src/consumer/issue.rs`, `cli/src/consumer/pr.rs`

**현재 문제**:

성공한 merge 처리 후에만 `remove_worktree`가 호출됩니다 (`consumer/merge.rs:75`). issue consumer와 PR consumer에서 생성한 worktree는 성공/실패 모두 정리되지 않습니다.

**변경 방향**:

각 consumer에서 처리 완료(성공/실패 모두) 후 worktree 정리:

```rust
// issue.rs — process_pending 내 각 아이템 처리 후
let cleanup = || async {
    let _ = workspace::remove_worktree(env, &item.repo_name, &task_id).await;
};

match result {
    Ok(res) => {
        // ... 상태 업데이트 ...
        cleanup().await;
    }
    Err(e) => {
        db.issue_mark_failed(&item.id, &format!("session error: {e}"))?;
        cleanup().await;
    }
}
```

**주의사항**:
- 디버깅을 위해 `failed` 상태의 worktree는 유지하는 옵션도 고려
  - 설정에 `cleanup_on_failure: bool` 추가 (default: true)
  - 또는 failed worktree를 별도 경로로 이동 (`.autonomous/failed-workspaces/`)

**디스크 절약 효과**:
- 현재: 레포당 처리한 이슈/PR 수만큼 worktree 누적
- 변경 후: 현재 진행 중인 작업의 worktree만 존재

---

## 테스트 계획

- [ ] 기존 테스트 전체 통과
- [ ] DESIGN.md — 코드와의 불일치 항목 0개 (수동 검증)
- [ ] workspace cleanup — consumer 테스트에서 worktree 디렉토리 생성/삭제 확인
- [ ] git pull 실패 시나리오 — mock git binary로 실패 케이스 테스트

## 영향 범위

| 파일 | 변경 유형 |
|------|----------|
| `DESIGN.md` | 문서 전면 업데이트 |
| `workspace/mod.rs` | git pull 실패 처리 강화 |
| `consumer/issue.rs` | worktree cleanup 추가 |
| `consumer/pr.rs` | worktree cleanup 추가 |

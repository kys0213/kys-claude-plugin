---
name: draft-branch
description: Draft branch 라이프사이클, 승격 규칙, autopilot 설정 로딩 가이드. build-issues, qa-boost, branch-promoter 등 draft branch를 사용하는 모든 컴포넌트가 참조
version: 1.0.0
---

# Draft Branch Lifecycle & Configuration

## 설정 로딩

프로젝트 루트의 `github-autopilot.local.md` YAML frontmatter에서 설정을 읽는다.

```yaml
---
branch_strategy: "draft-main"       # "draft-develop-main" | "draft-main"
auto_promote: true                   # draft → feature 자동 승격
label_prefix: "autopilot:"          # GitHub 라벨 접두사
spec_paths:                          # 스펙 파일 탐색 경로
  - "spec/"
  - "docs/spec/"
default_intervals:                   # autopilot dispatcher 기본 인터벌
  gap_watch: "30m"
  build_issues: "15m"
  merge_prs: "10m"
  ci_watch: "20m"
  qa_boost: "1h"
---
```

설정 파일이 없으면 위 기본값을 사용한다.

## Branch 계층 구조

```
main              ← 프로덕션 (보호됨)
  │
develop           ← 통합 브랜치 (draft-develop-main 전략에서만 사용)
  │
feature/issue-42  ← PR 대상, 사람이 리뷰
  │
draft/issue-42    ← agent 전용 작업 공간
```

## Draft Branch 규칙

1. **로컬 전용**: draft/* 브랜치는 remote에 push하지 않는다
2. **자유롭게 작업**: agent는 draft에서 자유롭게 커밋, 수정, 재시도 가능
3. **worktree 사용**: 병렬 작업을 위해 `isolation: "worktree"`로 동작

## 네이밍 규칙

| 용도 | 패턴 | 예시 |
|------|------|------|
| 이슈 구현 | `draft/issue-{number}` | `draft/issue-42` |
| QA 테스트 | `draft/qa-{short-hash}` | `draft/qa-a1b2c3` |
| 승격 후 | `feature/issue-{number}` | `feature/issue-42` |
| QA 승격 후 | `feature/qa-{short-hash}` | `feature/qa-a1b2c3` |

## 승격 (Promote) 프로세스

### 승격 조건 (Quality Gate)

모든 조건을 통과해야 승격 가능:

```bash
# Rust 프로젝트
cargo fmt --check
cargo clippy -- -D warnings
cargo test

# Node.js 프로젝트
npm run lint
npm test

# 범용
# 프로젝트의 quality gate 명령어를 자동 감지하여 실행
```

### 승격 절차

```bash
# 1. draft에서 feature 브랜치 생성
git checkout -b feature/issue-{N} draft/issue-{N}

# 2. remote push
git push -u origin feature/issue-{N}

# 3. PR 생성 (branch_strategy에 따라 base 결정)
#    draft-main:         --base main
#    draft-develop-main: --base develop
gh pr create \
  --base {base_branch} \
  --title "feat(scope): issue #{N} description" \
  --label "{label_prefix}auto" \
  --body "Closes #{N}\n\nAutopilot 자동 구현"

# 4. draft 브랜치 정리
git branch -D draft/issue-{N}
```

## GitHub 라벨 체계

| 라벨 | 용도 | 생성 시점 |
|------|------|-----------|
| `{label_prefix}ready` | 구현 대상 이슈 | gap-watch |
| `{label_prefix}wip` | 구현 진행 중 | build-issues |
| `{label_prefix}auto` | autopilot 생성 PR | branch-promoter |
| `{label_prefix}ci-failure` | CI 실패 이슈 | ci-watch |
| `{label_prefix}qa` | QA 테스트 PR | qa-boost |

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
| 승격 후 | `feature/issue-{number}` | `feature/issue-42` |

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

라벨 체계, 필수 규칙, fingerprint 기반 중복 방지는 **issue-label 스킬**을 참조한다.

## Always Pull First (필수 규칙)

autopilot의 모든 agent와 command는 작업 시작 전 반드시 최신 변경사항을 가져와야 한다.

```bash
git fetch origin
# remote tracking 브랜치가 있는 경우:
git pull --rebase origin $(git branch --show-current)
```

**이유**: autopilot은 주기적으로 실행되므로, 이전 실행 이후 다른 agent나 사람이 변경한 내용을 반영하지 않으면 충돌이나 중복 작업이 발생한다.

## Draft Branch 금지 사항

- `draft/*` 브랜치를 `git push`하지 않는다 (로컬 only)
- `main`, `develop` 브랜치에 직접 커밋하지 않는다
- 기존 `feature/*` 브랜치를 덮어쓰지 않는다 (이미 존재하면 skip)
- Quality gate 통과 전에 승격하지 않는다
- 승격 후 draft 브랜치는 즉시 삭제한다
- PR 라벨에 `{label_prefix}auto`를 반드시 포함한다

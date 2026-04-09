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
work_branch: ""                      # 에이전트 작업 base 브랜치 (비어있으면 branch_strategy에 따라 결정)
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
  ci_fix: "15m"
  qa_boost: "1h"
notification: ""                     # skip 이슈 알림 방법 (자연어, 예: "Slack DM으로 @irene에게 알려줘")
quality_gate_command: ""             # 커스텀 quality gate 명령어 (비어있으면 자동 감지)
max_consecutive_failures: 3          # 연속 실패 허용 횟수, 초과 시 에스컬레이션
max_ci_fix_retries: 3                # CI fix 루프 최대 재시도 횟수
test_watch: []                       # 테스트 스위트 정의 (아래 예시 참조)
# test_watch:
#   - name: "e2e"
#     command: "npm run test:e2e"
#     interval: "2h"
#   - name: "performance"
#     command: "cargo bench"
#     interval: "6h"
#     threshold: 10
---
```

설정 파일이 없으면 위 기본값을 사용한다.

## Base 브랜치 결정

base 브랜치 결정 및 동기화는 **branch-sync** 스킬을 참조한다.

## Branch 계층 구조

### work_branch 미설정 (기본)

```
main              ← 프로덕션 (보호됨)
  │
develop           ← 통합 브랜치 (draft-develop-main 전략에서만 사용)
  │
feature/issue-42  ← PR 대상, 사람이 리뷰
  │
draft/issue-42    ← agent 전용 작업 공간
```

### work_branch 설정 시 (예: alpha)

```
main              ← 프로덕션 (보호됨)
  │
develop           ← (선택) 통합 브랜치
  │
alpha             ← work_branch: 에이전트 작업 base
  │
feature/issue-42  ← PR 대상 (base: alpha)
  │
draft/issue-42    ← agent 전용 작업 공간 (alpha에서 분기)
```

> `work_branch → develop → main` 승격은 autopilot 범위 밖이며, 수동으로 관리한다.

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

`quality_gate_command`가 설정되어 있으면 해당 명령어를 실행한다. 미설정 시 프로젝트 파일 기반으로 자동 감지한다.

```bash
# 커스텀 quality gate (설정 시 우선 사용)
# 예: quality_gate_command: "make lint && make test"
${quality_gate_command}

# 자동 감지 (quality_gate_command 미설정 시)
# Rust (Cargo.toml 존재)
cargo fmt --check && cargo clippy -- -D warnings && cargo test

# Node.js (package.json 존재)
npm run lint && npm test

# Go (go.mod 존재)
go fmt ./... && go vet ./... && go test ./...
```

### 승격 절차

```bash
# 1. draft에서 feature 브랜치 생성
git checkout -b feature/issue-{N} draft/issue-{N}

# 2. remote push
git push -u origin feature/issue-{N}

# 3. PR 생성 (base 브랜치 결정: work_branch > branch_strategy)
#    work_branch 설정 시:  --base {work_branch}
#    draft-main:           --base main
#    draft-develop-main:   --base develop
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

## Draft Branch 금지 사항

- `draft/*` 브랜치를 `git push`하지 않는다 (로컬 only)
- `main`, `develop` 브랜치에 직접 커밋하지 않는다
- 기존 `feature/*` 브랜치를 덮어쓰지 않는다 (이미 존재하면 skip)
- Quality gate 통과 전에 승격하지 않는다
- 승격 후 draft 브랜치는 즉시 삭제한다
- PR 라벨에 `{label_prefix}auto`를 반드시 포함한다

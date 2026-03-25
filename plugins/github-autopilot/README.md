# github-autopilot

CronCreate 기반 자율 개발 루프 — gap 탐지, 테스트 갭 발행, CI 감시, 이슈 구현, PR 머지를 자동화합니다.

## 아키텍처

```
┌─────────────────────────────────────────────────────────────────┐
│                       이슈 소스                                   │
├──────────────┬──────────────┬──────────────┬────────────────────-┤
│  gap-watch   │   qa-boost   │   ci-watch   │    사람 (HITL)      │
│  스펙 갭 탐지 │  테스트 갭 탐지│  CI 실패 감지 │   수동 이슈 등록     │
└──────┬───────┴──────┬───────┴──────┬───────┴────────┬──────────-┘
       │              │              │                │
       ▼              ▼              ▼                ▼
    :ready          :ready     :ready + :ci-failure  라벨 없음
       │              │              │                │
       │              │              │       /analyze-issue (사람 실행)
       │              │              │                │
       │              │              │          ┌─────┴─────┐
       │              │              │          ▼           ▼
       │              │              │        ready        skip
       │              │              │        :ready    (코멘트만)
       │              │              │          │           │
       └──────────────┴──────────────┴──────────┘           │
                                                            │
                       ┌────────────────────────────────────┘
                       │
 ┌─────────────────────┼──────────────────────────────────────────┐
 │  build-issues       │                                          │
 │                     ▼                                          │
 │  Step 3: skip 이슈 대기 중?                                     │
 │           │                                                    │
 │           ├─ notification 설정 있음 → 자연어 지시대로 알림 전송     │
 │           │  (MCP/Skill 활용, 예: "Slack DM으로 알려줘")          │
 │           │                                                    │
 │  Step 4: :ready 이슈 조회                                       │
 │           │                                                    │
 │  Step 5: 의존성 분석                                             │
 │           │                                                    │
 │  Step 6: :wip 라벨 추가                                         │
 │           │                                                    │
 │  Step 7: issue-implementer (worktree 병렬 구현)                  │
 │           │                                                    │
 │  Step 9: branch-promoter → PR + :auto 라벨                     │
 │           │                                                    │
 │  Step 10: :wip, :ready 제거                                     │
 └───────────┼────────────────────────────────────────────────────┘
             │
             ▼
 ┌──────────────────┐
 │    merge-prs     │  :auto PR 조회 → squash merge
 └──────────────────┘
```

## 이슈 소스

| 소스 | 설명 | 부여 라벨 |
|------|------|----------|
| `gap-watch` | 스펙 문서와 코드 사이의 갭을 탐지하여 이슈 발행 | `:ready` |
| `qa-boost` | 최근 변경사항의 테스트 커버리지 갭을 탐지하여 이슈 발행 | `:ready` |
| `ci-watch` | CI 실패를 분석하여 이슈 발행 | `:ready` + `:ci-failure` |
| `analyze-issue` (HITL) | 사람이 등록한 이슈를 분석하여 ready/skip 판정 | `:ready` (ready 판정만) |

모든 이슈는 `:ready` 라벨을 통해 단일 파이프라인(`build-issues`)에 합류합니다.

## HITL (Human-in-the-Loop)

사람이 직접 등록한 이슈는 자동으로 라벨이 붙지 않습니다.

1. 사람이 이슈 등록
2. `/analyze-issue #42` 실행 (사람이 트리거)
3. ready → `:ready` 라벨 → build-issues 파이프라인 진입
4. skip → 코멘트만 게시, 다음 build-issues tick에서 `notification` 설정에 따라 알림

`notification` 설정은 자연어로 지정합니다:

```yaml
notification: "Slack DM으로 @irene에게 알려줘"
```

## 라벨

| 라벨 | 용도 |
|------|------|
| `:ready` | 구현 대상 — 유일한 파이프라인 진입점 |
| `:wip` | 구현 진행 중 — 중복 작업 방지 |
| `:ci-failure` | CI 실패 표시 — `:ready`와 함께 부여 |
| `:auto` | autopilot PR — `merge-prs` 대상 |

라벨 접두사(`label_prefix`)는 설정에서 변경 가능합니다 (기본값: `autopilot:`).

모든 라벨은 `/github-autopilot:setup`에서 일괄 생성됩니다.

## 중복 방지

이슈 body에 fingerprint를 HTML 주석으로 삽입하고, `scripts/check-duplicate.sh`로 생성 전 검색합니다.

| 소스 | fingerprint 형식 | 예시 |
|------|-----------------|------|
| `gap-watch` | `gap:{spec_path}:{keyword}` | `gap:spec/auth.md:token-refresh` |
| `qa-boost` | `qa:{source_path}:{test_type}` | `qa:src/auth/refresh.rs:unit` |
| `ci-watch` | `ci:{workflow}:{branch}:{failure_type}` | `ci:validate.yml:main:test-failure` |

```bash
# 중복 확인 — exit 0이면 생성 가능, exit 1이면 중복
bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-duplicate.sh "gap:spec/auth.md:token-refresh"
```

## 커맨드

| 커맨드 | 설명 |
|--------|------|
| `/github-autopilot:setup` | 초기 설정 (rules, 설정 파일, 라벨 생성) |
| `/github-autopilot:autopilot` | 5개 루프를 설정된 인터벌로 모두 시작 |
| `/github-autopilot:gap-watch [interval]` | 스펙 갭 탐지 → 이슈 발행 |
| `/github-autopilot:qa-boost [commit] [interval]` | 테스트 갭 탐지 → 이슈 발행 |
| `/github-autopilot:ci-watch [interval]` | CI 실패 감지 → 이슈 발행 |
| `/github-autopilot:build-issues [interval]` | `:ready` 이슈 구현 → PR |
| `/github-autopilot:merge-prs [interval]` | `:auto` PR 머지 |
| `/github-autopilot:analyze-issue <numbers>` | 사람 이슈 분석 (HITL) |

## 에이전트

| 에이전트 | model | 호출 위치 | 역할 |
|----------|-------|----------|------|
| `gap-detector` | - | gap-watch | 스펙 파싱 → 구조 매핑 → call chain 갭 분석 |
| `gap-issue-creator` | haiku | gap-watch | 갭 리포트 → GitHub 이슈 생성 (fingerprint 중복 검사 포함) |
| `issue-analyzer` | sonnet | analyze-issue | 이슈 분석 → ready/skip 판정 (HITL) |
| `issue-dependency-analyzer` | - | build-issues | 이슈 간 의존성 → 배치 분류 |
| `issue-implementer` | opus | build-issues | worktree에서 이슈 구현 |
| `branch-promoter` | haiku | build-issues | draft → feature 브랜치 승격 + PR (:auto 라벨) |
| `pr-merger` | - | merge-prs | PR 문제 해결 (conflict, CI 실패) |
| `ci-failure-analyzer` | - | ci-watch | CI 로그 분석 → 실패 원인 리포트 |

## 설정

`github-autopilot.local.md` YAML frontmatter:

```yaml
---
branch_strategy: "draft-main"
auto_promote: true
label_prefix: "autopilot:"
spec_paths:
  - "spec/"
  - "docs/spec/"
default_intervals:
  gap_watch: "30m"
  build_issues: "15m"
  merge_prs: "10m"
  ci_watch: "20m"
  qa_boost: "1h"
notification: ""
---
```

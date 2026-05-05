---
description: 세션 작업을 회고하고 검증 gap과 개선점을 도출합니다
argument-hint: "[회고 관점이나 강조 영역]"
allowed-tools:
  - Bash
  - Task
  - AskUserQuestion
---

# Session Retrospective (/retro)

세션의 작업을 회고하고, 검증 gap과 개선점을 도출합니다.

> `/develop` 워크플로우의 종료 단계로 사용하거나, 임의의 세션 마무리 시점에 단독으로 실행할 수 있습니다.

## Design Principle

- **Main agent는 diff/코드를 직접 읽지 않는다** — context 비용 최소화
- 분석은 기존 플러그인/에이전트에 위임하고 **결과만 취합**
- 세션 대화 맥락에서 "뭘 했고, 뭘 발견했는지"는 main agent가 이미 알고 있음

## 사용법

```bash
# 기본 회고 (대화 맥락 + 변경사항 자동 분석)
/retro

# 강조 영역 지정
/retro "테스트 커버리지 관점으로 회고"
/retro "spec ↔ 구현 일치 여부 위주로"
```

## Execution

### Step 1: Lightweight Summary

변경 범위를 경량으로 수집한다 (diff 내용은 읽지 않음):

```bash
git log --oneline <base>..HEAD
git diff --stat <base>..HEAD
gh issue list --state open --search "created:>=<session-start>"
```

> base 브랜치는 현재 브랜치의 upstream 또는 default 브랜치(main)를 사용합니다.
> diff 본문은 읽지 않고 stat만 수집해 토큰 비용을 최소화합니다.

### Step 2: Categorize from Context

대화 맥락에서 작업을 분류합니다 (코드를 다시 읽지 않음):

| 분류 | 설명 |
|------|------|
| Planned | 원래 목표 |
| Discovered | 작업 중 발견된 버그/이슈 |
| Scope Creep | 원래 범위 밖이지만 진행 |
| Deferred | 이슈로 등록하고 미뤄둔 항목 |

### Step 3: Delegate Analysis (Parallel)

설치 여부에 따라 다음 분석을 **병렬**로 위임합니다 (`Task` tool, `run_in_background=true`):

| Agent/Skill | 역할 | 조건 |
|---|---|---|
| `git-utils:branch-status` | 변경사항 + 남은 작업 정리 | 항상 |
| `github-autopilot:qa-boost` | 테스트 커버리지 gap | 테스트가 있는 프로젝트 |
| `spec-kit:gap-detect` | 스펙 ↔ 구현 불일치 | `spec/` 또는 스펙 문서가 있는 프로젝트 |
| `verify-rules` | `.claude/rules` 위반 | rules가 있는 프로젝트 |

> **플러그인 미설치 시**: 해당 단계는 skip하고 리포트에 "(skipped: plugin not installed)"로 표기합니다.

### Step 4: Verification Gap Analysis

위임 결과를 종합하여 다음 gap 유형으로 분류합니다:

| Gap 유형 | 설명 |
|---|---|
| Unit test gap | mock/stub으로 인한 실제 동작과의 괴리 |
| Integration gap | 레이어 간 연결 미검증 |
| E2E gap | 전체 사용자 흐름 미검증 |
| Spec gap | 스펙/문서와 구현 불일치 |
| DX gap | 로컬 개발/테스트 환경 미비 |
| CI gap | 자동화 검증 누락 |

### Step 5: Report

표준 리포트 템플릿으로 출력합니다:

```markdown
# Session Retrospective

## Summary
- 기간: <session-start> ~ <now>
- Base: <base-branch>, Head: <current-branch>
- Commits: N개, Changed files: M개

## Work Breakdown
- Planned: ...
- Discovered: ...
- Scope Creep: ...
- Deferred (issue): ...

## Verification Gaps
- Unit test gap: ...
- Integration gap: ...
- E2E gap: ...
- Spec gap: ...
- DX gap: ...
- CI gap: ...

## Spec ↔ Rules Consistency
- spec-kit:gap-detect 결과 요약
- verify-rules 결과 요약

## Takeaways
- 핵심 학습 1~3개

## Next Priorities
- 1~5순위 후속 작업 (이슈 등록 여부 표시)
```

### Step 6: Suggest Improvements

리포트의 **Next Priorities** 중 GitHub 이슈로 미등록된 항목을 `AskUserQuestion`으로 사용자에게 확인 후, 승인된 항목만 `gh issue create` 또는 `git-utils create-issue` 스킬로 등록합니다.

## 에러 처리

**git 저장소가 아닌 경우:**
```
이 디렉토리는 git 저장소가 아닙니다. /retro는 git 저장소에서만 동작합니다.
```
> 회고를 중단합니다.

**base 브랜치를 식별할 수 없는 경우:**
- 사용자에게 `AskUserQuestion`으로 base 브랜치를 직접 받거나 default 브랜치(main)로 fallback합니다.

**Step 3 위임 대상 플러그인이 모두 미설치인 경우:**
- Step 3을 통째로 skip하고, Step 4 gap 분석을 대화 맥락만으로 수행합니다.
- 리포트 상단에 "(no delegated analysis available)"을 명시합니다.

## Output Examples

### 성공 (전 단계 정상 수행)

```
# Session Retrospective

## Summary
- 기간: 2026-05-05 09:00 ~ 12:30
- Base: main, Head: feat/retro-migration
- Commits: 4개, Changed files: 6개

## Work Breakdown
- Planned: retro 커맨드 이관, README 갱신
- Discovered: develop-workflow에 README 부재 → 상위 README에 통합
- Deferred: retro 호출 통계 수집 (별도 이슈)

## Verification Gaps
- DX gap: ~/.claude/commands 정리 안내 필요
- CI gap: plugin command spec 검증 통과 (없음)

## Next Priorities
1. 사용자 환경에서 ~/.claude/commands/retro.md 제거 가이드 작성
2. /retro 사용 통계 텔레메트리 (deferred → issue)
```

### Skip 케이스 (의존 플러그인 미설치)

```
(no delegated analysis available — git-utils, github-autopilot, spec-kit 미설치)

# Session Retrospective
...
```

## Notes

- 세션 마무리 시점에 `/retro` 실행 권장.
- 핵심 takeaway는 사용자가 별도로 memory/문서에 기록.
- 위임 대상 플러그인이 부분적으로만 설치되어 있어도 동작하며, skip된 단계는 리포트에 표기됩니다.
- 다른 슬래시 커맨드(`/simplify`, `/review` 등)와 자유롭게 결합해 사용하세요.

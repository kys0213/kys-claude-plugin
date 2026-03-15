---
description: Claw 규칙 확인/편집
argument-hint: "<action> [rule] [--repo <name>]"
allowed-tools: ["AskUserQuestion", "Bash", "Read", "Edit"]
---

# Claw 설정 (/claw)

Claw 워크스페이스의 규칙을 확인하고 편집합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/claw rules [--repo <name>]` — 현재 적용 규칙 확인
- `/claw edit <rule> [--repo <name>]` — 규칙 편집

## 실행

### rules

적용 중인 규칙 목록을 조회합니다:

```bash
autodev claw rules --json [--repo <name>]
```

결과를 출력합니다:

```
📐 Claw 적용 규칙:

  글로벌 (~/.autodev/claw-workspace/.claude/rules/):
    scheduling.md        스케줄링 정책
    branch-naming.md     브랜치 네이밍 전략
    review-policy.md     리뷰 정책
    decompose-strategy.md 스펙 분해 전략
    hitl-policy.md       HITL 판단 기준

  레포 오버라이드 (org/repo-a):
    review-policy.md     (글로벌 오버라이드)
```

### edit

지정된 규칙 파일을 Read로 읽고 내용을 출력한 후, 사용자와 대화하며 Edit으로 수정합니다.

**규칙 파일 경로 결정**:

- `--repo` 없음 → `~/.autodev/claw-workspace/.claude/rules/<rule>.md`
- `--repo` 있음 → `~/.autodev/workspaces/<org-repo>/claw/.claude/rules/<rule>.md`

1. 해당 파일을 Read로 읽어 현재 내용을 출력합니다.
2. 사용자에게 어떤 부분을 변경할지 물어봅니다.
3. 사용자 요청에 따라 Edit으로 파일을 수정합니다.
4. 수정 결과를 확인하여 출력합니다.

```
✅ 규칙 수정 완료: branch-naming.md

변경 내용:
  - hotfix 브랜치 패턴 추가: hotfix/{이슈번호}
  - release 브랜치 패턴 추가: release/{버전}/{이슈번호}

Claw가 다음 세션부터 업데이트된 규칙을 적용합니다.
```

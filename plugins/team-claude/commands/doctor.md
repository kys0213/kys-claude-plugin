---
description: Team Claude 자가 진단 및 자동 수정
allowed-tools: ["Read", "Bash", "AskUserQuestion"]
---

# Team Claude Doctor

환경 진단 및 문제 자동 수정 도구입니다.

## Quick Start

```bash
# 진단만 실행
tc doctor

# 자동 수정 모드
tc doctor --fix

# JSON 출력
tc doctor --json

# 특정 카테고리만 검사
tc doctor --category server
```

## 진단 항목

| 카테고리 | 검사 항목 |
|---------|----------|
| Infrastructure | yq, jq, git, bun, curl 설치 여부 |
| Server | 바이너리 존재, 실행 상태, health check |
| Configuration | team-claude.yaml 유효성, 필수 필드 |
| Hooks | tc CLI 설치, hooks 설정, 레거시 스크립트 |
| State | workflow.json, psm-index.json 일관성 |
| Worktrees | 고아 worktree 감지 |

## 자동 수정 (--fix)

--fix 플래그 사용 시 다음을 자동 수정합니다:
- 누락된 디렉토리 생성 (sessions, state, worktrees)
- 손상된 상태 파일 초기화
- 레거시 .sh 스크립트 정리 (확인 후)
- 서버 재시작

## 워크플로우

```
tc doctor
    │
    ▼
┌─────────────────┐
│ 전체 진단 실행  │
│ (6개 카테고리)  │
└─────────────────┘
    │
    ├── 문제 없음 → ✅ 완료
    │
    └── 문제 발견
         │
         ▼
    ┌─────────────────┐
    │ --fix 모드?     │
    └─────────────────┘
         │
    ┌────┴────┐
    No        Yes
    │         │
    ▼         ▼
  문제 보고   자동 수정 시도
              │
              ▼
         확인 필요 시
         사용자 질문
```

## 출력 예시

```
━━━ Team Claude Doctor ━━━

📦 Infrastructure
  ✓ yq (4.35.1)
  ✓ jq (jq-1.7)
  ✓ git (2.42.0)
  ✓ bun (1.0.0)
  ✓ curl

🖥️  Server
  ✓ Binary: ~/.claude/team-claude-server
  ✓ Status: healthy (port 7890)

⚙️  Configuration
  ✓ team-claude.yaml exists
  ✓ Required fields present
  ✓ YAML valid

🪝 Hooks
  ✓ tc CLI available
  ✓ settings.local.json configured
  ⚠ Legacy script: on-worker-complete.sh (fixable)

📊 State
  ✓ workflow.json
  ✓ psm-index.json

🌳 Worktrees
  ✓ No orphan worktrees

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ 진단 완료: 오류 0개, 경고 1개, 수정가능 1개
→ 자동 수정: tc doctor --fix
```

## JSON 출력

```bash
tc doctor --json | jq '.summary'
```

```json
{
  "total": 15,
  "ok": 14,
  "errors": 0,
  "warnings": 1,
  "fixable": 1
}
```

## 참고

- 진단은 비파괴적입니다 (읽기 전용)
- --fix 모드에서도 파괴적 작업은 확인을 요청합니다
- 서버 관련 문제는 `tc server` 명령어로도 해결 가능합니다

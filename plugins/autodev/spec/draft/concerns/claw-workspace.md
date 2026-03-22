# Claw 워크스페이스 — 판단 레이어 설정 + 세션

> 사용자가 Claw의 판단 방식을 자연어로 커스터마이즈하고, Claw 세션으로 상호작용한다.

---

## 워크스페이스 구조

```
~/.autodev/claw-workspace/
├── CLAUDE.md                         # 판단 원칙
├── .claude/rules/
│   ├── scheduling.md                 # 큐 평가 규칙
│   ├── branch-naming.md              # 브랜치 네이밍
│   ├── review-policy.md              # 리뷰 정책
│   ├── decompose-strategy.md         # 스펙 분해 전략
│   ├── hitl-policy.md                # HITL 판단 기준
│   └── auto-approve-policy.md        # 자동 승인 기준
├── commands/                         # Claw 세션용 커맨드
└── skills/
    ├── decompose/                    # 스펙 분해
    ├── gap-detect/                   # gap 탐지
    └── prioritize/                   # 우선순위 판단
```

Per-repo 오버라이드: `~/.autodev/workspaces/org-repo/claw/` (같은 파일명 = 오버라이드)

---

## /claw 세션 진입 경험

```
/claw 실행 →

Step 1: 상태 수집
  autodev status --json
  autodev hitl list --json
  autodev decisions list --json -n 3
  autodev spec list --json

Step 2: 요약 표시

  📊 autodev 상태 요약

  ● daemon running (uptime 2h 15m)

  Repos:
    org/repo-a — queue: 1R 2P | specs: auth-v2 ████████░░ 60%

  ⚠ HITL 대기: 1건
    → #44 Session adapter — 3회 실패

  🧠 최근 판단:
    14:30 advance #42 | 14:25 decompose auth-v2

  무엇을 도와드릴까요?

Step 3: 자연어 대화
  → Bash tool로 autodev CLI 호출
```

---

## 자연어 → CLI 매핑 예시

```
"지금 상황 어때?"      → autodev status --format rich
"큐 막힌 거 있어?"     → autodev queue list --json → 분석
"413번 진행시켜"       → autodev queue advance issue:org/repo:413
"HITL 대기 목록"       → autodev hitl list --json
"cron 일시정지"        → autodev cron pause gap-detection
"뭐 하면 좋을까?"     → status + hitl + queue 종합 → 추천
```

---

## Plugin slash command 통합

```
v4 (15개) → v5 (3개):
  /auto   — 데몬 제어 (start/stop/setup/config/dashboard/update)
  /spec   — 스펙 CRUD (add/update/list/status/remove/pause/resume)
  /claw   — 대화 세션 (나머지 모든 조회/조작을 자연어로)
```

---

## Claw CLI 호출 메커니즘

Bash tool (확정). Claude Code의 Bash tool로 autodev CLI 실행.
추가 구현 불필요. autodev CLI가 --json 출력 지원.

---

## Agent와 AgentRuntime 연동

```yaml
# .autodev.yaml
runtime:
  default: claude
  overrides:
    claw_evaluate: claude    # Claw headless는 이 런타임 사용
```

agent .md 파일의 `model` 필드는 해당 runtime 내 모델 선택:

```markdown
# agents/issue-analyzer.md
---
model: sonnet     # ClaudeRuntime → --model sonnet
---               # GeminiRuntime → 무시 (기본 모델)
```

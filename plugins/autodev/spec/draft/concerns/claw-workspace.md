# Claw — 대화형 에이전트

> Claw = `/claw` 세션. 자연어로 시스템을 조회하고 조작하는 대화형 인터페이스.
>
> 분류(Done or HITL)는 **코어 evaluate 함수**가 담당한다. Claw는 분류기가 아니다.

---

## 코어 evaluate (참고)

분류 로직은 코어에 속한다. Claw와 무관.

```
handler 실행 완료
    │
    ▼
코어 evaluate:
  "완료 처리해도 되나? 사람이 봐야 하나?"
    │
    ├── Done → on_done 액션 실행 → 다음 state trigger
    └── HITL → HITL 이벤트 생성 → 사람 대기
```

evaluate cron: `interval 60s + force trigger on task complete`, 완료된 아이템 중 미분류 건이 있을 때만 실행.

---

## 대화형 에이전트 (/claw 세션)

v4와 동일하게 어디서든 실행 가능한 대화형 인터페이스.

### 진입 경험

```
/claw 실행 →

Step 1: 상태 수집
  autodev status --json
  autodev hitl list --json
  autodev decisions list --json -n 3

Step 2: 요약 표시

  ● daemon running (uptime 2h 15m)

  Repos:
    org/repo-a — queue: 1R 2P | specs: auth-v2 60%

  ⚠ HITL 대기: 1건
    → #44 Session adapter — 3회 실패

  무엇을 도와드릴까요?

Step 3: 자연어 대화
  → Bash tool로 autodev CLI 호출
```

### 자연어 → CLI 매핑 예시

```
"지금 상황 어때?"      → autodev status --format rich
"큐 막힌 거 있어?"     → autodev queue list --json → 분석
"HITL 대기 목록"       → autodev hitl list --json
"cron 일시정지"        → autodev cron pause gap-detection
"뭐 하면 좋을까?"     → status + hitl + queue 종합 → 추천
```

---

## 워크스페이스 구조

```
~/.autodev/claw-workspace/
├── CLAUDE.md                         # 판단 원칙
├── .claude/rules/
│   ├── classify-policy.md            # Done vs HITL 분류 기준
│   ├── hitl-policy.md                # HITL 판단 기준
│   └── auto-approve-policy.md        # 자동 승인 기준
├── commands/
└── skills/
    ├── gap-detect/
    └── prioritize/
```

Per-repo 오버라이드: `~/.autodev/workspaces/org-repo/claw/`

---

## Plugin slash command 통합

```
v4 (15개) → v5 (3개):
  /auto   — 데몬 제어 (start/stop/setup/config/dashboard/update)
  /spec   — 스펙 CRUD (add/update/list/status/remove/pause/resume)
  /claw   — 대화 세션 (조회/조작/모니터링을 자연어로, 읽기 전용 CLI 흡수)
```

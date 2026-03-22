# Claw — 분류기 + 대화형 에이전트

> Claw는 두 가지 모드로 동작한다:
> 1. **Daemon 내부 분류기** — handler 실행 결과를 보고 Done or HITL 분류
> 2. **대화형 에이전트** — `/claw` 세션에서 자연어로 시스템 조회/조작

---

## 모드 1: 분류기

Claw의 Daemon 내부 역할은 **최소한의 판단**만 한다.

```
handler 실행 완료
    │
    ▼
Claw evaluate:
  "완료 처리해도 되나? 사람이 봐야 하나?"
    │
    ├── Done → on_done 액션 실행 → 다음 state trigger
    └── HITL → HITL 이벤트 생성 → 사람 대기
```

스펙 적합성, 코드 품질, gap 검출은 Claw가 아닌 **Cron 품질 루프**가 담당.

### claw-evaluate cron

```
interval: 60s (+ force trigger on task complete)
guard: 완료된 아이템 중 미분류 건이 있을 때만 실행
```

---

## 모드 2: 대화형 에이전트 (/claw 세션)

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
  /claw   — 대화 세션 (나머지 모든 조회/조작을 자연어로)
```

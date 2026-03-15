---
description: 칸반 보드 출력 — 큐 아이템을 상태별로 시각화
argument-hint: "[repo]"
allowed-tools: ["Bash"]
---

# 칸반 보드 (/board)

큐 아이템을 칸반 보드 형식으로 출력합니다. 전체 또는 레포별로 조회할 수 있습니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/board` — 전체 칸반 보드
- `/board <repo>` — 특정 레포의 칸반 보드

## 실행

### Step 1: 큐 데이터 수집

인자로 레포가 주어진 경우:

```bash
autodev queue list --json --repo <repo>
```

인자가 없으면 전체 큐를 조회합니다:

```bash
autodev queue list --json
```

### Step 2: 칸반 보드 렌더링

수집된 큐 아이템을 상태별로 분류하여 칸반 보드 형식으로 출력합니다:

```
📋 칸반 보드 — org/repo-a

┌─ Pending ──────┬─ Ready ─────────┬─ Running ────────┬─ Done ──────────┐
│ #45 Error      │ #46 Missing     │ #44 Session      │ #42 JWT middle  │
│     handling   │     tests       │     adapter      │     ware        │
│                │                 │                  │ #43 Token API   │
└────────────────┴─────────────────┴──────────────────┴─────────────────┘

진행도: 2/5 (40%)  |  HITL 대기: 0건  |  실패: 0건
```

레포가 지정되지 않은 경우 레포별로 구분하여 출력합니다.

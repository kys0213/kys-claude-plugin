---
description: 전체 레포/스펙 상태 요약
allowed-tools: ["Bash"]
---

# 전체 상태 요약 (/status)

등록된 모든 레포와 스펙의 현재 상태를 한눈에 보여줍니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/status` — 전체 상태 요약

## 실행

### Step 1: 데몬 상태 확인

```bash
autodev status --json
```

### Step 2: 레포별 상태 수집

```bash
autodev repo list --json
```

각 레포에 대해 스펙 정보를 수집합니다:

```bash
autodev spec list --json --repo <name>
```

### Step 3: HITL 대기 현황

```bash
autodev hitl list --json
```

### Step 4: 결과 출력

수집된 정보를 자연어로 정리하여 출력합니다:

```
🦀 autodev 상태

데몬: ● 실행 중 (PID: 12345, uptime: 2h 30m)

등록된 레포:
  org/repo-a  Auth Module v2 [Active] 3/5 (60%)  HITL: 1건
  org/repo-b  Payment Gateway [Active] 1/4 (25%)
  org/repo-c  (스펙 없음, Issue 모드)

HITL 대기: 1건
  [HIGH] org/repo-a PR #42 리뷰 3회 반복

최근 활동:
  10분 전  org/repo-a #43 → done
  25분 전  org/repo-b #51 → implementing
```

데몬이 중지 상태면 `autodev start`로 시작하라고 안내합니다.

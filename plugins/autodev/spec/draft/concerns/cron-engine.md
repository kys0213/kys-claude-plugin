# Cron 엔진 — 주기 실행 + 품질 루프

> 주기적으로 실행되는 작업을 관리한다.
> 파이프라인은 1회성, 품질은 Cron이 지속 감시하여 새 아이템을 생성.

---

## 두 가지 역할

```
1. 인프라 유지 — hitl-timeout, log-cleanup, daily-report (결정적)
2. 품질 루프 — gap-detection, QA, knowledge-extract (새 아이템 생성 가능)
```

---

## 품질 루프

파이프라인이 아이템을 처리한 후, Cron이 지속적으로 결과물을 검증한다.

```
Pipeline: issue → analyze → implement → review → Done
                                                   │
Cron: gap-detection ─── 스펙 vs 코드 비교 ──────────┘
        │
        ▼
      gap 발견 → 중복 검사 → 새 이슈 생성 → DataSource.collect() → 파이프라인 재진입
```

부족하면 되돌아가는 게 아니라 **새 아이템이 생긴다**.

### Dedupe 가드

gap-detection이 이슈를 생성하기 전, DataSource의 현재 open 아이템 목록을 조회하여 **동일 gap에 대한 아이템이 이미 존재하면 skip**한다.

```
gap 발견 → DataSource에서 open 아이템 조회 (Pending/Ready/Running)
  → 동일 gap에 해당하는 아이템 존재 → skip (이미 처리 중)
  → 해당 아이템 없음 → 새 이슈 생성
```

이를 통해 동일 문제에 대한 이슈 무한 증식을 방지한다.

---

## 기본 Cron Jobs

### 인프라 (Global, 결정적, 토큰 0)

| Job | 주기 | 동작 |
|-----|------|------|
| hitl-timeout | 5분 | 미응답 HITL 만료 처리 |
| daily-report | 매일 06시 | 일간 리포트 |
| log-cleanup | 매일 00시 | 오래된 로그/worktree 삭제 |

### 품질 루프 (Per-workspace, LLM 사용)

| Job | 주기 | 동작 |
|-----|------|------|
| evaluate | 60초 | 완료 아이템 분류 (Done or HITL) |
| gap-detection | 1시간 | 스펙-코드 대조, gap 발견 시 이슈 생성 |
| knowledge-extract | 1시간 | merged PR 지식 추출 |

### 사용자 정의 (예시)

| Job | 주기 | 동작 |
|-----|------|------|
| qa-test | 30분 | 테스트 실행, 실패 시 이슈 생성 |
| security-scan | 2시간 | 보안 취약점 스캔 |

---

## Force Trigger

코어 이벤트에서 evaluate를 즉시 트리거:

```
task 완료/실패 → force_trigger("evaluate")
  → last_run_at = NULL → 다음 tick에서 즉시 실행
```

---

## 스크립트 구조

```bash
#!/bin/bash
# Guard
PENDING=$(autodev queue list --workspace "$AUTODEV_WORKSPACE_NAME" --json | jq 'length')
if [ "$PENDING" = "0" ]; then exit 0; fi

# 실행
autodev agent --workspace "$AUTODEV_WORKSPACE_NAME" -p "큐를 평가해줘"
```

---

## 환경변수 주입

| 변수 | 예시 |
|------|------|
| `AUTODEV_WORKSPACE_NAME` | `auth-project` |
| `AUTODEV_WORKSPACE_ROOT` | `/Users/me/repos/repo-a` |
| `AUTODEV_HOME` | `~/.autodev` |
| `AUTODEV_DB` | `~/.autodev/autodev.db` |
| `AUTODEV_CLAW_WORKSPACE` | `~/.autodev/claw-workspace` |

---

## Built-in vs Custom

| | Built-in | Custom |
|---|---|---|
| 생성 | workspace 등록 시 자동 | `autodev cron add` |
| 제거 | 불가 (pause/resume) | 자유 |
| Guard | 내장 | 사용자 정의 |

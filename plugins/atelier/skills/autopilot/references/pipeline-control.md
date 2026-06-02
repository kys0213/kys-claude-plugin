# 공통 파이프라인 제어

거의 모든 autopilot 커맨드가 본 작업 전에 수행하는 전처리 3단계. 기존에 각 커맨드(build-issues·ci-watch·ci-fix·merge-prs·gap-watch·qa-boost)가 중복 서술하던 것을 단일 출처로 수렴한 것이다. **로직 변경 시 이 파일만 수정**한다.

## 1. Base 브랜치 동기화

`branch-sync` 스킬의 절차를 수행한다.

## 2. Pipeline Idle / Capacity Check

`max_parallel_agents` (설정값, 기본 3) 를 capacity 로 전달하여 wip 라벨 수가 capacity 에 도달했는지 한 번의 호출로 함께 판정한다 (동시 실행 충돌 사전 차단).

```bash
autopilot pipeline idle \
  --label-prefix "{label_prefix}" \
  --max-parallel ${max_parallel_agents}
```

| exit code | 의미 | 동작 |
|-----------|------|------|
| `0` (idle) | `ready + wip + prs == 0` | `notification` 설정이 있으면 "autopilot 파이프라인 완료 — {cmd} cycle 중단" 알림 발송 후 종료 |
| `1` (active, 여유 있음) | wip < `max_parallel_agents` | 본 작업으로 정상 진행 |
| `3` (at-capacity) | wip ≥ `max_parallel_agents` | "capacity full (wip: ${WIP_COUNT}/${max_parallel_agents}) — next cron tick에 재시도" 출력 후 **즉시 종료**. 모든 agent 호출 건너뜀 (이번 cycle 비용 0) |
| `2` (error) | 실행 환경 오류 | 에러 메시지 출력 후 이번 cycle skip |

> capacity 검사가 필요 없는 커맨드(예: 단순 조회)는 `--max-parallel` 생략 시 기존 idle/active 2값 동작으로 동작한다. capacity 충돌 가능성이 있는 커맨드(build-issues·merge-prs·ci-fix 등)는 `--max-parallel` 을 필수 전달한다.

## 3. Idle Count + Adaptive Throttling

이전 단계 결과가 "대상 없음"(idle)이면 CLI 로 idle 횟수를 기록하고 출력에서 `idle_count` 를 읽는다.

```bash
autopilot check mark {cmd} --status idle
# 출력: marked {cmd}: abc1234 at 2026-04-13T10:00:00Z (idle_count: 4)
```

설정의 `idle_shutdown.max_idle` (기본 5) 에 따라 cron 간격을 동적 조정한다:

| idle_count | 동작 |
|------------|------|
| 1~3 | 현재 간격 유지 |
| 4~`max_idle`-1 | CronList 로 현재 cron 을 찾아 CronDelete 후, 간격 2배로 CronCreate 재등록 |
| `max_idle` 이상 | CronList 로 현재 cron 을 찾아 CronDelete. "연속 {N}회 idle — cron 자동 해제" 출력 후 종료 |

> 간격 확대는 한 번만 적용된다 (4회째에 2배로 변경 후, 5~max_idle-1 까지 유지).

실제 작업을 수행하면 idle count 를 리셋하고, 간격이 변경되었으면 원래 간격으로 복원한다:

```bash
autopilot check mark {cmd} --status active
# idle_count 가 0으로 리셋 → 간격 복원 필요 시 CronDelete + CronCreate
```

> `{cmd}` 는 호출 커맨드 이름(build-issues, gap-watch, ci-watch, merge-prs, qa-boost, work-ledger 등)으로 치환한다. loop 이름이 커맨드별로 분리되어 idle count 가 독립 추적된다.

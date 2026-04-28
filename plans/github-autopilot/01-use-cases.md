# 01. Use Cases

> Epic 기반 Task Store 가 만족해야 하는 시나리오를 페르소나 관점에서 정의한다. 본 문서는 02 (아키텍처) 와 03 (상세 스펙) 의 인터페이스 도출 출발점이며, 04 (테스트) 의 fixture 원본이 된다.
>
> 각 시나리오는 다음 항목으로 구성된다:
>
> - **트리거** — 시나리오를 시작시키는 외부 이벤트
> - **선행 조건** — 시나리오 시작 시점의 시스템 상태
> - **주 흐름** — 정상 경로의 단계
> - **대안 흐름** — 분기 / 실패 경로
> - **사후 조건** — 시나리오 종료 시점에 보장되어야 하는 상태
> - **관찰 가능 결과** — 외부에서 검증 가능한 부수 효과 (DB 행, git ref, GitHub 이슈/PR)

## 페르소나

| 코드 | 이름 | 역할 |
|------|------|------|
| **OP** | 운영자 (Operator) | autopilot 을 호스트하고 epic 을 시작/감독하는 사람 |
| **ML** | 메인 루프 (Main Loop) | epic 단위로 도는 부모 에이전트. DB 의 단일 writer |
| **IM** | 구현자 (Implementer) | worktree 안에서 코드를 작성하고 push 하는 자식 에이전트 |
| **CO** | 협업자 (Collaborator) | 같은 레포에서 일하는 다른 사람. autopilot 을 직접 다루지 않을 수도 있음 |
| **RV** | 리뷰어 (Reviewer) | PR 을 검토하는 사람 |

페르소나 간 직접 통신은 다음 경로로만 일어난다:

```
OP → ML : CLI 명령 (/github-autopilot:epic-start 등)
ML → OP : escalation 이슈, 완료 알림
ML → IM : task 사양 + 격리된 worktree
IM → ML : 브랜치 push (성공/실패) — DB 는 직접 안 만짐
ML ↔ git remote / GitHub : PR / 이슈 / branch
CO → GitHub : 이슈, PR 코멘트
CO ← ML : escalation 이슈 (CO 도 처리 가능)
```

## UC-1. Epic 시작

> 운영자가 새 spec 으로부터 자율 작업 권한을 부여한다.

- **트리거**: OP 가 `autopilot epic start --spec spec/auth.md --name auth-token-refresh` 실행
- **선행 조건**:
  - 현재 working tree 가 main 브랜치에 깨끗하게 위치
  - `spec/auth.md` 파일이 존재
  - 같은 이름의 active epic 이 없음
- **주 흐름**:
  1. ML 이 `epic/auth-token-refresh` 브랜치를 main 에서 분기하여 push
  2. spec 분해 결과로부터 결정적 task_id 들을 부여
  3. tasks 테이블에 status=pending 으로 모두 insert
  4. 의존성 그래프를 `task_deps` 에 insert
  5. 진입점 task (deps 없음) 들의 status=ready 로 전이
  6. epics 테이블에 (name, branch, status=active) insert
  7. ML 이 epic 스코프 cron 루프를 시작
- **대안 흐름**:
  - 같은 이름 epic 이 active → 에러 종료, OP 에게 `epic-resume` 안내
  - spec 분해 실패 → epic 행도 만들지 않고 에러 종료 (rollback)
  - main 이 dirty → 에러 종료, OP 에게 정리 요청
- **사후 조건**:
  - 리모트에 `epic/<name>` 브랜치 존재
  - DB 에 epic 1개 + task N개 (≥1 ready) 존재
- **관찰 가능 결과**:
  - `git ls-remote origin "refs/heads/epic/auth-token-refresh"` 결과 1줄
  - `events` 테이블에 kind='epic_started' 1행

## UC-2. Task 자동 구현 사이클

> 활성 epic 에서 ready task 가 PR 머지까지 진행된다.

- **트리거**: ML 의 `build-tasks` sub-loop tick
- **선행 조건**:
  - 활성 epic ≥1
  - 해당 epic 에 status=ready task ≥1
  - 동시 구현 한도 (`max_parallel_agents`) 미만
- **주 흐름**:
  1. ML 이 `claim_next_task` 로 ready 한 task 1건을 원자적으로 wip 전환
  2. ML 이 worktree 를 만들어 IM 을 띄움. task 사양 + 작업 브랜치명 (`epic/<name>/<task_id>`) 을 전달
  3. IM 이 코드 작성 후 작업 브랜치 push
  4. ML 이 `branch-promoter` 로 PR 생성 (target=epic 브랜치, `:auto` 라벨)
  5. ML 이 `merge-prs` sub-loop 에서 CI 통과한 `:auto` PR 머지
  6. 머지 성공 → ML 이 task.status=done, pr_number 기록
  7. ML 이 이 task 에 의존하던 blocked/pending 들의 deps 를 재평가, ready 로 승격
- **대안 흐름**:
  - claim 시 다른 sub-loop 가 먼저 가져감 → 다음 tick 까지 skip
  - IM 이 push 실패 → ML 이 task.status=ready 로 되돌리고 attempts 증가 (UC-8 진입 가능)
  - PR CI 실패 → 기존 `ci-watch` 흐름 적용, 일정 시도 후 escalation (UC-8)
- **사후 조건**: 해당 task.status=done, epic 브랜치에 코드 반영
- **관찰 가능 결과**:
  - `events`: claimed → started → completed
  - GitHub PR closed-merged 상태

## UC-3. 의존성 있는 Task

> Task B 가 Task A 에 의존할 때, A 완료 전에는 B 가 claim 되지 않는다.

- **트리거**: spec 분해 결과 deps 가 있음
- **선행 조건**: A.status=ready, B.status=pending, B → A 의존성 등록
- **주 흐름**:
  1. ML 이 ready 후보 조회 시 B 는 deps 미충족으로 제외 — A 만 claim 가능
  2. A 가 done 으로 전이됨
  3. ML 의 `unblock_dependents` 가 B 의 deps 를 재평가 → 모두 done 이면 B.status=ready
  4. 다음 tick 에서 B claim 가능
- **대안 흐름**:
  - A 가 escalated → B 는 blocked 로 전이 (사람 개입 대기)
  - A → B → A 사이클 발견 → epic-start 시 검증 단계에서 거부 (UC-1 의 선행 조건 추가)
- **사후 조건**: 모든 task 가 deps 순서대로만 wip 진입
- **관찰 가능 결과**: `events` 의 시간순으로 A.completed 가 B.claimed 보다 먼저

## UC-4. Epic 이어받기 (다른 사람 / 다른 머신)

> CO 가 OP 의 휴가 중에 같은 epic 을 이어서 진행한다.

- **트리거**: CO 가 `autopilot epic resume auth-token-refresh` 실행
- **선행 조건**:
  - 리모트에 `epic/auth-token-refresh` 브랜치 존재
  - CO 의 로컬 DB 에는 해당 epic 행이 없음 (또는 stale)
- **주 흐름**:
  1. ML 이 origin 에서 epic 브랜치 fetch
  2. spec 을 epic 브랜치 시점 기준으로 다시 분해 → 동일한 결정적 task_id 도출
  3. 리모트의 `epic/<name>/*` 브랜치 + 머지된 PR 을 스캔 (Reconciliation, 03 spec)
  4. 매칭 결과로 DB 를 idempotent 하게 재구성:
     - PR merged → done
     - feature 브랜치 + open PR → wip
     - feature 브랜치만 있고 PR 없음 → wip (stale 후보)
     - 브랜치 없음 → ready (deps OK 시) / pending
  5. epics 행을 active 로 upsert, ML 루프 시작
- **대안 흐름**:
  - 분해 결과에 없는 task_id 의 브랜치/PR 이 발견됨 → orphan 으로 표시, OP/CO 에게 알림
  - epic 브랜치 자체가 없음 → 에러 종료, `epic-start` 안내
- **사후 조건**: CO 의 로컬 DB 가 OP 의 직전 상태와 의미적으로 동일 (캐시 무손실 복구)
- **관찰 가능 결과**: `epic-resume` 직후 `epic-status` 출력이 직전 OP 의 출력과 일치

## UC-5. 로컬 DB 손실 후 복구

> OP 의 머신에서 `.autopilot/state.db` 가 삭제됐다.

- **트리거**: DB 파일 부재 상태에서 다음 ML tick 또는 OP 가 `epic resume` 실행
- **선행 조건**: 리모트의 epic 브랜치/PR 은 그대로 존재
- **주 흐름**: UC-4 와 동일 (`resume` 이 단일 진입점)
- **사후 조건**: DB 가 재구성되어 ML 루프가 재개됨. 진행 중이던 task 의 attempts 카운터는 0 으로 초기화 (캐시 손실의 허용 손실)
- **관찰 가능 결과**: `events` 에 kind='reconciled' 1행, 직전 attempts 정보는 손실됨

## UC-6. Watch 가 발견한 갭 (활성 epic 매칭 가능)

> `gap-watch` 가 spec ↔ 코드 갭을 발견했고, 그 spec 은 활성 epic 의 spec 과 일치한다.

- **트리거**: `gap-watch` sub-loop 가 갭 1건 발견
- **선행 조건**: 발견된 갭의 spec 경로가 활성 epic 의 `spec_path` 와 일치
- **주 흐름**:
  1. ML 이 갭의 fingerprint 로 기존 task 중복 검사
  2. 중복 없으면 새 task 행 (epic_name=매칭 epic, source='gap-watch', status=pending) insert
  3. deps 가 없으면 즉시 ready 로 승격
- **대안 흐름**:
  - 동일 fingerprint task 가 이미 존재 → skip (멱등성)
  - 갭의 spec 이 어떤 활성 epic 에도 매칭되지 않음 → UC-7 진입
- **사후 조건**: 매칭 epic 산하 task 1건 추가
- **관찰 가능 결과**: `tasks` 행 추가, GitHub 이슈는 발행되지 않음

## UC-7. Watch 가 발견한 갭 (활성 epic 없음)

> 발견된 갭이 어떤 활성 epic 에도 속하지 않는다.

- **트리거**: `gap-watch` / `qa-boost` / `ci-watch` 가 갭 1건 발견
- **선행 조건**: 갭의 spec 경로가 활성 epic 의 `spec_path` 와 매칭 안 됨
- **주 흐름**:
  1. ML 이 escalation 이슈 발행 (UC-9 의 후처리 대상)
  2. 이슈 본문에 발견 컨텍스트 + 가능한 epic 후보 (있다면) 명시
  3. tasks 테이블에는 행을 만들지 않음 — 에픽 미할당 task 는 존재할 수 없다
- **대안 흐름**: 동일 fingerprint 의 escalation 이슈가 24h 이내 이미 발행됨 → 신규 발행 skip
- **사후 조건**: GitHub 이슈 1건 신규 (label=`autopilot:hitl-needed`)
- **관찰 가능 결과**: `events` 에 kind='escalated', target='unmatched_watch', issue_number 기록

## UC-8. Task 반복 실패 → Escalate

> 한 task 가 max_attempts 까지 실패했다.

- **트리거**: ML 이 IM 의 push 실패 / PR CI 실패를 N회 누적 관찰
- **선행 조건**: task.attempts == max_attempts (직전 실패 처리 시점)
- **주 흐름**:
  1. ML 이 `mark_task_failed` 호출 → `TaskFailureOutcome::Escalated` 응답 받음
  2. ML 이 escalation 이슈 발행 (task_id, 시도 로그 요약 포함)
  3. task.status=escalated, escalated_issue=이슈번호 기록
  4. 이 task 에 의존하던 다른 task 들은 blocked 로 전이
- **대안 흐름**:
  - attempts < max → status=ready 로 되돌리고 다음 cycle 재시도
  - 동일 task 가 이미 escalated → 추가 이슈 발행 안 함
- **사후 조건**: task.status=escalated, 의존 task 들 blocked
- **관찰 가능 결과**: `events`: failed → escalated, GitHub 이슈 1건

## UC-9. 사람이 Escalation 이슈 처리

> OP/CO 가 escalation 이슈를 받고 결정을 내린다.

- **트리거**: 사람이 GitHub 에서 escalation 이슈에 결정 코멘트 또는 라벨 변경
- **선행 조건**: 이슈 label=`autopilot:hitl-needed` + 본문에 epic/task 메타 포함
- **주 흐름** (이슈 처리 패턴별):
  - **(a) 새 epic 으로 승격**: 사람이 이슈에 `/autopilot epic-start <spec>` 코멘트 또는 운영자가 CLI 실행
  - **(b) 기존 epic 에 흡수**: `analyze-issue` 가 이슈 본문에서 spec 매칭 후 task 행 신규 insert (UC-6 와 동일 흐름)
  - **(c) 거부 / 무시**: 사람이 이슈 close → ML 은 더 이상 해당 fingerprint 를 escalate 하지 않도록 suppression 기록
  - **(d) max-attempts 후 사람이 직접 수정**: 사람이 코드를 직접 작성하고 task 의 escalated_issue 이슈를 close → ML 은 reconcile 시 해당 task 가 done 으로 전이된 것으로 인식
- **사후 조건**: 이슈 closed, 결과에 따라 새 task / 기존 task 상태 갱신
- **관찰 가능 결과**: `events` 에 kind='escalation_resolved', resolution=(a|b|c|d)

## UC-10. Epic 완료 → main 머지 (반자동)

> 한 epic 의 모든 task 가 done 이 되었다.

- **트리거**: 마지막 task 가 done 으로 전이된 직후 ML 의 cycle
- **선행 조건**: epic 산하 task 의 status 가 모두 (done | escalated 중 사람이 close 한 것)
- **주 흐름**:
  1. ML 이 epic.status=completed, completed_at 기록
  2. ML 의 epic 스코프 cron 루프 종료
  3. 사용자 알림 발송 (notification 설정 사용)
  4. **OP 가 직접** epic → main PR 생성 — 자동화하지 않음
- **대안 흐름**:
  - 일부 task 가 escalated 인 채로 남아 있으나 사람이 모두 close → done 으로 reconcile 후 완료 처리
- **사후 조건**: epic.status=completed, ML 의 해당 epic 루프 부재
- **관찰 가능 결과**: `events`: epic_completed, 알림 송신 로그

## UC-11. 두 머신에서 같은 Epic 동시 진행 (충돌 자연 차단)

> OP 와 CO 가 모르는 사이에 같은 epic 을 동시에 resume 하여 같은 task 를 구현하려 한다.

- **트리거**: 두 머신에서 거의 동시에 동일 task_id 를 claim
- **선행 조건**: 둘 다 reconcile 후 동일 task 가 ready 로 보임
- **주 흐름**:
  1. 머신 A 의 IM 이 `epic/<name>/<task_id>` 브랜치 push 성공
  2. 머신 B 의 IM 이 같은 이름 브랜치 push 시도 → non-fast-forward reject
  3. 머신 B 의 ML 이 push 실패를 감지 → 해당 task.status=ready 로 되돌리고 다음 reconcile 에서 wip (다른 머신이 가져감) 으로 재인식
  4. 동일 task 의 PR 은 머신 A 의 것 1개만 존재
- **대안 흐름**:
  - 두 IM 의 코드가 다른 경우에도 git 차원에서 한 쪽만 살아남음. 머신 B 는 작업 결과 폐기
- **사후 조건**: task 1건 = PR 1건. 중복 PR 없음
- **관찰 가능 결과**: 머신 B 의 `events` 에 kind='claim_lost', upstream_existed=true

## UC-12. 운영자가 Epic 강제 중단

> OP 가 진행 중 epic 을 더 이상 진행하지 않기로 결정한다.

- **트리거**: OP 가 `autopilot epic stop <name>` 실행
- **선행 조건**: epic.status=active
- **주 흐름**:
  1. ML 이 epic.status=abandoned 로 전이
  2. 진행 중이던 task 들 중 wip 상태는 그대로 둠 (현재 push 된 코드는 보존)
  3. epic 스코프 ML 루프 종료
  4. 새 task claim 중단
- **대안 흐름**:
  - `--purge-branches` 플래그가 주어지면 머지되지 않은 feature 브랜치 정리 옵션 (기본 OFF)
- **사후 조건**: 해당 epic 의 새 작업이 진행되지 않음. 리모트의 코드는 보존
- **관찰 가능 결과**: `events`: epic_abandoned

## UC-13. 라벨 기반 → DB 기반 마이그레이션

> 기존 `:ready` 라벨 기반으로 운영하던 레포에서 epic 기반으로 전환한다.

- **트리거**: OP 가 `epic_based: true` 설정 후 `autopilot migrate import-issue <#>` 실행
- **선행 조건**: 활성 epic ≥1, 대상 이슈가 `:ready` 상태
- **주 흐름**:
  1. ML 이 이슈 본문/제목에서 spec 매칭 시도
  2. 매칭된 epic 의 task 로 신규 insert (source='human', body에 원래 이슈 번호 메타로 기록)
  3. 이슈에는 자동으로 코멘트 추가: "task <id> 로 흡수됨, label `:ready` 제거"
  4. 이슈에서 `:ready` 라벨 제거 (autopilot 이 더 이상 큐로 보지 않게)
- **대안 흐름**:
  - 매칭 실패 → UC-7 의 escalation 형태로 유지 또는 사람이 수동 mapping
- **사후 조건**: 이슈는 살아 있되 라벨이 정리됨, 새 task 행 1건
- **관찰 가능 결과**: `events`: kind='migrated_from_issue', issue_number 기록

## 시나리오 ↔ 인터페이스 매핑 (요약)

| UC | 도출되는 핵심 인터페이스 / 동작 |
|----|------------------------------|
| 1 | `EpicManager::start`, `TaskStore::insert_epic_with_tasks` (트랜잭션) |
| 2 | `TaskStore::claim_next_task` (원자성), `TaskStore::mark_task_done` |
| 3 | `TaskStore::list_ready_tasks` 의 deps 필터, `unblock_dependents` |
| 4-5 | `Reconciler::reconcile_epic`, 결정적 `task_id` 함수 |
| 6 | `TaskStore::find_by_fingerprint`, watch 측의 epic 매칭 함수 |
| 7-9 | `Escalator::escalate`, suppression 기록, `analyze-issue` 의 흡수 경로 |
| 8 | `mark_task_failed` → `TaskFailureOutcome` 분기 |
| 10 | epic 완료 판정 + 알림. main 머지는 자동화 대상 아님 |
| 11 | claim 의 원자성 + git push reject 자연 처리 |
| 12 | `EpicManager::stop`, abandoned 상태 |
| 13 | 마이그레이션 CLI + 이슈 → task 흡수 |

이 매핑은 02 (아키텍처) 의 모듈 경계와 03 (상세 스펙) 의 trait 시그니처가 충족해야 하는 최소 요구사항이다.

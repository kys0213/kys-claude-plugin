---
description: "스펙 기반 구현 갭을 탐지하여 autopilot ledger task로 등록합니다 (GitHub issue 생성 없음)"
argument-hint: ""
allowed-tools: ["Bash", "Glob", "Read", "Agent", "AskUserQuestion"]
---

# Gap Watch

스펙 문서와 구현 코드 사이의 갭을 분석하고, 발견된 갭을 **autopilot ledger task**로 등록합니다.

> **책임 경계**: gap-watch는 autopilot 내부 to-do 작성자입니다. 결과는 SQLite ledger의 `gap-backlog` epic에만 기록되며 GitHub issue는 생성하지 않습니다 (CLAUDE.md "책임 경계" — 팀원 visible UI 노이즈 최소화). 다운스트림인 `/github-autopilot:work-ledger`가 ledger task를 claim하여 implementer → PR 흐름으로 진행합니다.

## 사용법

```bash
/github-autopilot:gap-watch
```

> 반복 실행은 `/github-autopilot:autopilot`이 `CronCreate`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: Base 브랜치 동기화

**branch-sync** 스킬의 절차를 수행합니다.

### Step 1.5: Pipeline Idle Check

먼저 gap-watch의 분석 이력이 존재하는지 확인합니다:

```bash
REPO=$(basename "$(git rev-parse --show-toplevel)")
[ -f "/tmp/autopilot-${REPO}/state/gap-watch.state" ]
```

- **파일 없음 (exit 1)**: 분석 이력이 없습니다. idle check를 건너뛰고 **Step 2로 바로 진행**합니다.
- **파일 있음 (exit 0)**: 분석 이력이 존재합니다. 아래 pipeline idle check를 수행합니다.

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — gap-watch cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 2부터 정상 진행.

### Step 1.7: Idle Count Check + Adaptive Throttling

> build-issues Step 3.5에도 동일한 throttling 패턴이 적용됩니다. 로직 변경 시 양쪽을 함께 수정하세요.

이전 Step의 결과가 "대상 없음"(idle)이면, CLI로 idle 횟수를 기록하고 출력에서 `idle_count`를 읽습니다.

```bash
autopilot check mark gap-watch --status idle
# 출력: marked gap-watch: abc1234 at 2026-04-13T10:00:00Z (idle_count: 4)
```

설정에서 `idle_shutdown.max_idle` 값을 읽습니다 (기본값: 5).

출력의 `idle_count`에 따라 동적으로 간격을 조정합니다:

| idle_count | 동작 |
|------------|------|
| 1~3 | 현재 간격 유지 |
| 4~`max_idle`-1 | CronList로 현재 cron을 찾아 CronDelete 후, 간격 2배로 CronCreate 재등록 |
| `max_idle` 이상 | CronList로 현재 cron을 찾아 CronDelete. "연속 {N}회 idle — cron 자동 해제" 출력 후 종료 |

> 간격 확대는 한 번만 적용됩니다 (4회째에 2배로 변경 후, 5~max_idle-1까지 유지).

실제 작업을 수행하면 idle count를 리셋하고, 간격이 변경되었으면 원래 간격으로 CronDelete → CronCreate합니다:
```bash
autopilot check mark gap-watch --status active
# idle_count가 0으로 리셋됨 → 간격 복원 필요 시 CronDelete + CronCreate
```

### Step 2: 설정 로딩

`github-autopilot.local.md`에서 설정을 읽습니다.
- `spec_paths`: 스펙 파일 탐색 경로 (기본값: `["spec/", "docs/spec/"]`)
- `label_prefix`: 라벨 접두사 (역방향 분석에서 reverse-gap-ignore 파일 경로 명명 등에만 사용. ledger task에는 라벨이 부여되지 않습니다.)

### Step 3: 스펙 파일 수집

Glob으로 spec_paths에서 마크다운 파일을 수집합니다:
- `spec/**/*.md`
- `docs/spec/**/*.md`

> **필터링 규칙**: spec_paths에 명시된 디렉토리에서만 스펙 파일을 수집합니다.
> 다음 패턴에 해당하는 경로는 자동 제외합니다:
> - 테스트 디렉토리: `tests/`, `test_fixtures/`, `benches/`
> - 테스트 파일: `*_test.*`, `*_spec.{rs,ts,js,go,py}` (테스트 코드 자체, `.md`는 제외)
> - 인라인 fixture: gap-detector가 Phase 1에서 추가 검증합니다.
> - **실존 검증**: Glob 결과의 각 파일 경로가 실제로 존재하는지 `[ -f ]`로 확인합니다.
> - **ID 형식 필터**: spec ID가 테스트 픽스처 패턴(`spec-*-test`, `spec-no-*`, `spec-term` 등 하이픈으로 연결된 짧은 ID)인 경우 경고를 로그에 남기고 사용자에게 확인을 요청합니다.

스펙 파일이 없으면 에러 메시지 출력 후 종료.

### Step 4: 갭 분석 (Agent)

gap-detector 에이전트를 호출합니다 (background=false):

전달 정보:
- spec_files: Step 3에서 수집한 스펙 파일 경로 목록
- code_path: 프로젝트 루트
- (선택) reverse: `true`이면 Phase 4 역방향 분석을 활성화하여 코드에 있지만 스펙에 없는 entry point를 추가로 보고합니다.

에이전트가 스펙 파싱 → 구조 매핑 → call chain 갭 분석을 통합 수행합니다.

### Step 5: Ledger Task 등록 (Agent)

#### Step 5a: Ledger Epic 부트스트랩 (필수)

gap-ledger-writer 호출 직전에, 결정적 ledger의 `gap-backlog` epic이 존재하도록 한 번만 보장합니다 (idempotent).

`--idempotent` 플래그는 동일한 spec_path로 epic이 이미 존재하면 exit 0으로 정상 종료합니다. spec_path가 다르면 의미적 충돌이므로 exit 1로 보고됩니다.

```bash
EPIC_NAME="gap-backlog"
EPIC_SPEC="spec/gap-backlog.md"
if ! autopilot epic create --name "$EPIC_NAME" --spec "$EPIC_SPEC" --idempotent; then
  echo "ERROR: gap-backlog epic 부트스트랩 실패 — ledger 쓰기 불가, cycle 중단"
  exit 1
fi
```

> **중요**: ledger-only writer로 전환되면서 epic 부트스트랩 실패는 더 이상 best-effort observer가 아닌 **blocker**입니다. epic 없이는 task를 등록할 수 없으므로 cycle을 중단합니다.

#### Step 5b: Agent 호출 (정방향 갭)

gap-ledger-writer 에이전트를 호출합니다 (background=false):

전달 정보:
- **gap_report**: Step 4의 마크다운 리포트 (정방향 ❌ Missing / ⚠️ Partial 항목)
- **ledger_epic**: `$EPIC_NAME`
- **reverse_mode**: `false` (정방향)

에이전트가 ❌ Missing, ⚠️ Partial 항목을 ledger task로 변환합니다. 동일 fingerprint의 기존 task는 `skipped_duplicates`로 자동 흡수됩니다 (idempotent).

`created` 카운트를 누적 변수에 저장하고 Step 5.5로 진행합니다. `check mark` 호출은 정방향 + 역방향 결과를 모두 합산한 후 Step 6 직전에 1회만 수행합니다 (idle/active 판정의 일관성 확보).

### Step 5.5: 역방향 갭 분석 + HITL (Reverse Gap)

gap-detector의 Phase 4 결과에서 ❌ Unspecified 항목을 처리합니다.

> 이 단계는 Step 4의 gap-detector 호출 시 `reverse: true`를 전달한 경우에만 실행됩니다.

1. **Unspecified 항목이 없으면**: Step 6으로 진행

2. **Unspecified 항목이 있으면**: AskUserQuestion으로 사용자에게 제시

```
⚠️ 스펙에 정의되지 않은 코드 기능이 발견되었습니다:

1. `src/auth/oauth.rs:handle_callback` — OAuth callback 처리
2. `src/api/internal.rs:health_check` — 내부 헬스체크

각 항목의 처리 방법을 선택하세요 (번호:선택 형식, 예: 1:a 2:c):
(a) ledger task 생성 — 스펙 보강 필요 (rev-gap fingerprint, gap-backlog epic)
(b) internal 마킹 — 의도적 확장, 향후 분석에서 제외
(c) skip — 이번 cycle에서만 건너뜀
```

3. **선택 결과 처리**:
   - **(a) ledger task**: gap-ledger-writer를 `reverse_mode=true`로 다시 호출하여 ❌ Unspecified 항목만 `gap-backlog` epic에 등록.
     - fingerprint 형식: `rev-gap:{file_path}:{entry_point}`
     - body에 "스펙 보강 필요" 컨텍스트가 포함됩니다.
   - **(b) internal 마킹**: `.claude/.autopilot/reverse-gap-ignore.json`에 해당 entry point를 기록
     - 다음 cycle에서 Phase 4 결과에서 자동 제외
   - **(c) skip**: 이번 cycle에서만 무시 (다음 cycle에 다시 표시)

4. **internal 마킹 파일 형식**:

```json
{
  "internal": [
    "src/auth/oauth.rs:handle_callback",
    "src/api/internal.rs:health_check"
  ]
}
```

### Step 6: idle/active 마킹 및 결과 보고

정방향(Step 5b) + 역방향(Step 5.5 (a) 선택) 결과의 `created` 카운트를 합산합니다.

- 합계가 0이면 `autopilot check mark gap-watch --status idle`
- 합계가 1 이상이면 `autopilot check mark gap-watch --status active`

이후 갭 분석 요약과 등록된 ledger task 목록을 사용자에게 출력합니다:
- 전체 요구사항 수, Implemented/Partial/Missing 수
- (역방향 분석 시) 전체 entry point 수, Well-specified/Under-specified/Unspecified 수
- 등록된 ledger task id + 제목 (정방향 / 역방향 분리)
- skip된 항목 수 (duplicates / missing spec / warnings)

> 운영자가 결과를 직접 확인하려면: `autopilot epic status gap-backlog --json` 또는 `autopilot task list --epic gap-backlog`. GitHub issue 검색으로는 더 이상 보이지 않습니다.

## 주의사항

- 토큰 최적화: MainAgent는 스펙/코드 파일을 직접 읽지 않음. 파일 경로만 수집하고 gap-detector에 위임
- 스펙 파일 변경이 없어도 코드 변경으로 갭이 해소되었을 수 있으므로 매번 전체 분석
- 동일 fingerprint의 기존 ledger task는 자동 흡수되므로 별도의 중복 검사가 필요 없습니다
- **GitHub issue는 생성하지 않습니다** — `autopilot:ready` 라벨이 부여된 갭 이슈는 더 이상 만들어지지 않습니다. 운영자는 `autopilot epic status gap-backlog` / `autopilot task list --epic gap-backlog`로 결과를 확인합니다.
- 역방향 분석(Step 5.5)은 `reverse: true` 전달 시에만 활성화
- reverse-gap-ignore.json의 internal 항목은 다음 cycle부터 자동 제외
- stagnation/persona 기반 lateral thinking은 GitHub issue body에서 simhash를 추출하는 구조였으므로 ledger-only 전환과 함께 잠정 제거되었습니다 (ledger 기반 stagnation 감지는 추후 follow-up).

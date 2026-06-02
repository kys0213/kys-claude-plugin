# spec↔code 갭 감시 (gap-watch)

스펙 문서와 구현 코드 사이의 갭을 분석해 **autopilot ledger task**(`gap-backlog` epic)로 등록한다. GitHub issue 는 생성하지 않는다. 전처리(base 동기화·idle/throttling)는 `pipeline-control.md`.

> **책임 경계**: gap-watch 는 autopilot 내부 to-do 작성자. 결과는 SQLite ledger 의 `gap-backlog` epic 에만 기록(CLAUDE.md "책임 경계" — 팀원 visible UI 노이즈 최소화). 다운스트림인 work-ledger 가 ledger task 를 claim 해 implementer → PR 흐름으로 진행한다.

## 전처리 특이사항 (idle check 게이트)

`pipeline-control.md` 의 전처리를 따르되, gap-watch 는 idle check 앞에 분석 이력 게이트가 있다:

```bash
REPO=$(basename "$(git rev-parse --show-toplevel)")
[ -f "/tmp/autopilot-${REPO}/state/gap-watch.state" ]
```

- **파일 없음 (exit 1)**: 분석 이력이 없으므로 idle check 를 건너뛰고 **Step 2(설정 로딩)로 바로 진행**.
- **파일 있음 (exit 0)**: `pipeline-control.md` 의 Pipeline Idle Check + Idle Count + Adaptive Throttling 을 수행 (loop 이름 `gap-watch`, capacity 검사 불필요 — `--max-parallel` 생략).

> idle 시 알림 메시지: "autopilot 파이프라인 완료 — gap-watch cycle 중단". 나머지 idle_count/throttling 동작은 `pipeline-control.md` 표와 동일.

### Step 2: 설정 로딩

`github-autopilot.local.md`에서 읽는다:
- `spec_paths`: 스펙 파일 탐색 경로 (기본값 `["spec/", "docs/spec/"]`)
- `label_prefix`: 라벨 접두사 (역방향 분석의 reverse-gap-ignore 파일 경로 명명 등에만 사용. ledger task 에는 라벨이 부여되지 않음.)

### Step 3: 스펙 파일 수집

Glob 으로 spec_paths 에서 마크다운 파일을 수집: `spec/**/*.md`, `docs/spec/**/*.md`.

> **필터링 규칙**: spec_paths 에 명시된 디렉토리에서만 수집. 다음 패턴 경로는 자동 제외:
> - 테스트 디렉토리: `tests/`, `test_fixtures/`, `benches/`
> - 테스트 파일: `*_test.*`, `*_spec.{rs,ts,js,go,py}` (테스트 코드 자체, `.md`는 제외)
> - 인라인 fixture: gap-detector 가 Phase 1 에서 추가 검증.
> - **실존 검증**: Glob 결과의 각 파일 경로가 실제로 존재하는지 `[ -f ]`로 확인.
> - **ID 형식 필터**: spec ID 가 테스트 픽스처 패턴(`spec-*-test`, `spec-no-*`, `spec-term` 등 하이픈으로 연결된 짧은 ID)이면 경고를 로그에 남기고 사용자에게 확인 요청.

스펙 파일이 없으면 에러 메시지 출력 후 종료.

### Step 4: 갭 분석 (Agent)

gap-detector 에이전트**를** 호출한다 (background=false).

전달 정보:
- spec_files: Step 3 에서 수집한 스펙 파일 경로 목록
- code_path: 프로젝트 루트
- (선택) reverse: `true`이면 Phase 4 역방향 분석을 활성화해 코드에 있지만 스펙에 없는 entry point 를 추가 보고

에이전트가 스펙 파싱 → 구조 매핑 → call chain 갭 분석을 통합 수행한다.

### Step 5: Ledger Task 등록 (Agent)

**5a. Ledger Epic 부트스트랩 (필수)** — gap-ledger-writer 호출 직전, `gap-backlog` epic 을 한 번만 보장(idempotent). `--idempotent`는 동일 spec_path 로 이미 존재하면 exit 0, spec_path 가 다르면 의미적 충돌이므로 exit 1.

```bash
EPIC_NAME="gap-backlog"
EPIC_SPEC="spec/gap-backlog.md"
if ! autopilot epic create --name "$EPIC_NAME" --spec "$EPIC_SPEC" --idempotent; then
  echo "ERROR: gap-backlog epic 부트스트랩 실패 — ledger 쓰기 불가, cycle 중단"
  exit 1
fi
```

> **중요**: ledger-only writer 로 전환되면서 epic 부트스트랩 실패는 더 이상 best-effort observer 가 아닌 **blocker** 다. epic 없이는 task 등록 불가이므로 cycle 중단.

**5b. 정방향 갭 등록** — gap-ledger-writer 에이전트**를** 호출한다 (background=false).

전달 정보:
- **gap_report**: Step 4 의 마크다운 리포트 (정방향 ❌ Missing / ⚠️ Partial 항목)
- **ledger_epic**: `$EPIC_NAME`
- **reverse_mode**: `false` (정방향)

에이전트가 ❌ Missing, ⚠️ Partial 항목을 ledger task 로 변환한다. 동일 fingerprint 의 기존 task 는 `skipped_duplicates`로 자동 흡수(idempotent). `created` 카운트를 누적 변수에 저장하고 Step 5.5 로 진행. `check mark` 호출은 정방향 + 역방향 결과를 모두 합산한 후 Step 6 직전에 1회만 수행 (idle/active 판정 일관성 확보).

### Step 5.5: 역방향 갭 분석 + HITL (Reverse Gap)

gap-detector 의 Phase 4 결과에서 ❌ Unspecified 항목을 처리한다.

> 이 단계는 Step 4 의 gap-detector 호출 시 `reverse: true`를 전달한 경우에만 실행.

1. **Unspecified 항목이 없으면**: Step 6 으로 진행
2. **Unspecified 항목이 있으면**: AskUserQuestion 으로 사용자에게 제시

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
   - **(a) ledger task**: gap-ledger-writer 를 `reverse_mode=true`로 다시 호출해 ❌ Unspecified 항목만 `gap-backlog` epic 에 등록.
     - fingerprint 형식: `rev-gap:{file_path}:{entry_point}`
     - body 에 "스펙 보강 필요" 컨텍스트 포함.
   - **(b) internal 마킹**: `.claude/.autopilot/reverse-gap-ignore.json`에 해당 entry point 기록 → 다음 cycle Phase 4 결과에서 자동 제외.
   - **(c) skip**: 이번 cycle 에서만 무시 (다음 cycle 에 다시 표시).

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

정방향(5b) + 역방향(5.5 (a) 선택) 결과의 `created` 카운트를 합산한다.

- 합계 0 → `autopilot check mark gap-watch --status idle`
- 합계 1 이상 → `autopilot check mark gap-watch --status active`

이후 갭 분석 요약과 등록된 ledger task 목록을 출력:
- 전체 요구사항 수, Implemented/Partial/Missing 수
- (역방향 분석 시) 전체 entry point 수, Well-specified/Under-specified/Unspecified 수
- 등록된 ledger task id + 제목 (정방향 / 역방향 분리)
- skip 된 항목 수 (duplicates / missing spec / warnings)

> 운영자가 결과를 직접 확인하려면: `autopilot epic status gap-backlog --json` 또는 `autopilot task list --epic gap-backlog`. GitHub issue 검색으로는 더 이상 보이지 않는다.

**6b. 세션 누적 통계** — 매 cycle 종료 시 세션 통계 업데이트:

- `PROCESSED` = ledger 에 등록된 gap task 수 (정방향 + 역방향 `created` 합)
- `SUCCESS` = 동일 (ledger 쓰기 성공이 곧 success)
- `FAILED` = `0` (gap-watch 는 ledger 쓰기 외 실패 분류 없음)
- `FALSE_POSITIVE` = spec 미존재 등으로 필터링된 WARNING 항목 수

```bash
autopilot stats update --command gap-watch \
  --processed ${PROCESSED} --success ${SUCCESS} --failed ${FAILED} --false-positive ${FALSE_POSITIVE}
autopilot stats show --command gap-watch
```

> `processed=0`이면 `idle_cycles`, `processed>0`이면 `agent_calls` 자동 누적. 통계는 `/tmp/autopilot-{repo}/state/session-stats.json`, 세션 시작 시 `autopilot stats init`으로 초기화.

### 주의사항

- 토큰 최적화: MainAgent 는 스펙/코드 파일을 직접 읽지 않음. 파일 경로만 수집하고 gap-detector 에 위임.
- 스펙 파일 변경이 없어도 코드 변경으로 갭이 해소되었을 수 있으므로 매번 전체 분석.
- 동일 fingerprint 의 기존 ledger task 는 자동 흡수되므로 별도 중복 검사 불필요.
- **GitHub issue 는 생성하지 않는다** — `autopilot:ready` 라벨이 부여된 갭 이슈는 더 이상 생성되지 않음. 운영자는 `autopilot epic status gap-backlog` / `autopilot task list --epic gap-backlog`로 확인.
- 역방향 분석(5.5)은 `reverse: true` 전달 시에만 활성화.
- reverse-gap-ignore.json 의 internal 항목은 다음 cycle 부터 자동 제외.
- stagnation/persona 기반 lateral thinking 은 GitHub issue body 에서 simhash 를 추출하는 구조였으므로 ledger-only 전환과 함께 잠정 제거 (ledger 기반 stagnation 감지는 추후 follow-up).

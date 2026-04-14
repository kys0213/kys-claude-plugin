---
description: "스펙 기반 구현 갭을 탐지하여 GitHub issue를 자동 생성합니다"
argument-hint: ""
allowed-tools: ["Bash", "Glob", "Read", "Agent", "AskUserQuestion"]
---

# Gap Watch

스펙 문서와 구현 코드 사이의 갭을 분석하고, 발견된 갭을 GitHub issue로 등록합니다.

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
- `label_prefix`: 라벨 접두사 (기본값: `"autopilot:"`)

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

에이전트가 스펙 파싱 → 구조 매핑 → call chain 갭 분석을 통합 수행합니다.

### Step 4.5: Stagnation Check

갭 분석 리포트의 simhash를 계산하고 stagnation 여부를 판단합니다.

1. **Simhash 계산**: 갭 분석 리포트(Step 4 결과)에서 ❌ Missing, ⚠️ Partial 항목의 텍스트를 추출하여 simhash를 계산합니다.

```bash
# 리포트에서 핵심 텍스트 추출 후 autopilot CLI로 simhash 생성은
# gap-issue-creator가 내부적으로 수행합니다.
```

2. **이력 기록**: 현재 분석 결과의 simhash를 loop state에 기록합니다.

```bash
autopilot check mark gap-watch --output-hash "{simhash}"
```

3. **유사 이슈 검색**: 각 gap의 fingerprint에 대해 유사 이슈를 조회합니다.

```bash
autopilot issue search-similar \
  --fingerprint "gap:{spec_path}:{requirement_keyword}" \
  --simhash "{simhash}" \
  --limit 5
```

4. **Stagnation 판정**: 유사 이슈 결과에서 distance ≤ 5인 closed 이슈가 2개 이상이면 stagnation으로 판정합니다.
   - **Stagnation 감지**: Step 5에서 gap-issue-creator에 유사 이슈 목록과 함께 **resilience** 스킬의 persona 가이드를 전달합니다.
   - **Stagnation 미감지**: 기존 흐름대로 Step 5를 진행합니다.

### Step 5: Issue 생성 (Agent)

gap-issue-creator 에이전트를 호출합니다 (background=false):

전달 정보:
- 갭 분석 리포트 (Step 4 결과)
- label_prefix
- **(stagnation 시 추가)** 유사 이슈 목록 (번호, distance, 상태) + resilience persona 가이드

에이전트가 ❌ Missing, ⚠️ Partial 항목을 GitHub issue로 변환합니다.
중복 이슈는 자동 필터링됩니다.
stagnation이 감지된 gap은 과거 이슈를 참조하고 새 persona 관점으로 이슈를 생성합니다.

생성된 이슈가 0건이면 `autopilot check mark gap-watch --status idle` 후 Step 6으로 진행합니다.
생성된 이슈가 1건 이상이면 `autopilot check mark gap-watch --status active` 후 Step 5.5로 진행합니다.

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
(a) spec-needed 이슈 생성 — 스펙 보강 필요
(b) internal 마킹 — 의도적 확장, 향후 분석에서 제외
(c) skip — 이번 cycle에서만 건너뜀
```

3. **선택 결과 처리**:
   - **(a) spec-needed 이슈**: gap-issue-creator에 위임하여 `{label_prefix}spec-needed` 라벨로 이슈 생성
     - fingerprint 형식: `rev-gap:{file_path}:{entry_point}`
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

### Step 6: 결과 보고

갭 분석 요약과 생성된 이슈 목록을 사용자에게 출력합니다:
- 전체 요구사항 수, Implemented/Partial/Missing 수
- (역방향 분석 시) 전체 entry point 수, Well-specified/Under-specified/Unspecified 수
- 생성된 이슈 번호 + 제목

## 주의사항

- 토큰 최적화: MainAgent는 스펙/코드 파일을 직접 읽지 않음. 파일 경로만 수집하고 gap-detector에 위임
- 스펙 파일 변경이 없어도 코드 변경으로 갭이 해소되었을 수 있으므로 매번 전체 분석
- 기존 이슈와 중복되지 않도록 gap-issue-creator가 자동 필터링
- stagnation 감지 시 resilience 스킬의 persona를 활용하여 다른 관점의 이슈를 생성
- 역방향 분석(Step 5.5)은 `reverse: true` 전달 시에만 활성화
- reverse-gap-ignore.json의 internal 항목은 다음 cycle부터 자동 제외

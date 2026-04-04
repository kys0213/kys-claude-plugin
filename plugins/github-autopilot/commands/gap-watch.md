---
description: "스펙 기반 구현 갭을 탐지하여 GitHub issue를 자동 생성합니다"
argument-hint: "[interval: 30m, 1h, ...]"
allowed-tools: ["Bash", "Glob", "Read", "Agent", "CronCreate", "CronDelete", "CronList"]
---

# Gap Watch

스펙 문서와 구현 코드 사이의 갭을 분석하고, 발견된 갭을 GitHub issue로 등록합니다.

## 사용법

```bash
/github-autopilot:gap-watch          # 1회 실행
/github-autopilot:gap-watch 30m      # 30분마다 반복
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 인자 파싱

`$ARGUMENTS`에서 interval을 추출합니다.
- `/^\d+[smh]$/` 패턴 매칭 → interval 모드
- 비어있으면 → 1회 실행 모드

### Step 2: 최신 상태 동기화

```bash
git fetch origin
git pull --rebase origin $(git branch --show-current) 2>/dev/null || true
```

### Step 2.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: 기존 cron을 정리한 뒤 종료합니다.
  1. CronList로 현재 등록된 cron 목록을 조회
  2. `gap-watch`가 포함된 cron job을 찾아 CronDelete로 삭제
  3. `notification` 설정이 있으면 "autopilot 파이프라인 완료 — gap-watch cycle 중단" 알림 발송
  4. CronCreate를 등록하지 않고 종료
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다 (CronCreate는 등록하여 다음 cycle에서 재시도).
- **exit 1 (active)**: Step 3부터 정상 진행.

### Step 3: 설정 로딩

`github-autopilot.local.md`에서 설정을 읽습니다.
- `spec_paths`: 스펙 파일 탐색 경로 (기본값: `["spec/", "docs/spec/"]`)
- `label_prefix`: 라벨 접두사 (기본값: `"autopilot:"`)

### Step 4: 스펙 파일 수집

Glob으로 spec_paths에서 마크다운 파일을 수집합니다:
- `spec/**/*.md`
- `docs/spec/**/*.md`

스펙 파일이 없으면 에러 메시지 출력 후 종료.

### Step 5: 갭 분석 (Agent)

gap-detector 에이전트를 호출합니다 (background=false):

전달 정보:
- spec_files: Step 4에서 수집한 스펙 파일 경로 목록
- code_path: 프로젝트 루트

에이전트가 스펙 파싱 → 구조 매핑 → call chain 갭 분석을 통합 수행합니다.

### Step 6: Issue 생성 (Agent)

gap-issue-creator 에이전트를 호출합니다 (background=false):

전달 정보:
- 갭 분석 리포트 (Step 5 결과)
- label_prefix

에이전트가 ❌ Missing, ⚠️ Partial 항목을 GitHub issue로 변환합니다.
중복 이슈는 자동 필터링됩니다.

### Step 7: CronCreate (interval 모드)

interval이 지정된 경우에만 실행합니다:

CronCreate를 호출하여 `/github-autopilot:gap-watch`를 지정된 interval로 등록합니다.
등록 시 interval 인자는 포함하지 않습니다 (재귀 등록 방지).

### Step 8: 결과 보고

갭 분석 요약과 생성된 이슈 목록을 사용자에게 출력합니다:
- 전체 요구사항 수, Implemented/Partial/Missing 수
- 생성된 이슈 번호 + 제목

## 주의사항

- 토큰 최적화: MainAgent는 스펙/코드 파일을 직접 읽지 않음. 파일 경로만 수집하고 gap-detector에 위임
- 스펙 파일 변경이 없어도 코드 변경으로 갭이 해소되었을 수 있으므로 매번 전체 분석
- 기존 이슈와 중복되지 않도록 gap-issue-creator가 자동 필터링

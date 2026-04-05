---
description: "스펙 기반 구현 갭을 탐지하여 GitHub issue를 자동 생성합니다"
argument-hint: ""
allowed-tools: ["Bash", "Glob", "Read", "Agent"]
---

# Gap Watch

스펙 문서와 구현 코드 사이의 갭을 분석하고, 발견된 갭을 GitHub issue로 등록합니다.

## 사용법

```bash
/github-autopilot:gap-watch
```

> 반복 실행은 `/github-autopilot:autopilot`이 `run-loop.sh`로 관리합니다.

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 최신 상태 동기화

```bash
git fetch origin
git pull --rebase origin $(git branch --show-current) 2>/dev/null || true
```

### Step 1.5: Pipeline Idle Check

```bash
autopilot pipeline idle --label-prefix "{label_prefix}"
```

- **exit 0 (idle)**: `notification` 설정이 있으면 "autopilot 파이프라인 완료 — gap-watch cycle 중단" 알림 발송 후 종료합니다.
- **exit 2 (error)**: 스크립트 실행 환경 오류. 에러 메시지를 출력하고 이번 cycle을 skip합니다.
- **exit 1 (active)**: Step 2부터 정상 진행.

### Step 2: 설정 로딩

`github-autopilot.local.md`에서 설정을 읽습니다.
- `spec_paths`: 스펙 파일 탐색 경로 (기본값: `["spec/", "docs/spec/"]`)
- `label_prefix`: 라벨 접두사 (기본값: `"autopilot:"`)

### Step 3: 스펙 파일 수집

Glob으로 spec_paths에서 마크다운 파일을 수집합니다:
- `spec/**/*.md`
- `docs/spec/**/*.md`

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

### Step 6: 결과 보고

갭 분석 요약과 생성된 이슈 목록을 사용자에게 출력합니다:
- 전체 요구사항 수, Implemented/Partial/Missing 수
- 생성된 이슈 번호 + 제목

## 주의사항

- 토큰 최적화: MainAgent는 스펙/코드 파일을 직접 읽지 않음. 파일 경로만 수집하고 gap-detector에 위임
- 스펙 파일 변경이 없어도 코드 변경으로 갭이 해소되었을 수 있으므로 매번 전체 분석
- 기존 이슈와 중복되지 않도록 gap-issue-creator가 자동 필터링
- stagnation 감지 시 resilience 스킬의 persona를 활용하여 다른 관점의 이슈를 생성

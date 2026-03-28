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

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`
- 현재 브랜치: !`git branch --show-current`

## 작업 프로세스

### Step 1: 최신 상태 동기화

```bash
git fetch origin
git pull --rebase origin $(git branch --show-current) 2>/dev/null || true
```

### Step 2: 설정 로딩

`github-autopilot.local.md`에서 설정을 읽습니다.
- `spec_paths`: 스펙 파일 탐색 경로 (기본값: `["spec/", "docs/spec/"]`)
- `label_prefix`: 라벨 접두사 (기본값: `"autopilot:"`)

### Step 3: 변경 감지 (diff check)

`autopilot-check.sh diff`로 마지막 분석 이후 변경사항을 확인합니다:

```bash
SCRIPT_DIR="${CLAUDE_PLUGIN_ROOT}/scripts"
diff_result=$("${SCRIPT_DIR}/autopilot-check.sh" diff gap-watch ${spec_paths}) || diff_exit=$?
```

exit code에 따라 분기:
- **exit 0** (변경 없음): "변경 없음, skip" 출력 후 종료
- **exit 1** (스펙 변경): Step 4로 진행하여 전체 분석 수행
- **exit 2** (코드만 변경): Step 4로 진행하되, gap-detector에 `diff_result`의 `code_files`만 전달하여 경량 재검증
- **exit 3** (첫 실행): Step 4로 진행하여 전체 분석 수행

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
- changed_files: Step 3에서 exit 2인 경우, 변경된 코드 파일 목록 (경량 재검증 모드)

에이전트가 스펙 파싱 → 구조 매핑 → call chain 갭 분석을 통합 수행합니다.
경량 재검증 모드에서는 changed_files 범위만 분석합니다.

### Step 6: Issue 생성 (Agent)

gap-issue-creator 에이전트를 호출합니다 (background=false):

전달 정보:
- 갭 분석 리포트 (Step 5 결과)
- label_prefix

에이전트가 ❌ Missing, ⚠️ Partial 항목을 GitHub issue로 변환합니다.
중복 이슈는 자동 필터링됩니다.

### Step 7: 상태 기록 (mark)

분석 완료 후 현재 HEAD를 기록합니다:

```bash
"${SCRIPT_DIR}/autopilot-check.sh" mark gap-watch
```

### Step 8: 결과 보고

갭 분석 요약과 생성된 이슈 목록을 사용자에게 출력합니다:
- 전체 요구사항 수, Implemented/Partial/Missing 수
- 생성된 이슈 번호 + 제목

## 주의사항

- 토큰 최적화: MainAgent는 스펙/코드 파일을 직접 읽지 않음. 파일 경로만 수집하고 gap-detector에 위임
- diff check로 변경이 없으면 LLM 호출 없이 즉시 종료하여 불필요한 비용 절감
- 코드만 변경된 경우 경량 재검증으로 분석 범위를 축소
- 기존 이슈와 중복되지 않도록 gap-issue-creator가 자동 필터링

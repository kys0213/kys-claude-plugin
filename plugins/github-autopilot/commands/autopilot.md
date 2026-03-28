---
description: "autopilot 루프를 설정된 인터벌로 모두 시작합니다 (기본 6개 + test_watch + custom_loops)"
argument-hint: ""
allowed-tools: ["Read", "Bash", "Write", "Glob", "AskUserQuestion"]
---

# Autopilot

autopilot 루프를 설정된 인터벌로 모두 시작합니다. 각 루프는 `run-loop.sh`를 통해 백그라운드 프로세스로 실행됩니다.

## 사용법

```bash
/github-autopilot:autopilot
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -30 || echo "설정 파일 없음"`
- Setup 절차: !`cat ${CLAUDE_PLUGIN_ROOT}/skills/setup-init/SKILL.md`
- Preflight 절차: !`cat ${CLAUDE_PLUGIN_ROOT}/skills/preflight-check/SKILL.md`

## 작업 프로세스

### Step 0: 초기 설정 확인

`github-autopilot.local.md` 파일이 존재하는지 확인합니다.

```bash
test -f github-autopilot.local.md
```

**파일이 없으면**: setup-init 스킬을 참조하여 기본 설정을 자동 생성합니다:

1. 사용자에게 "설정 파일이 없습니다. 기본값으로 초기 설정을 진행합니다." 안내
2. setup-init 스킬의 "설정 파일 템플릿"으로 `github-autopilot.local.md` 생성
3. setup-init 스킬의 "Rules 파일"로 `.claude/rules/autopilot-*.md` 설치
4. setup-init 스킬의 "GitHub 라벨" 명령으로 라벨 생성
5. 설정 완료 후 Step 1로 진행

**파일이 있으면**: Step 0.5로 진행.

### Step 0.5: Preflight Check

preflight-check 스킬의 절차에 따라 환경을 검증합니다.

1. **Convention Verification** — Rules 파일, CLAUDE.md 점검
2. **Automation Environment Verification** — gh auth, hooks, quality gate, git remote 점검
3. **Spec Existence Check** — spec_paths 경로에 스펙 파일 존재 확인

결과를 테이블로 출력합니다.

**모든 항목 PASS (WARN 허용)**: Step 1로 진행합니다.

**FAIL 항목 있음**: `AskUserQuestion`으로 사용자에게 확인합니다.

```
결과물 퀄리티를 보장하기 어려운 환경입니다. 계속 진행하시겠습니까?
```

- **사용자 Yes** → `⚠️ Preflight FAIL 항목이 있지만 사용자 승인으로 계속 진행합니다.` 경고를 출력하고 Step 1로 진행
- **사용자 No** → preflight-check 스킬의 해결 가이드를 출력하고 종료

### Step 1: 세션 초기화

autopilot 세션 ID와 로그 디렉토리를 생성합니다:

```bash
SESSION_ID=$(uuidgen | tr '[:upper:]' '[:lower:]' | head -c 8)
REPO_NAME=$(basename "$(git rev-parse --show-toplevel 2>/dev/null || echo unknown)")
LOG_DIR="/tmp/autopilot-${REPO_NAME}-${SESSION_ID}"
mkdir -p "$LOG_DIR"
```

### Step 2: 설정 로딩

`github-autopilot.local.md`에서 `loops`, `test_watch`, `custom_loops`, `label_prefix`를 읽습니다.

#### 하위 호환

- `loops` 키가 있으면 → 직접 사용
- `default_intervals`만 있으면 → 각 항목을 `{interval: value, enabled: true}`로 변환
- 둘 다 없으면 → 아래 기본값 사용

기본값:
```yaml
loops:
  gap_watch:
    interval: "30m"
    enabled: true
  build_issues:
    interval: "15m"
    enabled: true
  merge_prs:
    interval: "10m"
    enabled: true
  ci_watch:
    interval: "20m"
    enabled: true
  ci_fix:
    interval: "15m"
    enabled: true
  qa_boost:
    interval: "1h"
    enabled: true
label_prefix: "autopilot:"
test_watch: []
custom_loops: []
```

### Step 3: 빌트인 루프 시작

`loops`에서 `enabled: true`인 항목에 대해 `Bash(run_in_background)`로 실행합니다.

루프 이름 → 커맨드 매핑:

| Config Key | Command |
|---|---|
| gap_watch | /github-autopilot:gap-watch |
| build_issues | /github-autopilot:build-issues |
| merge_prs | /github-autopilot:merge-prs |
| ci_watch | /github-autopilot:ci-watch |
| ci_fix | /github-autopilot:ci-fix |
| qa_boost | /github-autopilot:qa-boost |

각 항목:
```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/run-loop.sh "/github-autopilot:{command}" "{interval}" "{label_prefix}" "true" "${LOG_DIR}"
```

### Step 3.5: Test Watch 루프 시작

`test_watch` 배열이 비어있지 않으면, 각 스위트별 루프를 시작합니다:

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/run-loop.sh "/github-autopilot:test-watch {suite.name}" "{suite.interval}" "{label_prefix}" "true" "${LOG_DIR}"
```

### Step 4: Custom 루프 시작

`custom_loops` 배열이 비어있지 않으면, 각 항목에 대해 루프를 시작합니다:

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/run-loop.sh "{custom.command}" "{custom.interval}" "{label_prefix}" "false" "${LOG_DIR}"
```

> custom 루프는 idle check를 수행하지 않습니다 (`"false"`).

### Step 5: 결과 출력

시작된 루프 목록을 테이블로 출력합니다:

```
## Autopilot 시작

**Session**: {SESSION_ID}
**Logs**: {LOG_DIR}/

| Loop | Command | Interval | Type |
|------|---------|----------|------|
| Gap Watch | /github-autopilot:gap-watch | 30m | built-in |
| Build Issues | /github-autopilot:build-issues | 15m | built-in |
| Merge PRs | /github-autopilot:merge-prs | 10m | built-in |
| CI Watch | /github-autopilot:ci-watch | 20m | built-in |
| CI Fix | /github-autopilot:ci-fix | 15m | built-in |
| QA Boost | /github-autopilot:qa-boost | 1h | built-in |
| Test: e2e | /github-autopilot:test-watch e2e | 2h | test_watch |
| Deploy Check | /my-project:deploy-check | 30m | custom |

{N}개 루프가 시작되었습니다.
pipeline이 idle 상태가 되면 빌트인 루프는 자동으로 종료됩니다.

로그 확인: `ls {LOG_DIR}/` 또는 `cat {LOG_DIR}/gap-watch.log`
```

## 주의사항

- `/autopilot`을 다시 실행하면 새로운 세션(별도 LOG_DIR)으로 루프가 추가 spawn됩니다. 기존 루프를 먼저 종료하세요.
- 세션 종료 시 모든 백그라운드 루프가 함께 종료됩니다.
- 개별 커맨드를 1회 실행하려면 해당 커맨드를 직접 호출하세요 (예: `/github-autopilot:gap-watch`).
- 각 루프의 실행 이력은 `{LOG_DIR}/{command}.log`에서 tick별 START/END/IDLE 로그로 확인할 수 있습니다.

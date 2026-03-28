---
name: preflight-check
description: autopilot 시작 전 환경 검증 절차. Convention, Automation, Spec 3개 영역을 점검하여 자율 운영 준비 상태를 확인
version: 1.0.0
---

# Preflight Check

autopilot 루프를 시작하기 전에 환경이 자율 운영에 적합한지 검증한다.
3개 영역(Convention, Automation, Spec)을 순서대로 점검하고 결과를 테이블로 출력한다.

## 검증 영역

### A. Convention Verification

프로젝트에 autopilot 운영에 필요한 규칙과 문서가 갖춰져 있는지 확인한다.

#### A-1. Rules 파일 존재

`.claude/rules/autopilot-*.md` 파일이 존재하는지 확인한다.

```bash
ls .claude/rules/autopilot-*.md 2>/dev/null
```

| 조건 | 상태 |
|------|------|
| autopilot-*.md 파일 1개 이상 존재 | PASS |
| autopilot-*.md 파일 없음 | FAIL |

PASS 시 발견된 파일명을 Detail에 나열한다.

#### A-2. CLAUDE.md 존재 및 내용

`CLAUDE.md` 파일이 존재하고 다음 항목을 포함하는지 확인한다.

```bash
test -f CLAUDE.md
```

점검 항목:

| 항목 | 검색 기준 | 없을 때 |
|------|-----------|---------|
| Build/test commands | `cargo`, `npm`, `go`, `make`, `pytest` 등 빌드/테스트 명령어 존재 | WARN |
| File tree | `├`, `└`, `directory`, `structure`, `파일 구조`, `file tree` 등 트리 관련 키워드 | WARN |
| Tech stack / conventions | `stack`, `convention`, `기술`, `원칙`, `principle` 등 키워드 | WARN |

| 조건 | 상태 |
|------|------|
| CLAUDE.md 존재 + 3개 항목 모두 포함 | PASS |
| CLAUDE.md 존재 + 일부 항목 누락 | WARN (누락 항목 명시) |
| CLAUDE.md 없음 | FAIL |

### B. Automation Environment Verification

자동화 실행에 필요한 외부 도구와 설정이 준비되어 있는지 확인한다.

#### B-1. GitHub 인증

```bash
gh auth status
```

| 조건 | 상태 |
|------|------|
| 인증 성공 (exit 0) | PASS |
| 인증 실패 | FAIL |

#### B-2. Guard PR Base Hook 등록

`.claude/settings.local.json`에 `guard-pr-base` hook이 등록되어 있는지 확인한다.

```bash
cat .claude/settings.local.json 2>/dev/null | grep -q "guard-pr-base"
```

| 조건 | 상태 |
|------|------|
| guard-pr-base 포함 | PASS |
| 파일 없음 또는 미포함 | WARN |

#### B-3. Quality Gate Command

`github-autopilot.local.md`의 `quality_gate_command`가 설정되어 있으면 해당 명령어가 실행 가능한지 확인한다.

```bash
# quality_gate_command가 비어있지 않으면 첫 번째 토큰의 존재 확인
command -v "${first_token}" >/dev/null 2>&1
```

| 조건 | 상태 |
|------|------|
| 미설정 (비어있음) — 자동 감지 사용 | PASS (auto-detect) |
| 설정됨 + 명령어 실행 가능 | PASS |
| 설정됨 + 명령어 실행 불가 | FAIL |

#### B-4. Git Remote

```bash
git remote get-url origin
```

| 조건 | 상태 |
|------|------|
| origin URL 존재 (exit 0) | PASS |
| origin 없음 | FAIL |

### C. Spec Existence Check

스펙 파일이 존재하는지 경량 확인한다. 스펙 품질 검증은 이 스킬의 범위가 아니다.

#### C-1. Spec 파일 존재

`github-autopilot.local.md`의 `spec_paths`에 지정된 경로에서 `.md` 파일이 존재하는지 확인한다.

```bash
# spec_paths의 각 경로에 대해
find ${spec_path} -name "*.md" -type f 2>/dev/null | head -5
```

| 조건 | 상태 |
|------|------|
| spec_paths 설정 + .md 파일 1개 이상 존재 | PASS (발견된 파일 수 표시) |
| spec_paths 설정 + .md 파일 없음 | FAIL |
| spec_paths 미설정 | WARN (spec_paths not configured) |

## 결과 출력

모든 검증 완료 후 아래 형식으로 테이블을 출력한다:

```
## Preflight Check Results

| Check | Status | Detail |
|-------|--------|--------|
| Rules | ✅ PASS | autopilot-always-pull-first.md, autopilot-draft-branch.md |
| CLAUDE.md | ⚠️ WARN | file tree 없음 |
| gh auth | ✅ PASS | |
| Hooks | ✅ PASS | guard-pr-base registered |
| Quality Gate | ✅ PASS | auto-detect |
| Git Remote | ✅ PASS | origin → https://github.com/... |
| Spec files | ❌ FAIL | spec/ 디렉토리 비어있음 |
```

## 최종 판정

| 조건 | 판정 | 동작 |
|------|------|------|
| 모든 항목 PASS (WARN 허용) | READY | 루프 시작 진행 |
| FAIL 항목 1개 이상 | NOT READY | 사용자 확인 필요 |

### NOT READY 시 동작

FAIL 항목이 있으면 `AskUserQuestion`으로 사용자에게 확인한다:

```
결과물 퀄리티를 보장하기 어려운 환경입니다. 계속 진행하시겠습니까?

FAIL 항목:
- {실패 항목 목록}

해결 가이드:
- Rules 없음 → `/github-autopilot:setup` 실행
- gh auth 실패 → `gh auth login` 실행
- Spec 없음 → spec_paths 경로에 스펙 문서 추가
- Git remote 없음 → `git remote add origin <url>` 실행
- Quality gate 실패 → quality_gate_command 값 확인
```

- **사용자 Yes** → WARN 로그를 남기고 루프 시작 진행
- **사용자 No** → 위 해결 가이드를 출력하고 종료

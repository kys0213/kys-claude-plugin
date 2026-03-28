---
name: preflight-check
description: autopilot 시작 전 환경 검증 절차. CLAUDE.md 파일트리 기반 rules 커버리지, 자동화 환경, 스펙 존재 여부를 점검
version: 2.0.0
---

# Preflight Check

autopilot 루프를 시작하기 전에 환경이 자율 운영에 적합한지 검증한다.
결정적 검사는 `preflight-check.sh` 스크립트가 수행하고, 결과를 기반으로 판정한다.

## 실행

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/preflight-check.sh [config_file]
```

- exit 0: 모든 항목 PASS (WARN 허용) → 루프 시작 가능
- exit 1: FAIL 항목 있음 → 사용자 확인 필요

출력: JSON 배열 (각 항목별 check/status/detail)

## 검증 영역

### A. Convention Verification

#### A-1. CLAUDE.md 필수 항목

CLAUDE.md에 아래 3가지가 포함되어 있는지 확인:
- **파일 트리**: 프로젝트 구조화된 디렉토리 트리
- **빌드/테스트 명령어**: cargo, npm, go 등 실행 가능한 명령
- **기술 스택 / 코딩 컨벤션**: 프로젝트 원칙

#### A-2. Rules 커버리지 (CLAUDE.md 파일트리 기반)

CLAUDE.md에 정의된 파일트리의 주요 디렉토리가 `.claude/rules/`의 paths frontmatter로 커버되는지 검증.

예시:
```
CLAUDE.md 파일트리:
  src/api/      → rules에 paths: "src/api/**" 매칭하는 규칙 있는가?
  src/auth/     → rules에 auth 관련 규칙 있는가?
  src/domain/   → rules에 domain 규칙 있는가?

결과:
  ✅ src/api/    → api-conventions.md (paths: "src/api/**")
  ❌ src/auth/   → 대응하는 rule 없음
  ⚠️ 전체        → "**" 패턴으로 커버 (범용)
```

| 조건 | 상태 |
|------|------|
| 모든 주요 디렉토리 커버 | PASS |
| 1-2개 미커버 | WARN |
| 3개 이상 미커버 | FAIL |

### B. Automation Environment Verification

| 항목 | 검증 방법 | PASS | FAIL |
|------|-----------|------|------|
| gh auth | `gh auth status` | exit 0 | exit != 0 |
| Hooks | `settings.local.json`에 guard-pr-base 포함 | 포함 | 미포함 (WARN) |
| Quality Gate | command 존재 확인 (미설정 시 auto-detect) | 실행 가능 | command not found |
| Git Remote | `git remote get-url origin` | URL 존재 | origin 없음 |

### C. Spec Existence Check

`spec_paths`에 지정된 경로에 `.md` 파일이 존재하는지 확인.
스펙 품질 검증은 spec-validator agent의 범위.

| 조건 | 상태 |
|------|------|
| .md 파일 1개 이상 존재 | PASS |
| .md 파일 없음 | FAIL |
| spec_paths 미설정 | WARN |

## 결과 테이블 예시

```
| Check | Status | Detail |
|-------|--------|--------|
| CLAUDE.md | ✅ PASS | file tree, build commands, conventions 포함 |
| Rules coverage | ⚠️ WARN | 미커버: src/auth (5/6) |
| gh auth | ✅ PASS | authenticated |
| Hooks | ✅ PASS | guard-pr-base registered |
| Quality Gate | ✅ PASS | auto-detect |
| Git Remote | ✅ PASS | origin → https://github.com/... |
| Spec files | ❌ FAIL | spec/ 디렉토리 비어있음 |
```

## FAIL 시 해결 가이드

| FAIL 항목 | 해결 방법 |
|-----------|-----------|
| CLAUDE.md 없음 | 프로젝트 루트에 CLAUDE.md 생성. 파일트리, 빌드 명령어, 코딩 컨벤션 포함 |
| Rules 미커버 | `.claude/rules/`에 해당 디렉토리의 컨벤션 규칙 추가. paths frontmatter로 스코프 지정 |
| gh auth 실패 | `gh auth login` 실행 |
| Spec 없음 | spec_paths 경로에 스펙 문서 추가 |
| Git remote 없음 | `git remote add origin <url>` 실행 |
| Quality gate 실패 | `github-autopilot.local.md`의 quality_gate_command 값 확인 |

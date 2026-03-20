---
description: 스펙 문서와 구현 코드 간 갭을 체계적으로 분석합니다
argument-hint: "<스펙파일> [코드경로]"
allowed-tools: ["Task", "Glob"]
---

# 스펙 분석 커맨드 (/spec-analyze)

스펙 문서(마크다운)에서 요구사항을 추출하고, 구현 코드와 비교하여 행위 커버리지·보안·성능 관점의 갭 분석 리포트를 생성합니다.

## 사용법

```bash
/spec-analyze "docs/auth-spec.md"
/spec-analyze "docs/auth-spec.md" "src/auth"
/spec-analyze "plans/design.md" "src/**/*.rs"
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 분석 대상 스펙 마크다운 경로 |
| 코드경로 | No | 구현 코드 경로/패턴 (미지정 시 스펙에서 추론) |

## 작업 프로세스

### Step 1: 입력 파싱

사용자 요청에서 추출:
- **스펙 파일 경로**: 마크다운 파일 (필수)
- **코드 경로**: glob 패턴 또는 디렉토리 (선택)

스펙 파일이 존재하는지 Glob으로 확인. 없으면 즉시 에러:
```
Error: 스펙 파일을 찾을 수 없습니다: [경로]
```

### Step 2: 스펙 파싱 + 구조 매핑 (병렬)

spec-parser와 structure-mapper를 동시에 실행합니다.

**2-A: spec-parser** (`run_in_background=true`):
```
스펙 파일을 분석하여 구조화된 요구사항 목록을 추출해주세요.

스펙 파일: [경로]
```

**반환값**: 구조화된 요구사항 목록 (JSON)
```json
{
  "title": "스펙 제목",
  "summary": "스펙 요약 (1-2문장)",
  "components": ["컴포넌트1", "컴포넌트2"],
  "requirements": [
    {
      "id": "R1",
      "component": "컴포넌트1",
      "category": "functional | non-functional | security | performance",
      "description": "요구사항 설명",
      "expected_behavior": "기대 동작",
      "acceptance_criteria": ["조건1", "조건2"]
    }
  ]
}
```

**2-B: structure-mapper** (`run_in_background=true`):
```
코드 경로의 파일 트리를 분석하고 디렉토리 구조를 매핑해주세요.

코드 경로: [사용자 지정 경로 또는 프로젝트 루트]
```

**반환값 (structure-mapper)**: 파일 트리 + 디렉토리 매핑
```json
{
  "language": "rust | typescript | python | go | mixed",
  "mappings": [
    {
      "directory": "src/auth/",
      "files": ["src/auth/login.rs", "src/auth/token.rs"],
      "test_files": ["tests/auth_test.rs"]
    }
  ],
  "unmapped_files": ["src/utils/helpers.rs"]
}
```

### Step 3: 매핑 결합

두 에이전트의 결과를 대기한 후 결합합니다:
- spec-parser의 **components** ↔ structure-mapper의 **directories**를 이름 기반으로 매칭
- 매칭되지 않는 컴포넌트는 `unmapped_components`로 표시

### Step 4: 갭 분석 (Agent Team 병렬)

Step 2의 요구사항 + Step 3의 매핑 정보를 기반으로 3명의 분석가가 병렬로 동작합니다.

#### 4-A: Agent Teams 사용 (권장)

Agent Teams가 활성화된 경우:

```
에이전트 팀을 만들어 스펙-구현 갭 분석을 수행해주세요.

## 공통 정보

스펙: [스펙 제목]
요구사항 목록:
[Step 2 결과 전체]

컴포넌트-파일 매핑:
[Step 3 결과 전체]

## 팀 구성

### Teammate 1: 행위 커버리지 분석가 (compliance-checker)
각 요구사항(R1, R2, ...)이 매핑된 코드에서 실제로 구현되었는지 분석합니다.
매핑된 파일을 Read로 읽고, 요구사항의 기대 동작과 수용 기준을 코드와 1:1 대조합니다.
[아래 compliance-checker 에이전트 기준 참조]

### Teammate 2: 보안 분석가 (security-reviewer)
매핑된 코드 파일을 Read로 읽고, 보안 관점에서 개선 포인트를 분석합니다.
스펙에 보안 요구사항이 있으면 해당 구현도 확인합니다.
[아래 security-reviewer 에이전트 기준 참조]

### Teammate 3: 성능 분석가 (perf-reviewer)
매핑된 코드 파일을 Read로 읽고, 성능 관점에서 개선 포인트를 분석합니다.
스펙에 성능 요구사항이 있으면 해당 구현도 확인합니다.
[아래 perf-reviewer 에이전트 기준 참조]
```

#### 4-B: Subagent Fallback

Agent Teams가 비활성이면 Task 3개를 `run_in_background=true`로 병렬 실행합니다.
각 Task에 해당 에이전트의 분석 기준 + 공통 정보를 프롬프트로 전달합니다.

### Step 5: 결과 취합 및 종합 리포트

3개 분석 결과를 취합하여 아래 형식의 리포트를 작성합니다.

---

## 종합 리포트 형식

```markdown
# 스펙 분석 리포트

## 개요
- **스펙**: [파일명]
- **요구사항**: N개 추출
- **컴포넌트**: [컴포넌트 목록]
- **분석 대상 파일**: M개

## 행위 커버리지 (X/N, XX%)

| # | 요구사항 | 상태 | 구현 위치 | 비고 |
|---|---------|------|----------|------|
| R1 | 사용자 로그인 | ✅ 구현 | auth/login.rs:42 | |
| R2 | 토큰 갱신 | ⚠️ 부분 | auth/token.rs:15 | 만료 처리 미구현 |
| R3 | 세션 무효화 | ❌ 미구현 | - | 코드 없음 |

### 미구현 요구사항 상세
[❌ 항목에 대한 상세 설명]

### 부분 구현 요구사항 상세
[⚠️ 항목에 대한 갭 설명 및 보완 방안]

## 보안 분석
[security-reviewer 결과 — issue-report 스킬 형식]

## 성능 분석
[perf-reviewer 결과 — issue-report 스킬 형식]

## 종합 권장사항

### Critical (즉시 조치)
1. [미구현 요구사항 + 심각한 보안/성능 이슈]

### Important (조치 권장)
1. [부분 구현 보완 + 중요 개선 포인트]

### 참고사항
1. [경미한 개선 포인트]
```

## 주의사항

- **토큰 최적화**: MainAgent는 파일 내용을 읽지 않음. 파일 읽기는 모두 Teammate/Subagent가 수행
- **코드 경로 미지정 시**: structure-mapper가 스펙의 컴포넌트명을 기반으로 프로젝트 내 관련 디렉토리를 자동 탐색
- **Multi-LLM 확장**: compliance-checker 내에서 Codex/Gemini 호출 가능 (선택적)

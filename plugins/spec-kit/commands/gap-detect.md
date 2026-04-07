---
description: 스펙 문서 기반으로 구현 코드의 갭을 entry point별 call chain 분석으로 검출합니다
argument-hint: "<스펙파일> [코드경로] [--reverse]"
allowed-tools: ["Task", "Glob", "Read"]
---

# 갭 분석 커맨드 (/gap-detect)

스펙 문서에서 요구사항을 추출하고, entry point별 call chain을 추적하여 구현 갭을 분석합니다.
LSP가 설치되어 있으면 정밀한 call hierarchy를 사용하고, 없으면 grep fallback + 안내를 제공합니다.

## 사용법

```bash
/gap-detect "docs/auth-spec.md"
/gap-detect "docs/auth-spec.md" "src/auth"
/gap-detect "plans/design.md" "src/**/*.rs"
/gap-detect "docs/auth-spec.md" --reverse
/gap-detect "docs/auth-spec.md" "src/auth" --reverse
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 분석 대상 스펙 마크다운 경로 |
| 코드경로 | No | 구현 코드 경로/패턴 (미지정 시 스펙에서 추론) |
| --reverse | No | 역방향 갭 분석 추가 실행 (코드 → 스펙 방향) |

## 작업 프로세스

### Step 1: 입력 파싱

사용자 요청에서 추출:
- **스펙 파일 경로**: 마크다운 파일 (필수)
- **코드 경로**: glob 패턴 또는 디렉토리 (선택)
- **--reverse 플래그**: 역방향 분석 실행 여부 (선택)

인자 파싱 규칙:
- `--`로 시작하는 인자는 옵션으로 처리 (`--reverse` 플래그 설정)
- `--`로 시작하지 않는 인자는 경로로 처리 (첫 번째는 스펙 파일, 두 번째는 코드 경로)

스펙 파일이 존재하는지 Glob으로 확인. 없으면 즉시 에러:
```
Error: 스펙 파일을 찾을 수 없습니다: [경로]
```

### Step 2: 스펙 파싱 (spec-parser)

spec-parser 에이전트에게 스펙 파일 경로를 전달합니다.

**Agent 호출** (`run_in_background=false`):
```
스펙 파일을 분석하여 구조화된 요구사항 목록을 추출해주세요.

스펙 파일: [경로]
```

**반환값**: 구조화된 요구사항 목록 (JSON)

### Step 3: 구조 매핑 + Entry Point 추출 (structure-mapper)

structure-mapper 에이전트에게 요구사항의 컴포넌트 목록과 코드 경로를 전달합니다.

**Agent 호출** (`run_in_background=false`):
```
스펙의 컴포넌트를 기반으로 코드 구조를 매핑하고, entry point를 추출해주세요.
LSP 도구 존재 여부도 확인해주세요.

컴포넌트: [Step 2에서 추출한 components]
코드 경로: [사용자 지정 경로 또는 프로젝트 루트]
```

**반환값**: 매핑 + entry point 목록 + LSP 상태 (JSON)

**LSP 안내 출력**: structure-mapper가 `lsp_warnings`를 반환하면, 사용자에게 즉시 출력합니다:
```
⚠️ LSP 안내
[lsp_warnings 내용을 그대로 출력]
```
이 안내는 절대 생략하지 않습니다.

### Step 4: 갭 분석 (gap-analyzer)

Step 2의 요구사항 + Step 3의 매핑/entry point/LSP 정보를 모두 전달합니다.

**Agent 호출** (`run_in_background=false`):
```
스펙 요구사항과 코드의 갭을 entry point별 call chain 기반으로 분석해주세요.

## 요구사항
[Step 2 결과 전체]

## 컴포넌트-파일 매핑
[Step 3 mappings]

## Entry Points
[Step 3 entry_points]

## LSP 사용 가능 여부
[Step 3 lsp_available]
```

**반환값**: 종합 갭 분석 리포트 (Markdown)

### Step 5: 분석 방식 기록

LSP 안내가 있었다면 분석 방식을 기록해 둡니다 (최종 출력 시 사용):
```
📋 분석 방식: LSP call hierarchy (정밀) / grep fallback (추정)
```

> 결과 출력은 Step 7에서 일괄 수행합니다.

### Step 6: 역방향 갭 분석 (--reverse 옵션 시에만 실행)

`--reverse` 옵션이 지정된 경우, reverse-gap-analyzer 에이전트를 호출하여 코드 → 스펙 방향의 갭을 분석합니다.

**Agent 호출** (`run_in_background=false`):
```
코드의 entry point를 스펙 요구사항과 대조하여 미지정 구현을 탐지해주세요.

## 요구사항
[Step 2 결과 전체]

## 컴포넌트-파일 매핑
[Step 3 mappings]

## Entry Points
[Step 3 entry_points]

## LSP 사용 가능 여부
[Step 3 lsp_available]
```

**반환값**: 역방향 갭 분석 리포트 (Markdown)

### Step 7: 최종 결과 출력

#### --reverse 미지정 시

gap-analyzer의 정방향 리포트를 사용자에게 출력합니다.

#### --reverse 지정 시

정방향 리포트에서 "종합 권장사항" 섹션을 **제외**하고 출력합니다 (아래 통합 종합 권장사항으로 대체되므로 중복 방지).
이어서 역방향 리포트를 출력하고, 통합 종합 권장사항 섹션을 추가합니다:

```markdown
---

# 종합 권장사항 (정방향 + 역방향)

## Critical (즉시 조치)
1. [정방향: 미구현 요구사항]
2. [역방향: critical severity의 Unspecified 항목]

## Important (조치 권장)
1. [정방향: 부분 구현 보완]
2. [역방향: warning severity의 Unspecified 항목]
3. [역방향: Under-specified 항목]

## 참고사항
1. [정방향: 경미한 개선 포인트]
2. [역방향: info severity의 Unspecified 항목]

## 스펙-코드 일치도
- 정방향 커버리지 (스펙 → 코드): XX%
- 역방향 커버리지 (코드 → 스펙): YY%
```

## 주의사항

- **토큰 최적화**: MainAgent는 파일 내용을 읽지 않음. 파일 읽기는 모두 Sub-agent가 수행
- **코드 경로 미지정 시**: structure-mapper가 스펙의 컴포넌트명을 기반으로 프로젝트 내 관련 디렉토리를 자동 탐색
- **LSP 안내는 반드시 출력**: 미설치된 LSP가 있으면 설치 가이드를 사용자에게 보여주고, fallback으로 진행
- **--reverse 옵션**: 역방향 분석은 정방향 분석 완료 후 실행. 정방향 분석 로직은 수정하지 않음

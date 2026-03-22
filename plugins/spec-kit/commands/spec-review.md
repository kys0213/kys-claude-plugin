---
description: 작성된 스펙 문서의 완성도를 4가지 관점(구조, 상세, 검증, 일관성)으로 검증합니다
argument-hint: "<스펙파일>"
allowed-tools: ["Task", "Glob", "Read"]
---

# 스펙 리뷰 커맨드 (/spec-review)

작성된 스펙 문서 자체의 품질을 검증합니다. 코드 분석은 수행하지 않습니다.
전체 구조(Big Picture), 상세 정의(Detail), 검증 가능성(Verification), 일관성(Consistency) 4가지 관점으로 평가합니다.

## 사용법

```bash
/spec-review "docs/auth-spec.md"
/spec-review "plans/payment-design.md"
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 검증 대상 스펙 마크다운 경로 |

## 작업 프로세스

### Step 1: 입력 파싱

사용자 요청에서 스펙 파일 경로를 추출합니다.

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

### Step 3: 스펙 품질 검증 (spec-quality-checker)

spec-quality-checker 에이전트에게 파싱 결과와 원본 스펙 경로를 전달합니다.

**Agent 호출** (`run_in_background=false`):
```
스펙 문서의 완성도를 4가지 관점으로 검증해주세요.

## 파싱된 요구사항
[Step 2 결과 전체]

## 원본 스펙 파일
[경로] — 원문을 직접 Read하여 맥락을 확인하세요.
```

**반환값**: 스펙 품질 리포트 (Markdown)

### Step 4: 결과 출력

spec-quality-checker의 리포트를 사용자에게 출력합니다.

## 주의사항

- **코드 분석 없음**: 이 커맨드는 스펙 문서만 평가합니다. 코드 갭 분석은 `/gap-detect`를 사용하세요.
- **토큰 최적화**: MainAgent는 스펙 파일을 읽지 않음. 읽기는 모두 Sub-agent가 수행
- **스펙 형식 무관**: 마크다운 형식이 아닌 내용의 완성도를 평가

---
description: 작성된 스펙 문서의 완성도를 4가지 관점(구조, 상세, 검증, 일관성)으로 검증합니다
argument-hint: "<스펙파일 | 디렉터리 | 글로브패턴>"
allowed-tools: ["Task", "Glob", "Read"]
---

# 스펙 리뷰 커맨드 (/spec-review)

작성된 스펙 문서 자체의 품질을 검증합니다. 코드 분석은 수행하지 않습니다.
전체 구조(Big Picture), 상세 정의(Detail), 검증 가능성(Verification), 일관성(Consistency) 4가지 관점으로 평가합니다.

## 사용법

```bash
/spec-review "docs/auth-spec.md"              # 단일 파일
/spec-review "spec/v5.1/"                      # 디렉터리 (하위 .md 전체)
/spec-review "spec/**/*-spec.md"               # Glob 패턴
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 대상 | Yes | 스펙 마크다운 경로, 디렉터리, 또는 Glob 패턴 |

## 작업 프로세스

### Step 1: 입력 파싱 및 파일 수집

사용자 요청에서 대상 경로를 추출한 뒤, 입력 유형을 판별합니다:

1. **Glob 패턴** (`*`, `?` 포함): Glob 도구로 매칭되는 `.md` 파일 목록을 수집
2. **디렉터리** (`/`로 끝남): Glob으로 `{경로}/**/*.md` 패턴을 실행하여 하위 모든 .md 수집
3. **단일 파일**: 해당 파일만 사용 (기존 동작)

파일이 하나도 발견되지 않으면 즉시 에러:
```
Error: 스펙 파일을 찾을 수 없습니다: [경로]
```

수집된 파일 목록을 사용자에게 확인용으로 출력합니다:
```
발견된 스펙 파일 (N개):
- docs/overview.md
- docs/api-spec.md
- docs/data-model.md
```

### Step 2: 스펙 파싱 (spec-parser)

spec-parser 에이전트에게 수집된 파일 경로 목록을 전달합니다.

**Agent 호출** (`run_in_background=false`):
```
스펙 파일을 분석하여 구조화된 요구사항 목록을 추출해주세요.

스펙 파일:
- [경로1]
- [경로2]
- ...
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
- [경로1]
- [경로2]
- ...
원문을 직접 Read하여 맥락을 확인하세요.
```

**반환값**: 스펙 품질 리포트 (Markdown)

### Step 4: 결과 출력

spec-quality-checker의 리포트를 사용자에게 출력합니다.

## 주의사항

- **코드 분석 없음**: 이 커맨드는 스펙 문서만 평가합니다. 코드 갭 분석은 `/gap-detect`를 사용하세요.
- **토큰 최적화**: MainAgent는 스펙 파일을 읽지 않음. 읽기는 모두 Sub-agent가 수행
- **스펙 형식 무관**: 마크다운 형식이 아닌 내용의 완성도를 평가
- **다중 파일**: 디렉터리/Glob 입력 시 모든 .md 파일을 하나의 스펙 세트로 취급하여 통합 평가

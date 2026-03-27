---
description: 작성된 스펙 문서의 완성도를 4가지 관점(구조, 상세, 검증, 일관성)으로 병렬 검증합니다
argument-hint: "<스펙파일 [스펙파일2 ...]>"
allowed-tools: ["Task", "Glob", "Read", "AskUserQuestion"]
---

# 스펙 리뷰 커맨드 (/spec-review)

작성된 스펙 문서 자체의 품질을 검증합니다. 코드 분석은 수행하지 않습니다.
전체 구조(Big Picture), 상세 정의(Detail), 검증 가능성(Verification), 일관성(Consistency) 4가지 관점으로 평가합니다.

## 사용법

```bash
/spec-review "docs/auth-spec.md"                              # 단일 파일
/spec-review "docs/api-spec.md" "docs/data-model.md"          # 다중 파일 (명시적)
/spec-review "spec/v5.1/"                                      # 디렉터리 → 파일 목록 확인 후 진행
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 하나 이상의 스펙 마크다운 경로 |

## 작업 프로세스

### Step 1: 입력 파싱 및 파일 확정

사용자 요청에서 대상 경로를 추출합니다.

#### Case A: 명시적 파일 경로

파일 경로가 하나 이상 명시된 경우, 각 파일이 존재하는지 Glob으로 확인합니다.
존재하지 않는 파일이 있으면 즉시 에러:
```
Error: 스펙 파일을 찾을 수 없습니다: [경로]
```

#### Case B: 디렉터리 또는 Glob 패턴

디렉터리나 패턴이 입력된 경우, Glob으로 매칭되는 `.md` 파일 목록을 수집한 뒤 **사용자에게 확인을 요청**합니다:

```
발견된 스펙 파일 (N개):
1. docs/overview.md
2. docs/api-spec.md
3. docs/data-model.md
4. docs/changelog.md

이 파일들을 모두 리뷰 대상으로 사용할까요?
제외할 파일이 있으면 번호를 알려주세요. (예: "4번 제외")
```

AskUserQuestion으로 확인을 받은 뒤, 최종 파일 목록을 확정합니다.

#### Case C: 인자 없음

인자가 전달되지 않은 경우, AskUserQuestion으로 스펙 파일 경로를 요청합니다:

```
리뷰할 스펙 파일 경로를 알려주세요.
(예: "docs/auth-spec.md" 또는 여러 파일: "docs/api.md docs/db.md")
```

### Step 2: 스펙 파싱 (spec-parser)

spec-parser 에이전트에게 확정된 파일 경로 목록을 전달합니다.

**Agent 호출** (`run_in_background=false`):
```
스펙 파일을 분석하여 구조화된 요구사항, 섹션 원문, 용어집, 교차 참조를 추출해주세요.

스펙 파일:
- [경로1]
- [경로2]
- ...
```

**반환값**: 구조화된 요구사항, 섹션, 용어집, 교차 참조 (JSON)

### Step 3a: 스펙 품질 검증 (spec-quality-checker)

<<<<<<< HEAD
spec-quality-checker 에이전트에게 파싱 결과를 전달합니다.
=======
spec-quality-checker 에이전트에게 파싱 결과를 전달합니다. A/B/C 관점만 평가합니다.
>>>>>>> c666e9f (feat(spec-kit): add cross-reference-checker agent for structured D1-D5 validation)

**Agent 호출** (`run_in_background=true`):
```
스펙 문서의 완성도를 Big Picture, Detail, Verification 관점으로 검증해주세요.
(Consistency 관점은 별도 에이전트가 수행합니다.)

## 파싱된 스펙 데이터
[Step 2 결과 전체]

<<<<<<< HEAD
파싱 결과에 섹션 원문(sections.raw_text), 용어집(glossary), 교차 참조(cross_references)가 포함되어 있습니다.
=======
파싱 결과에 섹션 원문(sections.raw_text)이 포함되어 있습니다.
>>>>>>> c666e9f (feat(spec-kit): add cross-reference-checker agent for structured D1-D5 validation)
별도로 파일을 읽을 필요 없이 전달된 데이터만으로 평가하세요.
```

**반환값**: A/B/C 관점 품질 리포트 (Markdown)

### Step 3b: 교차 참조 일관성 검증 (cross-reference-checker)

cross-reference-checker 에이전트에게 구조화 데이터를 전달합니다. D 관점을 평가합니다.

**Agent 호출** (`run_in_background=true`):
```
스펙 문서 간 일관성을 교차 참조 기반으로 검증해주세요.

## 요구사항
[requirements JSON]

## 컴포넌트
[components JSON]

## 용어집
[glossary JSON]

## 교차 참조
[cross_references JSON]
```

**반환값**: D 관점 일관성 리포트 (Markdown)

> Step 3a와 3b는 `run_in_background=true`로 병렬 실행합니다. 두 Agent가 완료될 때까지 대기합니다.

### Step 4: 결과 병합

Step 3a 결과 (A/B/C 점수 + 상세)와 Step 3b 결과 (D 점수 + 상세)를 통합합니다.

**종합 점수 산정**:
```
종합 점수 = (A + B + C + D) / 4
```

**통합 리포트 형식**:
```markdown
# 스펙 품질 리포트

## 종합 점수

| 관점 | 점수 | 등급 |
|------|------|------|
| Big Picture (전체 구조) | XX/100 | A/B/C/D |
| Detail (상세 정의) | XX/100 | A/B/C/D |
| Verification (검증 가능성) | XX/100 | A/B/C/D |
| Consistency (일관성) | XX/100 | A/B/C/D |
| **종합** | **XX/100** | **X** |

등급 기준: A(90+), B(70-89), C(50-69), D(0-49)

## 관점별 상세
[Step 3a의 A/B/C 상세]
[Step 3b의 D 상세]

## 우선 보완 항목 (Top 5)
[A/B/C/D 전체에서 미충족/부분충족 항목 중 영향도 상위 5개 선정]
```

통합 리포트를 사용자에게 출력합니다.

## 주의사항

- **코드 분석 없음**: 이 커맨드는 스펙 문서만 평가합니다. 코드 갭 분석은 `/gap-detect`를 사용하세요.
- **토큰 최적화**: MainAgent는 스펙 파일을 읽지 않음. 읽기는 모두 Sub-agent가 수행
- **스펙 형식 무관**: 마크다운 형식이 아닌 내용의 완성도를 평가
- **다중 파일**: 여러 파일은 하나의 스펙 세트로 취급하여 통합 평가
- **명시적 입력 우선**: 파일 경로를 직접 받는 것을 우선하고, 디렉터리/패턴 입력 시 반드시 사용자 확인을 거침
- **quality-checker는 파일을 읽지 않음** — 파싱 데이터만으로 평가

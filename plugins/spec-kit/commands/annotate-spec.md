---
description: 외부 spec 파일에 frontmatter related_paths 추정값을 1차 분석 후 삽입합니다
argument-hint: "<스펙파일>"
allowed-tools: ["Task", "Read", "Edit", "Glob", "Grep", "AskUserQuestion"]
---

# Annotate Spec (/annotate-spec)

외부에서 받은 spec 파일은 보통 frontmatter `related_paths` 가 비어 있다. `/spec-kit:spec-review` 와 `/spec-kit:gap-detect` 가 자율 보강 fallback 에 의존해 정확도가 낮아진다.

이 커맨드는 spec 본문을 1차 분석하여 코드 경로 후보를 추정하고, 사용자 confirm 후 spec 파일 frontmatter 에 직접 write back 한다. `/spec-kit:design` / `/spec-kit:design-detail` 의 frontmatter 권고와 같은 효과를 design 단계가 없는 외부 spec 에도 적용한다.

## 사용법

```bash
/annotate-spec "docs/external-auth-spec.md"
/annotate-spec "spec/v5.1/proxy.md"
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 분석 대상 spec 마크다운 경로 |

## 작업 프로세스

### Step 1: 입력 파싱 + 파일 존재 확인

스펙 파일 경로 추출. Glob 으로 존재 확인. 없으면 즉시 에러:

```
Error: spec 파일을 찾을 수 없습니다: {경로}
```

### Step 2: 현재 frontmatter 확인

`Read` 로 spec 파일을 읽어 상단 YAML frontmatter 를 확인한다:

- **이미 `related_paths` 있음**: 보강 모드 — 기존 항목은 보존하고 신규 후보만 추가 제안
- **frontmatter 있으나 `related_paths` 없음**: 신규 모드 — 기존 frontmatter 에 `related_paths` 필드 추가
- **frontmatter 자체 없음**: 신규 모드 — 파일 상단에 frontmatter 블록 신규 추가
- **frontmatter 파싱 오류 (잘못된 YAML)**: 사용자에게 알리고 수동 수정 요청 후 종료

### Step 3: spec-annotator 에이전트 호출

`spec-annotator` (haiku) 를 `run_in_background=false` 로 spawn 한다. 입력 프롬프트:

```
# Spec Annotation Request

## Spec 파일
- 경로: {spec_file_path}

## 프로젝트 루트
- 경로: {project_root, 기본값: 현재 디렉터리}

## 출력
spec-annotator 의 출력 스키마를 엄수하여 마크다운 리포트 반환.
```

에이전트의 출력은 HIGH / MEDIUM / LOW 신뢰도 분류된 후보 목록이다.

### Step 4: 사용자 confirm (AskUserQuestion)

에이전트 출력을 사용자에게 제시하고 후보별로 확인:

#### 4.1 HIGH 신뢰도 후보

자동 채택 권장. 일괄 confirm:

```
HIGH 신뢰도 후보 ({N}개):
1. internal/auth/ — 근거: ...
2. internal/auth/handler.go — 근거: ...

이 항목들을 모두 채택할까요?
- 모두 채택
- 개별 선택
- 모두 거절
```

#### 4.2 MEDIUM 신뢰도 후보

각 항목별로 채택/거절/수정 선택:

```
MEDIUM: migrations/ — 근거: ...
- 채택
- 거절
- 수정 (다른 경로 입력)
```

#### 4.3 LOW 신뢰도 후보

기본값 거절. 사용자가 명시적으로 채택하지 않으면 무시:

```
LOW 신뢰도 후보 ({N}개) — 기본값 거절:
1. pkg/util/ — 근거: ...

채택할 항목이 있나요? (없으면 모두 거절)
```

#### 4.4 미매칭 키워드 안내

참고용으로 표시 (액션 없음):

```
미매칭 키워드 (참고):
- RefreshTokenRotator
- rate-limiting

이 키워드들은 프로젝트에서 매칭되지 않았습니다. 수동으로 경로를 추가하려면 다음 단계에서 입력해주세요.
```

#### 4.5 추가 경로 수동 입력 (선택)

사용자가 직접 경로를 추가하고 싶으면 입력 받음:

```
추가로 채택할 경로가 있나요? (쉼표로 구분, 없으면 skip)
```

### Step 5: spec 파일 frontmatter 갱신

채택된 경로 목록을 확정한 뒤 `Edit` 로 spec 파일을 수정한다:

#### 5.1 frontmatter 있음 + related_paths 있음 (보강 모드)

기존 `related_paths` 배열에 신규 항목을 append (중복 제거):

```yaml
---
related_paths:
  - {기존 경로 1}
  - {기존 경로 2}
  - {신규 경로 1}   # 추가
  - {신규 경로 2}   # 추가
---
```

#### 5.2 frontmatter 있음 + related_paths 없음

기존 frontmatter 끝에 `related_paths` 필드 추가:

```yaml
---
{기존 필드들}
related_paths:
  - {경로 1}
  - {경로 2}
---
```

#### 5.3 frontmatter 자체 없음

파일 최상단에 frontmatter 블록 신규 추가:

```yaml
---
related_paths:
  - {경로 1}
  - {경로 2}
---

{기존 본문 그대로}
```

### Step 6: 결과 보고

사용자에게 다음 형식으로 출력:

```markdown
# Annotate Spec Report

## Summary
- spec_file: {경로}
- mode: {신규 | 보강}
- candidates_total: {N}
- accepted: {M}
- rejected: {K}
- manually_added: {L}

## 추가된 경로
- `{경로 1}` (HIGH)
- `{경로 2}` (MEDIUM, 사용자 채택)
- `{경로 3}` (수동 입력)

## 거절된 경로
- `{경로 X}` (LOW, 기본 거절)
- `{경로 Y}` (MEDIUM, 사용자 거절)

✅ {spec_file_path} 의 frontmatter 가 갱신되었습니다.
```

## 주의사항

- **MainAgent 는 spec 파일을 직접 분석하지 않음** — 에이전트 (`spec-annotator`) 가 1차 분석을 담당하고, MainAgent 는 사용자 confirm 흐름과 frontmatter 갱신만 수행 (Read, Edit).
- **사용자 confirm 후에만 write back** — 자동 채택 (HIGH 일괄 confirm 포함) 은 사용자가 명시적으로 동의한 경우에만 진행.
- **LOW 신뢰도 후보 자동 채택 금지** — 거짓 매핑이 자율 보강 fallback 보다 나쁘다.
- **frontmatter 파싱 오류 silent fail 금지** — YAML 깨졌을 때 임의 수정하지 말고 사용자에게 알린다.
- **멱등성** — 같은 spec 에 두 번 실행해도 안전. 이미 채택된 경로는 중복 추가하지 않음 (보강 모드의 dedupe).

## 에러 처리

**spec 파일 미존재**: Step 1 에서 즉시 에러.

**프로젝트 디렉터리에서 매칭 0건**: 에이전트 출력의 모든 신뢰도 섹션이 `(없음)` 인 경우. 사용자에게 알리고 옵션 제공:

```
프로젝트에서 매칭되는 경로를 찾지 못했습니다.

옵션:
- 빈 frontmatter 추가 (related_paths: []) — 추후 수동 보강 의도 명시
- 수동 경로 입력 — 직접 경로를 적어 frontmatter 에 추가
- 종료 — frontmatter 변경 없음
```

**frontmatter 파싱 오류 (잘못된 YAML)**: 다음 메시지로 종료:

```
Error: spec 파일의 YAML frontmatter 가 깨져 있습니다.

수동으로 수정 후 다시 실행해주세요. 깨진 부분:
{frontmatter 본문 발췌}
```

**spec-annotator 호출 실패**: 1회 retry. 2회째 실패 시 에러 메시지 출력 후 종료 (frontmatter 변경 없음).

**Edit 실패 (예: 동시 수정)**: 에러 메시지 출력 후 종료. spec 파일은 변경되지 않은 상태 유지.

## Output Examples

### 신규 frontmatter 추가

```markdown
# Annotate Spec Report

## Summary
- spec_file: docs/external-auth-spec.md
- mode: 신규
- candidates_total: 5
- accepted: 3
- rejected: 2
- manually_added: 0

## 추가된 경로
- `internal/auth/` (HIGH)
- `internal/auth/handler.go` (HIGH)
- `migrations/` (MEDIUM, 사용자 채택)

## 거절된 경로
- `pkg/util/` (LOW, 기본 거절)
- `internal/middleware/` (MEDIUM, 사용자 거절)

✅ docs/external-auth-spec.md 의 frontmatter 가 갱신되었습니다.
```

### 보강 모드

```markdown
# Annotate Spec Report

## Summary
- spec_file: spec/v5.1/proxy.md
- mode: 보강
- candidates_total: 3
- accepted: 1
- rejected: 1
- manually_added: 1

## 추가된 경로
- `internal/proxy/router.go` (HIGH)
- `internal/proxy/middleware.go` (수동 입력)

## 거절된 경로
- `cmd/proxy/` (LOW, 기본 거절)

기존 related_paths (2개) 는 보존되었습니다:
- `internal/proxy/`
- `internal/proxy/server.go`

✅ spec/v5.1/proxy.md 의 frontmatter 가 갱신되었습니다.
```

### 매칭 0건

```markdown
# Annotate Spec Report

## Summary
- spec_file: docs/draft-spec.md
- mode: 신규
- candidates_total: 0
- accepted: 0
- rejected: 0
- manually_added: 0

⚠️ 프로젝트에서 매칭되는 경로를 찾지 못했습니다.

빈 frontmatter (related_paths: []) 가 추가되었습니다. 추후 수동으로 보강하거나 `/spec-kit:annotate-spec` 을 다시 실행할 수 있습니다.
```

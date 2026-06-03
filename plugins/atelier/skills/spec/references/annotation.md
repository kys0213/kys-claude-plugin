# 외부 spec frontmatter 주석 절차

`annotate-spec` 가 외부에서 받은 spec 파일에 `related_paths` frontmatter 를 1차 분석 후 삽입하는 프로토콜. `spec-annotator` 에이전트가 후보를 추정하고, 메인은 사용자 confirm 흐름과 frontmatter 갱신만 수행한다.

## frontmatter 모드 판정

`Read` 로 spec 상단 YAML frontmatter 를 확인해 모드를 정한다:

- **이미 `related_paths` 있음**: 보강 모드 — 기존 항목 보존, 신규 후보만 추가 제안
- **frontmatter 있으나 `related_paths` 없음**: 신규 모드 — 기존 frontmatter 에 필드 추가
- **frontmatter 자체 없음**: 신규 모드 — 파일 상단에 frontmatter 블록 신규 추가
- **frontmatter 파싱 오류 (잘못된 YAML)**: 사용자에게 알리고 수동 수정 요청 후 종료

## spec-annotator 호출

`spec-annotator` (haiku) 를 `run_in_background=false` 로 spawn. 입력:

```
# Spec Annotation Request

## Spec 파일
- 경로: {spec_file_path}

## 프로젝트 루트
- 경로: {project_root, 기본값: 현재 디렉터리}

## 출력
spec-annotator 의 출력 스키마를 엄수하여 마크다운 리포트 반환.
```

출력은 HIGH / MEDIUM / LOW 신뢰도 분류된 후보 목록.

## 신뢰도별 confirm (AskUserQuestion)

| 신뢰도 | 기본 정책 | confirm 방식 |
|---|---|---|
| **HIGH** | 자동 채택 권장 | 일괄 confirm (모두 채택 / 개별 선택 / 모두 거절) |
| **MEDIUM** | 항목별 결정 | 각 항목 채택 / 거절 / 수정(다른 경로 입력) |
| **LOW** | 기본 거절 | 명시적으로 채택하지 않으면 무시 |
| 미매칭 키워드 | 액션 없음 | 참고 표시만 — 수동 경로 입력 안내 |

추가로, 채택 확정 전 "수동으로 추가할 경로가 있는지" (쉼표 구분) 물어 보강한다.

## frontmatter 갱신 (Edit)

채택된 경로를 확정한 뒤 `Edit` 로 spec 파일 수정:

- **보강 모드** (related_paths 있음): 기존 배열에 신규 항목 append (중복 제거)
- **신규 모드, frontmatter 있음**: 기존 frontmatter 끝에 `related_paths` 필드 추가
- **신규 모드, frontmatter 없음**: 파일 최상단에 frontmatter 블록 신규 추가 (기존 본문 보존)

## 결과 보고

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

✅ {spec_file_path} 의 frontmatter 가 갱신되었습니다.
```

## 원칙

- **MainAgent 는 spec 파일을 직접 분석하지 않음** — `spec-annotator` 가 1차 분석, 메인은 confirm + frontmatter 갱신 (Read, Edit) 만.
- **사용자 confirm 후에만 write back** — HIGH 일괄 confirm 포함, 명시적 동의 후에만 진행.
- **LOW 신뢰도 자동 채택 금지** — 거짓 매핑이 자율 보강 fallback 보다 나쁘다.
- **frontmatter 파싱 오류 silent fail 금지** — YAML 깨졌을 때 임의 수정 말고 사용자에게 알린다.
- **멱등성** — 같은 spec 에 두 번 실행해도 안전 (보강 모드 dedupe).

## 에러 처리

- **spec 파일 미존재**: 즉시 에러 `Error: spec 파일을 찾을 수 없습니다: {경로}`.
- **frontmatter 파싱 오류 (잘못된 YAML)**: 사용자에게 알리고 수동 수정 요청 후 종료 (임의 수정 금지).
- **spec-annotator 호출 실패**: 1회 retry, 그래도 실패 시 사용자에게 알리고 종료.
- **매칭 0건** (후보 없음): 빈 `related_paths: []` 를 추가할지 / 수동 경로 입력할지 / 그냥 종료할지 AskUserQuestion 으로 선택.
- **Edit 실패** (write back 실패): 변경 내용을 사용자에게 보여주고 수동 반영 안내.

## Output Examples

### 신규 모드 (frontmatter 추가)

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
- `cmd/server/` (MEDIUM, 사용자 거절)

✅ docs/external-auth-spec.md 의 frontmatter 가 갱신되었습니다.
```

### 보강 모드 (기존 related_paths 에 append)

```markdown
# Annotate Spec Report

## Summary
- spec_file: spec/v5.1/proxy.md
- mode: 보강
- candidates_total: 2
- accepted: 1
- manually_added: 0

## 추가된 경로
- `internal/proxy/middleware.go` (HIGH)

(기존 related_paths 2개는 보존, 중복 제거됨)
```

### 매칭 0건

```markdown
매칭된 코드 경로 후보가 없습니다.
- 빈 related_paths: [] 를 추가하시겠습니까?
- 또는 수동으로 경로를 입력하시겠습니까?
- 또는 종료하시겠습니까?
```

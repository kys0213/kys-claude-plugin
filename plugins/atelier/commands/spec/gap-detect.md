---
description: 스펙 문서와 구현 코드 간 갭(미구현, 부분 구현, 발산, spec 부재 코드)을 file:line 인용으로 검출합니다
argument-hint: "<스펙파일>"
allowed-tools: ["Task", "Glob", "Read", "Grep", "AskUserQuestion"]
---

# Gap Detect (/gap-detect)

단일 spec 과 그 구현 코드의 갭(SPEC_ONLY / CODE_ONLY / PARTIAL / DIVERGENT)을 검출합니다. `/spec-review` 와 **백본(L1→L2→audit)이 동일**하고 출력 emphasis 만 다릅니다 — code↔spec 갭을 우선 표시합니다.

> 프로토콜 전체는 `spec-workflow` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/gap-detect "docs/auth-spec.md"
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 분석할 스펙 마크다운 경로 |

## 작업 프로세스

### Step 1: 입력 파싱

spec 파일 경로 추출 + Glob 존재 확인. 미존재 시 즉시 에러.

### Step 2: 관련 코드 경로 결정

frontmatter `related_paths` 추출. 비었으면 본문 식별자/경로를 Grep 으로 추출해 후보 추정 (사용자 confirm). 해결 실패 시 명시적 경로 입력 요청.

### Step 3~6: L1 → L2 → audit 오케스트레이션

`spec-workflow` skill 을 로드하여 수행합니다 (단일 spec):

- **L1 관찰 + 인용 검증 + 피드백 루프**: `references/file-observation.md`
- **L2 종합 + gap-auditor 단일 게이트 루프**: `references/gap-audit-loop.md`

단일 spec 이므로 spec↔spec gaps 섹션은 보통 비고, code↔spec gaps 가 핵심 출력입니다.

### Step 7: 최종 리포트 출력

`spec-workflow` `references/report-format.md` 의 **gap-detect 출력** 형식(Code↔Spec Gaps 우선, 부속 섹션은 발견 시에만)으로 출력합니다.

## 주의사항

- **`/spec-review` 와 백본 동일**, 출력 emphasis 만 다름. 다중 spec 분석은 `/spec-review` 권장.
- **frontmatter `related_paths` 권장** — 자율 보강 노이즈를 줄임.
- **인용 검증 silent fail 금지** — 모든 drop 은 사용자에게 노출.

## 에러 처리

- **spec 파일 미존재**: Step 1 에서 즉시 에러
- **code 경로 미해결**: 사용자에게 명시적 경로 입력 요청
- **L1 50% drop (3회 재시도 후)**: 사용자 confirm — 진행 또는 중단
- **L2 finding 0개**: "갭 없음" 메시지와 함께 정상 종료

상세 프로토콜·종료 조건·Output Examples 는 `spec-workflow` skill 의 references 참조.

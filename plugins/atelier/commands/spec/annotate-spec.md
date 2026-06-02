---
description: 외부 spec 파일에 frontmatter related_paths 추정값을 1차 분석 후 삽입합니다
argument-hint: "<스펙파일>"
allowed-tools: ["Task", "Read", "Edit", "Glob", "Grep", "AskUserQuestion"]
---

# Annotate Spec (/annotate-spec)

외부에서 받은 spec 파일은 보통 frontmatter `related_paths` 가 비어 있어 `/atelier:spec-review` 와 `/atelier:gap-detect` 가 자율 보강 fallback 에 의존하게 됩니다. 이 커맨드는 spec 본문을 1차 분석해 코드 경로 후보를 추정하고, 사용자 confirm 후 frontmatter 에 write back 합니다 — `/atelier:design` 의 frontmatter 권고를 design 단계가 없는 외부 spec 에도 적용하는 효과입니다.

> 신뢰도별 confirm 흐름·frontmatter 갱신 모드·결과 포맷·원칙은 `spec-workflow` skill 의 `references/annotation.md` 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
/annotate-spec "docs/external-auth-spec.md"
/annotate-spec "spec/v5.1/proxy.md"
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 분석 대상 spec 마크다운 경로 |

## 작업 프로세스

1. **입력 파싱 + 존재 확인**: 경로 추출 + Glob 확인. 미존재 시 즉시 에러 `Error: spec 파일을 찾을 수 없습니다: {경로}`.
2. **frontmatter 모드 판정 + spec-annotator 호출 + 신뢰도별 confirm + frontmatter 갱신 + 결과 보고**: `spec-workflow` `references/annotation.md` 절차를 따른다.

## 에러 처리

- **spec 파일 미존재**: 즉시 에러
- **frontmatter 파싱 오류 (잘못된 YAML)**: 사용자에게 알리고 수동 수정 요청 후 종료 (임의 수정 금지)
- **spec-annotator 호출 실패**: 1회 retry, 그래도 실패 시 사용자에게 알리고 종료
- **후보 0개**: "매칭된 코드 경로 후보 없음" 안내 후 종료

상세 절차·신뢰도 정책·Output 형식은 `spec-workflow` skill 의 `references/annotation.md` 참조.

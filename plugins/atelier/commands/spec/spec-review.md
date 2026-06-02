---
description: 스펙 문서와 관련 코드를 대조하여 spec↔code 갭, spec↔spec 갭, 모호함을 통합 분석합니다
argument-hint: "<스펙파일 [스펙파일2 ...]>"
allowed-tools: ["Task", "Glob", "Read", "Grep", "AskUserQuestion"]
---

# Spec Review (/spec-review)

스펙 문서의 완성도를 **실제 코드와의 대조**로 검증합니다. per-file 관찰자(L1)가 사실을 나열하고, 종합 분석기(L2)가 cross-file 패턴을 찾고, 감사기(audit)가 인용·의미를 검증합니다. 모든 결론은 `file:line` 인용으로 추적 가능합니다.

> 프로토콜 전체는 `spec-workflow` skill 에 있습니다. 이 커맨드는 진입점(인자 파싱 + 오케스트레이션 호출)만 담습니다.

## 사용법

```bash
/spec-review "docs/auth-spec.md"                       # 단일 파일
/spec-review "docs/api-spec.md" "docs/data-model.md"   # 다중 파일 (명시적)
/spec-review "spec/v5.1/"                               # 디렉터리 → 파일 목록 확인 후 진행
```

| 인자 | 필수 | 설명 |
|------|------|------|
| 스펙파일 | Yes | 하나 이상의 스펙 마크다운 경로 또는 디렉터리 |

## 작업 프로세스

### Step 1: 입력 파싱 및 파일 확정

- **명시적 파일 경로**: Glob 으로 존재 확인. 미존재 시 즉시 에러 `Error: 스펙 파일을 찾을 수 없습니다: [경로]`
- **디렉터리/Glob 패턴**: 매칭 `.md` 목록을 수집한 뒤 AskUserQuestion 으로 대상 확정 (제외 파일 선택)
- **인자 없음**: AskUserQuestion 으로 경로 요청

### Step 2: 각 spec 파일의 `related_paths` 결정

각 spec 파일에 대해:
1. frontmatter 의 `related_paths` 추출
2. 비었으면 본문에서 식별자/경로를 Grep 으로 추출 → 디렉터리 구조와 매칭해 후보 추정 (사용자 confirm)
3. 각 spec 별 `(spec_path, [related_paths])` 페어 확정

### Step 3~6: L1 → L2 → audit 오케스트레이션

`spec-workflow` skill 을 로드하여 다음을 수행합니다 (다중 spec 이므로 L1 은 병렬 spawn):

- **L1 관찰 + 인용 검증 + 피드백 루프**: `references/file-observation.md`
- **L2 종합 + gap-auditor 단일 게이트 루프**: `references/gap-audit-loop.md`

각 단계의 측정값(호출 횟수, wall-clock)을 누적해 Step 7 footer 에 채웁니다.

### Step 7: 최종 리포트 출력

`spec-workflow` `references/report-format.md` 의 **spec-review 출력** 형식으로 결과 + 검증 통계를 출력합니다.

## 주의사항

- **MainAgent 는 spec/code 파일을 분석하지 않음** — 분석은 sub-agent, 인용 검증 시 Read 도구만 사용
- **인용 검증 silent fail 금지** — 모든 drop 은 사용자에게 노출
- **frontmatter `related_paths` 권장** — 자율 보강은 fallback
- **출력은 마크다운만**

## 에러 처리

- **spec 파일 미존재**: Step 1 에서 즉시 에러
- **L1 모두 50% 이상 drop (3회 재시도 후)**: 사용자 confirm 으로 일부 제외 후 진행 또는 중단
- **gap-auditor 호출 실패**: 1회 retry, 2회째 실패 시 audit skip + "감사 단계 미수행" 알림 후 L2 raw output 출력
- **gap-auditor 무한 major (3회/진전 없음)**: 잔여 major 를 drop log 로 노출 후 정상 종료
- **gh / Glob 에러**: 표준 에러 메시지 출력 후 종료

상세 프로토콜·종료 조건·Output Examples 는 `spec-workflow` skill 의 references 참조.

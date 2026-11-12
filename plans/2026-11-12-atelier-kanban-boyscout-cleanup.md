# 칸반 대리 답변 PR 의 규칙 위반 정리 (보이스카웃)

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-12 · 브랜치: claude/magical-lovelace-bnfj4j (PR kys0213/kys-claude-plugin#966) · 승인 출처: 사용자 승인 (2026-11-12 "지금 pr 에 작업해줘" — 제시한 정리 계획 1~3 단계를 같은 PR 에서 진행)
> 선행 기록: `plans/2026-11-11-atelier-kanban-delegate-run.md`

## 배경

PR #966 이 편집한 파일 80개(plan 제외)를 파일 전체 기준으로 규칙과 대조한 감사에서 약 75건의 위반이 나왔다.

| | 이번 PR 이 만든 것 | 원래 있던 것 |
|---|---|---|
| 범위 안 | 약 39 | 약 13 |
| 범위 밖 | 12 | 약 11 |

## 원인 (자율 런의 피드백 루프)

1. 분해 단계에서 편집 대상 파일마다 위반을 조사해 선행 정리 task 를 만드는 절차(orchestrator 자율 루프 3단계, `plugins/atelier/spec/concerns/boy-scout.md` 적용 지점 '분해')를 건너뛰었다.
2. 게이트 공통 계약이 모든 관점에 "minor 만이면 pass" 를 적용했다. 보이스카웃 spec 의 규칙 준수 관점은 범위 밖을 뺀 모든 위반을 reject 한다. 이 차이로 규칙 위반이 task 마다 minor 로 통과해 쌓였다.
3. 파일 전체를 읽지 못했다고 밝힌 규칙 준수 게이트의 pass 를 그대로 받았다.

## 결정

- 정리는 같은 PR 브랜치에서 한다.
- 게이트 계약 v2: 규칙 준수 관점은 범위 안 위반이 하나라도 있으면 reject. 편집 파일을 다 읽지 못하면 `incomplete` 로 판정을 보류한다.
- 이번 PR 이 만든 범위 밖 위반도 고친다. 원래 있던 범위 밖 위반은 PR 설명에 보고만 한다.
  - 대상: 모드 안 판정, skill 의 장부 집계, `Setting` 문자열 필드, 깨우기의 대기 종류 분기 중복.
- 모드 안 판정을 CLI 로 옮긴다.
  - Bash 가드의 `kanban` 낱말 사전 판정은 지운다. 모든 Bash 호출을 CLI 가 판정하고, 실패하면 통과시킨다.
  - inbox 중지의 턴 비교, 위임의 delivered 중복 제거·토스트 1회·맥락 첨부 출처 판정, nonce 보존 상한, relay 질문 수 비교도 CLI 로 옮긴다.
  - 모듈 테스트는 어댑터 계약만 남긴다.
- skill 이 세던 보고 수치(물은 건·대리·relay·추천 승인율·직접 답)는 CLI 가 집계해 준다.
- spec 에서 구현 세부(CLI 플래그·모듈 분담·내부 표지)를 걷어 동작 문장으로 바꾼다.

## 순서

1. 병렬 (파일 집합이 겹치지 않음)
   - Rust core 첫 묶음 정리와 `Setting` enum
   - Rust core 둘째 묶음 정리와 깨우기 분기 중복
   - commands·git·shared 정리와 생성자 주입
   - Rust 테스트 하네스 통합
2. 모드 판정을 CLI 로 옮김 — 1 뒤
3. 보고 집계를 CLI 로 옮기고 spec·skill·README 정리 — 2 뒤
4. 통합 검증 후 push, PR 설명에 원래 있던 범위 밖 위반을 보고

## 남겨 두는 것 (원래 있던 범위 밖 위반 — 보고만)

- `Request::permit(&str)` (`Verdict` enum 존재, 호출부 변경 필요)
- git `analyzer.rs` 의 `pub(super)` 상태 필드 직접 대입
- `kanban_card_detail.rs` 의 `only` 헬퍼가 편집하지 않은 `kanban_background.rs` 와 중복
- 모드의 기존 절단·상한 상수와 기록 갱신 정책, 그것을 재검증하는 기존 모듈 테스트

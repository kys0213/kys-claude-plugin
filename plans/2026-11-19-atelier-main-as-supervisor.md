# 메인 에이전트를 공장장으로 두고 실작업은 크기와 무관하게 작업자에게 맡긴다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-19 · 브랜치: claude/wizardly-carson-a62l3m · 이슈: 없음 · 승인 출처: 사용자 승인

## 배경

사용자 관측: 오케스트레이터 모드에서 메인 에이전트가 감독하지 않고 직접 일하려 한다.

원인은 2026-11-12 결정(`plans/2026-11-12-atelier-approval-and-delegation-defaults.md` §B)이에요. 그 결정은 위임 비용을 줄이려고 위임 기준을 "규모"로 좁혔어요.

- orchestrator `SKILL.md` description 이 "몇 번의 도구 호출로 닫히는 단일 단위 작업은 이 스킬 없이 메인이 직접 한다"고 말해요. 스킬은 description 으로만 로드되므로, 작은 편집에서는 스킬이 아예 열리지 않아요.
- 템플릿 CLAUDE.md 가 "위임은 비용이다, 클 때만 위임한다"고 말해요.
- Claude Code 기본 시스템 프롬프트도 "확신이 없으면 sub-agent 를 띄우지 말고 직접 하라"는 방향이에요. 위 두 문구가 이 기본 지침과 같은 쪽을 가리켜서, 메인은 대부분의 일을 직접 해요.

## 요구사항 (사용자 결정)

메인 에이전트는 소프트웨어 팩토리의 공장장이다.

1. 전체 생산 라인이 잘 돌아가는지 감독한다.
2. 작업자가 실수하면 개입해 피드백한다.
3. 직접 일하지 않는다.

## 사이드이펙트 조사

- 위임 기준을 "규모"로 적은 곳(`grep -rn "직접 하\|메인이 직접\|main agent without" plugins/atelier`):
  - `templates/claude-md/CLAUDE.md` §작업 흐름 orchestrator bullet — 모든 세션에 로드되는 `~/.claude/CLAUDE.md` 블록이라 가장 강한 신호예요.
  - `skills/orchestrator/SKILL.md` description · 첫 문단.
  - `skills/communicate/SKILL.md`, `skills/spec-write/SKILL.md`(3곳), `skills/spec-write/references/authoring.md` — "규모에 따라 메인이 직접 Write".
  - `skills/grill/SKILL.md` 핸드오프 — "여러 갈래로 나뉘면 orchestrator 로".
- 그대로 두는 곳:
  - orchestrator 불변식 1(메인은 tracked 파일을 직접 편집하지 않음)·11(red 를 메인이 고치지 않음)·15(reject 는 findings 와 함께 재위임) — 이미 공장장 모델이에요. 스킬이 열리기만 하면 돼요.
  - orchestrator 표 3 "1턴에 닫히는 단발 조회는 메인이 답한다", 절차 1 "단순 요청은 메인이 직접 분해" — 감독 업무(조회·분해)라 요구사항과 맞아요.
  - communicate "채팅 답변은 위임하지 않는다" — 산출물 파일이 없어요.
  - 루트 `CLAUDE.md` — atelier 를 설치하지 않은 기여자도 읽는 저장소 규칙이고, 위임 언급이 없어 충돌하지 않아요.
- `scripts/check-decision-graph.sh` 줄 수 상한: orchestrator `SKILL.md` 는 description 과 첫 문단 두 줄만 바꿔 181줄을 유지해요.
- description 은 YAML plain scalar 라 `: `(콜론+공백)를 넣으면 깨져요 (#967 의 yaml 수정 커밋과 같은 함정).
- 템플릿이 바뀌면 설치 사용자의 drift 가 `DRIFTED` 가 되고 `/atelier:update` 로 맞춰져요. 의도된 동작이에요.
- 테스트·스크립트가 바뀌는 문구를 검사하지 않아요 (`grep` 으로 확인).

## 결정과 근거

1. **위임 기준을 "규모"에서 "산출물 유무"로 바꾼다.** tracked 편집 · 문서 산출물 · 여러 파일을 읽는 조사는 크기와 무관하게 작업자에게 맡겨요. 메인은 감독 업무(분해 · dispatch · 사실 확인 · 검수 · 피드백 · 통합 · 채팅 답변)만 직접 해요. 사용자가 직접 수행을 지시하면 그 지시가 앞서요.
2. **템플릿이 기본 지침보다 앞선다고 명시한다.** Claude Code 기본 지침의 "작은 일은 직접"은 이 세션에서 적용하지 않는다고 적어요. 명시하지 않으면 기본 지침과 스킬 description 이 경쟁하고, 기본 지침이 이겨요.
3. **작업자가 틀리면 메인이 고치지 않고 피드백한다.** `요구사항 ↔ 파일:라인` 으로 지목해 다시 맡겨요. 불변식 15 의 재위임 규칙을 템플릿 수준의 역할 정의로 올린 거예요.
4. **위임 비용은 tier · effort 배정으로 줄인다.** #967 이 지적한 비용 문제는 남아 있어요. 작은 일은 경량 tier · 낮은 effort 작업자에게 맡겨서 줄여요. 메인이 직접 일하는 것은 비용 절감 수단으로 쓰지 않아요.

## 버린 대안

| 대안 | 이유 |
|---|---|
| #967 이전 문구로 그대로 되돌리기 | "한 턴 단일 파일 편집은 예외"가 남아 있어서 메인이 직접 일하는 출구가 돼요. 공장장 역할도 명시돼 있지 않았어요 |
| 작은 편집은 메인 직접 수행 예외로 유지 | 사용자 요구("직접 일하지 않는다")와 어긋나요. 예외 경계가 모호하면 모델이 기본 지침 쪽으로 기울어요 |
| hook 으로 메인의 Edit/Write 를 차단 | plan 기록(불변식 1 예외)·통합 복구 같은 정당한 메인 편집을 함께 막아요. 판단을 결정적 도구에 묻는 것이라 책임 경계에도 어긋나요 |
| 루트 `CLAUDE.md` 에도 같은 절 추가 | 루트 규칙은 atelier 미설치 기여자도 읽어요. atelier 설치 사용자는 `~/.claude/CLAUDE.md` 의 템플릿 블록으로 받아요 |

## 작업 순서

1. 이 plan 을 `docs` 커밋으로 남겨요.
2. 템플릿 · orchestrator · communicate · spec-write · grill 문구를 한 커밋으로 고쳐요.
3. `bash scripts/check-decision-graph.sh` 와 `make validate-ci` 로 확인해요.

## PR 타이틀

- `feat(atelier): make main agent supervise and delegate all work`
- type 근거: `plugins/` 변경이고 사용자가 받는 작업 흐름이 바뀌는 동작 변경이라 `feat` 예요.

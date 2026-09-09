# plan 기록 위치를 repo 안 `plans/` 로 고정

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-09-09 · 브랜치: claude/adoring-johnson-looqd4 · 이슈: 없음 · 승인 출처: 사용자 승인

## 요구사항

- atelier 흐름에서 만들어지는 구현 계획(plan)이 변경 이력의 일부로 repo 에 남아야 한다.
- 날짜별로 언제의 기록인지 한눈에 보여야 한다.

## 현재 상태 (사이드이펙트 조사)

- `grill` 은 구현 전환 단계에서 Claude Code Plan Mode 로 넘기고, "설계는 Plan Mode 의 plan 파일이 담는다"고만 적혀 있었다. Plan Mode 는 기본적으로 `~/.claude/plans/` 에 임의 이름으로 파일을 쓴다 — repo 밖이라 커밋되지 않고 브랜치·이슈와 연결되지 않는다.
- `rules/policies/plan-writing.md` 는 이미 `plans/**/*.md` 를 대상으로 헤더에 날짜·브랜치·이슈를 남기라고 했지만, 그 경로로 파일이 가는 흐름이 없었다 (정책과 흐름의 단절).
- Claude Code 의 `plansDirectory` 설정으로 Plan Mode 저장 위치는 바꿀 수 있으나 파일명은 도구가 정하므로 날짜 규약은 설정만으로 안 된다.
- orchestrator 의 설계 승인 마커는 `<log_dir>/design-approval.md`(repo 밖) 에 남고, 근거 경로 필드가 있다.

## 검토한 대안

| 대안 | 판단 |
|---|---|
| `plansDirectory` 를 프로젝트 settings.json 에 넣어 Plan Mode 가 직접 `plans/` 에 쓰게 한다 | 보조 수단으로만. 파일명 규약이 안 잡히고, settings.json 편집은 규칙상 CLI 를 거쳐야 해서 setup 에 서브커맨드가 필요 — 범위가 커진다. 이번엔 버림 |
| `plans/YYYY/MM/DD-slug.md` 계층 구조 | 파일 수가 적을 때 탐색만 늘린다. 버림 |
| `plans/YYYY-MM-DD-<slug>.md` 평면 구조 + 헤더 규약 | **채택**. 기존 `plans/atelier/00~11` 번호 시리즈(epic 단위)와 병존 |

## 결정

1. **규약은 rules 에**: `plan-writing.md` 에 저장 경로 `plans/YYYY-MM-DD-<slug>.md`, 헤더(날짜·브랜치·이슈·승인 출처), 구현과 분리된 `docs` 커밋을 명시한다. 복사형 산출물이라 `drift check/sync` 로 설치된 사용자에게 전파된다.
2. **흐름은 skill 에**: `grill` 구현 전환 단계에서 Plan Mode/orchestrator 어느 쪽이든 승인된 계획을 위 경로로 저장·커밋하는 것까지를 핸드오프로 정의한다.
3. **마커는 그대로**: 설계 승인 마커는 repo 밖 이벤트 로그로 유지하고, 근거 경로가 커밋된 plan 파일을 가리키게 한다.

## 작업 순서

1. `plan-writing.md` 규약 추가
2. `grill/SKILL.md`, `grill/references/design-generation.md` 구현 전환 문구 갱신
3. `orchestrator/references/contracts.md` 근거 경로 예시 갱신
4. 이 문서를 첫 번째 날짜 plan 으로 커밋 (규약 실적용)

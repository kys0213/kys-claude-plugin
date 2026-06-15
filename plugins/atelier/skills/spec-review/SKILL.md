---
name: spec-review
description: 작성된 스펙 문서를 코드와 대조 분석하고 품질을 평가하는 스킬. "스펙 리뷰해줘", "spec↔code 갭 봐줘", "이 spec 들 검증", "스펙 품질 평가", "외부 spec 에 related_paths 주석" 같은 요청에 사용합니다. 슬래시로 직접 호출하거나 맥락에서 모델이 자동 호출합니다. 스펙 문서 작성은 `spec-write`, 설계 대화는 `brainstorm`(무에서 설계)·`grill`(기존 계획 심문) 스킬이 담당합니다. L1(관찰)→L2(종합)→audit(감사) 레이어로 file:line 인용 기반 분석.
version: 1.0.0
---

# spec-review

작성된 스펙 문서를 **코드와 대조 분석·평가·주석**하는 스킬입니다. 사용자가 spec-review 슬래시로 진입하거나 모델이 맥락에서 자동 호출하며, 의도에 따라 아래 `references/` 로 디스패치합니다.

> 스펙 문서를 **작성**하려면 `spec-write`, 설계를 **대화로 합의**하려면 `brainstorm`(무에서 설계)·`grill`(기존 계획 심문)을 씁니다. spec-review 는 이미 존재하는 스펙을 분석하는 단계입니다.

## 진입 라우팅 (의도 → reference)

spec-review 슬래시 또는 모델 자동 호출로 진입하면, 사용자의 자연어 의도를 분류해 해당 흐름을 수행합니다.

| 사용자 의도 (예) | 흐름 | 로드할 references |
|---|---|---|
| "스펙 리뷰", "이 spec 들 검증", 다중 spec 대조 | spec-review (다중 spec, L1 병렬) | file-observation → gap-audit-loop → report-format(spec-review) |
| "갭 봐줘", "spec 과 code 차이", 단일 spec | gap-detect (단일 spec, code↔spec 우선) | file-observation → gap-audit-loop → report-format(gap-detect) |
| "스펙 품질 평가", "이 스펙 등급" | quality (4관점 등급 산정) | quality-criteria |
| "related_paths 채워줘", 외부 spec 주석 | annotate | annotation |

입력 인자(spec 파일 경로 등)가 함께 오면 그대로 사용하고, 없으면 AskUserQuestion 으로 확인합니다. 결정적 동작은 없으며(전부 판단/분석), 모든 분석은 sub-agent 에 위임합니다.

스펙 문서를 다루는 워크플로우의 프로토콜·종료 조건·출력 포맷은 이 skill 의 `references/` 에서 progressive disclosure 로 로드합니다.

## 레이어 모델 (L1 → L2 → audit)

스펙 분석은 3개 레이어로 구성됩니다. 각 레이어는 독립 sub-agent 이며, 메인 에이전트는 오케스트레이션(인용 검증 + 피드백 루프)만 합니다.

| 레이어 | 에이전트 | 모델 | 역할 |
|---|---|---|---|
| **L1 관찰** | `file-pair-observer` | haiku | spec 1개 + 관련 code 를 읽고 사실을 `file:line` 인용으로 나열 (per-file 리포트) |
| **L2 종합** | `gap-aggregator` | sonnet | 검증 통과한 L1 리포트들을 cross-file 로 종합 → gap finding |
| **audit 감사** | `gap-auditor` | sonnet | L2 finding 의 인용 정확성(M-0) + 의미 적합성(M-1~M-6) 단일 게이트 감사 |

## 인용 검증 철학 (silent fail 금지)

모든 결론은 `file:line` 인용으로 추적 가능해야 하며, **검증 실패는 절대 조용히 버리지 않습니다**. 메인 에이전트는:

1. 각 레이어 출력의 인용을 실제 파일과 대조 검증
2. 실패 항목은 **targeted 피드백**으로 해당 에이전트에 수정 요청 (전체 재실행 아님)
3. 진전 없거나 한도 도달 시 종료, **모든 drop 을 사용자에게 노출**

상세 프로토콜은 아래 references 참조.

## references 로드 가이드

| reference | 언제 로드 | 내용 |
|---|---|---|
| `references/file-observation.md` | L1 spawn + 인용 검증 + 피드백 루프 수행 시 | file-pair-observer 입력 프롬프트, 인용 검증 절차, 피드백 루프 알고리즘/종료 조건, drop 로그 |
| `references/gap-audit-loop.md` | L2 종합 + audit 감사 수행 시 | gap-aggregator 입력, gap-auditor 단일 게이트, audit 루프 정책, fix request 형식, 실패 모드 |
| `references/report-format.md` | 최종 리포트 출력 시 | spec-review / gap-detect 출력 구조, 검증 통계 footer, Output Examples |
| `references/annotation.md` | 외부 spec frontmatter 주석(`annotate-spec`) 시 | spec-annotator 호출, 신뢰도별 confirm, frontmatter 갱신 모드 |
| `references/quality-criteria.md` | 스펙 품질 평가·등급 산정 시 (autopilot Spec Quality Gate 포함) | Big Picture/Detail/Verification/Consistency 4관점 체크리스트, 점수·등급 기준 |
| `references/issue-report.md` | 분석 결과를 심각도 기반 이슈 리포트로 보고할 때 | severity 기준, 이슈 항목 구조, 마크다운 출력 형식 |

## 공통 원칙

- **MainAgent 는 spec/code 파일을 분석하지 않음** — 분석은 sub-agent, 메인은 인용 검증 시 Read 도구만 사용.
- **frontmatter `related_paths` 권장** — 자율 보강은 fallback. 정확한 코드 영역은 명시가 최선.
- **재시도는 같은 spec 으로만** — 한 spec 의 drop 비율이 높아도 다른 spec 에 영향 주지 않음.
- **출력은 마크다운만** — JSON 출력 금지.

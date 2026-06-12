---
name: workflow
description: 에이전트 워크플로우·컨벤션 설계의 단일 진입점. "컨벤션 잡아줘", ".claude/rules 만들어줘/점검해줘", "CLAUDE.md 정리", "에이전트 설계 원칙 룰 설치", "기존 command/agent/skill 리뷰" 같은 요청에 사용합니다. 슬래시로 직접 호출하거나 맥락에서 모델이 자동 호출합니다. 코드베이스 분석 기반 .claude/rules 구조 설계 지식 포함.
version: 1.0.0
---

# workflow

프로젝트의 에이전트 워크플로우와 컨벤션(`.claude/rules/`, CLAUDE.md)을 다루는 **관심사 단위 진입점이자 공통 도메인 지식**입니다. 사용자가 workflow 슬래시로 진입하거나 모델이 맥락에서 자동 호출하며, 의도에 따라 아래 `references/` 로 디스패치합니다.

## 진입 라우팅 (의도 → 흐름)

| 사용자 의도 (예) | 흐름 | 로드할 references / 동작 |
|---|---|---|
| "컨벤션 잡아줘", ".claude/rules 만들어줘", "규칙 scaffold" | scaffold (6단계 HITL) | scaffold-protocol → codebase-detection·rules-design·value-interview |
| "rules gap 점검", "규칙 누락 봐줘" | scaffold (gap-only) | scaffold-protocol Step 1 에 gap-only 지시 추가 |
| "에이전트 설계 원칙 룰 설치해줘" | rule 설치 | 아래 §"설계 원칙 룰 설치" |
| "기존 command/agent/skill 설계 리뷰" | workflow 리뷰 | workflow-reviewer 에이전트 위임 (agent-design-principles 기준) |

입력 인자(`--gap-only`, `--force` 등 의도)가 함께 오면 그대로 적용하고, 모호하면 AskUserQuestion 으로 확인합니다.

## 설계 원칙 룰 설치

플러그인 내부의 룰 원본을 프로젝트에 복사합니다:

1. `.claude/rules/` 디렉토리가 없으면 생성. 대상 파일이 이미 존재하면 덮어쓸지 확인 (`--force` 의도 시 즉시 덮어쓰기)
2. `${CLAUDE_PLUGIN_ROOT}/rules/agent-design-principles.md` 를 Read 하여 **내용 수정 없이 그대로** `.claude/rules/agent-design-principles.md` 에 Write
3. 기존 `.claude/{commands,agents,skills}/**/*.md` 가 감지되면 workflow-reviewer 로 설계 원칙 준수 리뷰를 제안

## references 로드 가이드

| reference | 언제 로드 | 내용 |
|---|---|---|
| `references/scaffold-protocol.md` | 컨벤션 scaffold 수행 시 | 6단계 HITL 워크플로우 (병렬 분석 → 인터뷰 → CLAUDE.md → 규칙 생성) |
| `references/codebase-detection.md` | 코드베이스 분석 시 (codebase-analyzer) | 언어/프레임워크/구조 감지 시그널, LSP-Enhanced Analysis |
| `references/rules-design.md` | 규칙 구조 제안·생성·검증 시 | 레이어 매핑, paths 전략, 규칙 템플릿, 다중 언어, paths 범용화 원칙 |
| `references/value-interview.md` | 가치관 인터뷰·CLAUDE.md 배치 결정 시 | 인터뷰 카테고리, CLAUDE.md vs rules 배치 기준 |

## 공통 원칙

- **분석은 sub-agent 에 위임**: codebase-analyzer / document-analyzer / rules-generator / workflow-reviewer. 메인 에이전트는 인터뷰·승인(HITL)과 결과 취합만 합니다.
- **HITL 필수**: CLAUDE.md 와 `.claude/rules/` 변경은 반드시 사용자 승인 후 수행합니다.
- **paths 는 레이어를 표현**: 위치(컨테이너)가 아닌 역할(레이어) 기준 — `references/rules-design.md` §paths 범용화 원칙.
- 코딩 원칙 자체는 `coding-style` skill, 에이전트 레이어링 교리는 `agent-design-principles` skill 이 단일 출처입니다.

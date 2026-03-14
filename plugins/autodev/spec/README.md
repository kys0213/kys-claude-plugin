# SPEC v4: Claw Layer

> **Date**: 2026-03-14
> **구조**: 각 항목은 `flow.md`(사용자 플로우)로 구성, 전체 설계는 `DESIGN.md` 참조

## 개요

autodev v4는 **두 가지 모드**로 동작한다:

- **Issue 모드**: 사람이 이슈를 등록하면 autodev가 처리 (v3 호환)
- **Spec 모드**: 사람이 디자인 스펙을 등록하면 Claw가 이슈 분해 → 구현 → 검증을 스펙 충족까지 반복

두 모드 모두 동일한 파이프라인(Analyze → Implement → Review → Improve)을 사용하며,
차이는 **이슈의 출처**(사람 vs Claw)와 **완료 기준**(개별 이슈 vs 스펙 전체).

## 용어 정의

| 용어 | 정의 |
|------|------|
| **Spec (스펙)** | "무엇을 만들 것인가"의 전체 설계 문서. 여러 이슈의 원천. 완료 = 모든 이슈 done + gap 없음 + acceptance criteria 통과 |
| **Issue (이슈)** | 하나의 작업 단위. Claw 또는 사람이 등록. 완료 = PR merged |
| **Claw** | claw-workspace에서 실행되는 Claude Code 세션. `autodev agent`로 실행. 대화 + 판단 + 중재를 담당 |
| **HITL** | Human-in-the-Loop. 사람의 판단이 필요한 시점 |
| **Closed Loop** | 구현 → 테스트 → 피드백 → 수정이 자동으로 반복되는 구조 |
| **Gap** | 스펙에 정의된 요구사항/아키텍처와 실제 구현 사이의 구조적 불일치. Claw가 코드와 스펙을 대조하여 탐지하며, 발견 시 자동으로 이슈를 생성한다 |
| **Drain** | 큐에서 아이템을 꺼내 다음 phase로 전이하는 동작. v3에서는 기계적(슬롯 기반), v4에서는 Claw가 판단 |

## 스펙 목록

| # | 항목 | 설명 | flow |
|---|------|------|------|
| 1 | [레포 등록](./01-repo-registration/) | autodev로 관리할 레포 등록 | ✅ |
| 2 | [이슈 등록](./02-issue-registration/) | Issue 모드 (v3 호환 + Claw 확장) | ✅ |
| 3 | [스펙 등록](./03-spec-registration/) | Spec 모드, 필수 섹션 검증, /add-spec | ✅ |
| 4 | [다중 스펙 우선순위](./04-spec-priority/) | 스펙 간 우선순위/충돌 판단 | ✅ |
| 5 | [HITL 알림](./05-hitl-notification/) | 알림 채널 OCP (Notifier trait) | ✅ |
| 6 | [칸반 보드](./06-kanban-board/) | TUI 칸반, BoardRenderer trait | ✅ |
| 7 | [피드백 루프](./07-feedback-loop/) | /update-spec 대화형 수정 | ✅ |
| 8 | [스펙 완료 판정](./08-spec-completion/) | 완료 조건 + HITL 최종 확인 | ✅ |
| 9 | [실패 복구](./09-failure-recovery/) | 유형별 대응 + 에스컬레이션 | ✅ |
| 10 | [Claw 워크스페이스](./10-claw-workspace/) | 자연어 기반 Claw 행동 설정 | ✅ |
| 11 | [컨벤션 부트스트랩](./11-convention-bootstrap/) | 기술 스택 기반 규칙 제안 + 자율 개선 | ✅ |
| 12 | [인터페이스 레퍼런스](./12-cli-reference/) | Plugin commands (SSOT) + autodev CLI | ✅ |
| 13 | [Cron 관리](./13-cron/) | 스크립트 기반 cron, 환경변수 주입, 유효성 검증 | ✅ |

## 메인 에이전트 모델

### Claw = Claude Code in claw-workspace

Claw는 별도 스케줄러가 아니라 **claw-workspace에서 실행되는 Claude Code 세션** 그 자체다.
claw-workspace의 CLAUDE.md, rules, skills가 Claw의 판단 원칙이 된다.

```bash
# 이 두 개가 동일
cd ~/.autodev/claw-workspace && claude
autodev agent
```

```
사용자 ↔ Claude Code (claw-workspace)  ← 이것이 Claw
              │
              │  CLAUDE.md  → 판단 원칙
              │  rules/     → 브랜치, 리뷰, 스케줄링 정책
              │  skills/    → 스펙 분해, gap 탐지
              │
              ├─→ autodev daemon (백그라운드, 틱 기반)
              │     ├─→ Collector (GitHub 이슈/PR 수집)
              │     ├─→ TaskQueue (통합 큐)
              │     └─→ Task Pipeline (Analyze → Implement → Review → Improve)
              │
              ├─→ Notifier (HITL 알림, OCP)
              ├─→ BoardRenderer (칸반 보드, OCP)
              └─→ Convention Engine (규칙/스킬 자동 정제)
```

### 사용 패턴

```
터미널 1: autodev dashboard        ← TUI 칸반 (모니터링)
터미널 2: autodev agent             ← Claw (대화 + 판단 + 중재)
          백그라운드: autodev daemon ← 수집 + Task 실행
```

- **대시보드** = 관찰 (전체/레포별 상태 확인)
- **Claw (autodev agent)** = 판단 + 중재 (스펙 등록, HITL 응답, 피드백, 방향 전환)
- **데몬** = 실행 (수집 → 큐 → Task 실행)

### 왜 Claude Code 자체가 Claw인가

```
별도 스케줄러 방식:
  daemon이 프롬프트를 빌드 → LLM 호출 → JSON 파싱 → Decision 적용
  → 프롬프트 엔지니어링 필요, 구조화된 출력 파싱 필요, 디버깅 어려움

Claude Code 방식:
  claw-workspace에서 claude 실행 → CLAUDE.md/rules/skills 자동 로드
  → 네이티브 기능 그대로 활용 (도구 호출, 파일 읽기, 대화)
  → rules 수정 = 동작 변경 (코드 변경 없음)
  → 사용자가 직접 대화하며 중재 가능
```

## 플로우 간 관계

```
Flow 10: Claw 워크스페이스 설정 (판단 원칙, 전략, 스킬)
    │
    ▼
Flow 1: 레포 등록
    │    │
    │    └──→ Flow 11: 컨벤션 부트스트랩 (.claude/rules/ 없을 때)
    │
    ├──→ Flow 2: 이슈 등록 (Issue 모드)
    │         │
    │         └──→ 기존 파이프라인 (v3)
    │
    └──→ Flow 3: 스펙 등록 (Spec 모드)
              │
              ├──→ Flow 4: 다중 스펙 우선순위
              │
              ├──→ 기존 파이프라인 (이슈 단위)
              │         │                          ↑ Claw의 판단에
              │         ├──→ Flow 9: 실패 시 복구   │ 워크스페이스 규칙이
              │         │                          │ 적용됨
              │         └──→ Flow 5: HITL 알림     │ (브랜치 전략, 리뷰 정책 등)
              │                    │
              │                    └──→ 피드백이 반복 패턴이면
              │                         → Flow 11: 규칙/스킬 자동 정제
              │
              ├──→ Flow 6: 칸반 보드 (전체/레포별 보기)
              │         ↑
              │     모든 플로우의 상태가 여기에 반영됨
              │
              ├──→ Flow 7: 피드백 루프 (수정 요청)
              │         │
              │         ├──→ 이슈 재처리 또는 스펙 업데이트
              │         └──→ 피드백 패턴 감지 → Flow 11: 규칙 정제
              │
              └──→ Flow 8: 스펙 완료 판정
                        │
                        └──→ HITL: 최종 확인
```

## 다음 단계

플로우 문서 + DESIGN.md 확정 완료. 구현 진행:

1. v4 패키지 구조 생성 (`core/`, `infra/`, `daemon/`, `cli/`, `tui/`, `tasks/`)
2. v3 코드를 v4 구조로 이동
3. DESIGN.md Phase 순서로 신규 컴포넌트 구현

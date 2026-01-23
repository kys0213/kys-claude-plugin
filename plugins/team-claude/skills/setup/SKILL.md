---
name: team-claude:setup
description: Team Claude 환경 설정 - 초기화, 설정 관리, 에이전트 관리, 서버 관리
invokable: true
---

# Team Claude Setup

단일 진입점으로 모든 환경 설정을 관리합니다.

## 워크플로우

```
/team-claude:setup
        │
        ▼
.claude/team-claude.yaml 존재?
        │
   ┌────┴────┐
   No        Yes
   │         │
   ▼         ▼
초기화     메인 메뉴
모드       │
           ├── 설정 조회
           ├── 설정 수정
           ├── 에이전트 관리
           ├── 서버 관리
           └── 종료
```

## 실행 절차

### Phase 1: 상태 감지

`.claude/team-claude.yaml` 존재 여부 확인:

- **없음** → [초기화 모드](./init-mode.md) 진입
- **있음** → 메인 메뉴 표시

### Phase 2: 메인 메뉴 (설정 존재 시)

```typescript
AskUserQuestion({
  questions: [{
    question: "무엇을 하시겠습니까?",
    header: "Setup",
    options: [
      { label: "현재 설정 보기", description: "전체 설정 조회" },
      { label: "설정 수정", description: "대화형 위자드로 설정 변경" },
      { label: "에이전트 관리", description: "에이전트 생성/수정/삭제/활성화" },
      { label: "서버 관리", description: "서버 설치/시작/중지" },
      { label: "종료", description: "설정 메뉴 종료" }
    ],
    multiSelect: false
  }]
})
```

선택에 따라 해당 reference 파일 참조:

| 선택 | Reference |
|------|-----------|
| 현재 설정 보기 / 설정 수정 | [config-management.md](./config-management.md) |
| 에이전트 관리 | [agent-management.md](./agent-management.md) |
| 서버 관리 | [server-management.md](./server-management.md) |

## 설정 파일

### 위치

```
.claude/team-claude.yaml
```

### 스키마

```yaml
version: "1.0"

project:
  name: "{project_name}"
  language: "{detected_language}"
  framework: "{detected_framework}"
  domain: "{selected_domain}"
  test_command: "{test_command}"
  build_command: "{build_command}"
  lint_command: "{lint_command}"

feedback_loop:
  mode: auto                  # auto | semi-auto | manual
  max_iterations: 5
  auto_retry_delay: 5000

validation:
  method: test                # test | script | manual
  timeout: 120000

notification:
  method: system              # system | slack | none
  slack:
    webhook_url: ""
    channel: ""

server:
  port: 7890
  executor: iterm             # iterm | terminal-app | headless

agents:
  enabled:
    - spec_validator
    - test_oracle
    - impl_reviewer
  custom:
    - payment-expert
  overrides:
    spec_validator:
      model: opus
```

## 디렉토리 구조

```
.team-claude/
├── sessions/                # 세션 데이터
├── state/                   # 런타임 상태
├── hooks/                   # Hook 스크립트
├── templates/               # 템플릿
└── agents/                  # 커스텀 에이전트
    ├── payment-expert.md
    └── security-auditor.md

.claude/
└── team-claude.yaml         # 메인 설정
```

## Reference Files

- [init-mode.md](./init-mode.md) - 초기화 모드 (프로젝트 분석, 인터뷰)
- [config-management.md](./config-management.md) - 설정 조회/수정
- [agent-management.md](./agent-management.md) - 에이전트 CRUD (HITL)
- [server-management.md](./server-management.md) - 서버 관리

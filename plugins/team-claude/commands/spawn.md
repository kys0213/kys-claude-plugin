---
name: spawn
description: 새로운 Worker Claude를 생성합니다 - Git worktree에서 독립적으로 작업하는 에이전트를 생성합니다
argument-hint: "<feature-name> [task-spec]"
allowed-tools: ["Bash", "Read", "Write", "Glob", "AskUserQuestion"]
---

# Worker Spawn 커맨드

새로운 Worker Claude 인스턴스를 생성합니다. 각 Worker는 독립된 Git worktree에서 할당된 작업을 수행합니다.

## 핵심 워크플로우

```
1. 피처 이름 확인
    │
    ▼
2. Task Spec 확인/생성
    │
    ▼
3. Git Worktree 생성
    │
    ▼
4. Worker 설정 초기화
    │
    ▼
5. Worker 등록 및 시작 안내
```

## 실행 단계

### 1. 피처 이름 확인

사용자가 제공한 피처 이름을 확인합니다. 피처 이름이 없으면 질문합니다.

```
피처 이름 규칙:
- 영문 소문자, 숫자, 하이픈만 사용
- 예: auth, payment-gateway, user-profile
```

### 2. Task Spec 확인

사용자가 task-spec을 제공했는지 확인합니다. 없으면 간단한 설명을 요청합니다.

Task Spec 템플릿:
```markdown
# Task Specification: <feature-name>

## 목표
[이 피처가 달성해야 할 목표]

## 범위
- [ ] [구현해야 할 항목 1]
- [ ] [구현해야 할 항목 2]

## 제약사항
- [기술적 제약사항]
- [시간 제약사항]

## 참고 파일
- [관련 기존 파일 경로]

## 완료 조건
- [테스트 통과]
- [코드 리뷰 준비]
```

### 3. Git Worktree 생성

spawn-worker.sh 스크립트를 실행하여 worktree를 생성합니다.

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/spawn-worker.sh <feature-name> <base-branch> [task-spec-file]
```

### 4. Worker 설정 초기화

생성된 worktree에 다음 파일들이 설정됩니다:
- `.claude/CLAUDE.md` - Worker 동작 가이드
- `.claude/task-spec.md` - 작업 명세
- `.claude/settings.local.json` - Stop hook 설정

### 5. Worker 등록 및 시작 안내

Coordination Server에 Worker를 등록하고, Worker 시작 방법을 안내합니다.

## 사용 예시

```bash
# 기본 사용
/team-claude:spawn auth-feature

# task spec 포함
/team-claude:spawn payment "결제 시스템 구현 - Stripe 연동 포함"

# 상세 task spec 파일과 함께
/team-claude:spawn user-profile ./specs/user-profile-spec.md
```

## 생성되는 구조

```
../worktrees/feature-<name>/
├── .claude/
│   ├── CLAUDE.md           # Worker 가이드
│   ├── task-spec.md        # 작업 명세
│   └── settings.local.json # Hook 설정
├── (프로젝트 파일들)
└── ...
```

## 주의사항

- Coordination Server가 실행 중이어야 Worker 등록이 됩니다
- 같은 이름의 worktree가 있으면 생성 실패
- Worker 시작은 수동으로 새 터미널에서 `claude` 실행 필요

## 서버 상태 확인

Worker 생성 전 서버 상태를 확인하려면:
```bash
curl http://localhost:3847/health
```

서버가 실행 중이 아니면 먼저 시작하세요:
```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/install-server.sh --start
```

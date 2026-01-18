---
name: team-claude:config
description: Team Claude 설정 조회 및 수정 - get, set, list, reset 작업 지원
argument-hint: "<action> [key] [value]"
allowed-tools: ["Read", "Write", "Bash", "AskUserQuestion"]
---

# Team Claude 설정 관리 커맨드

개별 설정 값을 조회하거나 수정합니다.

## 사용법

```bash
# 전체 설정 보기
/team-claude:config list

# 특정 값 조회
/team-claude:config get <key>

# 값 변경
/team-claude:config set <key> <value>

# 섹션 초기화
/team-claude:config reset <section>
```

## Arguments

| Argument | 필수 | 설명 |
|----------|------|------|
| action | O | get, set, list, reset |
| key | △ | 설정 키 (점 표기법) |
| value | △ | 설정 값 (set 시) |

---

## Action: list

전체 설정을 트리 형태로 출력합니다.

### 출력 예시

```
📋 Team Claude 설정

project:
  name: my-project
  domain: ecommerce
  language: TypeScript

server:
  port: 3847
  host: localhost

worktree:
  root: ../worktrees
  branchPrefix: feature/

worker:
  maxConcurrent: 5
  timeout: 1800

terminal:
  type: iterm2
  layout: tabs

notification:
  method: notification

review:
  autoLevel: semi-auto
  agents:
    - code-reviewer
    - qa-agent
    - security-auditor

completion:
  requiredChecks:
    - lint
    - typecheck
    - test
  coverageThreshold: 80
```

---

## Action: get

특정 설정 값을 조회합니다. 점 표기법으로 중첩된 값에 접근합니다.

### 예시

```bash
/team-claude:config get terminal.type
# 출력: iterm2

/team-claude:config get worker.maxConcurrent
# 출력: 5

/team-claude:config get review.agents
# 출력: ["code-reviewer", "qa-agent", "security-auditor"]
```

---

## Action: set

설정 값을 변경합니다.

### 예시

```bash
# 숫자 값
/team-claude:config set worker.maxConcurrent 3

# 문자열 값
/team-claude:config set terminal.type tmux

# 배열 값 (JSON 형식)
/team-claude:config set review.agents '["code-reviewer", "qa-agent"]'

# 불리언 값
/team-claude:config set review.requireApproval true
```

### 유효성 검사

설정 값 변경 시 다음을 검사합니다:

| 키 | 유효한 값 |
|----|----------|
| terminal.type | iterm2, tmux, terminal, manual |
| terminal.layout | tabs, split |
| notification.method | notification, slack, none |
| review.autoLevel | manual, semi-auto, full-auto |
| worker.maxConcurrent | 1-10 |
| completion.coverageThreshold | 0-100 |

### 출력 예시

```
✅ 설정 변경 완료

  worker.maxConcurrent: 5 → 3
```

---

## Action: reset

특정 섹션을 기본값으로 초기화합니다.

### 사용 가능한 섹션

- server
- worktree
- worker
- terminal
- notification
- review
- completion

### 예시

```bash
/team-claude:config reset terminal
```

### 출력 예시

```
🔄 terminal 섹션 초기화 완료

변경 사항:
  type: tmux → iterm2
  layout: split → tabs
```

---

## 설정 키 전체 목록

```
project.name              # 프로젝트명
project.domain            # 도메인 영역
project.language          # 주 언어
project.framework         # 프레임워크

server.port               # 서버 포트 (기본: 3847)
server.host               # 서버 호스트 (기본: localhost)
server.timeout            # 타임아웃 ms (기본: 60000)

worktree.root             # worktree 루트 경로
worktree.branchPrefix     # 브랜치 접두사
worktree.cleanupOnMerge   # 머지 시 정리 여부

worker.maxConcurrent      # 동시 Worker 수
worker.timeout            # Worker 타임아웃 (초)
worker.defaultTemplate    # 기본 템플릿

terminal.type             # 터미널 종류
terminal.layout           # 레이아웃
terminal.maxPanes         # 최대 pane 수
terminal.sessionName      # 세션명

notification.method       # 알림 방식
notification.slack.webhookUrl   # Slack 웹훅 URL
notification.slack.channel      # Slack 채널

agents.enabled            # 활성화된 에이전트 목록
agents.custom             # 커스텀 에이전트 목록
agents.overrides          # 에이전트 설정 오버라이드

review.autoLevel          # 자동화 레벨
review.requireApproval    # 승인 필요 여부

completion.requiredChecks       # 필수 체크 항목
completion.coverageThreshold    # 커버리지 기준
```

---

## 에이전트 설정 관리

에이전트 관련 설정은 `/team-claude:agent` 커맨드 사용을 권장합니다.

```bash
# 에이전트 목록
/team-claude:agent list

# 에이전트 추가
/team-claude:agent add payment-expert

# 에이전트 활성화/비활성화
/team-claude:agent enable domain-expert
/team-claude:agent disable security-auditor
```

config 명령어로 직접 수정도 가능합니다:

```bash
# 활성화된 에이전트 확인
/team-claude:config get agents.enabled

# 에이전트 목록 직접 수정
/team-claude:config set agents.enabled '["code-reviewer", "qa-agent"]'

# 에이전트 모델 오버라이드
/team-claude:config set agents.overrides.code-reviewer.model opus
```

---

## 에러 처리

### 설정 파일 없음

```
❌ Team Claude가 초기화되지 않았습니다.

먼저 /team-claude:init 을 실행해주세요.
```

### 잘못된 키

```
❌ 알 수 없는 설정 키: terminal.invalid

사용 가능한 키:
  terminal.type
  terminal.layout
  terminal.maxPanes
  terminal.sessionName
```

### 잘못된 값

```
❌ 유효하지 않은 값: terminal.type = "invalid"

허용되는 값: iterm2, tmux, terminal, manual
```

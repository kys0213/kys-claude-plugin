---
name: team-claude:setup
description: Team Claude 설정 변경 위자드 - 대화형으로 설정 수정
argument-hint: "[section]"
allowed-tools: ["Read", "Write", "AskUserQuestion"]
---

# Team Claude 설정 위자드 커맨드

대화형 위자드로 설정을 수정합니다.

## 사용법

```bash
# 전체 설정 위자드
/team-claude:setup

# 특정 섹션만 설정
/team-claude:setup server
/team-claude:setup worker
/team-claude:setup terminal
/team-claude:setup notification
/team-claude:setup review
/team-claude:setup agents
```

## Arguments

| Argument | 필수 | 설명 |
|----------|------|------|
| section | X | 특정 섹션만 설정 |

---

## 전체 설정 위자드

section 인자 없이 실행하면 모든 섹션을 순차적으로 설정합니다.

### Step 1/5: 서버 설정

```
━━━ 1/5: 서버 설정 ━━━

현재 값:
  port: 3847
  host: localhost
  timeout: 60000

변경할 항목을 선택하세요:
  1. 포트 변경
  2. 호스트 변경
  3. 타임아웃 변경
  4. 다음으로 건너뛰기
```

### Step 2/5: Worker 설정

```
━━━ 2/5: Worker 설정 ━━━

현재 값:
  maxConcurrent: 5
  timeout: 1800

동시 실행 Worker 최대 수:
  1. 3개 (보수적)
  2. 5개 (기본, 권장)
  3. 8개 (공격적)
  4. 직접 입력
```

### Step 3/5: 터미널 설정

```
━━━ 3/5: 터미널 설정 ━━━

실행 환경:
  1. iTerm2 (탭/분할 지원, 권장)
  2. tmux (세션 관리)
  3. Terminal.app (기본 터미널)
  4. 수동 (직접 터미널 열기)

레이아웃:
  1. tabs (탭으로 분리, 권장)
  2. split (화면 분할)
```

### Step 4/5: 알림 설정

```
━━━ 4/5: 알림 설정 ━━━

완료 알림 방식:
  1. 시스템 알림 (macOS Notification, 권장)
  2. Slack 웹훅
  3. 알림 없음

[Slack 선택 시]
Slack 웹훅 URL을 입력하세요:
> https://hooks.slack.com/services/...
```

### Step 5/5: 리뷰 설정

```
━━━ 5/5: 리뷰 설정 ━━━

리뷰 자동화 레벨:
  1. manual - 수동 리뷰 요청
  2. semi-auto - 완료 시 자동 리뷰 (권장)
  3. full-auto - 피드백까지 자동

사용할 에이전트:
  ☑️ Code Reviewer (필수)
  ☑️ QA Agent (필수)
  ☐ Security Auditor
  ☐ Performance Analyst
  ☐ Domain Expert
```

---

## 섹션별 설정

### /team-claude:setup server

서버 관련 설정만 수정합니다.

```
━━━ 서버 설정 ━━━

서버 포트 [현재: 3847]:
>

서버 호스트 [현재: localhost]:
>

타임아웃 (ms) [현재: 60000]:
>
```

### /team-claude:setup worker

Worker 관련 설정만 수정합니다.

```
━━━ Worker 설정 ━━━

동시 Worker 최대 수 [현재: 5]:
>

Worker 타임아웃 (초) [현재: 1800]:
>

기본 템플릿 [현재: standard]:
>
```

### /team-claude:setup terminal

터미널 관련 설정만 수정합니다.

```
━━━ 터미널 설정 ━━━

실행 환경:
  1. iTerm2 ← 현재
  2. tmux
  3. Terminal.app
  4. 수동

레이아웃:
  1. tabs ← 현재
  2. split

세션명 [현재: team-claude]:
>
```

### /team-claude:setup notification

알림 관련 설정만 수정합니다.

```
━━━ 알림 설정 ━━━

알림 방식:
  1. 시스템 알림 ← 현재
  2. Slack
  3. 없음
```

### /team-claude:setup review

리뷰 관련 설정만 수정합니다.

```
━━━ 리뷰 설정 ━━━

자동화 레벨:
  1. manual
  2. semi-auto ← 현재
  3. full-auto

승인 필요 여부:
  1. 예 ← 현재
  2. 아니오

커버리지 기준 [현재: 80]:
>
```

### /team-claude:setup agents

에이전트 구성을 수정합니다.

```
━━━ 에이전트 설정 ━━━

현재 활성화된 에이전트:
  ✓ code-reviewer
  ✓ qa-agent
  ✓ security-auditor

사용 가능한 에이전트:
  1. code-reviewer (코드 품질)
  2. qa-agent (테스트 케이스)
  3. security-auditor (보안 검토)
  4. performance-analyst (성능 분석)
  5. domain-expert (도메인 전문가)
  6. architecture-reviewer (아키텍처 검토)

활성화할 에이전트 번호 (쉼표 구분):
> 1,2,3,5
```

---

## 완료 출력

```
✅ 설정 변경 완료

변경 사항:
  terminal.type: iterm2 → tmux
  terminal.layout: tabs → split
  worker.maxConcurrent: 5 → 3

설정 파일 저장됨: .team-claude/config.json
```

---

## 에러 처리

### 설정 파일 없음

```
❌ Team Claude가 초기화되지 않았습니다.

먼저 /team-claude:init 을 실행해주세요.
```

### 알 수 없는 섹션

```
❌ 알 수 없는 섹션: invalid

사용 가능한 섹션:
  server, worker, terminal, notification, review, agents
```

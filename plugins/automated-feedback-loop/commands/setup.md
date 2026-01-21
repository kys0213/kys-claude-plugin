---
name: afl:setup
description: AFL 설정 위자드 - 대화형으로 설정 수정
argument-hint: "[section]"
allowed-tools: ["Read", "Write", "AskUserQuestion", "Bash", "Glob"]
---

# Setup 커맨드

대화형 위자드로 설정을 수정합니다.

## 사용법

```bash
# 전체 설정 위자드
/afl:setup

# 특정 섹션만 설정
/afl:setup project
/afl:setup feedback
/afl:setup validation
/afl:setup notification
/afl:setup agents
/afl:setup server     # 서버 설정 및 설치
```

---

## 설정 파일 위치

```
.claude/afl.yaml
```

---

## 전체 설정 위자드

section 인자 없이 실행하면 모든 섹션을 순차적으로 설정합니다.

### Step 1/5: 프로젝트 설정

```
━━━ 1/5: 프로젝트 설정 ━━━

현재 감지된 값:
  language: python
  framework: fastapi
  test_command: pytest
  build_command: poetry build

다시 감지하거나 수동 설정할 수 있습니다.
```

```typescript
AskUserQuestion({
  questions: [{
    question: "프로젝트 설정을 어떻게 하시겠습니까?",
    header: "Project",
    options: [
      { label: "자동 감지 유지 (권장)", description: "현재 감지된 값 사용" },
      { label: "다시 감지", description: "프로젝트 재분석" },
      { label: "수동 입력", description: "직접 값 입력" }
    ],
    multiSelect: false
  }]
})
```

### Step 2/5: 피드백 루프 설정

```
━━━ 2/5: 피드백 루프 설정 ━━━

현재 값:
  mode: auto
  max_iterations: 5
  auto_retry_delay: 5000ms
```

```typescript
AskUserQuestion({
  questions: [{
    question: "피드백 루프 모드를 선택하세요",
    header: "Mode",
    options: [
      { label: "auto (권장)", description: "실패 시 자동 분석 + 재시도" },
      { label: "semi-auto", description: "분석만 자동, 재시도는 수동" },
      { label: "manual", description: "모든 단계 수동 확인" }
    ],
    multiSelect: false
  }, {
    question: "최대 재시도 횟수는?",
    header: "Iterations",
    options: [
      { label: "3회", description: "빠른 에스컬레이션" },
      { label: "5회 (권장)", description: "균형잡힌 설정" },
      { label: "10회", description: "끈질기게 시도" }
    ],
    multiSelect: false
  }]
})
```

### Step 3/5: 검증 설정

```
━━━ 3/5: 검증 설정 ━━━

현재 값:
  method: test
  timeout: 120000ms (2분)
```

```typescript
AskUserQuestion({
  questions: [{
    question: "Checkpoint 검증 방식을 선택하세요",
    header: "Validation",
    options: [
      { label: "테스트 명령어 (권장)", description: "pytest, go test 등 실행" },
      { label: "커스텀 스크립트", description: "직접 작성한 검증 스크립트" },
      { label: "수동 확인", description: "사람이 직접 확인" }
    ],
    multiSelect: false
  }]
})
```

### Step 4/5: 알림 설정

```
━━━ 4/5: 알림 설정 ━━━

현재 값:
  method: system
```

```typescript
AskUserQuestion({
  questions: [{
    question: "작업 완료/에스컬레이션 알림 방식을 선택하세요",
    header: "Notification",
    options: [
      { label: "시스템 알림 (권장)", description: "OS 알림 센터 사용" },
      { label: "Slack", description: "Slack 웹훅으로 알림" },
      { label: "없음", description: "알림 비활성화" }
    ],
    multiSelect: false
  }]
})
```

Slack 선택 시 추가 입력:

```
Slack 웹훅 URL을 입력하세요:
> https://hooks.slack.com/services/...
```

### Step 5/5: 에이전트 설정

```
━━━ 5/5: 에이전트 설정 ━━━

현재 활성화된 에이전트:
  ✓ spec_validator (스펙 검증)
  ✓ test_oracle (테스트 분석)
  ✓ impl_reviewer (구현 검토)
```

```typescript
AskUserQuestion({
  questions: [{
    question: "활성화할 에이전트를 선택하세요",
    header: "Agents",
    options: [
      { label: "spec_validator", description: "설계 문서 일관성 검증" },
      { label: "test_oracle", description: "테스트 실패 분석 및 피드백" },
      { label: "impl_reviewer", description: "구현 품질 검토" }
    ],
    multiSelect: true
  }]
})
```

---

## 섹션별 설정

### /afl:setup project

프로젝트 관련 설정만 수정합니다.

```
━━━ 프로젝트 설정 ━━━

language [현재: python]:
>

framework [현재: fastapi]:
>

test_command [현재: pytest]:
>

build_command [현재: poetry build]:
>

lint_command [현재: ruff check .]:
>
```

### /afl:setup feedback

피드백 루프 설정만 수정합니다.

```
━━━ 피드백 루프 설정 ━━━

mode [현재: auto]:
  1. auto ← 현재
  2. semi-auto
  3. manual

max_iterations [현재: 5]:
>

auto_retry_delay (ms) [현재: 5000]:
>
```

### /afl:setup validation

검증 설정만 수정합니다.

```
━━━ 검증 설정 ━━━

method [현재: test]:
  1. test ← 현재
  2. script
  3. manual

timeout (ms) [현재: 120000]:
>
```

### /afl:setup notification

알림 설정만 수정합니다.

```
━━━ 알림 설정 ━━━

method [현재: system]:
  1. system ← 현재
  2. slack
  3. none
```

### /afl:setup agents

에이전트 설정만 수정합니다.

```
━━━ 에이전트 설정 ━━━

활성화할 에이전트 (쉼표 구분):
  1. spec_validator [✓]
  2. test_oracle [✓]
  3. impl_reviewer [✓]

선택 (예: 1,2,3):
>
```

### /afl:setup server

AFL 로컬 서버를 설치하고 설정합니다.

```
━━━ 서버 설정 ━━━

AFL 서버는 Worker Claude들을 관리하고 피드백 루프를 자동화합니다.
```

```typescript
AskUserQuestion({
  questions: [{
    question: "Worker 실행 방식을 선택하세요",
    header: "Executor",
    options: [
      { label: "iTerm2 (권장)", description: "새 탭에서 실행 - 작업 과정이 보임" },
      { label: "Terminal.app", description: "macOS 기본 터미널 사용" },
      { label: "Headless", description: "백그라운드 실행 - 로그로만 확인" }
    ],
    multiSelect: false
  }]
})
```

터미널 선택 후:

```typescript
AskUserQuestion({
  questions: [{
    question: "서버 포트를 선택하세요",
    header: "Port",
    options: [
      { label: "7890 (기본값)", description: "http://localhost:7890" },
      { label: "8080", description: "일반적인 개발 포트" },
      { label: "직접 입력", description: "커스텀 포트 지정" }
    ],
    multiSelect: false
  }]
})
```

#### 서버 설치

Bun이 설치되어 있으면:

```bash
# 의존성 설치
cd plugins/automated-feedback-loop/server && bun install

# 서버 빌드 (단일 바이너리)
bun build src/index.ts --compile --outfile ~/.local/bin/afl-server
```

Bun이 없으면 먼저 설치 안내:

```
⚠️ Bun이 설치되어 있지 않습니다.

Bun은 빠른 JavaScript 런타임으로, AFL 서버 실행에 필요합니다.
```

```typescript
AskUserQuestion({
  questions: [{
    question: "Bun을 설치할까요?",
    header: "Install",
    options: [
      { label: "예, 설치 (권장)", description: "curl로 자동 설치" },
      { label: "아니오", description: "직접 설치 후 다시 실행" }
    ],
    multiSelect: false
  }]
})
```

설치 명령:
```bash
curl -fsSL https://bun.sh/install | bash
```

#### 서버 시작/중지

```
━━━ 서버 상태 ━━━

  상태: 🔴 중지됨
  포트: 7890
  Executor: iterm
```

```typescript
AskUserQuestion({
  questions: [{
    question: "서버를 시작할까요?",
    header: "Server",
    options: [
      { label: "시작", description: "afl-server를 백그라운드로 실행" },
      { label: "나중에", description: "수동으로 시작" }
    ],
    multiSelect: false
  }]
})
```

시작 명령:
```bash
# 환경 변수와 함께 서버 시작
AFL_PORT=7890 AFL_EXECUTOR=iterm nohup afl-server > ~/.afl/server.log 2>&1 &
echo $! > ~/.afl/server.pid
```

서버 상태 확인:
```bash
curl -s http://localhost:7890/health | jq
```

---

## 완료 출력

```
✅ 설정 변경 완료

변경 사항:
  feedback_loop.mode: auto → semi-auto
  feedback_loop.max_iterations: 5 → 3
  notification.method: system → slack

저장됨: .claude/afl.yaml
```

---

## 설정 파일이 없을 때

```
⚠️ 설정 파일이 없습니다.

새로 생성하시겠습니까?
```

```typescript
AskUserQuestion({
  questions: [{
    question: "설정 파일을 새로 생성할까요?",
    header: "Create",
    options: [
      { label: "예, 생성 (권장)", description: "프로젝트 분석 후 기본 설정 생성" },
      { label: "/afl:init 실행", description: "전체 초기화 위자드 실행" }
    ],
    multiSelect: false
  }]
})
```

---

## 에러 처리

### 알 수 없는 섹션

```
❌ 알 수 없는 섹션: invalid

사용 가능한 섹션:
  project, feedback, validation, notification, agents
```

### 잘못된 값 입력

```
❌ 잘못된 값: abc

max_iterations는 숫자여야 합니다.
```

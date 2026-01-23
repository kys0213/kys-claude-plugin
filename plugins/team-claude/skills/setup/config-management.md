# 설정 관리

설정 조회 및 수정 기능입니다.

---

## 현재 설정 보기

전체 설정을 출력합니다:

```
📋 Team Claude 설정

━━━ 프로젝트 ━━━
  language:      python
  framework:     fastapi
  test_command:  pytest
  build_command: poetry build

━━━ 피드백 루프 ━━━
  mode:           auto
  max_iterations: 5
  retry_delay:    5000ms

━━━ 검증 ━━━
  method:  test
  timeout: 120000ms

━━━ 알림 ━━━
  method: system

━━━ 서버 ━━━
  port: 7890
  executor: iterm

━━━ 에이전트 ━━━
  활성화: spec_validator, test_oracle, impl_reviewer
  커스텀: payment-expert, security-auditor
```

조회 후 후속 액션:

```typescript
AskUserQuestion({
  questions: [{
    question: "설정을 변경하시겠습니까?",
    header: "Modify",
    options: [
      { label: "아니오", description: "메인 메뉴로 돌아가기" },
      { label: "예, 변경", description: "설정 수정 진행" }
    ],
    multiSelect: false
  }]
})
```

---

## 설정 수정

### 섹션 선택

```typescript
AskUserQuestion({
  questions: [{
    question: "어떤 설정을 변경하시겠습니까?",
    header: "Section",
    options: [
      { label: "전체 위자드", description: "모든 섹션 순차 설정" },
      { label: "프로젝트", description: "언어, 테스트 명령어 등" },
      { label: "피드백 루프", description: "모드, 재시도 횟수" },
      { label: "검증", description: "검증 방식, 타임아웃" },
      { label: "알림", description: "알림 방식" }
    ],
    multiSelect: false
  }]
})
```

---

## 전체 설정 위자드

### Step 1/5: 프로젝트 설정

```
━━━ 1/5: 프로젝트 설정 ━━━

현재 감지된 값:
  language: python
  framework: fastapi
  test_command: pytest
  build_command: poetry build
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

Slack 선택 시:

```
Slack 웹훅 URL을 입력하세요:
> https://hooks.slack.com/services/...
```

### Step 5/5: 에이전트 활성화

```typescript
AskUserQuestion({
  questions: [{
    question: "활성화할 기본 에이전트를 선택하세요",
    header: "Agents",
    options: [
      { label: "spec_validator", description: "설계 문서 일관성 검증" },
      { label: "test_oracle", description: "테스트 실패 분석 및 피드백" },
      { label: "impl_reviewer", description: "구현 품질 검토" },
      { label: "conflict_analyzer", description: "머지 충돌 분석" }
    ],
    multiSelect: true
  }]
})
```

---

## 섹션별 설정

### 프로젝트 설정

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

### 피드백 루프 설정

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

### 검증 설정

```
━━━ 검증 설정 ━━━

method [현재: test]:
  1. test ← 현재
  2. script
  3. manual

timeout (ms) [현재: 120000]:
>
```

### 알림 설정

```
━━━ 알림 설정 ━━━

method [현재: system]:
  1. system ← 현재
  2. slack
  3. none
```

---

## 완료 출력

```
✅ 설정 변경 완료

변경 사항:
  feedback_loop.mode: auto → semi-auto
  feedback_loop.max_iterations: 5 → 3
  notification.method: system → slack

저장됨: .claude/team-claude.yaml
```

---

## 설정 키 전체 목록

| 섹션 | 키 | 설명 |
|------|-----|------|
| **project** | language | 프로젝트 언어 |
| | framework | 프레임워크 |
| | test_command | 테스트 명령어 |
| | build_command | 빌드 명령어 |
| | lint_command | 린트 명령어 |
| **feedback_loop** | mode | `auto` / `semi-auto` / `manual` |
| | max_iterations | 최대 재시도 횟수 |
| | auto_retry_delay | 재시도 지연 (ms) |
| **validation** | method | `test` / `script` / `manual` |
| | timeout | 타임아웃 (ms) |
| **notification** | method | `system` / `slack` / `none` |
| | slack.webhook_url | Slack 웹훅 URL |
| **server** | port | 서버 포트 |
| | executor | `iterm` / `terminal-app` / `headless` |
| **agents** | enabled | 활성화된 에이전트 목록 |
| | custom | 커스텀 에이전트 목록 |
| | overrides | 에이전트별 설정 오버라이드 |

---

## 에러 처리

### 설정 파일 없음

```
⚠️ 설정 파일이 없습니다.

설정 파일을 생성할까요?
```

→ 초기화 모드로 안내

### 잘못된 값

```
❌ 잘못된 값: abc

max_iterations는 숫자여야 합니다.
```

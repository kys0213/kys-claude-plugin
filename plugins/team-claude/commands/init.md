---
name: team-claude:init
description: Team Claude 초기 설정 - 프로젝트 분석 및 환경 구성
argument-hint: ""
allowed-tools: ["Bash", "Read", "Write", "Glob", "Grep", "AskUserQuestion"]
---

# Team Claude 초기화 커맨드

프로젝트를 분석하고 Team Claude 환경을 구성합니다.

## 워크플로우

```
1. 프로젝트 자동 분석
   │
   ▼
2. 언어/프레임워크 감지
   │
   ▼
3. 인터뷰 (AskUserQuestion)
   │
   ▼
4. 설정 파일 생성
   │
   ▼
5. Hook 설정
```

---

## Step 1: 프로젝트 자동 분석

다음 파일들을 분석하여 프로젝트 특성을 파악합니다:

### 언어 감지

| 감지 파일 | 언어 | 테스트 도구 | 빌드 도구 |
|-----------|------|------------|----------|
| `package.json` | JavaScript/TypeScript | Jest, Vitest, Mocha | npm, yarn, pnpm |
| `pyproject.toml`, `setup.py` | Python | pytest, unittest | pip, poetry |
| `go.mod` | Go | go test | go build |
| `Cargo.toml` | Rust | cargo test | cargo build |
| `pom.xml` | Java | JUnit, TestNG | Maven |
| `build.gradle` | Java/Kotlin | JUnit | Gradle |
| `*.csproj` | C# | xUnit, NUnit | dotnet |
| `Gemfile` | Ruby | RSpec, Minitest | bundler |
| `mix.exs` | Elixir | ExUnit | mix |

### 분석 결과 정리

```markdown
## 프로젝트 분석 결과

- **언어**: {detected_language}
- **프레임워크**: {detected_framework}
- **테스트 도구**: {test_tool}
- **빌드 도구**: {build_tool}
- **린터**: {linter}
- **구조**: {project_structure}
```

---

## Step 2: 인터뷰 (AskUserQuestion)

### Q1: 프로젝트 도메인

```typescript
AskUserQuestion({
  questions: [{
    question: "이 프로젝트의 도메인 영역은 무엇인가요?",
    header: "Domain",
    options: [
      { label: "이커머스/결제", description: "상품, 주문, 결제 관련" },
      { label: "금융/핀테크", description: "계좌, 거래, 투자 관련" },
      { label: "SaaS/B2B", description: "기업용 서비스" },
      { label: "소비자 앱", description: "일반 사용자 대상 서비스" }
    ],
    multiSelect: false
  }]
})
```

### Q2: 피드백 루프 설정

```typescript
AskUserQuestion({
  questions: [{
    question: "자동 피드백 루프 설정을 어떻게 하시겠습니까?",
    header: "Feedback Loop",
    options: [
      { label: "자동 (권장)", description: "실패 시 자동 분석 + 재시도 (최대 5회)" },
      { label: "반자동", description: "실패 시 분석만, 재시도는 수동" },
      { label: "수동", description: "모든 검증 후 수동 개입" }
    ],
    multiSelect: false
  }]
})
```

### Q3: Checkpoint 검증 방식

```typescript
AskUserQuestion({
  questions: [{
    question: "Checkpoint 검증은 어떻게 하시겠습니까?",
    header: "Validation",
    options: [
      { label: "테스트 명령어 (권장)", description: "npm test, pytest 등 실행" },
      { label: "커스텀 스크립트", description: "직접 작성한 검증 스크립트" },
      { label: "수동 확인", description: "사람이 직접 확인" }
    ],
    multiSelect: false
  }]
})
```

### Q4: 알림 방식

```typescript
AskUserQuestion({
  questions: [{
    question: "작업 완료/에스컬레이션 알림을 어떻게 받으시겠습니까?",
    header: "Notification",
    options: [
      { label: "시스템 알림 (권장)", description: "OS 알림 센터" },
      { label: "Slack 웹훅", description: "Slack 채널로 알림" },
      { label: "알림 없음", description: "로그로만 확인" }
    ],
    multiSelect: false
  }]
})
```

---

## Step 3: 설정 파일 생성

### 생성되는 디렉토리 구조

```
.team-claude/
├── config.json              # 메인 설정
├── sessions/                # 세션 데이터
│   └── index.json
├── state/                   # 런타임 상태
│   └── current-delegation.json
├── hooks/                   # Hook 스크립트
│   ├── on-worker-complete.sh
│   ├── on-validation-complete.sh
│   ├── on-worker-question.sh
│   └── on-worker-idle.sh
└── templates/               # 템플릿
    ├── checkpoint.yaml
    └── delegation-spec.md
```

### config.json 스키마

```json
{
  "version": "1.0",
  "project": {
    "name": "{project_name}",
    "language": "{detected_language}",
    "framework": "{detected_framework}",
    "domain": "{selected_domain}"
  },
  "detection": {
    "testCommand": "{auto_detected_test_command}",
    "buildCommand": "{auto_detected_build_command}",
    "lintCommand": "{auto_detected_lint_command}"
  },
  "feedbackLoop": {
    "mode": "auto",
    "maxIterations": 5,
    "autoRetryDelay": 5000,
    "escalationThreshold": 3
  },
  "validation": {
    "method": "test_command",
    "timeout": 120000
  },
  "notification": {
    "method": "system",
    "slack": {
      "webhookUrl": "",
      "channel": ""
    }
  },
  "architectLoop": {
    "requireHumanApproval": ["architecture", "contracts", "checkpoints"],
    "autoProgress": ["implementation", "test"]
  },
  "agents": {
    "specValidator": true,
    "testOracle": true,
    "implReviewer": true
  }
}
```

---

## Step 4: Hook 설정

플러그인의 hook 스크립트를 프로젝트로 복사합니다:

```bash
# Hook 스크립트 복사
cp -r {plugin_path}/hooks/scripts/* .team-claude/hooks/

# 실행 권한 부여
chmod +x .team-claude/hooks/*.sh
```

---

## 완료 메시지

```
✅ Team Claude 초기화 완료

📁 생성된 설정:
  .team-claude/
  ├── config.json
  ├── sessions/
  ├── state/
  ├── hooks/ (4개 스크립트)
  └── templates/

📊 감지된 프로젝트 정보:
  • 언어: {language}
  • 프레임워크: {framework}
  • 테스트: {test_command}
  • 도메인: {domain}

⚙️ 설정:
  • 피드백 루프: {feedback_mode}
  • 최대 재시도: {max_iterations}회
  • 알림: {notification_method}

다음 단계:
  1. 설계 루프 시작:
     /team-claude:architect "요구사항"

  2. 설정 변경:
     /team-claude:config list
     /team-claude:setup
```

---

## 재초기화

이미 `.team-claude/`가 존재하는 경우:

```typescript
AskUserQuestion({
  questions: [{
    question: "AFL이 이미 초기화되어 있습니다. 어떻게 하시겠습니까?",
    header: "Reinit",
    options: [
      { label: "재초기화", description: "기존 설정 백업 후 재설정" },
      { label: "유지", description: "기존 설정 유지" },
      { label: "설정만 수정", description: "/team-claude:setup 실행" }
    ],
    multiSelect: false
  }]
})
```

---

## 언어별 기본 설정

### JavaScript/TypeScript

```json
{
  "detection": {
    "testCommand": "npm test",
    "buildCommand": "npm run build",
    "lintCommand": "npm run lint"
  }
}
```

### Python

```json
{
  "detection": {
    "testCommand": "pytest",
    "buildCommand": "python -m build",
    "lintCommand": "ruff check ."
  }
}
```

### Go

```json
{
  "detection": {
    "testCommand": "go test ./...",
    "buildCommand": "go build ./...",
    "lintCommand": "golangci-lint run"
  }
}
```

### Rust

```json
{
  "detection": {
    "testCommand": "cargo test",
    "buildCommand": "cargo build",
    "lintCommand": "cargo clippy"
  }
}
```

### Java (Maven)

```json
{
  "detection": {
    "testCommand": "mvn test",
    "buildCommand": "mvn package",
    "lintCommand": "mvn checkstyle:check"
  }
}
```

### Java (Gradle)

```json
{
  "detection": {
    "testCommand": "./gradlew test",
    "buildCommand": "./gradlew build",
    "lintCommand": "./gradlew check"
  }
}
```

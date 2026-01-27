# 초기화 모드

`.claude/team-claude.yaml`이 없을 때 자동 진입합니다.

## Step 1: 프로젝트 자동 분석

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

### 분석 결과 출력

```
## 프로젝트 분석 결과

- **언어**: {detected_language}
- **프레임워크**: {detected_framework}
- **테스트 도구**: {test_tool}
- **빌드 도구**: {build_tool}
- **린터**: {linter}
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
    header: "Feedback",
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

### 생성되는 디렉토리

```
~/.team-claude/{project-hash}/     # 전역 데이터 (프로젝트별 격리)
├── team-claude.yaml               # 프로젝트 설정
├── sessions/
│   └── index.json
├── state/
│   └── workflow.json
└── worktrees/

<project>/.claude/                 # Claude Code 표준 디렉토리
├── settings.local.json            # hooks 설정 (레포별)
├── agents/                        # 프로젝트 에이전트 정의
└── hooks/                         # hook 스크립트
    ├── on-worker-complete.sh
    ├── on-validation-complete.sh
    ├── on-worker-question.sh
    └── on-worker-idle.sh
```

**프로젝트 해시**: git root 경로의 md5 해시 앞 12자리

---

## Step 4: Hook 설정

`tc-config init` 명령이 자동으로 처리합니다:

1. 플러그인의 hook 스크립트를 `.claude/hooks/`에 복사
2. `.claude/settings.local.json`에 hooks 설정 추가/병합

### 수동 설정 (참고용)

```bash
# Hook 스크립트 복사
mkdir -p .claude/hooks
cp -r ${CLAUDE_PLUGIN_ROOT}/hooks/scripts/* .claude/hooks/
chmod +x .claude/hooks/*.sh
```

### 생성되는 settings.local.json

```json
{
  "hooks": {
    "Stop": [
      {
        "type": "command",
        "command": ".claude/hooks/on-worker-complete.sh"
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Task",
        "hooks": [
          {
            "type": "command",
            "command": ".claude/hooks/on-worker-question.sh"
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": ".*",
        "hooks": [
          {
            "type": "command",
            "command": ".claude/hooks/on-worker-idle.sh"
          }
        ]
      }
    ]
  }
}
```

---

## 완료 메시지

```
✅ Team Claude 초기화 완료

📁 생성된 설정:
  ~/.team-claude/{hash}/
  ├── team-claude.yaml (설정)
  ├── sessions/
  ├── state/
  └── worktrees/

  .claude/
  ├── settings.local.json (hooks 설정)
  ├── agents/
  └── hooks/ (4개 스크립트)

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
  /team-claude:architect "요구사항"
```

---

## 재초기화

이미 설정이 존재하는 경우:

```typescript
AskUserQuestion({
  questions: [{
    question: "Team Claude가 이미 초기화되어 있습니다. 어떻게 하시겠습니까?",
    header: "Reinit",
    options: [
      { label: "재초기화", description: "기존 설정 백업 후 재설정" },
      { label: "유지", description: "기존 설정 유지하고 메인 메뉴로" }
    ],
    multiSelect: false
  }]
})
```

---

## 언어별 기본 설정

### JavaScript/TypeScript

```yaml
project:
  test_command: npm test
  build_command: npm run build
  lint_command: npm run lint
```

### Python

```yaml
project:
  test_command: pytest
  build_command: python -m build
  lint_command: ruff check .
```

### Go

```yaml
project:
  test_command: go test ./...
  build_command: go build ./...
  lint_command: golangci-lint run
```

### Rust

```yaml
project:
  test_command: cargo test
  build_command: cargo build
  lint_command: cargo clippy
```

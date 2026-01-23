# 서버 관리

Team Claude 로컬 서버를 설치하고 관리합니다.

## 메인 메뉴

```typescript
AskUserQuestion({
  questions: [{
    question: "서버 관리 작업을 선택하세요",
    header: "Server",
    options: [
      { label: "서버 상태 확인", description: "현재 서버 상태 조회" },
      { label: "서버 설정 변경", description: "포트, 실행 방식 변경" },
      { label: "서버 설치/빌드", description: "서버 바이너리 설치" },
      { label: "서버 시작", description: "서버 백그라운드 실행" },
      { label: "서버 중지", description: "실행 중인 서버 종료" }
    ],
    multiSelect: false
  }]
})
```

---

## 서버 상태 확인

```
━━━ 서버 상태 ━━━

  상태: 🟢 실행 중 (PID: 12345)
  포트: 7890
  Executor: iterm
  가동 시간: 2시간 15분

━━━ 최근 활동 ━━━

  • Worker 3개 실행 중
  • 마지막 작업: coupon-service (진행 중)
  • 완료된 작업: 5개
```

또는 중지 상태:

```
━━━ 서버 상태 ━━━

  상태: 🔴 중지됨
  포트: 7890
  Executor: iterm

서버를 시작하시겠습니까?
```

---

## 서버 설정 변경

### Executor 선택

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

### 포트 선택

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

### 설정 완료

```
✅ 서버 설정 변경 완료

변경 사항:
  server.executor: iterm → headless
  server.port: 7890 → 8080

저장됨: .claude/team-claude.yaml

⚠️ 서버가 실행 중입니다. 재시작해야 변경사항이 적용됩니다.
재시작하시겠습니까?
```

---

## 서버 설치/빌드

### Bun 확인

```bash
# Bun 설치 확인
which bun
```

### Bun 미설치 시

```
⚠️ Bun이 설치되어 있지 않습니다.

Bun은 빠른 JavaScript 런타임으로, Team Claude 서버 실행에 필요합니다.
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

### 서버 빌드

```bash
# 의존성 설치
cd plugins/team-claude/server && bun install

# 서버 빌드 (단일 바이너리)
bun build src/index.ts --compile --outfile ~/.local/bin/team-claude-server

# 실행 권한
chmod +x ~/.local/bin/team-claude-server
```

### 빌드 완료

```
✅ 서버 빌드 완료

  바이너리: ~/.local/bin/team-claude-server
  버전: 1.0.0

서버를 지금 시작하시겠습니까?
```

---

## 서버 시작

### 시작 확인

```typescript
AskUserQuestion({
  questions: [{
    question: "서버를 시작할까요?",
    header: "Start",
    options: [
      { label: "시작", description: "백그라운드로 서버 실행" },
      { label: "취소", description: "시작하지 않음" }
    ],
    multiSelect: false
  }]
})
```

### 시작 명령

```bash
# 로그 디렉토리 생성
mkdir -p ~/.team-claude

# 환경 변수와 함께 서버 시작
TEAM_CLAUDE_PORT=7890 \
TEAM_CLAUDE_EXECUTOR=iterm \
nohup team-claude-server > ~/.team-claude/server.log 2>&1 &

# PID 저장
echo $! > ~/.team-claude/server.pid
```

### 시작 확인

```bash
# 서버 상태 확인 (몇 초 대기 후)
curl -s http://localhost:7890/health | jq
```

### 시작 완료

```
✅ 서버 시작 완료

  상태: 🟢 실행 중
  PID: 12345
  URL: http://localhost:7890
  로그: ~/.team-claude/server.log

서버가 정상적으로 시작되었습니다.
```

---

## 서버 중지

### 중지 확인

```typescript
AskUserQuestion({
  questions: [{
    question: "⚠️ 실행 중인 Worker가 있을 수 있습니다. 서버를 중지하시겠습니까?",
    header: "Stop",
    options: [
      { label: "예, 중지", description: "서버 프로세스 종료" },
      { label: "아니오", description: "중지하지 않음" }
    ],
    multiSelect: false
  }]
})
```

### 중지 명령

```bash
# PID 파일에서 프로세스 종료
kill $(cat ~/.team-claude/server.pid)

# PID 파일 삭제
rm ~/.team-claude/server.pid
```

### 중지 완료

```
✅ 서버 중지 완료

  상태: 🔴 중지됨
  이전 PID: 12345

서버가 정상적으로 중지되었습니다.
```

---

## 로그 확인

```bash
# 실시간 로그 확인
tail -f ~/.team-claude/server.log

# 최근 50줄
tail -50 ~/.team-claude/server.log

# 에러만 확인
grep -i error ~/.team-claude/server.log
```

---

## 트러블슈팅

### 포트 충돌

```
❌ 포트 7890이 이미 사용 중입니다.

  사용 중인 프로세스: node (PID: 54321)
```

```typescript
AskUserQuestion({
  questions: [{
    question: "어떻게 하시겠습니까?",
    header: "Conflict",
    options: [
      { label: "다른 포트 사용", description: "포트 변경" },
      { label: "기존 프로세스 종료", description: "PID 54321 종료 후 시작" },
      { label: "취소", description: "서버 시작 취소" }
    ],
    multiSelect: false
  }]
})
```

### 서버 응답 없음

```
⚠️ 서버가 응답하지 않습니다.

  PID 파일: 존재함 (12345)
  프로세스: 실행 중이지만 응답 없음

로그를 확인하시겠습니까?
```

### 바이너리 없음

```
❌ team-claude-server 바이너리를 찾을 수 없습니다.

서버를 먼저 빌드해야 합니다.
```

→ 서버 설치/빌드로 안내

---

## 설정 키

| 키 | 설명 | 기본값 |
|----|------|--------|
| `server.port` | 서버 포트 | `7890` |
| `server.executor` | Worker 실행 방식 | `iterm` |

### Executor 옵션

| 값 | 설명 |
|----|------|
| `iterm` | iTerm2 새 탭에서 실행 (작업 과정 시각화) |
| `terminal-app` | macOS Terminal.app 사용 |
| `headless` | 백그라운드 실행 (로그로만 확인) |

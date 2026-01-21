---
name: afl:config
description: AFL 설정 조회 및 변경 - 현재 설정 확인 후 대화형으로 변경
argument-hint: ""
allowed-tools: ["Read", "Write", "AskUserQuestion", "Bash"]
---

# Config 커맨드

현재 설정을 보여주고, 원하면 바로 변경할 수 있습니다.

## 사용법

```bash
/afl:config
```

---

## 설정 파일 위치

```
.claude/afl.yaml
```

---

## 실행 절차

```
1. 현재 설정 출력
       │
       ▼
2. AskUserQuestion: "변경하시겠습니까?"
       │
       ├─ 아니오 → 종료
       │
       └─ 예 → 어떤 섹션?
                  │
                  ▼
            3. 해당 섹션 변경 (AskUserQuestion)
                  │
                  ▼
            4. 저장 및 완료
```

---

## Step 1: 현재 설정 출력

```
📋 AFL 설정

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

━━━ 에이전트 ━━━
  ✓ spec_validator
  ✓ test_oracle
  ✓ impl_reviewer
```

---

## Step 2: 변경 여부 확인

```typescript
AskUserQuestion({
  questions: [{
    question: "설정을 변경하시겠습니까?",
    header: "Config",
    options: [
      { label: "아니오", description: "현재 설정 유지" },
      { label: "예, 변경", description: "설정 변경 진행" }
    ],
    multiSelect: false
  }]
})
```

---

## Step 3: 섹션 선택 (변경 시)

```typescript
AskUserQuestion({
  questions: [{
    question: "어떤 설정을 변경하시겠습니까?",
    header: "Section",
    options: [
      { label: "프로젝트", description: "언어, 테스트 명령어 등" },
      { label: "피드백 루프", description: "모드, 재시도 횟수" },
      { label: "검증", description: "검증 방식, 타임아웃" },
      { label: "알림", description: "알림 방식" }
    ],
    multiSelect: true
  }]
})
```

---

## Step 4: 섹션별 변경

선택한 섹션에 대해 AskUserQuestion으로 변경 진행합니다.

### 피드백 루프 변경 예시

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

### 알림 변경 예시

```typescript
AskUserQuestion({
  questions: [{
    question: "알림 방식을 선택하세요",
    header: "Notification",
    options: [
      { label: "시스템 알림 (권장)", description: "OS 알림 센터" },
      { label: "Slack", description: "Slack 웹훅" },
      { label: "없음", description: "알림 비활성화" }
    ],
    multiSelect: false
  }]
})
```

---

## Step 5: 완료

```
✅ 설정 변경 완료

변경 사항:
  feedback_loop.mode: auto → semi-auto
  notification.method: system → slack

저장됨: .claude/afl.yaml
```

---

## 설정 파일이 없을 때

```typescript
AskUserQuestion({
  questions: [{
    question: "설정 파일이 없습니다. 생성할까요?",
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

## 설정 키 설명

| 섹션 | 키 | 설명 |
|------|-----|------|
| **project** | language | 프로젝트 언어 (자동 감지) |
| | test_command | 테스트 실행 명령어 |
| | build_command | 빌드 명령어 |
| **feedback_loop** | mode | `auto` / `semi-auto` / `manual` |
| | max_iterations | 최대 재시도 횟수 |
| **validation** | method | `test` / `script` / `manual` |
| | timeout | 검증 타임아웃 (ms) |
| **notification** | method | `system` / `slack` / `none` |
| **agents** | spec_validator | 스펙 검증 에이전트 |
| | test_oracle | 테스트 분석 에이전트 |
| | impl_reviewer | 구현 검토 에이전트 |

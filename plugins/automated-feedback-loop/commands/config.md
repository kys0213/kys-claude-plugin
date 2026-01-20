---
name: afl:config
description: AFL 설정 조회 - 현재 설정 확인
argument-hint: ""
allowed-tools: ["Read", "Bash"]
---

# Config 커맨드

현재 설정을 조회합니다. 변경은 `/afl:setup`을 사용하세요.

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

1. `.claude/afl.yaml` 파일 읽기
2. 현재 설정을 보기 좋게 출력
3. 변경 방법 안내

---

## 출력 예시

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

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

설정 변경: /afl:setup
파일 위치: .claude/afl.yaml
```

---

## 설정 파일이 없을 때

```
⚠️ 설정 파일이 없습니다.

초기 설정:
  /afl:init

또는 대화형 설정:
  /afl:setup
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

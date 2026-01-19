---
name: afl:config
description: AFL 설정 조회 및 수정 - CLI 방식으로 설정값 확인/변경
argument-hint: "list | get <key> | set <key> <value>"
allowed-tools: ["Read", "Write", "Bash"]
---

# Config 커맨드

CLI 방식으로 설정을 조회하고 수정합니다.

## 사용법

```bash
# 전체 설정 조회
/afl:config list

# 특정 설정 조회
/afl:config get feedback_loop.max_iterations

# 설정 변경
/afl:config set feedback_loop.max_iterations 3

# 설정 파일 경로 확인
/afl:config path
```

---

## 설정 파일 위치

```
.claude/afl.yaml
```

---

## 명령어 상세

### list - 전체 설정 조회

```bash
/afl:config list
```

```yaml
# .claude/afl.yaml

project:
  language: python
  framework: fastapi
  test_command: pytest
  build_command: poetry build
  lint_command: ruff check .

feedback_loop:
  mode: auto              # auto | semi-auto | manual
  max_iterations: 5
  auto_retry_delay: 5000  # ms

validation:
  method: test            # test | script | manual
  timeout: 120000         # ms

notification:
  method: system          # system | slack | none

agents:
  spec_validator: true
  test_oracle: true
  impl_reviewer: true
```

### get - 특정 설정 조회

```bash
/afl:config get feedback_loop.max_iterations
```

```
feedback_loop.max_iterations = 5
```

중첩된 키는 `.`으로 구분:

```bash
/afl:config get project.language
# project.language = python

/afl:config get agents
# agents:
#   spec_validator: true
#   test_oracle: true
#   impl_reviewer: true
```

### set - 설정 변경

```bash
/afl:config set feedback_loop.max_iterations 3
```

```
✅ 설정 변경됨
  feedback_loop.max_iterations: 5 → 3

저장됨: .claude/afl.yaml
```

여러 값 한 번에 변경:

```bash
/afl:config set feedback_loop.mode semi-auto
/afl:config set notification.method slack
```

### path - 설정 파일 경로

```bash
/afl:config path
```

```
.claude/afl.yaml
```

---

## 주요 설정 키

| 키 | 타입 | 기본값 | 설명 |
|----|------|--------|------|
| `project.language` | string | 자동감지 | 프로젝트 언어 |
| `project.test_command` | string | 자동감지 | 테스트 실행 명령어 |
| `feedback_loop.mode` | string | `auto` | 피드백 루프 모드 |
| `feedback_loop.max_iterations` | number | `5` | 최대 재시도 횟수 |
| `validation.timeout` | number | `120000` | 검증 타임아웃 (ms) |
| `notification.method` | string | `system` | 알림 방식 |
| `agents.spec_validator` | boolean | `true` | 스펙 검증 에이전트 활성화 |
| `agents.test_oracle` | boolean | `true` | 테스트 오라클 에이전트 활성화 |
| `agents.impl_reviewer` | boolean | `true` | 구현 검토 에이전트 활성화 |

---

## 설정 파일이 없을 때

```
❌ 설정 파일을 찾을 수 없습니다.

/afl:init 을 먼저 실행하거나
/afl:setup 으로 대화형 설정을 시작하세요.
```

---

## 잘못된 키/값

```bash
/afl:config set invalid.key value
```

```
❌ 알 수 없는 설정 키: invalid.key

사용 가능한 키 목록:
  /afl:config list
```

```bash
/afl:config set feedback_loop.mode invalid_mode
```

```
❌ 잘못된 값: invalid_mode

feedback_loop.mode 허용 값:
  - auto
  - semi-auto
  - manual
```

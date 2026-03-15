---
description: HITL 대기 목록 확인 + 대화형 응답
allowed-tools: ["AskUserQuestion", "Bash"]
---

# HITL 관리 (/hitl)

사람의 판단이 필요한 항목(HITL)을 목록으로 확인하고, 대화형으로 응답합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/hitl` — HITL 대기 목록 + 대화형 응답

## 실행

### Step 1: HITL 대기 목록 조회

```bash
autodev hitl list --json
```

### Step 2: 목록 출력

결과를 심각도 순으로 정렬하여 출력합니다:

```
🔔 HITL 대기 2건:

  1. [HIGH] org/repo-a PR #42 리뷰 3회 반복
     "동일한 피드백(에러 핸들링 누락)이 반복되고 있습니다."
     선택지: [재시도] [skip] [스펙 수정]

  2. [MED]  org/repo-b 스펙 충돌 감지
     "Payment Spec과 Refund Spec이 같은 디렉토리를 수정합니다."
     선택지: [Payment 우선] [Refund 우선] [수동 조정]
```

대기 항목이 없으면 "HITL 대기 항목이 없습니다."를 출력합니다.

### Step 3: 대화형 응답

목록 출력 후 사용자의 자연어 응답을 기다립니다.

사용자가 번호 또는 자연어로 응답하면 적절한 CLI 명령을 호출합니다:

**선택지 번호로 응답하는 경우**:

```bash
autodev hitl respond <hitl-id> --choice <N>
```

**자연어 메시지로 응답하는 경우** (방향 지시):

```bash
autodev hitl respond <hitl-id> --message "<사용자 메시지>"
```

**상세 정보가 필요한 경우**:

```bash
autodev hitl show <hitl-id> --json
```

응답 결과를 사용자에게 확인하여 출력합니다.

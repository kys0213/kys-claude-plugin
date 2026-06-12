# Stagnation Redirect (task 단위 정체 방어)

> `autopilot` skill 의 reference. autopilot 자율 모드에서 worker 가 같은 영역을
> 반복 시도하다 무한 루프에 빠지는 것을 막는 **task 단위** 방어 로직이다. (루프 단위 방어는
> `orchestrator` skill 의 `references/autonomous-driving.md` 가 담당 — 두 층위는 상호 보완.)

## 개요

autopilot 모드는 사람 개입 없이 task 를 dispatch 하므로, 같은 영역을 반복 시도하다 무한 루프에 빠질 위험이 있다.
이 reference 는 PreToolUse hook(`atelier autopilot hook protect-stagnation`, #776 으로 CLI 이전)이 ledger 기반 stagnation check 결과 JSON 을 해석하고, worker 가 task 를 claim 하기 직전에 **새로운 방향으로 진로를 재설정**하도록 안내한다.

진입 시점: worker 가 `autopilot task claim ...` 을 실행 → PreToolUse hook 이 인프로세스로 stagnation check(`autopilot check stagnation --task <id>` 와 동일 로직) 수행 → exit 4 (stagnation) 또는 5 (escalate) 이면 본 redirect 발동.

CLI 는 deterministic primitive (simhash hamming distance, path Jaccard) 만 담당하고, 이 redirect 가 후보 검증 / persona 매핑 / 진로 변경 prompt 합성을 책임진다.

## 1. 입력 — Ledger Stagnation JSON

CLI 가 stdout 으로 다음 형태의 JSON 을 출력한다 (`plans/ledger-stagnation-redesign.md` §3.10).

```json
{
  "status": "stagnation",
  "current_task": {
    "id": "abc123",
    "simhash": "0x...",
    "affected_paths": ["src/cmd/task.rs"]
  },
  "similar_tasks": [
    {
      "id": "def456",
      "title": "...",
      "similarity": {
        "simhash_distance": 2,
        "jaccard": 0.75,
        "shared_paths": ["src/cmd/task.rs"]
      },
      "outcome": "failed",
      "failure_reason": "test_compile_error",
      "completed_at": "2026-..."
    }
  ],
  "pattern": {
    "shared_paths": ["src/cmd/task.rs"],
    "common_failure_categories": ["test_compile_error"],
    "consecutive_failures": 3
  },
  "recommended_persona": "hacker"
}
```

| 필드 | 의미 |
|------|------|
| `status` | `"ok"` / `"stagnation"` / `"escalate"` 중 하나. CLI exit code 와 1:1 매핑 (0/4/5). |
| `current_task` | claim 직전인 task. simhash + affected_paths 동봉. |
| `similar_tasks` | hybrid (simhash distance ≤ T **또는** Jaccard ≥ J) 로 잡힌 후보. 각 항목에 distance / jaccard / shared_paths / outcome / failure_reason 이 들어 있다. |
| `pattern.shared_paths` | 모든 후보가 공통으로 가지는 path. 좁을수록 영역이 갇혀 있다는 신호. |
| `pattern.common_failure_categories` | 후보들이 공유하는 실패 카테고리. |
| `pattern.consecutive_failures` | 같은 카테고리로 연속 실패한 횟수. persona index 결정에 사용. |
| `recommended_persona` | CLI 가 결정적으로 추천한 persona (없으면 `null`). |

## 2. Haiku Verify 호출 가이드

simhash + Jaccard 는 후보 좁히기에는 유용하지만 false positive 를 완전히 제거하지 못한다.
후보가 충분히 모인 경우에만 Haiku 에게 "정말 같은 문제인가" 를 검증시킨다.

### 호출 여부 판단

| `similar_tasks.len()` | 동작 |
|-----------------------|------|
| 0 | status 가 stagnation 이 될 수 없으므로 본 스킬은 발동하지 않는다. |
| 1–2 | simhash + Jaccard 만으로 trust. Haiku 호출 생략. |
| 3+ | Haiku verify 호출 권장. false positive 비용보다 검증 비용이 더 작다. |

### Prompt 템플릿

```
System:
당신은 software task 의 유사도를 판단하는 검증자입니다.
"같은 문제 / 같은 영역 / 같은 접근법" 인 task 그룹만 골라야 합니다.
파일 경로가 겹친다는 이유만으로 같은 문제라고 판단하지 마세요.

User:
다음 N+1 개 task 중, current_task 와 정말 같은 문제인 task id 만 JSON 배열로 반환하세요.
출력 형식: {"verified_similar": ["id1", "id2"]}

current_task:
  id: abc123
  title: "..."
  affected_paths: ["..."]
  body: "..."

candidates:
  - id: def456
    title: "..."
    affected_paths: ["..."]
    failure_reason: "..."
  - id: ghi789
    ...
```

### 응답 처리

- 응답에서 `verified_similar` 만 사용한다. 다른 필드 / 자유 텍스트는 무시.
- false positive 로 분류된 task 는 stagnation 그룹에서 제외한다.
- 재평가: verified 그룹의 크기가 새로운 N 이 된다. N < 3 이면 stagnation 판정을 철회하고 worker 에게 진행 허용 메시지를 노출한다.
- N ≥ 3 이면 다음 단계 (persona 매핑 + redirect prompt) 로 진행한다.

> 본 epic 에서는 Haiku 호출의 실제 SDK 코드는 다루지 않는다. 위 prompt / 응답 처리는 가이드까지만이며, 실제 호출은 후속 epic 에서 구현한다.

## 3. Persona 매핑

PERSONAS 배열은 `cli/src/cmd/issue.rs:355` 의 정의를 그대로 따른다 (CLI 와 Skill 이 동일한 순서를 공유한다).

```
[ "hacker", "researcher", "simplifier", "architect", "contrarian" ]
```

`pattern.consecutive_failures` 횟수에 따라 index 를 결정한다.

| consecutive_failures | index | persona |
|----------------------|-------|---------|
| 2 | 0 | hacker |
| 3 | 1 | researcher |
| 4 | 2 | simplifier |
| 5 | 3 | architect |
| 6+ | 4 | contrarian (clamp) |

`recommended_persona` 가 채워져 있으면 그대로 사용한다. CLI 가 이미 동일 매핑을 적용한 결과이므로 재계산하지 않는다.

### Persona 행동 양식 (worker prompt 주입용)

| persona | 핵심 메시지 |
|---------|-------------|
| hacker | 제약 자체를 의심하라. "이 제약이 정말 필수인가" — 우회 경로 / 다른 진입점 탐색. |
| researcher | 추측을 멈추고 증거를 모아라. 에러 메시지를 다시 정독, 문서 / 소스 grep 으로 정확한 케이스 확인. |
| simplifier | 복잡성을 제거하라. YAGNI. "동작하는 가장 단순한 것" 을 먼저 만들고 점진 확장. |
| architect | 구조 자체를 의심하라. 같은 영역에서 반복 실패하면 추상화 누수가 원인일 수 있다. 최소 구조 변경 제안. |
| contrarian | 모든 가정을 뒤집어라. "아무것도 하지 않으면" / "당연한 해결책의 반대" 를 시도. |

## 4. Worker 진로 변경 가이드

stagnation 판정이 유지되면 hook 은 worker 가 보는 stderr 에 redirect prompt 를 노출한다 (spec §3.11). 본 스킬이 prompt 합성에 사용할 원칙은 다음과 같다.

### 4.1 기본 구조

```
[STAGNATION DETECTED] task <id>
This task's territory is exhausted — N similar tasks have failed before:
  - shared paths: <pattern.shared_paths>
  - shared failure category: <pattern.common_failure_categories> (<consecutive_failures> consecutive)

DO NOT proceed with the same approach. Try one of:
  1. <영역 변경 제안>
  2. Persona shift: "<persona>" — <핵심 메시지>

Recommended persona: <persona>
```

### 4.2 강조 규칙

- **shared_paths 가 좁다 (≤ 2 개)**: "이 영역 밖으로 시야 넓혀라. 호출 측 / 설정 / 인접 모듈을 의심하라" 를 추가.
- **common_failure_categories 가 같다**: "같은 진단 카테고리 (`<category>`) 가 반복된다. 다른 카테고리로 의심 범위를 옮겨라" 를 추가.
- **persona 가 결정되었다**: 해당 persona 의 행동 양식 (위 표) 을 그대로 prompt 에 주입한다. worker 가 persona 정의를 다시 추론하지 않도록 명시 노출.

### 4.3 worker prompt 에 들어갈 항목 체크리스트

- [ ] `pattern.shared_paths` 명시 (좁으면 "영역 밖으로" 강조)
- [ ] `pattern.common_failure_categories` 명시 (반복이면 "다른 카테고리 의심" 강조)
- [ ] persona 이름 + 핵심 메시지 + 탐색 질문 1–2 개
- [ ] 과거 실패 task id 목록 (`similar_tasks[].id`) — worker 가 ledger 에서 직접 조회 가능하도록

## 5. Escalate 단계 (status = "escalate")

CLI exit 5 인 경우 (`pattern.consecutive_failures` 가 N_esc=5 이상에 도달) 본 스킬은 **Haiku 검증을 생략**하고 즉시 사람 개입 요청 메시지를 노출한다.

- hook 은 escalate 를 자동 기록하지 **않는다** — `task escalate` 는 HITL issue 번호(`--issue <N>`)가
  필수이고, HITL issue 생성은 judgment 영역이라 hook(결정적 도구)이 수행하지 않는다 (CLAUDE.md "책임 경계").
- 본 스킬은 worker prompt 끝에 다음 라인을 추가한다.

```
ESCALATION REQUIRED — DO NOT retry.
HITL 이슈 생성 후 'atelier autopilot task escalate <id> --issue <N>' 으로 기록하고 사람의 지시를 기다리세요.
```

- persona shift 권유는 escalate 단계에서는 부수적이다. 메인 메시지는 "사람 개입 필요" 임을 분명히 한다.

## 6. 한계 / Out of Scope

본 epic (C10) 에서는 다음을 다루지 **않는다**. 후속 epic 으로 분리한다.

| 항목 | 비고 |
|------|------|
| Haiku 호출 코드 (SDK 클라이언트 / 비용 가드) | 본 스킬은 prompt 가이드까지. 실제 호출 흐름은 별도. |
| Mid-dispatch 강제 종료 | worker 가 작업 중일 때 ledger event 를 보고 `TaskStop` + 재dispatch 하는 흐름. 본 epic 은 pre-dispatch only (claim 직전 hook). |
| Simhash / Jaccard 알고리즘 교체 | Storage 옵션 B (단일 컬럼) 로 시작. 다중 알고리즘 동시 보관은 schema 확장이 필요할 때 재검토. |
| 기존 row 의 simhash / paths backfill | 정책상 수행하지 않음. 새 task 부터만 채워진다. |

## 7. 참조

- 스펙: `plans/ledger-stagnation-redesign.md` (§3.6 LLM 사용, §3.10 결과 JSON, §3.11 Hook prompt, §5 Acceptance)
- PERSONAS 배열 정의: `plugins/atelier/cli/src/cmd/issue.rs:355`
- Persona 결정 로직 (consecutive → index): 같은 파일 `:553`
- TaskEscalated event: `plugins/atelier/cli/src/domain/event.rs:55`
- 책임 경계 원칙: 레포 루트 `CLAUDE.md` "책임 경계 (CLI vs Skill/Agent)"

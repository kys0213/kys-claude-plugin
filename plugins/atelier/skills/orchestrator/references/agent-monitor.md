---
name: agent-monitor
description: 백그라운드 sub-agent/team 진행 추적, 정체/실패 시 보고 패턴, 재위임 판단 기준. 휴먼-인-더-루프(opt-out) 모드의 자동 개입 금지 규칙. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Agent Monitor

백그라운드로 위임한 sub-agent / agent team의 진행을 추적하고, 정체나 실패를 사용자에게 보고하는 패턴. **자동 개입은 하지 않는다** — 결정은 사용자가 한다.

> **적용 범위**: 이 문서의 "자동 개입 금지·보고 후 결정" 규칙은 사용자가 **휴먼-인-더-루프로 opt-out** 했을 때 적용된다. 오케스트레이터 **기본 동작은 자율 주행**이며(`autonomous-driving.md`), 자율 모드에서는 가드레일 안에서 자동 재위임·머지를 진행한다. 단 진행 추적·정체 감지·재위임 판단 기준 자체는 두 모드 공통으로 쓴다.

## 기본 원칙

```
정상 진행 중      → 침묵 (사용자에게 알리지 않음)
완료              → 결과 수령 후 다음 단계 진행
정체 / 실패       → 사용자에게 보고 + 결정 요청
                   (자동으로 SendMessage로 명령 주입 X)
```

자동 개입을 피하는 이유:
- LLM 판단으로 sub-agent에 명령을 주입하면 폭주/루프 위험
- 사용자가 의도와 다른 방향으로 진행되는 걸 모를 수 있음
- 보고 위주가 안전하고 추적 가능

---

## 백그라운드 위임 기본형

```
Agent({
  description: "...",
  prompt: "<자기완결>",
  run_in_background: true,
  isolation: "worktree",  # 필요 시
})
```

- `run_in_background: true`로 띄우면 메인은 다른 일 진행 가능
- 완료 시 자동 알림 — **sleep / poll 금지**
- 알림에 결과가 포함됨 → 메인이 수령 후 다음 단계

---

## 진행 상황 추적 (Monitor 도구)

장시간 백그라운드 작업의 stdout 스트림을 읽고 싶을 때:

```
Monitor({...})  # 백그라운드 프로세스의 라인별 알림
```

### 언제 사용
- 정말 긴 빌드/테스트 결과를 line-by-line으로 봐야 할 때
- 특정 패턴 (예: "ERROR")이 나타나면 즉시 보고하고 싶을 때

### 언제 사용 안 함
- 일반 sub-agent 위임 — `run_in_background: true`만으로 충분
- 단순 완료 대기 — 알림 자체가 도착함

대부분의 경우 Monitor는 불필요하다. 위임된 agent는 완료 알림으로 결과를 보낸다.

---

## Task 시스템으로 다중 작업 추적 (선택)

작업이 여러 개이고 **의존성·소유권·진행 상태**를 구조적으로 추적해야 할 때, ad-hoc 메모 대신 Task 시스템을 쓴다. 단순 1~2개 fan-out에는 불필요하다 (오버헤드만 큼 — Monitor와 같은 절제 원칙).

```
TaskCreate({subject: "auth 리팩토링", description: "<범위/검증 기준>"})  # pending으로 생성
TaskUpdate({taskId: "2", addBlockedBy: ["1"]})                          # 의존성: 1 완료 전 2 claim 불가
TaskUpdate({taskId: "1", owner: "<agent name>"})                        # 소유권(claim)
TaskList()                                                              # pending·미소유·non-blocked = 작업 가능
TaskGet({taskId: "1"})                                                  # 전체 요구사항/blockedBy 확인
TaskUpdate({taskId: "1", status: "completed"})                         # 완료 → blocked 작업 자동 해제
```

용도:

- **작업 분배**: 분해된 단위를 `TaskCreate`로 등록하고, dispatch한 agent를 `owner`로 표시한다.
- **의존성**: `addBlockedBy`로 순차 의존을 표현 — blocked task는 선행 완료 전 claim되지 않는다.
- **진행 추적**: `TaskList`로 pending/in_progress/completed 상태를 결정적으로 조회한다.

주의:

- Task 시스템은 **작업 상태**(분배·의존성·소유권)를 추적하는 것이지 agent의 **출력**을 추적하지 않는다. agent 결과는 여전히 **완료 알림 + Agent 결과**로 받는다.
- `TaskOutput`/`.output` 파일을 agent 추적에 쓰지 말 것 — deprecated이며, local agent의 `.output`은 전체 transcript symlink라 메인 컨텍스트를 넘치게 한다.

---

## SendMessage: 사용자 결정 후에만

```
SendMessage({to: "<agent_name>", message: "..."})
```

### 허용 케이스
- 사용자가 명시적으로 "그 agent에게 이렇게 전달해줘"라고 지시
- agent team 내에서 단계 전환 (예: reviewer 결과를 implementer에 전달) — 이미 계획된 흐름

### 금지 케이스
- 메인이 자체 판단으로 정체된 agent에 "다시 시도해줘" 보내기 → 폭주 위험
- 실패한 agent에 자동으로 수정 지시 보내기 → 사용자가 모르게 진행 방향 변경됨

원칙: **새 정보는 사용자에게서만 온다**. 메인이 만들어낸 지시를 sub-agent에 주입하지 않음.

---

## 정체 / 실패 감지

### 감지 신호

- 완료 알림이 매우 오래 안 옴 (사용자에게 진행 상황 확인 권유)
- 결과에 에러 메시지 포함
- worktree에 변경이 없는데 작업이 끝남 (실패 가능성)
- 빌드/테스트 실패

### 보고 형식

사용자에게 보고할 때 포함할 정보:

```
1. 어떤 작업이 어떤 상태인가
   - "task A (auth 리팩토링)"의 sub-agent가 빌드 실패로 종료"
2. 가능한 원인 (메인이 결과를 보고 추정한 내용)
3. 가능한 다음 액션 (선택지)
   - 같은 prompt로 재위임
   - prompt 수정 후 재위임
   - worktree 폐기 후 사용자가 직접 작업
4. 결정 요청
```

### no-op run 감지 (수령 검증)

편집 목적 agent의 완료 알림을 수령하면, 보고 내용만으로 성공을 단정하지 않고 해당 worktree에 **실제 변경이 존재하는지** 검증한다 — 예: `git diff --stat <base>...<worktree-branch>` 또는 결과에 포함된 커밋 해시의 존재 확인.

변경이 0건이면 성공으로 취급하지 않는다. 위 §감지 신호의 "worktree에 변경이 없는데 작업이 끝남"을 확정 신호로 보고 §재위임 판단 기준으로 회부한다 — 분류는 **prompt 결함** (검증 기준·범위가 모호해 agent가 아무것도 하지 않고 종료했을 가능성).

---

## fan-out 복원력 (checkpoint · retry · fallback)

대규모 병렬 fan-out(예: 15개 이상 agent)에서는 일부 agent가 504 Gateway Time-out·API 에러로 죽는 것을 정상 케이스로 전제하고 설계한다. 실패한 조각이 조용히 누락되면 최종 취합 리포트의 수치 일관성이 깨진다. SKILL.md 본문의 필수 규칙 4개(체크포인트·재시도·폴백·투명 보고)의 상세 절차는 이 섹션이 단일 출처다.

### 1. 체크포인트 — 결과는 완료 즉시 파일로

- 각 agent의 prompt에 **결과를 지정된 체크포인트 파일로 저장하는 마지막 단계**를 포함한다 (예: scratchpad의 `checkpoints/<task-id>.md`). 저장 주체는 agent 자신이다 — 메인의 Edit/Write 금지 원칙을 유지한다.
- 체크포인트 경로는 repo 밖(scratchpad 등)으로 지정해 worktree 정리·머지와 무관하게 보존한다.
- 메인은 완료 알림 수신 즉시 `Glob`/`Read`로 체크포인트 존재와 형식을 확인한다. **전체 완료를 기다렸다가 한꺼번에 수집하지 않는다** — 나중 agent의 실패가 앞선 성공 결과를 잃게 만들지 못하게 하는 장치다.
- 완료 알림은 왔는데 체크포인트가 없거나 비어 있으면 실패로 취급한다 (아래 재시도).

### 2. 재시도 — 일시적 인프라 오류는 N회

- 504 / gateway time-out / API 에러 등 **일시적 인프라 오류가 명백한 실패**는 같은 prompt로 재시도한다. 예산은 **agent당 기본 3회**다 (자율 계약에서 달리 정했으면 그 값).
- worktree는 재사용 불가이므로 재시도마다 새 `isolation:"worktree"` dispatch다 (아래 §재위임 판단 기준의 isolation 행과 동일).
- prompt 결함·도메인 판단이 원인인 실패는 이 예산의 대상이 아니다 — §재위임 판단 기준 표를 따른다 (prompt 수정 또는 보고).
- 재시도 이력(몇 번째 시도, 실패 사유)은 decision log와 최종 보고에 남긴다.

### 3. 폴백 — 재시도 소진 시에도 취합은 완성

재시도 예산을 소진한 조각은 버리지 않고 대체 경로로 채운다. **취합을 미완성 상태로 종료하지 않는다.**

```
❌ 재시도 소진 → 해당 조각 없이 취합 종료 (조용한 누락)
✅ 재시도 소진 → read-only 직접 분석 또는 조건 바꾼 재위임으로 채움 → 보고에 폴백 명시
```

- **read-only 분석·요약 수준의 조각** → 메인이 `Read`/`Glob`/`Grep`으로 직접 분석해 채운다 (읽기·보고는 메인의 허용 범위 — *사고 모드* 위반 아님).
- **편집이 필요한 조각** → 메인이 편집권을 회수하지 않는다. 조건을 바꿔(다른 tier/모델, 범위 축소, prompt 보강) **새 agent로 재위임**한다.
- 그래도 채울 수 없으면 해당 조각을 "미완(사유 포함)"으로 명시해 취합에 포함한다 — 조용한 누락이 아니라 명시적 구멍이어야 한다.

### 4. 투명 보고 — 실패·폴백을 최종 보고에 명시

최종 취합 보고에 agent(조각)별 상태 표를 포함한다 — "실패한 건도 누락 없이 보고" 원칙의 fan-out 구체화:

```
| 조각 | 상태 | 비고 |
|------|------|------|
| task-03 | 성공 | 1회 시도 |
| task-07 | 성공 (재시도 2회) | 504 × 2 |
| task-11 | 폴백 (메인 직접 분석) | 재시도 3회 소진 |
```

- 취합 수치(합계·통계)는 **체크포인트 파일 기준**으로 계산해 보고 수치와 실제 산출물의 불일치를 막는다.
- 폴백으로 채운 조각은 원 agent 산출과 신뢰도가 다를 수 있음을 함께 명시한다.

---

## 재위임 판단 기준

실패한 작업을 다시 위임할지, prompt를 수정할지, 사용자에게 넘길지 결정하는 기준.

| 실패 원인 추정 | 권장 액션 |
|--------------|----------|
| 외부 환경 (네트워크, 빌드 환경, 일시적 도구 오류) | 같은 prompt로 재위임 가능 |
| prompt 결함 (scope 과다, 검증 기준 누락, 모호한 지시) | prompt 수정 후 재위임 |
| 원인 불명확 또는 도메인 판단 필요 | 사용자에게 보고 (자동 재시도 금지) |
| isolation worktree에서 실패 | 같은 worktree에 재위임 불가 (worktree는 매 호출마다 새로 생성됨). 원인 분석 → prompt 수정 → 새 isolation worktree로 재위임, 또는 사용자 보고 |
| no-op run (편집 agent가 변경 0건으로 완료) | prompt 결함으로 분류 — 검증 기준·범위를 보강해 재위임 (§no-op run 감지) |

원칙:
- **자동 재시도 예산**: 일반 실패는 1회까지만 검토 (외부 환경 원인이 명백할 때). 단 fan-out에서 504/gateway/API 등 일시적 인프라 오류가 원인이면 위 §fan-out 복원력의 재시도 규칙(agent당 기본 3회)을 따른다. 예산 초과는 폴백 또는 보고.
- **불명확하면 항상 사용자에게**. 임의 추정으로 prompt 수정 → 본래 의도와 어긋날 위험.
- **재위임 시에도 자기완결성 유지**: 이전 실패 정보를 새 prompt에 포함 (어디까지 진행됐고 무엇이 실패했는지).
- **agentId로 재개**: 완료된 background agent는 `SendMessage({to: "<agentId>", ...})`로 컨텍스트를 유지한 채 다시 깨워 이어갈 수 있다 (spawn 결과의 agentId, 형식 `a...`). 직전 실패 맥락이 이미 그 agent에 남아 있으면 새 자기완결 prompt를 처음부터 짜는 비용을 던다. 단 외부 환경이 아니라 prompt 결함이 원인이면 새 위임이 더 깨끗하다.

---

## team 진행 추적

agent team(실험 플래그 `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` 필요)의 경우 여러 agent가 동시에 진행 중일 수 있다.

```
# team은 공유 checkout — 편집 격리 없음. 편집은 각 teammate가 isolated subagent로 위임.
Agent({name: "reviewer", run_in_background: true, ...})      # team_name은 무시됨
Agent({name: "implementer", run_in_background: true, ...})

# 진행 추적
- 각 agent의 완료 알림이 별개로 도착
- name으로 식별 가능 → 어떤 역할이 끝났는지 즉시 파악
- 한 agent가 다른 agent의 결과를 기다려야 할 때:
   * 미리 의존성을 명시한 prompt로 띄움 (reviewer 결과를 받아 처리하도록)
   * 또는 메인이 한 단계 완료 후 다음 단계를 호출 (순차)
```

team은 session 종료 시 **자동 정리**된다 (`TeamDelete` 없음 — 제거된 도구). 별도 정리 단계가 필요 없다.

---

## 안티패턴

1. **sleep + 폴링**: `Bash sleep 60 && check` 루프로 진행 상황 확인 → 금지. 알림이 자동으로 옴.
2. **메인의 자동 개입**: 정체 감지 → 자동으로 SendMessage로 "재시도해줘" 보냄 → 폭주 위험. 사용자에게 보고.
3. **모든 진행을 사용자에게 보고**: 정상 진행도 일일이 알림 → 노이즈. 정체/실패 시에만 보고.
4. **teammate에 편집 격리 기대**: team은 공유 checkout이라 worktree 격리가 없다 → teammate가 직접 편집하면 충돌/덮어쓰기. 편집은 `isolation:"worktree"` subagent로 위임.
5. **Monitor 남발**: 일반 위임에도 Monitor 도구 사용 → 불필요한 컨텍스트. `run_in_background`만으로 충분.

---

## 체크리스트

위임 직후:

- [ ] `run_in_background: true`로 띄웠는가?
- [ ] sleep/poll 루프를 짜고 있지 않은가?

진행 중:

- [ ] 정상 진행은 침묵하고 있는가?
- [ ] 정체/실패만 사용자에게 보고하는가?
- [ ] 자동으로 SendMessage를 보내려 하고 있지 않은가? (사용자 결정 우선)
- [ ] fan-out이면 각 agent 완료 즉시 체크포인트를 확인하고 있는가? (전체 완료 대기 금지)
- [ ] gateway/API 에러 실패는 재시도 예산(기본 3회) 안에서 재위임하고 있는가?
- [ ] 편집 목적 agent의 완료 수령 시 실제 diff 존재를 확인했는가? (§no-op run 감지)

종료 시:

- [ ] team을 사용했다면 편집을 teammate가 직접 하지 않고 isolated subagent로 위임했는가? (team은 session 종료 시 자동 정리됨)
- [ ] fan-out이면 최종 보고에 실패/재시도/폴백 상태 표를 포함했는가? 취합 수치는 체크포인트 기준인가?

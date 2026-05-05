---
name: agent-monitor
description: 백그라운드 sub-agent/team 진행 추적, 정체/실패 시 보고 패턴, 재위임 판단 기준. 자동 개입 금지. orchestrator 스킬 내부 참조 문서.
version: 0.1.0
user-invocable: false
---

# Agent Monitor

백그라운드로 위임한 sub-agent / agent team의 진행을 추적하고, 정체나 실패를 사용자에게 보고하는 패턴. **자동 개입은 하지 않는다** — 결정은 사용자가 한다.

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

## SendMessage: 사용자 결정 후에만

```
SendMessage({to: "<agent_name>", message: "..."})
```

### 허용 케이스
- 사용자가 명시적으로 "그 agent에게 이렇게 전달해줘"라고 지시
- agent team 내에서 단계 전환 (예: designer 결과를 implementer에 전달) — 이미 계획된 흐름

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

---

## 재위임 판단 기준

실패한 작업을 다시 위임할지, prompt를 수정할지, 사용자에게 넘길지 결정하는 기준.

| 실패 원인 추정 | 권장 액션 |
|--------------|----------|
| 외부 환경 (네트워크, 빌드 환경, 일시적 도구 오류) | 같은 prompt로 재위임 가능 |
| prompt 결함 (scope 과다, 검증 기준 누락, 모호한 지시) | prompt 수정 후 재위임 |
| 원인 불명확 또는 도메인 판단 필요 | 사용자에게 보고 (자동 재시도 금지) |
| isolation worktree에서 실패 | 같은 worktree에 재위임 불가 (worktree는 매 호출마다 새로 생성됨). 원인 분석 → prompt 수정 → 새 isolation worktree로 재위임, 또는 사용자 보고 |

원칙:
- **자동 재시도는 1회까지만** 검토 (외부 환경 원인이 명백할 때). 그 이상은 보고.
- **불명확하면 항상 사용자에게**. 임의 추정으로 prompt 수정 → 본래 의도와 어긋날 위험.
- **재위임 시에도 자기완결성 유지**: 이전 실패 정보를 새 prompt에 포함 (어디까지 진행됐고 무엇이 실패했는지).

---

## team 진행 추적

agent team의 경우 여러 agent가 동시에 진행 중일 수 있다.

```
TeamCreate({name: "feature-x"})
Agent({team_name: "feature-x", name: "designer", run_in_background: true, ...})
Agent({team_name: "feature-x", name: "implementer", run_in_background: true, ...})

# 진행 추적
- 각 agent의 완료 알림이 별개로 도착
- name으로 식별 가능 → 어떤 역할이 끝났는지 즉시 파악
- 한 agent가 다른 agent의 결과를 기다려야 할 때:
   * 미리 의존성을 명시한 prompt로 띄움 (designer 결과를 받아 처리하도록)
   * 또는 메인이 designer 완료 후 implementer를 호출 (순차)
```

team 정리:

```
TeamDelete({name: "feature-x"})  # 작업 완료 후
```

team을 정리하지 않으면 식별자가 남아 다음 작업과 충돌할 수 있다.

---

## 안티패턴

1. **sleep + 폴링**: `Bash sleep 60 && check` 루프로 진행 상황 확인 → 금지. 알림이 자동으로 옴.
2. **메인의 자동 개입**: 정체 감지 → 자동으로 SendMessage로 "재시도해줘" 보냄 → 폭주 위험. 사용자에게 보고.
3. **모든 진행을 사용자에게 보고**: 정상 진행도 일일이 알림 → 노이즈. 정체/실패 시에만 보고.
4. **team 미정리**: 작업 끝나도 `TeamDelete` 안 함 → 식별자 충돌 / 리소스 누수.
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

종료 시:

- [ ] team을 사용했다면 TeamDelete로 정리했는가?

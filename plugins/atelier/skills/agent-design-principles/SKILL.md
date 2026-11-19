---
name: agent-design-principles
description: Claude Code 확장 설계 원칙 — Command·Skill·Rule 세 컴포넌트로 워크플로우를 구성하고 custom agent 정의는 만들지 않는다. 새 command/skill/rule 을 만들거나 리뷰할 때 참조하는 컴포넌트 선택 기준·레이어 매핑·안티패턴 가이드
version: 2.0.0
user-invocable: false
---

# Agent Design Principles Skill

> 레이어드 아키텍처에 익숙한 개발자가 Claude Code 확장을 바라보는 방법
> ref: https://toss.tech/article/software-3-0-era

새 Command, Skill, Rule 을 만들거나 기존 구성을 리뷰할 때 참조하는 설계 원칙입니다.
좋은 설계의 원칙(응집도, 결합도, 추상화)은 Software 3.0 에서도 그대로 유효합니다. 바뀐 것은 그 원칙을 담을 **컴포넌트의 종류**입니다.

---

## 1. 핵심: 세 컴포넌트 + 도구 계층

워크플로우는 **Command · Skill · Rule** 세 가지로 구성합니다. Custom agent 정의(`agents/*.md`)는 만들지 않습니다 (§2).

```
┌──────────────────────────────────────────────────────────────┐
│  사용자 ──/cmd──▶  Command  (user-only 진입점)                │
│                     │ 인자 파싱 → 위임                        │
│  모델 ──맥락──▶     ▼                                         │
│                   Skill  (절차·전문성·진입점)                 │
│                     │   context: fork → 격리 실행             │
│                     │   (built-in subagent 위에서)            │
│                     ▼                                         │
│        CLI · 스크립트 · MCP  (결정적 도구 / Adapter)          │
│                                                              │
│  Rule ── CLAUDE.md · .claude/rules/ (paths) ── 자동 주입     │
│          모든 레이어의 작업에 컨벤션을 강제                   │
└──────────────────────────────────────────────────────────────┘
```

| | Command | Skill | Rule |
|---|---|---|---|
| 레이어 비유 | Controller | Service + Domain | Cross-cutting (Aspect) |
| 발동 주체 | 사용자만 (`/name`) | 모델 맥락 판단 · 사용자 · 상위 컴포넌트 | 파일 경로(`paths`) · 세션 시작 시 자동 |
| 컨텍스트 점유 | 호출 시에만 | description 은 **매 턴**, 본문은 호출 시 | 매칭되는 동안 본문 전체 |
| 담는 것 | 인자 파싱, 위임 대상 지정 | 절차, 판단 기준, 전문성(암묵지) | 프로젝트 컨벤션, 경계 원칙 |
| 실행 컨텍스트 | 메인 대화 | 메인 대화 또는 fork (격리) | — (지식만 주입) |

**도구 계층**(CLI·스크립트·MCP)은 컴포넌트가 아니라 컴포넌트가 호출하는 결정적 도구입니다. 판단은 Skill, 변환은 도구 — 경계는 `tool-layer-boundary` 원칙을 따릅니다.

---

## 2. Custom Agent 를 만들지 않는 이유

과거 agent 정의 파일이 주던 가치는 세 가지였고, 지금은 모두 Skill 이 제공합니다.

| agent 정의가 주던 것 | 지금의 대체 수단 |
|---|---|
| 별도 컨텍스트(격리 실행) | Skill frontmatter `context: fork` (+ `agent: general-purpose \| Explore \| Plan`) |
| model · effort 지정 | Skill frontmatter `model`, `effort` — fork 시 subagent 에 적용 |
| 도구 제한 | `disallowed-tools` 로 제거, 읽기 전용이면 `agent: Explore` |
| 재사용 가능한 프롬프트 | Skill 본문 + `references/` |

여기에 agent 정의를 따로 두면 비용만 남습니다:

- **이중 출처**: 같은 절차가 agent 본문과 skill 에 나뉘어 따로 늙습니다 (agent 가 `skills:` 로 skill 을 끌어와도 역할 서술은 agent 에 남음).
- **실행 위치 고정**: agent 는 항상 별도 컨텍스트에서만 돕니다. Skill 은 메인 대화에서 바로 쓰다가, 격리가 필요할 때만 fork 로 돌릴 수 있습니다.
- **컨텍스트 점유**: agent description 도 skill description 처럼 상시 목록에 올라갑니다. 같은 일을 두 목록에 올리는 셈입니다.

### 런타임 위임은 여전히 쓴다

"agent 정의를 만들지 않는다" 는 **파일**에 대한 원칙입니다. 실행 중에 Agent 도구로 subagent 를 띄우는 것(병렬 fan-out, 대량 탐색 격리)은 그대로 씁니다:

- built-in 타입(`general-purpose`, `Explore`, `Plan`)을 띄우고, prompt 에 따를 skill 과 입출력 계약을 적습니다.
- 띄울 때의 model · effort 배정 기준과 재평가 절차는 `orchestrator` skill 이 정합니다.

```
❌ agents/code-analyzer.md 를 새로 만들고 command 에서 subagent_type 으로 호출
✅ skills/code-analysis/SKILL.md (context: fork, agent: Explore) 를 만들고 command 가 위임
✅ 병렬이 필요하면 Agent 도구로 Explore 를 N 개 띄우고 prompt 에 "code-analysis skill 을 따르라"
```

### 이미 있는 agent 정의

새로 추가하지 않습니다. 기존 정의는 동작을 유지하며 형식 규칙(`plugin-agent` rule)대로 유지보수하고, 본문을 크게 고칠 일이 생기면 그때 fork skill 로 전환할지 검토합니다.

---

## 3. Command = Controller (user-only 진입점)

**원칙**: 사용자가 명시적으로 부를 때만 실행돼야 하는 진입점. 로직을 직접 구현하지 않습니다.

### Command 로 둘지의 기준

**"모델이 맥락을 보고 자동 실행해도 되는가?"** 하나로 가릅니다.

- 안 된다 → Command. 비용이 크거나, 되돌리기 어렵거나, 명시적 셋업인 것. 예: setup, update.
- 된다 → user-invocable Skill (§4).

> Claude Code 에서 Command 는 Skill 로 통합되었습니다. `commands/<name>.md` 와 `skills/<name>/SKILL.md` + `disable-model-invocation: true` 는 같은 user-only 진입점입니다. 이 프로젝트는 user-only 진입점을 `commands/` 에 둬서 디렉터리만 보고 발동 주체를 알 수 있게 합니다. Command 가 `references/` 같은 보조 파일이 필요할 만큼 커지면 그것은 위임할 skill 이 필요하다는 신호입니다.

### 체크리스트

- [ ] 사용자 인자(`$ARGUMENTS`) 파싱이 명확한가?
- [ ] 판단·절차는 Skill 에, 결정적 작업은 CLI 에 위임하는가?
- [ ] `allowed-tools` 가 실제로 쓰는 도구만 사전 승인하는가?
- [ ] 같은 명령을 여러 번 실행해도 안전한가? (멱등성)

---

## 4. Skill = Service + Domain

**원칙**: 하나의 Skill 은 하나의 관심사를 다룹니다. 절차(Service)와 전문성(Domain)을 함께 담을 수 있고, 필요하면 격리 실행됩니다.

### 4.1 Skill 의 네 가지 역할

| 역할 | frontmatter | 쓰임 | 예 |
|---|---|---|---|
| 진입점 Skill | `user-invocable: true` (기본) | 사용자 `/name` + 모델 자동 발동. 도메인이 맞으면 모델이 알아서 꺼내야 하는 것 | git, grill, spec-write, workflow, orchestrator |
| Protocol Skill | `user-invocable: false` | 단독 호출은 무의미. 상위 command/skill 이 디스패치하는 절차 본문 | — |
| Reference Skill | `user-invocable: false` | 단독으로도 의미 있는 durable 전문성. 맥락이나 rule 포인터로 on-demand 로드 | 이 skill 자체 |
| Forked Skill | `context: fork` (+ `agent`) | 격리 컨텍스트가 필요한 작업. 과거 agent 정의의 자리 | 대량 파일 분석, 독립 리뷰 |

Forked Skill 은 호출한 쪽과 컨텍스트를 공유하지 않습니다. 그래서 **입력 형식·출력 형식·실패 케이스**를 본문에 명시합니다 (과거 agent 명세의 입출력 계약이 이리로 옮겨옵니다).

### 4.2 frontmatter 선택 가이드

| 필요 | 필드 |
|---|---|
| 사용자만 호출 (모델 자동 발동 금지) | `disable-model-invocation: true` — description 도 상시 목록에서 빠짐 |
| 모델만 호출 (`/` 메뉴에서 숨김) | `user-invocable: false` — description 은 여전히 상시 점유 |
| 격리 실행 | `context: fork`, 실행 타입은 `agent` |
| 역량 지정 | `model`, `effort` (fork 가 아니면 그 턴에만 적용) |
| 도구 | `allowed-tools` 는 사전 **승인**일 뿐 제한이 아님. 제한은 `disallowed-tools` |
| 특정 파일 작업 시에만 자동 활성 | `paths` |

### 4.3 Skill 폭발 경고

모든 Skill 의 description 은 매 턴 시스템 프롬프트에 올라갑니다. SRP 를 맹목적으로 따르면 Class Explosion 이 나듯, Skill 도 과도하게 쪼개면 컨텍스트를 낭비합니다.

```
"이 Skill 이 2개 이상의 Command/Skill 에서 참조되는가?"
  → Yes: 분리 유지
  → No:  호출하는 쪽에 인라인

"이 Skill 이 다루는 관심사가 둘 이상인가?"
  → Yes: 분리 고려 (SRP 위반 신호)
  → No:  인라인이 더 효율적일 수 있음
```

- 줄 수 자체는 분리 기준이 아닙니다. 관심사가 하나면 길어도 유지하고, 세부는 `references/` 로 내려 필요할 때만 읽게 합니다 (progressive disclosure). SKILL.md 길이 제약은 CI `validate` 가 별도로 강제합니다.
- description 은 앞부분에 핵심 사용 맥락을 둡니다. 목록에서는 길이 제한으로 잘립니다.

---

## 5. Rule = 항상 적용되는 컨벤션

**원칙**: 작업할 때마다 늘 지켜야 하는 것은 모델의 판단에 맡기지 않고 자동 주입합니다.

- **CLAUDE.md**: 프로젝트 전역 원칙 (design-first, SOLID, 품질 게이트 등). 세션 시작 시 항상 로드.
- **`.claude/rules/*.md`**: `paths` 에 걸리는 파일을 다룰 때 주입되는 레이어별 컨벤션. `paths` 가 없으면 항상 로드.

### 5.1 Skill vs Rule 경계 — 암묵지 vs 컨벤션

| | Skill | Rule |
|---|---|---|
| 본질 | 코드·문서만으로는 알 수 없는 **외부 지식·전문성(암묵지)** — "전문가는 이럴 때 이렇게 판단한다" | 이 프로젝트에 종속적이고 **언젠가 바뀌거나 사라질 컨벤션** |
| 수명 | 프로젝트와 무관하게 durable | 프로젝트와 함께 늙고 사라짐 |
| 발동 | 모델이 **로드할지 판단** (on-demand) | 경로에 걸리면 **자동 주입** (강제) |
| 예 | spec 갭 분석법, 위임·병렬화 판단, 설계 교리 | 커밋 형식, 디렉터리 컨벤션, 의존성 정책 |

판단 한 줄: **"코드를 읽어서 나오는가?"** 나오면 컨벤션(rule), 안 나오는 전문성이면 skill.

**항상 적용되는 지식은 skill 이 아닙니다.** 코드를 쓸 때마다 적용돼야 하는 원칙은 on-demand 로드 대상이 아니라 CLAUDE.md / rule 에 둡니다. Skill 슬롯은 *가끔* 맥락에서 끌어오는 지식에만 씁니다.

### 5.2 전문성이 강제 컨벤션으로 굳을 때 — 두 층 분리

하나로 합치지 않습니다:

- **skill = 전문성("왜")의 단일 출처**. 교본·예제·판단 기준은 여기에만 둡니다.
- **rule = 이 repo 의 얇은 바인딩("무엇을")**. `paths` 로 강제하되 skill 을 **참조만** 하고 재서술하지 않습니다.

> ❌ skill 의 교본을 rule 에 압축 복붙 → 두 곳이 따로 늙어 어긋납니다. rule 은 "이 skill 을 따른다 + repo 특화 바인딩" 만 담습니다.

### 5.3 Rule 작성 시

- `paths` 는 위치(컨테이너)가 아니라 역할(레이어)을 표현합니다.
- 본문은 현재 구현 스냅샷이 아니라 역할·계약·불변식을 서술합니다.
- 동적 정보(이슈 번호, 오늘의 우선순위)는 넣지 않습니다. 그건 대화로 전달합니다.

---

## 6. 컴포넌트 선택 결정 트리

```
이 지식/절차는 작업할 때마다 항상 적용돼야 하는가?
  ├─ Yes → Rule (전역이면 CLAUDE.md, 레이어별이면 .claude/rules/ + paths)
  └─ No  → 판단 없이 동일 입력 → 동일 출력인가?
            ├─ Yes → CLI / 스크립트 (Skill·Command 가 호출)
            └─ No  → Skill
                      ├─ 사용자만 발동해야 하는가? → Command (얇게, Skill 에 위임)
                      ├─ 메인 컨텍스트를 오염시킬 만큼 읽거나 독립 판단이 필요한가?
                      │    → context: fork
                      └─ 그 외 → 메인 대화에서 실행되는 Skill
```

어느 갈래에도 "agent 정의 파일" 은 없습니다.

---

## 7. 도구 계층: CLI · 스크립트 · MCP

- **CLI · 스크립트**: 판단이 필요 없는 작업(훅 등록, 설정 편집, fingerprint 계산)은 LLM 이 매번 구조를 해석하게 두지 말고 캡슐화합니다. Skill 은 인자를 정하고 결과를 해석합니다.
- **MCP = Adapter**: 외부 시스템(API, DB, SaaS)과의 인터페이스를 표준화된 도구로 감쌉니다.

---

## 8. 토큰 관리

전통 서버에서 RAM 을 걱정하듯, 에이전트에서는 토큰을 걱정합니다.

1. **경로만 수집 → 격리 컨텍스트가 읽기**: 메인 대화는 Glob 으로 경로만 모으고, 실제 읽기는 fork skill 이나 Agent 도구로 띄운 subagent 가 합니다. 메인에는 결론만 돌아옵니다.
2. **결정적 로직은 스크립트로**: 프롬프트로 절차를 설명하는 대신 CLI 를 호출합니다.
3. **상시 점유를 줄인다**: description 은 짧고 핵심부터. 사용자만 부르는 skill 은 `disable-model-invocation: true` 로 목록에서 뺍니다.
4. **CLAUDE.md 는 정적 원칙만**: 기술 스택·빌드·컨벤션처럼 잘 안 변하는 것만 둡니다.

---

## 9. 안티패턴

| 안티패턴 | 증상 | 처방 |
|---|---|---|
| **Fat Command** | Command 에 판단·절차가 직접 들어 있음 | Skill 로 옮기고 Command 는 인자 파싱·위임만 |
| **Agent 신설** | 격리·model 지정을 위해 `agents/*.md` 를 새로 만듦 | `context: fork` skill + frontmatter `model`/`effort` |
| **God Skill** | 하나의 skill 이 여러 관심사를 다룸 | 관심사별로 분리, 공통 지식은 Reference Skill 로 |
| **Skill Explosion** | 한 곳에서만 쓰는 조각 skill 이 여럿 | 호출하는 쪽에 인라인하거나 관련 skill 통합 |
| **Convention in Skill** | 항상 지켜야 할 컨벤션을 on-demand skill 에 둠 | Rule 로 내림 |
| **Rule 복붙** | skill 교본을 rule 에 재서술 | rule 은 skill 참조 + repo 바인딩만 |
| **Chatty Context** | CLAUDE.md·rule 에 동적 정보 | 정적 원칙만 남기고 나머지는 대화로 |
| **LLM 에 결정적 로직** | 판단 없는 변환을 매번 LLM 이 해석 | CLI 로 캡슐화 |

---

## 10. HITL (Human-in-the-Loop)

모든 것을 미리 정의하기보다 git · package.json 등으로 환경을 자동 감지하고, 애매한 순간에만 묻습니다. 되돌리기 어려운 행위(설정 덮어쓰기, 외부 전송)는 Command 로 두거나 실행 전 확인을 받습니다.

---

## 11. 새 워크플로우 체크리스트

```
□ 컴포넌트가 맞는가? (§6 결정 트리)
  - 항상 적용돼야 하는 지식을 skill 에 두진 않았는가?
  - 모델이 자동 발동해도 되는 것을 command 로 막아두진 않았는가?
  - 새 agent 정의 파일을 만들고 있진 않은가?

□ 레이어가 맞는가?
  - Command 가 로직을 직접 구현하고 있진 않은가?
  - Skill 이 결정적 변환을 직접 구현하고 있진 않은가?

□ 격리가 필요한가?
  - 필요하면 context: fork 이고, 입출력 계약·실패 케이스가 본문에 있는가?

□ 도구 권한이 최소인가?
  - allowed-tools 가 실제로 쓰는 도구만 승인하는가?
  - 제한이 필요하면 disallowed-tools 나 agent: Explore 를 썼는가?

□ 역량 배정이 적절한가?
  - 단순 작업에 강한 model·높은 effort 를 고정하고 있진 않은가?

□ 토큰을 낭비하고 있진 않은가?
  - 메인 대화에서 대량 파일을 직접 읽고 있진 않은가?
  - description 이 짧고 핵심부터인가?

□ Skill 이 적정 수준으로 분리되었는가?
  - 2곳 이상에서 재사용되는가?
  - 과도한 분리로 컨텍스트를 낭비하고 있진 않은가?
```

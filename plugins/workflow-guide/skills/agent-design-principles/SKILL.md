---
name: agent-design-principles
description: 레이어드 아키텍처 기반 에이전트 설계 원칙 - Skill, Sub-agent, Slash Command 구조화 가이드
---

# Agent Design Principles Skill

> 레이어드 아키텍처에 익숙한 개발자가 Claude Code를 바라보는 방법
> ref: https://toss.tech/article/software-3-0-era

이 스킬은 새로운 Slash Command, Sub-agent, Skill을 만들거나 리뷰할 때 참조하는 설계 원칙을 제공합니다.

---

## 핵심 원칙: 레이어드 아키텍처 매핑

Software 3.0 시대에도 좋은 설계의 원칙(응집도, 결합도, 추상화)은 유효합니다.

```
┌─────────────────────────────────────────────────────────────┐
│                    에이전트 아키텍처                          │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  Slash Command (Controller)                         │   │
│  │  사용자 요청의 진입점. 입력 파싱 → Sub-agent 위임   │   │
│  └──────────────────────┬──────────────────────────────┘   │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────────┐   │
│  │  Sub-agent (Service Layer)                          │   │
│  │  여러 Skill을 조합하여 워크플로우 완성              │   │
│  │  별도 Context = 별도 스레드                         │   │
│  └──────────────────────┬──────────────────────────────┘   │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────────┐   │
│  │  Skill (Domain / Repository)                        │   │
│  │  단일 책임 원칙(SRP). 재사용 가능한 기능 단위       │   │
│  └──────────────────────┬──────────────────────────────┘   │
│                         │                                   │
│  ┌──────────────────────▼──────────────────────────────┐   │
│  │  MCP (Adapter)                                      │   │
│  │  외부 시스템 인터페이스. Adapter Pattern             │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 구성 요소별 설계 가이드

### 1. Slash Command = Controller

**원칙**: 진입점 역할만 수행. 로직을 직접 구현하지 않음.

Spring의 `@RestController`, Express의 `router.get()`처럼 Slash Command는 사용자 요청의 진입점입니다.

#### 체크리스트

- [ ] 사용자 인자(`$ARGUMENTS`) 파싱이 명확한가?
- [ ] 비즈니스 로직 없이 Sub-agent/Skill에 위임하는가?
- [ ] `allowed-tools`가 최소한인가?
- [ ] 같은 명령을 여러 번 실행해도 안전한가? (멱등성)

#### allowed-tools 가이드

| 패턴 | 권장 도구 | 설명 |
|------|----------|------|
| Sub-agent 위임 | `Task`, `Glob` | Glob으로 경로 수집, Task로 위임 |
| 직접 실행 | `Bash`, `Read` | 스크립트 실행 + 결과 확인 |
| 사용자 확인 필요 | `AskUserQuestion` | 실행 전 확인 |

---

### 2. Sub-agent = Service Layer

**원칙**: 여러 Skill을 조합하여 워크플로우를 완성. 각 Sub-agent는 별도 Context를 가짐.

Service 계층이 여러 Repository와 Domain 객체를 조율하듯이, Sub-agent는 Skill을 조합합니다.

#### model 선택 기준

| 작업 유형 | model | 예시 |
|----------|-------|------|
| 복잡한 추론, 설계 판단 | `opus` | 아키텍처 설계, 복잡한 코드 생성 |
| 코드 분석, 리뷰, 구현 | `sonnet` | 코드 리뷰, 일반 구현, 리팩토링 |
| 단순 분류, 파싱, 변환 | `haiku` | 파일 분류, 포맷 변환, 간단한 추출 |

#### tools 최소 권한 원칙

```yaml
# 리뷰 Agent: 읽기만 필요
tools: ["Read", "Glob", "Grep"]

# 구현 Agent: 쓰기도 필요
tools: ["Read", "Write", "Bash", "Glob", "Grep"]

# 오케스트레이션 Agent: 위임만
tools: ["Task", "Glob"]
```

---

### 3. Skill = SRP (단일 책임 원칙)

**원칙**: 하나의 Skill은 하나의 관심사만 다룸.

#### Skill 폭발 경고

Claude는 시작 시 **모든 Skill의 메타데이터**(name/description)를 시스템 프롬프트에 로드합니다.

```
Skill 5개 → 시스템 프롬프트에 5개 description 상주 (적정)
Skill 20개 → 시스템 프롬프트에 20개 description 상주 (과다)
```

전통 아키텍처에서 SRP를 맹목적으로 따르면 Class Explosion이 발생하듯, Skill도 과도하게 분리하면 Context를 낭비합니다.

#### 적정 분리 판단 기준

```
"이 Skill이 2개 이상의 Command/Agent에서 참조되는가?"
  → Yes: 분리 유지
  → No: 해당 Command/Agent에 인라인

"이 Skill의 내용이 200줄을 넘는가?"
  → Yes: 분리 고려
  → No: 인라인이 더 효율적일 수 있음
```

---

### 4. MCP = Adapter Pattern

**원칙**: 외부 시스템과의 인터페이스를 캡슐화.

외부 API, CLI 도구, 데이터베이스 등과의 통신을 표준화된 인터페이스로 감쌈.

---

## 토큰 관리 (Memory Management)

전통 서버에서 RAM을 걱정하듯, 에이전트에서는 토큰을 걱정해야 합니다.

### 3가지 핵심 전략

#### 전략 1: Glob 경로만 수집 → Sub-agent가 읽기

```
MainAgent (토큰 절약):
  Glob("src/**/*.ts") → ["src/a.ts", "src/b.ts", ...]
  Task(prompt="다음 파일을 리뷰해줘: src/a.ts, src/b.ts")

SubAgent (별도 Context):
  Read("src/a.ts") → 파일 내용
  Read("src/b.ts") → 파일 내용
  → 리뷰 결과 반환
```

#### 전략 2: 결정적 로직은 스크립트로 분리

판단이 필요 없는 작업은 셸 스크립트나 CLI로 캡슐화합니다.

```bash
# Bad: LLM이 매번 컨벤션을 해석하며 토큰 소비
"브랜치명은 feat/ 접두사를 쓰고, 소문자 kebab-case로..."

# Good: 스크립트가 컨벤션을 캡슐화
bash scripts/create-branch.sh feat my-feature
# LLM은 결과만 확인
```

#### 전략 3: CLAUDE.md는 정적 원칙만

```markdown
# CLAUDE.md에 넣어야 할 것 (정적, 잘 안 변함)
- 기술 스택: TypeScript, Bun, Hono
- 빌드: bun run build
- 테스트: bun test
- 컨벤션: Conventional Commits

# CLAUDE.md에 넣으면 안 되는 것 (동적, 자주 변함)
- 현재 작업 중인 이슈 번호
- 오늘의 우선순위
- 디버깅 중인 에러 메시지
→ 이런 정보는 대화로 전달하거나 Sub-agent Context로 넘기세요
```

---

## 안티패턴 목록

### 1. Fat Controller (뚱뚱한 Command)

```yaml
# Bad: Command에 모든 로직이 들어있음
---
name: review
allowed-tools: ["Read", "Write", "Bash", "Glob", "Grep", "Task"]
---
Step 1: 파일을 읽는다
Step 2: 분석한다
Step 3: 결과를 쓴다
Step 4: 테스트를 실행한다
...
```

**해결**: Sub-agent로 위임, Command는 진입점만

### 2. God Agent (만능 Agent)

```yaml
# Bad: 하나의 Agent가 모든 것을 담당
---
name: do-everything
model: opus
tools: ["Read", "Write", "Bash", "Glob", "Grep", "Task"]
---
```

**해결**: 역할별로 Agent 분리, 적절한 model 선택

### 3. Skill Explosion (스킬 폭발)

```
# Bad: 과도하게 잘게 쪼갠 Skill
skills/
  validate-email/SKILL.md
  validate-phone/SKILL.md
  validate-name/SKILL.md
  validate-address/SKILL.md
```

**해결**: 관련된 것은 하나의 Skill로 (`input-validation/SKILL.md`)

### 4. Chatty Context (수다스러운 컨텍스트)

```markdown
# Bad: CLAUDE.md에 모든 것을 담으려 함
현재 이슈: #123 로그인 버그
어제 회의 결과: API 변경 예정
TODO: 리팩토링 필요한 파일 목록...
```

**해결**: 정적 원칙만 CLAUDE.md에, 나머지는 대화로

### 5. LLM에게 결정적 로직 위임

```yaml
# Bad: 브랜치 이름 규칙을 매번 LLM이 해석
"브랜치명은 type/description 형식이고, type은 feat, fix, refactor 중 하나..."

# Good: 스크립트로 캡슐화
bash scripts/create-branch.sh $TYPE $DESCRIPTION
```

---

## HITL (Human-in-the-Loop)

모든 것을 미리 정의하려 하기보다는, 애매한 부분은 묻게 두는 접근을 고려하세요.

```yaml
# 에이전트가 자동 감지하되, 불확실한 순간에만 질문
- 환경 자동 감지: git, package.json 등으로 프로젝트 파악
- 애매할 때 질문: "TypeScript와 JavaScript 모두 있는데 어떤 것을 기준으로 할까요?"
- 확정적 작업 자동 수행: lint, test, build 등
```

---

## 새 워크플로우 체크리스트

새로운 Command, Agent, Skill을 만들기 전에 확인하세요:

```
□ 레이어가 맞는가?
  - Command가 로직을 직접 구현하고 있진 않은가?
  - Sub-agent가 단순 파싱만 하고 있진 않은가?

□ 도구 권한이 최소인가?
  - allowed-tools/tools에 불필요한 도구가 있진 않은가?

□ model이 적절한가?
  - 단순 작업에 opus를 쓰고 있진 않은가?

□ 토큰을 낭비하고 있진 않은가?
  - MainAgent에서 파일을 직접 읽고 있진 않은가?
  - 결정적 로직을 프롬프트로 설명하고 있진 않은가?

□ Skill이 적정 수준으로 분리되었는가?
  - 2곳 이상에서 재사용되는가?
  - 과도한 분리로 Context를 낭비하고 있진 않은가?
```

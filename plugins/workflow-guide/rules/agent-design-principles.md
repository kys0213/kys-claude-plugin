---
paths:
  - ".claude/commands/**/*.md"
  - ".claude/agents/**/*.md"
  - ".claude/skills/**/*.md"
  - "commands/**/*.md"
  - "agents/**/*.md"
  - "skills/**/*.md"
---

# Agent Design Principles

> 레이어드 아키텍처에 익숙한 개발자를 위한 에이전트 설계 원칙
> ref: https://toss.tech/article/software-3-0-era

## 레이어드 아키텍처 매핑

에이전트 구성 요소는 전통적인 레이어드 아키텍처와 1:1로 대응됩니다.
새로운 Skill, Sub-agent, Slash Command를 만들 때 이 매핑을 기준으로 설계하세요.

| 에이전트 구성 요소 | 레이어드 아키텍처 | 역할 |
|---|---|---|
| **Slash Command** | Controller | 사용자 요청의 진입점. 입력을 파싱하고 적절한 Sub-agent에게 위임 |
| **Sub-agent** | Service Layer | 여러 Skill을 조합하여 워크플로우를 완성. 별도 Context로 독립 동작 |
| **Skill** | Domain / Repository | 단일 책임 원칙(SRP)을 따르는 재사용 가능한 기능 단위 |
| **MCP** | Adapter | 외부 시스템과의 인터페이스. Adapter Pattern 적용 |
| **CLAUDE.md** | Config (package.json) | 잘 변하지 않는 프로젝트 원칙만 기록 |

---

## Slash Command 설계 원칙 (Controller)

1. **진입점 역할만 수행**: 비즈니스 로직을 직접 구현하지 않음
2. **입력 파싱과 위임**: 사용자 인자를 파싱하고 적절한 Sub-agent/Skill에 위임
3. **allowed-tools 최소화**: 필요한 도구만 명시적으로 허용
4. **멱등성 고려**: 같은 명령을 여러 번 실행해도 안전하게 설계

```yaml
# Good: 역할이 명확한 Command
---
name: review
description: 코드 리뷰를 실행합니다
allowed-tools: ["Task", "Glob"]  # 최소한의 도구
---
# Step 1: 파일 수집 (Glob)
# Step 2: Sub-agent에 위임 (Task)
# Step 3: 결과 취합
```

```yaml
# Bad: Command에 로직이 직접 구현됨
---
name: review
allowed-tools: ["Read", "Write", "Bash", "Glob", "Grep", "Task", ...]  # 모든 도구
---
# 파일을 직접 읽고, 분석하고, 결과를 쓰고...
```

---

## Sub-agent 설계 원칙 (Service Layer)

1. **Skill 조합자**: 여러 Skill을 조합하여 하나의 워크플로우를 완성
2. **독립적 Context**: 각 Sub-agent는 별도의 Context Window를 가짐 (별도 스레드)
3. **model 선택 기준**:
   - `opus`: 복잡한 추론, 설계 판단이 필요한 경우
   - `sonnet`: 일반적인 코드 분석, 리뷰
   - `haiku`: 단순 분류, 파싱, 변환
4. **tools 최소 권한**: 필요한 도구만 부여

```yaml
# Good: 역할이 명확하고 model이 적절한 Agent
---
name: code-reviewer
model: sonnet        # 코드 분석에 적합
tools: ["Read", "Glob", "Grep"]  # 읽기 전용
---
```

```yaml
# Bad: 과도한 권한의 Agent
---
name: helper
model: opus          # 단순 작업에 opus는 낭비
tools: ["Read", "Write", "Bash", "Glob", "Grep", "Task"]  # 모든 권한
---
```

---

## Skill 설계 원칙 (Domain / SRP)

1. **단일 책임**: 하나의 Skill은 하나의 관심사만 다룸
2. **재사용성**: 여러 Command/Agent에서 참조 가능하도록 설계
3. **Skill 폭발 주의**: Claude는 시작 시 모든 Skill 메타데이터를 로드함
   - Skill이 20개면 20개의 description이 항상 Context를 점유
   - 과도한 분리는 Class Explosion과 같은 문제를 야기
4. **정적 지식만 포함**: 동적으로 변하는 정보는 대화로 전달

```
# Good: 응집도 높은 Skill
skills/
  auth-patterns/SKILL.md        # 인증 관련 패턴
  error-handling/SKILL.md       # 에러 처리 컨벤션

# Bad: 과도하게 분리된 Skill (Class Explosion)
skills/
  jwt-validation/SKILL.md
  jwt-refresh/SKILL.md
  jwt-revocation/SKILL.md
  session-create/SKILL.md
  session-validate/SKILL.md
  session-expire/SKILL.md
```

---

## 토큰 관리 원칙 (Memory Management)

전통 서버에서 RAM을 관리하듯, 에이전트에서는 토큰을 관리해야 합니다.

### Context Window 소비 구조

```
Context Window (200K tokens)
├── System Prompt (고정)
├── CLAUDE.md (고정)
├── Skill descriptions (고정) ← Skill 수에 비례
├── 대화 히스토리 (누적)
├── MCP 응답 (가변)
└── 파일 내용 (가변)
```

### 최적화 전략

1. **Glob으로 경로만 수집, 내용은 Sub-agent가 읽기**
   ```
   MainAgent: Glob → 파일 경로 목록
   SubAgent: Read → 실제 파일 내용 (별도 Context)
   ```

2. **결정적 로직은 스크립트로 분리**
   ```bash
   # Bad: LLM이 컨벤션을 매번 해석
   "브랜치명은 feat/xxx 형식으로..."

   # Good: 스크립트가 캡슐화
   bash scripts/create-branch.sh feat my-feature
   ```

3. **CLAUDE.md에는 정적 원칙만**
   - 기술 스택, 코딩 컨벤션, 빌드 명령어 등
   - 현재 작업 이슈, 오늘의 우선순위 등은 대화로 전달

---

## 안티패턴 체크리스트

새로운 워크플로우를 만들 때 아래 항목을 확인하세요:

- [ ] Slash Command가 직접 로직을 구현하고 있지 않은가? (Controller에 Service 로직)
- [ ] Sub-agent에 불필요하게 높은 model을 사용하고 있지 않은가?
- [ ] Skill이 과도하게 분리되어 있지 않은가? (Class Explosion)
- [ ] allowed-tools / tools에 불필요한 도구가 포함되어 있지 않은가?
- [ ] CLAUDE.md에 동적으로 변하는 정보를 넣고 있지 않은가?
- [ ] LLM이 판단할 필요 없는 결정적 로직을 프롬프트로 설명하고 있지 않은가?
- [ ] 파일 내용을 MainAgent에서 직접 읽고 있지 않은가? (토큰 낭비)

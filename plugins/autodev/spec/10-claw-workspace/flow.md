# Flow 10: Claw 워크스페이스 설정

### 시나리오

사용자가 Claw의 판단 방식, 브랜치 전략, 리뷰 정책 등을 자연어로 커스터마이즈한다.

### 핵심 아이디어

Claw는 **자체 Claude 워크스페이스**에서 실행된다. 이 워크스페이스에 CLAUDE.md, rules, skills를 배치하면 Claw의 행동이 자연어로 정의된다. 하드코딩된 설정 대신 **Claude Code의 확장 시스템을 그대로 활용**.

```
룰 기반 (하드코딩):
  branch_strategy: "feat/{issue-number}-{short-desc}"
  → 변경하려면 코드 수정

자연어 기반 (워크스페이스):
  .claude/rules/branch-naming.md:
    "브랜치명은 feat/{이슈번호}-{짧은설명} 형식으로 만든다.
     hotfix인 경우 hotfix/{이슈번호}를 사용한다."
  → 파일 수정만으로 동작 변경
```

### 워크스페이스 구조

```
~/.autodev/claw-workspace/
├── CLAUDE.md                        ← Claw의 판단 원칙 + 세션 시작 안내
├── .claude/
│   └── rules/
│       ├── scheduling.md            ← 스케줄링 정책
│       ├── branch-naming.md         ← 브랜치 네이밍 전략
│       ├── review-policy.md         ← 리뷰 정책
│       ├── decompose-strategy.md    ← 스펙 분해 전략
│       └── hitl-policy.md           ← HITL 판단 기준
├── commands/                        ← Slash Commands (사용자 진입점)
│   ├── status.md                    ← /status — 전체 상태 요약
│   ├── board.md                     ← /board [repo] — 칸반 보드
│   ├── hitl.md                      ← /hitl — HITL 대기 목록 + 응답
│   ├── spec.md                      ← /spec <action> — 스펙 관리
│   ├── repo.md                      ← /repo <action> — 레포 관리
│   └── decisions.md                 ← /decisions — Claw 판단 이력
└── skills/
    ├── decompose/SKILL.md           ← 스펙 → 이슈 분해 스킬
    ├── gap-detect/SKILL.md          ← gap 탐지 방법론
    └── prioritize/SKILL.md          ← 우선순위 판단 스킬
```

### CLAUDE.md (Claw 판단 원칙)

```markdown
# Claw 판단 원칙

## 역할
나는 Claw, 자율 개발 스케줄러다.
매 틱마다 큐 전체 상태를 보고 어떤 작업을 진행할지 판단한다.

## 핵심 원칙
1. 독립적인 이슈는 병렬 진행한다
2. 같은 파일을 수정하는 이슈는 순차 처리한다
3. 리뷰가 3회 반복되면 HITL을 요청한다
4. 스펙의 acceptance criteria를 항상 참조한다
5. gap을 발견하면 즉시 이슈를 생성한다

## 판단 시 참고
- 스펙 문서의 아키텍처 섹션을 기준으로 이슈 간 의존성을 판단한다
- 테스트 환경 정의를 기준으로 검증 가능 여부를 판단한다
```

### Rules 예시

**branch-naming.md**:
```markdown
# 브랜치 네이밍 규칙

- 기능 구현: `feat/{이슈번호}-{짧은-설명}`
- 버그 수정: `fix/{이슈번호}-{짧은-설명}`
- 리팩토링: `refactor/{이슈번호}-{짧은-설명}`
- 설명은 영문 소문자, 하이픈 구분, 5단어 이내

예시:
- `feat/42-jwt-middleware`
- `fix/45-token-expiry-check`
```

**review-policy.md**:
```markdown
# 리뷰 정책

- 코드 변경이 100줄 이하면 단일 리뷰어로 충분
- 100줄 이상이면 multi-LLM 리뷰 (/code-review) 사용
- 보안 관련 변경은 항상 HITL 요청
- 테스트 커버리지가 낮아지는 변경은 HITL 요청
```

**decompose-strategy.md**:
```markdown
# 스펙 분해 전략

- 하나의 이슈는 하나의 모듈/파일 그룹만 수정한다
- 인터페이스 정의 이슈를 먼저 생성한다
- 구현 이슈는 인터페이스 이슈에 의존성을 건다
- 테스트 이슈는 구현 이슈와 병렬로 생성한다
- 이슈 하나의 예상 작업량은 PR 1개 = 파일 5개 이내를 목표한다
```

### 레포별 오버라이드

글로벌 워크스페이스 위에 레포별 오버라이드가 가능:

```
~/.autodev/claw-workspace/              ← 글로벌 (기본 정책)
~/.autodev/workspaces/org-repo/claw/    ← 레포별 오버라이드

머지 순서:
  글로벌 CLAUDE.md + 레포별 CLAUDE.md
  글로벌 rules/ + 레포별 rules/ (동일 파일명이면 레포별 우선)
  글로벌 skills/ + 레포별 skills/
```

이 구조는 기존 autodev의 config deep-merge 패턴과 동일.

### 규칙 충돌 해소

다중 레포에서 규칙이 충돌하는 경우:

```
우선순위 (높은 순):
  1. 레포별 오버라이드 (~/.autodev/workspaces/org-repo/claw/)
  2. 글로벌 claw-workspace (~/.autodev/claw-workspace/)
  3. 대상 레포의 .claude/rules/ (레포 자체 규칙)
```

- **동일 파일명**: 레포별 > 글로벌 (완전 대체, 머지 아님)
- **파일명이 다른 규칙**: 모두 로드 (누적)
- **규칙 간 모순**: Claw가 판단 시 감지하면 HITL 요청
  - "브랜치 규칙과 리뷰 규칙이 충돌합니다. 확인해주세요."

### 설정 CLI

```bash
# Claw 워크스페이스 초기화 (기본 rules/skills 생성)
autodev claw init

# 레포별 오버라이드 추가
autodev claw init --repo org/repo

# 현재 적용 중인 규칙 확인 (글로벌 + 레포별 머지 결과)
autodev claw rules [--repo org/repo]

# 규칙 편집 (에디터 열기)
autodev claw edit branch-naming [--repo org/repo]
```

### 세션 시작

`autodev agent` 실행 시 CLAUDE.md의 세션 시작 안내가 표시된다:

```
$ autodev agent

🦀 Claw — 자율 개발 에이전트

등록된 레포:
  org/repo-a  Auth Module v2 [Active] 3/5 (60%)  HITL: 1건
  org/repo-b  Payment Gateway [Active] 1/4 (25%)  HITL: 1건
  org/repo-c  (스펙 없음, Issue 모드)

명령어:
  /status              전체 상태 요약
  /board [repo]        칸반 보드 (전체 또는 레포별)
  /hitl                HITL 대기 목록 + 대화형 응답
  /spec list           스펙 목록
  /spec status <id>    스펙 진행도 상세
  /repo list           레포 목록
  /repo show <name>    레포 상세
  /decisions [repo]    최근 Claw 판단 이력
  /claw rules [repo]   현재 적용 규칙 확인
  /claw edit <rule>    규칙 편집
  /help                전체 명령어 목록

  (레포 Claude 세션 전용)
  /add-spec [file]     스펙 등록 (대화형)
  /update-spec <id>    스펙 수정 (대화형)

또는 자연어로 대화하세요:
  "repo-a의 HITL 처리해줘"
  "auth 스펙 진행 상황 알려줘"
  "repo-b 브랜치 전략을 conventional로 바꿔줘"
>
```

### Slash Commands

slash command는 내부적으로 `autodev` CLI를 호출한다.
사용자는 `--repo`, `--json` 같은 플래그를 몰라도 자연어로 사용 가능.

| Command | 내부 동작 | 예시 |
|---------|----------|------|
| `/status` | `autodev status --json` → 자연어 요약 | `/status` |
| `/board` | `autodev queue list --json` → 칸반 포맷 | `/board repo-a` |
| `/hitl` | `autodev hitl list --json` → 목록 + 선택지 | `/hitl` |
| `/spec list` | `autodev spec list --json` | `/spec list` |
| `/spec status X` | `autodev spec status X --json` | `/spec status auth-v2` |
| `/decisions` | `autodev spec decisions --json` | `/decisions repo-a` |

### 자연어 + CLI 혼용

사용자는 slash command와 자연어를 자유롭게 혼용할 수 있다:

```
> /hitl
HITL 대기 2건:
  1. [HIGH] org/repo-a PR #42 리뷰 3회 반복
  2. [MED]  org/repo-b 스펙 충돌 감지

> 1번 재시도해줘
→ autodev hitl respond hitl-01 --choice 1 실행
→ "PR #42를 재시도합니다."

> repo-b의 충돌 상세히 알려줘
→ autodev hitl show hitl-02 --json 실행
→ "repo-b에서 Payment Spec과 Refund Spec이 src/payment/ 디렉토리를
   동시에 수정하려 합니다. 우선순위를 정해주시겠어요?"

> Payment 먼저 해줘
→ autodev hitl respond hitl-02 --message "Payment Spec 우선 진행"
→ "Payment Spec을 우선 처리하고, Refund Spec은 대기합니다."
```

### Claw 실행 시 워크스페이스 적용

```
autodev agent 실행 시:
  1. ~/.autodev/claw-workspace/ 경로에서 claude 실행
  2. CLAUDE.md + rules + skills + commands 자동 로드
  3. 세션 시작 시 등록된 레포 + 스펙 상태 + HITL 대기 목록 표시
  4. 사용자가 slash command 또는 자연어로 상호작용
  5. Claw가 내부적으로 autodev CLI를 도구로 호출하여 처리
```

### 왜 자연어인가

```
YAML 설정:
  branch_naming:
    pattern: "{type}/{number}-{desc}"
    types: [feat, fix, refactor]
    desc_max_words: 5
  → 예외 케이스마다 필드 추가 필요
  → "hotfix는 다르게" → hotfix_pattern 필드 추가...

자연어 규칙:
  "브랜치명은 feat/{번호}-{설명} 형식.
   단, hotfix인 경우 hotfix/{번호}를 사용하고
   release 브랜치에서 분기할 때는 release/{버전}/{번호}를 사용한다."
  → LLM이 맥락을 이해하고 적용
  → 새 예외 = 문장 하나 추가
```

# barrier-sync 플러그인 설계

## 요구사항 정리

### 핵심 문제
background Task 여러 개를 병렬 실행 후, 전원 완료를 **LLM 턴 소모 없이** 감지하는 방법이 없다.
현재 방법: output_file을 주기적으로 Read → 턴 낭비 + polling 비효율.

### 해결 방법
FIFO 기반 barrier 패턴:
- **Producer** (SubagentStop hook): Task 완료 시 FIFO에 write
- **Consumer** (background Bash): FIFO에서 blocking read → CPU 0%, 턴 0
- 전원 완료 감지 후 stdout으로 결과 반환 → 메인 에이전트가 Read

### 플러그인으로 제공할 가치
1. 비자명한 패턴을 패키징하여 재사용 가능하게 만듦
2. hook 등록, barrier 실행, 결과 수집을 skill 문서로 LLM이 자동 참조
3. `/barrier-setup` 커맨드로 hook 등록 자동화

---

## 사이드이펙트 조사

### 1. 기존 hooks와의 충돌

현재 `.claude/settings.json` 등록된 hooks:
- PreToolUse: `Write|Edit` → default-branch-guard + develop-phase-gate
- PreToolUse: `Bash` → default-branch-guard-commit + develop-phase-gate
- PostToolUse: `Write|Edit` → cargo-check

**SubagentStop hook은 현재 등록된 것이 없다** → 충돌 없음.

단, 사용자가 이 플러그인을 설치할 때 `settings.local.json`에 SubagentStop hook을 수동 등록해야 한다.
→ `/barrier-setup` 커맨드에서 `git-utils hook register` 또는 직접 JSON 편집으로 자동화 가능.

### 2. plugin.json의 hooks 필드

Claude Code 플러그인 스펙에서 `plugin.json`에 hooks 배열을 선언하면 플러그인 설치 시 자동 등록될 수 있다.
단, 현재 프로젝트의 기존 플러그인 중 hooks를 plugin.json에 선언한 예가 없다 (모두 settings.json에서 직접 관리).
→ 일관성을 위해 `/barrier-setup` 커맨드 방식 채택.

### 3. validation 도구 호환성

`tools/validate/` Go 검증기가 체크하는 항목:
- `plugin.json`: name(kebab-case), version(semver), commands 파일 존재 여부
- `marketplace.json`: name, source, version
- `SKILL.md`: frontmatter(name, description), 본문 50~500줄
- `commands/*.md`: frontmatter(description)
- `agents/*.md`: frontmatter(description)
- `hooks/*.md`: frontmatter(name, description), event 유효성
- `scripts/*.sh`: sensitive data 검출

→ 각 파일이 이 규칙을 따라야 CI가 통과함.

### 4. `/tmp` 사용

barrier가 `/tmp/claude-barriers/` 에 FIFO와 메타데이터를 생성.
- trap EXIT로 자동 클린업 → 잔여 파일 문제 없음
- 동시 실행 시 BARRIER_ID로 격리 → 충돌 없음

### 5. 플랫폼 제약

- macOS: 정상 동작 (POSIX FIFO 지원)
- Linux: 정상 동작
- Windows: 미지원 (named pipe 비호환) → README에 명시

---

## 설계

### 플러그인 구조

```
plugins/barrier-sync/
├── .claude-plugin/
│   └── plugin.json
├── commands/
│   └── barrier-setup.md          # /barrier-setup — hook 등록 자동화
├── hooks/
│   └── signal-done.sh            # SubagentStop hook (producer)
├── scripts/
│   └── wait-for-tasks.sh         # barrier (consumer)
└── skills/
    └── barrier-sync/
        └── SKILL.md              # LLM 참조용 사용법 가이드
```

### 각 파일 상세 설계

#### 1. `plugin.json`

```json
{
  "author": { "name": "kys0213" },
  "name": "barrier-sync",
  "description": "FIFO-based barrier pattern for parallel background Task synchronization",
  "version": "0.1.0",
  "commands": ["./commands/barrier-setup.md"],
  "skills": ["./skills"]
}
```

#### 2. `signal-done.sh` (SubagentStop Hook — Producer)

**책임**: Task 완료 이벤트를 수신하여 FIFO에 기록

**입력**: stdin으로 SubagentStop hook JSON
```json
{
  "agent_id": "ad7baaa",
  "agent_type": "general-purpose",
  "last_assistant_message": "검토 결과...",
  "session_id": "...",
  "hook_event_name": "SubagentStop"
}
```

**동작**:
1. stdin에서 JSON 파싱 (jq)
2. `/tmp/claude-barriers/` 하위에서 활성 barrier 검색 (meta.json의 pid가 살아있는 것)
3. 활성 barrier가 없으면 즉시 exit 0 (barrier 미사용 세션에 영향 없음)
4. agent_id + last_assistant_message 앞 500자를 결과 파일에 저장
5. FIFO에 agent_id를 write (blocking이 아닌 순간 write)

**엣지 케이스**:
- jq 미설치: 기본 grep/sed로 fallback
- 활성 barrier 없음: silent exit
- FIFO write 실패: stderr에 경고, exit 0 (hook이 에이전트를 차단하면 안 됨)

#### 3. `wait-for-tasks.sh` (Barrier — Consumer)

**책임**: 지정된 수의 완료 신호를 FIFO에서 blocking read

**인터페이스**:
```bash
BARRIER_ID="my-barrier" bash wait-for-tasks.sh <expected_count> [timeout_sec]
```

**동작**:
1. BARRIER_ID 결정 (환경변수 or `barrier-$$`)
2. `/tmp/claude-barriers/{BARRIER_ID}/` 디렉토리 생성
3. `mkfifo pipe` → FIFO 생성
4. `meta.json` 작성 (pid, fifo_path, expected_count)
5. `results/` 디렉토리 생성
6. timeout 타이머 시작 (background subshell)
7. loop: `read < pipe` × expected_count회
8. 전원 도착 → stdout에 결과 출력
9. trap EXIT → 클린업 (디렉토리 전체 삭제)

**stdout 형식**:
```
--- BARRIER COMPLETE (5/5) ---
agents: editor(abc) continuity(def) ...

=== editor-abc ===
[last_assistant_message 앞 500자]

=== continuity-def ===
[last_assistant_message 앞 500자]
```

**타임아웃 시** (exit 1):
```
--- BARRIER TIMEOUT (300s) 3/5 completed ---
agents: editor(abc) continuity(def) unifier(ghi)

=== editor-abc ===
[부분 결과]
```

#### 4. `SKILL.md`

LLM이 barrier-sync를 사용해야 하는 상황에서 자동으로 참조하는 문서.

**frontmatter**:
```yaml
name: barrier-sync
description: Use this skill when running 2+ background Tasks in parallel and need to wait for all to complete without consuming LLM turns. Provides FIFO-based barrier synchronization.
allowed-tools: Bash, Read, Task
```

**본문 구성**:
1. 개요 — 언제 사용하는지
2. 사전 조건 — `/barrier-setup` 으로 hook 등록 필요
3. 사용 패턴 — 3단계 (barrier 시작 → Task 실행 → 결과 Read)
4. 파라미터 레퍼런스
5. 출력 형식 설명
6. 주의사항 (ordering, timeout, BARRIER_ID 충돌 방지)
7. 예시 시나리오 2-3개

#### 5. `barrier-setup.md` (커맨드)

**frontmatter**:
```yaml
description: barrier-sync 플러그인의 SubagentStop hook을 등록합니다
allowed-tools:
  - Bash
  - Read
  - Write
  - AskUserQuestion
```

**동작**:
1. 현재 hook 등록 상태 확인 (settings.json / settings.local.json 읽기)
2. SubagentStop hook이 이미 등록되어 있으면 안내 후 종료
3. 미등록이면 등록 범위 선택 (프로젝트 / 사용자)
4. settings에 SubagentStop hook 추가
5. jq 설치 여부 확인, 미설치 시 안내

### marketplace.json 등록

```json
{
  "category": "infrastructure",
  "description": "FIFO 기반 barrier 패턴 — 병렬 background Task 동기화 (턴 소모 없이 전원 완료 대기)",
  "keywords": [
    "barrier",
    "sync",
    "parallel",
    "background",
    "task",
    "fifo",
    "hook"
  ],
  "name": "barrier-sync",
  "source": "./plugins/barrier-sync",
  "version": "0.1.0"
}
```

---

## 구현 순서

### Phase 1: 스크립트 구현
1. `scripts/wait-for-tasks.sh` — barrier consumer
2. `hooks/signal-done.sh` — hook producer

### Phase 2: 플러그인 메타
3. `.claude-plugin/plugin.json`
4. `commands/barrier-setup.md`
5. `skills/barrier-sync/SKILL.md`

### Phase 3: 레지스트리 등록
6. `.claude-plugin/marketplace.json`에 barrier-sync 추가

### Phase 4: 검증
7. validation 통과 확인 (`make validate`)

---

## 변경하지 않는 것

- `.claude/settings.json`: 글로벌 설정은 건드리지 않음 (사용자가 `/barrier-setup`으로 선택)
- 기존 플러그인 코드: 의존성 없음, 독립 플러그인
- Go validation 도구: 기존 규칙만으로 검증 가능

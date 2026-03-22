---
description: Claw 대화형 세션 — 상태 조회, 큐 관리, 판단 이력, HITL, 규칙 편집
argument-hint: "[query or subcommand]"
allowed-tools: ["AskUserQuestion", "Bash", "Read", "Edit"]
---

# Claw 대화형 세션 (/claw)

Claw 워크스페이스의 상태 조회, 큐 관리, 판단 이력 확인, HITL 응답, 규칙 편집을 자연어 세션으로 통합합니다.

> v4에서 `/status`, `/board`, `/decisions`, `/hitl`, `/repo`, `/claw`, `/cron`으로 분리되어 있던 기능을 통합합니다.
> 자연어 질의를 파싱하여 적절한 autodev CLI 명령을 호출합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

| 입력 형태 | 설명 | v4 대응 |
|----------|------|---------|
| `/claw` (인자 없음) | 전체 상태 요약 + 대화 세션 시작 | `/status` |
| `/claw rules [--repo <name>]` | 현재 적용 규칙 확인 | `/claw rules` |
| `/claw edit <rule> [--repo <name>]` | 규칙 편집 | `/claw edit` |
| `/claw <자연어 질의>` | 자연어로 조회/조작 | 아래 매핑 참조 |

### 자연어 질의 매핑

Claw는 사용자의 자연어 질의를 파싱하여 적절한 CLI 명령으로 변환합니다.

| 자연어 예시 | 내부 호출 | v4 대응 |
|------------|----------|---------|
| "현재 상태 알려줘" | `autodev status --json` | `/status` |
| "보드 보여줘", "칸반" | `autodev queue list --json` | `/board` |
| "repo-a 보드" | `autodev queue list --json --repo repo-a` | `/board repo-a` |
| "판단 이력", "decisions" | `autodev spec decisions --json -n 20` | `/decisions` |
| "HITL 대기 목록" | `autodev hitl list --json` | `/hitl` |
| "레포 목록" | `autodev repo list --json` | `/repo list` |
| "repo-a 상세" | `autodev repo show repo-a --json` | `/repo show repo-a` |
| "cron 목록" | `autodev cron list --json` | `/cron list` |
| "cron 일시정지 <name>" | `autodev cron pause <name>` | `/cron pause` |
| "cron 재개 <name>" | `autodev cron resume <name>` | `/cron resume` |
| "cron 즉시 실행 <name>" | `autodev cron trigger <name>` | `/cron trigger` |

## 실행

### Step 0: CLI 바이너리 확인

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
```

실패 시 바이너리 설치/업데이트를 안내하고 중단합니다.

### Step 1: 서브커맨드 분기

인자를 파싱합니다:
- `rules` → **rules** 섹션으로 분기
- `edit` → **edit** 섹션으로 분기
- 인자 없음 → **세션 시작** (전체 상태 요약 + 대화)
- 그 외 → **자연어 질의** 처리

---

## 세션 시작 (인자 없음)

전체 상태를 수집하고 요약한 뒤, 대화형 세션을 시작합니다.

### Session Step 1: 상태 수집

```bash
autodev status --json
autodev hitl list --json
autodev queue list --phase failed --json
```

### Session Step 2: 요약 표시

수집된 정보를 자연어로 정리하여 출력합니다:

```
autodev 상태

데몬: 실행 중 (PID: 12345, uptime: 2h 30m)

등록된 레포:
  org/repo-a  Auth Module v2 [Active] 3/5 (60%)  HITL: 1건
  org/repo-b  Payment Gateway [Active] 1/4 (25%)
  org/repo-c  (스펙 없음, Issue 모드)

HITL 대기: 1건
  [HIGH] org/repo-a PR #42 리뷰 3회 반복

최근 활동:
  10분 전  org/repo-a #43 → done
  25분 전  org/repo-b #51 → implementing
```

데몬이 중지 상태면 `/auto start`로 시작하라고 안내합니다.

### Session Step 3: 대화 세션

상태 요약 후 사용자의 후속 질의를 자연어로 처리합니다.
각 질의에 대해 자연어 매핑 테이블을 참고하여 적절한 CLI 명령을 호출하고, 결과를 자연어로 응답합니다.

---

## 자연어 질의 처리

사용자의 자연어 입력을 분석하여 해당하는 기능을 실행합니다.

### 상태 조회 (v4 `/status`)

```bash
autodev status --json
```

레포별 스펙 정보도 수집합니다:

```bash
autodev repo list --json
autodev spec list --json --repo <name>
autodev hitl list --json
```

### 칸반 보드 (v4 `/board`)

```bash
autodev queue list --json [--repo <repo>]
```

수집된 큐 아이템을 상태별로 분류하여 칸반 보드 형식으로 출력합니다:

```
칸반 보드 — org/repo-a

  Pending        Ready          Running         Done
  #45 Error      #46 Missing    #44 Session     #42 JWT middle
      handling       tests          adapter         ware
                                                #43 Token API

진행도: 2/5 (40%)  |  HITL 대기: 0건  |  실패: 0건
```

### 판단 이력 (v4 `/decisions`)

```bash
autodev spec decisions --json -n 20
```

시간순으로 정렬하여 출력합니다:

```
Claw 판단 이력 (최근 20건):

  시각              레포           동작        근거
  03-15 14:30      org/repo-a     advance     #44 구현 완료, 리뷰 단계로 전이
  03-15 14:25      org/repo-a     decompose   Auth v2 스펙 → 이슈 5개 분해
  03-15 14:20      org/repo-b     hitl        #51 리뷰 3회 반복, 사람 확인 필요
  03-15 14:15      org/repo-a     skip        #43 세션 어댑터 불필요 (스펙 변경)
```

### HITL 관리 (v4 `/hitl`)

```bash
autodev hitl list --json
```

대기 항목을 심각도 순으로 출력합니다:

```
HITL 대기 2건:

  1. [HIGH] org/repo-a PR #42 리뷰 3회 반복
     "동일한 피드백(에러 핸들링 누락)이 반복되고 있습니다."
     선택지: [재시도] [skip] [스펙 수정]

  2. [MED]  org/repo-b 스펙 충돌 감지
     "Payment Spec과 Refund Spec이 같은 디렉토리를 수정합니다."
     선택지: [Payment 우선] [Refund 우선] [수동 조정]
```

대기 항목이 없으면 "HITL 대기 항목이 없습니다."를 출력합니다.

**대화형 HITL 응답:**

사용자가 번호 또는 자연어로 응답하면 적절한 CLI 명령을 호출합니다:

- 선택지 번호로 응답:
  ```bash
  autodev hitl respond <hitl-id> --choice <N>
  ```

- 자연어 메시지로 응답 (방향 지시):
  ```bash
  autodev hitl respond <hitl-id> --message "<사용자 메시지>"
  ```

- 상세 정보 필요 시:
  ```bash
  autodev hitl show <hitl-id> --json
  ```

### 레포 조회 (v4 `/repo`)

#### 목록

```bash
autodev repo list --json
```

```
등록된 레포:

  이름            URL                                  스펙    상태
  org/repo-a      https://github.com/org/repo-a       2개     Active
  org/repo-b      https://github.com/org/repo-b       1개     Active
  org/repo-c      https://github.com/org/repo-c       없음    Issue 모드
```

#### 상세

```bash
autodev repo show <name> --json
```

```
org/repo-a

  URL: https://github.com/org/repo-a
  로컬 경로: /Users/me/repos/repo-a
  기본 브랜치: main

  설정:
    스캔 주기: 300초
    이슈 동시 처리: 2
    PR 동시 처리: 2
    감시 대상: Issues, Pull Requests

  스펙:
    auth-v2     Auth Module v2      Active  3/5 (60%)
    cache       Cache Layer         Paused  0/2 (0%)

  최근 활동:
    10분 전  #43 Token API → done
    25분 전  #44 Session adapter → implementing
```

### Cron 관리 (v4 `/cron`)

#### 목록

```bash
autodev cron list --json
```

```
Cron Jobs:

  유형      이름                레포           주기      상태      마지막 실행
  built-in  claw-evaluate       org/repo-a     60초      active    2분 전
  built-in  gap-detection       org/repo-a     1시간     active    30분 전
  built-in  knowledge-extract   org/repo-a     1시간     active    45분 전
  built-in  hitl-timeout        (global)       5분       active    3분 전
  built-in  daily-report        (global)       매일 06시 active    12시간 전
  built-in  log-cleanup         (global)       매일 00시 active    14시간 전
  custom    code-smell          org/repo-a     1시간     paused    —
```

#### 추가

스크립트를 추가하기 전에 유효성을 검증합니다.

1. Read로 스크립트 파일 내용을 읽습니다.
2. 검증 항목을 확인합니다:
   - shebang (`#!/bin/bash` 또는 `#!/bin/sh`) 존재 여부
   - 실행 권한 (`chmod +x`) 여부
   - `$AUTODEV_*` 환경변수 사용 여부 (하드코딩 경로 경고)
   - guard 로직 존재 여부
   - `autodev agent` 사용 여부 (`claude -p` 직접 호출 경고)

3. 검증 결과를 출력합니다.
4. 경고 사항이 있으면 AskUserQuestion으로 수정 여부를 확인합니다.
5. 등록합니다:

```bash
autodev cron add --name <name> --repo <repo> --interval <interval> --script <path>
```

#### 기타 조작

```bash
autodev cron pause <name> [--repo <repo>]
autodev cron resume <name> [--repo <repo>]
autodev cron trigger <name> [--repo <repo>]
autodev cron update <name> [--repo <repo>] --interval <interval>
autodev cron remove <name> [--repo <repo>]  # custom만
```

built-in cron은 제거할 수 없음을 안내합니다 (pause/resume만 가능).

---

## rules

적용 중인 규칙 목록을 조회합니다:

```bash
autodev claw rules --json [--repo <name>]
```

결과를 출력합니다:

```
Claw 적용 규칙:

  글로벌 (~/.autodev/claw-workspace/.claude/rules/):
    scheduling.md        스케줄링 정책
    branch-naming.md     브랜치 네이밍 전략
    review-policy.md     리뷰 정책
    decompose-strategy.md 스펙 분해 전략
    hitl-policy.md       HITL 판단 기준

  레포 오버라이드 (org/repo-a):
    review-policy.md     (글로벌 오버라이드)
```

---

## edit

지정된 규칙 파일을 Read로 읽고 내용을 출력한 후, 사용자와 대화하며 Edit으로 수정합니다.

**규칙 파일 경로 결정**:

- `--repo` 없음 → `~/.autodev/claw-workspace/.claude/rules/<rule>.md`
- `--repo` 있음 → `~/.autodev/workspaces/<org-repo>/claw/.claude/rules/<rule>.md`

1. 해당 파일을 Read로 읽어 현재 내용을 출력합니다.
2. 사용자에게 어떤 부분을 변경할지 물어봅니다.
3. 사용자 요청에 따라 Edit으로 파일을 수정합니다.
4. 수정 결과를 확인하여 출력합니다.

```
규칙 수정 완료: branch-naming.md

변경 내용:
  - hotfix 브랜치 패턴 추가: hotfix/{이슈번호}
  - release 브랜치 패턴 추가: release/{버전}/{이슈번호}

Claw가 다음 세션부터 업데이트된 규칙을 적용합니다.
```

---
description: Cron 관리 — list, add, pause, resume, trigger, remove
argument-hint: "<action> [name] [--repo <name>]"
allowed-tools: ["AskUserQuestion", "Bash", "Read"]
---

# Cron 관리 (/cron)

cron job의 목록 조회, 추가, 일시정지/재개, 즉시 실행, 제거를 수행합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/cron list` — cron job 목록 (global + per-repo)
- `/cron add <name> --repo <r> --interval <s> --script <path>` — cron 추가
- `/cron pause <name> [--repo <r>]` — 일시정지
- `/cron resume <name> [--repo <r>]` — 재개
- `/cron trigger <name> [--repo <r>]` — 즉시 실행
- `/cron update <name> [--repo <r>] --interval <s>` — 주기 수정
- `/cron remove <name> [--repo <r>]` — 제거 (custom만)

## 실행

인자에 따라 적절한 CLI 명령을 실행합니다.

### list

```bash
autodev cron list --json
```

결과를 테이블 형식으로 출력합니다:

```
⏰ Cron Jobs:

  유형      이름                레포           주기      상태      마지막 실행
  built-in  claw-evaluate       org/repo-a     60초      active    2분 전
  built-in  gap-detection       org/repo-a     1시간     active    30분 전
  built-in  knowledge-extract   org/repo-a     1시간     active    45분 전
  built-in  hitl-timeout        (global)       5분       active    3분 전
  built-in  daily-report        (global)       매일 06시 active    12시간 전
  built-in  log-cleanup         (global)       매일 00시 active    14시간 전
  custom    code-smell          org/repo-a     1시간     paused    —
```

### add

스크립트를 추가하기 전에 유효성을 검증합니다.

1. Read로 스크립트 파일 내용을 읽습니다.
2. 검증 항목을 확인합니다:
   - shebang (`#!/bin/bash` 또는 `#!/bin/sh`) 존재 여부
   - 실행 권한 (`chmod +x`) 여부
   - `$AUTODEV_*` 환경변수 사용 여부 (하드코딩 경로 경고)
   - guard 로직 존재 여부
   - `autodev agent` 사용 여부 (`claude -p` 직접 호출 경고)

3. 검증 결과를 출력합니다:

```
스크립트 검증 결과:
  ✅ shebang (#!/bin/bash) 존재
  ✅ $AUTODEV_REPO_ROOT 사용
  ✅ guard 로직 존재
  ⚠️ claude -p를 직접 호출하고 있습니다
     → autodev agent --repo "$AUTODEV_REPO_NAME" -p "..." 로 변경하시겠어요?
```

4. 경고 사항이 있으면 AskUserQuestion으로 수정 여부를 확인합니다.
5. 등록합니다:

```bash
autodev cron add --name <name> --repo <repo> --interval <interval> --script <path>
```

### update

```bash
autodev cron update <name> [--repo <repo>] --interval <interval>
```

주기 수정 결과를 출력합니다.

### pause

```bash
autodev cron pause <name> [--repo <repo>]
```

### resume

```bash
autodev cron resume <name> [--repo <repo>]
```

### trigger

```bash
autodev cron trigger <name> [--repo <repo>]
```

즉시 실행 결과를 출력합니다.

### remove

built-in cron은 제거할 수 없음을 안내합니다 (pause/resume만 가능).
custom cron만 제거할 수 있습니다:

```bash
autodev cron remove <name> [--repo <repo>]
```

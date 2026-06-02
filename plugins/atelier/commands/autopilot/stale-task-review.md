---
description: "stale Wip task를 관찰한 뒤 task별로 release/fail/escalate/leave alone을 결정합니다"
argument-hint: "[--before <duration> | --candidates <JSON>]"
allowed-tools: ["Bash", "Agent"]
---

# Stale Task Review (/atelier:stale-task-review)

stale Wip task 후보를 `stale-task-reviewer` 에이전트에 전달하여 task 별로 어떻게 처리할지(release/fail/escalate/leave alone) 결정합니다.

> 단계별 프로토콜과 책임 경계는 `autopilot-pipeline` skill 에 있습니다. 이 커맨드는 진입점만 담습니다.

## 사용법

```bash
# 1) 이벤트 드리븐 (autopilot Monitor가 STALE_WIP 이벤트 수신 시 호출)
/atelier:stale-task-review --candidates '["abc123","def456"]'

# 2) 매뉴얼 / cron 모드 (cutoff 기반 직접 조회)
/atelier:stale-task-review --before 1h
/atelier:stale-task-review                # 인자 없음 → stale_wip.threshold 사용
```

> hybrid 모드에서는 cron 등록이 없습니다. `autopilot watch` daemon 이 `STALE_WIP candidates=<JSON> epic=<E>` 이벤트를 emit 하면 Monitor 가 `--candidates <JSON>`을 붙여 호출합니다. cron 모드는 기존대로 `--before` 인자로 호출됩니다.

## 작업 프로세스

### Step 0: 인자 파싱

`$ARGUMENTS`에서 `--candidates <JSON>` 또는 `--before <duration>` 을 추출합니다. 둘 다 없으면 설정의 `stale_wip.threshold` 를 사용합니다.

### Step 1: stale 회수 결정 파이프라인

`autopilot-pipeline` `references/ledger.md` §B(stale-task-review) 절차를 수행합니다:
- stale 후보 조회(`--candidates` 입력 그대로 / `--before` 는 `task list-stale`) → 에이전트 디스패치(release/fail/escalate/leave alone 결정) → 결과 로그

> CLI(`task list-stale`)는 후보 관찰만, 결정(judgment)은 stale-task-reviewer 에이전트가, 결정 실행(`task release`/`task fail`/`task escalate`)은 CLI 가 수행합니다 (CLAUDE.md "CLI vs Skill/Agent").

## 주의사항

- `autopilot task list-stale` 가 exit 2(DB 접근 실패 등)면 cycle 중단하지 않고 다음 tick 으로 넘김 (failure isolation)
- 개별 task 결정 실행 실패 → 해당 task 만 skip, 나머지 진행
- 단건 회수는 `release-stale --task-id` 가 아닌 `release` 사용 (PR #696 audit)

상세 결정 기준·에러 처리·Output Examples 는 `autopilot-pipeline` skill 의 references 및 `agents/stale-task-reviewer.md` 참조.

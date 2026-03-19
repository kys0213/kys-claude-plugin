---
name: cli-reference
description: autodev CLI 전체 명령어 레퍼런스 — 각 slash command가 참조하여 적절한 CLI 명령을 호출
version: 1.0.0
---

# autodev CLI Reference

autodev CLI는 DB/GitHub 조작만 수행하는 인프라 도구입니다.
Plugin command(slash command)가 이 CLI를 내부적으로 호출하며, 사용자가 직접 사용할 필요는 없습니다.

모든 조회 명령은 `--json` 플래그를 지원하여 구조화된 출력을 반환합니다.

---

## 데몬 제어

```bash
autodev start [-d|--daemon]     # 데몬 시작 (백그라운드)
autodev stop                     # 데몬 중지
autodev restart [-d|--daemon]    # 데몬 재시작
autodev status --json            # 데몬 상태 (JSON)
```

---

## 레포 관리

```bash
autodev repo add <url> [--config '<JSON>']     # 레포 등록
autodev repo list --json                        # 등록된 레포 목록
autodev repo show <name> --json                 # 레포 상세 정보
autodev repo update <name> --config '<JSON>'    # 레포 설정 수정
autodev repo remove <name>                      # 레포 제거
```

**출력 필드 (repo list/show)**:
- `name`: 레포 식별자 (org/repo)
- `url`: GitHub URL
- `root`: 로컬 경로
- `default_branch`: 기본 브랜치
- `config`: 워크플로우 설정 (scan_interval, concurrency 등)
- `status`: 활성 상태

---

## 스펙 관리

```bash
autodev spec add --title <t> --file <f> --repo <r>    # 스펙 등록
autodev spec list --json [--repo <name>]               # 스펙 목록
autodev spec show <id> --json                          # 스펙 상세
autodev spec status <id> --json                        # 스펙 진행도 (이슈별 상태 포함)
autodev spec update <id> --file <f>                    # 스펙 내용 수정
autodev spec pause <id>                                # 스펙 일시정지
autodev spec resume <id>                               # 스펙 재개
autodev spec prioritize <id1> <id2> ...                # 스펙 우선순위 지정
autodev spec evaluate <id>                             # claw-evaluate 즉시 트리거
autodev spec decisions --json [-n <limit>]             # Claw 판단 이력 조회
```

**출력 필드 (spec status)**:
- `id`: 스펙 ID
- `title`: 스펙 제목
- `status`: Active / Paused / Completed
- `progress`: 진행도 (완료/전체)
- `issues`: 이슈별 상태 목록 (id, title, state, label)
- `acceptance_criteria`: 검증 항목 목록

**출력 필드 (spec decisions)**:
- `timestamp`: 판단 시각
- `repo`: 대상 레포
- `action`: 수행한 동작 (advance, skip, hitl, decompose 등)
- `reason`: 판단 근거
- `confidence`: 판단 확신도

---

## HITL 관리

```bash
autodev hitl list --json [--repo <name>]         # HITL 대기 목록
autodev hitl show <id> --json                    # HITL 상세 (상황 + 선택지)
autodev hitl respond <id> --choice <N>           # 선택지 번호로 응답
autodev hitl respond <id> --message "..."        # 자연어 메시지로 응답
```

**출력 필드 (hitl list/show)**:
- `id`: HITL ID (hitl-xxx)
- `repo`: 관련 레포
- `severity`: HIGH / MEDIUM / LOW
- `situation`: 상황 설명
- `context`: 상세 분석
- `options`: 선택지 배열
- `work_id`: 관련 이슈/PR ID
- `spec_id`: 관련 스펙 ID
- `created_at`: 생성 시각

---

## 큐 관리

```bash
autodev queue list --json [--repo <name>]     # 큐 아이템 목록
autodev queue show <work-id> --json           # 큐 아이템 상세
autodev queue advance <work-id>               # 다음 phase로 전이
autodev queue skip <work-id>                  # skip 처리
```

**상태 머신**: Pending → Ready → Running → Done / Failed

---

## Cron 관리

```bash
autodev cron list --json                       # 등록된 cron job 목록
autodev cron add --name <n> --repo <r> --interval <s> --script <path>
                                                # interval 기반 cron 추가
autodev cron add --name <n> --repo <r> --schedule "<cron>" --script <path>
                                                # cron expression 기반 추가
autodev cron update <name> [--repo <r>] --interval <s>
                                                # cron 주기 수정
autodev cron pause <name> [--repo <r>]         # 일시정지
autodev cron resume <name> [--repo <r>]        # 재개
autodev cron remove <name> [--repo <r>]        # 제거 (custom만)
autodev cron trigger <name> [--repo <r>]       # 즉시 실행
```

**출력 필드 (cron list)**:
- `name`: cron job 이름
- `type`: built-in / custom
- `scope`: global / per-repo
- `repo`: 대상 레포 (per-repo인 경우)
- `interval` 또는 `schedule`: 실행 주기
- `script`: 스크립트 경로
- `status`: active / paused
- `last_run`: 마지막 실행 시각
- `next_run`: 다음 실행 예정 시각

**per-repo job은 `--repo` 필수** (daemon이 환경변수를 주입하여 실행)

---

## Claw 워크스페이스

```bash
autodev claw init                              # 워크스페이스 초기화
autodev claw init --repo <name>                # 레포별 오버라이드 초기화
autodev claw rules --json [--repo <name>]      # 적용 중인 규칙 목록
autodev claw edit <rule> [--repo <name>]       # 규칙 편집
```

### Claw Headless 실행

```bash
autodev agent                                  # 대화형 Claw 세션
autodev agent -p "<prompt>"                    # headless 실행 (global 컨텍스트)
autodev agent --repo <name> -p "<prompt>"      # headless 실행 (repo 컨텍스트)
```

**`autodev agent`가 내부적으로 보장하는 것**:
- **작업 디렉토리**: Claw 워크스페이스를 서브프로세스의 working directory로 자동 설정
- **중복 실행 방지**: daemon cron engine이 job 실행 상태를 내부에서 관리
- **환경변수 주입**: `--repo` 지정 시 해당 레포의 `AUTODEV_*` 변수 자동 설정
- **실행 로그**: 실행 결과를 DB에 자동 기록

cron 스크립트에서는 guard(사전조건)만 작성하고, `autodev agent`를 호출하면 됩니다:
```bash
# guard 통과 후
autodev agent --repo "$AUTODEV_REPO_NAME" -p "큐를 평가해줘"
```

**워크스페이스 경로**:
- 글로벌: `~/.autodev/claw-workspace/`
- 레포별: `~/.autodev/workspaces/<org-repo>/claw/`

---

## 로그 / 사용량 / 대시보드

```bash
autodev logs [--repo <name>] [-n <limit>]      # 실행 로그 조회
autodev usage [--repo <name>] [--since <date>] # 토큰 사용량 조회
autodev dashboard [--repo <name>]              # TUI 칸반 대시보드 (읽기 전용)
```

---

## 환경변수 (Cron 스크립트용)

daemon이 cron 스크립트 실행 시 자동 주입:

### Per-repo

| 변수 | 설명 |
|------|------|
| `AUTODEV_REPO_NAME` | 레포 이름 (org/repo) |
| `AUTODEV_REPO_ROOT` | 레포 로컬 경로 |
| `AUTODEV_REPO_URL` | GitHub URL |
| `AUTODEV_REPO_DEFAULT_BRANCH` | 기본 브랜치 |
| `AUTODEV_WORKSPACE` | autodev 워크스페이스 경로 |

### Global

| 변수 | 설명 |
|------|------|
| `AUTODEV_HOME` | autodev 홈 (~/.autodev) |
| `AUTODEV_DB` | DB 경로 |
| `AUTODEV_CLAW_WORKSPACE` | Claw 워크스페이스 경로 |

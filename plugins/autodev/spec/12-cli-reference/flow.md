# Flow 12: 인터페이스 레퍼런스

### 설계 원칙

autodev의 인터페이스는 **2개 레이어**로 구성된다:

```
Plugin Commands (SSOT)           ← 사람 + Claw 모두 사용
  /add-spec, /update-spec, /hitl, /status, /board, ...
       │
       │ 내부에서 호출
       ▼
autodev CLI (인프라 도구)         ← plugin이 호출, 사람이 직접 쓸 일 없음
  autodev spec add, autodev hitl respond, ...
       │
       │ 데이터 조회/수정
       ▼
autodev daemon (백그라운드)       ← 수집 + Task 실행
```

**Plugin command가 SSOT**인 이유:
- user scope로 설치되므로 사람의 Claude 세션과 Claw(autodev agent) 모두 동일 로직 실행
- 검증, 대화형 보완, 컨텍스트 분석 등 판단 로직이 command에 집중
- autodev CLI는 DB/GitHub 조작만 수행하는 단순 도구

---

## Layer 1: Plugin Commands (사용자 + Claw 공용)

사람이 레포에서 실행하든, Claw가 autodev agent에서 실행하든 동일한 command.

### 스펙 관리

| Command | 설명 | 실행 위치 |
|---------|------|----------|
| `/add-spec [file]` | 스펙 등록 (대화형 검증 + 보완) | 레포 Claude 세션 |
| `/update-spec <id>` | 스펙 수정 (대화형 영향 분석) | 레포 Claude 세션 |

- 레포 컨텍스트에서 실행 → 코드베이스 분석, 테스트 환경 감지, 기존 규칙 참조
- 5개 필수 섹션 검증 + 누락 시 대화형 보완
- 내부적으로 `autodev spec add/update` CLI 호출

### 상태 확인

| Command | 설명 | 실행 위치 |
|---------|------|----------|
| `/status` | 전체 레포/스펙 상태 요약 | Claw 세션 |
| `/board [repo]` | 칸반 보드 (전체 또는 레포별) | Claw 세션 |
| `/spec list` | 스펙 목록 | Claw 세션 |
| `/spec status <id>` | 스펙 진행도 상세 | Claw 세션 |
| `/decisions [repo]` | Claw 판단 이력 | Claw 세션 |

### HITL 관리

| Command | 설명 | 실행 위치 |
|---------|------|----------|
| `/hitl` | HITL 대기 목록 + 대화형 응답 | Claw 세션 |

- `/hitl` 실행 후 자연어로 응답: "1번 재시도해줘", "Payment 우선 진행해"
- 내부적으로 `autodev hitl respond` CLI 호출

### 레포 관리

| Command | 설명 | 실행 위치 |
|---------|------|----------|
| `/repo list` | 등록된 레포 목록 | Claw 세션 |
| `/repo show <name>` | 레포 상세 | Claw 세션 |

### Claw 설정

| Command | 설명 | 실행 위치 |
|---------|------|----------|
| `/claw rules [repo]` | 현재 적용 규칙 확인 | Claw 세션 |
| `/claw edit <rule>` | 규칙 편집 | Claw 세션 |

### Cron 관리

| Command | 설명 | 실행 위치 |
|---------|------|----------|
| `/cron list` | cron job 목록 (global + per-repo) | Claw 세션 |
| `/cron pause <name> [--repo]` | cron 일시정지 | Claw 세션 |
| `/cron resume <name> [--repo]` | cron 재개 | Claw 세션 |
| `/cron trigger <name> [--repo]` | cron 즉시 실행 | Claw 세션 |
| `/cron add ...` | cron 추가 | Claw 세션 |

- per-repo job은 `--repo` 필수 (claude -p 실행 시 작업 경로 지정)
- 내부적으로 `autodev cron` CLI 호출

---

## Layer 2: autodev CLI (인프라 도구)

Plugin command 내부에서 호출되는 도구. `--json` 출력을 지원하여 command가 결과를 파싱.

### 데몬 제어

```bash
autodev start [-d|--daemon]     # 데몬 시작
autodev stop                     # 데몬 중지
autodev restart [-d|--daemon]    # 데몬 재시작
autodev status --json            # 데몬 상태 (JSON)
```

### 레포

```bash
autodev repo add <url> [--config '<JSON>']
autodev repo list --json
autodev repo show <name> --json
autodev repo update <name> --config '<JSON>'
autodev repo remove <name>
```

### 스펙

```bash
autodev spec add --title <t> --file <f> --repo <r>
autodev spec list --json [--repo <name>]
autodev spec show <id> --json
autodev spec status <id> --json
autodev spec update <id> --file <f>
autodev spec pause <id>
autodev spec resume <id>
autodev spec prioritize <id1> <id2> ...   # 스펙 우선순위 지정
autodev spec evaluate <id>                # claw-evaluate cron 즉시 트리거
autodev spec decisions --json [-n 20]
```

### HITL

```bash
autodev hitl list --json [--repo <name>]
autodev hitl show <id> --json
autodev hitl respond <id> --choice <N>
autodev hitl respond <id> --message "..."
```

### 큐

```bash
autodev queue list --json [--repo <name>]
autodev queue show <work-id> --json
autodev queue advance <work-id>           # 다음 phase로 전이 (Claw가 호출)
autodev queue skip <work-id>              # skip 처리 (Claw가 호출)
```

### Cron

```bash
autodev cron list --json                   # 등록된 cron job 목록
autodev cron add --name <n> --repo <r> --interval <s> --script <path>
                                           # interval 기반, 스크립트 파일
autodev cron add --name <n> --repo <r> --schedule "<cron>" --script <path>
                                           # cron expression 기반
autodev cron update <name> [--repo <r>] --interval <s>
autodev cron pause <name> [--repo <r>]     # 일시정지
autodev cron resume <name> [--repo <r>]    # 재개
autodev cron remove <name> [--repo <r>]    # 제거 (custom만)
autodev cron trigger <name> [--repo <r>]   # 즉시 실행
```

### 로그 / 사용량

```bash
autodev logs [--repo <name>] [-n <limit>]
autodev usage [--repo <name>] [--since <date>]
```

### Claw 워크스페이스

```bash
autodev claw init                 # 워크스페이스 초기화
autodev claw rules --json         # 적용 중인 규칙 목록
```

### Claw 에이전트

```bash
autodev agent                              # 대화형 Claw 세션
autodev agent -p "<prompt>"                # headless (global 컨텍스트)
autodev agent --repo <name> -p "<prompt>"  # headless (repo 컨텍스트)
```

`autodev agent`는 Claw 실행의 단일 진입점이다. 내부적으로 다음을 보장한다:

- **작업 디렉토리 자동 설정**: Claw 워크스페이스를 서브프로세스의 working directory로 지정
- **환경변수 자동 주입**: `--repo` 지정 시 해당 레포의 `AUTODEV_*` 변수 설정
- **실행 로그 기록**: 실행 결과를 DB에 자동 저장

cron 스크립트에서 `claude -p` 를 직접 호출하지 않고,
`autodev agent --repo -p` 를 호출하여 실행 인프라를 캡슐화한다.

### 대시보드

```bash
autodev dashboard [--repo <name>] # TUI 칸반 (읽기 전용 모니터링)
```

---

## 실행 위치별 정리

```
레포 Claude 세션 (사람이 코드 작업 중):
  /add-spec         스펙 등록
  /update-spec      스펙 수정

Claw 세션 (autodev agent):
  /status           전체 상태
  /board            칸반 보드
  /hitl             HITL 처리
  /spec             스펙 관리
  /decisions        판단 이력
  /claw rules       규칙 확인/편집
  /cron             cron job 관리

별도 터미널:
  autodev dashboard  TUI 모니터링 (읽기 전용)
  autodev start/stop 데몬 제어
```

### SSOT가 보장하는 것

```
/add-spec의 검증 로직 수정 → 사람과 Claw 모두 즉시 반영
/hitl의 응답 처리 변경     → 사람과 Claw 모두 즉시 반영

plugin command 하나만 수정하면 모든 진입점이 동기화된다.
```

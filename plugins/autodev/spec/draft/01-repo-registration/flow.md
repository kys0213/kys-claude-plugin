# Flow 1: Workspace 등록/변경/제거

## Workspace 등록

### 시나리오

사용자가 autodev로 관리할 workspace를 등록한다.
workspace는 하나 이상의 DataSource(GitHub, Jira, Slack 등)를 바인딩하는 최상위 그룹이다.

### 입력

```bash
autodev workspace add <github-url> [--config '<JSON>']
```

### 설정 필드

```yaml
sources:
  github:
    scan_interval_secs: 300
    scan_targets: [issues, pulls]
    issue_concurrency: 1
    pr_concurrency: 1
    filter_labels: null
    ignore_authors: [dependabot, renovate]
    gh_host: null
    auto_approve: false
    auto_approve_threshold: 0.8
    knowledge_extraction: true

runtime:
  default: claude
  claude:
    model: sonnet
  overrides: {}
```

### 기대 동작

```
1. URL에서 org/repo 이름 추출
2. DB에 workspace 등록 (UUID 생성, enabled=1)
3. workspace 디렉토리 생성 (~/.autodev/workspaces/org-repo/)
4. --config 제공 시 → workspace YAML에 deep merge 저장
5. DataSource 바인딩:
   → sources 섹션 파싱 → GitHubDataSource 인스턴스 생성
   → Daemon에 등록 (collect, hook 호출 대상)
6. AgentRuntime 바인딩:
   → runtime 섹션 파싱 → RuntimeRegistry 구성
7. per-workspace cron seed (claw-evaluate, gap-detection, knowledge-extract)
```

### 저장 구조

```
~/.autodev/
├── autodev.db
├── .autodev.yaml                  # 글로벌 설정
└── workspaces/
    └── org-repo/
        ├── .autodev.yaml          # workspace별 설정 오버라이드
        └── claw/                  # Claw per-workspace 오버라이드
```

설정은 2-level merge: 글로벌 + workspace별 → 유효 설정

### Claw 활성화 시 추가 단계

```
8. Claw 워크스페이스 초기화 확인
   → ~/.autodev/claw-workspace/ 없으면 autodev claw init
9. 대상 레포의 .claude/rules/ 확인
   → 비어있으면 → Flow 11: 컨벤션 부트스트랩 제안
10. workspace별 Claw 오버라이드 초기화
```

---

## Workspace 설정 변경

```bash
autodev workspace update <name> --config '<JSON>'
autodev workspace show <name>    # 유효 설정 조회
```

---

## Workspace 제거

```bash
autodev workspace remove <name>
```

```
1. 연관 데이터 cascade 삭제 (token_usage, scan_cursors, specs, queue_items, cron_jobs)
2. workspaces 레코드 삭제
3. GitHub 이슈/PR/라벨은 유지 (로컬 데이터만 삭제)
```

---

## Daemon의 Workspace 활용

Daemon tick마다:

```
1. workspace_find_enabled() → 활성 workspace 목록
2. workspace별 DataSource 인스턴스로 collect()
3. issue_concurrency, pr_concurrency로 동시 Task 수 제한
4. RuntimeRegistry.resolve(task_kind)로 적절한 AgentRuntime 선택
```

---

### 관련 플로우

- [Flow 0: DataSource](../00-datasource/flow.md)
- [Flow 0: AgentRuntime](../00-agent-runtime/flow.md)
- [Flow 10: Claw 워크스페이스](../10-claw-workspace/flow.md)
- [Flow 11: 컨벤션 부트스트랩](../11-convention-bootstrap/flow.md)

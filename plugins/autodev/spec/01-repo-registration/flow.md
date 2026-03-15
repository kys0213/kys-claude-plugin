# Flow 1: 레포 등록/변경/제거

## 레포 등록

### 시나리오

사용자가 autodev로 관리할 레포를 등록한다.

### 입력

```bash
autodev repo add <github-url> [--config '<JSON>']
```

- `<github-url>`: `https://github.com/org/repo` 또는 `org/repo` 형식
- `--config`: `GitHubSourceConfig` 필드를 JSON으로 전달 (선택)

### 설정 필드 (`GitHubSourceConfig`)

| 항목 | 기본값 | 설명 |
|------|--------|------|
| scan_interval_secs | 300 | GitHub API 스캔 주기 (초) |
| scan_targets | ["issues", "pulls"] | 스캔 대상 |
| issue_concurrency | 1 | 동시 처리 이슈 수 |
| pr_concurrency | 1 | 동시 처리 PR 수 |
| model | "sonnet" | Task 실행용 LLM 모델 |
| workspace_strategy | "worktree" | 작업 공간 전략 |
| filter_labels | null | 라벨 필터 (null이면 전체) |
| ignore_authors | ["dependabot", "renovate"] | 무시할 작성자 |
| gh_host | null | GitHub Enterprise 호스트 |
| confidence_threshold | 0.7 | 분석 신뢰도 임계값 |
| knowledge_extraction | true | 머지 후 학습 추출 활성화 |
| auto_approve | false | 분석 자동 승인 |
| auto_approve_threshold | 0.8 | 자동 승인 신뢰도 임계값 |

### 기대 동작

```
1. URL에서 org/repo 이름 추출
2. DB에 레포 등록 (UUID 생성, enabled=1)
3. workspace 디렉토리 생성 (~/.autodev/workspaces/org-repo/)
4. --config 제공 시 → workspace YAML에 deep merge 저장
```

### 저장 구조

```
~/.autodev/
├── autodev.db                              # SQLite (repositories 테이블)
├── .autodev.yaml                           # 글로벌 설정
└── workspaces/
    └── org-repo/
        └── .autodev.yaml                   # 레포별 설정 오버라이드
```

설정은 2-level merge: 글로벌 `.autodev.yaml` + 레포별 `.autodev.yaml` → 유효 설정

### Claw 활성화 시 추가 단계

`claw.enabled: true`로 등록한 경우:

```
5. Claw 워크스페이스 초기화 확인
   → ~/.autodev/claw-workspace/ 없으면 autodev claw init 실행
6. 레포의 .claude/rules/ 확인
   → 비어있으면 → 레포 코드에서 기술 스택 자동 감지
   → 감지 성공 시 → Flow 11: 컨벤션 부트스트랩 제안
   → 감지 실패 시 → 스킵
7. 레포별 Claw 오버라이드 초기화
   → ~/.autodev/workspaces/org-repo/claw/ 생성
```

---

## 레포 설정 변경

### 시나리오

등록된 레포의 설정을 변경한다.

### 입력

```bash
autodev repo update <name> --config '<JSON>'
```

### 기대 동작

```
1. DB에서 레포 존재 확인
2. JSON 파싱 → 기존 workspace YAML에 deep merge
3. 병합 결과를 workspace YAML에 저장
4. 유효 설정 출력
```

빈 JSON(`{}`)은 무시되며 기존 설정이 유지된다.

### 설정 조회

```bash
autodev repo config <name>
```

- 글로벌 설정 경로
- 레포별 설정 경로
- 병합된 유효 설정 출력

---

## 레포 조회

```bash
autodev repo list
```

등록된 레포 목록과 활성화 상태를 표시한다.

```
● org/repo-a  https://github.com/org/repo-a
○ org/repo-b  https://github.com/org/repo-b   (disabled)
```

---

## 레포 제거

### 입력

```bash
autodev repo remove <name>
```

### 기대 동작

```
1. DB에서 레포 조회
2. 연관 데이터 cascade 삭제:
   - token_usage
   - scan_cursors
   - consumer_logs
   - (specs, queue_items, cron_jobs 등 foreign key cascade)
3. repositories 레코드 삭제
```

- GitHub의 이슈/PR/라벨은 그대로 유지
- DB 레코드만 삭제 (로컬 workspace 정리는 수동)

---

## 데몬의 레포 활용

daemon은 tick마다 다음을 수행:

1. `repo_find_enabled()` → 활성 레포 목록 조회
2. 레포별 merged config 로드 → `scan_interval_secs`, `scan_targets` 확인
3. `issue_concurrency`, `pr_concurrency`에 따라 동시 Task 수 제한
4. `ignore_authors`, `filter_labels`로 수집 대상 필터링

---

## 관련 플로우

- [Flow 10: Claw 워크스페이스](../10-claw-workspace/flow.md)
- [Flow 11: 컨벤션 부트스트랩](../11-convention-bootstrap/flow.md)
- [Flow 12: CLI 레퍼런스](../12-cli-reference/flow.md)

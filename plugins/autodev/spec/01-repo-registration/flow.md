# Flow 1: 레포 등록

### 시나리오

사용자가 autodev로 관리할 레포를 등록한다.

### 입력

```bash
autodev repo add <github-url> [--config '<JSON>']
```

### 등록 시 필요한 정보

| 항목 | 필수 | 기본값 | 설명 |
|------|------|--------|------|
| GitHub URL | ✅ | - | `https://github.com/org/repo` 또는 `org/repo` |
| issue_concurrency | - | 1 | 동시 처리 이슈 수 |
| pr_concurrency | - | 1 | 동시 처리 PR 수 |
| model | - | sonnet | Task 실행용 LLM 모델 |
| scan_interval_secs | - | 300 | GitHub API 스캔 주기 |
| claw.enabled | - | false | Claw 스케줄러 활성화 |
| claw.model | - | sonnet | Claw 판단용 LLM 모델 |
| auto_approve | - | false | 분석 자동 승인 |

### 기대 동작

```
1. GitHub 인증 확인 (gh auth status)
2. 레포 접근 권한 확인
3. DB에 레포 등록 + workspace 디렉토리 생성
4. autodev 라벨이 없으면 → 라벨 자동 등록 제안
5. 설정 저장 (~/.autodev/workspaces/org-repo/.autodev.yaml)
```

### HITL 포인트

- 라벨 자동 등록 여부 확인
- claw.enabled 활성화 시 비용 안내 (LLM 호출 추가)

### Claw 활성화 시 추가 단계

`claw.enabled: true`로 등록한 경우:

```
6. Claw 워크스페이스 초기화 확인
   → ~/.autodev/claw-workspace/ 없으면 autodev claw init 실행
7. 레포의 .claude/rules/ 확인
   → 비어있으면 → 레포 코드에서 기술 스택 자동 감지
     (Cargo.toml, package.json, go.mod 등 스캔)
   → 감지 성공 시 → Flow 11: 컨벤션 부트스트랩 제안
   → 감지 실패 시 → 스킵 (이후 /add-spec 시 재시도)
8. 레포별 Claw 오버라이드 초기화
   → ~/.autodev/workspaces/org-repo/claw/ 생성
```

관련 플로우:
- [Flow 10: Claw 워크스페이스](../10-claw-workspace/flow.md)
- [Flow 11: 컨벤션 부트스트랩](../11-convention-bootstrap/flow.md)

---

### 레포 제거

```bash
autodev repo remove <name>
```

진행 중인 작업이 있는 경우:

```
autodev repo remove org/repo-a

⚠️ org/repo-a에 진행 중인 작업이 있습니다:
  - 활성 스펙: Auth Module v2 (3/5 진행 중)
  - 진행 중 이슈: #44 (implementing), #46 (analyzing)
  - HITL 대기: 1건

[1: 모든 작업 중단 후 제거]
  → 진행 중 이슈 skip 처리
  → 스펙 Archived 전환
  → worktree 정리
  → DB에서 레포 제거

[2: 취소]
```

- 진행 중 Task가 있으면 daemon이 완료를 대기하거나 skip 처리
- GitHub의 이슈/PR/라벨은 그대로 유지 (autodev 라벨만 남음)
- DB 레코드와 로컬 workspace만 삭제

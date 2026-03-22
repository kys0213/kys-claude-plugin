# Flow 1: 온보딩 — 레포 등록 → 컨벤션 → Claw 초기화

> 사용자가 autodev로 관리할 레포를 등록하고, 프로젝트 컨벤션과 Claw 판단 환경이 자동으로 구성된다.

---

## 1. 레포 등록

### 입력

```bash
autodev repo add <github-url> [--config '<JSON>']
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
2. DB에 레포 등록 (UUID 생성, enabled=1)
3. workspace 디렉토리 생성 (~/.autodev/workspaces/org-repo/)
4. --config 제공 시 → workspace YAML에 deep merge 저장
5. DataSource 바인딩:
   → sources 섹션 파싱 → GitHubDataSource 인스턴스 생성
   → Daemon에 등록 (collect, hook 호출 대상)
6. AgentRuntime 바인딩:
   → runtime 섹션 파싱 → RuntimeRegistry 구성
7. per-repo cron seed (claw-evaluate, gap-detection, knowledge-extract)
```

### 저장 구조

```
~/.autodev/
├── autodev.db
├── .autodev.yaml                  # 글로벌 설정
└── workspaces/
    └── org-repo/
        ├── .autodev.yaml          # 레포별 설정 오버라이드
        └── claw/                  # Claw per-repo 오버라이드
```

설정은 2-level merge: 글로벌 + 레포별 → 유효 설정

---

## 2. 컨벤션 부트스트랩

레포 등록 시 `.claude/rules/`가 비어있다면, 기술 스택을 기반으로 컨벤션을 자동 생성한다.

### Phase 1: Bootstrap

```
1. .claude/rules/ 또는 CLAUDE.md 존재 확인 → 있으면 skip
2. 스펙/코드에서 기술 스택 추출 (Rust, TypeScript, Go, Python 등)
3. 카테고리별 컨벤션 제안 (대화형):
   - 프로젝트 구조
   - 에러 처리
   - 테스트 전략
   - Git 워크플로우
   - 코드 스타일
4. 사용자 승인 → PR로 커밋
```

### Phase 2: 자율 개선

```
피드백 소스:
  - HITL 응답의 message 필드
  - PR 리뷰 코멘트 반복 패턴 (3회 이상)
  - /spec update로 인한 컨벤션 변경
  - 사용자 직접 지시

Claw가 패턴 감지 → 규칙 변경 제안 → HITL 승인 → 자동 업데이트
  - 레포 규칙: PR로 반영
  - Claw workspace 규칙: 즉시 반영
```

### DataSource.before_task()에서 활용

```rust
impl DataSource for GitHubDataSource {
    async fn before_task(&self, kind, item, ctx) -> Result<()> {
        if kind == TaskKind::Implement {
            // convention의 git-workflow 규칙에서 브랜치명 결정
            let branch = ctx.repo.convention.branch_name(item);
            // 예: feat/42-jwt-middleware
        }
        Ok(())
    }
}
```

### Claw decompose 시 이슈 템플릿

```
convention/issue-template:
  - 이슈 본문 섹션: 변경 대상 파일, 테스트 계획
  - 라벨 자동 부착 규칙
  - 브랜치 네이밍 패턴
```

---

## 3. Claw 워크스페이스 초기화

레포 등록 시 Claw 환경도 함께 구성된다.

```
8. Claw 워크스페이스 초기화 확인
   → ~/.autodev/claw-workspace/ 없으면 autodev claw init
9. 레포의 .claude/rules/ 확인
   → 비어있으면 → 컨벤션 부트스트랩 제안 (위 Phase 1)
10. 레포별 Claw 오버라이드 초기화
```

---

## 4. 레포 설정 변경 / 제거

### 설정 변경

```bash
autodev repo update <name> --config '<JSON>'
autodev repo config <name>    # 유효 설정 조회
```

### 레포 제거

```bash
autodev repo remove <name>
```

```
1. 연관 데이터 cascade 삭제 (token_usage, scan_cursors, specs, queue_items, cron_jobs)
2. repositories 레코드 삭제
3. GitHub 이슈/PR/라벨은 유지 (로컬 데이터만 삭제)
```

---

## 5. Daemon의 레포 활용

Daemon tick마다:

```
1. repo_find_enabled() → 활성 레포 목록
2. 레포별 DataSource 인스턴스로 collect()
3. issue_concurrency, pr_concurrency로 동시 Task 수 제한
4. RuntimeRegistry.resolve(task_kind)로 적절한 AgentRuntime 선택
```

---

### 관련 문서

- [DataSource](../concerns/datasource.md) — DataSource 바인딩 상세
- [AgentRuntime](../concerns/agent-runtime.md) — RuntimeRegistry 구성
- [Claw 워크스페이스](../concerns/claw-workspace.md) — Claw 규칙/스킬 구조
- [Cron 엔진](../concerns/cron-engine.md) — per-repo cron seed

# Flow 11: 컨벤션 부트스트랩 + 자율 개선

### 시나리오

Workspace 등록 시 `.claude/rules/`가 비어있다면, 기술 스택을 기반으로 컨벤션을 자동 생성한다.

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
  - workspace 규칙: PR로 반영
  - Claw workspace 규칙: 즉시 반영
```

### DataSource.before_task()에서 활용

```rust
impl DataSource for GitHubDataSource {
    async fn before_task(&self, kind: TaskKind, item: &QueueItem, ctx: &HookContext) -> Result<()> {
        if kind == TaskKind::Implement {
            // convention의 git-workflow 규칙에서 브랜치명 결정
            let branch = ctx.workspace.convention.branch_name(item);
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

### 관련 플로우

- [Flow 0: DataSource](../00-datasource/flow.md) — before_task hook
- [Flow 1: Workspace 등록](../01-repo-registration/flow.md) — 등록 시 트리거
- [Flow 3: 스펙 등록](../03-spec-registration/flow.md) — decompose 시 템플릿

# Plan: GitRepository Aggregate 도입

## 목표
흩어진 DTO + 전역 TaskQueues를 `GitRepository` aggregate로 통합하여
이벤트 루프(daemon)와 핵심 도메인 로직을 분리한다.

## 구현 단계 (각 단계 독립 컴파일 + 테스트 통과)

### Phase 1: GitRepository 구조체 + Factory 생성
**목표:** EnabledRepo/ResolvedRepo를 GitRepository로 통합

1. `domain/git_repository.rs` 생성
   - `GitRepository` struct: id, name, url, gh_host, issues, pulls + per-repo 큐 3종
   - `Issue`/`Pull` 타입은 기존 `RepoIssue`/`RepoPull`을 rename (또는 type alias)
   - GitHub 상태 조회 메서드: `open_issues()`, `open_pulls()`, `wip_issues()` 등
   - 큐 접근 메서드: `contains()`, `total_items()`
   - `refresh()`: GitHub API로 issues/pulls 재갱신

2. `domain/git_repository_factory.rs` 생성
   - `resolve_repos()` 로직을 factory로 이관 (gh_host 캐싱 포함)
   - `create()`: 단일 repo 생성
   - `create_all()`: 모든 enabled repos를 `HashMap<String, GitRepository>`로 생성

3. 기존 코드는 아직 변경하지 않음 (새 파일만 추가)
4. 단위 테스트 작성: factory 생성, refresh, 큐 기본 동작

### Phase 2: Scanning 로직을 GitRepository로 이동
**목표:** scanner/ 모듈의 핵심 로직을 GitRepository 메서드로 흡수

1. `GitRepository`에 scan 메서드 추가:
   - `scan_issues()`: scanner/issues.rs의 `scan()` + `scan_approved()` 통합
   - `scan_pulls()`: scanner/pulls.rs의 `scan()` + `scan_merges()` 통합
   - 라벨 전이(analyze→wip 등)도 내부에서 처리

2. scanner/mod.rs의 `scan_all()`을 GitRepository 기반으로 전환:
   ```rust
   // 변경 전
   scanner::scan_all(&db, &env, &gh, &mut queues).await;
   // 변경 후
   for repo in repos.values_mut() {
       repo.scan_issues(&*gh, &db, &scan_cfg).await;
       repo.scan_pulls(&*gh, &db, &scan_cfg).await;
   }
   ```

3. scanner/ 모듈은 thin wrapper로 유지하거나 제거 결정

### Phase 3: Recovery 로직을 GitRepository로 이동
**목표:** daemon/recovery.rs의 복구 로직을 GitRepository 메서드로 흡수

1. `GitRepository`에 recovery 메서드 추가:
   - `recover_orphan_wip()`: orphan wip 라벨 정리
   - `recover_orphan_implementing()`: orphan implementing 복구
   - `reconcile()`: startup reconcile 로직 (wip issue/PR → 큐 복구)

2. daemon/recovery.rs에서 `resolve_repos()` 제거 (factory로 이관 완료)
3. `recover_orphan_wip()`, `recover_orphan_implementing()` 제거

### Phase 4: 큐 관리를 GitRepository로 완전 이관
**목표:** 전역 TaskQueues 제거, per-repo 큐로 전환

1. `GitRepository`에 큐 조작 메서드 추가:
   - `pop_analyzing_issue()`, `pop_ready_issue()`, `pop_pending_pr()` 등
   - `handle_task_output()`: pipeline 결과를 받아 큐 상태 전이 + 로그 기록

2. daemon/mod.rs의 `spawn_ready_tasks()` 리팩토링:
   ```rust
   for repo in repos.values_mut() {
       while tracker.can_spawn() {
           if let Some(item) = repo.pop_pending_issue() {
               tracker.track(&repo.name);
               repo.issue_queue.push(ANALYZING, item.clone());
               join_set.spawn(async move { pipeline::issue::analyze_one(item, ...) });
           }
       }
   }
   ```

3. `TaskQueues` struct 제거 (또는 deprecated)
4. `IssueItem`/`PrItem`/`MergeItem` → GitRepository 내부 타입으로 이동

### Phase 5: Daemon 이벤트 루프 정리 + 테스트 마이그레이션
**목표:** daemon이 순수 오케스트레이터가 되도록 정리

1. daemon/mod.rs 간소화:
   ```rust
   let mut repos = GitRepositoryFactory::create_all(&db, &env, &gh).await;
   loop {
       tokio::select! {
           Some(result) = join_set.join_next() => {
               let repo = repos.get_mut(&result.repo_name).unwrap();
               repo.handle_task_output(&db, result);
               spawn_from_repo(repo, &mut tracker, &mut join_set, ...);
           }
           _ = tick.tick() => {
               for repo in repos.values_mut() {
                   repo.refresh(&*gh).await;
                   repo.recover_orphans(&*gh).await;
                   repo.scan(&*gh, &db, &cfg).await;
               }
               for repo in repos.values_mut() {
                   spawn_from_repo(repo, &mut tracker, &mut join_set, ...);
               }
           }
       }
   }
   ```

2. Integration test 마이그레이션:
   - `GitRepository` builder/mock으로 테스트 리팩토링
   - 기존 TaskQueues 기반 테스트 → GitRepository 기반으로 전환

## 불변 사항 (변경하지 않음)
- `pipeline/` 모듈: 작업 실행 로직은 현재 구조 유지
- `infrastructure/` 모듈: Gh, Git, Claude 인터페이스 유지
- `queue/state_queue.rs`: StateQueue 자료구조 유지 (GitRepository 내부에서 재사용)
- `queue/Database` + SQLite 구현: DB 레이어 유지
- `domain/labels.rs`: 라벨 상수 유지
- `domain/repository.rs`: DB trait 유지

## 제거 대상
- `domain/models.rs`의 `EnabledRepo`, `ResolvedRepo` → GitRepository로 통합
- `domain/models.rs`의 `RepoIssue`, `RepoPull` → `Issue`, `Pull`로 rename
- `queue/task_queues.rs`의 `IssueItem`, `PrItem`, `MergeItem` → domain으로 이동
- `queue/task_queues.rs`의 `TaskQueues` → per-repo 큐로 대체
- `daemon/recovery.rs`의 `resolve_repos()` → factory로 이관
- `scanner/` 모듈 대부분 → GitRepository로 흡수

## Quality Gate
각 Phase 완료 시:
- `cargo fmt --check` 통과
- `cargo clippy -- -D warnings` 통과
- `cargo test` 전체 통과
- 커밋 + 푸시

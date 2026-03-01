# DESIGN Gap Analysis v3

> **Date**: 2026-03-01
> **Scope**: DESIGN-v2.md + DESIGN-v3-ARCHITECTURE.md 와 현재 구현 간의 차이 분석
> **Base**: [DESIGN-v2-GAP-ANALYSIS-v2.1.md](./DESIGN-v2-GAP-ANALYSIS-v2.1.md) (2026-03-01)

---

## 요약

### v2.1 Gap Analysis 해소 현황

이전 gap analysis (v2.1)에서 식별한 **15개 gap 모두 해소**됨:

| GAP | 주제 | 상태 | 해소 위치 |
|-----|------|------|----------|
| GAP 1 | `changes-requested` 라벨 상수 | ✅ | `labels.rs:14` |
| GAP 2 | `extracted` 라벨 상수 | ✅ | `labels.rs:15` |
| GAP 3 | startup_reconcile changes-requested 처리 | ✅ | `git_repository.rs:691-719` |
| GAP 4 | `scan_done_merged()` 구현 | ✅ | `git_repository.rs:372` |
| GAP 5 | PR scan Label-Positive 전환 | ✅ | `git_repository.rs:289-291` |
| GAP 6 | Merge pipeline 제거 | ✅ | `pipeline/` 모듈 삭제 완료 |
| GAP 7 | ReviewTask changes-requested 라벨 추가 | ✅ | `review.rs:344-360` |
| GAP 8 | ImproveTask changes-requested → wip 전이 | ✅ | `improve.rs:149-165` |
| GAP 9 | PR scan 자동 wip 추가 제거 | ✅ | `git_repository.rs:288-291` |
| GAP 10 | orphan wip PR 재큐잉 | ✅ | `git_repository.rs:492-525` |
| GAP 11 | `scan_merges()` 제거 | ✅ | 함수 삭제 완료 |
| GAP 12/15 | 지식추출 merge 후 트리거 | ✅ | `review.rs:317`, `sources/github.rs:188-193` |
| GAP 13 | Improved → Pending 경유 | ⚠️ Minor | Improved → Reviewing 직접 전이 (기능 동일) |
| GAP 14 | `merge_phase::CONFLICT` dead code | ✅ | 삭제 완료 |
| GAP 16 | `is_changes_requested()` 메서드 | ✅ | `models.rs:141-143` |

### 신규 Gap 요약

| Severity | 개수 | 주요 테마 |
|----------|------|----------|
| **Critical** | 0 | — |
| **Medium** | 3 | v3 아키텍처 마이그레이션 미완료 (Daemon struct, TaskManager, Daily report) |
| **Low** | 5 | 라벨 정리 누락, dead code, `#[allow]` |

v2.1의 **Critical gap은 모두 해소**됨. 남은 gap은 v3 아키텍처 리팩토링 범위.

---

## Medium Gaps

### NEW-GAP-1: Daemon이 함수(`start()`)이지 struct가 아님 + 의존성 조립 미분리

| 항목 | 내용 |
|------|------|
| **카테고리** | v3 Architecture (Phase 4) |
| **디자인** | DESIGN-v3 §2, §9: `pub struct Daemon { manager, runner, inflight }` + `impl Daemon { async fn run() }` |
| **구현** | `daemon/mod.rs:100`: `pub async fn start(home, env, gh, git, claude, sw) -> Result<()>` 함수 — 의존성 조립과 이벤트 루프 실행이 혼재 |
| **파일** | `cli/src/daemon/mod.rs:100-335` |

**영향**: Daemon 자체의 단위 테스트가 불가능. MockTaskManager + MockTaskRunner를 주입하여 오케스트레이션 로직(인플라이트 제한, task 완료 후 즉시 spawn 등)을 검증할 수 없음.

**수정 방향**: `bootstrap` → `Daemon::new` → `Daemon::run` 3단계 분리

```rust
// 1. 조립 결과를 담는 구조체
struct DaemonDeps {
    manager:  Box<dyn TaskManager>,
    runner:   Arc<dyn TaskRunner>,
    reporter: Box<dyn DailyReporter>,
    log_db:   Database,
    config:   DaemonConfig,
}

// 2. 의존성 조립만 담당 (외부 리소스 생성: DB, config, workspace 등)
async fn bootstrap(
    home:   &Path,
    env:    Arc<dyn Env>,
    gh:     Arc<dyn Gh>,
    git:    Arc<dyn Git>,
    claude: Arc<dyn Claude>,
    sw:     Arc<dyn SuggestWorkflow>,
) -> Result<DaemonDeps> {
    let db = Database::open(home.join("daemon.db"))?;
    let log_db = Database::open(home.join("daemon-log.db"))?;
    let config = config::loader::load_merged(&env)?;
    let workspace = OwnedWorkspace::new(git.clone(), env.clone());
    let agent = ClaudeAgent::new(claude);
    let runner = DefaultTaskRunner::new(Arc::new(agent));
    let source = GitHubTaskSource::new(workspace, gh, config.clone(), env, git, sw, db);
    let manager = DefaultTaskManager::new(vec![Box::new(source)]);
    let reporter = DefaultDailyReporter::new(/* ... */);
    Ok(DaemonDeps { manager: Box::new(manager), runner, reporter, log_db, config })
}

// 3. 테스트 가능한 Daemon struct
pub struct Daemon {
    manager:  Box<dyn TaskManager>,
    runner:   Arc<dyn TaskRunner>,
    reporter: Box<dyn DailyReporter>,
    log_db:   Database,
    config:   DaemonConfig,
}

// 4. start()는 bootstrap → run 연결만
pub async fn start(home, env, gh, git, claude, sw) -> Result<()> {
    let deps = bootstrap(home, &env, &gh, &git, &claude, &sw).await?;
    let mut daemon = Daemon::new(deps.manager, deps.runner, deps.log_db, deps.reporter, deps.config);
    daemon.run().await
}
```

**테스트 시**: `bootstrap` 없이 mock만으로 `Daemon::new(mock, mock, ...)` 직접 생성 가능.

```rust
#[tokio::test]
async fn daemon_respects_inflight_limit() {
    let manager = MockTaskManager::new()...;
    let runner = MockTaskRunner::new()...;
    let reporter = MockDailyReporter::new()...;
    let log_db = Database::open_in_memory()?;
    let mut daemon = Daemon::new(Box::new(manager), runner, log_db, Box::new(reporter), config);
    // bootstrap 불필요 — 순수 오케스트레이션 로직만 검증
}
```

**현재**: 통합 테스트만 가능 (실제 DB, 실제 프로세스 필요).

**책임 분리 요약**:
| 계층 | 책임 | 테스트 |
|------|------|--------|
| `bootstrap()` | 외부 리소스 생성 + 의존성 조립 | 통합 테스트 |
| `Daemon::new()` | 조립된 의존성 수신 | — |
| `Daemon::run()` | 오케스트레이션 (poll, dispatch, inflight) | mock으로 단위 테스트 |
| `start()` | 진입점 — bootstrap → run 연결 | E2E |

---

### NEW-GAP-2: TaskManager가 Daemon에서 미사용

| 항목 | 내용 |
|------|------|
| **카테고리** | v3 Architecture (Phase 4) |
| **디자인** | DESIGN-v3 §2: Daemon → TaskManager → TaskSource 계층 |
| **구현** | `daemon/mod.rs`에서 `TaskManager`를 사용하지 않고 `TaskSource`를 직접 호출 |
| **파일** | `daemon/mod.rs:232` (`source.poll()` 직접 호출), `daemon/task_manager_impl.rs` (구현 완료되었으나 미사용) |

**영향**: TaskManager가 제공하는 **다중 source 집계** 기능이 무시됨. 향후 Slack, Jira 등 추가 소스를 TaskManager에 등록하여 확장하는 것이 불가능.

```
디자인: Daemon → manager.tick() → manager.drain_ready() → runner.run()
                 ↓
         TaskManager.tick() → source1.poll() + source2.poll()
                             → ready_tasks에 수집

현재:   Daemon → source.poll() 직접 호출 → runner.run()
```

---

### ~~NEW-GAP-3~~: TaskContext — 성급한 추상화로 판단, Gap에서 제외

> **제외 사유**: TaskContext(`{ workspace, gh, config }`)는 GitHub 전용 의존성.
> 소스가 다양해지면(Slack, Jira 등) 각 소스마다 필요한 의존성이 다르므로,
> 하나의 TaskContext로 묶으면 소스 추가 시 TaskContext 자체가 비대해져 OCP/ISP 위반.
> 각 Source가 자기 Task에 필요한 의존성을 직접 주입하는 현재 방식이 적절.

---

### NEW-GAP-4: Daily report가 daemon event loop에 인라인

| 항목 | 내용 |
|------|------|
| **카테고리** | v3 Architecture / SRP |
| **디자인** | DESIGN-v3 §3: "daily report → TaskManager" 또는 별도 DailyReportSource |
| **구현** | `daemon/mod.rs:238-301`에 ~60줄의 daily report 로직이 인라인 |
| **파일** | `cli/src/daemon/mod.rs:238-301` |

**영향**: Daemon이 knowledge 도메인에 직접 의존. SRP 위반:
- `crate::knowledge::daily` 모듈 직접 호출
- `crate::knowledge::daily::parse_daemon_log()`
- `crate::knowledge::daily::detect_patterns()`
- `crate::knowledge::daily::build_daily_report()`
- `crate::knowledge::daily::generate_daily_suggestions()`
- `crate::knowledge::daily::post_daily_report()`
- `crate::knowledge::daily::create_knowledge_prs()`

Daemon 단위 테스트 불가의 원인 중 하나.

---

## Low Gaps

### NEW-GAP-5: ReviewTask max_iterations 시 `changes-requested` 라벨 미제거

| 항목 | 내용 |
|------|------|
| **카테고리** | State transition |
| **디자인** | DESIGN-v2 §3: max iteration → `autodev:skip` (다른 라벨 없음) |
| **구현** | `review.rs:344-360`에서 request_changes 시 `changes-requested` 추가 → `review.rs:367-409`에서 max_iterations 시 `skip` 추가하지만 `changes-requested` 미제거 |
| **파일** | `cli/src/tasks/review.rs:367-409` |

**최종 라벨 상태**:
```
디자인: autodev:skip (만)
현재:   autodev:changes-requested + autodev:skip
```

**수정**: max_iterations 분기에서 `changes-requested` 라벨 제거 추가.

---

### NEW-GAP-6: SkipReason에 `AlreadyProcessed` variant 누락

| 항목 | 내용 |
|------|------|
| **카테고리** | v3 Architecture (Phase 1) |
| **디자인** | DESIGN-v3 §5: `SkipReason::AlreadyProcessed` — 이미 처리됨 (dedup) |
| **구현** | `daemon/task.rs:117-120`: `SkipReason::PreflightFailed(String)` 만 존재 |
| **파일** | `cli/src/daemon/task.rs:117-120` |

**영향**: dedup 시 명시적 사유 구분 불가. 로그에서 "preflight: already processed" vs "preflight: issue closed" 구분이 문자열 비교에 의존.

---

### NEW-GAP-7: Merger 컴포넌트가 고아 상태 (dead code)

| 항목 | 내용 |
|------|------|
| **카테고리** | Dead code |
| **디자인** | DESIGN-v2 §12: "PR Merge: scope 외" |
| **구현** | `components/merger.rs`: `MergeOutcome`, `MergeOutput`, `Merger` struct가 존재하지만 호출하는 코드 없음 |
| **파일** | `cli/src/components/merger.rs` (122줄) |

**영향**: Dead code. merge pipeline 제거 시 함께 제거되어야 했으나 잔존. `cargo clippy` dead_code 경고 대상.

---

### NEW-GAP-8: `#[allow]` 어노테이션 잔존

| 항목 | 내용 |
|------|------|
| **카테고리** | Code quality |
| **디자인** | DESIGN-v3 Phase 4: `#[allow(clippy::too_many_arguments)]` 0건 목표 |
| **구현** | 아래 표 참조 |

| 위치 | 어노테이션 | 상태 |
|------|-----------|------|
| `knowledge/extractor.rs:139` | `#[allow(clippy::too_many_arguments)]` | **해결 필요** — builder 패턴 또는 config struct로 인자 축소 |
| `knowledge/extractor.rs:298` | `#[allow(clippy::too_many_arguments)]` | **해결 필요** — 동일 |
| `tasks/extract.rs:43` | `#[allow(dead_code)]` config | **해결 필요** — config 미사용 |
| `tasks/improve.rs:33` | `#[allow(dead_code)]` config | **해결 필요** — config 미사용 |
| `domain/models.rs:100` | `#[allow(dead_code)]` author | 수용 가능 (향후 사용 예정) |
| mock 파일들 | `#[allow(dead_code)]` | 수용 가능 (테스트 인프라) |

---

### NEW-GAP-9: Improved → Reviewing 직접 전이

| 항목 | 내용 |
|------|------|
| **카테고리** | Queue phase |
| **디자인** | DESIGN-v2 §6: `Improved → autodev:wip + Pending으로 재진입` |
| **구현** | `sources/github.rs:254-267`: Improved → Reviewing 직접 전이 (Pending 건너뜀) |
| **파일** | `cli/src/sources/github.rs:254-267` |

**참고**: v2.1 GAP 13과 동일. 기능적으로 동일 (같은 drain 사이클). scan 시 dedup/재검증 로직을 건너뛸 수 있으나 현재 drain은 scan 이후에 실행되므로 실질적 영향 없음.

---

## v3 아키텍처 마이그레이션 진행 현황

### Phase별 완료도

| Phase | 범위 | 상태 | 완료도 |
|-------|------|------|--------|
| **Phase 1**: Trait 정의 | Task, Agent, TaskSource, TaskRunner, TaskManager, WorkspaceOps, ConfigLoader | ✅ 완료 | 100% |
| **Phase 2**: Task 구현체 | AnalyzeTask, ImplementTask, ReviewTask, ImproveTask, ExtractTask (TDD) | ✅ 완료 | 100% |
| **Phase 3**: Source + Runner + Manager | DefaultTaskRunner, ClaudeAgent, DefaultTaskManager, GitHubTaskSource | ✅ 완료 | 100% |
| **Phase 4**: Daemon 전환 + Legacy 제거 | Daemon struct, main.rs DI, legacy 제거 | ⚠️ 부분 | 60% |

### Phase 4 세부 현황

| 항목 | 상태 | 비고 |
|------|------|------|
| TaskRunner 사용 | ✅ | Daemon에서 `Arc<dyn TaskRunner>` 사용 |
| TaskSource 사용 | ✅ | Daemon에서 `GitHubTaskSource` 사용 |
| Legacy `pipeline/` 제거 | ✅ | 모듈 완전 삭제 |
| Legacy `scanner/` 제거 | ✅ | 모듈 완전 삭제 |
| Merge pipeline 제거 | ✅ | `MergeItem`, `merge_queue`, `scan_merges` 삭제 |
| **bootstrap 분리** | ❌ | 의존성 조립이 start()에 인라인 (NEW-GAP-1) |
| **Daemon struct** | ❌ | 함수로 구현 (NEW-GAP-1) |
| **TaskManager 연동** | ❌ | 미사용 (NEW-GAP-2) |
| ~~TaskContext 사용~~ | — | 성급한 추상화로 판단, Gap에서 제외 |
| **Daily report 분리** | ❌ | 인라인 (NEW-GAP-4) |
| `#[allow(too_many_arguments)]` 0건 | ❌ | 2건 잔존 (NEW-GAP-8) |

---

## 구현 우선순위 제안

### Priority 1: 코드 품질 (빠르게 수정 가능)

```
1. NEW-GAP-5: ReviewTask max_iterations에서 changes-requested 제거 (1줄 추가)
2. NEW-GAP-7: components/merger.rs 삭제 (dead code)
3. NEW-GAP-6: SkipReason::AlreadyProcessed variant 추가
```

### Priority 2: v3 Phase 4 완료

```
4. NEW-GAP-1: bootstrap + Daemon struct 전환
   a. bootstrap() 함수 추출 — 의존성 조립 분리
   b. Daemon struct 정의 — 조립된 의존성 수신
   c. start()를 bootstrap → Daemon::new → run 연결로 축소
5. NEW-GAP-2: Daemon에서 TaskManager 사용 (source.poll → manager.tick)
6. NEW-GAP-4: Daily report 로직 분리 (DailyReporter trait + DefaultDailyReporter)
```

### Priority 3: 정리

```
8. NEW-GAP-8: #[allow] 어노테이션 해소
9. NEW-GAP-9: Improved → Pending 경유 (선택적)
```

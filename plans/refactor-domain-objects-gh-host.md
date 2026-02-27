# 도메인 객체 리팩토링: gh_host를 큐 아이템에 바인딩

> **Date**: 2026-02-27
> **Scope**: IssueItem, PrItem, MergeItem에 `gh_host` 필드 추가 + 파이프라인 gh_host 소스 변경 + infrastructure 로깅 강화
> **Status**: 구현 중

---

## 1. 요구사항 정리

### 문제

GHE 환경에서 파이프라인의 모든 `gh api` 호출이 HTTP 503으로 실패.
스캐너(scanner/GitRepository)는 정상 동작하는데 파이프라인(pipeline)만 실패.

### 근본 원인

`gh_host`가 스캐너 → 큐 아이템 전환 과정에서 유실됨.
파이프라인은 `gh_host`를 얻기 위해 `config::loader::load_merged(env, None)`을 호출하지만,
이는 글로벌 config만 참조하여 per-repo `gh_host`를 찾지 못함.

```
Scanner: ResolvedRepo.gh_host ✅ → IssueItem (gh_host 없음 ❌)
Pipeline: load_merged(env, None) → 글로벌 config → gh_host: None ❌
```

### 해결 방향

큐 아이템(`IssueItem`, `PrItem`, `MergeItem`)에 `gh_host: Option<String>` 필드를 추가하여,
스캐너가 이미 알고 있는 `gh_host`를 직렬화 과정에서 유실하지 않도록 한다.
파이프라인은 `item.gh_host.as_deref()`로 바로 사용한다.

---

## 2. 사이드이펙트 조사

### 2-1. 큐 아이템 구조체 변경 (`queue/task_queues.rs`)

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| `IssueItem` struct | `gh_host: Option<String>` 필드 추가 | 모든 생성 지점에서 값 설정 필요 |
| `PrItem` struct | `gh_host: Option<String>` 필드 추가 | 모든 생성 지점에서 값 설정 필요 |
| `MergeItem` struct | `gh_host: Option<String>` 필드 추가 | 모든 생성 지점에서 값 설정 필요 |
| `task_queues.rs` 테스트 헬퍼 | `issue()`, `pr()`, `merge()` 함수에 필드 추가 | `gh_host: None` 기본값 |

### 2-2. 큐 아이템 생성 지점 — 프로덕션 코드

| 파일 | 함수 | 아이템 타입 | gh_host 소스 |
|------|------|------------|-------------|
| `scanner/issues.rs:84` | `scan()` | IssueItem | 함수 파라미터 `gh_host` |
| `scanner/issues.rs:169` | `scan_approved()` | IssueItem | 함수 파라미터 `gh_host` |
| `scanner/pulls.rs:110` | `scan()` | PrItem | 함수 파라미터 `gh_host` |
| `scanner/pulls.rs:205` | `scan_merges()` | MergeItem | 함수 파라미터 `gh_host` |
| `git_repository.rs:204` | `scan_issues()` | IssueItem | `self.gh_host` |
| `git_repository.rs:292` | `scan_approved_issues()` | IssueItem | `self.gh_host` |
| `git_repository.rs:375` | `scan_pulls()` | PrItem | `self.gh_host` |
| `git_repository.rs:461` | `scan_merges()` | MergeItem | `self.gh_host` |
| `git_repository.rs:619` | `startup_reconcile()` approved | IssueItem | `self.gh_host` |
| `git_repository.rs:638` | `startup_reconcile()` wip | IssueItem | `self.gh_host` |
| `git_repository.rs:668` | `startup_reconcile()` PR | PrItem | `self.gh_host` |
| `pipeline/issue.rs:444` | `process_ready()` | PrItem (issue→PR) | `item.gh_host` (IssueItem에서 전파) |
| `pipeline/issue.rs:905` | `implement_one()` | PrItem (issue→PR) | `item.gh_host` (IssueItem에서 전파) |

### 2-3. 파이프라인 소비 지점 — `load_merged(env, None)` 제거 대상

| 파일 | 함수 | 라인 | 변경 내용 |
|------|------|------|----------|
| `issue.rs` | `process_pending` | L89-91 | `cfg.consumer.gh_host` → `item.gh_host` |
| `issue.rs` | `process_ready` | L334-336 | `cfg.consumer.gh_host` → `item.gh_host` |
| `issue.rs` | `analyze_one` | L561-562 | `cfg.consumer.gh_host` → `item.gh_host` |
| `issue.rs` | `implement_one` | L792-793 | `cfg.consumer.gh_host` → `item.gh_host` |
| `pr.rs` | `process_pending` | L53-55 | `cfg.consumer.gh_host` → `item.gh_host` |
| `pr.rs` | `process_review_done` | L303-304 | `cfg.consumer.gh_host` → `item.gh_host` |
| `pr.rs` | `process_improved` | L440-441 | `cfg.consumer.gh_host` → `item.gh_host` |
| `pr.rs` | `review_one` | L701-702 | `cfg.consumer.gh_host` → `item.gh_host` |
| `pr.rs` | `improve_one` | L963-964 | `cfg.consumer.gh_host` → `item.gh_host` |
| `pr.rs` | `re_review_one` | L1105-1106 | `cfg.consumer.gh_host` → `item.gh_host` |
| `merge.rs` | `process_pending` | L34-36 | `cfg.consumer.gh_host` → `item.gh_host` |
| `merge.rs` | `merge_one` | L194-195 | `cfg.consumer.gh_host` → `item.gh_host` |

**주의**: `load_merged(env, None)` 자체를 완전히 제거하지 않음.
`concurrency`, `confidence_threshold`, `knowledge_extraction`, `max_iterations` 등
다른 config 값은 여전히 필요하므로 config 로딩은 유지하되, `gh_host`만 item에서 가져옴.

### 2-4. 테스트 파일

| 파일 | 영향 |
|------|------|
| `queue/task_queues.rs` 테스트 | `issue()`, `pr()`, `merge()` 헬퍼에 `gh_host: None` 추가 |
| `domain/git_repository.rs` 테스트 | IssueItem/PrItem 생성 부분에 `gh_host` 추가 |
| `pipeline/mod.rs` 테스트 | `make_issue()`, `make_pr()` 헬퍼에 `gh_host: None` 추가 |
| `daemon/mod.rs` 테스트 | IssueItem/PrItem/MergeItem 생성에 `gh_host: None` 추가 |
| `daemon/status.rs` 테스트 | IssueItem/PrItem 생성에 `gh_host: None` 추가 |
| `tests/issue_verdict_tests.rs` | IssueItem 생성에 `gh_host: None` 추가 |
| `tests/pipeline_e2e_tests.rs` | 모든 아이템 생성에 `gh_host: None` 추가 |
| `tests/daemon_consumer_tests.rs` | 모든 아이템 생성에 `gh_host: None` 추가 |
| `tests/daemon_recovery_tests.rs` | 모든 아이템 생성에 `gh_host: None` 추가 |
| `tests/resource_cleanup_tests.rs` | 모든 아이템 생성에 `gh_host: None` 추가 |
| `tests/autodev_marker_tests.rs` | 모든 아이템 생성에 `gh_host: None` 추가 |

---

## 3. 구현 설계

### 3-1. 큐 아이템에 `gh_host` 필드 추가

```rust
// queue/task_queues.rs

#[derive(Debug, Clone)]
pub struct IssueItem {
    // ... 기존 필드
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PrItem {
    // ... 기존 필드
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MergeItem {
    // ... 기존 필드
    /// GHE hostname (e.g. "git.example.com"). None이면 github.com.
    pub gh_host: Option<String>,
}
```

### 3-2. 스캐너 — 생성 시 gh_host 바인딩

```rust
// scanner/issues.rs — scan()
let item = IssueItem {
    // ... 기존 필드
    gh_host: gh_host.map(String::from),  // 함수 파라미터에서 바인딩
};

// scanner/pulls.rs — scan()
let item = PrItem {
    // ... 기존 필드
    gh_host: gh_host.map(String::from),
};
```

### 3-3. GitRepository — 생성 시 self.gh_host 바인딩

```rust
// domain/git_repository.rs — scan_issues(), scan_approved_issues(), scan_pulls(), scan_merges()
let item = IssueItem {
    // ... 기존 필드
    gh_host: self.gh_host.clone(),
};
```

### 3-4. 파이프라인 — item에서 gh_host 직접 사용

process_* 함수 (루프 내 여러 아이템 처리):

```rust
// 변경 전
let cfg = config::loader::load_merged(env, None);
let gh_host = cfg.consumer.gh_host.as_deref();
// ... 루프에서 gh_host 사용

// 변경 후
let cfg = config::loader::load_merged(env, None);
// gh_host는 cfg에서 가져오지 않음
// ... 루프 내에서:
let gh_host = item.gh_host.as_deref();
```

_one 함수 (단일 아이템 처리):

```rust
// 변경 전
let cfg = config::loader::load_merged(env, None);
let gh_host = cfg.consumer.gh_host.as_deref();

// 변경 후
let cfg = config::loader::load_merged(env, None);
let gh_host = item.gh_host.as_deref();
```

### 3-5. Issue → PR 전환 시 gh_host 전파

```rust
// pipeline/issue.rs — process_ready(), implement_one()
let pr_item = PrItem {
    // ... 기존 필드
    gh_host: item.gh_host.clone(),  // IssueItem에서 PrItem으로 전파
};
```

---

## 4. 구현 순서

```
Step 1: task_queues.rs — IssueItem, PrItem, MergeItem에 gh_host 필드 추가
Step 2: task_queues.rs 테스트 — 헬퍼 함수에 gh_host: None 추가
Step 3: scanner/issues.rs — scan(), scan_approved() 아이템 생성 시 gh_host 바인딩
Step 4: scanner/pulls.rs — scan(), scan_merges() 아이템 생성 시 gh_host 바인딩
Step 5: git_repository.rs — 6개 생성 지점에 self.gh_host.clone() 바인딩
Step 6: pipeline/issue.rs — 4개 함수에서 item.gh_host 사용 + PrItem 전파
Step 7: pipeline/pr.rs — 6개 함수에서 item.gh_host 사용
Step 8: pipeline/merge.rs — 2개 함수에서 item.gh_host 사용
Step 9: 나머지 테스트 파일 — 모든 아이템 생성에 gh_host: None 추가
Step 10: cargo fmt + clippy + test 전체 통과 확인
```

## 5. 테스트 계획

| 대상 | 테스트 |
|------|--------|
| `IssueItem.gh_host` | 스캐너에서 생성된 아이템에 gh_host가 바인딩되는지 확인 (기존 테스트 확장) |
| `PrItem.gh_host` | Issue→PR 전환 시 gh_host가 전파되는지 확인 (기존 테스트 확장) |
| `MergeItem.gh_host` | scan_merges()에서 gh_host 바인딩 확인 (기존 테스트 확장) |
| 기존 테스트 전체 | 모든 기존 테스트가 gh_host: None으로 통과하는지 확인 |
| `cargo clippy` | 경고 없음 확인 |

## 6. 설계 판단 근거

### "왜 config에서 gh_host를 제거하지 않는가?"

`ConsumerConfig.gh_host`는 스캐너/GitRepository가 최초 `gh_host`를 얻는 소스이므로 유지.
파이프라인만 item에서 가져오도록 변경하여, **config 로딩 경로 불일치** 문제를 구조적으로 제거.

### "왜 load_merged를 완전히 제거하지 않는가?"

파이프라인은 `gh_host` 외에도 `concurrency`, `confidence_threshold`, `knowledge_extraction`,
`workflow`, `max_iterations` 등 다른 config 값을 필요로 함. config 로딩 자체는 유지.

### "왜 도메인 객체(RepoIssue/RepoPull)를 큐 아이템에 포함하지 않는가?"

현재 스코프는 **gh_host 유실 버그**의 구조적 해결에 집중.
도메인 객체를 큐 아이템으로 통합하는 것은 PLAN.md의 Phase 4 범위이며,
해당 리팩토링은 큐 아이템 자체의 재설계를 수반하므로 별도 작업으로 분리.

### "daemon/recovery.rs는 왜 변경하지 않는가?"

`recovery.rs`는 `ResolvedRepo`를 직접 받아 `repo.gh_host()`를 사용한다.
큐 아이템을 생성하지 않으므로 이번 리팩토링에 영향 없음.

---

## 7. 로깅 강화 (infrastructure 레이어)

### 7-1. gh 호출 로깅 (`infrastructure/gh/real.rs`)

현재 상태: 실패 시 `warn!`만 출력. 호출 시작, 전체 args, duration, 응답 크기 없음.

**변경:**
- 호출 시작: `debug!` — 전체 args (hostname 포함 여부 확인 가능)
- 호출 성공: `debug!` — duration, stdout 크기
- 호출 실패: `warn!` — duration, exit code, stderr 내용
- `api_paginate` 실패: 기존 `bail!`에 stderr만 포함 → duration도 추가

```rust
// 예시 (모든 메서드에 적용)
let start = std::time::Instant::now();
tracing::debug!("[gh:{method}] >>> gh {}", args.join(" "));
let output = tokio::process::Command::new("gh").args(&args).output().await?;
let elapsed = start.elapsed();
if output.status.success() {
    tracing::debug!("[gh:{method}] <<< OK ({}ms, {} bytes)", elapsed.as_millis(), output.stdout.len());
} else {
    let stderr = String::from_utf8_lossy(&output.stderr);
    tracing::warn!("[gh:{method}] <<< FAILED (exit={}, {}ms): {stderr}", output.status.code().unwrap_or(-1), elapsed.as_millis());
}
```

**로그 레벨:**
| 상황 | 레벨 | 이유 |
|------|------|------|
| gh 호출 시작/성공 | `DEBUG` | tick당 수십 건 — INFO는 과도 |
| gh 호출 실패 | `WARN` | 조치 필요 (기존과 동일) |

### 7-2. Claude 호출 로깅 (`infrastructure/claude/real.rs`)

현재 상태: 시작 `info!`(프롬프트 80자), exit≠0 `warn!`(stderr). 완료 로그, duration 없음.

**변경:**
- 호출 완료: `info!` — exit code, duration, stdout/stderr 크기
- 비정상 종료: `warn!` → `error!`

```rust
let start = std::time::Instant::now();
tracing::info!("[claude] >>> claude -p \"{}\" in {:?} (args={})", truncate(prompt, 80), cwd, args.len());
let result = tokio::process::Command::new("claude")...;
let elapsed = start.elapsed();
if exit_code == 0 {
    tracing::info!("[claude] <<< OK ({}ms, stdout={} bytes, stderr={} bytes)", elapsed.as_millis(), stdout.len(), stderr.len());
} else {
    tracing::error!("[claude] <<< FAILED (exit={exit_code}, {}ms, stdout={} bytes): {}", elapsed.as_millis(), stdout.len(), truncate(&stderr, 200));
}
```

### 7-3. 재시도(retry) 로직 — 이번 스코프 제외

재시도 로직은 별도 이슈로 분리. 현재 `real.rs`에는 재시도 로직이 없으므로
attempt/backoff 관련 로깅은 추후 재시도 구현 시 함께 추가.

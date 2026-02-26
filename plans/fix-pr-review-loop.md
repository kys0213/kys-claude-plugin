# PR Review-Implement 무한 루프 수정 (Issue #120)

> **Date**: 2026-02-25
> **Scope**: `PrItem`에 iteration counter 추가 + `max_iterations` 가드 적용 + 외부 PR 자동수정 차단
> **Status**: 설계 검토 중

---

## 1. 요구사항 정리

### 변경 1: review_iteration counter + max_iterations 가드

`PrItem`에 `review_iteration: u32` 필드를 추가하고, `improve_one()` / `re_review_one()`에서 상한 초과 시 루프를 중단한다.

**현재**: `review_one()` → `improve_one()` → `re_review_one()` → `improve_one()` → ... (무한)
**목표**: `max_iterations` (기본 2) 초과 시 `autodev:skip` 라벨 부착 후 큐에서 제거

```
review_one (iteration 0)
  → request_changes → ReviewDone
    → improve_one (iteration 0→1) → Improved
      → re_review_one (iteration 1)
        → request_changes → ReviewDone
          → improve_one (iteration 1→2) → Improved
            → re_review_one (iteration 2)
              → request_changes → iteration(2) >= max(2) → SKIP + done
```

### 변경 2: 외부 PR 자동수정 차단

`source_issue_number`가 없는 PR (외부 기여자 PR)은 리뷰 댓글만 달고 자동수정을 시도하지 않는다.

**현재**: 모든 PR이 `ReviewDone` → `improve_one()` 흐름에 진입
**목표**: `source_issue_number.is_none()` PR → 리뷰 후 `autodev:done` 라벨 부착, 큐에서 제거

---

## 2. 사이드이펙트 조사

### 변경 1: review_iteration

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| `PrItem` (task_queues.rs:31) | 필드 추가 `review_iteration: u32` | `Default`=0, clone 시 보존 |
| `review_one()` (pr.rs:600) | RequestChanges 경로에서 `item.review_iteration` 유지 (아직 0) | 변경 없음 (첫 리뷰) |
| `improve_one()` (pr.rs:842) | Improved 전이 전에 `item.review_iteration += 1` | counter 증가 |
| `re_review_one()` (pr.rs:960) | RequestChanges 시 `max_iterations` 체크 | 초과 시 skip 처리 |
| `daemon/mod.rs:142-159` | `spawn_ready_tasks`에서 `REVIEW_DONE` pop → `improve_one` spawn | **외부 PR 가드 추가** |
| scanner/pulls.rs | `PrItem` 생성 시 `review_iteration: 0` 추가 | 기본값 추가 |
| scanner/issues.rs | issue→PR 전환 시 `PrItem` 생성 | `review_iteration: 0` 추가 |
| pipeline/issue.rs | `QueueOp::PushPr` 호출부 | `review_iteration: 0` 추가 |
| 기존 테스트 | `PrItem` 생성하는 모든 테스트 코드 | `review_iteration: 0` 추가 |
| startup_reconcile | PR 복구 시 `PrItem` 생성 | `review_iteration: 0` (리셋) |

### 변경 2: 외부 PR 차단

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| `review_one()` | RequestChanges 경로에서 외부 PR 분기 | `source_issue_number.is_none()` → done 처리 |
| `daemon/mod.rs:142-159` | ReviewDone pop 시 외부 PR 필터 불필요 | review_one에서 이미 차단하므로 도달 안함 |

---

## 3. 구현 설계

### 3-1. PrItem 필드 추가

```rust
// queue/task_queues.rs
pub struct PrItem {
    // ... 기존 필드
    /// 리뷰→수정 반복 횟수 (improve_one에서 +1)
    pub review_iteration: u32,
}
```

### 3-2. review_one() — 외부 PR 차단

```rust
// pr.rs review_one() — RequestChanges 분기 내
Some(ReviewVerdict::RequestChanges) | None => {
    // GitHub 리뷰/댓글 게시 (기존 로직 유지)
    ...

    // 외부 PR: 리뷰 댓글만 달고 done 처리
    if item.source_issue_number.is_none() {
        gh.label_remove(&item.repo_name, github_number, labels::WIP, gh_host).await;
        gh.label_add(&item.repo_name, github_number, labels::DONE, gh_host).await;
        tracing::info!("PR #{github_number}: external PR, review-only → done");
        ops.push(QueueOp::Remove);
    } else {
        // 내부 PR: 기존 ReviewDone 루프 진입
        item.review_comment = Some(output.review);
        ops.push(QueueOp::Remove);
        ops.push(QueueOp::PushPr { phase: pr_phase::REVIEW_DONE, item });
    }
}
```

### 3-3. improve_one() — iteration 증가

```rust
// pr.rs improve_one() — exit_code == 0 성공 경로
if res.exit_code == 0 {
    item.review_iteration += 1;  // 반복 횟수 증가
    ops.push(QueueOp::Remove);
    ops.push(QueueOp::PushPr { phase: pr_phase::IMPROVED, item });
}
```

### 3-4. re_review_one() — max_iterations 가드

```rust
// pr.rs re_review_one() — RequestChanges 분기 내
Some(ReviewVerdict::RequestChanges) | None => {
    let max_iterations = cfg.develop.review.max_iterations;

    if item.review_iteration >= max_iterations {
        // 상한 초과: skip 처리
        gh.label_remove(&repo_name, github_number, labels::WIP, gh_host).await;
        gh.label_add(&repo_name, github_number, labels::SKIP, gh_host).await;
        let comment = format!(
            "<!-- autodev:skip -->\n\
             ## Autodev: Review iteration limit reached\n\n\
             Reached maximum review iterations ({max_iterations}). \
             Marking as `autodev:skip`. Manual intervention required."
        );
        gh.comment_create(&repo_name, github_number, &comment, gh_host).await;
        tracing::info!(
            "PR #{github_number}: iteration limit ({max_iterations}) reached → skip"
        );
        ops.push(QueueOp::Remove);
    } else {
        // 기존 로직: ReviewDone 재진입
        item.review_comment = Some(output.review);
        ops.push(QueueOp::Remove);
        ops.push(QueueOp::PushPr { phase: pr_phase::REVIEW_DONE, item });
    }
}
```

---

## 4. 구현 순서

```
Step 1: PrItem에 review_iteration: u32 필드 추가 (task_queues.rs)
Step 2: PrItem 생성하는 모든 곳에 review_iteration: 0 추가 (scanner, pipeline, tests)
Step 3: review_one() — 외부 PR (source_issue_number.is_none()) 차단
Step 4: improve_one() — item.review_iteration += 1
Step 5: re_review_one() — max_iterations 가드 + skip 처리
Step 6: 테스트 작성 + 기존 테스트 수정
```

## 5. 테스트 계획

| 대상 | 테스트 |
|------|--------|
| `review_one` 외부 PR | `source_issue_number: None` + `RequestChanges` → done 처리, ReviewDone 미진입 |
| `review_one` 내부 PR | `source_issue_number: Some(10)` + `RequestChanges` → ReviewDone 진입 |
| `re_review_one` 가드 | `review_iteration >= max_iterations` → skip 라벨 + 큐 제거 |
| `re_review_one` 통과 | `review_iteration < max_iterations` → ReviewDone 재진입 |
| `improve_one` counter | 성공 시 `review_iteration` 증가 확인 |
| 기존 E2E | `pipeline_e2e_tests.rs` 기존 테스트 통과 확인 |

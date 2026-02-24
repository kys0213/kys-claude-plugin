# PR Review API 구현 (H-1)

> **Priority**: High — GitHub PR UI에서 리뷰 상태 미반영
> **분석 리포트**: design-implementation-analysis.md §3-5
> **난이도**: 중간

## 배경

설계에서는 PR 리뷰 시 `gh pr review --approve` / `POST /pulls/{N}/reviews` API를 호출하도록 명시했으나,
구현에서는 `issue_comment`로 댓글만 게시하고 있음. GitHub PR UI에서 "Approved" / "Changes Requested" 상태가 설정되지 않는 UX 차이 존재.

현재 merge scan이 `autodev:done` 라벨 기반이라 기능적으로는 동작하지만, GitHub의 공식 리뷰 시스템과 통합되지 않음.

## 항목

- [ ] **1. Gh trait에 `pr_review()` 메서드 추가**
  - `infrastructure/gh/mod.rs` — `async fn pr_review(&self, repo: &str, number: i64, event: &str, body: &str, host: Option<&str>) -> bool`
  - event: `"APPROVE"` | `"REQUEST_CHANGES"` | `"COMMENT"`

- [ ] **2. RealGh 구현**
  - `infrastructure/gh/real.rs` — `gh pr review {number} --approve -b "{body}"` 또는 `gh api` 호출
  - approve: `gh pr review {N} --approve -b "{summary}"`
  - request_changes: `gh api repos/{owner}/{repo}/pulls/{N}/reviews -f event=REQUEST_CHANGES -f body="{comment}"`

- [ ] **3. MockGh 구현**
  - `infrastructure/gh/mock.rs` — `reviews: Arc<Mutex<Vec<(repo, number, event, body)>>>`

- [ ] **4. pipeline/pr.rs 리팩토링**
  - ReviewDone 전이 시 verdict에 따라 `gh.pr_review()` 호출
  - `approve` → `pr_review(APPROVE)` + 라벨 전이
  - `request_changes` → `pr_review(REQUEST_CHANGES)` + Improving 전이

- [ ] **5. 테스트 추가**
  - MockGh에 pr_review 호출 기록 검증
  - approve/request_changes 시나리오 각각 테스트

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `infrastructure/gh/mod.rs` | `pr_review()` trait 메서드 추가 |
| `infrastructure/gh/real.rs` | gh CLI 기반 구현 |
| `infrastructure/gh/mock.rs` | 테스트용 mock 구현 |
| `pipeline/pr.rs` | verdict → pr_review 호출 연결 |
| `tests/component_tests.rs` | pr_review 시나리오 테스트 |

## 완료 조건

- [ ] `gh pr review --approve` 호출로 GitHub PR UI에서 Approved 표시
- [ ] request_changes 시 Changes Requested 표시
- [ ] 기존 댓글 게시 기능 유지 (리뷰 + 댓글 병행)
- [ ] cargo test 통과

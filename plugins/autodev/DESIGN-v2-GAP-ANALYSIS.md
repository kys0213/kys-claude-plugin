# DESIGN-v2 Gap Analysis: 설계 vs 구현

> **Date**: 2026-02-25
> **Scope**: DESIGN-v2.md의 모든 섹션과 실제 구현 코드를 1:1 비교

---

## 요약

| 구분 | 갭 수 | 심각도 |
|------|-------|--------|
| 크래시 안전성 위반 | 1 | **높음** |
| 의도적 미구현 (사람 판단) | 1 | 정보 |
| 설계-구현 불일치 | 3 | 중간 |
| **전체** | **5** | |

---

## Gap 1: `scan_approved()` 라벨 전이 순서 — 크래시 안전성 위반

**심각도**: 높음

### 설계 (DESIGN-v2 Section 4)

```rust
// 주의: implementing을 먼저 추가한 후 approved-analysis를 제거한다.
// 이 순서가 중요한 이유: 두 API 호출 사이에 크래시 발생 시,
// "라벨 없음" 상태(→ 재분석)를 방지하고 "양쪽 다 있는" 상태(→ 안전)를 보장.
gh.label_add(repo_name, number, labels::IMPLEMENTING, gh_host).await;
gh.label_remove(repo_name, number, labels::APPROVED_ANALYSIS, gh_host).await;
```

### 구현 (`scanner/issues.rs:192-198`)

```rust
// 현재 구현: 제거 먼저 → 추가 나중 (크래시 위험)
gh.label_remove(repo_name, issue.number, labels::APPROVED_ANALYSIS, gh_host).await;
gh.label_remove(repo_name, issue.number, labels::ANALYZED, gh_host).await;
gh.label_add(repo_name, issue.number, labels::IMPLEMENTING, gh_host).await;
```

### 동일 문제: `startup_reconcile()` (`daemon/mod.rs:302-306`)

```rust
// reconcile에서도 동일한 역순
gh.label_remove(&repo.name, number, labels::APPROVED_ANALYSIS, gh_host).await;
gh.label_remove(&repo.name, number, labels::ANALYZED, gh_host).await;
gh.label_add(&repo.name, number, labels::IMPLEMENTING, gh_host).await;
```

### 위험 시나리오

```
1. approved-analysis 제거 완료
2. ── 크래시 발생 ──
3. analyzed도 제거된 상태, implementing 미추가
4. 이슈에 autodev 라벨 없음
5. 다음 scan()에서 새 이슈로 인식 → 재분석 (사람의 승인이 무효화됨)
```

### 수정 방향

```rust
// 올바른 순서: 추가 먼저, 제거 나중
gh.label_add(repo_name, issue.number, labels::IMPLEMENTING, gh_host).await;
gh.label_remove(repo_name, issue.number, labels::APPROVED_ANALYSIS, gh_host).await;
gh.label_remove(repo_name, issue.number, labels::ANALYZED, gh_host).await;
```

크래시 발생 시: `implementing` + `approved-analysis` 양쪽 라벨이 존재 → `scan_approved()`의 dedup이 중복 적재 방지 → 안전.

---

## Gap 2: Safety Valve (재분석 무한루프 방지) — 의도적 미구현

**심각도**: 정보 (사람이 판단하기로 결정)

### 설계 (DESIGN-v2 Section 4)

```rust
const MAX_ANALYSIS_ATTEMPTS: usize = 3;

let analysis_count = count_analysis_comments(gh, repo_name, number, gh_host).await;

if analysis_count >= MAX_ANALYSIS_ATTEMPTS {
    gh.label_add(repo_name, number, labels::SKIP, gh_host).await;
    // "max analysis attempts reached" 코멘트
    continue;
}
```

### 구현 (`scanner/issues.rs`)

`count_analysis_comments()`, `MAX_ANALYSIS_ATTEMPTS` 모두 미구현. `scan()` 함수에 분석 횟수 체크 로직 없음.

### 판단

무한루프 방지는 **사람이 직접 판단**하기로 결정. 분석이 반복되면 사람이 `autodev:skip` 라벨을 직접 추가하거나, 이슈를 닫는 방식으로 대응.

DESIGN-v2 문서에서 해당 섹션을 제거하거나 "사람 판단으로 대체" 주석을 추가하면 설계-구현 간 문서 일관성이 유지됨.

---

## Gap 3: Knowledge PR — suggestion type 필터 누락

**심각도**: 중간

### 설계 (DESIGN-v2 Section 8)

```rust
// Skill/Subagent 타입만 actionable PR로 생성
let actionable: Vec<&Suggestion> = suggestion.suggestions.iter()
    .filter(|s| matches!(s.suggestion_type, SuggestionType::Skill | SuggestionType::Subagent))
    .collect();

if !actionable.is_empty() {
    create_knowledge_pr(gh, git, repo_name, &actionable, ...).await;
}
```

### 구현 (`knowledge/extractor.rs:210-221`)

```rust
// 모든 suggestion type에 대해 PR 생성 (필터 없음)
create_task_knowledge_prs(gh, workspace, repo_name, ks, task_type, github_number, gh_host).await;
```

### 영향

`Rule`, `ClaudeMd`, `Hook` 등 단순 텍스트 추천도 PR로 생성됨. 설계 의도는 **파일을 직접 커밋할 수 있는** Skill/Subagent만 PR로 만들고, 나머지는 코멘트로만 게시하는 것.

### 수정 방향

`create_task_knowledge_prs()` 호출 전에 Skill/Subagent 타입만 필터링:

```rust
let actionable: Vec<_> = ks.suggestions.iter()
    .filter(|s| matches!(s.suggestion_type, SuggestionType::Skill | SuggestionType::Subagent))
    .collect();
if !actionable.is_empty() {
    create_task_knowledge_prs(gh, workspace, repo_name, &actionable, ...).await;
}
```

또는, 모든 타입에서 PR을 생성하는 현재 방식이 더 유용하다면 DESIGN-v2 문서를 수정.

---

## Gap 4: Daily Knowledge PR — Worktree 격리 누락

**심각도**: 중간

### 설계 (DESIGN-v2 Section 8)

Knowledge PR 생성 시 **별도 worktree**에서 작업하여 구현 worktree와 격리:

```
Knowledge PR:
  create_worktree(task_id, "main")  ← 별도 격리 worktree
  → branch 생성 + 파일 작성 + PR 생성
  → remove_worktree()
```

### 구현

| 함수 | Worktree 격리 | 일치 여부 |
|------|--------------|----------|
| `create_task_knowledge_prs()` (per-task) | ✅ 별도 worktree 생성 | 일치 |
| `create_knowledge_prs()` (daily) | ❌ `base_path`에서 직접 작업 | **불일치** |

### 영향

Daily knowledge PR 생성 시 `base_path`에서 직접 `checkout_new_branch()`를 호출하면:
- 해당 경로의 현재 branch가 변경됨
- 다른 작업이 동시에 해당 경로를 사용하면 충돌 가능
- 연속으로 여러 PR을 생성할 때 이전 branch의 변경이 다음 branch에 누적

### 수정 방향

`create_knowledge_prs()`도 `create_task_knowledge_prs()`와 동일하게 per-suggestion worktree 격리 패턴 적용.

---

## Gap 5: `detect_cross_task_patterns()` — ReviewCycle 패턴 미구현

**심각도**: 낮음

### 설계 (DESIGN-v2 Section 8)

```rust
// 같은 type (skill/subagent)이 3회 이상 → Pattern { type: ReviewCycle, ... }
```

### 구현 (`knowledge/daily.rs:393-421`)

`target_file` 기준 그룹핑으로 **Hotfile** 패턴만 감지. `suggestion_type` 기준의 **ReviewCycle** 패턴은 미구현.

`PatternType::ReviewCycle`은 `models.rs`에 정의되어 있으나 실제로 사용되지 않음.

### 수정 방향

```rust
// suggestion_type 기준 그룹핑 추가
let mut type_counts: HashMap<SuggestionType, u32> = HashMap::new();
for s in suggestions {
    *type_counts.entry(s.suggestion_type.clone()).or_default() += 1;
}
for (st, count) in &type_counts {
    if *count >= 3 && matches!(st, SuggestionType::Skill | SuggestionType::Subagent) {
        patterns.push(Pattern {
            pattern_type: PatternType::ReviewCycle,
            description: format!("{st:?} suggested {count} times across tasks"),
            occurrences: *count,
            affected_tasks: vec![],
        });
    }
}
```

---

## 일치 확인 (갭 없음)

아래 항목들은 설계와 구현이 일치합니다:

| 항목 | 설계 | 구현 | 상태 |
|------|------|------|------|
| 라벨 상수 (ANALYZED, APPROVED_ANALYSIS, IMPLEMENTING) | Section 6 | `task_queues.rs:118-120` | ✅ |
| PrItem.source_issue_number | Section 6 | `task_queues.rs:45` | ✅ |
| process_pending() → analyzed + exit queue | Section 5 | `pipeline/issue.rs` | ✅ |
| format_analysis_comment() with `<!-- autodev:analysis -->` | Section 5 | `components/verdict.rs` | ✅ |
| scan_approved() → Ready 큐 적재 | Section 4 | `scanner/issues.rs:158-228` | ✅ |
| scan_all()에 scan_approved() 호출 | Section 4 | `scanner/mod.rs` | ✅ |
| extract_pr_number() (URL + JSON) | Section 5 | `infrastructure/claude/output.rs` | ✅ |
| find_existing_pr() fallback | Section 5 | `pipeline/issue.rs:21-34` | ✅ |
| process_ready() → PR 생성 + PR queue push | Section 5 | `pipeline/issue.rs` | ✅ |
| `<!-- autodev:pr-link:{N} -->` 코멘트 | Section 5 | `pipeline/issue.rs` | ✅ |
| PR approve → source_issue done 전이 | Section 5 | `pipeline/pr.rs:202, 532` | ✅ |
| startup_reconcile() 라벨 필터 (analyzed, implementing skip) | Section 7 | `daemon/mod.rs` | ✅ |
| recover_orphan_implementing() + pr-link 마커 | Section 7 | `daemon/recovery.rs:72-150` | ✅ |
| PR pipeline worktree cleanup | Section 9 | `pipeline/pr.rs` (4 calls) | ✅ |
| collect_existing_knowledge() | Section 8 | `knowledge/extractor.rs:17-109` | ✅ |
| Delta-aware prompt (기존 지식 포함) | Section 8 | `knowledge/extractor.rs:154-180` | ✅ |
| Per-task knowledge PR (격리 worktree) | Section 8 | `knowledge/extractor.rs:298-388` | ✅ |
| aggregate_daily_suggestions() | Section 8 | `knowledge/daily.rs:370-387` | ✅ |
| Daily integration sequence in daemon loop | Section 8 | `daemon/mod.rs:136-178` | ✅ |

---

## 조치 우선순위

| 순위 | Gap | 이유 |
|------|-----|------|
| 1 | Gap 1: 라벨 전이 순서 | 크래시 시 사람의 승인이 유실됨. 코드 3줄 순서만 변경하면 수정 가능 |
| 2 | Gap 3: Knowledge PR 타입 필터 | 불필요한 PR noise 발생. 필터 추가 또는 설계 문서 수정 |
| 3 | Gap 4: Daily worktree 격리 | branch 오염 가능성. per-task과 동일 패턴 적용 |
| 4 | Gap 5: ReviewCycle 패턴 | 사용되지 않는 enum variant. 구현 추가 또는 variant 제거 |
| 정보 | Gap 2: Safety Valve | 사람이 판단하기로 결정. 설계 문서만 갱신 |

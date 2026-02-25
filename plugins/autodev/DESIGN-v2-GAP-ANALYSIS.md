# DESIGN-v2 Gap Analysis: 설계 vs 구현

> **Date**: 2026-02-25
> **Scope**: DESIGN-v2.md의 모든 섹션과 실제 구현 코드를 1:1 비교

---

## 요약

| 구분 | 갭 수 | 심각도 |
|------|-------|--------|
| 설계 변경 (Label-Positive 전환) | 1 | **높음** → 설계로 해소 |
| 의도적 미구현 (사람 판단) | 1 | 정보 (Gap 1로 자연 해소) |
| 설계 문서 갱신 필요 (구현이 더 나음) | 1 | 낮음 |
| 설계-구현 불일치 | 2 | 중간 |
| **전체** | **5** | |

---

## Gap 1: `scan()` Label-Negative 방식 — 크래시 안전성 구조적 결함

**심각도**: 높음 → **설계 변경으로 해소**

### 문제

현재 `scan()`은 **Label-Negative** 방식: `autodev:*` 라벨이 없는 이슈를 분석 대상으로 잡음.

```rust
// scanner/issues.rs:81-90
if has_autodev_label(&issue.labels) {
    continue; // skip
}
// → autodev 라벨이 없으면 분석 대상
```

이 방식에서는 라벨 전이 중 크래시가 발생하면:

```
1. approved-analysis 제거 완료
2. analyzed 제거 완료
3. ── 크래시 ──
4. implementing 미추가
5. 이슈에 autodev 라벨 없음 → scan()이 새 이슈로 인식 → 재분석
```

설계(DESIGN-v2)에서는 "추가 먼저, 제거 나중" 순서로 완화하려 했지만, 이는 **증상 완화**일 뿐 근본 해결이 아님.

### 근본 원인

Label-Negative 방식 자체가 문제:
- "라벨이 없는 상태"가 "새 이슈"와 "크래시로 라벨 유실된 이슈"를 구분할 수 없음
- 라벨 전이 순서를 아무리 조심해도, 어떤 경로에서든 모든 라벨이 사라지면 재분석 위험 존재

### 수정 방향: Label-Positive 전환 (트리거 라벨 도입)

`scan()`을 **Label-Positive** 방식으로 전환: 명시적 트리거 라벨이 있는 이슈만 분석 대상으로 잡음.

```
[Before] Label-Negative
scan(): autodev:* 라벨 없음 → 분석 대상
→ 크래시로 라벨 유실 시 재분석 위험

[After] Label-Positive
scan(): autodev:analyze 라벨 있음 → 분석 대상
→ 크래시로 라벨 유실 시 아무 일도 안 일어남 (안전)
```

#### 새 라벨: `autodev:analyze`

```rust
pub mod labels {
    pub const ANALYZE: &str = "autodev:analyze";   // 트리거 (사람이 붙임)
    pub const WIP: &str = "autodev:wip";
    pub const ANALYZED: &str = "autodev:analyzed";
    pub const APPROVED_ANALYSIS: &str = "autodev:approved-analysis";
    pub const IMPLEMENTING: &str = "autodev:implementing";
    pub const DONE: &str = "autodev:done";
    pub const SKIP: &str = "autodev:skip";
}
```

#### 변경되는 라벨 전이

```
[사람]                    [autodev]
autodev:analyze 붙임  →  scan()이 감지
                          → analyze 제거 + wip 추가
                          → 분석 시작
                          → wip 제거 + analyzed 추가
                          → 분석 코멘트 게시
[사람]
approved-analysis 붙임 →  scan_approved()가 감지
                          → implementing 추가 (기존 라벨 제거)
                          → PR 생성
```

#### 변경되는 scan() 필터

```rust
// Before: Label-Negative
fn scan() {
    let issues = gh.list_issues(repo, state: "open").await;
    for issue in issues {
        if has_autodev_label(&issue.labels) { continue; } // 라벨 없으면 대상
    }
}

// After: Label-Positive
fn scan() {
    let issues = gh.list_issues(repo, state: "open", labels: "autodev:analyze").await;
    for issue in issues {
        // autodev:analyze가 있는 것만 가져옴 → 트리거 명시적
    }
}
```

#### 크래시 안전성 비교

| 시나리오 | Label-Negative | Label-Positive |
|----------|---------------|----------------|
| 정상 동작 | ✅ | ✅ |
| 라벨 전이 중 크래시 (라벨 전부 유실) | ❌ 재분석 | ✅ 무시 (안전) |
| 라벨 전이 중 크래시 (일부만 유실) | 순서에 따라 다름 | ✅ 무시 (안전) |
| 새 이슈 자동 분석 | ✅ 자동 | ❌ 사람이 라벨 필요 |

#### 부수 효과

1. **Gap 2 (Safety Valve) 자연 해소**: 재분석 자체가 불가능하므로 무한루프 방지 로직 불필요
2. **`filter_labels` 설정 단순화**: 트리거 라벨이 `filter_labels` 역할을 대체
3. **startup_reconcile()의 라벨 순서 문제도 해소**: 라벨이 없으면 아무 일도 안 생김

#### 영향받는 코드

| 파일 | 변경 내용 |
|------|----------|
| `scanner/issues.rs` `scan()` | GitHub API 호출에 `labels: "autodev:analyze"` 필터 추가 |
| `scanner/issues.rs` `scan()` | `has_autodev_label()` 체크 제거 (API 레벨에서 필터됨) |
| `scanner/issues.rs` `scan()` | `analyze` 라벨 제거 + `wip` 라벨 추가 로직 |
| `queue/task_queues.rs` | `labels::ANALYZE` 상수 추가 |
| `daemon/mod.rs` `startup_reconcile()` | 라벨 전이 순서 고려 불필요 (선택적 정리) |
| DESIGN-v2.md | Section 4 scan() 설명 갱신 |

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

## Gap 3: Knowledge PR — 설계 문서의 타입 필터가 불필요

**심각도**: 낮음 (구현이 설계보다 나음 → 설계 문서 갱신 필요)

### 설계 (DESIGN-v2 Section 8)

```rust
// Skill/Subagent 타입만 actionable PR로 생성
let actionable: Vec<&Suggestion> = suggestion.suggestions.iter()
    .filter(|s| matches!(s.suggestion_type, SuggestionType::Skill | SuggestionType::Subagent))
    .collect();
```

### 구현 (`knowledge/extractor.rs:210-221`)

```rust
// 모든 suggestion type에 대해 PR 생성 (필터 없음)
create_task_knowledge_prs(gh, workspace, repo_name, ks, task_type, github_number, gh_host).await;
```

### 판단: 구현이 올바름

5개 타입 모두 실제 파일 변경이 필요한 actionable 항목이다:

| Type | 대상 파일 | 파일 변경 |
|------|----------|----------|
| `Rule` | `.claude/rules/*.md` | ✅ 파일 생성 |
| `ClaudeMd` | `CLAUDE.md` | ✅ 파일 수정 |
| `Hook` | `.claude/hooks.json` | ✅ 파일 수정 |
| `Skill` | `plugins/*/commands/*.md` | ✅ 파일 생성 |
| `Subagent` | `.develop-workflow.yaml` | ✅ 파일 수정 |

코멘트만 남기면 사람이 직접 파일을 만들어야 하므로, PR로 올리는 현재 구현이 더 실용적이다.

### 수정 방향

DESIGN-v2 Section 8의 Skill/Subagent 필터를 제거하고, 모든 suggestion type에 대해 PR을 생성하도록 설계 문서를 갱신.

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
| 1 | Gap 1: Label-Positive 전환 | 크래시 안전성을 구조적으로 해소. scan() 필터 + 트리거 라벨 도입 |
| 2 | Gap 4: Daily worktree 격리 | branch 오염 가능성. per-task과 동일 패턴 적용 |
| 3 | Gap 5: ReviewCycle 패턴 | 사용되지 않는 enum variant. 구현 추가 또는 variant 제거 |
| 문서 | Gap 3: Knowledge PR 타입 필터 | 구현이 설계보다 나음. DESIGN-v2 문서만 갱신 |
| 해소 | Gap 2: Safety Valve | Gap 1의 Label-Positive 전환으로 재분석 자체가 불가능 → 자연 해소 |

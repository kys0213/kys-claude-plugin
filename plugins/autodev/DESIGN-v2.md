# DESIGN v2: Implement Workflow (Analysis Review + Issue-PR Linkage)

> **Date**: 2026-02-24
> **Base**: DESIGN.md (v1) — 3-Tier 상태 관리, 메인 루프, workspace 등 기존 아키텍처 유지
> **변경 범위**: Issue Flow에 분석 리뷰 게이트 추가, Issue-PR 연동, 라벨 세분화

---

## 1. 변경 동기

### v1의 한계

v1 Issue Flow는 분석 → 구현이 자동으로 연결된다:

```
v1: Pending → Analyzing → Ready → Implementing → done
                 (자동)      (자동)     (자동)
```

문제:
- 분석 품질이 낮아도 곧바로 구현에 진입 → 잘못된 방향의 구현 → 리소스 낭비
- 구현 결과(PR)와 원본 이슈의 연결 고리가 없음
- PR 리뷰가 끝나도 이슈 상태는 수동으로 관리해야 함

### v2 목표

1. **분석 리뷰 게이트 (HITL)**: 분석 완료 후 사람이 검토/승인해야 구현 진행
2. **Issue-PR 연동**: 이슈에서 생성된 PR이 approve되면 이슈도 자동으로 done
3. **세분화된 라벨**: 이슈의 현재 상태를 GitHub UI에서 명확히 파악 가능

---

## 2. Label Scheme v2

### Issue 라벨

| 라벨 | 의미 | 전이 조건 |
|------|------|----------|
| `autodev:wip` | 분석 진행중 | scanner가 새 이슈 발견 |
| `autodev:analyzed` | 분석 완료, **사람 리뷰 대기** | 분석 성공 시 |
| `autodev:approved-analysis` | 사람이 분석 승인, **구현 대기** | 사람이 라벨 변경 |
| `autodev:implementing` | PR 생성됨, **PR 리뷰 진행중** | 구현 + PR 생성 성공 시 |
| `autodev:done` | 완료 | PR approve 시 (자동) |
| `autodev:skip` | HITL 대기/제외 | clarify/wontfix |

### PR 라벨 (v1과 동일)

| 라벨 | 의미 |
|------|------|
| `autodev:wip` | 리뷰 진행중 |
| `autodev:done` | 리뷰 approve 완료 |
| `autodev:skip` | 제외 |

### 전체 라벨 상태 전이

```
Issue:
(없음) ─scan─→ autodev:wip ─analysis─→ autodev:analyzed
                    │                       │
                    ├──skip──→ autodev:skip  │ ← 사람 리뷰 대기
                    │                       │
                    │              ┌────────────────────────┐
                    │              │  사람이 라벨 변경        │
                    │              │  analyzed 제거           │
                    │              │  approved-analysis 추가  │
                    │              └────────┬───────────────┘
                    │                       │
                    │                       ▼
                    │            autodev:approved-analysis
                    │                       │
                    │              ┌────────────────────┐
                    │              │  daemon이 감지       │
                    │              │  scan_approved()     │
                    │              └────────┬───────────┘
                    │                       │
                    │                       ▼
                    │            autodev:implementing ←─ PR 생성됨
                    │                       │
                    │              ┌────────────────────┐
                    │              │  PR approve 시      │
                    │              │  PR pipeline이 전이  │
                    │              └────────┬───────────┘
                    │                       │
                    │                       ▼
                    └───failure──→ (없음)   autodev:done

PR:
(없음) ─scan─→ autodev:wip ─approve─→ autodev:done
                    │                      │
                    └──failure──→ (없음)    └─→ source_issue도 done 전이

사람이 분석을 reject하는 경우:
autodev:analyzed → (사람이 코멘트 + analyzed 라벨 제거)
                 → (없음) → 다음 scan에서 재발견 → 재분석
                    (이전 코멘트가 context로 포함되어 분석 품질 향상)
```

---

## 3. Issue Flow v2

### 전체 흐름

```
┌─────────────────────────────────────────────────────────────────────┐
│  Phase 1: Analysis (자동)                                           │
│                                                                     │
│  Scanner: 새 이슈 (no autodev label)                                │
│  → autodev:wip + queue[Pending]                                    │
│  → Analyze → 분석 리포트를 이슈 코멘트로 게시                         │
│  → autodev:wip → autodev:analyzed                                  │
│  → queue에서 제거 (사람 리뷰 대기)                                    │
└─────────────────────────────────────┬───────────────────────────────┘
                                      │
              ┌───────────────────────▼──────────────────────┐
              │  Gate: Human Review (수동)                     │
              │                                               │
              │  사람이 분석 리포트를 검토:                      │
              │    ✅ 승인 → autodev:approved-analysis 라벨 추가 │
              │    ❌ 거부 → analyzed 라벨 제거 + 피드백 코멘트  │
              │              (다음 scan에서 재분석)              │
              └───────────────────────┬──────────────────────┘
                                      │
┌─────────────────────────────────────▼───────────────────────────────┐
│  Phase 2: Implementation (자동)                                      │
│                                                                      │
│  Scanner: autodev:approved-analysis 라벨 감지                        │
│  → approved-analysis 제거, autodev:implementing 추가                  │
│  → queue[Ready]에 push                                               │
│  → Implement → PR 생성 (body에 Closes #N 포함)                       │
│  → PR에 autodev:wip 라벨 + PR queue[Pending]에 직접 push             │
│  → queue에서 issue 제거 (PR 리뷰 대기)                                │
└─────────────────────────────────────┬───────────────────────────────┘
                                      │
┌─────────────────────────────────────▼───────────────────────────────┐
│  Phase 3: PR Review Loop (자동, 기존 v1 메커니즘)                     │
│                                                                      │
│  PR queue[Pending] → Reviewing → verdict 분기                        │
│    approve → autodev:done (PR)                                       │
│    request_changes → ReviewDone → Improving → Improved → re-review   │
│                                                                      │
│  PR approve 시:                                                      │
│    source_issue_number가 있으면 →                                     │
│      Issue: autodev:implementing → autodev:done                      │
└──────────────────────────────────────────────────────────────────────┘
```

### Issue Phase 정의 v2

```
Issue Phase (v2):
  Pending       → scan에서 등록됨 (분석 대기)
  Analyzing     → 분석 프롬프트 실행중
  (exit queue)  → autodev:analyzed 라벨 (사람 리뷰 대기)
  Ready         → approved scan에서 등록됨 (구현 대기)
  Implementing  → 구현 프롬프트 실행중 + PR 생성
  (exit queue)  → autodev:implementing 라벨 (PR 리뷰 대기)
  (done)        → PR approve 시 자동 전이
```

v1과의 차이:
| | v1 | v2 |
|---|---|---|
| Analyzing → Ready | 내부 자동 전이 | queue 이탈 → 사람 리뷰 → scanner 재진입 |
| Ready → done | 구현 성공 시 즉시 done | PR 생성 후 queue 이탈 → PR approve 시 done |
| Issue-PR 연결 | 없음 | `PrItem.source_issue_number` |

---

## 4. Scanner 변경

### 기존 scan 구조 (v1)

```
scan_all():
  issues::scan()       — since=cursor, no autodev label → Pending
  pulls::scan()        — since=cursor, no autodev label → Pending
  pulls::scan_merges() — labels=autodev:done, open → merge Pending
```

### 새 scan 구조 (v2)

```
scan_all():
  issues::scan()            — since=cursor, no autodev label → Pending (분석 대기)
  issues::scan_approved()   — labels=autodev:approved-analysis → Ready (구현 대기)  ← NEW
  pulls::scan()             — since=cursor, no autodev label → Pending (리뷰 대기)
  pulls::scan_merges()      — labels=autodev:done, open → merge Pending
```

### issues::scan_approved() 구현

```rust
/// autodev:approved-analysis 라벨이 있는 이슈를 감지하여 Ready 큐에 적재
pub async fn scan_approved(
    gh: &dyn Gh,
    repo_id: &str,
    repo_name: &str,
    repo_url: &str,
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    // 1. autodev:approved-analysis 라벨이 있는 open 이슈 조회
    let params = [
        ("state", "open"),
        ("labels", "autodev:approved-analysis"),
        ("per_page", "30"),
    ];
    let data = gh.api_paginate(repo_name, "issues", &params, gh_host).await?;
    let issues: Vec<serde_json::Value> = serde_json::from_slice(&data)?;

    for issue in &issues {
        if issue.get("pull_request").is_some() { continue; } // PR 제외

        let number = match issue["number"].as_i64() {
            Some(n) if n > 0 => n,
            _ => continue,
        };

        let work_id = make_work_id("issue", repo_name, number);
        if queues.contains(&work_id) { continue; } // dedup

        // 라벨 전이: approved-analysis → implementing
        gh.label_remove(repo_name, number, labels::APPROVED_ANALYSIS, gh_host).await;
        gh.label_add(repo_name, number, labels::IMPLEMENTING, gh_host).await;

        // 이전 분석 리포트를 이슈 코멘트에서 추출 (최신 autodev 분석 코멘트)
        let analysis_report = extract_analysis_from_comments(
            gh, repo_name, number, gh_host
        ).await;

        let item = IssueItem {
            work_id,
            repo_id: repo_id.to_string(),
            repo_name: repo_name.to_string(),
            repo_url: repo_url.to_string(),
            github_number: number,
            title: issue["title"].as_str().unwrap_or("").to_string(),
            body: issue["body"].as_str().map(String::from),
            labels: vec![],
            author: issue["user"]["login"].as_str().unwrap_or("").to_string(),
            analysis_report,
        };

        queues.issues.push(issue_phase::READY, item);
        tracing::info!("approved issue #{number}: → Ready (implementation)");
    }

    Ok(())
}

/// 이슈 코멘트에서 autodev 분석 리포트를 추출
/// `<!-- autodev:analysis -->` 마커가 포함된 최신 코멘트를 찾는다
async fn extract_analysis_from_comments(
    gh: &dyn Gh,
    repo_name: &str,
    number: i64,
    gh_host: Option<&str>,
) -> Option<String> {
    let jq = r#"[.[] | select(.body | contains("<!-- autodev:analysis -->"))] | last | .body"#;
    gh.api_get_field(
        repo_name,
        &format!("issues/{number}/comments"),
        jq,
        gh_host,
    ).await
}
```

### scan_all() 변경

```rust
// scanner/mod.rs
for target in &cfg.consumer.scan_targets {
    match target.as_str() {
        "issues" => {
            // 1. 새 이슈 scan (분석 대기)
            issues::scan(db, gh, ..., queues).await?;

            // 2. 승인된 이슈 scan (구현 대기) — analysis_review가 활성화된 경우만
            if cfg.consumer.analysis_review {
                issues::scan_approved(gh, &repo.id, &repo.name, &repo.url, gh_host, queues).await?;
            }
        }
        "pulls" => { ... }  // 기존 유지
        "merges" => { ... } // 기존 유지
    }
}
```

---

## 5. Pipeline 변경

### Issue process_pending() — 분석 후 queue 이탈

```
v1: analysis OK + implement → Ready에 push (큐 내부 전이)
v2: analysis OK + implement → analyzed 라벨 + 분석 코멘트 게시 + queue에서 제거
```

```rust
// pipeline/issue.rs — process_pending() 핵심 변경

// 기존 v1:
Some(ref a) => {
    // Analyzing → Ready 전이 (queue 내부)
    remove_from_phase(queues, &work_id);
    item.analysis_report = Some(a.report.clone());
    queues.issues.push(issue_phase::READY, item);
}

// v2: analysis_review 모드
Some(ref a) if analysis_review => {
    // 분석 리포트를 이슈 코멘트로 게시
    let comment = verdict::format_analysis_comment(a);
    notifier.post_issue_comment(&item.repo_name, item.github_number, &comment, gh_host).await;

    // autodev:wip → autodev:analyzed (사람 리뷰 대기)
    remove_from_phase(queues, &work_id);
    gh.label_remove(&item.repo_name, item.github_number, labels::WIP, gh_host).await;
    gh.label_add(&item.repo_name, item.github_number, labels::ANALYZED, gh_host).await;
    tracing::info!("issue #{}: Analyzing → analyzed (awaiting human review)", item.github_number);
    let _ = workspace.remove_worktree(&item.repo_name, &task_id).await;
}

// v2: analysis_review 비활성화 시 (v1 호환 동작)
Some(ref a) if !analysis_review => {
    remove_from_phase(queues, &work_id);
    item.analysis_report = Some(a.report.clone());
    queues.issues.push(issue_phase::READY, item);
}
```

### 분석 코멘트 포맷

```rust
// components/verdict.rs

pub fn format_analysis_comment(analysis: &AnalysisResult) -> String {
    format!(
        "<!-- autodev:analysis -->\n\
         ## Autodev Analysis Report\n\n\
         **Verdict**: {} (confidence: {:.0}%)\n\n\
         {}\n\n\
         ---\n\
         > 이 분석을 승인하려면 `autodev:approved-analysis` 라벨을 추가하세요.\n\
         > 수정이 필요하면 코멘트로 피드백을 남기고 `autodev:analyzed` 라벨을 제거하세요.",
        analysis.verdict,
        analysis.confidence * 100.0,
        analysis.report
    )
}
```

### Issue process_ready() — PR 생성 + Issue-PR 연동

```
v1: 구현 성공 → autodev:done (이슈 완료)
v2: 구현 성공 → PR 생성 → PR queue에 push → autodev:implementing (PR 리뷰 대기)
```

```rust
// pipeline/issue.rs — process_ready() 핵심 변경

if res.exit_code == 0 {
    // PR 생성 결과에서 PR 번호 추출
    let pr_number = extract_pr_number_from_output(&res.stdout);

    match pr_number {
        Some(pr_num) => {
            // PR queue에 직접 push (scanner 경유 불필요)
            let pr_work_id = make_work_id("pr", &item.repo_name, pr_num);
            if !queues.contains(&pr_work_id) {
                let pr_item = PrItem {
                    work_id: pr_work_id,
                    repo_id: item.repo_id.clone(),
                    repo_name: item.repo_name.clone(),
                    repo_url: item.repo_url.clone(),
                    github_number: pr_num,
                    title: format!("Implementation for issue #{}", item.github_number),
                    head_branch: format!("autodev/issue-{}", item.github_number),
                    base_branch: "main".to_string(),
                    review_comment: None,
                    source_issue_number: Some(item.github_number),  // Issue-PR 연결
                };
                gh.label_add(&item.repo_name, pr_num, labels::WIP, gh_host).await;
                queues.prs.push(pr_phase::PENDING, pr_item);
                tracing::info!(
                    "issue #{}: PR #{} created → PR queue (review)",
                    item.github_number, pr_num
                );
            }

            // Issue: queue에서 제거 (PR 리뷰가 끝나면 PR pipeline이 done 전이)
            remove_from_phase(queues, &work_id);
            // implementing 라벨은 scan_approved()에서 이미 추가됨
            tracing::info!("issue #{}: Implementing → PR review pending", item.github_number);
        }
        None => {
            // PR 번호 추출 실패 → 기존 v1 동작 (즉시 done)
            remove_from_phase(queues, &work_id);
            if knowledge_extraction {
                let _ = extractor::extract_task_knowledge(...).await;
            }
            gh.label_remove(&item.repo_name, item.github_number, labels::IMPLEMENTING, gh_host).await;
            gh.label_add(&item.repo_name, item.github_number, labels::DONE, gh_host).await;
            tracing::info!("issue #{}: Implementing → done (no PR detected)", item.github_number);
        }
    }
}
```

### PR 번호 추출

```rust
// infrastructure/claude/output.rs

/// Claude 구현 세션 출력에서 PR 번호를 추출
/// PR 생성 시 `gh pr create` 출력을 파싱하거나, `commit-and-pr` 플러그인 결과를 파싱
pub fn extract_pr_number(stdout: &str) -> Option<i64> {
    // 패턴 1: "https://github.com/org/repo/pull/123"
    let re = regex::Regex::new(r"github\.com/[^/]+/[^/]+/pull/(\d+)").ok()?;
    if let Some(cap) = re.captures(stdout) {
        return cap[1].parse().ok();
    }

    // 패턴 2: JSON 결과에서 pr_number 필드
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(stdout) {
        if let Some(n) = v["pr_number"].as_i64() {
            return Some(n);
        }
    }

    None
}
```

### PR pipeline done 시 Issue 연동

```rust
// pipeline/pr.rs — process_pending() 및 process_improved()의 approve 경로

Some(ReviewVerdict::Approve) => {
    // ... 기존 approve 로직 ...

    // Knowledge extraction
    if cfg.consumer.knowledge_extraction { ... }

    // Reviewing → done (PR)
    remove_from_phase(queues, &work_id);
    gh.label_remove(&item.repo_name, pr_num, labels::WIP, gh_host).await;
    gh.label_add(&item.repo_name, pr_num, labels::DONE, gh_host).await;

    // ─── NEW: Issue-PR 연동 ───
    // PR이 이슈에서 생성된 경우, 이슈도 done으로 전이
    if let Some(issue_num) = item.source_issue_number {
        gh.label_remove(&item.repo_name, issue_num, labels::IMPLEMENTING, gh_host).await;
        gh.label_add(&item.repo_name, issue_num, labels::DONE, gh_host).await;
        tracing::info!("issue #{issue_num}: implementing → done (PR #{pr_num} approved)");
    }

    tracing::info!("PR #{pr_num}: Reviewing → done (approved)");
}
```

---

## 6. Model 변경

### 새 라벨 상수

```rust
// queue/task_queues.rs

pub mod labels {
    pub const WIP: &str = "autodev:wip";
    pub const DONE: &str = "autodev:done";
    pub const SKIP: &str = "autodev:skip";

    // v2 추가
    pub const ANALYZED: &str = "autodev:analyzed";
    pub const APPROVED_ANALYSIS: &str = "autodev:approved-analysis";
    pub const IMPLEMENTING: &str = "autodev:implementing";
}
```

### PrItem 확장

```rust
pub struct PrItem {
    pub work_id: String,
    pub repo_id: String,
    pub repo_name: String,
    pub repo_url: String,
    pub github_number: i64,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    pub review_comment: Option<String>,

    // v2 추가: Issue-PR 연결
    /// 이 PR이 어떤 이슈로부터 생성되었는지 (issue pipeline에서 설정)
    pub source_issue_number: Option<i64>,
}
```

### ConsumerConfig 확장

```rust
pub struct ConsumerConfig {
    // ... 기존 필드 ...

    /// 분석 리뷰 게이트 활성화 (default: true)
    /// true: 분석 후 사람 승인 필요 (v2 flow)
    /// false: 분석 → 자동 구현 (v1 호환)
    pub analysis_review: bool,
}
```

---

## 7. Reconciliation 변경

### startup_reconcile() 라벨 필터 확장

v1에서는 `done/skip` 라벨만 skip했으나, v2에서는 더 많은 라벨을 처리해야 한다:

```rust
// daemon/mod.rs — startup_reconcile()

for item in items {
    let labels = get_labels(&item);

    // 영속 완료/제외 → skip
    if has_label(&labels, labels::DONE) || has_label(&labels, labels::SKIP) {
        continue;
    }

    // 분석 완료, 사람 리뷰 대기 → skip (사람의 action 대기)
    if has_label(&labels, labels::ANALYZED) {
        continue;
    }

    // 사람이 분석 승인 → Ready 큐에 적재
    if has_label(&labels, labels::APPROVED_ANALYSIS) {
        gh.label_remove(repo, number, labels::APPROVED_ANALYSIS, gh_host).await;
        gh.label_add(repo, number, labels::IMPLEMENTING, gh_host).await;
        let item = build_issue_item(..., extract_analysis_from_comments(...).await);
        queues.issues.push(issue_phase::READY, item);
        recovered += 1;
        continue;
    }

    // 구현중 (PR 리뷰 대기) → skip (PR pipeline이 처리)
    if has_label(&labels, labels::IMPLEMENTING) {
        continue;
    }

    // orphan wip → 정리 후 적재
    if has_label(&labels, labels::WIP) {
        gh.label_remove(repo, number, labels::WIP, gh_host).await;
    }

    // no autodev label 또는 정리된 wip → Pending 적재
    gh.label_add(repo, number, labels::WIP, gh_host).await;
    queues.issues.push(issue_phase::PENDING, item);
    recovered += 1;
}
```

### recovery() 변경

v2에서는 `autodev:wip` 외에 `autodev:implementing` 상태의 이슈도 체크:

```
recovery() 추가 로직:
  autodev:implementing + 연결된 PR이 이미 merged/closed → implementing → done
```

이 로직은 PR approve 시점에 크래시가 발생했을 때를 커버한다.

---

## 8. Configuration 변경

```yaml
# .develop-workflow.yaml

consumer:
  # ... 기존 필드 ...
  analysis_review: true    # v2: 분석 리뷰 게이트 (default: true)
```

| 설정 | 값 | 동작 |
|------|----|------|
| `analysis_review: true` | 기본값 | v2 flow: 분석 → 사람 승인 → 구현 |
| `analysis_review: false` | opt-out | v1 flow: 분석 → 자동 구현 |

---

## 9. 사이드이펙트 & 영향 범위

### 코드 변경

| 파일 | 변경 내용 | 위험도 |
|------|----------|--------|
| `queue/task_queues.rs` | `labels` 모듈에 상수 3개 추가, `PrItem.source_issue_number` 추가 | 낮음 (additive) |
| `config/models.rs` | `ConsumerConfig.analysis_review` 필드 추가 | 낮음 (serde default) |
| `scanner/issues.rs` | `scan_approved()` 함수 추가 | 낮음 (new function) |
| `scanner/mod.rs` | `scan_all()`에 `scan_approved()` 호출 추가 | 낮음 |
| `pipeline/issue.rs` | `process_pending()` 분기 추가, `process_ready()` PR 연동 로직 | **중간** |
| `pipeline/pr.rs` | approve 경로에 Issue done 전이 추가 | 낮음 |
| `components/verdict.rs` | `format_analysis_comment()` 함수 추가 | 낮음 (new function) |
| `infrastructure/claude/output.rs` | `extract_pr_number()` 함수 추가 | 낮음 (new function) |
| `daemon/mod.rs` | `startup_reconcile()` 라벨 필터 확장 | **중간** |

### 기존 테스트 영향

| 테스트 파일 | 영향 | 대응 |
|------------|------|------|
| `pipeline_e2e_tests.rs` | `analysis_review=false` 시 v1 동작 유지 필요 | 기존 테스트에 config mock 추가 |
| `daemon_consumer_tests.rs` | reconcile 라벨 필터 변경 | 새 라벨 케이스 추가 |
| `task_queues.rs` (unit) | `PrItem` 필드 추가 | 테스트 헬퍼에 `source_issue_number: None` 추가 |

### 하위 호환성

- `analysis_review` default가 `true`이므로, **기존 사용자는 자동으로 v2 flow로 전환**됨
- v1 동작을 원하면 `analysis_review: false` 설정
- 기존 `autodev:wip/done/skip` 라벨은 그대로 유지
- 새 라벨(`analyzed`, `approved-analysis`, `implementing`)은 GitHub에 자동 생성됨
  (label_add API가 존재하지 않는 라벨을 자동 생성)

---

## 10. End-to-End Flow (v2)

```
┌──────────────────────────────────────────────────────────────────────┐
│                        DAEMON LOOP (v2)                              │
│                                                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 1. RECOVERY                                                   │  │
│  │    autodev:wip + queue에 없음 → wip 라벨 제거                  │  │
│  │    autodev:implementing + PR merged → implementing → done      │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 2. SCAN                                                       │  │
│  │    2a. issues::scan()         — 새 이슈 → Pending (분석)       │  │
│  │    2b. issues::scan_approved()— approved → Ready (구현)  ←NEW │  │
│  │    2c. pulls::scan()          — 새 PR → Pending (리뷰)        │  │
│  │    2d. pulls::scan_merges()   — approved PR → merge Pending   │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 3. CONSUME                                                    │  │
│  │                                                               │  │
│  │  Issues:                                                      │  │
│  │    Pending → Analyzing:                                       │  │
│  │      analysis_review=true:                                    │  │
│  │        OK → 분석 코멘트 + autodev:analyzed + exit queue       │  │
│  │      analysis_review=false:                                   │  │
│  │        OK → Ready에 push (v1 호환)                            │  │
│  │      clarify/wontfix → autodev:skip                          │  │
│  │                                                               │  │
│  │    Ready → Implementing:                                      │  │
│  │      OK + PR 생성 → PR queue push + autodev:implementing     │  │
│  │      OK + PR 없음 → autodev:done (v1 호환)                   │  │
│  │      Err → 라벨 제거 + 재시도                                 │  │
│  │                                                               │  │
│  │  PRs (리뷰):                                                  │  │
│  │    Reviewing → approve → autodev:done (PR)                   │  │
│  │                         + source_issue → done    ← NEW        │  │
│  │    Reviewing → request_changes → ReviewDone → Improving      │  │
│  │                                    → Improved → re-review     │  │
│  │                                                               │  │
│  │  Merges: (기존 유지)                                          │  │
│  │    Pending → Merging → done | Conflict → 재시도               │  │
│  │                                                               │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                            │                                        │
│                      sleep(tick)                                     │
│                            │                                        │
│                            └──→ loop                                │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 11. Status Transitions (v2)

| Type | Phase Flow | 라벨 전이 |
|------|-----------|----------|
| Issue (v2, review=true) | `Pending → Analyzing → (exit)` | `(없음) → wip → analyzed` |
| Issue (human approve) | `(scan_approved) → Ready → Implementing → (exit)` | `approved-analysis → implementing` |
| Issue (PR approved) | `(PR pipeline triggers)` | `implementing → done` |
| Issue (v2, review=false) | `Pending → Analyzing → Ready → Implementing → done` | `(없음) → wip → implementing → done` |
| Issue (clarify/wontfix) | `Pending → Analyzing → skip` | `(없음) → wip → skip` |
| Issue (analysis reject) | `analyzed → (없음) → re-scan` | `analyzed → (없음) → wip → analyzed` |
| PR (리뷰) | `Pending → Reviewing → approve → done` | `(없음) → wip → done` |
| PR (리뷰 + 피드백) | `Pending → Reviewing → ReviewDone → Improving → Improved → Reviewing (반복)` | `wip` 유지 |
| Merge | `Pending → Merging → done` | `(없음) → wip → done` |

---

## 12. 구현 우선순위

### Phase A: 라벨 + 모델 (기반)

1. `labels` 모듈에 `ANALYZED`, `APPROVED_ANALYSIS`, `IMPLEMENTING` 상수 추가
2. `PrItem`에 `source_issue_number: Option<i64>` 필드 추가
3. `ConsumerConfig`에 `analysis_review: bool` 필드 추가 (default: true)
4. 기존 테스트 수정 (PrItem 생성자에 새 필드 추가)

### Phase B: 분석 리뷰 게이트

5. `verdict.rs`에 `format_analysis_comment()` 추가
6. `pipeline/issue.rs` `process_pending()` 변경 — `analysis_review` 분기
7. 테스트: analysis_review=true 시 analyzed 라벨 + exit queue 검증
8. 테스트: analysis_review=false 시 v1 호환 동작 검증

### Phase C: Approved Scan + 구현

9. `scanner/issues.rs`에 `scan_approved()` 추가
10. `scanner/mod.rs`에 `scan_approved()` 호출 추가
11. `output.rs`에 `extract_pr_number()` 추가
12. `pipeline/issue.rs` `process_ready()` 변경 — PR 생성 + PR queue push
13. 테스트: scan_approved → Ready 큐 적재 검증
14. 테스트: process_ready → PR 생성 + PR queue push 검증

### Phase D: Issue-PR 연동

15. `pipeline/pr.rs` approve 경로에 source_issue done 전이 추가
16. `daemon/mod.rs` reconcile 라벨 필터 확장
17. 테스트: PR approve 시 source_issue done 전이 검증
18. 테스트: reconcile에서 approved-analysis → Ready 적재 검증

---

## 13. v1 → v2 전환 체크리스트

- [ ] 새 라벨 상수 추가 (additive, 기존 코드 영향 없음)
- [ ] PrItem.source_issue_number 추가 (Option, 기존 코드 영향 없음)
- [ ] analysis_review config 추가 (serde default, 기존 설정 파일 호환)
- [ ] process_pending() 분기 (analysis_review flag로 v1/v2 전환)
- [ ] scan_approved() 추가 (새 함수, 기존 코드 영향 없음)
- [ ] process_ready() PR 연동 (PR 번호 추출 실패 시 v1 fallback)
- [ ] PR approve 시 Issue done (source_issue_number가 None이면 기존 동작)
- [ ] reconcile 확장 (새 라벨 추가 처리, 기존 라벨 동작 유지)
- [ ] 기존 테스트 통과 확인 (analysis_review=false로 v1 동작 검증)

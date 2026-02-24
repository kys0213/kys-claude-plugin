# DESIGN-v2 Gap 개선 계획

> **Date**: 2026-02-24
> **Base**: `kanban/done/design-v2-review.md` — 9개 gap 식별
> **목표**: 설계(DESIGN-v2.md)와 구현의 불일치를 해소

---

## 사이드이펙트 조사 결과

### 인프라 현황 (구현에 필요한 trait 메서드)

| 필요 메서드 | 존재 여부 | 위치 |
|------------|----------|------|
| `Gh::create_pr()` → `Option<i64>` | **있음** | `infrastructure/gh/mod.rs` |
| `Git::checkout_new_branch()` | **있음** | `infrastructure/git/mod.rs` |
| `Git::add_commit_push()` | **있음** | `infrastructure/git/mod.rs` |
| `SuggestionType::Skill/Subagent` | **있음** | `knowledge/models.rs` |
| `Gh::api_paginate()` (PR 조회) | **있음** | `infrastructure/gh/mod.rs` |

→ 새로운 trait 메서드 추가 없이 기존 인프라로 모든 gap 해결 가능.

### 테스트 영향

| 테스트 파일 | 영향 | 이유 |
|------------|------|------|
| `pipeline_e2e_tests.rs` | **수정 필요** | `process_ready()` 라벨 전이 기대값 변경 |
| `knowledge_pr_tests.rs` | **새 테스트** | per-task actionable PR 검증 |
| `daemon_recovery_tests.rs` | **새 테스트** | implementing + merged PR recovery 검증 |
| 기타 기존 테스트 | **영향 없음** | Phase A/B 관련 테스트는 변경 없음 |

---

## 개선 Phase 설계

### 의존성 다이어그램

```
Phase 1 (process_ready 수정)
    │
    ├─→ Phase 2 (recovery 확장) — Phase 1에 의존
    │       implementing 라벨이 유지되어야 recovery가 의미 있음
    │
    └─→ Phase 3 (extract_pr_number 보강) — 독립적, Phase 1과 병렬 가능

Phase 4 (knowledge 수집 확장) — 독립
    │
    └─→ Phase 5 (per-task actionable PR + daily 연동) — Phase 4에 의존
```

---

### Phase 1: `process_ready()` 라벨 전이 수정 — High

**목적**: PR 생성 시 Issue를 done이 아닌 implementing 상태로 유지

#### 1-1. `pipeline/issue.rs` — `process_ready()` PR 생성 성공 경로

**현재** (line 432-451):
```rust
// Issue: Implementing → done (PR pipeline이 이어서 처리)
remove_from_phase(queues, &work_id);
gh.label_remove(..., labels::IMPLEMENTING, ...).await;
gh.label_add(..., labels::DONE, ...).await;
```

**변경**:
```rust
// Issue: queue에서 제거 (implementing 라벨 유지 — PR pipeline이 done 전이)
remove_from_phase(queues, &work_id);
// implementing 라벨은 scan_approved()에서 이미 추가됨, 유지
```

- `label_remove(IMPLEMENTING)` 제거
- `label_add(DONE)` 제거
- 로그 메시지 변경: `"issue #{}: exit queue (PR #{pr_num} review pending)"`

#### 1-2. `pipeline/issue.rs` — `process_ready()` PR 번호 추출 실패 경로

**현재** (line 453-474):
```rust
remove_from_phase(queues, &work_id);
gh.label_remove(..., labels::IMPLEMENTING, ...).await;
gh.label_add(..., labels::DONE, ...).await;
```

**변경**:
```rust
remove_from_phase(queues, &work_id);
gh.label_remove(..., labels::IMPLEMENTING, ...).await;
// done 라벨 추가하지 않음 — 다음 scan에서 재발견
```

- `label_add(DONE)` 제거
- 로그 메시지 변경: `"issue #{}: PR number extraction failed, implementing removed (will retry)"`

#### 1-3. 기존 테스트 수정

`pipeline_e2e_tests.rs`에서 `process_ready()` 관련 테스트의 기대값 수정:
- PR 생성 성공: `DONE` 라벨 추가 assert 제거, `IMPLEMENTING` 라벨 제거 assert 제거
- PR 번호 추출 실패: `DONE` 라벨 추가 assert 제거

#### 1-4. 새 테스트 추가

- `process_ready_pr_created_keeps_implementing_label`: PR 생성 성공 시 implementing 라벨 유지 + done 미추가
- `process_ready_no_pr_number_removes_implementing`: PR 번호 추출 실패 시 implementing만 제거, done 미추가
- `process_ready_pr_created_pushes_to_pr_queue_with_source_issue`: PR queue에 source_issue_number 설정 확인

**검증**: `cargo test` — pipeline 테스트 전부 통과

---

### Phase 2: Recovery 확장 — Medium→High (Phase 1 이후)

**목적**: implementing + 연결 PR merged/closed → Issue done 전이 (크래시 복구)

Phase 1에서 Issue가 implementing 상태로 남게 되므로, 크래시 시 복구 로직이 필요.

#### 2-1. `daemon/recovery.rs` — `recover_orphan_implementing()` 추가

```rust
/// autodev:implementing 라벨이 있지만 연결된 PR이 이미 merged/closed인 Issue를 done으로 전이
pub async fn recover_orphan_implementing(
    repos: &[EnabledRepo],
    gh: &dyn Gh,
    queues: &TaskQueues,
    gh_host: Option<&str>,
) -> Result<u64> {
    // 1. autodev:implementing 라벨이 있는 open 이슈 조회
    // 2. 각 이슈의 timeline/events에서 연결된 PR 번호 추출
    //    (issue body에 "Closes #N" → PR 본문, 또는 linked PRs API)
    //    대안: 이슈 코멘트에서 "PR #{N} created" 패턴 검색 (daemon 로그에서 기록)
    // 3. 연결 PR의 state 확인 (merged/closed)
    // 4. merged/closed이면 implementing → done 전이
}
```

**단순화 접근**: PR의 `Closes #N` 본문보다, 이슈 코멘트에서 autodev가 남긴 로그(`PR #{N} created`)를 파싱하는 것이 더 신뢰성 있음. 하지만 현재 `process_ready()`가 이슈 코멘트를 남기지 않으므로, Phase 1에서 PR 생성 시 이슈 코멘트를 추가하는 것도 고려.

**대안**: `startup_reconcile()`의 implementing 처리를 강화하는 방향. 현재는 skip인데, PR이 이미 완료되었는지 확인 후 done 전이.

#### 2-2. `daemon/mod.rs` — 메인 루프에 호출 추가

```rust
// 1. Recovery
recover_orphan_wip(...).await;
recover_orphan_implementing(...).await;  // NEW
```

#### 2-3. 테스트

- `recover_orphan_implementing_merged_pr`: implementing + linked PR merged → done 전이
- `recover_orphan_implementing_open_pr`: implementing + linked PR still open → skip

**검증**: `cargo test` — daemon 테스트 통과

---

### Phase 3: `extract_pr_number()` JSON fallback — Low

**목적**: `{"pr_number": 123}` JSON 패턴도 파싱

#### 3-1. `infrastructure/claude/output.rs` — `extract_pr_number()` 확장

현재 코드 이후에 JSON fallback 추가:

```rust
// 기존 /pull/ 패턴 검색 후...

// Pattern 2: JSON에서 pr_number 필드
if let Ok(v) = serde_json::from_str::<serde_json::Value>(&search_text) {
    if let Some(n) = v["pr_number"].as_i64() {
        if n > 0 {
            return Some(n);
        }
    }
}
```

#### 3-2. 테스트

- `extract_pr_number_from_json_field`: `{"pr_number": 42}` → Some(42)
- `extract_pr_number_from_envelope_json_field`: envelope 안 JSON → Some(42)

**검증**: `cargo test` — output 테스트 통과

---

### Phase 4: Knowledge 수집 범위 확장 — Medium

**목적**: `collect_existing_knowledge()`가 설계의 전체 지식 베이스를 수집

#### 4-1. `knowledge/extractor.rs` — `collect_existing_knowledge()` 확장

현재: CLAUDE.md, .claude/rules/*.md
추가:
- `plugins/*/commands/*.md` (skill 파일명 + 간략 내용)
- `.claude/hooks.json` (있으면 내용 포함)
- `.develop-workflow.yaml` (있으면 내용 포함)

```rust
// 기존 코드 이후에 추가

// plugins/*/commands/*.md (skill 목록)
let plugins_dir = wt_path.join("plugins");
if plugins_dir.is_dir() {
    knowledge.push_str("--- Existing Skills ---\n");
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for plugin_entry in entries.flatten() {
            let cmds_dir = plugin_entry.path().join("commands");
            if cmds_dir.is_dir() {
                if let Ok(cmd_entries) = std::fs::read_dir(&cmds_dir) {
                    for cmd in cmd_entries.flatten() {
                        if cmd.path().extension().is_some_and(|e| e == "md") {
                            let rel = cmd.path().strip_prefix(wt_path)
                                .unwrap_or(&cmd.path()).display().to_string();
                            knowledge.push_str(&format!("- {rel}\n"));
                        }
                    }
                }
            }
        }
    }
    knowledge.push('\n');
}

// .claude/hooks.json
let hooks = wt_path.join(".claude/hooks.json");
if hooks.exists() {
    if let Ok(content) = std::fs::read_to_string(&hooks) {
        knowledge.push_str("--- .claude/hooks.json ---\n");
        knowledge.push_str(&content);
        knowledge.push_str("\n\n");
    }
}

// .develop-workflow.yaml
let dw = wt_path.join(".develop-workflow.yaml");
if dw.exists() {
    if let Ok(content) = std::fs::read_to_string(&dw) {
        knowledge.push_str("--- .develop-workflow.yaml ---\n");
        knowledge.push_str(&content);
        knowledge.push_str("\n\n");
    }
}
```

#### 4-2. `extract_task_knowledge()` — empty suggestions 반환값 수정

**현재**:
```rust
if let Some(ref ks) = suggestion {
    if !ks.suggestions.is_empty() {
        // 코멘트 게시
    }
}
Ok(suggestion)  // 빈 suggestions도 Some으로 반환
```

**변경**:
```rust
let suggestion = match suggestion {
    Some(ref ks) if ks.suggestions.is_empty() => {
        tracing::debug!("{task_type} #{github_number}: no new knowledge (delta check passed)");
        return Ok(None);
    }
    other => other,
};

if let Some(ref ks) = suggestion {
    // 코멘트 게시
}
Ok(suggestion)
```

#### 4-3. 테스트

- `collect_existing_knowledge_reads_skills_and_hooks`: tmpdir에 plugins/*/commands/*.md + hooks.json 배치 → 수집 검증
- `extract_task_knowledge_empty_suggestions_returns_none`: 빈 suggestions → `Ok(None)` 반환 검증

**검증**: `cargo test` — knowledge 테스트 통과

---

### Phase 5: Per-task Actionable PR + Daily 연동 — Medium

Phase 4 완료 후 진행.

#### 5-1. `knowledge/extractor.rs` — `create_knowledge_pr()` 추가

설계 Section 8의 per-task actionable PR 생성 로직:

```rust
/// Per-task actionable knowledge suggestion으로 PR 생성
///
/// skill/subagent type의 suggestion이 있으면 코멘트 외에 실제 PR을 생성한다.
async fn create_knowledge_pr(
    gh: &dyn Gh,
    git: &dyn Git,
    repo_name: &str,
    suggestions: &[&Suggestion],
    source_number: i64,
    wt_path: &Path,
    gh_host: Option<&str>,
) {
    let branch = format!("autodev/knowledge-{source_number}");

    // 1. 브랜치 생성
    if let Err(e) = git.checkout_new_branch(wt_path, &branch).await {
        tracing::warn!("knowledge PR branch creation failed: {e}");
        return;
    }

    // 2. 파일 쓰기
    let mut files = Vec::new();
    for s in suggestions {
        let file_path = wt_path.join(&s.target_file);
        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&file_path, &s.content).is_ok() {
            files.push(s.target_file.as_str());
        }
    }

    if files.is_empty() { return; }

    // 3. commit + push
    let message = format!("feat(autodev): add knowledge from #{source_number}");
    if let Err(e) = git.add_commit_push(wt_path, &files, &message, &branch).await {
        tracing::warn!("knowledge PR commit+push failed: {e}");
        return;
    }

    // 4. PR 생성 (autodev:skip 라벨)
    let body = format_knowledge_pr_body(suggestions, source_number);
    if let Some(pr_num) = gh.create_pr(repo_name, &branch, "main",
        &format!("feat(autodev): knowledge from #{source_number}"), &body, gh_host
    ).await {
        gh.label_add(repo_name, pr_num, labels::SKIP, gh_host).await;
        tracing::info!("knowledge PR #{pr_num} created from #{source_number}");
    }
}
```

#### 5-2. `knowledge/extractor.rs` — `extract_task_knowledge()` 에서 호출

```rust
// 코멘트 게시 후
if let Some(ref ks) = suggestion {
    // actionable suggestions 필터
    let actionable: Vec<&Suggestion> = ks.suggestions.iter()
        .filter(|s| matches!(s.suggestion_type,
            SuggestionType::Skill | SuggestionType::Subagent))
        .collect();

    if !actionable.is_empty() {
        create_knowledge_pr(gh, git, repo_name, &actionable,
            github_number, wt_path, gh_host).await;
    }
}
```

**주의**: `extract_task_knowledge()` 시그니처에 `git: &dyn Git` 파라미터 추가 필요.
→ 호출부(`pipeline/pr.rs` 2곳)도 수정 필요.

#### 5-3. `detect_cross_task_patterns()` daily flow 연결

`daemon/mod.rs` daily report 생성 로직에서:

```rust
// 기존 patterns 후에 추가
let cross_patterns = crate::knowledge::daily::detect_cross_task_patterns(
    &report.suggestions
);
report.patterns.extend(cross_patterns);
```

#### 5-4. 테스트

- `create_knowledge_pr_creates_branch_and_pr`: mock Git/Gh → 브랜치 생성 + PR 생성 검증
- `extract_task_knowledge_calls_create_knowledge_pr_for_skill`: skill suggestion → PR 생성 호출 검증
- `detect_cross_task_patterns_integrated_in_daily`: daily flow에서 패턴 감지 호출 검증

**검증**: `cargo test` + `cargo clippy`

---

## Phase 간 의존성 & 실행 순서

```
Phase 1 (process_ready 수정) ──── 가장 먼저, 핵심 설계 수정
    │
    ├── Phase 2 (recovery 확장) ── Phase 1 이후 (implementing 유지가 전제)
    │
    └── Phase 3 (extract_pr_number) ── Phase 1과 병렬 가능

Phase 4 (knowledge 수집 확장) ── 독립 (Phase 1과 병렬 가능)
    │
    └── Phase 5 (actionable PR + daily 연동) ── Phase 4 이후

실행 순서: 1 → (2, 3, 4 병렬) → 5
```

---

## 각 Phase별 검증 기준

| Phase | 검증 | 통과 조건 |
|-------|------|----------|
| 1 | `cargo test` | process_ready 관련 테스트: implementing 유지 + done 미추가 |
| 2 | `cargo test` | implementing + merged PR → done recovery 검증 |
| 3 | `cargo test` | JSON pr_number 파싱 + 기존 URL 파싱 유지 |
| 4 | `cargo test` | 확장된 knowledge 수집 + empty suggestions → None |
| 5 | `cargo test` + `cargo clippy` | actionable PR 생성 + daily 패턴 연동 |
| 최종 | `cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test` | Quality Gate 전부 통과 |

---

## 위험 요소

| 위험 | 영향 | 대응 |
|------|------|------|
| Phase 1: `process_ready()` 변경 시 기존 E2E 테스트 실패 | **높음** | 기대값 수정 필수, 테스트부터 변경 |
| Phase 2: implementing 이슈의 연결 PR 번호 추출 | **중간** | `process_ready()`에서 PR 생성 시 이슈 코멘트 남기는 방안 추가 검토 |
| Phase 5: `extract_task_knowledge()` 시그니처 변경 | **중간** | 호출부 2곳(pr.rs process_pending, process_improved) cascading 수정 |
| Phase 5: `detect_cross_task_patterns()`가 `report.suggestions` 기반 | **낮음** | daily report 생성 시점에 suggestions가 이미 채워져 있어야 함 → 순서 확인 |

---

## 구현 체크리스트 (13항목)

- [ ] 1-1. `process_ready()` PR 생성 성공 시 implementing 유지 (done 제거)
- [ ] 1-2. `process_ready()` PR 번호 추출 실패 시 done 제거
- [ ] 1-3. 기존 E2E 테스트 기대값 수정
- [ ] 1-4. 새 테스트: process_ready 라벨 전이 검증
- [ ] 2-1. `recover_orphan_implementing()` 추가
- [ ] 2-2. daemon 메인 루프에 호출 추가
- [ ] 2-3. recovery 테스트 추가
- [ ] 3-1. `extract_pr_number()` JSON pr_number fallback 추가
- [ ] 3-2. extract_pr_number 테스트 추가
- [ ] 4-1. `collect_existing_knowledge()` 수집 범위 확장
- [ ] 4-2. `extract_task_knowledge()` empty suggestions → Ok(None)
- [ ] 4-3. knowledge 수집 테스트 추가
- [ ] 5-1. `create_knowledge_pr()` per-task actionable PR
- [ ] 5-2. `extract_task_knowledge()` 시그니처 확장 + 호출부 수정
- [ ] 5-3. `detect_cross_task_patterns()` daily flow 연결
- [ ] 5-4. actionable PR + daily 연동 테스트

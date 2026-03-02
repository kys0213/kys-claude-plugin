# Autodev Gap 개선 계획 (**구현 완료**)

> **Date**: 2026-02-22
> **Scope**: DESIGN-GAP-REPORT.md에서 식별된 H-01, H-02, H-03, M-02 해소
> **Status**: 전체 구현 완료 (2026-02-23 검증). kanban → [done/gap-improvement.md](../plugins/autodev/kanban/done/gap-improvement.md)

---

## 1. 요구사항 정리

### 변경 1: Config 구조 정렬 (H-01 + H-02)

`DaemonConfig` 구조체를 추가하여 daemon 전용 설정을 분리한다.

**현재**: `ConsumerConfig`에 모든 설정이 혼재, `tick_interval_secs`와 `reconcile_window_hours` 하드코딩
**목표**: `WorkflowConfig.daemon` 섹션 추가, YAML에서 설정 가능

```yaml
# Before (.develop-workflow.yaml)
consumer:
  daily_report_hour: 6
  # tick_interval_secs → 하드코딩 10
  # reconcile_window_hours → 하드코딩 24

# After
consumer:
  scan_interval_secs: 300
  # ... (기존 유지)
daemon:
  tick_interval_secs: 10
  reconcile_window_hours: 24
  daily_report_hour: 6
```

**변경 파일**:
- `config/models.rs` — `DaemonConfig` 추가, `WorkflowConfig.daemon` 필드 추가, `daily_report_hour` ConsumerConfig에서 제거
- `daemon/mod.rs` — 하드코딩 제거 → config에서 읽기

### 변경 2: PR 리뷰 verdict 파싱 (H-03)

리뷰 결과를 JSON으로 파싱하여 approve/request_changes를 결정적으로 분기한다.

**현재**: `exit_code == 0`이면 무조건 ReviewDone (피드백 루프 진입)
**목표**: JSON verdict 파싱 → approve면 즉시 done, request_changes면 피드백 루프

```
Before: Pending → exit_code==0 → ReviewDone → ... → 재리뷰 → done
After:  Pending → verdict:approve → done (바로 완료)
        Pending → verdict:request_changes → ReviewDone → ... → 재리뷰 → done
```

**변경 파일**:
- `infrastructure/claude/output.rs` — `ReviewResult` 구조체 + `parse_review()` 추가
- `components/reviewer.rs` — `ReviewOutput`에 `verdict` 필드 추가
- `pipeline/pr.rs` — `process_pending()`과 `process_improved()`에서 verdict 분기

### 변경 3: Merge scan 구현 (M-02)

approved PR을 자동 감지하여 merge queue에 적재한다.

**현재**: merge pipeline은 존재하지만 merge queue에 아이템을 넣는 경로가 없음
**목표**: PR scan 시 approved + autodev:done PR을 merge queue에 적재

```
PR done → autodev:done 라벨 부착
  ↓
다음 scan cycle:
  pulls scan → autodev:done + state:open → merge queue push
```

**변경 파일**:
- `scanner/pulls.rs` — `scan_merges()` 함수 추가
- `scanner/mod.rs` — `scan_all()`에서 `scan_merges()` 호출
- `config/models.rs` — `ConsumerConfig.auto_merge` 필드 추가

---

## 2. 사이드이펙트 조사

### 변경 1: Config 구조 정렬

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| `daemon/mod.rs:54` | `cfg.consumer.daily_report_hour` 참조 → 깨짐 | `cfg.daemon.daily_report_hour`로 변경 |
| `daemon/mod.rs:55` | `cfg.consumer.knowledge_extraction` 참조 | 유지 (knowledge_extraction은 consumer 소속) |
| `daemon/mod.rs:58` | `reconcile_window_hours = 24u32` 하드코딩 | `cfg.daemon.reconcile_window_hours` |
| `daemon/mod.rs:148` | `Duration::from_secs(10)` 하드코딩 | `cfg.daemon.tick_interval_secs` |
| YAML backward compat | `daemon:` 섹션 없는 기존 설정 | `serde(default)` + `Default impl`으로 해결 |
| `WorkflowConfig` | `deny_unknown_fields` 때문에 `daemon` 추가 시 에러 가능 | ✅ `WorkflowConfig`만 `deny_unknown_fields`, 추가하므로 OK |
| 기존 테스트 | `config_loader_tests.rs`에서 daily_report_hour 참조 여부 | 확인 필요 |

### 변경 2: PR verdict 파싱

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| `ReviewOutput.review` | String → 구조화 필요 | `verdict` 필드 추가 (기존 `review` 유지) |
| `process_pending()` | exit_code 분기 로직 변경 | verdict 파싱 실패 시 fallback: exit_code 기반 (기존 동작 유지) |
| `process_improved()` | 동일하게 verdict 파싱 필요 | 같은 패턴 적용 |
| 리뷰 댓글 포맷 | `format_review_comment()`에 verdict 표시 | approve/request_changes 표시 추가 |
| 기존 테스트 | `pipeline_e2e_tests.rs` 등 | MockClaude가 ReviewResult JSON을 반환하도록 수정 |

### 변경 3: Merge scan

| 영향 대상 | 사이드이펙트 | 대응 |
|-----------|-------------|------|
| scan 순서 | PR review scan 후 merge scan 실행 | review scan 결과로 autodev:done 라벨 붙은 PR이 merge scan 대상 |
| work_id 충돌 | `pr:org/repo:15` (review) vs `merge:org/repo:15` (merge) | prefix가 다르므로 충돌 없음 |
| auto_merge 미설정 시 | merge scan 실행 안 함 | `ConsumerConfig.auto_merge` 기본값 `false` |
| approved 판별 | GitHub PR reviews API 조회 필요 | `Gh.api_get_field()` + jq로 APPROVED count 확인 |
| scan_targets | `"merges"` 타겟 추가 필요 | `scan_targets`에 `"merges"` 추가 시에만 동작 |

---

## 3. 구현 설계

### Phase A: Config 구조 정렬

```rust
// config/models.rs

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WorkflowConfig {
    pub consumer: ConsumerConfig,
    pub daemon: DaemonConfig,      // 추가
    pub workflow: WorkflowRouting,
    pub commands: CommandsConfig,
    pub develop: DevelopConfig,
}

/// 데몬 루프 전용 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub tick_interval_secs: u64,        // default: 10
    pub reconcile_window_hours: u32,    // default: 24
    pub daily_report_hour: u32,         // default: 6
}

/// ConsumerConfig에서 daily_report_hour 제거
pub struct ConsumerConfig {
    // ... 기존 필드 유지
    // daily_report_hour: u32,  ← 제거
}
```

```rust
// daemon/mod.rs — 하드코딩 제거
let tick_interval = cfg.daemon.tick_interval_secs;
let reconcile_window_hours = cfg.daemon.reconcile_window_hours;
let daily_report_hour = cfg.daemon.daily_report_hour;
```

### Phase B: PR verdict 파싱

```rust
// infrastructure/claude/output.rs

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approve,
    RequestChanges,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewResult {
    pub verdict: ReviewVerdict,
    pub summary: String,
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewComment {
    pub path: String,
    pub line: Option<u32>,
    pub body: String,
}

/// 리뷰 JSON 파싱 (envelope → inner 패턴)
pub fn parse_review(stdout: &str) -> Option<ReviewResult> {
    // 1차: ClaudeJsonOutput envelope → result 필드 → ReviewResult
    // 2차: 직접 파싱
    // 실패 시 None (호출측에서 exit_code 기반 fallback)
}
```

```rust
// components/reviewer.rs

pub struct ReviewOutput {
    pub review: String,
    pub verdict: Option<ReviewVerdict>,  // 추가
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl Reviewer {
    pub async fn review_pr(...) -> Result<ReviewOutput> {
        let result = self.claude.run_session(wt_path, prompt, Some("json")).await?;
        let (review, verdict) = if result.exit_code == 0 {
            match output::parse_review(&result.stdout) {
                Some(r) => (r.summary.clone(), Some(r.verdict)),
                None => (output::parse_output(&result.stdout), None),
            }
        } else {
            (String::new(), None)
        };
        Ok(ReviewOutput { review, verdict, stdout: result.stdout, stderr: result.stderr, exit_code: result.exit_code })
    }
}
```

```rust
// pipeline/pr.rs — process_pending() 핵심 변경

if output.exit_code == 0 {
    match output.verdict {
        Some(ReviewVerdict::Approve) => {
            // Knowledge extraction + done 즉시 전이
            // wip → done
        }
        Some(ReviewVerdict::RequestChanges) | None => {
            // GitHub 댓글 게시
            // ReviewDone에 push (피드백 루프 진입)
        }
    }
}
```

### Phase C: Merge scan

```rust
// config/models.rs — ConsumerConfig에 auto_merge 추가
pub struct ConsumerConfig {
    // ... 기존
    pub auto_merge: bool,  // default: false
}
```

```rust
// scanner/pulls.rs — scan_merges() 추가

/// approved + autodev:done + open 상태인 PR을 merge queue에 적재
pub async fn scan_merges(
    gh: &dyn Gh,
    repo_name: &str,
    repo_url: &str,
    repo_id: &str,
    gh_host: Option<&str>,
    queues: &mut TaskQueues,
) -> Result<()> {
    // 1. autodev:done 라벨이 있는 open PR 조회
    //    gh api repos/{repo}/issues?state=open&labels=autodev:done
    //    (issues endpoint는 PR도 포함, pull_request 필드로 구분)
    //
    // 2. 각 PR에 대해:
    //    - merge queue에 이미 있으면 skip
    //    - merged 상태면 skip
    //    - wip 라벨 추가 + done 라벨 제거
    //    - merge queue push
}
```

```rust
// scanner/mod.rs — scan_all()에 merge scan 추가
"merges" => {
    if cfg.consumer.auto_merge {
        pulls::scan_merges(gh, &repo.name, &repo.url, &repo.id, gh_host, queues).await?;
    }
}
```

---

## 4. 구현 순서 (의존성)

```
Phase A (Config) ← 독립, 먼저
    ↓
Phase B (PR verdict) ← A 불필요하지만 A 후 진행이 깔끔
    ↓
Phase C (Merge scan) ← A 필요 (auto_merge 설정), B 불필요
```

## 5. 테스트 계획

| Phase | 테스트 |
|-------|--------|
| A | `DaemonConfig` default 검증, YAML 파싱 (daemon 섹션), backward compat (daemon 없는 YAML) |
| B | `parse_review()` approve/request_changes/malformed 케이스, pipeline flow 검증 |
| C | `scan_merges()` approved PR 감지, dedup, auto_merge=false 시 skip |

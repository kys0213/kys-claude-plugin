# Autodev: DESIGN.md vs 구현 Gap 분석 리포트

> **Date**: 2026-02-22
> **Scope**: `plugins/autodev/DESIGN.md` ↔ `plugins/autodev/cli/src/` 전체 소스코드 대조
> **방법**: DESIGN.md 섹션별로 실제 구현 코드를 1:1 대조하여 gap 식별

---

## 1. Executive Summary

GAP-ANALYSIS.md는 "모든 gap 해소됨"으로 보고하고 있으나, **독립 검증 결과 아직 남아있는 gap이 존재**한다.
근본적 아키텍처 전환(SQLite 큐 → In-Memory StateQueue, GitHub Labels SSOT)은 성공적으로 완료되었으나,
설계 문서의 세부 사항과 실제 구현 사이에 **설정, 흐름, 구조적 불일치**가 남아있다.

### 요약

| 심각도 | 건수 | 설명 |
|--------|------|------|
| **High** | 3건 | 설계와 동작이 명확히 다름 |
| **Medium** | 5건 | 기능 누락 또는 부분 구현 |
| **Low** | 4건 | 사소한 불일치, 문서 정합성 |

---

## 2. High Priority Gaps

### H-01: `reconcile_window_hours` 하드코딩 — 설정 불가

| | 설계 (DESIGN.md §12) | 구현 (daemon/mod.rs:58) |
|---|------|------|
| 위치 | `DaemonConfig.reconcile_window_hours` | 로컬 변수 `24u32` |
| 설정 | YAML: `daemon.reconcile_window_hours: 24` | 하드코딩 |
| 변경 가능성 | 사용자 설정 가능 | 코드 수정 필요 |

**설계**: `DaemonConfig` 구조체에 `reconcile_window_hours: u32` 필드가 있어야 함 (§12, §5).
**구현**: `ConsumerConfig`에 해당 필드 없음. `daemon/mod.rs:58`에서 `let reconcile_window_hours = 24u32;`로 하드코딩.

`startup_reconcile()` 함수 자체는 파라미터로 받으므로, **설정 구조체에 필드를 추가하고 연결만 하면 해결**.

**파일**: `config/models.rs`, `daemon/mod.rs:58`

---

### H-02: Config 구조 불일치 — `DaemonConfig` 부재

| | 설계 (DESIGN.md §12) | 구현 (config/models.rs) |
|---|------|------|
| 최상위 | `repos[]` + `daemon{}` | `consumer{}` + `workflow{}` + `commands{}` + `develop{}` |
| 데몬 설정 | `DaemonConfig { tick_interval_secs, reconcile_window_hours, log_dir, log_retention_days, daily_report_hour }` | `ConsumerConfig` 안에 `daily_report_hour`, `scan_interval_secs` 등이 혼재 |
| 레포별 설정 | `repos[].scan_interval_secs`, `repos[].auto_merge` 등 | `ConsumerConfig` 플랫 구조 (레포별 분리 없음) |

**설계**: 명확하게 분리된 2-tier 설정 (`repos[]` 배열 + `daemon{}` 객체).
**구현**: `WorkflowConfig { consumer, workflow, commands, develop }` — 레포별 설정과 데몬 설정이 분리되지 않음.

이로 인해:
- `tick_interval_secs`는 설정에 없고 `daemon/mod.rs:148`에서 `Duration::from_secs(10)` 하드코딩
- `log_dir`, `log_retention_days` 설정 없음
- 레포별 `auto_merge`, `merge_require_ci`, `model` 등이 per-repo가 아닌 글로벌

**파일**: `config/models.rs`, `daemon/mod.rs`

---

### H-03: PR 리뷰 verdict 판정 — exit_code 기반 vs JSON verdict 파싱

| | 설계 (DESIGN.md §6 PR Flow) | 구현 (pipeline/pr.rs) |
|---|------|------|
| 리뷰 결과 | `ReviewResult { verdict: "approve" \| "request_changes", summary, comments }` | `Reviewer.review_pr()` → `exit_code` 기반 분기 |
| approve 판정 | `verdict == "approve"` | `exit_code == 0` |
| request_changes 판정 | `verdict == "request_changes"` | `exit_code != 0` |
| GitHub PR review | `POST /pulls/{N}/reviews` (event: REQUEST_CHANGES + inline comments) | `notifier.post_issue_comment()` (일반 댓글) |

**설계**: JSON으로 구조화된 ReviewResult를 파싱하여 `approve`/`request_changes`를 결정적으로 분기.
GitHub PR review API로 정식 리뷰를 게시 (inline comments 포함).

**구현**:
- `process_pending()`: `exit_code == 0` → ReviewDone 전이 (모든 리뷰를 request_changes로 취급)
- `process_improved()`: `exit_code == 0` → done (approve), `exit_code != 0` → ReviewDone 재진입
- 리뷰 결과를 일반 이슈 댓글로 게시 (`issue_comment`), PR review API가 아님
- `ReviewResult` 구조체 자체가 구현에 존재하지 않음

**영향**: approve와 request_changes를 구분하지 못하므로, 모든 리뷰가 피드백 반영 사이클을 거치게 됨.
첫 리뷰에서 approve가 나와도 불필요하게 improve → re-review를 실행함.

**파일**: `pipeline/pr.rs`, `components/reviewer.rs`, `infrastructure/claude/output.rs`

---

## 3. Medium Priority Gaps

### M-01: Phase 상태 축소 — Analyzing/Implementing 미구현

| | 설계 (DESIGN.md §2 Phase 정의) | 구현 (task_queues.rs) |
|---|------|------|
| Issue phases | `Pending → Analyzing → Ready → Implementing` | `PENDING → READY` (2개만) |
| PR phases | `Pending → Reviewing → ReviewDone → Improving → Improved` | `PENDING → REVIEW_DONE → IMPROVED` (3개) |
| Merge phases | `Pending → Merging → Conflict` | `PENDING` (1개만) |

**설계**: 6개의 Issue 상태, 5개의 PR 상태, 3개의 Merge 상태로 세밀한 상태 추적.
**구현**: Analyzing/Implementing/Reviewing/Merging/Conflict가 함수 내부 로컬 상태로 처리됨 (큐에 명시적으로 존재하지 않음).

**영향**: TUI 대시보드에서 현재 진행 중인 작업의 세부 상태(Analyzing vs Implementing)를 표시할 수 없음.
데몬 로그에서도 상태 전이 이벤트가 설계만큼 세밀하지 않음.

---

### M-02: Merge scan 미구현 — approved PR 자동 감지 없음

| | 설계 (DESIGN.md §6 scan) | 구현 (scanner/pulls.rs) |
|---|------|------|
| PR scan | 새 PR → review queue | 새 PR → review queue |
| **Merge scan** | **approved PR → merge queue** | **미구현** |

**설계**: `scan()` 내에서 PR scan과 별도로 merge scan을 실행.
approved + CI 통과 + auto_merge 설정 활성화된 PR을 발견하여 merge queue에 적재.

**구현**: `scanner/pulls.rs`는 PR을 review queue에만 적재. Merge queue에 적재하는 별도 스캔 로직 없음.
merge pipeline은 존재하지만 merge queue에 아이템을 넣는 경로가 없음.

**영향**: auto_merge가 작동하지 않음. 사람이 approve하거나 autodev가 approve한 PR이 자동으로 머지되지 않음.

**파일**: `scanner/pulls.rs`, `scanner/mod.rs`

---

### M-03: suggest-workflow 통합 미구현 — Knowledge Extraction 데이터 소스 절반 누락

| | 설계 (DESIGN.md §13) | 구현 (knowledge/) |
|---|------|------|
| 데이터 소스 A | daemon.YYYY-MM-DD.log | ✅ `parse_daemon_log()` |
| **데이터 소스 B** | **suggest-workflow index.db 조회** | **❌ 미구현** |
| 세션 식별 | `suggest-workflow query --session-filter "first_prompt_snippet LIKE '[autodev]%'"` | 프롬프트에 `[autodev]` 마커 삽입만 (조회 없음) |

**설계**: per-task에서 suggest-workflow의 세션 데이터(도구 사용 패턴, 파일 수정 이력)를 교차 분석.
daily에서 suggest-workflow의 cross-session 데이터를 분석하여 이상치 발견.

**구현**: knowledge 모듈은 daemon.log 파싱 + Claude LLM 호출만 수행.
suggest-workflow CLI 호출이나 index.db 접근이 전혀 없음.

**영향**: Knowledge Extraction의 분석 깊이가 설계 대비 절반 수준.
도구 사용 패턴, 파일 수정 이상치 등의 인사이트를 발견할 수 없음.

---

### M-04: Cargo.toml 의존성 불일치

| 의존성 | 설계 (DESIGN.md §4) | 구현 (Cargo.toml) |
|--------|------|------|
| `reqwest` | 있음 (HTTP client) | **없음** |
| `tracing-appender` | 있음 (로그 롤링) | **없음** |
| `async-trait` | 없음 | **있음** |
| `libc` | 없음 | **있음** |
| `serde_yaml` | 없음 | **있음** |
| version | `0.1.0` | `0.2.3` |

**설계**: `reqwest`로 GitHub API 직접 호출, `tracing-appender`로 일자별 로그 롤링.
**구현**: GitHub API는 `gh` CLI를 통해 호출 (reqwest 불필요), 로그 롤링은 미구현.

**영향**: 로그 롤링(일자별 자동 생성, 보존 기간 후 삭제)이 구현되지 않음.

---

### M-05: 로그 롤링/보존 미구현

| | 설계 (DESIGN.md §9) | 구현 |
|---|------|------|
| 롤링 | `tracing-appender::rolling::daily()` | 미구현 (stdout 로깅) |
| 보존 | `log_retention_days` 설정 후 자동 삭제 | 미구현 |
| 파일명 | `daemon.YYYY-MM-DD.log` | daily report에서 참조하지만 생성 로직 없음 |

`daemon/mod.rs:107`에서 `home.join(format!("daemon.{yesterday}.log"))`을 참조하지만,
실제로 이 파일을 생성하는 로그 설정이 없음. stdout으로 로깅하는 것으로 보임.

---

## 4. Low Priority Gaps

### L-01: TUI 대시보드 — 설계와 구현 범위 차이

설계(§10)에서는 Active Items, Labels Summary, Activity Log를 보여주는 상세 레이아웃을 정의.
구현은 기본적인 TUI 프레임워크가 존재하나, 설계의 모든 뷰가 구현되어 있는지는 별도 검증 필요.

### L-02: `Reviewer.review_pr()` 반환 타입 — `ReviewOutput` vs `ReviewResult`

설계에서는 `ReviewResult { verdict, summary, comments[] }`를 정의하지만,
구현의 `Reviewer`는 `ReviewOutput { stdout, stderr, exit_code, review }` 구조체를 반환.
`review` 필드는 String이며 구조화된 verdict 파싱 없음.

### L-03: `session/output.rs` — 설계에 존재하나 구현에서 불명확

DESIGN.md §3 디렉토리 구조에 `session/output.rs`가 있으나,
실제 출력 파싱은 `infrastructure/claude/output.rs`에 위치. 모듈 이름 불일치.

### L-04: 문서 간 정합성

`GAP-ANALYSIS.md`는 "182 tests passing, zero compiler warnings, 모든 gap 해소"로 보고.
실제로는 본 리포트에서 식별한 12건의 gap이 존재. 이는 GAP-ANALYSIS.md가 **리팩토링 scope 내의 gap만 추적**했기 때문.
설계 문서 전체 대비 정합성 검증은 수행되지 않았음.

---

## 5. 구현이 설계보다 나은 부분

| 항목 | 설계 | 구현 | 평가 |
|------|------|------|------|
| Pre-flight check | 불필요 (scan에서 확인) | `notifier.is_issue_open()`, `is_pr_reviewable()`, `is_pr_mergeable()` 호출 | **구현이 안전** — scan-consume 시차 고려 |
| Verdict enum | 미정의 (String 암시) | `Verdict { Implement, NeedsClarification, Wontfix }` enum + serde | **구현이 타입 안전** |
| confidence_threshold | 설정 존재 암시 | `ConsumerConfig.confidence_threshold: f64` (default 0.7) | **구현이 실용적** |
| Concurrency 제어 | 미정의 | `issue_concurrency`, `pr_concurrency`, `merge_concurrency` 설정 | **구현이 실용적** |
| Git trait 확장 | `checkout_new_branch`, `add_commit_push` 필요 언급 | 구현 완료 (knowledge PR 생성에 사용) | 설계 요구사항 충족 |
| `create_issue`, `create_pr` | DESIGN.md §13에서 필요성 언급 | `Gh` trait에 구현 완료 | 설계 요구사항 충족 |

---

## 6. Gap별 수정 난이도

| ID | 내용 | 난이도 | 추정 수정 범위 |
|----|------|--------|-------------|
| H-01 | reconcile_window_hours 설정화 | 낮음 | `ConsumerConfig` 필드 추가 + daemon 연결 |
| H-02 | Config 구조 정렬 | 높음 | 설정 스키마 전면 재설계 or 설계 문서 갱신 |
| H-03 | PR verdict 파싱 | 중간 | `ReviewResult` 구조체 + JSON 파싱 + PR review API |
| M-01 | Phase 세분화 | 중간 | phase 상수 추가 + push/pop 지점 수정 |
| M-02 | Merge scan | 중간 | `scanner/pulls.rs`에 approved PR 감지 로직 추가 |
| M-03 | suggest-workflow 통합 | 높음 | CLI 호출 로직 + 데이터 교차 분석 |
| M-04 | Cargo.toml 정리 | 낮음 | 불필요한 의존성 정리 or 설계 갱신 |
| M-05 | 로그 롤링 | 중간 | tracing-appender 도입 + retention 로직 |

---

## 7. 권장 사항

### 우선 수정 (구현 변경)
1. **H-01**: `ConsumerConfig`에 `reconcile_window_hours` 추가, daemon에서 설정값 사용
2. **H-03**: `ReviewResult` 구조체 정의, PR 리뷰 결과를 JSON 파싱하여 approve/request_changes 분기
3. **M-02**: Merge scan 로직 추가 — approved + CI 통과 PR을 merge queue에 적재

### 설계 문서 갱신 검토 (구현이 더 나은 경우)
1. **H-02**: Config 구조는 구현의 `WorkflowConfig` 방식이 실제 사용에 더 적합할 수 있음. DESIGN.md §12를 현행 구현에 맞게 갱신하는 것도 선택지
2. **M-01**: Phase 축소는 구현 복잡도를 줄이는 실용적 판단. 단, TUI에서 세부 상태 표시가 필요하면 설계대로 확장
3. **M-04**: Cargo.toml은 구현이 `gh` CLI 기반으로 전환되어 reqwest 불필요. 설계 갱신이 적절

### 후속 작업
1. **M-03**: suggest-workflow 통합은 scope가 크므로 별도 이슈로 분리
2. **M-05**: 로그 롤링은 운영 안정성에 필요. `tracing-appender` 도입 권장
3. **L-04**: GAP-ANALYSIS.md를 본 리포트 결과로 갱신

---

## 8. 결론

autodev 플러그인의 **핵심 아키텍처 전환(In-Memory StateQueue + GitHub Labels SSOT)은 성공적**으로 완료됨.
DESIGN.md의 근본 철학이 코드에 반영되어 있으며, 182개 테스트가 통과하는 안정적 상태.

그러나 설계 문서의 세부 사항과 대조하면 **12건의 gap이 존재**하며,
특히 PR 리뷰 verdict 파싱(H-03)과 merge scan 미구현(M-02)은 자동화 품질에 직접적 영향을 미침.

이 gap들은 **설계를 구현에 맞추거나, 구현을 설계에 맞추는** 양방향 판단이 필요하며,
각 gap별로 어느 방향이 적절한지는 위 §7 권장 사항을 참고.

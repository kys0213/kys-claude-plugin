# DESIGN-v2 구현 계획

> **Date**: 2026-02-24
> **Base**: DESIGN-v2.md
> **현재 코드베이스 상태**: REFACTORING-PLAN.md 완료 (SQLite → In-memory StateQueue 전환 완료)
> **진행 상태**: Phase A~D 완료, Phase E 60% (2026-02-25 검증 — design-v2-gap-analysis-final.md 참조)

---

## 요구사항 정리

DESIGN-v2의 핵심 변경 3가지:

1. **분석 리뷰 게이트 (HITL)**: `process_pending()` 분석 완료 후 `analyzed` 라벨 → 사람 리뷰 대기 → `approved-analysis` 라벨 시 `scan_approved()`로 재진입
2. **Issue-PR 연동**: 구현 완료 시 PR 생성 → PR approve 시 source issue도 done 전이
3. **Knowledge Extraction v2**: Delta-aware 추출 + Actionable PR 생성 + Daily 교차 task 패턴

---

## 사이드이펙트 분석

### 직접 영향

| 파일 | 변경 유형 | 영향도 |
|------|----------|--------|
| `queue/task_queues.rs` | 라벨 상수 추가, `PrItem` 필드 추가 | **높음** — 모든 PrItem 생성 코드 수정 필요 |
| `pipeline/issue.rs` | `process_pending()` 로직 변경, `process_ready()` PR 연동 | **높음** — 핵심 파이프라인 변경 |
| `pipeline/pr.rs` | approve 경로에 Issue done 전이 추가 | **중간** |
| `scanner/issues.rs` | `scan_approved()` 신규 함수 | **낮음** — 새 함수 추가 |
| `scanner/mod.rs` | `scan_all()`에 호출 추가 | **낮음** |
| `components/verdict.rs` | `format_analysis_comment()` 추가 | **낮음** — 새 함수 |
| `infrastructure/claude/output.rs` | `extract_pr_number()` 추가 | **낮음** — 새 함수 |
| `daemon/mod.rs` | `startup_reconcile()` 라벨 필터 확장 | **중간** |
| `knowledge/extractor.rs` | delta check + actionable PR | **중간** |
| `knowledge/daily.rs` | 교차 task 패턴 집계 | **중간** |

### 간접 영향 (PrItem 필드 추가로 인한 cascading)

`PrItem`에 `source_issue_number: Option<i64>` 추가 시 **모든** PrItem 생성 코드에 영향:

1. `scanner/pulls.rs` — `scan()` 에서 PrItem 생성 → `source_issue_number: None` 추가
2. `pipeline/pr.rs` — `process_review_done()` 등 PrItem 복사/이동 부분
3. `daemon/mod.rs` — `startup_reconcile()` 에서 PrItem 생성 → `source_issue_number: None` 추가
4. `knowledge/daily.rs` — `create_knowledge_prs()` 에서 PR 생성 시
5. **테스트 코드** — PrItem을 직접 생성하는 모든 테스트 (queue/task_queues.rs 테스트, daemon/mod.rs 테스트)

### IssueItem 변경 필요성

DESIGN-v2의 `scan_approved()`에서 `analysis_report` 필드를 활용함 → **IssueItem 구조체는 변경 불필요** (이미 `analysis_report: Option<String>` 필드가 존재)

### 테스트 영향

| 테스트 | 영향 | 대응 |
|--------|------|------|
| `queue/task_queues.rs` 테스트 | PrItem 생성자 변경 | `source_issue_number: None` 추가 |
| `daemon/mod.rs` 테스트 | reconcile 로직 변경 + PrItem 변경 | 새 라벨 케이스 추가 + 필드 추가 |
| `pipeline_e2e_tests` (만약 존재) | process_pending 동작 변경 | 기대값 수정 |
| `knowledge/extractor.rs` 테스트 | 시그니처 확장 시 | 마이그레이션 |

---

## 구현 Phase 계획

### Phase A: Labels + Models (기반) — ✅ 완료

**목적**: 나머지 Phase의 기반이 되는 타입 변경. 이 Phase가 완료되어야 B~E를 진행 가능.

#### A-1. 라벨 상수 추가
- **파일**: `cli/src/queue/task_queues.rs`
- **변경**: `labels` 모듈에 3개 상수 추가
  ```rust
  pub const ANALYZED: &str = "autodev:analyzed";
  pub const APPROVED_ANALYSIS: &str = "autodev:approved-analysis";
  pub const IMPLEMENTING: &str = "autodev:implementing";
  ```

#### A-2. PrItem에 `source_issue_number` 필드 추가
- **파일**: `cli/src/queue/task_queues.rs`
- **변경**: `PrItem` 구조체에 `pub source_issue_number: Option<i64>` 추가

#### A-3. PrItem 생성 코드 일괄 수정 (cascading)
- **파일들**:
  - `scanner/pulls.rs` — `scan()` 내 PrItem 생성
  - `daemon/mod.rs` — `startup_reconcile()` 내 PrItem 생성
  - `knowledge/daily.rs` — `create_knowledge_prs()` 내 PrItem 생성 (있다면)
  - `queue/task_queues.rs` 테스트 — PrItem 생성하는 모든 테스트
  - `daemon/mod.rs` 테스트 — PrItem 생성하는 모든 테스트
- **변경**: 모든 PrItem 생성에 `source_issue_number: None` 추가

#### A-4. 기존 테스트 통과 확인
- `cargo test` 실행 → 전부 통과해야 함 (additive 변경이므로)

**검증**: `cargo test` + `cargo clippy`

---

### Phase B: 분석 리뷰 게이트 — ✅ 완료

**목적**: `process_pending()` 결과가 바로 Ready로 가지 않고, `analyzed` 라벨 + 코멘트 게시 후 queue 이탈

#### B-1. `format_analysis_comment()` 추가
- **파일**: `cli/src/components/verdict.rs`
- **변경**: 분석 리포트를 GitHub 이슈 코멘트 포맷으로 생성하는 함수
- **TDD**: 포맷 결과에 `<!-- autodev:analysis -->` 마커, verdict, confidence, report 포함 검증

#### B-2. `process_pending()` 분석 완료 경로 변경
- **파일**: `cli/src/pipeline/issue.rs`
- **변경** (핵심):
  - 기존: `Implement` verdict → `queue.push(READY)` (내부 전이)
  - 변경: `Implement` verdict → 분석 코멘트 게시 + `wip` 라벨 제거 + `analyzed` 라벨 추가 + queue에서 제거
- **주의**: `NeedsClarification`, `Wontfix` 경로는 기존 유지

#### B-3. 재분석 Safety Valve 추가
- **파일**: `cli/src/scanner/issues.rs`
- **변경**: `scan()` 에서 Pending 적재 전 `count_analysis_comments()` 호출
  - 분석 코멘트 수 >= `MAX_ANALYSIS_ATTEMPTS`(기본 3) → `autodev:skip` 라벨 + 안내 코멘트
  - 그 외 → 기존 Pending 적재 로직 유지
- **TDD**: 분석 코멘트 3개 이상인 이슈 → skip 전이 검증, 0~2개인 이슈 → 정상 Pending 적재 검증

#### B-4. 테스트 작성
- 분석 성공 (`Implement` verdict) 시:
  - `autodev:analyzed` 라벨 추가 확인
  - `autodev:wip` 라벨 제거 확인
  - 이슈 코멘트에 분석 리포트 게시 확인
  - queue에서 완전 제거 확인 (Ready로 이동하지 않음)
- 기존 `NeedsClarification`, `Wontfix` 테스트가 여전히 통과하는지 확인
- 재분석 Safety Valve: 분석 코멘트 3회 이상 → skip 전이 검증

**검증**: `cargo test` — process_pending + safety valve 관련 테스트 통과

---

### Phase C: Approved Scan + 구현 — ✅ 완료

**목적**: 사람이 `approved-analysis` 라벨을 추가하면, `scan_approved()`가 감지하여 Ready 큐에 적재 → 구현 → PR 생성

#### C-1. `extract_analysis_from_comments()` 추가
- **파일**: `cli/src/scanner/issues.rs`
- **변경**: 이슈 코멘트에서 `<!-- autodev:analysis -->` 마커가 포함된 최신 코멘트의 body를 추출
- **TDD**: Gh mock으로 코멘트 목록 반환 → 분석 리포트 추출 검증

#### C-2. `scan_approved()` 추가
- **파일**: `cli/src/scanner/issues.rs`
- **변경**:
  - `autodev:approved-analysis` 라벨이 있는 open 이슈 조회
  - `implementing` 라벨 **먼저 추가** → `approved-analysis` 라벨 제거 (크래시 시 "라벨 없음" 방지)
  - 분석 리포트를 코멘트에서 추출
  - `IssueItem` 생성 (analysis_report 포함)
  - `Ready` 큐에 push
- **TDD**: Mock API → approved 이슈 반환 → Ready 큐 적재 + 라벨 전이 검증

#### C-3. `scan_all()`에 `scan_approved()` 호출 추가
- **파일**: `cli/src/scanner/mod.rs`
- **변경**: `"issues"` 타겟 처리 블록에 `issues::scan_approved()` 호출 추가

#### C-4. `extract_pr_number()` 추가
- **파일**: `cli/src/infrastructure/claude/output.rs`
- **변경**: Claude 세션 stdout에서 PR 번호를 추출하는 유틸리티
  - 패턴 1: `github.com/org/repo/pull/123` URL
  - 패턴 2: JSON `{"pr_number": 123}`
- **의존성**: `regex` crate 필요 — Cargo.toml에 이미 있는지 확인, 없으면 추가
- **TDD**: 다양한 stdout 포맷에서 PR 번호 추출 검증

#### C-5. `process_ready()` PR 생성 + PR queue push
- **파일**: `cli/src/pipeline/issue.rs`
- **변경** (핵심):
  - 기존: 구현 성공 → `autodev:done` (이슈 완료)
  - 변경: 구현 성공 → PR 번호 추출 (stdout 파싱 + `find_existing_pr()` fallback) → PrItem 생성 (`source_issue_number` 설정) → PR queue push + `autodev:wip` (PR) → **이슈 코멘트 게시** (`<!-- autodev:pr-link:{N} -->` 마커, recovery 추적용) → Issue queue에서 제거
  - PR 번호 추출 실패 시 → `implementing` 라벨 제거 + queue 제거 (다음 scan에서 재시도)
- **주의**: 더 이상 knowledge extraction을 process_ready()에서 직접 호출하지 않음 (PR approve 시점에서 호출)

#### C-6. 테스트 작성
- `scan_approved()`: approved 이슈 → Ready 큐, 라벨 전이, dedup
- `extract_pr_number()`: URL 패턴, JSON 패턴, 없는 경우
- `process_ready()`: PR 생성 성공 → PR queue push + source_issue_number 설정
- `process_ready()`: PR 번호 추출 실패 → 에러 복구

**검증**: `cargo test` — 새 함수 + process_ready 테스트 통과

---

### Phase D: Issue-PR 연동 — ✅ 완료

**목적**: PR approve 시 source issue도 자동으로 done 전이

#### D-1. PR pipeline worktree 정리 추가
- **파일**: `cli/src/pipeline/pr.rs`
- **변경**: `process_pending()`, `process_review_done()`, `process_improved()` 각 함수에서 작업 완료 후 `remove_worktree()` 호출 추가
  - 현재 `pr.rs` 전체에 `remove_worktree()` 호출이 0건 → worktree 누적 문제
  - `issue.rs`와 동일한 패턴: 함수 끝에서 success/failure 모두 정리
  - **주의**: `process_review_done()`은 `git push` 완료 후에 정리해야 함
- **branch 보존**: `remove_worktree()`는 worktree 디렉토리만 제거. remote branch는 유지됨
  → 다음 단계(`process_improved()` 등)에서 `create_worktree(task_id, head_branch)`로 재생성 가능

#### D-2. PR approve 경로에 Issue done 전이 추가
- **파일**: `cli/src/pipeline/pr.rs`
- **변경**: `process_pending()` 및 `process_improved()` 의 approve 분기에서:
  ```rust
  if let Some(issue_num) = item.source_issue_number {
      gh.label_remove(repo, issue_num, labels::IMPLEMENTING, gh_host).await;
      gh.label_add(repo, issue_num, labels::DONE, gh_host).await;
  }
  ```
- **위치**: knowledge extraction 후, done 전이 전

#### D-2. `startup_reconcile()` 라벨 필터 확장
- **파일**: `cli/src/daemon/mod.rs`
- **변경**: Issue reconcile 로직에 새 라벨 케이스 추가:
  - `autodev:analyzed` → skip (사람 리뷰 대기)
  - `autodev:approved-analysis` → `implementing` 라벨 전이 + Ready 큐 적재
  - `autodev:implementing` → skip (PR pipeline이 처리)
- **주의**: 기존 `done/skip` 필터는 유지

#### D-3. Recovery 로직 확장 (선택적)
- **파일**: `cli/src/daemon/recovery.rs`
- **변경**: `autodev:implementing` + 연결 PR이 이미 merged/closed → done 전이
- **복잡도**: PR 조회가 필요하므로 Phase E 이후로 미룰 수도 있음

#### D-5. 테스트 작성
- PR approve 시 `source_issue_number`가 Some이면 Issue done 전이
- PR approve 시 `source_issue_number`가 None이면 기존 동작 유지
- reconcile: `analyzed` 라벨 → skip
- reconcile: `approved-analysis` → Ready 적재 + 라벨 전이
- reconcile: `implementing` → skip
- PR pipeline worktree 정리: 각 process_* 함수 완료 후 worktree 제거 확인

**검증**: `cargo test` — PR pipeline + reconcile + worktree 정리 테스트 통과

---

### Phase E: Knowledge Extraction v2 — 🔶 60% (잔존 갭 3건)

**목적**: Delta-aware 지식 추출 + Actionable PR 생성 + Daily 교차 task 패턴

#### E-1. `collect_existing_knowledge()` 추가
- **파일**: `cli/src/knowledge/extractor.rs`
- **변경**: worktree에서 기존 지식 베이스를 문자열로 수집
  - CLAUDE.md, .claude/rules/*.md, plugins/*/commands/*.md 등
- **TDD**: tempdir에 파일 배치 → 수집 결과 검증

#### E-2. `extract_task_knowledge()` 확장
- **파일**: `cli/src/knowledge/extractor.rs`
- **변경**:
  - 기존 지식과 비교하는 delta check 프롬프트
  - suggestions가 비어있으면 skip (기존 지식과 차이 없음)
  - `Skill`/`Subagent` type suggestion → `create_knowledge_pr()` 호출
- **주의**: 기존 코멘트 게시 로직은 유지

#### E-3. `create_knowledge_pr()` 추가
- **파일**: `cli/src/knowledge/extractor.rs`
- **변경**: actionable suggestion으로 브랜치 생성 + 파일 작성 + PR 생성 + `autodev:skip` 라벨
- **의존성**: `Git` trait에 `add_and_commit()`, `push()` 메서드 필요 → 있는지 확인

#### E-4. `aggregate_daily_suggestions()` 추가
- **파일**: `cli/src/knowledge/daily.rs`
- **변경**: consumer_logs에서 당일 per-task knowledge extraction 결과를 집계

#### E-5. `detect_cross_task_patterns()` 추가
- **파일**: `cli/src/knowledge/daily.rs`
- **변경**: target_file 기준 그룹핑 → 2회 이상 등장하는 패턴 감지

#### E-6. 테스트 작성
- `collect_existing_knowledge()`: 파일 수집 결과 검증
- `extract_task_knowledge()`: delta check — 기존 지식과 동일하면 skip
- `create_knowledge_pr()`: PR 생성 플로우 (mock)
- `aggregate_daily_suggestions()`: 집계 결과 검증
- `detect_cross_task_patterns()`: 패턴 감지 검증

**검증**: `cargo test` + `cargo clippy`

---

## Phase 간 의존성 다이어그램

```
Phase A ─── (기반) ──┬── Phase B (분석 리뷰 게이트)
                     │
                     ├── Phase C (Approved Scan + 구현)
                     │       │
                     │       └── Phase D (Issue-PR 연동)
                     │               │
                     └───────────────┴── Phase E (Knowledge v2)
```

B, C는 A 완료 후 병렬 가능하나, C의 `process_ready()` 변경이 B의 `process_pending()` 변경과 같은 파일이므로 순차 진행 권장.

---

## 각 Phase별 검증 기준

| Phase | 검증 | 통과 조건 |
|-------|------|----------|
| A | `cargo test` + `cargo clippy` | 기존 테스트 전부 통과 |
| B | 위 + 새 테스트 | analyzed 라벨 전이 + queue 이탈 검증 |
| C | 위 + 새 테스트 | scan_approved + process_ready PR 생성 검증 |
| D | 위 + 새 테스트 | Issue-PR 연동 + reconcile 확장 + PR worktree 정리 검증 |
| E | 위 + 새 테스트 | delta check + daily 패턴 검증 |
| 최종 | `cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test` | Quality Gate 전부 통과 |

---

## 구현 순서 요약 (25 항목)

| # | Phase | 항목 | 파일 | 상태 |
|---|-------|------|------|------|
| 1 | A | 라벨 상수 추가 | queue/task_queues.rs | ✅ |
| 2 | A | `PrItem.source_issue_number` 추가 | queue/task_queues.rs | ✅ |
| 3 | A | PrItem 생성 코드 일괄 수정 | pulls.rs, daemon/mod.rs, daily.rs, 테스트 | ✅ |
| 4 | A | 기존 테스트 통과 확인 | — | ✅ |
| 5 | B | `format_analysis_comment()` 추가 | components/verdict.rs | ✅ |
| 6 | B | `process_pending()` 분석 완료 경로 변경 | pipeline/issue.rs | ✅ |
| 7 | B | 재분석 Safety Valve 추가 | scanner/issues.rs | ✅ |
| 8 | B | Phase B 테스트 | pipeline/issue.rs, verdict.rs, scanner/issues.rs | ✅ |
| 9 | C | `extract_analysis_from_comments()` 추가 | scanner/issues.rs | ✅ |
| 10 | C | `scan_approved()` 추가 | scanner/issues.rs | ✅ |
| 11 | C | `scan_all()` 호출 추가 | scanner/mod.rs | ✅ |
| 12 | C | `extract_pr_number()` + `find_existing_pr()` 추가 | infrastructure/claude/output.rs, pipeline/issue.rs | ✅ |
| 13 | C | `process_ready()` PR 생성 + queue push + pr-link 코멘트 | pipeline/issue.rs | ✅ |
| 14 | C | Phase C 테스트 | scanner/issues.rs, output.rs, pipeline/issue.rs | ✅ |
| 15 | D | PR pipeline worktree 정리 추가 | pipeline/pr.rs | ✅ |
| 16 | D | PR approve → Issue done 전이 | pipeline/pr.rs | ✅ |
| 17 | D | `startup_reconcile()` 라벨 필터 확장 | daemon/mod.rs | ✅ |
| 18 | D | Recovery 확장 (implementing + merged PR) | daemon/recovery.rs | ✅ |
| 19 | D | Phase D 테스트 | pipeline/pr.rs, daemon/mod.rs | ✅ |
| 20 | E | `collect_existing_knowledge()` | knowledge/extractor.rs | 🔶 plugins skills 누락 |
| 21 | E | `extract_task_knowledge()` 확장 | knowledge/extractor.rs | ✅ |
| 22 | E | `create_knowledge_pr()` (격리 worktree) | knowledge/extractor.rs | 🔶 worktree 격리 미적용 |
| 23 | E | `aggregate_daily_suggestions()` | knowledge/daily.rs | ❌ 미구현 |
| 24 | E | `detect_cross_task_patterns()` | knowledge/daily.rs | ✅ (입력 데이터 의존: #23) |
| 25 | E | Phase E 테스트 | knowledge/ | 🔶 부분 |

---

## 위험 요소 및 대응

| 위험 | 영향 | 대응 |
|------|------|------|
| `PrItem` 필드 추가로 인한 cascading 수정 | 컴파일 에러 다수 | Phase A에서 일괄 처리, 컴파일 확인 후 진행 |
| `process_pending()` 로직 변경으로 기존 테스트 실패 | 파이프라인 동작 변경 | Phase B에서 기존 테스트를 v2 기대값으로 수정 |
| `regex` crate 의존성 | 빌드 | Cargo.toml 확인 후 필요 시 추가 |
| `extract_analysis_from_comments()`의 Gh trait 메서드 | 인터페이스 확장 | `api_get_field()` 메서드가 이미 있으면 활용, 없으면 추가 |
| `create_knowledge_pr()`의 Git trait 메서드 | 인터페이스 확장 | `add_and_commit()`, `push()` 존재 확인 → 없으면 추가 |
| Daily extraction의 consumer_logs 테이블 구조 | DB 스키마 | 기존 스키마 확인 후 필요시 마이그레이션 |

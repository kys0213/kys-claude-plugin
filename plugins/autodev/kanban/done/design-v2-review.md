# DESIGN-v2 구현 검토 리포트

> **Date**: 2026-02-24
> **Reviewer**: autodev (claude)
> **Scope**: `plugins/autodev/DESIGN-v2.md` 체크리스트 대비 현재 구현 코드 비교

---

## Phase A: Labels + Models — PASS

| 항목 | 상태 | 파일:라인 |
|------|------|----------|
| `ANALYZED`, `APPROVED_ANALYSIS`, `IMPLEMENTING` 상수 | Pass | `queue/task_queues.rs:117-121` |
| `PrItem.source_issue_number: Option<i64>` | Pass | `queue/task_queues.rs:45` |
| PrItem 생성 코드 일괄 수정 (`source_issue_number: None`) | Pass | `scanner/pulls.rs:119`, `daemon/mod.rs:388`, 테스트 헬퍼 |

---

## Phase B: 분석 리뷰 게이트 — PASS

| 항목 | 상태 | 파일:라인 |
|------|------|----------|
| `format_analysis_comment()` | Pass | `components/verdict.rs:42-55` |
| `process_pending()` → analyzed 라벨 + 코멘트 + queue 이탈 | Pass | `pipeline/issue.rs:216-243` |
| Wontfix/NeedsClarification 경로 기존 유지 | Pass | `pipeline/issue.rs:172-214` |
| 파싱 실패 fallback → analyzed | Pass (추가 안전장치) | `pipeline/issue.rs:244-278` |

---

## Phase C: Approved Scan + 구현 — GAP

| 항목 | 상태 | 파일:라인 |
|------|------|----------|
| `scan_approved()` | Pass | `scanner/issues.rs:158-228` |
| `extract_analysis_from_comments()` | Pass | `scanner/issues.rs:231-257` |
| `scan_all()`에 호출 추가 | Pass | `scanner/mod.rs:63-68` |
| `extract_pr_number()` — URL 패턴 | Pass | `infrastructure/claude/output.rs:143-166` |
| `extract_pr_number()` — JSON `pr_number` 필드 | **Gap** | Pattern 2 미구현 |
| `process_ready()` — PR queue push | Pass | `pipeline/issue.rs:404-430` |
| `process_ready()` — Issue implementing 라벨 유지 | **Major Gap** | 아래 상세 |

### process_ready() 설계 불일치 (Major)

**설계**:
```
PR 생성 성공 → PR queue push → Issue queue 제거 → implementing 라벨 유지
  (PR pipeline이 approve 시 Issue done 전이)
PR 번호 추출 실패 → implementing 라벨 제거 → queue 제거 (재시도)
```

**구현** (`pipeline/issue.rs:432-475`):
```
PR 생성 성공 → PR queue push → Issue queue 제거 → implementing 제거 + done 추가  ❌
PR 번호 추출 실패 → implementing 제거 + done 추가  ❌
```

**영향**:
- Issue가 PR 생성 시점에 즉시 done 전이 → 설계의 "PR approve 시 Issue done" 시나리오 무효화
- PR 리뷰 실패/reject 시에도 Issue는 이미 done 상태
- `pipeline/pr.rs`의 `source_issue_number` 기반 done 전이 코드가 사실상 중복 (Issue 이미 done)

---

## Phase D: Issue-PR 연동 — PARTIAL

| 항목 | 상태 | 파일:라인 |
|------|------|----------|
| PR approve → source_issue done 전이 (process_pending) | 코드 존재, 사실상 무효 | `pipeline/pr.rs:179-192` |
| PR approve → source_issue done 전이 (process_improved) | 코드 존재, 사실상 무효 | `pipeline/pr.rs:478-492` |
| `startup_reconcile()` — analyzed skip | Pass | `daemon/mod.rs:269-271` |
| `startup_reconcile()` — implementing skip | Pass | `daemon/mod.rs:274-276` |
| `startup_reconcile()` — approved-analysis → Ready | Pass | `daemon/mod.rs:284-307` |
| Recovery: implementing + merged PR → done | **Not Implemented** | `daemon/recovery.rs` — orphan wip만 처리 |

---

## Phase E: Knowledge Extraction v2 — PARTIAL

| 항목 | 상태 | 파일:라인 |
|------|------|----------|
| `collect_existing_knowledge()` — CLAUDE.md | Pass | `knowledge/extractor.rs:19-26` |
| `collect_existing_knowledge()` — .claude/rules/*.md | Pass | `knowledge/extractor.rs:29-48` |
| `collect_existing_knowledge()` — plugins/*/commands/*.md | **Gap** | 미구현 |
| `collect_existing_knowledge()` — .claude/hooks.json | **Gap** | 미구현 |
| `collect_existing_knowledge()` — .develop-workflow.yaml | **Gap** | 미구현 |
| `extract_task_knowledge()` — delta check 프롬프트 | Pass | `knowledge/extractor.rs:72-98` |
| `extract_task_knowledge()` — empty suggestions → `Ok(None)` | **Gap** | 빈 suggestions도 `Some(ks)` 반환 |
| Per-task actionable PR (skill/subagent → PR) | **Not Implemented** | `extractor.rs`에 `create_knowledge_pr()` 없음 |
| `aggregate_daily_suggestions()` — DB 기반 집계 | **Not Implemented** | consumer_logs 쿼리 로직 없음 |
| `detect_cross_task_patterns()` | Implemented, unused | `daily.rs:368-396` — `#[allow(dead_code)]` |

---

## 종합

### 심각도별 분류

**High (설계 목표 미달)**:
1. `process_ready()` Issue 즉시 done 전이 — Phase D의 핵심 시나리오 무효화
2. Phase D의 Issue-PR 연동이 코드는 있으나 실질적 효과 없음

**Medium (기능 누락)**:
3. Per-task actionable PR 생성 미구현
4. `collect_existing_knowledge()` — skill/hook/workflow 파일 수집 누락
5. `aggregate_daily_suggestions()` 미구현
6. `detect_cross_task_patterns()` 미사용

**Low (영향 제한적)**:
7. `extract_pr_number()` JSON `pr_number` fallback 누락
8. `extract_task_knowledge()` empty suggestions 반환값 불일치
9. Recovery: implementing + merged PR → done 미구현

### 수정 우선순위 제안

1. **`process_ready()`**: PR 생성 성공 시 `implementing` 라벨 유지, `done` 추가 제거. PR 번호 추출 실패 시 `implementing` 라벨 제거만 (done 추가 안함).
2. **`extract_pr_number()`**: JSON `pr_number` 필드 파싱 추가.
3. **`collect_existing_knowledge()`**: plugins skills, hooks, workflow 파일 수집 추가.
4. **`extract_task_knowledge()`**: empty suggestions → `Ok(None)` 반환 + per-task actionable PR 생성.
5. **`detect_cross_task_patterns()`**: daily flow에서 실제 호출 연결.

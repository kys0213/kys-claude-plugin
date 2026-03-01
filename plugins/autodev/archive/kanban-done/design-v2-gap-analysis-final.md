# DESIGN-v2 최종 갭 분석 리포트

> **Date**: 2026-02-25
> **Scope**: `DESIGN-v2.md` 설계 vs 현재 구현 코드 — design-v2-review.md 후속 검증
> **결론**: 리뷰에서 식별된 9개 갭 중 대부분 해결됨, 3건 잔존

---

## 1. 해결된 갭 (Resolved)

| # | 항목 | 리뷰 당시 상태 | 수정 근거 |
|---|------|--------------|----------|
| 1 | `process_ready()` Issue 즉시 done 전이 | Major Gap | `pipeline/issue.rs:470-475` — implementing 유지, done 미추가 |
| 2 | `process_ready()` PR 번호 실패 시 done 추가 | Gap | `pipeline/issue.rs:477-491` — implementing 제거만, done 미추가 |
| 3 | `extract_pr_number()` JSON pr_number fallback | Gap | `output.rs:165-172` — Pattern 2로 JSON 파싱 구현 |
| 4 | `collect_existing_knowledge()` hooks.json | Gap | `extractor.rs:35-42` — `.claude/hooks.json` 수집 구현 |
| 5 | `collect_existing_knowledge()` workflow yaml | Gap | `extractor.rs:64-72` — `.develop-workflow.yaml` 수집 구현 |
| 6 | `extract_task_knowledge()` empty → Ok(None) | Gap | `extractor.rs:155` — `.filter(ks → !ks.suggestions.is_empty())` |
| 7 | Per-task actionable PR 생성 | Not Implemented | `extractor.rs:266-328` — `create_task_knowledge_prs()` 구현 |
| 8 | Recovery: `recover_orphan_implementing()` | Not Implemented | `recovery.rs:72-150` + `daemon/mod.rs:105` — 메인 루프 호출 |
| 9 | `detect_cross_task_patterns()` daily 연결 | Unused | `daemon/mod.rs:158` — daily flow에서 호출됨 |

---

## 2. 잔존 갭 (Open)

### Gap A: `collect_existing_knowledge()` — `plugins/*/commands/*.md` 미수집 [Medium]

**설계** (DESIGN-v2.md §8):
```
기존 지식 수집 대상:
  - plugins/*/commands/*.md (skill 정의)
```

**구현** (`extractor.rs:44-62`):
- `.claude-plugin/` 디렉토리(plugin.json 등)를 수집
- 설계의 `plugins/*/commands/*.md` (skill 파일)은 미구현

**영향**: delta check 시 기존 skill 정의를 인식하지 못해 중복 suggestion 발생 가능

**수정**: `collect_existing_knowledge()`에 plugins glob 수집 추가

---

### Gap B: `aggregate_daily_suggestions()` 미구현 [Medium]

**설계** (DESIGN-v2.md §8):
```rust
pub fn aggregate_daily_suggestions(db: &Database, date: &str) -> Vec<Suggestion> {
    // consumer_logs에서 당일 knowledge extraction 결과 집계
    // → stdout에서 KnowledgeSuggestion 파싱
    // → flat suggestions 반환
}
```

**구현**: 함수 자체가 존재하지 않음

**현재 흐름**:
- `daily.rs:152` — `suggestions: Vec::new()` (항상 빈 벡터)
- `daemon/mod.rs:158` — `detect_cross_task_patterns(&report.suggestions)` 호출
- **입력 데이터가 항상 비어있으므로 교차 패턴 감지가 동작하지 않음**

**영향**: 일간 리포트에서 per-task 결과를 교차 분석하는 핵심 시나리오 무효화

**수정**:
1. consumer_logs에서 당일 knowledge stdout 조회 로직 구현
2. KnowledgeSuggestion 파싱 → flat suggestions 수집
3. daily report 생성 시 호출하여 `report.suggestions` 채우기
4. 이후 `detect_cross_task_patterns()`가 실질적으로 동작

---

### Gap C: Knowledge PR worktree 격리 미적용 [Low]

**설계** (DESIGN-v2.md §8):
```rust
// main 기반 별도 worktree 생성 (구현 worktree와 격리)
let kn_wt_path = workspace.create_worktree(repo_name, &task_id, "main").await?;
```

**구현** (`extractor.rs:266-328`):
- `base_path` (구현 worktree)에서 직접 branch 생성 + 파일 작성
- `Workspace` trait 미의존, 별도 worktree 미생성

**영향**: 구현 worktree에서 knowledge branch를 만들면 uncommitted 변경과 충돌 가능성 존재. done 전이 직전(커밋 완료 후) 실행이므로 실제 확률은 낮음.

**수정**: `Workspace` 파라미터 추가 → main 기반 별도 worktree에서 격리 실행

---

## 3. 종합

```
설계 대비 구현 완성도:

Phase A (Labels + Models)         ████████████████████ 100%
Phase B (Analysis Review Gate)    ████████████████████ 100%
Phase C (Approved Scan + 구현)    ████████████████████ 100%
Phase D (Issue-PR 연동)           ████████████████████ 100%
Phase E (Knowledge Extraction v2) ████████████░░░░░░░░  60%
  - collect_existing_knowledge     ██████████████░░░░░░  75% (plugins skills 누락)
  - per-task actionable PR         ████████████████░░░░  80% (worktree 격리 누락)
  - daily aggregation              ░░░░░░░░░░░░░░░░░░░░   0% (aggregate 미구현)
  - cross-task patterns            ████████████████░░░░  80% (연결됨, 입력 데이터 없음)
```

### 수정 우선순위

1. **Gap B** (`aggregate_daily_suggestions`) — daily 교차 분석의 전제 조건
2. **Gap A** (`plugins/*/commands/*.md` 수집) — delta check 정확도
3. **Gap C** (worktree 격리) — 안전성 개선

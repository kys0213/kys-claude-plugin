# suggest-workflow 통합 (M-03)

> **Priority**: Medium — Knowledge Extraction 분석 깊이 향상
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-03
> **난이도**: 높음

## 배경

DESIGN.md §13에서 Knowledge Extraction은 두 가지 데이터 소스를 교차 분석하도록 설계됨:
- **A**: `daemon.YYYY-MM-DD.log` — "무엇을 처리했는가" (상태 전이, 에러, 시간)
- **B**: `suggest-workflow index.db` — "어떻게 실행했는가" (도구 사용, 파일 수정, 프롬프트)

현재 데이터 소스 A만 구현되어 있고, B는 전혀 연동되지 않음.

## 항목

- [ ] **31. suggest-workflow CLI wrapper 추가**
  - `infrastructure/suggest_workflow/mod.rs` — trait 정의
  - `infrastructure/suggest_workflow/real.rs` — `suggest-workflow query` CLI 래핑
  - `infrastructure/suggest_workflow/mock.rs` — 테스트용 mock

- [ ] **32. Per-task knowledge extraction에 suggest-workflow 연동**
  - `knowledge/extractor.rs` — done 전이 시 해당 세션의 tool-frequency, file-edits 조회
  - `suggest-workflow query --perspective tool-frequency --session-filter "session_id = '<id>'"`

- [ ] **33. Daily knowledge extraction에 suggest-workflow 연동**
  - `knowledge/daily.rs` — 전일 autodev 세션 전체의 cross-session 분석
  - `suggest-workflow query --perspective filtered-sessions --param prompt_pattern="[autodev]"`
  - `suggest-workflow query --perspective repetition --session-filter ...`

- [ ] **34. 교차 분석 로직 구현**
  - daemon.log 통계 + suggest-workflow 세션 데이터를 결합하여 인사이트 도출
  - 예: "src/api/ 수정 시 Bash:test 평균 대비 3배 호출 → 테스트 전략 개선 필요"

## 현재 상태

```rust
// knowledge/extractor.rs — Claude에게만 의존
let prompt = format!(
    "[autodev] knowledge: per-task {task_type} #{github_number}\n\n\
     Analyze the completed {task_type} task..."
);
let result = claude.run_session(wt_path, &prompt, None).await;
// ← suggest-workflow 데이터 미포함
```

## 세션 식별

autodev의 모든 `claude -p` 호출은 `[autodev]` 마커가 삽입됨 (구현 완료).
suggest-workflow가 `first_prompt_snippet`으로 이를 인덱싱하므로 조회 가능.

```bash
suggest-workflow query \
  --perspective filtered-sessions \
  --param prompt_pattern="[autodev]"
```

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `infrastructure/suggest_workflow/` | 새 모듈 (trait + real + mock) |
| `knowledge/extractor.rs` | suggest-workflow 조회 연동 |
| `knowledge/daily.rs` | cross-session 분석 연동 |
| `cli/Cargo.toml` | 필요 시 의존성 추가 |

## 완료 조건

- [ ] Per-task extraction에서 suggest-workflow 세션 데이터 활용
- [ ] Daily report에서 cross-session 패턴 분석 활용
- [ ] 교차 분석 결과가 GitHub 코멘트/리포트에 반영
- [ ] mock 기반 테스트 통과

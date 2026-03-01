# suggest-workflow 통합 (M-03)

> **Priority**: Medium — Knowledge Extraction 분석 깊이 향상
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-03
> **난이도**: 높음
> **Completed**: 2026-02-23

## 배경

DESIGN.md §13에서 Knowledge Extraction은 두 가지 데이터 소스를 교차 분석하도록 설계됨:
- **A**: `daemon.YYYY-MM-DD.log` — "무엇을 처리했는가" (상태 전이, 에러, 시간)
- **B**: `suggest-workflow index.db` — "어떻게 실행했는가" (도구 사용, 파일 수정, 프롬프트)

## 항목

- [x] **31. suggest-workflow CLI wrapper 추가**
  - `infrastructure/suggest_workflow/mod.rs` — `SuggestWorkflow` trait 정의
  - `infrastructure/suggest_workflow/real.rs` — `suggest-workflow query` CLI 래핑
  - `infrastructure/suggest_workflow/mock.rs` — 테스트용 MockSuggestWorkflow

- [x] **32. Per-task knowledge extraction에 suggest-workflow 연동**
  - `knowledge/extractor.rs` — done 전이 시 해당 세션의 tool-frequency 조회
  - session filter: `first_prompt_snippet LIKE '[autodev]%{task_type}%#{number}%'`

- [x] **33. Daily knowledge extraction에 suggest-workflow 연동**
  - `knowledge/daily.rs` — `enrich_with_cross_analysis()` 추가
  - filtered-sessions, tool-frequency, repetition 3개 perspective 조회

- [x] **34. 교차 분석 로직 구현**
  - `CrossAnalysis` 모델: tool_frequencies + anomalies + sessions
  - DailyReport에 cross_analysis 필드 추가
  - Claude 프롬프트에 교차 분석 데이터 + 해석 힌트 포함

## 구현 요약

### 변경 파일

| 파일 | 변경 내용 |
|------|----------|
| `infrastructure/suggest_workflow/mod.rs` | `SuggestWorkflow` trait (3 methods) |
| `infrastructure/suggest_workflow/real.rs` | `RealSuggestWorkflow` — CLI 실행 |
| `infrastructure/suggest_workflow/mock.rs` | `MockSuggestWorkflow` — 테스트용 |
| `infrastructure/mod.rs` | `pub mod suggest_workflow` 추가 |
| `knowledge/models.rs` | `ToolFrequencyEntry`, `SessionEntry`, `RepetitionEntry`, `CrossAnalysis` 모델 추가; `DailyReport.cross_analysis` 필드 추가 |
| `knowledge/extractor.rs` | `sw: &dyn SuggestWorkflow` 파라미터 추가; `build_suggest_workflow_section()` 함수 추가 |
| `knowledge/daily.rs` | `enrich_with_cross_analysis()` 함수 추가; `generate_daily_suggestions()` 프롬프트에 교차 분석 힌트 추가; `format_daily_report_body()`에 Cross Analysis 섹션 추가 |
| `pipeline/mod.rs` | `process_all()` 시그니처에 `sw` 추가 |
| `pipeline/issue.rs` | `process_ready()` 시그니처에 `sw` 추가 |
| `pipeline/pr.rs` | `process_pending()`, `process_improved()` 시그니처에 `sw` 추가 |
| `daemon/mod.rs` | `start()` 시그니처에 `sw` 추가; daily report에서 `enrich_with_cross_analysis()` 호출 |
| `main.rs` | `RealSuggestWorkflow` 생성 및 전달 |

### 테스트: 324 passing (기존 313 + 신규 11)

신규 테스트 (`tests/suggest_workflow_tests.rs`):
- MockSuggestWorkflow 기본 동작 (3개)
- Per-task extraction + suggest-workflow 연동 (2개)
- Daily report + cross analysis 연동 (4개)
- CrossAnalysis 직렬화/역직렬화 (2개)

## 완료 조건

- [x] Per-task extraction에서 suggest-workflow 세션 데이터 활용
- [x] Daily report에서 cross-session 패턴 분석 활용
- [x] 교차 분석 결과가 GitHub 코멘트/리포트에 반영
- [x] mock 기반 테스트 통과

# `aggregate_daily_suggestions()` 구현 (Gap B)

> **Priority**: Medium — daily 교차 분석의 전제 조건
> **출처**: design-v2-gap-analysis-final.md §2 Gap B
> **관련 계획**: IMPROVEMENT-PLAN-v2-gaps.md Phase 5, IMPLEMENTATION-PLAN-v2.md #23

## 배경

DESIGN-v2.md §8에서 정의된 `aggregate_daily_suggestions()` 함수가 미구현.
현재 `daily.rs:152`에서 `suggestions: Vec::new()`으로 항상 빈 벡터를 사용하고 있어,
`detect_cross_task_patterns()`이 연결되어 있어도 입력 데이터가 없어 교차 패턴 감지가 동작하지 않음.

## 항목

- [ ] **1. consumer_logs에서 당일 knowledge stdout 조회 로직 구현**
  - `knowledge/daily.rs` — `aggregate_daily_suggestions()` 함수 추가
  - consumer_logs 테이블에서 당일 knowledge extraction 결과 조회

- [ ] **2. KnowledgeSuggestion 파싱**
  - stdout에서 KnowledgeSuggestion 구조체 파싱
  - flat suggestions 벡터로 수집

- [ ] **3. daily report 생성 시 호출**
  - `daily.rs` — `report.suggestions` 채우기
  - 이후 `detect_cross_task_patterns()`가 실질적으로 동작

- [ ] **4. 테스트 추가**
  - aggregate 결과 검증
  - 빈 logs 시 빈 suggestions 반환 검증

## 영향

- `knowledge/daily.rs` — 함수 추가 + report 생성 로직 수정
- `daemon/mod.rs` — daily flow에서 이미 `detect_cross_task_patterns()` 호출 중 (변경 없음)

## 완료 조건

- [ ] daily report의 suggestions가 per-task knowledge 결과로 채워짐
- [ ] `detect_cross_task_patterns()`가 실질적으로 동작
- [ ] cargo test 통과

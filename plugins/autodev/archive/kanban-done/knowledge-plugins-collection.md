# `plugins/*/commands/*.md` Knowledge 수집 (Gap A)

> **Priority**: Medium — delta check 정확도에 영향
> **출처**: design-v2-gap-analysis-final.md §2 Gap A
> **관련 계획**: IMPROVEMENT-PLAN-v2-gaps.md Phase 4, IMPLEMENTATION-PLAN-v2.md #20

## 배경

DESIGN-v2.md §8에서 `collect_existing_knowledge()`의 수집 대상으로 `plugins/*/commands/*.md` (skill 정의)를 명시.
현재 구현(`extractor.rs:44-62`)은 `.claude-plugin/` 디렉토리를 수집하지만 설계의 skill 파일은 미구현.

delta check 시 기존 skill 정의를 인식하지 못해 중복 suggestion이 발생할 수 있음.

## 항목

- [ ] **1. `collect_existing_knowledge()` 확장**
  - `knowledge/extractor.rs` — plugins glob 수집 추가
  - `plugins/*/commands/*.md` 경로의 skill 파일 내용 수집

- [ ] **2. 테스트 추가**
  - tmpdir에 `plugins/test-plugin/commands/test.md` 배치 → 수집 결과 검증

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `knowledge/extractor.rs` | `collect_existing_knowledge()`에 plugins glob 추가 |

## 완료 조건

- [ ] skill 파일이 기존 지식으로 수집됨
- [ ] delta check 시 기존 skill과 중복되는 suggestion이 필터됨
- [ ] cargo test 통과

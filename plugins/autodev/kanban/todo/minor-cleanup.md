# 소규모 정리 (L-4, L-5, L-6)

> **Priority**: Low — 기능 영향 없음, 코드/설계 품질 개선
> **분석 리포트**: design-implementation-analysis.md §3-1, §3-3, §4-2
> **난이도**: 낮음

## 항목

### L-4: Analyzer 컴포넌트 분리 검토

- [ ] **1. `components/analyzer.rs` 추출 여부 결정**
  - 현재: 이슈 분석 로직이 `pipeline/issue.rs::process_pending()` 내 인라인
  - 설계: `Analyzer { claude: &dyn Claude }` 독립 컴포넌트
  - 옵션 A: 설계대로 분리 → `Reviewer`, `Merger`와 동일 패턴
  - 옵션 B: 설계 문서를 인라인 방식으로 갱신 (현 구현 유지)
  - **판단 기준**: 분석 로직 재사용 필요성 여부

### L-5: config show/edit CLI

- [ ] **2. `autodev config show` 구현 또는 설계에서 제거**
  - 현재: DESIGN.md §9에 `config show`, `config edit` 명시, 미구현
  - 구현 시: `config::loader::load_merged()` 결과를 YAML로 출력
  - 제거 시: DESIGN.md §9에서 해당 항목 삭제

### L-6: 미사용 ConsumerConfig 필드 정리

- [ ] **3. `stuck_threshold_secs` 사용 여부 확인 및 정리**
  - SQLite 기반 stuck 탐지용이었으나 InMemory 전환 후 미사용 가능성
  - `Grep "stuck_threshold"` → 참조 없으면 ConsumerConfig에서 제거
  - default 값(1800)만 있고 실제 로직에서 미참조 시 dead config

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `components/analyzer.rs` | (신규) 또는 설계 갱신 |
| `main.rs` | config show 서브커맨드 추가 (선택) |
| `config/models.rs` | stuck_threshold_secs 제거 (확인 후) |
| `DESIGN.md` | 결정에 따라 갱신 |

## 완료 조건

- [ ] 각 항목에 대해 구현 또는 설계 갱신 중 하나 완료
- [ ] dead config 제거 시 cargo test 통과

# 문서 정합성 갱신 (L-03 + L-04)

> **Priority**: Low — 기능 영향 없음, 문서 품질 개선
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §4 L-03, L-04
> **난이도**: 낮음

## 항목

- [ ] **35. DESIGN.md §3 모듈 경로 수정 (L-03)**
  - `session/output.rs` → `infrastructure/claude/output.rs`로 경로 갱신
  - `queue/repo_store.rs`, `queue/cursor_store.rs`, `queue/log_store.rs`는
    실제 구현에서 `queue/repository.rs`로 통합됨 — 반영 필요

- [ ] **36. DESIGN.md §4 Cargo.toml 갱신**
  - `reqwest` 제거 (gh CLI로 대체됨)
  - `async-trait`, `libc`, `serde_yaml` 추가
  - version `0.1.0` → `0.2.3` 반영

- [ ] **37. DESIGN.md §12 Config 구조 갱신**
  - 설계의 `repos[]` 배열 + `daemon{}` → 구현의 `WorkflowConfig` 구조 반영
  - 워크스페이스별 `.develop-workflow.yaml` 오버라이드 방식 문서화

- [ ] **38. GAP-ANALYSIS.md 갱신 (L-04)**
  - "모든 gap 해소" → DESIGN-GAP-REPORT.md v2 결과 반영
  - 리팩토링 scope gap vs 설계 전체 scope gap 구분 명시

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `DESIGN.md` | §3 경로, §4 Cargo.toml, §12 Config 갱신 |
| `GAP-ANALYSIS.md` | 현행 gap 상태 반영 |

## 완료 조건

- [ ] DESIGN.md의 디렉토리 구조가 실제 코드와 일치
- [ ] DESIGN.md의 Cargo.toml 섹션이 실제와 일치
- [ ] GAP-ANALYSIS.md가 최신 gap 상태를 반영

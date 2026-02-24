# 문서 정합성 갱신 (L-03 + L-04)

> **Priority**: Low — 기능 영향 없음, 문서 품질 개선
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §4 L-03, L-04
> **난이도**: 낮음

## 항목

- [x] **35. DESIGN.md §3 모듈 경로 수정 (L-03)**
  - `session/output.rs` → `knowledge/` 모듈로 교체 (output.rs는 이미 infrastructure/claude/ 하위에 기재)
  - `queue/repo_store.rs`, `queue/cursor_store.rs`, `queue/log_store.rs` → `queue/repository.rs` 통합 반영
  - `queue/task_queues.rs` 추가, `config/loader.rs` 추가

- [x] **36. DESIGN.md §4 Cargo.toml 갱신**
  - `reqwest` 제거 (gh CLI로 대체됨)
  - `async-trait`, `libc`, `serde_yaml` 추가
  - version `0.1.0` → `0.2.3` 반영
  - `[lib]` 섹션 + `[dev-dependencies]` 추가

- [x] **37. DESIGN.md §12 Config 구조 갱신**
  - `repos[]` 배열 + `daemon{}` → `WorkflowConfig` 5-section 구조 반영
  - `.develop-workflow.yaml` 파일명 + 글로벌/워크스페이스 오버라이드 방식 문서화
  - `DaemonConfig.log_dir`: `PathBuf` → `String` 수정

- [x] **38. GAP-ANALYSIS.md 갱신 (L-04)** — N/A
  - GAP-ANALYSIS.md 및 DESIGN-GAP-REPORT.md 파일이 존재하지 않음 (skip)

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `DESIGN.md` | §3 경로, §4 Cargo.toml, §12 Config 갱신 |

## 완료 조건

- [x] DESIGN.md의 디렉토리 구조가 실제 코드와 일치
- [x] DESIGN.md의 Cargo.toml 섹션이 실제와 일치
- [x] GAP-ANALYSIS.md — 파일 부재로 해당 없음

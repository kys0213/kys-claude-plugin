# 로그 롤링/보존 구현 (M-04 + M-05)

> **Priority**: Critical — Daily Report 작동에 필수
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-04, M-05
> **난이도**: 중간
> **완료일**: 2026-02-23

## 항목

- [x] **20. `tracing-appender` 의존성 추가**
  - `cli/Cargo.toml`에 `tracing-appender = "0.2"` 추가

- [x] **21. DaemonConfig에 로그 설정 필드 추가**
  - `config/models.rs` — `DaemonConfig`에 `log_dir: String`, `log_retention_days: u32` 추가
  - 기본값: `log_dir: "logs"`, `log_retention_days: 30`

- [x] **22. 데몬 시작 시 rolling appender 설정**
  - `main.rs` — `tracing_appender::rolling::RollingFileAppender` (DAILY rotation)
  - 파일명 형식: `daemon.YYYY-MM-DD.log`
  - stderr 출력도 병행 유지 (layered subscriber)

- [x] **23. 로그 보존 기간 초과 파일 자동 삭제**
  - `daemon/log.rs` — `cleanup_old_logs()` 구현 (단위 테스트 6개 포함)
  - `daemon/mod.rs` — 데몬 시작 시 + daily report 시 cleanup 호출
  - `daemon.YYYY-MM-DD.log` 패턴으로 대상 탐색

## 구현 요약

| 파일 | 변경 내용 |
|------|----------|
| `cli/Cargo.toml` | `tracing-appender = "0.2"` 의존성 |
| `config/models.rs` | `DaemonConfig`에 `log_dir`, `log_retention_days` 필드 |
| `config/mod.rs` | `resolve_log_dir()` 경로 해석 함수 |
| `main.rs` | `RollingFileAppender` + non-blocking + layered subscriber |
| `daemon/log.rs` | `cleanup_old_logs()` + 6개 단위 테스트 |
| `daemon/mod.rs` | 시작 시 cleanup + daily report 시 cleanup |

## 완료 조건

- [x] `daemon.YYYY-MM-DD.log` 파일이 `~/.autodev/logs/`에 자동 생성됨
- [x] Daily Report에서 전일 로그 파일을 정상적으로 읽을 수 있음
- [x] `log_retention_days` 초과 파일이 자동 삭제됨
- [x] 기존 테스트 모두 통과 (297개, 2026-02-23 기준)

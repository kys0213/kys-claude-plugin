# 로그 롤링/보존 구현 (M-04 + M-05)

> **Priority**: Critical — Daily Report 작동에 필수
> **Gap Report**: DESIGN-GAP-REPORT.md v2 §3 M-04, M-05
> **난이도**: 중간

## 항목

- [ ] **20. `tracing-appender` 의존성 추가**
  - `cli/Cargo.toml`에 `tracing-appender = "0.2"` 추가

- [ ] **21. DaemonConfig에 로그 설정 필드 추가**
  - `config/models.rs` — `DaemonConfig`에 `log_dir: String`, `log_retention_days: u32` 추가
  - 기본값: `log_dir: "~/.autodev/logs"`, `log_retention_days: 30`

- [ ] **22. 데몬 시작 시 rolling appender 설정**
  - `main.rs` — `tracing_subscriber::fmt()` → `tracing_appender::rolling::daily()` 전환
  - 파일명 형식: `daemon.YYYY-MM-DD.log`
  - stderr 출력도 병행 유지 (layered subscriber)

- [ ] **23. 로그 보존 기간 초과 파일 자동 삭제**
  - `daemon/mod.rs` — 데몬 시작 시 + 매일 자정에 `log_retention_days` 초과 파일 삭제
  - `~/.autodev/logs/daemon.*.log` 패턴으로 대상 탐색

## 현재 문제

`daemon/mod.rs:108`에서 `daemon.{yesterday}.log` 파일을 읽으려 하지만,
로그 파일이 생성되지 않아 Daily Report가 **항상 skip** 됨.

```rust
let log_path = home.join(format!("daemon.{yesterday}.log"));
if log_path.exists() { ... }  // ← 파일 미존재 → skip
```

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `cli/Cargo.toml` | `tracing-appender` 의존성 추가 |
| `config/models.rs` | `DaemonConfig`에 `log_dir`, `log_retention_days` 필드 |
| `main.rs` | rolling appender 설정 |
| `daemon/mod.rs` | 로그 retention 정리 로직 |

## 완료 조건

- [ ] `daemon.YYYY-MM-DD.log` 파일이 `~/.autodev/logs/`에 자동 생성됨
- [ ] Daily Report에서 전일 로그 파일을 정상적으로 읽을 수 있음
- [ ] `log_retention_days` 초과 파일이 자동 삭제됨
- [ ] 기존 테스트 185개 모두 통과

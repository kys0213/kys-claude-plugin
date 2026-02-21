# Round 2: 데몬 핵심 로직 개선

> **Type**: `refactor(autonomous)`
> **Priority**: P0–P1
> **Depends on**: Round 1 (데이터 정합성)

## 요약

데몬의 핵심 동작 신뢰성을 개선합니다. claude 세션 실행 중 전체 루프가 블록되는 문제, concurrency 설정이 무시되는 문제, PID 검증 부재 등을 수정합니다.

---

## 작업 항목

### #6 순차 처리 → 병렬화 (P0)

**파일**: `cli/src/daemon/mod.rs`

**현재 문제**:
```rust
loop {
    scanner::scan_all(&db, env).await?;      // ← claude -p 실행 시 수분~수십분 블록
    consumer::process_all(&db, env).await?;
    tokio::time::sleep(Duration::from_secs(10)).await;
}
```

scan과 consumer가 순차 실행되어 claude 세션 중 전체 기능이 정지됩니다.

**변경 방향**:
- scanner와 consumer를 독립 `tokio::spawn` 태스크로 분리
- 각 태스크가 자체 루프와 sleep 주기를 가짐
- rusqlite가 `!Sync`이므로 태스크 간 DB 공유 전략 필요:
  - 옵션 A: 각 태스크가 자체 `Database` 인스턴스를 열기 (SQLite WAL이 동시 접근 허용)
  - 옵션 B: `tokio::task::spawn_blocking` + `Arc<Mutex<Database>>`
  - **권장: 옵션 A** — 가장 단순하고 WAL 모드의 설계 의도에 부합

**예상 구조**:
```rust
let scan_handle = tokio::task::spawn_blocking(move || {
    let db = Database::open(&db_path)?;
    loop {
        scanner::scan_all_sync(&db, &env)?;
        std::thread::sleep(Duration::from_secs(scan_interval));
    }
});

let consumer_handle = tokio::task::spawn_blocking(move || {
    let db = Database::open(&db_path)?;
    loop {
        consumer::process_all_sync(&db, &env)?;
        std::thread::sleep(Duration::from_secs(10));
    }
});
```

**사이드이펙트**:
- scanner, consumer 모듈의 async fn → sync fn 전환 필요 (내부에서 tokio::process를 쓰므로 별도 runtime handle 필요하거나 `std::process::Command`로 전환)
- 또는 각 태스크에 자체 tokio runtime을 부여

---

### #3 concurrency 설정 반영 (P0)

**파일**: `cli/src/consumer/issue.rs`, `cli/src/consumer/pr.rs`, `cli/src/consumer/merge.rs`

**현재 문제**:
```rust
// issue.rs:17 — 하드코딩
let items = db.issue_find_pending(5)?;

// pr.rs:15 — 하드코딩
let items = db.pr_find_pending(5)?;

// merge.rs:14 — 하드코딩
let items = db.merge_find_pending(1)?;
```

`ConsumerConfig`의 `issue_concurrency`, `pr_concurrency`, `merge_concurrency` 값이 전혀 사용되지 않습니다.

**변경 방향**:
- `process_pending` 함수에 config를 전달하여 limit으로 사용
- `process_all`에서 YAML 설정을 로드하여 각 consumer에 전달

**예상 시그니처**:
```rust
pub async fn process_pending(db: &Database, env: &dyn Env, limit: u32) -> Result<()> {
    let items = db.issue_find_pending(limit)?;
    // ...
}
```

---

### #4 PID 프로세스 검증 강화 (P1)

**파일**: `cli/src/daemon/pid.rs`

**현재 문제**:
```rust
pub fn is_running(home: &Path) -> bool {
    if let Some(pid) = read_pid(home) {
        Path::new(&format!("/proc/{pid}")).exists()  // ← 다른 프로세스일 수 있음
    } else {
        false
    }
}
```

이전 데몬이 종료 후 같은 PID를 다른 프로세스가 재사용하면 false positive.

**변경 방향**:
```rust
pub fn is_running(home: &Path) -> bool {
    if let Some(pid) = read_pid(home) {
        match std::fs::read_to_string(format!("/proc/{pid}/cmdline")) {
            Ok(cmdline) => cmdline.contains("autodev"),
            Err(_) => false,
        }
    } else {
        false
    }
}
```

---

### #7 stop 후 종료 확인 (P1)

**파일**: `cli/src/daemon/mod.rs`

**현재 문제**:
```rust
pub fn stop(home: &Path) -> Result<()> {
    let pid = pid::read_pid(home).ok_or_else(|| ...)?;
    std::process::Command::new("kill").arg(pid.to_string()).status()?;
    pid::remove_pid(home);  // ← 프로세스가 아직 살아있을 수 있음
}
```

**변경 방향**:
```rust
pub fn stop(home: &Path) -> Result<()> {
    let pid = pid::read_pid(home).ok_or_else(|| ...)?;
    std::process::Command::new("kill").arg(pid.to_string()).status()?;

    // 최대 5초 대기
    for _ in 0..50 {
        if !pid::is_running(home) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    pid::remove_pid(home);
    Ok(())
}
```

---

### #5 PR scanner since 로직 수정 (P1)

**파일**: `cli/src/scanner/pulls.rs`

**현재 문제**:
```rust
let mut latest_updated = since;  // ← 비교 기준이자 갱신 대상

for pr in &prs {
    if let Some(ref s) = latest_updated {
        if pr.updated_at <= *s {
            continue;  // ← latest_updated가 루프 내에서 바뀌면 필터 조건도 변함
        }
    }
    // ...
    if latest_updated.as_ref().map_or(true, |l| pr.updated_at > *l) {
        latest_updated = Some(pr.updated_at.clone());  // ← 여기서 갱신
    }
}
```

1. `since` 파라미터를 GitHub API에 전달하지 않음 (issues.rs는 전달함)
2. 비교 기준(`since_threshold`)과 갱신 대상(`latest_updated`)이 같은 변수

**변경 방향**:
```rust
// API에 since 전달
if let Some(ref s) = since {
    args.push("-f".to_string());
    args.push(format!("since={s}"));
}

// 비교 기준과 추적 변수 분리
let since_threshold = since.clone();
let mut latest_updated = since;

for pr in &prs {
    if let Some(ref s) = since_threshold {
        if pr.updated_at <= *s {
            continue;
        }
    }
    // ...
}
```

---

## 테스트 계획

- [ ] 기존 42개 테스트 전체 통과
- [ ] `daemon_consumer_tests` — concurrency limit 반영 확인 테스트 추가
- [ ] `daemon_scan_tests` — PR scanner since 로직 검증 테스트 추가
- [ ] PID 검증 — cmdline 확인 유닛 테스트

## 영향 범위

| 파일 | 변경 유형 |
|------|----------|
| `daemon/mod.rs` | 구조 변경 (병렬화) |
| `daemon/pid.rs` | 로직 보강 |
| `consumer/mod.rs` | 시그니처 변경 |
| `consumer/issue.rs` | limit 파라미터화 |
| `consumer/pr.rs` | limit 파라미터화 |
| `consumer/merge.rs` | limit 파라미터화 |
| `scanner/pulls.rs` | since 로직 수정 |

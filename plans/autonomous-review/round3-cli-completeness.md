# Round 3: CLI/커맨드 완성도

> **Type**: `fix(autonomous)`
> **Priority**: P1–P2
> **Depends on**: Round 1, Round 2

## 요약

CLI 인터페이스의 미완성 부분과 코드 품질 이슈를 수정합니다. `/auto-setup` 워크플로우가 실제로 동작하도록 누락된 플래그를 구현하고, TUI의 설계 원칙 위반을 수정합니다.

---

## 작업 항목

### #12 `--config` 플래그 구현 (P1)

**파일**: `cli/src/main.rs`, `cli/src/client/mod.rs`

**현재 문제**:

`auto-setup.md` Step 8에서 다음 명령을 실행하도록 지시합니다:
```bash
autonomous repo add <url> --config '<json>'
```

그러나 `RepoAction::Add`는 `url` 하나만 받습니다:
```rust
RepoAction::Add {
    url: String,
    // --config 없음
}
```

**변경 방향**:
```rust
RepoAction::Add {
    /// 레포 URL
    url: String,
    /// 초기 설정 (JSON)
    #[arg(long)]
    config: Option<String>,
}
```

`client::repo_add`에서 config JSON을 파싱하여 YAML 설정 파일로 저장:
```rust
pub fn repo_add(db: &Database, env: &dyn Env, url: &str, config_json: Option<&str>) -> Result<()> {
    let name = extract_name(url);
    db.repo_add(url, &name)?;

    if let Some(json) = config_json {
        let config: WorkflowConfig = serde_json::from_str(json)?;
        let ws = config::workspaces_path(env).join(&name);
        std::fs::create_dir_all(&ws)?;
        let yaml = serde_yaml::to_string(&config)?;
        std::fs::write(ws.join(".develop-workflow.yaml"), yaml)?;
    }

    println!("registered: {name} ({url})");
    Ok(())
}
```

**사이드이펙트**:
- `repo_add` 시그니처 변경 → `main.rs` 호출부 수정 필요
- 기존 테스트 `cli_tests.rs`의 `repo_add_then_list` 등은 `--config` 없이 호출하므로 영향 없음

---

### #8 TUI raw SQL 제거 (P2)

**파일**: `cli/src/tui/views.rs`, `cli/src/queue/repository.rs`

**현재 문제**:

TUI에서 `db.conn()`으로 raw SQL을 직접 실행하는 곳이 3곳:

1. `render_header` (line 79–82):
   ```rust
   let repo_count: i64 = db.conn()
       .query_row("SELECT COUNT(*) FROM repositories", [], |row| row.get(0))
       .unwrap_or(0);
   ```

2. `render_repos_panel` (line 122–129):
   ```rust
   let repos: Vec<(String, bool)> = conn
       .prepare("SELECT name, enabled FROM repositories ORDER BY name")
       // ...
   ```

3. `render_queues_panel` (line 164–178):
   ```rust
   let count: i64 = conn
       .query_row(
           &format!("SELECT COUNT(*) FROM {table} WHERE status NOT IN ('done', 'failed')"),
           // ...
       )
   ```

Repository trait 패턴을 도입한 이유가 SQL 추상화인데 TUI에서는 이를 우회합니다.

**변경 방향**:

1. `repo_count()`는 이미 `RepoRepository` trait에 존재 → 그대로 사용
2. `repo_list()`는 이미 존재하지만 `RepoInfo`를 반환 → TUI에서 그대로 사용 가능
3. 큐 카운트용 메서드 추가 필요:

```rust
// repository.rs에 추가
pub trait QueueSummary {
    fn queue_active_counts(&self) -> Result<QueueCounts>;
}

pub struct QueueCounts {
    pub issue: i64,
    pub pr: i64,
    pub merge: i64,
}
```

**TUI 변경 후**:
```rust
fn render_header(f: &mut Frame, area: Rect, db: &Database) {
    let repo_count = db.repo_count().unwrap_or(0);
    // ...
}

fn render_repos_panel(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let repos = db.repo_list().unwrap_or_default();
    // ...
}
```

---

### #9 TUI 버전 하드코딩 (P2)

**파일**: `cli/src/tui/views.rs:87`

**현재 문제**:
```rust
Span::styled(" autodev v0.1.0 ", Style::default().add_modifier(Modifier::BOLD))
```

실제 Cargo.toml 버전은 `0.2.3`.

**변경 방향**:
```rust
Span::styled(
    format!(" autodev v{} ", env!("CARGO_PKG_VERSION")),
    Style::default().add_modifier(Modifier::BOLD),
)
```

---

### #11 compiler warnings 정리 (P2)

**현재 상태**: `cargo check`에서 15개 warning 발생

```
warning: unused import
warning: method `issue_count_active` is never used
warning: method `pr_count_active` is never used
warning: methods `merge_insert` and `merge_count_active` are never used
```

**변경 방향**:

| Warning | 조치 |
|---------|------|
| 미사용 trait 메서드 (`issue_count_active` 등) | TUI에서 raw SQL 대신 이 메서드를 사용하도록 변경 (#8과 연계) |
| 미사용 import | 제거 |
| `merge_insert` 미사용 | PR consumer에서 review 완료 후 merge_queue에 삽입하는 로직이 아직 미구현 → Round 2 #6 병렬화와 함께 구현하거나, `#[allow(dead_code)]` 주석과 함께 TODO 마킹 |

**주의**: #8 작업이 완료되면 일부 warning은 자동으로 해결됩니다. #8 이후에 남은 warning만 별도 처리.

---

## 테스트 계획

- [ ] 기존 테스트 전체 통과
- [ ] `cli_tests.rs` — `--config` 플래그 테스트 추가:
  - `repo add <url> --config '{"consumer":{"model":"opus"}}'` → config 파일 생성 확인
  - `repo config <name>` → 적용된 설정 확인
- [ ] warning 0개 확인: `cargo check 2>&1 | grep warning | wc -l` == 0

## 영향 범위

| 파일 | 변경 유형 |
|------|----------|
| `main.rs` | `--config` 인자 추가 |
| `client/mod.rs` | `repo_add` 시그니처 확장 |
| `tui/views.rs` | raw SQL → trait 메서드 교체 |
| `queue/repository.rs` | `QueueSummary` trait 추가 |
| 여러 파일 | 미사용 import 정리 |

# 토큰 사용량 추적 (Token Usage Tracking)

> autodev 데몬이 소비하는 Claude API 토큰을 세션 단위로 기록하고, CLI로 조회하는 기능

## CRUD Data Flow

```mermaid
flowchart TB
    subgraph CREATE ["C (Create) — Insert token_usage records"]
        direction TB
        D1[Daemon Main Loop] -->|task completes| TR[TaskResult]
        TR -->|logs: Vec‹NewConsumerLog›| LI[log_insert → consumer_logs]
        LI -->|log_id FK| UI[usage_insert → token_usage]
        UI --> DB_TU[(token_usage table)]

        note1["NewTokenUsage fields:
        log_id, repo_id, queue_type,
        queue_item_id, input_tokens,
        output_tokens, cache_write_tokens,
        cache_read_tokens, created_at"]
    end

    subgraph READ ["R (Read) — Query token usage"]
        direction TB
        CLI_U[autodev usage] --> R1{flags?}
        R1 -->|--repo --issue| UBI["usage_by_issue()
        GROUP BY queue_item_id, queue_type"]
        R1 -->|--repo / --since| US["usage_summary()
        Aggregates across tables"]

        US --> US1["Total: sessions, duration,
        input/output/cache tokens"]
        US --> US2["By queue_type:
        issue / pr / knowledge"]
        US --> US3["By repository:
        per-repo aggregation"]

        UBI --> UBI1["Per-issue breakdown:
        sessions, duration, tokens
        by queue_type"]
    end

    subgraph UPDATE ["U (Update) — No updates"]
        direction TB
        NO_UP["token_usage records are
        append-only (INSERT only).
        No UPDATE operations exist."]
    end

    subgraph DELETE ["D (Delete) — Cascade on repo removal"]
        direction TB
        CLI_RM[autodev repo remove] --> RM["repo_remove()"]
        RM --> TX["Transaction:"]
        TX --> D_TU["DELETE FROM token_usage
        WHERE repo_id = ?"]
        TX --> D_SC["DELETE FROM scan_cursors
        WHERE repo_id = ?"]
        TX --> D_CL["DELETE FROM consumer_logs
        WHERE repo_id = ?"]
        TX --> D_R["DELETE FROM repositories
        WHERE id = ?"]
    end

    style CREATE fill:#d4edda,stroke:#28a745
    style READ fill:#cce5ff,stroke:#0366d6
    style UPDATE fill:#fff3cd,stroke:#856404
    style DELETE fill:#f8d7da,stroke:#dc3545
```

## Schema

```mermaid
erDiagram
    repositories ||--o{ consumer_logs : "1:N"
    repositories ||--o{ token_usage : "1:N"
    consumer_logs ||--o| token_usage : "1:1"

    repositories {
        TEXT id PK
        TEXT url
        TEXT name
        BOOLEAN enabled
    }

    consumer_logs {
        TEXT id PK
        TEXT repo_id FK
        TEXT queue_type
        TEXT command
        TEXT stdout
        TEXT stderr
        INTEGER exit_code
        INTEGER duration_ms
        TEXT started_at
    }

    token_usage {
        INTEGER id PK "AUTOINCREMENT"
        TEXT log_id FK "→ consumer_logs.id"
        TEXT repo_id FK "→ repositories.id"
        TEXT queue_type
        TEXT queue_item_id
        INTEGER input_tokens
        INTEGER output_tokens
        INTEGER cache_write_tokens
        INTEGER cache_read_tokens
        TEXT created_at
    }
```

## CRUD 요약

| Op | Method | Trigger | 비고 |
|---|---|---|---|
| **C** | `usage_insert()` | 데몬 루프 — consumer task 완료 후 Claude API 응답의 토큰 카운트 저장 | Append-only, `consumer_logs`와 1:1 (log_id FK) |
| **R** | `usage_summary()` | `autodev usage [--repo] [--since]` | `token_usage` + `consumer_logs` + `repositories` JOIN, queue_type/repo별 집계 |
| **R** | `usage_by_issue()` | `autodev usage --repo X --issue N` | repo + issue 필터, queue_type별 그룹핑 |
| **U** | _(없음)_ | — | 레코드는 삽입 후 불변 |
| **D** | `repo_remove()` | `autodev repo remove <name>` | 트랜잭션 내 cascade: token_usage → scan_cursors → consumer_logs → repositories |

## CLI 사용법

```bash
# 전체 요약
autodev usage

# 특정 레포 필터
autodev usage --repo org/repo-name

# 기간 필터
autodev usage --since 2026-03-01

# 특정 이슈의 토큰 사용량
autodev usage --repo org/repo-name --issue 42
```

## 입력 검증

| 파라미터 | 검증 | 에러 메시지 |
|---|---|---|
| `--since` | `chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")` | `invalid --since format: expected YYYY-MM-DD` |
| `--repo` | `chars().all(alphanumeric \| / \| - \| _ \| .)` | `invalid repo name: {name}` |

## 관련 파일

| 파일 | 역할 |
|---|---|
| `cli/src/queue/schema.rs` | `token_usage` 테이블 DDL |
| `cli/src/domain/models.rs` | `NewTokenUsage`, `UsageSummary`, `UsageByQueueType`, `UsageByRepo`, `UsageByIssue` |
| `cli/src/domain/repository.rs` | `TokenUsageRepository` trait |
| `cli/src/queue/repository.rs` | SQLite 구현 (insert, summary, by_issue, cascade delete) |
| `cli/src/client/mod.rs` | `usage()` 리포트 포맷팅 |
| `cli/src/main.rs` | `Commands::Usage` CLI 진입점 |

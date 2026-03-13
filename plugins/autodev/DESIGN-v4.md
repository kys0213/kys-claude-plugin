# DESIGN v4: Tick Supervisor + Spec Gap Analyzer

> **Date**: 2026-03-13
> **Revision**: v4.0 — Supervisor Agent, Spec Gap Analyzer (Scheduled Task)
> **Base**: [DESIGN-v3.md](./DESIGN-v3.md) — Issue-PR Workflow (Auto-Approve + Label-Positive)
> **Related Issue**: #235 (Phase 0)

---

## 1. 변경 동기

### v3의 한계

```
v3: task 완료 → 기계적 상태 전이 (confidence threshold, FIFO, 무조건 re-review)
                      ↑
                  맥락을 고려한 판단이 없음
```

- **기계적 auto-approve**: confidence >= 0.8이면 auth 모듈 변경이든 README 수정이든 동일하게 자동 승인
- **맹목적 re-review 루프**: ImproveTask 완료 후 무조건 Pending으로 복귀 — 개선이 충분한지 판단 없음
- **실패 시 일괄 포기**: 구현 실패 → `impl-failed` 라벨로 종료 — 재시도 가능한 오류인지 구분 없음
- **이슈 간 맥락 단절**: 각 task가 독립적으로 실행되어, 관련 이슈 간 순서·충돌을 고려하지 못함
- **수동 이슈 등록 의존**: 사람이 직접 이슈를 만들고 `autodev:analyze` 라벨을 추가해야만 워크플로우 시작

### v4 목표

1. **Tick Supervisor**: 레포별 틱마다 누적된 작업 결과를 LLM이 검토하여 다음 상태를 판단
2. **Spec Gap Analyzer**: Claude Scheduled Task로 스펙 문서 vs 구현 상태를 비교하여 자동으로 이슈 등록

### v3 → v4 주요 차이

| | v3 | v4 |
|---|---|---|
| 상태 전이 판단 | 기계적 (threshold, FIFO) | Supervisor Agent가 맥락 기반 판단 |
| auto-approve 기준 | confidence 값만 | confidence + 영향 범위 + 파일 위치 종합 판단 |
| 실패 처리 | 일괄 종료 (impl-failed) | 재시도 가능 여부 판단 후 retry 또는 hold |
| 이슈 생성 | 수동 (HITL only) | Scheduled Task가 스펙 갭을 분석하여 자동 생성 |
| 이슈 간 관계 | 고려 안 함 | Supervisor가 관련 이슈 순서/충돌 감지 |

---

## 2. Tick Supervisor Agent

### 2.1 개요

레포별로 매 tick마다 누적된 task 결과를 검토하는 경량 LLM Agent.
기존 task 에이전트(AnalyzeTask, ReviewTask 등)는 그대로 유지하고, 그 결과물을 Supervisor가 한 번 검토하여 다음 상태로의 전이를 결정한다.

```
v3:  task 완료 → queue_ops 즉시 적용 → 상태 전이
v4:  task 완료 → PendingTransition 버퍼 → [Supervisor 판단] → 승인된 전이만 적용
```

### 2.2 아키텍처 위치

```
┌─────────────────────────────────────────────────────────────────────┐
│                          Daemon Event Loop                            │
│                                                                       │
│  task 완료 → TaskResult                                              │
│       │                                                               │
│       ▼                                                               │
│  ┌───────────────────────┐                                           │
│  │ PendingTransitionBuffer│  레포별로 완료된 TaskResult 누적           │
│  └───────────┬───────────┘                                           │
│              │ on tick                                                 │
│              ▼                                                        │
│  ┌───────────────────────┐                                           │
│  │   Supervisor Agent    │  레포별 누적 결과 + 현재 큐 상태 →          │
│  │   (per-repo, LLM)     │  전이 판정 (proceed / hold / retry)       │
│  └───────────┬───────────┘                                           │
│              │                                                        │
│         ┌────┴────┐                                                   │
│         ▼         ▼                                                   │
│    Approved    Held                                                   │
│    transitions transitions                                            │
│         │         │                                                   │
│         ▼         ▼                                                   │
│   manager.apply() → HITL 알림 (GitHub comment / label)               │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

### 2.3 Supervisor 입력 (SupervisorContext)

Supervisor는 레포별로 다음 컨텍스트를 받는다:

```rust
pub struct SupervisorContext {
    /// 레포 식별자
    pub repo_name: String,
    /// 이번 tick에 완료된 task 결과들
    pub pending_transitions: Vec<PendingTransition>,
    /// 현재 큐 상태 요약 (issue/pr 각 phase별 항목 수 + 내용 요약)
    pub queue_snapshot: QueueSnapshot,
    /// 현재 HITL 대기 중인 항목들
    pub hitl_items: Vec<HitlItem>,
    /// 레포별 설정 (auto_approve, confidence_threshold 등)
    pub repo_config: RepoConfigSummary,
}

pub struct PendingTransition {
    pub work_id: String,
    pub task_type: TaskType,        // Analyze, Implement, Review, Improve, Extract
    pub result_summary: String,     // task 결과 요약 (verdict, confidence 등)
    pub proposed_ops: Vec<QueueOp>, // task가 제안한 queue 연산
    pub affected_files: Vec<String>,// 변경된 파일 목록 (implement/improve 시)
    pub labels: Vec<String>,        // 현재 라벨 상태
}
```

### 2.4 Supervisor 출력 (SupervisorVerdict)

```rust
pub struct SupervisorVerdict {
    pub decisions: Vec<TransitionDecision>,
}

pub struct TransitionDecision {
    pub work_id: String,
    pub action: SupervisorAction,
    pub reason: String,
}

pub enum SupervisorAction {
    /// 제안된 queue_ops를 그대로 적용
    Proceed,
    /// HITL 필요 — 알림 전송 후 대기
    Hold { notification: String },
    /// 재시도 — 같은 task를 다시 큐에 넣기
    Retry { hint: String },
    /// 순서 조정 — 다른 작업이 먼저 완료되어야 함
    Defer { blocked_by: String },
}
```

### 2.5 Supervisor 판단 기준

Supervisor는 다음 기준을 종합적으로 판단한다:

| 판단 영역 | Proceed 조건 | Hold 조건 |
|-----------|-------------|-----------|
| **분석 완료** | confidence 높음 + 영향 범위가 작음 + 단일 모듈 | 핵심 모듈(auth, payment, infra) 변경 또는 breaking change 가능성 |
| **구현 완료** | 테스트 통과 + 단일 파일/모듈 변경 | 다수 파일 변경 또는 다른 open 이슈와 파일 충돌 |
| **리뷰 결과** | approve + 변경 규모 작음 | approve이지만 변경 규모가 크거나, request_changes 반복 3회 이상 |
| **개선 완료** | diff가 리뷰 피드백을 정확히 반영 | 피드백과 무관한 변경이 포함됨 |
| **구현 실패** | 컴파일 에러, lint 실패 등 재시도 가능 오류 | 설계 수준 문제, 요구사항 불명확 |

### 2.6 Supervisor 판단 기준의 인터페이스화

Supervisor가 참조하는 판단 기준은 **설정으로 커스터마이즈 가능**해야 한다.
LLM 프롬프트에 주입되는 정책이므로, 레포별로 다른 기준을 적용할 수 있다.

```yaml
# .autodev.yaml
supervisor:
  enabled: true
  model: haiku                    # 경량 모델 (분류 작업)
  policy: default                 # 판단 정책 (default / strict / permissive)
  critical_paths:                 # Hold 강제 경로 패턴
    - "src/auth/**"
    - "src/payment/**"
    - "migrations/**"
  auto_retry_patterns:            # 자동 재시도할 에러 패턴
    - "cargo build failed"
    - "npm test.*timeout"
  max_retries: 2                  # task별 최대 재시도 횟수
```

### 2.7 Supervisor Prompt 구조

```
You are a Supervisor Agent for repository '{repo_name}'.
Review the following completed tasks and decide whether each should proceed
to the next state or requires human review.

## Policy
{policy_description}

## Critical Paths (always require HITL)
{critical_paths}

## Current Queue State
{queue_snapshot}

## Pending Transitions
{pending_transitions_json}

## Instructions
For each transition, respond with a JSON array:
[
  {
    "work_id": "...",
    "action": "proceed" | "hold" | "retry" | "defer",
    "reason": "...",
    "notification": "..."  // only for hold
  }
]
```

### 2.8 Fallback 전략

Supervisor Agent 호출이 실패하거나 타임아웃 시, **v3 기계적 동작으로 폴백**한다.

```
Supervisor 성공 → 판단에 따라 전이
Supervisor 실패 → v3 방식으로 즉시 적용 (기존 동작 보장)
Supervisor 비활성 (enabled: false) → v3 방식 (opt-in 구조)
```

이는 Supervisor를 **점진적으로 도입**할 수 있게 한다. 기존 레포는 변경 없이 동작하고, 신규 레포부터 활성화할 수 있다.

### 2.9 Daemon Event Loop 변경

```rust
impl Daemon {
    pub async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                // Task 완료 → 버퍼에 누적 (즉시 적용하지 않음)
                Some(result) = join_set.join_next() => {
                    self.inflight.release(&result.repo_name);
                    self.transition_buffer.push(result);
                    // v3: self.manager.apply(result);  ← 제거
                    // v4: 버퍼에 쌓고, tick에서 Supervisor가 판단
                }

                // Tick → Supervisor 실행 후 승인된 전이만 적용
                _ = tick.tick() => {
                    // 1. 기존 poll
                    self.manager.tick().await;

                    // 2. Supervisor 판단 (NEW)
                    let verdicts = self.supervisor.review(
                        &self.transition_buffer,
                        &self.manager,
                    ).await;

                    // 3. 승인된 전이만 적용
                    for (result, decision) in verdicts.approved() {
                        self.manager.apply(result);
                    }

                    // 4. Hold 항목은 HITL 알림
                    for (result, decision) in verdicts.held() {
                        self.notifier.notify_hitl(&decision).await;
                    }

                    // 5. Retry 항목은 큐에 재삽입
                    for (result, decision) in verdicts.retries() {
                        self.manager.requeue(result, &decision.hint);
                    }

                    // 6. 버퍼 정리
                    self.transition_buffer.clear();

                    // 7. 새 task spawn
                    self.try_spawn(&mut join_set).await;
                }
            }
        }
    }
}
```

### 2.10 상태 전이 변경 요약

| 전이 지점 | v3 | v4 |
|-----------|-----|-----|
| 분석 완료 → 구현 | confidence >= threshold → auto-approve | Supervisor가 confidence + 영향 범위 + critical path 종합 판단 |
| 구현 완료 → PR 리뷰 | 무조건 PR queue push | Supervisor가 다른 이슈와의 파일 충돌 검사 후 proceed/defer |
| 리뷰 approve → done | 무조건 done 라벨 | Supervisor가 변경 규모 확인 후 proceed/hold |
| 리뷰 request_changes → improve | 무조건 improve 루프 | Supervisor가 반복 횟수 + 피드백 성격 판단 → retry/hold |
| 구현 실패 | 무조건 impl-failed | Supervisor가 에러 분류 → retry 가능 시 재시도 |
| improve 완료 → re-review | 무조건 Pending 복귀 | Supervisor가 개선 충분성 판단 → proceed/hold |

---

## 3. Spec Gap Analyzer (Scheduled Task)

### 3.1 개요

Claude Code의 **Scheduled Task** 기능을 활용하여, 레포별 스펙 문서와 실제 구현 상태를 주기적으로 비교한다.
발견된 갭에 대해 GitHub 이슈를 자동 생성하여 `autodev:analyze` 라벨을 추가하면, 기존 autodev 파이프라인이 이를 자동으로 처리한다.

```
Scheduled Task (주기적 실행)
     │
     ▼
Spec Gap Analyzer
     │
     ├─ 스펙 문서 수집 (DESIGN-*.md, ARCHITECTURE.md, etc.)
     ├─ 코드베이스 구조 파악
     ├─ 갭 분석 (스펙에 있지만 미구현 / 부분 구현 / 설계 불일치)
     ├─ 기존 이슈 중복 확인
     │
     ▼
GitHub Issue 자동 생성 (autodev:analyze 라벨)
     │
     ▼
기존 autodev 파이프라인이 처리
```

### 3.2 Claude Scheduled Task 활용

Desktop 또는 CLI의 Scheduled Task로 등록하여 주기적으로 실행한다.

```
~/.claude/scheduled-tasks/spec-gap-analyzer/SKILL.md
```

또는 CLI에서:

```bash
/loop 24h /auto spec-gap --repo kys0213/my-project
```

### 3.3 분석 프로세스

#### Phase 1: 스펙 수집

레포 설정에 정의된 스펙 문서 경로에서 설계 문서를 수집한다.

```yaml
# .autodev.yaml
spec_gap:
  enabled: true
  schedule: "0 9 * * 1"              # 매주 월요일 09:00
  spec_paths:                         # 스펙 문서 경로 패턴
    - "DESIGN*.md"
    - "docs/architecture/**/*.md"
    - "specs/**/*.md"
  exclude_paths:                      # 제외 경로
    - "archive/**"
    - "**/CHANGELOG.md"
  max_issues_per_run: 5               # 1회 실행당 최대 이슈 생성 수
  auto_label: true                    # 생성된 이슈에 autodev:analyze 자동 부착
  severity_threshold: medium          # 이슈 생성 최소 심각도 (low/medium/high)
```

#### Phase 2: 코드베이스 매핑

스펙에서 언급된 모듈/컴포넌트/기능을 코드베이스의 실제 구현과 매핑한다.

```
스펙 문서의 기능 목록:
  ├─ Feature A: "사용자 인증 OAuth2 지원"
  ├─ Feature B: "실시간 알림 WebSocket"
  └─ Feature C: "다국어 지원 i18n"

코드베이스 스캔:
  ├─ src/auth/   → OAuth2 관련 코드 존재 여부
  ├─ src/notify/ → WebSocket 관련 코드 존재 여부
  └─ src/i18n/   → 국제화 관련 코드 존재 여부
```

#### Phase 3: 갭 분류

| 갭 유형 | 설명 | 심각도 기준 |
|---------|------|-----------|
| **Missing** | 스펙에 있지만 구현이 전혀 없음 | High |
| **Partial** | 구현이 시작되었지만 스펙 대비 불완전 | Medium |
| **Divergent** | 구현이 스펙과 다른 방향으로 진행됨 | High |
| **Stale** | 스펙은 업데이트되었지만 코드가 구버전 | Medium |
| **Undocumented** | 코드에는 있지만 스펙에 없음 (역방향 갭) | Low |

#### Phase 4: 중복 확인

기존 open 이슈와의 중복을 방지한다.

```
1. 레포의 open 이슈 목록 조회
2. 각 갭에 대해 유사한 제목/내용의 이슈가 있는지 확인
3. 중복 감지 시 해당 갭은 스킵
4. 기존 이슈에 추가 정보가 있으면 코멘트로 보강
```

#### Phase 5: 이슈 생성

```markdown
## Spec Gap: {gap_title}

### Gap Type
{Missing | Partial | Divergent | Stale}

### Spec Reference
- **문서**: `{spec_file_path}`
- **섹션**: {section_name}
- **내용**: {relevant_spec_excerpt}

### Current State
{현재 코드베이스의 관련 구현 상태 설명}

### Expected vs Actual
| 항목 | 스펙 (Expected) | 코드 (Actual) |
|------|-----------------|---------------|
| ... | ... | ... |

### Suggested Action
{구현 방향 제안}

---
*Generated by autodev spec-gap-analyzer*
```

### 3.4 Spec Gap Analyzer Prompt 구조

```
You are a Spec Gap Analyzer for repository '{repo_name}'.

## Task
Compare the specification documents against the actual codebase implementation.
Identify gaps where the spec describes functionality that is missing, partially
implemented, or diverges from the implementation.

## Spec Documents
{spec_contents}

## Codebase Structure
{file_tree_summary}

## Existing Open Issues
{existing_issues_titles_and_bodies}

## Instructions
1. For each spec section, determine if the described functionality exists in the codebase
2. Classify gaps: Missing, Partial, Divergent, Stale, Undocumented
3. Skip gaps that overlap with existing open issues
4. Return a JSON array of gaps:

[
  {
    "title": "feat(scope): description",
    "gap_type": "Missing|Partial|Divergent|Stale",
    "severity": "low|medium|high",
    "spec_file": "path/to/spec.md",
    "spec_section": "Section Name",
    "spec_excerpt": "relevant text from spec",
    "current_state": "description of current code state",
    "suggested_action": "what should be done",
    "affected_paths": ["src/module/..."]
  }
]
```

### 3.5 End-to-End Flow

```
┌─────────────────────────────────────────────────────────────┐
│  Claude Scheduled Task (주기적 실행)                           │
│  trigger: cron schedule (e.g., weekly)                        │
│                                                               │
│  1. 등록된 레포 목록 조회                                      │
│  2. 레포별 spec_gap 설정 확인                                  │
│     └─ enabled: false → skip                                 │
│  3. 스펙 문서 수집 (spec_paths 패턴)                           │
│  4. 코드베이스 구조 파악                                       │
│  5. 기존 open 이슈 조회 (중복 방지)                            │
│  6. LLM 분석: 스펙 vs 코드 갭 식별                            │
│  7. severity >= threshold인 갭만 필터                         │
│  8. max_issues_per_run 제한 적용                              │
│  9. GitHub 이슈 생성                                          │
│     └─ auto_label: true → autodev:analyze 라벨 추가          │
│                                                               │
└─────────────────────────────────┬─────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────┐
│  기존 autodev 파이프라인                                       │
│                                                               │
│  scan → autodev:analyze 감지 → AnalyzeTask → ...             │
│  (v3 플로우 그대로 + v4 Supervisor 판단 적용)                  │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. 컴포넌트 설계

### 4.1 신규 Trait

```rust
/// Supervisor: 레포별 tick에서 누적된 전이를 검토
#[async_trait]
pub trait Supervisor: Send + Sync {
    async fn review(
        &self,
        transitions: &[PendingTransition],
        queue_snapshot: &QueueSnapshot,
        config: &SupervisorConfig,
    ) -> SupervisorVerdict;
}
```

### 4.2 구현체

```rust
/// LLM 기반 Supervisor
pub struct LlmSupervisor {
    agent: Arc<dyn Agent>,
}

/// Fallback: v3 호환 (모든 전이를 Proceed)
pub struct PassthroughSupervisor;

impl Supervisor for PassthroughSupervisor {
    async fn review(&self, transitions: &[PendingTransition], ..) -> SupervisorVerdict {
        // 모든 전이를 무조건 승인 (v3 동작)
        SupervisorVerdict {
            decisions: transitions.iter().map(|t| TransitionDecision {
                work_id: t.work_id.clone(),
                action: SupervisorAction::Proceed,
                reason: "passthrough".into(),
            }).collect()
        }
    }
}
```

### 4.3 전이 버퍼

```rust
/// 틱 사이에 완료된 task 결과를 레포별로 버퍼링
pub struct PendingTransitionBuffer {
    /// repo_name → Vec<(TaskResult, PendingTransition)>
    buffer: HashMap<String, Vec<(TaskResult, PendingTransition)>>,
}

impl PendingTransitionBuffer {
    pub fn push(&mut self, result: TaskResult, transition: PendingTransition) { .. }
    pub fn drain_by_repo(&mut self) -> Vec<(String, Vec<(TaskResult, PendingTransition)>)> { .. }
    pub fn is_empty(&self) -> bool { .. }
}
```

### 4.4 HITL 알림

Supervisor가 `Hold` 판정을 내리면 HITL 알림을 전송한다.

```rust
pub trait HitlNotifier: Send + Sync {
    async fn notify(&self, repo_name: &str, decision: &TransitionDecision) -> Result<()>;
}

/// GitHub 이슈/PR에 코멘트로 알림
pub struct GitHubHitlNotifier {
    gh: Arc<dyn Gh>,
}
```

알림 형태:

```markdown
## 🔍 Supervisor Review: HITL Required

**Task**: {work_id}
**Reason**: {decision.reason}

{decision.notification}

> To proceed, add the `autodev:approved-analysis` label.
> To skip, add the `autodev:skip` label.
```

### 4.5 모듈 구조 (v4 추가분)

```
cli/src/
├── daemon/
│   ├── mod.rs                    // transition_buffer 추가
│   ├── supervisor.rs             // Supervisor trait + LlmSupervisor + PassthroughSupervisor (NEW)
│   └── transition_buffer.rs      // PendingTransitionBuffer (NEW)
├── components/
│   └── hitl_notifier.rs          // HitlNotifier trait + GitHubHitlNotifier (NEW)
├── config/
│   └── models.rs                 // SupervisorConfig, SpecGapConfig 추가
└── ...
```

---

## 5. Config 변경 (CONFIG-SCHEMA v3)

### 5.1 supervisor 섹션 추가

```yaml
supervisor:
  enabled: false                    # Supervisor 활성화 (기본: false, opt-in)
  model: haiku                      # LLM 모델 (경량 분류 작업)
  timeout_secs: 30                  # Supervisor 호출 타임아웃
  policy: default                   # 판단 정책
  critical_paths:                   # Hold 강제 경로 패턴
    - "src/auth/**"
    - "migrations/**"
  auto_retry_patterns:              # 자동 재시도 에러 패턴
    - "cargo build failed"
    - "compilation error"
  max_retries: 2                    # task별 최대 재시도 횟수
  fallback: passthrough             # 실패 시 폴백 (passthrough = v3 동작)
```

### 5.2 spec_gap 섹션 추가

```yaml
spec_gap:
  enabled: false                    # Spec Gap Analyzer 활성화
  schedule: "0 9 * * 1"            # cron 스케줄 (기본: 매주 월요일 09:00)
  spec_paths:                       # 스펙 문서 경로 패턴
    - "DESIGN*.md"
    - "docs/**/*.md"
  exclude_paths:                    # 제외 경로
    - "archive/**"
  max_issues_per_run: 5             # 1회 실행당 최대 이슈 생성 수
  auto_label: true                  # autodev:analyze 자동 부착
  severity_threshold: medium        # 최소 심각도
  model: sonnet                     # 분석 모델 (스펙 이해에 높은 추론력 필요)
```

### 5.3 전체 스키마 (v4)

```yaml
daemon:                             # v3 유지
  tick_interval_secs: 10
  max_concurrent_tasks: 3
  # ...

sources:                            # v3 유지
  github:
    # ...

workflows:                          # v3 유지
  analyze:
    # ...
  implement:
    # ...
  review:
    # ...

supervisor:                         # v4 NEW
  enabled: false
  model: haiku
  # ...

spec_gap:                           # v4 NEW
  enabled: false
  schedule: "0 9 * * 1"
  # ...
```

---

## 6. 상태 전이 변경

### 6.1 Issue 전이 (v4)

```
                         ┌─────────────────┐
                    HITL │  사람이 라벨 추가  │
                         │  또는             │
                         │  Spec Gap Analyzer│  ← NEW: 자동 이슈 생성
                         └────────┬────────┘
                                  │
                          autodev:analyze
                                  │
                    ──────────────┼──────────────
                                  │
                         ┌────────▼────────┐
                  daemon │  scanner 감지    │
                         └────────┬────────┘
                                  │
                           autodev:wip
                                  │
                         ┌────────▼────────┐
                  daemon │  AnalyzeTask     │
                         └────────┬────────┘
                                  │
                         ┌────────▼────────┐
                         │ PendingTransition│  ← NEW: 버퍼에 누적
                         └────────┬────────┘
                                  │
                    ┌─────────────▼─────────────┐
                    │     Supervisor Agent       │  ← NEW
                    │                            │
                    │  confidence + 영향 범위 +   │
                    │  critical path 종합 판단    │
                    └─────────────┬─────────────┘
                                  │
                     ┌────────────┼────────────┐
                     │            │            │
                  Proceed       Hold         Retry
                     │            │            │
                     ▼            ▼            ▼
              (v3 전이 적용)  HITL 알림     큐 재삽입
```

### 6.2 HITL 요약 (v4)

| 전이 | v3 | v4 |
|------|-----|-----|
| (없음) → `analyze` | HITL only | HITL **또는** Spec Gap Analyzer |
| `analyzed` → `approved-analysis` | threshold 기반 auto/HITL | **Supervisor가 판단** (Proceed/Hold) |
| 구현 실패 → retry/hold | 일괄 impl-failed | **Supervisor가 에러 분류** (Retry/Hold) |
| improve → re-review | 무조건 Pending | **Supervisor가 충분성 판단** (Proceed/Hold) |
| approve → done | 무조건 done | **Supervisor가 규모 확인** (Proceed/Hold) |

---

## 7. 구현 단계

### Phase 0: Supervisor 기반 인프라 (이 문서)

```
0-1. Supervisor trait + PassthroughSupervisor (v3 호환)
0-2. PendingTransitionBuffer
0-3. Daemon event loop 변경 (buffer → supervisor → apply)
0-4. HitlNotifier trait + GitHubHitlNotifier
0-5. Config: supervisor 섹션 추가
0-6. LlmSupervisor 구현
0-7. 테스트: Supervisor 판단 시나리오
```

### Phase 0.5: Spec Gap Analyzer

```
0.5-1. SpecGapAnalyzer 컴포넌트 설계
0.5-2. Config: spec_gap 섹션 추가
0.5-3. Scheduled Task skill 작성 (SKILL.md)
0.5-4. 갭 분류 로직 + 중복 확인
0.5-5. GitHub 이슈 생성 + autodev:analyze 라벨
0.5-6. 테스트: 갭 분석 시나리오
```

### Phase 1~4: #235 기존 로드맵

```
Phase 1: Issue Orchestrator (우선순위, 의존성 분석)
Phase 2: Execution Planner (병렬/순차 전략)
Phase 3: Agent Coordinator (병렬 에이전트 관리)
Phase 4: Review Pipeline (#232의 /simplify 포함)
```

> Phase 0의 Supervisor가 Phase 1~4의 **판단 인프라** 역할을 한다.
> Supervisor의 판단 기준을 점진적으로 확장하여 오케스트레이션 판단도 포함할 수 있다.

---

## 8. 테스트 전략

### Supervisor 단위 테스트

```rust
#[tokio::test]
async fn supervisor_proceeds_low_impact_analysis() {
    // Given: confidence 0.9, 단일 모듈 변경, critical path 아님
    let supervisor = LlmSupervisor::new(mock_agent_returning(proceed_verdict()));
    let transition = analysis_transition(confidence: 0.9, files: vec!["src/utils.rs"]);

    // When
    let verdict = supervisor.review(&[transition], &empty_queue(), &default_config()).await;

    // Then: Proceed
    assert_eq!(verdict.decisions[0].action, SupervisorAction::Proceed);
}

#[tokio::test]
async fn supervisor_holds_critical_path_change() {
    // Given: confidence 0.9이지만 auth 모듈 변경
    let supervisor = LlmSupervisor::new(mock_agent_returning(hold_verdict()));
    let transition = analysis_transition(confidence: 0.9, files: vec!["src/auth/login.rs"]);
    let config = config_with_critical_paths(vec!["src/auth/**"]);

    // When
    let verdict = supervisor.review(&[transition], &empty_queue(), &config).await;

    // Then: Hold
    assert!(matches!(verdict.decisions[0].action, SupervisorAction::Hold { .. }));
}

#[tokio::test]
async fn supervisor_retries_compilation_error() {
    // Given: 구현 실패, 에러 메시지가 컴파일 에러
    let supervisor = LlmSupervisor::new(mock_agent_returning(retry_verdict()));
    let transition = impl_failed_transition(error: "cargo build failed: missing semicolon");
    let config = config_with_retry_patterns(vec!["cargo build failed"]);

    // When
    let verdict = supervisor.review(&[transition], &empty_queue(), &config).await;

    // Then: Retry
    assert!(matches!(verdict.decisions[0].action, SupervisorAction::Retry { .. }));
}

#[tokio::test]
async fn passthrough_supervisor_always_proceeds() {
    // Given: PassthroughSupervisor (v3 호환)
    let supervisor = PassthroughSupervisor;
    let transitions = vec![analysis_transition(..), impl_transition(..)];

    // When
    let verdict = supervisor.review(&transitions, &empty_queue(), &default_config()).await;

    // Then: 모든 전이가 Proceed
    assert!(verdict.decisions.iter().all(|d| d.action == SupervisorAction::Proceed));
}

#[tokio::test]
async fn supervisor_fallback_on_agent_failure() {
    // Given: Agent가 에러 반환
    let supervisor = LlmSupervisor::new(mock_agent_failing());

    // When
    let verdict = supervisor.review(&[transition], &empty_queue(), &default_config()).await;

    // Then: Passthrough 폴백 (모두 Proceed)
    assert!(verdict.decisions.iter().all(|d| d.action == SupervisorAction::Proceed));
}
```

### PendingTransitionBuffer 테스트

```rust
#[test]
fn buffer_groups_by_repo() {
    let mut buffer = PendingTransitionBuffer::new();
    buffer.push(result("repo-a", "task-1"), transition("task-1"));
    buffer.push(result("repo-b", "task-2"), transition("task-2"));
    buffer.push(result("repo-a", "task-3"), transition("task-3"));

    let grouped = buffer.drain_by_repo();
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped["repo-a"].len(), 2);
    assert_eq!(grouped["repo-b"].len(), 1);
}
```

### Daemon 통합 (Supervisor 주입)

```rust
#[tokio::test]
async fn daemon_applies_only_approved_transitions() {
    // Given: Supervisor가 task-a는 Proceed, task-b는 Hold
    let supervisor = MockSupervisor::new()
        .on("task-a", SupervisorAction::Proceed)
        .on("task-b", SupervisorAction::Hold { notification: "review needed".into() });

    let manager = MockTaskManager::new()
        .on_drain(vec![mock_task("task-a"), mock_task("task-b")]);
    let runner = MockTaskRunner::new()
        .returning(completed("task-a"))
        .returning(completed("task-b"));

    let mut daemon = Daemon::new(manager, runner, supervisor, ..);

    // When
    daemon.run_one_tick().await;

    // Then: task-a만 apply됨, task-b는 HITL 알림
    assert_eq!(manager.apply_count(), 1);
    assert_eq!(notifier.notify_count(), 1);
}
```

---

## 9. Scope 외

다음은 v4 범위에 포함되지 않으며, 후속 Phase(#235 Phase 1~4)에서 다룬다:

- **병렬 에이전트 실행**: 여러 이슈를 동시에 구현하는 Agent Coordinator (Phase 3)
- **의존성 그래프**: 이슈 간 선후관계 파악 및 실행 순서 결정 (Phase 1)
- **Multi-LLM 리뷰 파이프라인**: Claude + Codex + Gemini 병렬 리뷰 (Phase 4)
- **Jira/Linear 연동**: 외부 프로젝트 관리 도구와의 통합
- **PR merge 자동화**: Supervisor가 merge까지 판단하는 것은 별도 결정 사항

---

## 10. 관계 범례

```
  ──▶   소유 (owns / has-a)
  ─ ─▶  생성 (creates)
  ──▷   구현 (implements trait)
  ◇───▶ 선택적 의존 (optional dependency)
  ★     v4 신규 컴포넌트
```

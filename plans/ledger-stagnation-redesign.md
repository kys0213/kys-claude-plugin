# Ledger 기반 Stagnation 재설계

> **Epic**: `epic/c10-ledger-stagnation`
> **Scope**: autopilot 의 stagnation 감지를 GitHub issue body 기반에서 ledger 기반으로 재구축
> **Status**: spec draft (구현 미착수)

---

## 1. Background

autopilot 모드는 사람 개입 없이 task 를 dispatch 하므로, 같은 issue / 같은 영역을 retry 반복하다가 **infinite loop** 에 빠질 위험이 상존한다.

기존에는 `cli/src/cmd/check/stagnation.rs` 가 simhash 기반으로 GitHub issue body 를 비교하며 stagnation 을 감지했으나, autopilot 흐름이 ledger-only 로 전환되면서 (`commands/gap-watch.md:233` 메모) 잠정 비활성화되었다. 한편 `cli/src/cmd/issue.rs:553` 의 persona 추천 로직 (filter-comments) 은 GitHub issue 컨텍스트 한정으로 여전히 작동한다.

본 epic 은 stagnation 감지를 **ledger 기반으로 재구축**한다. 같은 simhash / 같은 path 영역으로 N 번 반복 시도된 task 그룹을 자동 감지하고, worker 가 task 를 claim 하기 직전에 **새로운 방향으로 강제 redirect** 한다.

핵심 변화는 다음과 같다.

- 비교 대상이 **GitHub issue body → ledger task (DB)** 로 이동
- 비교 차원이 **simhash 단일 → simhash + 영향 path Jaccard hybrid** 로 확장
- 트리거가 **이슈 등록 시 → task claim 직전 PreToolUse hook** 으로 이동
- 결과가 **이슈 close → worker prompt redirect + 조건부 escalate** 로 변화

## 2. 철학 / 비기능 요구사항

| 항목 | 정책 |
|------|------|
| Stagnation 정의 | "유사한 작업 N 개" — 정확한 fingerprint 중복이 아니라 **유사도 기반** 그룹화. 같은 영역을 반복 시도하는 패턴을 잡는다. |
| 역할 | autopilot 의 핵심 안전핀 — 사람 개입 없이 retry loop 가 무한 반복하지 않도록 자동 escape hatch. |
| 대응 | 감지 시 단순 skip 이 아니라 **worker 의 진로를 새 방향으로 강제 redirect**. |
| 책임 경계 | CLI 는 deterministic primitive (simhash 계산, Jaccard query, stagnation status) 만 담당. 가공 / 판단 / LLM 호출은 Skill 영역. (CLAUDE.md "책임 경계" 원칙 준수) |
| 멱등성 | 동일 입력 → 동일 출력. CLI 는 backlog 크기에 따라 임계값을 자동 조정하지 않는다. 임계값은 인자로만 받는다. |

## 3. Design Decisions (확정)

### 3.1 Simhash 입력

- **기본 fallback**: `title + body` 를 simhash 입력으로 사용 (기존 fingerprint 와 동일 입력).
- **명시 전달**: Skill 이 가공한 텍스트를 명시할 수 있도록 `--simhash-input "<text>"` 옵션 제공.
- **가공 전략 (Skill 책임)**: PR 기반 task 인 경우 `gh pr view <N> --json files` 로 파일별 라인수 desc 정렬 → 상위 N 개 path 와 title/body 를 합쳐 한 텍스트로 가공한다. 가공 자체는 Skill 영역이며, 결과 텍스트만 CLI 에 전달한다.

### 3.2 Storage (옵션 B 확정)

`tasks` 테이블에 두 개 nullable 컬럼을 추가한다.

| 컬럼 | 타입 | 설명 |
|------|------|------|
| `simhash` | `INTEGER NULL` | 64-bit unsigned. rusqlite 의 i64 매핑을 통해 bit pattern 을 보존한다. |
| `affected_paths` | `TEXT NULL` | JSON array of strings. 라인수 desc 정렬한 top N path. |

**마이그레이션 정책**: 기존 row 는 NULL 인 채로 둔다. 별도 backfill SQL 은 작성하지 않는다 (사용자 본인 DB 한정 사용 정책). 새 task 부터 `task add` / `task add-batch` 가 두 컬럼을 채운다.

### 3.3 알고리즘

| 차원 | 알고리즘 | 설명 |
|------|----------|------|
| 텍스트 유사도 | **simhash 64-bit** | token shingles (whitespace + punctuation split) → weighted bit voting → 64-bit signature. hamming distance 가 유사도 척도. |
| 영역 유사도 | **Jaccard similarity** | path-set 비교. `|A ∩ B| / |A ∪ B|`. |
| 결합 | **Hybrid (OR 합집합)** | simhash 거리 ≤ T **또는** Jaccard ≥ J 인 task 모두 후보로 잡는다. 한쪽 차원에서만 잡혀도 stagnation 가능성이 있다고 본다. |

### 3.4 Threshold (default, CLI 인자로도 받음)

| 기호 | 의미 | Default |
|------|------|---------|
| N | 유사 task 개수 | **3** |
| T | simhash hamming distance 상한 | **3** (64-bit 기준 ~95% 유사) |
| J | Jaccard 하한 | **0.5** |
| N\_esc | 조건부 escalate threshold | **5** |

### 3.5 Escalate 정책

- **조건부 escalate**: 1차 감지 시 hook 은 redirect prompt 만 노출 (persona / 영역 변경 권유).
- 같은 그룹의 후속 task 가 N≥N\_esc (= 5) 까지 도달해도 stagnation 이 풀리지 않으면, hook 이 **자동으로 escalate event 를 ledger 에 기록** + 사람 개입 요청 prompt 를 노출한다.
- escalate 동작은 ledger 에 `TaskEscalated` event 를 emit 하는 형태로 구현한다. **기존 `EventKind` 에 이미 존재** (`cli/src/domain/event.rs:55`, `as_str()` 매핑은 line 82) 하므로 새 variant 추가 / 마이그레이션 작업은 불필요. 본 epic 에선 기존 event 를 그대로 재사용한다.

### 3.6 LLM 사용 (C-2 hybrid)

| 영역 | 동작 |
|------|------|
| CLI | deterministic only — LLM 호출 금지. simhash / Jaccard / 후보 조회만 담당. |
| Skill (`resilience` SKILL.md) | simhash / Jaccard 후보를 Haiku 에게 던져 "정말 유사한가" verify → false positive 감소. 후보가 있을 때만 호출하므로 비용 / latency 절약. |

### 3.7 시점 (1차: pre-dispatch only)

- **pre-dispatch only**: worker 가 task claim 직전 hook 이 stagnation 체크.
- mid-dispatch (worker 작업 중 강제 종료) 는 본 epic 범위 밖 — Out of Scope / Future Work 참조.

### 3.8 Hook 설계

- PreToolUse hook (Bash matcher) 이 `autopilot task claim ...` 명령만을 대상으로 한다. `task add` 시점은 worker 와 무관하게 ledger 에 task 가 들어오는 단계일 뿐이라 stagnation redirect 의 본질 (worker 진로 차단) 과 맞지 않다. 추가 명령으로의 확장은 운영 데이터를 보고 follow-up 에서 결정.
- 감지 시 hook 이 `autopilot check stagnation --task <id>` 를 호출한다.
- exit code 별 hook 동작:

| CLI exit | Hook 동작 |
|----------|-----------|
| 0 | 정상 진행 (hook exit 0) |
| 4 (stagnation detected, N≥3) | hook exit 2 + stderr 에 redirect prompt 노출 |
| 5 (escalate required, N≥N\_esc) | hook exit 2 + redirect prompt + `autopilot task escalate` 자동 호출 |

### 3.9 CLI 인터페이스

```bash
# 저장 — task add 시 자동 계산 또는 명시 전달
autopilot task add --title T --body B \
  [--simhash-input "<가공 텍스트>"] \
  [--paths "p1,p2,p3,..."]

# 조회 — Skill 호출용 deterministic primitive
autopilot check stagnation --task X \
  [--max-distance 3] [--min-jaccard 0.5] \
  [--n-threshold 3] [--n-escalate 5]
```

**Exit code**:

| Code | 의미 |
|------|------|
| 0 | OK (stagnation 아님) |
| 1 / 2 / 3 | 기존 의미 유지 (validation / runtime / at-capacity 등) |
| 4 | stagnation detected (default N 도달) |
| 5 | escalate required (N\_esc 도달) |

**Stdout**: 아래 §3.10 의 JSON.

### 3.10 결과 JSON 형태

```json
{
  "status": "stagnation",
  "current_task": {
    "id": "abc123",
    "simhash": "0x...",
    "affected_paths": ["..."]
  },
  "similar_tasks": [
    {
      "id": "def456",
      "title": "...",
      "similarity": {
        "simhash_distance": 2,
        "jaccard": 0.75,
        "shared_paths": ["..."]
      },
      "outcome": "failed",
      "failure_reason": "...",
      "completed_at": "2026-..."
    }
  ],
  "pattern": {
    "shared_paths": ["..."],
    "common_failure_categories": ["..."],
    "consecutive_failures": 3
  },
  "recommended_persona": "hacker"
}
```

| 필드 | 비고 |
|------|------|
| `status` | `"stagnation"` \| `"escalate"` \| `"ok"` |
| `similar_tasks[].outcome` | `"failed"` \| `"completed"` \| `"wip"` 등 ledger 의 task 상태 |
| `similar_tasks[].failure_reason` | events 테이블에서 derive |
| `pattern.shared_paths` | 모든 후보가 공통으로 가지는 path |
| `recommended_persona` | persona 추천 결과 또는 `null` |

### 3.11 Hook prompt (예시)

영문 / 한국어 혼용은 허용. 아래는 영문 예시.

```text
[STAGNATION DETECTED] task XXXX
This task's territory is exhausted — N similar tasks have failed before:
  - same paths: src/cmd/task.rs, src/store/sqlite.rs
  - same failure category: test_compile_error (3 consecutive)

DO NOT proceed with the same approach. Try one of:
  1. Different file area entirely (look outside src/cmd/, src/store/)
  2. Persona shift: "hacker" — challenge the underlying assumption

Recommended persona: hacker
```

N≥N\_esc (escalate) 인 경우 위 prompt 끝에 다음 라인 추가:

```text
  3. ESCALATED to human review (autopilot task escalate XXXX has been called automatically)
```

## 4. Implementation Areas

| 영역 | 변경 |
|------|------|
| `cli/src/domain/task.rs` | `Task` struct 에 `simhash: Option<u64>`, `affected_paths: Option<Vec<String>>` 추가 |
| `cli/src/store/sqlite.rs` | `tasks` 테이블 schema 마이그레이션 (ALTER TABLE), serialize / deserialize 갱신 |
| `cli/src/store/memory.rs` | 동일 (in-memory 테스트 backend) |
| `cli/src/cmd/task.rs::add` / `add_batch` | `--simhash-input` / `--paths` 인자 처리. 미지정 시 `derive_simhash(title + body)` fallback |
| `cli/src/cmd/check/stagnation.rs` | 기존 simhash 코드를 base 로 ledger-native 재작성 (501 라인 → 새 흐름) |
| `cli/src/cmd/check/mod.rs` | `check stagnation` 을 별개 서브명령으로 노출 (`check diff` 와 통합하지 않음 — SRP / clap 구조 분리 우선) |
| `cli/src/cmd/mod.rs` | clap 인자 정의 |
| `cli/src/main.rs::exit_code_for` | exit 4 / exit 5 분류 추가 |
| Skill: `plugins/github-autopilot/skills/resilience/SKILL.md` | 입력 형태 갱신 (ledger-based stagnation JSON 수신), Haiku verify 가이드 추가 |
| Hook: PreToolUse 등록 위치 | 스크립트는 `plugins/github-autopilot/hooks/<name>.sh` (이번 epic 의 hook 은 `protect-stagnation.sh` 가 자연스러움 — 정확한 이름은 구현 단계에서 확정). 사용자 settings.json 에 `bash ${CLAUDE_PLUGIN_ROOT}/hooks/<name>.sh` 형태로 등록하고, `commands/setup.md` 에 등록 가이드 한 절을 추가한다 (`hooks/guard-pr-base.sh` / `hooks/check-cli-version.sh` 등록 사례를 따름). |
| Test: `cli/tests/check_stagnation_tests.rs` (신규) | scenario 테스트 (in-memory store + Hybrid 알고리즘 검증) |

## 5. Acceptance Criteria

- [ ] `task add` / `task add-batch` 가 `simhash` 와 `affected_paths` 를 채운다 (기본 fallback 또는 명시 전달).
- [ ] `autopilot check stagnation --task X` 가 정확한 exit code (0 / 4 / 5) 와 §3.10 의 결과 JSON 을 반환한다.
- [ ] PreToolUse hook 이 `task claim` 직전에 stagnation 체크를 수행하고 redirect prompt 를 stderr 로 노출한다 (exit 2).
- [ ] N ≥ N\_esc 인 경우 hook 이 자동으로 `autopilot task escalate` 를 호출하고 ledger 에 escalate event 를 기록한다.
- [ ] `cargo fmt --check` / `cargo clippy --tests -- -D warnings` / `cargo test` 모두 통과.
- [ ] `resilience` SKILL.md 가 새 흐름을 반영한다 (입력 / 출력 / persona 가이드 / Haiku verify 절차). **본 epic 의 acceptance 는 SKILL.md 가이드 문서 갱신까지로 한정** — 실제 Haiku 호출 코드는 별도 후속 epic.

## 6. Out of Scope / Future Work

본 epic 에서는 다루지 **않는다**. 1차 운영 후 follow-up 으로 검토한다.

| 항목 | 설명 |
|------|------|
| Mid-dispatch 강제 종료 (시점 B) | autopilot watch 가 ledger event 를 보면서 stagnation 패턴이 명확해지면 `TaskStagnationDetected` event 를 emit. 메인 에이전트가 이 event 를 받아 `TaskStop` + 새 prompt 로 재dispatch 하는 흐름. 본 epic 범위 밖. |
| Haiku verify 의 실제 호출 코드 | 본 epic 에서는 Skill 가이드만 정리한다. 실제 LLM 호출 흐름 (API 클라이언트, 비용 가드 등) 은 별도 epic. |
| Simhash 알고리즘 교체 / 다중 알고리즘 보관 | Storage 옵션 B (단일 컬럼) 을 채택했으므로, 다중 알고리즘 동시 보관은 향후 schema 확장이 필요할 때 재검토. |
| 기존 row 의 simhash / paths backfill SQL | 정책상 수행하지 않는다. 새 task 부터만 채운다. |

## 7. Resolved Decisions

spec 검토 단계에서 다음 결정이 확정되었다.

1. **TaskEscalated event 는 기존 `EventKind` 에 이미 존재** (`cli/src/domain/event.rs:55`, `as_str()` 매핑은 line 82). 새 variant 추가 불필요. 본 epic 에선 escalate 흐름이 기존 event 를 그대로 재사용한다.

2. **`check stagnation` 은 별개 서브명령으로 정의**한다. `check diff` 와 통합하지 않음. 이유: SRP — stagnation 은 다른 책임 (similarity grouping vs diff comparison) 이며 clap 구조도 명확히 분리되는 게 가독성/유지보수에 유리하다.

3. **PreToolUse hook 등록은 plugin 표준을 따른다**:
   - 스크립트 위치: `plugins/github-autopilot/hooks/<name>.sh` (이번 epic 의 hook 은 `protect-stagnation.sh` 가 자연스러움 — 정확한 이름은 구현 단계에서 확정)
   - 사용자 settings.json 에 `bash ${CLAUDE_PLUGIN_ROOT}/hooks/<name>.sh` 형태로 등록
   - `commands/setup.md` 에 등록 가이드 한 절 추가 (`hooks/guard-pr-base.sh` / `hooks/check-cli-version.sh` 의 등록 사례를 따름)

4. **Skill 의 Haiku verify 호출 인터페이스는 본 epic 에선 SKILL.md 가이드 문서까지만 정리**한다. 실제 코드 호출 (어떤 SDK / 어떤 함수) 은 별도 후속 epic 으로 분리. 본 epic 의 Acceptance Criteria 도 가이드 문서까지로 한정한다.

5. **Hook matcher 범위는 `autopilot task claim` 만**으로 한정한다. `add` 시점은 worker 와 무관하게 ledger 에 task 가 들어오는 단계일 뿐이라 stagnation redirect 의 본질 (worker 진로 차단) 과 맞지 않다. 추가 명령으로 확장은 운영 데이터를 보고 follow-up 에서 결정.

## 8. References

- 기존 stagnation 구현: `cli/src/cmd/check/stagnation.rs`
- ledger-only 전환 메모: `commands/gap-watch.md:233`
- persona 추천 로직: `cli/src/cmd/issue.rs:553`
- 책임 경계 원칙: 본 레포 `CLAUDE.md` "책임 경계 (CLI vs Skill/Agent)" 섹션
- Skill 갱신 대상: `plugins/github-autopilot/skills/resilience/SKILL.md`

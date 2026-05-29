# Atelier — Skill 중심 재구성

> **상태**: 설계 단계 (02-architecture 의 심화 / 일부 개정)
> **계기**: \"plugin 기능을 skill 로 분리하거나 skill reference 형태로 통합\" 검토 요청
> **선행**: 01-inventory(자산), 02-architecture(구조)

02 는 6개 plugin 을 폴더로 \"이동\"하는 데 초점을 뒀다. 이 문서는 한 단계 더 들어가
**기능을 Skill 레이어로 재구성**하는 모델을 정의한다. 이는 이 repo 자신의 설계 교리와
CLAUDE.md 책임 경계를 atelier 통합 기회에 실제로 적용하는 작업이다.

---

## 1. 문제 진단 (측정 결과)

### 1.1 Fat Controller 만연

이 repo 의 `agent-design-principles` skill 은 명시한다: **\"Slash Command = Controller, 로직을
직접 구현하지 않음\"**. 그러나 현실은 반대다.

| command | 줄 수 | 인라인된 내용 |
|---|---:|---|
| `spec-kit/spec-review` | 459 | L1/L2/audit 오케스트레이션 프로토콜 전체 (프롬프트 템플릿, 피드백 루프, 종료 조건, drop-log) |
| `git-utils/setup` | 389 | 환경 감지 + hook 설치 절차 |
| `github-autopilot/build-issues` | 385 | idle/capacity, adaptive throttling, 의존성·유사도 분석, 에스컬레이션 |
| `github-autopilot/autopilot` | 361 | 이벤트 라우팅 + 파이프라인 전체 |
| `git-utils/git-resolve` | 313 | 충돌 해결 판단 프로토콜 |
| `spec-kit/annotate-spec` | 301 | 주석 생성 절차 |
| `git-utils/prioritize-issues` | 301 | 이슈 우선순위 판단 로직 |
| `github-autopilot/work-ledger` | 299 | ledger 운영 로직 |
| ... | | (200줄 이상 command 14개) |

### 1.1.1 도구가 같은 진단을 강제

`tools/validate` 는 이미 \"command 가 300줄 초과 → agent/skill 레이어로 비즈니스 로직을 옮길지
검토하라\" 경고를 자동 생성한다. Phase 1 시도에서 11건이 경고로 떴고(spec-review 455·git-utils
setup 382 등) 본 문서의 fat command 진단과 정확히 일치한다. Epic 2 추출은 사용자 직관과
도구 양쪽에 의해 정당화된다.

### 1.2 Skill 레이어 미사용

command 가 skill 을 참조하는 빈도:

```
35개 command 중 "skill" 을 언급하는 것: 6개 (대부분 1~2회 단순 언급)
```

→ command 들이 도메인 지식을 **공유 skill 로 끌어올리지 않고 각자 인라인**한다.
동일 패턴(예: spec 의 L1/L2 오케스트레이션)이 spec-review·gap-detect·annotate-spec 에
**중복 서술**된다.

### 1.3 이미 존재하는 좋은 모델

| 모델 | 패턴 |
|---|---|
| `orchestrator` skill | SKILL.md(222줄) + `references/` 4개 (merge-coordinator 207, agent-monitor 179, delegation-patterns 157, worktree-lifecycle 129) — **progressive disclosure** |
| `workflow-guide` | command(install/scaffold)는 얇고, 지식은 convention-architect(363)·agent-design-principles(294) skill 에 |

atelier 는 이 두 모델을 **전 도메인 표준**으로 삼는다.

---

## 2. 목표 레이어 모델

`agent-design-principles` 교리 + CLAUDE.md CLI/Skill 경계를 atelier 에 적용:

```
┌──────────────────────────────────────────────────────────┐
│ Slash Command = Controller                                │
│   인자 파싱 → 적절한 skill 로드 + agent 위임. 로직 없음.   │
│   목표: 대부분 50~120줄                                    │
├──────────────────────────────────────────────────────────┤
│ Sub-agent = Service                                       │
│   여러 skill 조합해 워크플로우 실행. 별도 context.         │
├──────────────────────────────────────────────────────────┤
│ Skill (SKILL.md + references/) = Domain                   │
│   판단·프로토콜·도메인 지식. progressive disclosure 로     │
│   bulk 는 references/ 에 두어 on-demand 로드.              │
├──────────────────────────────────────────────────────────┤
│ Rust CLI (atelier) = 결정적 변환                          │
│   동일 입력 → 동일 출력. 상태 전이·계산. 판단 없음.        │
└──────────────────────────────────────────────────────────┘
```

핵심: **판단(judgment)은 Skill, 변환(transform)은 CLI** (CLAUDE.md). 지금 Fat Command 안에
섞여 있는 둘을 이 통합 기회에 분리한다.

---

## 3. Skill 폭발 가드 (중요 제약)

`agent-design-principles` 경고: Claude 는 시작 시 **모든 skill 의 name/description 를
시스템 프롬프트에 상주**시킨다. atelier 는 이미 skill 11개(02 §1) + 다른 active plugin 들의
skill 까지 메타데이터가 누적된다.

따라서 재구성은 **새 top-level skill 을 최소화**하고, bulk 지식은 `references/` 로 넣는다.

| 하면 안 됨 | 해야 함 |
|---|---|
| 명령마다 skill 1개 (spec-review-skill, gap-detect-skill, ...) → 폭발 | 도메인당 skill 1개 + 명령별 `references/*.md` |
| 작은 판단을 굳이 skill 로 | 1곳에서만 쓰고 짧으면 thin command 에 인라인 |

판단 기준 (교리 그대로):
- **2곳 이상 참조 OR 200줄 이상 재사용 지식** → skill(필요 시 references 분할)
- **1곳 전용 + 짧음** → thin command 인라인

---

## 4. 도메인별 재구성안

### 4.1 spec 도메인

신규 skill **`spec-workflow`** (SKILL.md + references):

```
skills/spec-workflow/
├── SKILL.md                      # 공통 원칙: L1/L2/audit 레이어, 인용 검증 철학
└── references/
    ├── file-observation.md       # ← spec-review Step 3~4 (L1 프로토콜)
    ├── gap-audit-loop.md         # ← spec-review Step 5~6 (L2 + audit 루프/종료조건)
    ├── report-format.md          # ← spec-review Step 7 + Output Examples
    ├── design-protocol.md        # ← design + design-detail 대화형 설계 절차
    └── annotation.md             # ← annotate-spec 주석 생성 절차
```

기존 유지 skill: `spec-criteria`(84), `issue-report`(89) — 이미 SRP, 그대로.

command 변화 (thin 화):
- `spec/spec-review` 459 → ~80 (인자 파싱 → spec-workflow 로드 + file-pair-observer/gap-aggregator/gap-auditor agent spawn)
- `spec/design`·`spec/design-detail` → design-protocol 참조하는 thin controller
- `spec/annotate-spec` → annotation 참조

### 4.2 autopilot 도메인

신규 skill **`autopilot-pipeline`** (SKILL.md + references):

```
skills/autopilot-pipeline/
├── SKILL.md                      # 공통: 이벤트/cron 모드, idle·throttling 철학, ledger 개념
└── references/
    ├── build-pipeline.md         # ← build-issues (capacity, 의존성, 에스컬레이션)
    ├── ci-watch.md               # ← ci-watch + ci-fix
    ├── merge.md                  # ← merge-prs
    ├── gap-watch.md              # ← gap-watch
    ├── qa-boost.md               # ← qa-boost
    └── ledger.md                 # ← work-ledger + stale-task-review
```

기존 유지 skill: `resilience`(205), `draft-branch`(153), `issue-label`(142), `branch-sync`(46) — SRP 양호, 유지.

command 변화: `autopilot/*` 11개가 해당 reference 를 로드하는 thin controller 로. 결정적 상태 전이(task add/claim, epic status 등)는 **`atelier autopilot` CLI 호출**로 위임 (이미 CLI 보유).

> **중복 수렴 (중요)**: autopilot 은 현재 build-issues·merge-prs·autopilot 에서 \"Agent Team\",
> worktree, 병렬 dispatch 를 **자체 서술**하고 `pr-merger` agent 가 머지를 직접 다룬다. 이는
> orchestrator skill 과 중복이다. autopilot-pipeline 은 위임·worktree·머지 메커니즘을 **재구현하지
> 않고 orchestrator skill 에 위임**한다 (§4.5). reference 에는 \"무엇을 위임할지\"만 두고 \"어떻게
> 병렬화/머지할지\"는 orchestrator 가 단일 소유.

### 4.3 git 도메인

기존 **`git`** skill(261) 을 references 로 확장 (신규 top-level skill 없음):

```
skills/git/
├── SKILL.md                      # 기존 유지 (git 워크플로우 개요)
└── references/
    ├── conflict-resolution.md    # ← git-resolve 충돌 판단 (orchestrator/merge-coordinator 와 정합)
    ├── issue-prioritization.md   # ← prioritize-issues 판단 로직
    └── sync-strategy.md          # ← git-sync 절차
```

결정적 git 연산(commit, branch, guard, PR)은 **`atelier git` CLI** (Rust, 02 §4). command 는
판단만 남기고 CLI 호출.

`hook-config`·`branch-status`·`check-ci` 등 짧고 결정적인 것은 thin command 유지(인라인) 또는 CLI 흡수.

### 4.4 workflow 도메인

이미 skill 중심 (convention-architect, agent-design-principles). **변경 최소** — 모범 사례로 유지.
`workflow/scaffold-conventions`(rename됨)·`workflow/install` 은 현 구조 유지.

### 4.5 cross-cutting 기반 skill (orchestrator, resilience) + coding-style

모든 skill 이 \"관심사(slash) 도메인\"인 것은 아니다. orchestrator·resilience 는 **다른 도메인
skill 과 메인 에이전트가 공통으로 의존하는 가로지르는 기반(foundation)** 이다. 06 의 슬래시
표면에 넣지 않고, 풍부한 트리거 description 으로 **모델이 자동 호출**하게 둔다.

**skill 3계층 분류** (atelier 의 13개를 역할로 정리):

| 계층 | 호출 | skill |
|---|---|---|
| 관심사(domain) | user-invocable 슬래시 + 모델 (06) | spec-workflow, autopilot-pipeline, git, workflow(convention) |
| **기반(foundation)** | 모델 자동 (슬래시 X) | **orchestrator**, resilience |
| 헬퍼(SRP) | 모델 자동 (슬래시 X) | draft-branch, issue-label, branch-sync, issue-report, spec-criteria, agent-design-principles, coding-style |

#### orchestrator — 통합의 모범이자 단일 위임 엔진

- 이미 command/agent/CLI 없이 **순수 skill + references 4개**(delegation-patterns·worktree-lifecycle·
  agent-monitor·merge-coordinator). atelier 가 목표하는 패턴 그 자체 → **구조·이름 그대로 보존**.
- **단일 소유(single owner) 원칙**: 병렬/순차 판단, worktree 토폴로지, agent team, **머지 조정**은
  orchestrator 만 소유한다. autopilot-pipeline·git 은 여기에 위임한다 (§4.2 수렴).
- **cross-plugin 참조 내부화**: orchestrator 진입 절차의 `git-utils:/epic init`·`git-utils:/git-branch`
  참조와, spec-kit `gap-auditor` → orchestrator 참조는 atelier 내부 namespace 로 치환 (02 §2.2 의 13건에 포함).

#### 머지/충돌 로직 수렴 (3곳 → 1곳)

현재 충돌·머지 책임이 **세 곳**에 흩어져 있다:

```
orchestrator/references/merge-coordinator.md   ← 병렬 결과 통합 (canonical 후보)
github-autopilot/agents/pr-merger.md           ← autopilot PR 머지
git-utils/commands/git-resolve.md              ← rebase 충돌 해결
```

Epic 2 수렴 방향:
- **merge-coordinator 를 canonical** 로. \"여러 변경을 어떤 순서로, 충돌을 어떻게 위임\"의 단일 출처.
- git skill `references/conflict-resolution.md`(§4.3) 는 충돌 *해결 전략* 만 담고 조정은 위임.
- autopilot `pr-merger` agent 는 service 레이어로 남되, 머지 *판단* 은 orchestrator 를 참조.

#### resilience — 실패/재시도 기반

autopilot 의 실패 처리(retry/backoff/escalation) 지식. autopilot-pipeline 이 의존. 그대로 유지하되
\"언제 에스컬레이션\" 판단의 단일 출처로 (§5 의 판단 항목과 정합).

#### coding-style

templates/CLAUDE.md → `skills/coding-style/` (02 §1). hook(suggest-simplify)은 그대로. SRP 헬퍼.

### 4.6 신규 top-level skill 합계

```
신규: spec-workflow, autopilot-pipeline       → +2
확장(references 추가, top-level 불변): git    → +0
유지: 기존 11개 그대로
─────────────────────────────────────────────
atelier top-level skill: 11 → 13 (메타데이터 예산 안전)
bulk 지식: 14개 references/*.md 로 on-demand
```

---

## 5. CLI 경계 재확인 (판단 vs 변환 분리)

Fat Command 를 해체하면서 **결정적 부분은 CLI 로 내린다**. 통합 시 함께 정리:

| 현재 command 인라인 | 분류 | 이동처 |
|---|---|---|
| spec L1/L2 루프 종료조건·drop 정책 | 판단 | spec-workflow skill |
| spec 인용 검증의 문자열 매칭/카운트 | 변환 | `atelier spec` CLI (신규 검토) 또는 기존 도구 |
| autopilot idle_count·throttling 계산 | 변환 | `atelier autopilot` CLI (이미 일부 보유) |
| autopilot \"언제 에스컬레이션\" 판단 | 판단 | autopilot-pipeline skill |
| git 충돌 해결 \"어느 전략\" | 판단 | git skill / conflict-resolution |
| git rebase/merge 실행 | 변환 | `atelier git` CLI |

> spec 쪽 결정적 변환(인용 검증)을 CLI 로 내릴지는 별도 판단 — 04 Phase 0 에서 비용 평가.
> 무리하면 spec skill 안에 \"검증 규칙\"으로 두고 CLI 화는 후속으로.

---

## 6. 02/04 에 대한 영향 (개정 사항)

이 모델 채택 시 02·04 의 다음이 바뀐다.

### 02-architecture 개정
- §1 디렉토리: `skills/` 에 `spec-workflow/`, `autopilot-pipeline/` 추가, `git/references/` 추가.
- commands/agents 폴더 구조는 동일하되 command 가 **thin** 으로 재작성됨을 명시.

### 04-rollout 개정
- **Phase 3 의 성격 변화**: \"이동\"이 아니라 \"이동 + 추출(extract) + 수렴(converge)\". 도메인별 PR 분할:
  - `feat(atelier): extract spec workflow into skill + references`
  - `feat(atelier): extract autopilot pipeline into skill + references`
  - `feat(atelier): extract git judgment into skill references`
  - `refactor(atelier): converge merge/orchestration onto orchestrator skill` (§4.5)
- 각 PR 은 **동작 보존(behavior-preserving)** 이어야 함. 기존 command 를 명세로 삼아 회귀 검증.
- 검증 체크리스트 추가:
  ```
  □ command 평균 줄 수 대폭 감소 (Fat Controller 해소)
  □ 추출 전/후 동일 입력에 동일 동작 (회귀 0) — 대표 시나리오 수동 검증
  □ top-level skill 수 ≤ 13 (메타데이터 예산)
  □ references/ 는 on-demand (SKILL.md 가 명시적으로 가리킬 때만 로드)
  □ 머지/orchestration 단일 소유: autopilot 이 worktree/병렬/머지를 재서술하지 않음
  □ merge-coordinator 가 충돌 조정의 유일한 canonical 출처
  ```

---

## 7. 적용 깊이 결정 — ✅ C (단계 분리) 확정

이 재구성은 \"이동\"보다 **훨씬 큰 리팩토링**(동작 보존하며 14개 fat command 해체)이다.
검토한 깊이:

| 깊이 | 내용 | 비용/리스크 | 결정 |
|---|---|---|---|
| A. 이동만 (현 02) | 폴더 이동 + namespace 만. Fat Controller 유지. | 낮음. \"깔끔\" 목표 미달. | |
| B. 전체 추출 | 4.1~4.3 전부. 통합 PR 에서 thin 화까지. | 높음(회귀 리스크). | |
| **C. 단계 분리** | **통합(A)을 먼저 머지 → 별도 epic 으로 추출(B).** | 중간. 리스크 격리, 깔끔함은 2단계 후 달성. | ✅ |

**결정**: **C**. 통합과 리팩토링의 리스크를 분리한다. 같은 깔끔함을 얻되, atelier 동작 안정화를
먼저 확보하고 fat command 해체는 그 위에서 독립적으로 검증한다.

### 7.1 C 가 만드는 두 개의 epic

```
Epic 1: atelier consolidation  (이동 전용)
  - 범위: 00~04 그대로. 02 = 폴더 이동 + namespace + CLI 통합 + freeze.
  - Fat Controller 는 일단 그대로 옮긴다 (동작 보존이 최우선).
  - 산출: atelier 가 6개 기능을 동일하게 수행. marketplace 노출 + freeze.

Epic 2: atelier skill extraction  (이 문서 §4~6)
  - 선행: Epic 1 머지 + 안정화.
  - 범위: spec-workflow / autopilot-pipeline / git references 추출. command thin 화.
  - 도메인별 PR, 각각 동작 보존 검증 (§6 검증 체크리스트).
```

---

## 8. 다음 단계

- **Epic 1 (통합)**: 02·04 **개정 불필요** — 현 상태가 곧 \"이동 전용\". 이 문서의 §4~6 은
  Epic 1 에 영향을 주지 않는다.
- **Epic 2 (추출)**: 이 문서(§2~6)가 그대로 설계서가 된다. Epic 1 머지 후 #738 의 두 번째
  sub-epic 으로 개설. Epic 2 착수 시점에 02 §1(skills 트리)·04 Phase 3 검증 항목을 §6 대로 반영.
- 04 §6 작업 추적에 Epic 2 를 명시 (별도 편집).

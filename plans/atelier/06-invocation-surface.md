# Atelier — 호출 표면 (Command vs Skill vs CLI)

> **상태**: 설계 단계 (05 의 심화 / §4 command 목록 일부 개정)
> **계기**: \"자주 안 쓰는 command 가 많고, 똑똑해진 모델로 좋은 skill 로 디자인할 것이 많다\" 검토 요청
> **선행**: 02(구조), 05(skill 추출)

05 는 \"command 로직을 skill 로 추출, command 는 thin controller 유지\"였다. 이 문서는 그 전제를
한 단계 더 의심한다: **애초에 이게 command 여야 하는가?** 어떤 기능은 사용자가 슬래시를 기억해
입력하지 않는다 — 모델이 맥락을 보고 자동 호출하는 **skill** 이 더 맞다.

---

## 1. 세 가지 호출 표면

| 표면 | 누가 호출 | 언제 | 근거 |
|---|---|---|---|
| **Slash Command** | 사용자 (명시적 `/name args`) | 의식적·반복적, 구조적 인자로 시작 | 결정적 프롬프트 주입 |
| **Skill** | 모델 (description 매칭 자동) 또는 자동화 | 맥락이 맞을 때 모델이 판단 / cron·loop 이 호출 | `user-invocable` 로 슬래시 노출 여부 제어 |
| **CLI** (`atelier ...`) | 모델/command 가 도구로 | 결정적 변환이 필요할 때 | 동일 입력 → 동일 출력 |

**근거 (이미 repo 에 존재)**: `convention-architect`, `agent-design-principles` 는
`user-invocable: false` — 모델 전용 skill 의 선례. 즉 \"슬래시 없는 skill\" 은 이 코드베이스의
확립된 패턴이다.

---

## 2. 결정 트리

각 기능에 대해 위에서부터 적용:

```
Q1. 결정적 변환인가? (판단 없이 동일 입력→동일 출력)
    └ YES → CLI (atelier ...)

Q2. 사용자가 의식적으로, 특정 시점에, 구조적 인자를 줘서 시작하는가? (자주)
    └ YES → Slash Command (argument-hint + $ARGUMENTS)

Q3. 모델이 대화 맥락에서 "지금 이게 필요"를 판단할 수 있는가?
    또는 자동화(cron/loop/orchestrator)가 내부 단계로 호출하는가?
    └ YES → Skill (도메인 skill 의 동작으로 흡수; user-invocable: false)
```

> Q2 와 Q3 는 배타가 아니다. \"의식적 시작 + 모델 자동화\"를 둘 다 원하면 도메인 skill 을
> `user-invocable: true` 로 두어 슬래시로도 노출한다 (3절 가드 주의).

---

## 3. Skill 폭발 가드 (필수 제약)

`agent-design-principles`: Claude 는 시작 시 **모든 skill 의 description 를 시스템 프롬프트에 상주**.
→ command 15개를 skill 15개로 바꾸면 **메타데이터 폭발**. 이건 피해야 한다.

**해법**: Pattern 1·2 기능을 **개별 skill 로 만들지 않고, 05 가 정의한 도메인 skill 에 흡수**한다.
도메인 skill 의 description 을 충분히 풍부하게 써서 모델이 그 도메인 작업(\"이슈 우선순위\",
\"갭 탐지\", \"커버리지 보강\")에 자동 관여하게 하고, 구체 프로토콜은 `references/` 에 둔다.

```
❌ skills/prioritize/  skills/analyze-issue/  skills/gap-detect/ ... (폭발)
✅ skills/autopilot-pipeline/ (description 이 우선순위·분석·갭을 포괄)
     references/{prioritization,issue-analysis,gap-detection}.md
```

결과 top-level skill 수: 05 의 **13개 유지** (신규 skill 추가 없음). 호출 표면만 재배치.

---

## 4. 35개 command 분류

| 현재 command | 표면 | 이동처 |
|---|---|---|
| git-utils/setup, github-autopilot/setup, coding-style/setup, workflow-guide/install, scaffold-rules, scaffold-spec-rules, hook-config | **Slash (통합)** | `/atelier:setup` + 모듈 (02 §3) |
| spec-kit/design, design-detail | **Slash** | 대화형 설계 시작 — deliberate |
| spec-kit/spec-review | **Slash** | deliberate 리뷰 실행 (spec-workflow skill 호출) |
| git-utils/epic | **Slash** | init/plan/next/status — 구조적 서브커맨드 |
| git-utils/git-resolve | **Slash** | rebase 중 deliberate (judgment 은 git skill ref) |
| github-autopilot/autopilot | **Slash** | 자동화 루프 진입점 (cron 도 호출) |
| commit-and-pr, commit-and-push, git-branch, git-sync, merge-pr, create-issue, branch-status, check-ci, unresolved-reviews | **CLI** (+판단부는 skill) | `atelier git ...` (02 §4). 메시지 생성 등 판단은 git skill |
| **prioritize-issues** | **Skill** (Pattern 2) | git/autopilot skill — \"뭐부터?\" 에 모델 자동 |
| **analyze-issue** | **Skill** (Pattern 2) | autopilot-pipeline ref — 이슈 언급 시 자동 |
| **gap-detect** | **Skill** (Pattern 2) | spec-workflow ref — spec↔code 괴리 감지 시 |
| **annotate-spec** | **Skill** (Pattern 2) | spec-workflow ref — frontmatter 누락 시 |
| **qa-boost** | **Skill** (Pattern 2) | autopilot-pipeline ref — 변경 후 커버리지 |
| **build-issues, ci-watch, ci-fix, gap-watch, merge-prs, work-ledger, stale-task-review, test-watch** | **Skill** (Pattern 1) | autopilot-pipeline ref — autopilot 오케스트레이터/cron 이 호출, 슬래시 제거 |

### 4.1 결과 슬래시 표면 — 관심사(도메인) 단위 (확정)

**핵심 통찰**: skill 이 `user-invocable: true` 면 슬래시로도 호출된다. 따라서 \"command vs skill\"
이분법은 불필요하다. **command 레이어를 걷어내고 기능을 도메인 skill 로 통합하되, 그 도메인 skill 을
user-invocable 로 두면 슬래시 호출 + 모델 자동 호출을 모두 얻는다.**

슬래시 표면을 capability 단위(35개)가 아니라 **관심사 단위**로 재편한다:

```
/atelier:setup       설치·환경 설정          (concern: setup)
/atelier:spec        design / review / gap / annotate  → spec-workflow skill (user-invocable)
/atelier:autopilot   build / ci / merge / ledger / gap-watch / qa  → autopilot-pipeline skill
/atelier:git         branch-judgment / sync / resolve / prioritize → git skill (user-invocable)
/atelier:workflow    convention / scaffold-rules        → workflow skill
(+ 결정적 구조 동작은 CLI: atelier git branch <a> <b>, atelier epic init <name> ...)
```

- 사용자는 `/atelier:spec` 를 입력하고 \"리뷰해줘 / 갭 봐줘\" 라고 하거나, 그냥 맥락을 말하면 모델이
  spec-workflow skill 을 자동 호출한다. **둘 다 같은 skill 로 수렴**.
- 세부 capability(gap-detect, prioritize 등)는 도메인 skill 의 `references/` 동작 — 별도 슬래시 없음.
- 결정적·구조적 인자 동작(epic init, git branch)은 슬래시도 skill 도 아닌 **CLI**.

\"자주 안 쓰는 capability 슬래시\" 20여 개가 사라지고, 슬래시는 **관심사 5개 안팎**으로 수렴한다.

---

## 5. 핵심 효과

- **인지 부하 감소**: 사용자가 기억할 슬래시가 8개 안팎. 나머지는 \"하고 싶은 일\"을 말하면 모델이
  적절한 skill 을 자동 호출.
- **05 와의 관계 정리**: 05 가 \"thin command + skill\" 이라 했던 Pattern 1·2 항목 다수는
  **thin command 조차 불필요** — 도메인 skill 동작으로 충분. (05 §4 command 목록을 이 표 기준으로 축소.)
- **CLI 경계 강화**: 결정적 git 동작은 슬래시도 skill 도 아닌 CLI 로. (CLAUDE.md 경계와 일치.)

---

## 6. 전환 공격성 — ✅ 공격적 + 관심사 단위 (확정)

**결정**: command 레이어를 공격적으로 걷어내고 기능을 **관심사(도메인) skill 로 통합**한다.
skill 이 user-invocable 이라 슬래시 호출이 그대로 유지되므로, \"슬래시 제거 vs 유지\"의 trade-off 가
애초에 없다 — **공격적 통합과 명시적 슬래시 호출을 동시에** 얻는다.

| 항목 | 처리 |
|---|---|
| capability 단위 슬래시 (35개) | **제거**. 관심사 단위 user-invocable skill 로 통합 (§4.1) |
| Pattern 1 (자동화 내부) | 슬래시 노출 0. 도메인 skill 동작 (orchestrator/cron 호출) |
| Pattern 2 (모델 판단) | 도메인 skill 동작 + 도메인 슬래시로 진입 가능 (둘 다) |
| 결정적·구조적 동작 | CLI |

> **메타데이터 가드 유지**: 관심사 단위라 user-invocable 슬래시는 5개 안팎. top-level skill 총수는
> 05 의 13개를 넘기지 않는다 (capability 마다 skill 신설 금지 — §3).
>
> **concern 입도(granularity)**: 한 도메인 안에서 design 과 review 처럼 충분히 구별되는 관심사를
> 별도 user-invocable skill 로 둘지는 Epic 2 구현에서 확정. 원칙은 \"capability 당 하나가 아니라
> concern 당 하나\".

---

## 7. 영향 (05/04 개정)

- **05 §4**: \"thin command + skill\" 전제를 폐기. capability 단위 command 를 만들지 않고, 기능을
  관심사 단위 도메인 skill 로 통합한다. 도메인 skill 은 user-invocable 로 슬래시 진입점 겸함.
- **04 Epic 2**: 추출 PR = \"도메인 skill 통합 + 호출 표면 관심사화\". 검증 체크리스트:
  ```
  □ 슬래시 표면 = 관심사 단위 (~5개) + setup. capability 단위 슬래시 0
  □ 도메인 skill user-invocable: true 동작 (슬래시 진입 확인)
  □ 모델 자동 호출 검증: Pattern 2 대표 시나리오에서 도메인 skill 트리거
  □ top-level skill 수 ≤ 13 (capability 마다 skill 신설 금지)
  □ Pattern 1 슬래시 노출 0 (자동화 전용)
  □ 결정적·구조적 동작은 CLI 로 (슬래시/skill 아님)
  ```
- **Epic 1(통합)에는 영향 없음**: 이동 단계에서는 35 command 를 그대로 옮긴다. 호출 표면 재편은
  Epic 2 에서 수행 (단계 분리 원칙 유지).

---

## 8. 다음 단계

- 결정 확정 (공격적 + 관심사 단위). 05 §4·04 Epic 2 검증은 본 문서 §4.1·§7 이 기준.
- Epic 2 착수 시 concern 입도(§6 주석)를 구현에서 확정.

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

### 4.1 결과 슬래시 표면 (35 → ~8)

```
/atelier:setup          (통합)
/atelier:design
/atelier:design-detail
/atelier:spec-review
/atelier:epic
/atelier:git-resolve
/atelier:autopilot      (루프 진입점)
(+ git 결정적 동작은 atelier CLI)
```

나머지 ~20개 기능은 **모델/자동화가 호출하는 skill 동작**으로 전환. \"자주 안 쓰는 슬래시\"가 사라진다.

---

## 5. 핵심 효과

- **인지 부하 감소**: 사용자가 기억할 슬래시가 8개 안팎. 나머지는 \"하고 싶은 일\"을 말하면 모델이
  적절한 skill 을 자동 호출.
- **05 와의 관계 정리**: 05 가 \"thin command + skill\" 이라 했던 Pattern 1·2 항목 다수는
  **thin command 조차 불필요** — 도메인 skill 동작으로 충분. (05 §4 command 목록을 이 표 기준으로 축소.)
- **CLI 경계 강화**: 결정적 git 동작은 슬래시도 skill 도 아닌 CLI 로. (CLAUDE.md 경계와 일치.)

---

## 6. 결정 필요 — 전환 공격성

Pattern 2(모델 판단 가능) 기능에서 **슬래시를 완전히 없앨지**가 UX 판단이다.

| 모델 | Pattern 2 처리 | 장점 | 단점 |
|---|---|---|---|
| **X. 공격적** | 슬래시 완전 제거, 순수 모델 호출 | 가장 깔끔, 슬래시 최소 | 자동 호출이 description 품질·모델 판단에 의존. 명시적 제어 불가 |
| **Y. 보수적** | 도메인 skill `user-invocable: true` 로 슬래시도 유지 | 모델 자동 + 사용자 수동 둘 다 | 슬래시 목록에 다시 노출 (단, 통합 노출이라 8→13 수준) |
| **Z. 혼합** | 자주 쓸 법한 것(gap-detect, analyze-issue)만 슬래시 유지, 나머지 제거 | 균형 | 경계 판단 필요 |

> Pattern 1(자동화 내부 단계)은 어느 모델이든 슬래시 제거 (사용자 호출 대상 아님) — 이견 없음.
> 결정 대상은 Pattern 2 뿐.

---

## 7. 영향 (05/04 개정)

- **05 §4**: command 목록을 본 문서 4절 분류로 교체. \"thin command\" 는 Slash 표면만,
  Pattern 1·2 는 도메인 skill 동작으로.
- **04 Epic 2**: 추출 PR 에 \"호출 표면 전환\" 포함. 검증 체크리스트 추가:
  ```
  □ 슬래시 표면 = 의도한 deliberate 집합만 (~8개)
  □ 모델 자동 호출 검증: Pattern 2 대표 시나리오에서 도메인 skill 이 실제 트리거되는가
  □ top-level skill 수 ≤ 13 (메타데이터 예산 — 개별 skill 신설 금지)
  □ Pattern 1 슬래시 노출 0 (자동화 전용)
  ```
- **Epic 1(통합)에는 영향 없음**: 이동 단계에서는 35 command 를 그대로 옮긴다. 호출 표면 재배치는
  Epic 2 에서 수행 (단계 분리 원칙 유지).

---

## 8. 다음 단계

6절 전환 공격성(X/Y/Z) 결정 후 → 05 §4·04 Epic 2 검증을 7절대로 반영.

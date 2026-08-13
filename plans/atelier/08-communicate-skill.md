# Atelier — spec-review 제거 · communicate 스킬 신설

> **상태**: 설계 (구현 반영됨)
> **계기**: "spec-review 는 잘 안 쓰게 되니 삭제하고, 보고서 작성 스킬을 커뮤니케이션 스킬로 끌어올리자"
> **선행**: `.claude/rules/git-workflow.md §PR 본문 작성 스타일` (기본값의 출처)

두 변경을 한 묶음으로 다룬다. **쓰이지 않는 것을 덜어내고**, 산출물 이름에 묶여 있던 작문 스킬을
**전달 행위 기준으로 승격**한다.

---

## 1. 요구사항

| ID | 요구사항 |
|---|---|
| FR-1 | `spec-review` 스킬과 그 전용 에이전트를 제거한다 |
| FR-2 | 제거로 끊어지는 참조를 모두 정리한다 (문서·매니페스트·CI 불변식) |
| FR-3 | 작문 스킬의 축을 "보고서"에서 **"맥락 전달"** 로 옮긴다 |
| FR-4 | 기본값은 이 repo 의 PR 본문 규칙(독자 수준·해요체 단문·개조식·한국어)을 승격해 쓴다 |
| FR-5 | 에이전트와 쌓은 맥락을 **그 대화를 못 본 사람**에게 옮기는 기준을 제공한다 |
| FR-6 | 글이 놓이는 자리에 따른 적응을 다루되, **특정 도구가 아니라 성질 축**으로 기술한다 |
| NFR-1 | 기존 `report-write` 의 유효한 내용(문체·용어·청중 적응)은 잃지 않는다 |

---

## 2. 사이드이펙트 조사

### 2.1 spec-review 는 전용 에이전트 4개를 데리고 있었다

`agents/spec/*` 4개(`file-pair-observer`, `gap-aggregator`, `gap-auditor`, `spec-annotator`)는
description 에 "(내부용) spec-review skill 이 호출하는" 으로 명시된 **전용 에이전트**다. 스킬이
사라지면 호출자가 없어 전부 고아가 된다 → 함께 제거하고 `plugin.json` 의 `agents` 배열에서도 뺀다.

### 2.2 CI 불변식이 삭제를 막는다

`tools/validate/extraction-invariants.json` 은 "리팩토링이 내용을 떨어뜨렸는지"를 토큰 존재로
검사한다. `spec/review` 도메인의 두 토큰(`excerpt mismatch`, `gap-auditor`)은 삭제 대상 안에만
있으므로, 매니페스트에서 같이 빼지 않으면 CI 가 실패한다. **의도적 삭제와 사고를 구분하는 장치이므로
매니페스트 수정이 정상 절차다.**

`spec/annotate` 도메인의 `## 에러 처리` 토큰은 `spec-write/references/authoring.md` 에도 있어
그대로 살아남는다.

### 2.3 끊어지는 참조 (7곳)

`spec-write`(SKILL + authoring), `grill`, `agent-design-principles`,
`orchestrator/references/spec-driven-review.md`, `README.md` ×2, 그리고 위 매니페스트 2종.

특히 `spec-write` 의 `related_paths` 규약은 **존재 이유가 "후속 spec-review·gap-detect 가 쓴다"**
였다. 소비자가 사라지므로 근거 문장을 다시 써야 한다 (§4 미결 판단).

### 2.4 이름만 같은 다른 기능

`plugins/external-llm/commands/spec-review.md` 는 **외부 LLM 여러 개로 스펙을 리뷰**하는 별개
플러그인의 커맨드다. 이름만 겹치므로 이번 삭제 대상이 아니다.

### 2.5 report-write 의 내용은 대부분 살아남는다

문체·용어 해설·청중 적응·체크리스트는 "보고서"가 아니라 **글 일반**에 적용되는 기준이다. 이름과
축만 바뀌고 내용은 승계된다 (NFR-1).

---

## 3. 설계

### 3.1 축 이동: 산출물 → 전달 행위

`report-write` 는 산출물 이름에 묶여 "슬랙 답변"·"노션 문서"·"PR 설명"을 끌어오지 못했다. 실제
문제는 하나다 — **읽는 사람은 우리가 나눈 대화를 보지 못했다.** 스킬 이름을 `communicate` 로 바꿔
그 문제를 축에 세운다.

### 3.2 PR 본문 규칙을 자리와 무관한 기본값으로 승격

이 repo 는 이미 PR 본문에 대해 답을 갖고 있었다 — 독자는 IT 특성화 고등학생 수준, 친근한 해요체
단문, 개조식 우선, 한국어. 이 넷은 PR 에만 유효한 규칙이 아니라 **어디에 놓든 통하는 기본값**
이므로 그대로 승격한다.

더 중요한 건 **토스 4단(왜 / 무엇을 / 어떻게 / 확인 방법)이 맥락 이전의 4요소와 같다**는 점이다:

| 4단 | 맥락 이전에서의 의미 |
|---|---|
| 왜 | 계기 — 없으면 "이걸 왜 지금?"에서 막힌다 |
| 무엇을 | 사실 — 없으면 변경 범위를 각자 추측한다 |
| 어떻게 | 선택 + **버린 선택지** — 없으면 나중에 "왜 이렇게 했지?"가 복원 불가 |
| 확인 방법 | 액션 — 없으면 믿을지 말지 판단할 수 없다 |

따라서 4단을 PR 전용 양식이 아니라 **자리와 무관한 질문 세트**로 일반화하고, PR 은 그 질문 세트가
고정 양식을 만난 경우로 둔다.

### 3.3 로딩 계층으로 나눈다 (CLAUDE.md ↔ 스킬)

스킬 트리거는 확률적이다. `orchestrator` 의 description 이 "리포트 작성"·"보고서로 정리" 같은
문구를 이미 잡고 있어, 사용자가 그렇게 말하면 **위임 판단에서 끝나고 작문 기준은 로드되지 않을**
수 있다. 두 스킬은 경쟁 관계가 아닌데(누가 쓰는가 vs 어떻게 쓰는가) 트리거 표면이 겹친 것이다.

스킬끼리 서로를 참조해 푸는 대신 **로딩 계층을 나눈다**:

| 소유 | 내용 | 로딩 |
|---|---|---|
| `templates/claude-md/CLAUDE.md` | 기준선 — 독자·문체·네 가지(왜/무엇을/어떻게/확인 방법)·대화 잔재 제거·자가 점검 | **항상** |
| `communicate/SKILL.md` | 기준선 요약 + **맥락 이전** + 라우팅 + 체크리스트 | 트리거 시 |
| `communicate/references/writing-standards.md` | 적용법 — 영향 환산 예시·용어 처리·문장/어미·청중별 우선순위 | 문장을 고칠 때 |
| `communicate/references/delivery-context.md` | 놓을 자리의 성질 축 | 자리가 정해졌을 때 |
| `git/references/cli-reference.md` | PR 본문의 **섹션 4단 고정** (양식) | PR 작성 시 |

기준선이 항상 로드되므로 스킬이 안 불려도 최소 품질이 보장되고, 스킬은 깊은 기준만 담아 가벼워진다.
CLAUDE.md 의 orchestrator 항목에도 "위임하더라도 작문 기준은 이 문서를 따른다"를 명시해 두 축을
분리했다.

### 3.4 측정으로 확인한 트리거 경합

3.3 의 경합 가설을 실측했다. 맥락을 심은 프로젝트 루트에 `communicate`·`orchestrator`·
`spec-write`·`git` 을 동시 등록하고 `claude -p` 를 95회 돌려 **발동한 스킬 전체를 순서대로**
셌다 (`tools/skilleval`).

| 스킬 | 발동 | 그중 먼저 발동 |
|---|---|---|
| `communicate` | 71 | 54 |
| `git` | 23 | 17 |
| `orchestrator` | **0** | 0 |
| `spec-write` | **0** | 0 |

**3.3 이 걱정한 경합은 일어나지 않는다.** `orchestrator` 는 95회 중 한 번도 발동하지 않았다.
"리포트 작성"·"보고서로 정리" 로 트리거 표면이 겹칠 것이라는 예상은 실제 동작으로 뒷받침되지
않으므로, 이 걱정 때문에 문구를 조정할 이유는 없다.

`git` 과의 관계는 경합이 아니라 **병용**이다. "PR 설명 써줘" 를 15회 반복하면 두 스킬이
**15회 전부 함께** 발동하고, `git` 이 먼저인 경우가 11회다 — `git` 이 본문 양식을 잡고
`communicate` 가 작문 기준을 대는, 3.3 이 의도한 계층 그대로다. 발동 순서만 보고 "뺏겼다" 고
읽으면 정반대 결론이 나온다.

발동하면 안 되는 요청(버그 수정·테스트 추가)에서 `communicate` 오발동은 20회 중 0회다.

description 압축(#850, 909자 → 623자)은 발동률에 측정 가능한 차이를 만들지 않았다 — 단독
27/30 vs 26/30, 경쟁 27/30 vs 29/30 으로 방향이 일정하지 않고, n=5 에서 보이던 쿼리별 차이는
n=15 에서 사라진다.

> skill-creator 의 `run_eval.py` 로 낸 수치는 이 스킬군에 무효다. 빈 디렉토리에서 맥락 없는
> 일회성 프롬프트를 주므로 옮겨 적을 대상이 없고, Claude 가 "무엇을 정리할까요" 로 되묻고 끝나
> 스킬을 부를 자리에 도달하지 못한다. 이 계열을 측정하려면 맥락을 먼저 심어야 한다.

### 3.5 새로 들어간 것

- **맥락 이전**: 옮겨야 할 네 가지 + **대화 잔재 제거**(지시어·별명·합의된 전제·시간 순서 서술)
  + 자가 점검 질문 — "이 문장을 이해하려면 우리 대화를 봤어야 하는가?"
- **읽기 수준**: 용어를 *설명*하지 말고 **영향으로 환산**한다. 3단계 예시로 고정.
- **전달 맥락 적응**(`references/delivery-context.md`): 도구 이름 대신 **성질 축**으로 기술한다 —
  휘발성(흘러감/남음) · 동시성(되물을 수 있는가) · 응답 기대(통보/결정/검토/기록) · 독자 범위 ·
  양식 강제 · 서식 표현력. 각 축이 길이·구조·서식·질문 위치의 무엇을 바꾸는지를 표로 고정하고,
  판단 순서와 조합 예시, **자리 선택 자체가 틀리는 경우**를 덧붙인다.

  > 도구 이름을 문서에 박지 않는 이유는 모델명을 박지 않는 것과 같다 — 도구는 바뀌고 성질은
  > 바뀌지 않는다. 다만 **스킬 description 의 트리거 예시 문구**는 사용자가 실제로 쓰는 말이어야
  > 매칭되므로 제품명이 남을 수 있다 (지식이 아니라 트리거 표면이다).

---

## 4. 변경 파일

| 파일 | 변경 |
|---|---|
| `skills/spec-review/` | **삭제** (SKILL + references 5, 551줄) |
| `agents/spec/*` | **삭제** (전용 에이전트 4개) |
| `.claude-plugin/plugin.json` | agents 배열에서 4개 제거 |
| `tools/validate/extraction-invariants.json` | `spec/review` 도메인 2개 제거 (12 → 10) |
| `skills/report-write/` → `skills/communicate/` | 이름·축 변경 + 맥락 이전·읽기 수준 추가 |
| `skills/communicate/references/delivery-context.md` | **신규** — 성질 축별 적응 기준 |
| `skills/communicate/references/writing-standards.md` | **신규** — 적용 상세 (SKILL.md 에서 분리) |
| `skills/git/references/cli-reference.md` | communicate 와의 단일 출처 분할 명시 |
| `templates/claude-md/CLAUDE.md` | 기준선을 항상 로드되는 계층에 배치 (§3.3) + orchestrator 항목에 작문 기준 분리 명시 |
| `skills/spec-write/` · `skills/grill/` · `skills/agent-design-principles/` | spec-review 참조 제거 |
| `skills/orchestrator/references/spec-driven-review.md` | "spec-review 레이어를 빌려온다" 문단 → 게이트 범위 서술로 교체 |
| `README.md` · `plugins/atelier/README.md` | 슬래시 목록·흡수 매핑 갱신 |

---

## 5. 미결 판단 (사용자 확인 필요)

1. **`spec-write` 의 `related_paths`** — 유일한 소비자였던 spec-review 가 사라졌다. 지금은 필드를
   유지하되 근거 문장만 정직하게 고쳤다("스펙을 읽는 사람·에이전트가 코드 영역을 바로 찾게 하는
   힌트"). 소비자 없는 규약을 지운다는 원칙을 그대로 적용하면 **필드 자체를 빼는 것**도 선택지다.
2. **`external-llm` 의 `/spec-review`** — 이름만 같은 별개 기능이라 유지했다 (§2.4).
3. **spec 파이프라인 구성** — `grill`(설계 합의) → `spec-write`(문서화)만 남고 검증 단계가 비었다.
   자율 루프의 spec 게이트(`orchestrator/references/spec-driven-review.md`)가 그 자리를 대신하는
   구성이 맞는지 확인이 필요하다.

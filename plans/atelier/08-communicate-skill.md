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
| FR-6 | 채널(Slack·Discord / Notion·Confluence / PR·이슈 / 이메일·보고서)별 적응을 다룬다 |
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

### 3.2 PR 본문 규칙을 채널 무관 기본값으로 승격

이 repo 는 이미 PR 본문에 대해 답을 갖고 있었다 — 독자는 IT 특성화 고등학생 수준, 친근한 해요체
단문, 개조식 우선, 한국어. 이 넷은 PR 에만 유효한 규칙이 아니라 **어느 채널에서나 통하는 기본값**
이므로 그대로 승격한다.

더 중요한 건 **토스 4단(왜 / 무엇을 / 어떻게 / 확인 방법)이 맥락 이전의 4요소와 같다**는 점이다:

| 4단 | 맥락 이전에서의 의미 |
|---|---|
| 왜 | 계기 — 없으면 "이걸 왜 지금?"에서 막힌다 |
| 무엇을 | 사실 — 없으면 변경 범위를 각자 추측한다 |
| 어떻게 | 선택 + **버린 선택지** — 없으면 나중에 "왜 이렇게 했지?"가 복원 불가 |
| 확인 방법 | 액션 — 없으면 믿을지 말지 판단할 수 없다 |

따라서 4단을 PR 전용 양식이 아니라 **채널 무관 질문 세트**로 일반화하고, PR 은 그 질문에 대한
"섹션 이름이 고정된 채널 특수화"로 둔다.

### 3.3 단일 출처 분할 (git ↔ communicate)

| 소유 | 내용 |
|---|---|
| `git/references/cli-reference.md` | PR 본문의 **섹션 4단 고정** (양식) |
| `communicate/SKILL.md` | 그 안을 채우는 **작문 기준** (독자 수준·맥락 이전·용어·문체) |

양쪽에 교차 참조를 달아 중복 서술을 막는다.

### 3.4 새로 들어간 것

- **맥락 이전**: 옮겨야 할 네 가지 + **대화 잔재 제거**(지시어·별명·합의된 전제·시간 순서 서술)
  + 자가 점검 질문 — "이 문장을 이해하려면 우리 대화를 봤어야 하는가?"
- **읽기 수준**: 용어를 *설명*하지 말고 **영향으로 환산**한다. 3단계 예시로 고정.
- **채널 적응**(`references/channels.md`): 채널을 고르는 질문 셋(스크롤 의지 / 남는 문서인가 /
  답을 기다리는가)과 채널별 길이·구조·서식·기대 응답, 그리고 **채널 선택 자체가 틀리는 경우**.

---

## 4. 변경 파일

| 파일 | 변경 |
|---|---|
| `skills/spec-review/` | **삭제** (SKILL + references 5, 551줄) |
| `agents/spec/*` | **삭제** (전용 에이전트 4개) |
| `.claude-plugin/plugin.json` | agents 배열에서 4개 제거 |
| `tools/validate/extraction-invariants.json` | `spec/review` 도메인 2개 제거 (12 → 10) |
| `skills/report-write/` → `skills/communicate/` | 이름·축 변경 + 맥락 이전·읽기 수준 추가 |
| `skills/communicate/references/channels.md` | **신규** — 채널별 관습 |
| `skills/git/references/cli-reference.md` | communicate 와의 단일 출처 분할 명시 |
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

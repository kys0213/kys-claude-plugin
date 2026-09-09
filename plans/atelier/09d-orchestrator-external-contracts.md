# D — orchestrator 재구조화 외부 계약 / 사이드이펙트 조사

> **09 설계의 입력 자료.** 2026-09-07 시점 orchestrator 스킬(commit `ccd6b94` 기준)의 스냅샷 기록이며 이후 갱신하지 않는다.
> 기계 파서 입력으로 쓰지 않는다 — 근거는 [`09` §9.2](09-orchestrator-graph-restructure.md) 참조.
> **조사 관점**: 재구조화가 건드리면 깨지는 외부 계약 — 다른 스킬·에이전트의 참조, 파일명·절 이름·용어, CI validator 와 매니페스트 — 을 조사했다.
> 본문은 조사 원문 그대로다.

조사 대상 브랜치: `claude/atelier-plugin-github-issue-tazv6a` (수정 없음, read-only)
증거: 모두 `grep -n` / 파일 열람으로 확인한 `파일:행`.

기준 현황 (참고):
- `plugins/atelier/skills/orchestrator/SKILL.md` = 230줄, `references/` 10개 = 2,637줄 (합 2,867줄)
- 현재 `validate --skip-versions` 결과: **Passed 163 / Warnings 0 / Failed 0** (직접 실행 확인)

---

## 1. 외부 참조 표

### 1-A. orchestrator 의 **파일 + § 절 이름**을 가리키는 참조 (재구조화로 가장 잘 깨지는 것) — 5건

| 참조하는 곳 (파일:행) | 가리키는 orchestrator 대상 | 대상 실재 확인 |
|---|---|---|
| `plugins/atelier/skills/grill/SKILL.md:50` | `references/autonomous-driving.md §spec 확정 게이트` | 있음 — `autonomous-driving.md:350` `## spec 확정 게이트 (Spec Resolution Gate — hard stop)` |
| `plugins/atelier/skills/grill/SKILL.md:52` | `references/architect-council.md §대화 스킬의 자율 어댑테이션` | 있음 — `architect-council.md:57` `### 대화 스킬의 자율 어댑테이션` |
| `plugins/atelier/skills/grill/SKILL.md:70` | `references/architect-council.md §설계 승인 마커` + "설계 승인 마커" 용어 | 있음 — `architect-council.md:118` `## 설계 승인 마커 (implementer dispatch 게이트의 계약)` |
| `plugins/atelier/skills/git/SKILL.md:124` | `references/merge-coordinator.md §머지 대상: epic 브랜치가 canonical` | 있음 — `merge-coordinator.md:16` `## 머지 대상: epic 브랜치` |
| `plugins/atelier/skills/orchestrator/SKILL.md:186~199` (자체 References 표) | 자기 references 10개 + `§경로 판정 경계 케이스`·`§경로 전환`·`§공유 전제 preflight`·`§탐색 예산`·`§병렬 vs 순차 결정 트리`·`§team mode 강제 등급`·`§근본원인 swarm`·`§모델 선택`·`§spec 확정 게이트` | 스킬 **내부** 계약이지만 재구조화 시 함께 갱신 필수 |

> 추가: `references/delegation-patterns.md:354` 가 `§필수 포함 요소 11번` 을 자기 파일 내부(`delegation-patterns.md:47`)로 참조. `references/spec-driven-review.md:?` 는 `autonomous-driving.md §spec 확정 게이트` 를 참조 (SKILL.md:199 References 표에도 동일 참조). 이는 스킬 내부 cross-reference — 파일 병합/분할 시 함께 깨짐.

### 1-B. orchestrator 를 **파일 단위**로만 가리키는 참조 (절 이름 없음) — 3건

| 참조하는 곳 | 가리키는 대상 |
|---|---|
| `plugins/atelier/skills/git/references/conflict-resolution.md:3` | `orchestrator` skill 의 `references/merge-coordinator.md` 가 canonical |
| `plugins/atelier/skills/git/SKILL.md:184` | `orchestrator` skill 의 `references/merge-coordinator.md` 가 canonical |
| `plugins/atelier/skills/agent-design-principles/SKILL.md:142` | "orchestrator 의 `references/` 프로토콜 문서" (Protocol Skill 예시) |

### 1-C. orchestrator 를 **skill 이름/역할**로만 가리키는 참조 (파일 경로 없음) — 20건

| 참조하는 곳 | 내용 |
|---|---|
| `plugins/atelier/skills/grill/SKILL.md:79` | 연계 skill 표 — orchestrator 가 합의된 작업을 분해·실행 |
| `plugins/atelier/skills/grill/references/design-generation.md:36,52,68,134` | 구현 전환 시 orchestrator 위임 (52·68은 코드펜스 안 graphviz 노드 이름) |
| `plugins/atelier/skills/spec-write/SKILL.md:11,32,34,41,47` | 실제 Write 는 orchestrator 위임 sub-agent 가 수행 (§32 는 절 제목 `## 위임 흐름 (orchestrator 연계)`) |
| `plugins/atelier/skills/spec-write/references/authoring.md:17` | 승인 후 orchestrator 위임 sub-agent 가 Write |
| `plugins/atelier/skills/communicate/SKILL.md:3` (frontmatter description), `:13` | "orchestrator 와 함께 씁니다" / 실제 Write 는 orchestrator 위임 |
| `plugins/atelier/skills/agent-design-principles/SKILL.md:80` | 모델 배정 기준·재평가 절차는 **orchestrator 가 canonical** (→ `references/model-routing.md` 내용) |
| `plugins/atelier/skills/agent-design-principles/SKILL.md:141` | user-invocable Skill 예시 목록에 orchestrator |
| `plugins/atelier/templates/claude-md/CLAUDE.md:9,15,16,19` | 모든 실작업 orchestrator 위임 / 자율주행 모드에서 아키텍트 협의체가 grill 대행 / **에스컬레이션 조건** 용어 |
| `plugins/atelier/templates/spec/spec-concern.md:45` | 미결 남은 spec 은 자율 구현(orchestrator) 입력 불가 — dispatch 전 hard stop |
| `plugins/atelier/rules/policies/spec-writing.md:14` | 동일 (orchestrator 가 구현 dispatch 전 hard stop) |
| `plugins/atelier/README.md:16,20,35,45,101` | 플러그인 목록·슬래시 표·"기본 자율 주행(HITL opt-out)" |
| `.claude-plugin/marketplace.json:11,17`, `plugins/atelier/.claude-plugin/plugin.json:16` | 플러그인 description / keywords 에 `orchestrator` |
| `README.md:47` | `/atelier:orchestrator` 설명 |
| `tools/skilleval/README.md:26,33` | 경쟁 skill 예시로 `plugins/atelier/skills/orchestrator/SKILL.md` 경로 사용 |

### 1-D. 참조가 **없는** 곳 (확인 완료)

- 루트 `CLAUDE.md` — orchestrator 언급 0건
- `.claude/rules/*` — orchestrator 언급 0건
- `plugins/atelier/commands/{setup,update}.md` — orchestrator 언급 0건
- `plugins/atelier/agents/workflow/*.md` (4개) — orchestrator 언급 0건
- `plugins/atelier/skills/workflow/**` — orchestrator 언급 0건
- `plugins/atelier/hooks/`, `output-styles/`, `cli/`, `scripts/` — 0건
- `plugins/atelier/rules/agent-design-principles.md` — orchestrator 언급 0건

### 1-E. 마크다운 링크(`[..](..)`) 형태의 orchestrator 참조

**0건.** 외부 참조는 전부 인라인 코드/산문 형태이므로 CI 의 `markdown-link` 검사(§2)에는 걸리지 않는다 — 즉 **파일을 지워도 CI 는 잡아주지 않는다** (조용히 깨진다).

### 요약 수치

- 외부(orchestrator 스킬 밖) 참조 총 **28건** — 1-A 4건 + 1-B 3건 + 1-C 21건(코드펜스 안 2건 포함)
- 그중 **§절 이름을 직접 지목하는 참조 4건** (`grill/SKILL.md:50,52,70`, `git/SKILL.md:124`)
- 그중 **파일 경로를 직접 지목하는 참조 7건** (1-A 4 + 1-B 3)

---

## 2. validator 제약 표

실행 경로: `.github/workflows/validate.yml:35` → `make validate-ci` → `Makefile:68-69` → `./bin/validate --skip-versions .`
**CI 실패 조건**: `tools/validate/main.go:161-168` — `totalFailed > 0` 일 때만 exit 1. **Warnings 는 CI 를 깨지 않는다.**

| 검사 | 대상 파일 (glob) | 규칙 | 심각도 | 재구조화 시 영향 |
|---|---|---|---|---|
| **spec / skill frontmatter** (`spec/validator.go:81,347`) | `**/skills/*/SKILL.md` — **references/ 미포함** | `name`, `description` 필수 (`parser.ValidateFrontmatter`). 누락 시 Result.Errors → **Failed** | **error** | SKILL.md 를 라우터로 축소해도 frontmatter 2키 유지 필요. `version` 은 validator 가 요구하지 않음(rules 는 요구, §3) |
| **skill 줄수** (`spec/validator.go:351-363`) | `**/skills/*/SKILL.md` | `strings.Count(parsed.Body,"\n")` 기준 **50 미만 → 경고, 500 초과 → 경고**. Body = frontmatter 제외 | **warning** (CI 안 깨짐) | 라우터화로 SKILL.md 가 50줄 밑으로 내려가면 경고. 현재 230줄 |
| **민감데이터** (`spec/validator.go:125`) | `**/scripts/*.sh`, `**/scripts/*.js`, `**/skills/*/SKILL.md` | password/api_key/token/secret 패턴 금지 | error | 영향 없음 |
| **skill reference 경로 존재** (`path/validator.go:38,130-170`) | `**/skills/*/SKILL.md` | 코드상 `references/`·`examples/` 접두 경로의 실존을 검사하게 되어 있으나, 추출기 `parser.ExtractPluginRootPaths` 는 **`${CLAUDE_PLUGIN_ROOT}` 접두 경로만** 반환하고 `validator.go:147-149` 가 그것을 전부 skip → **사실상 dead code, 아무것도 검사하지 않음** | — | **경고: `references/foo.md` 를 지우거나 이름을 바꿔도 CI 가 못 잡는다.** 수동 확인 필요 |
| **markdown 링크 실존** (`path/validator.go:111-125,459`) | `plugins/**/*.md` (**references/ 포함**) | `[텍스트](경로)` 링크의 파일 실존. 코드펜스 안은 skip, `#anchor`·http·`${CLAUDE_PLUGIN_ROOT}` 는 skip. **앵커(`#절이름`)는 잘라내고 파일만 검사 — 절 이름은 검증 안 함** | error | orchestrator 파일을 마크다운 링크로 거는 곳은 현재 0건이라 즉시 영향 없음. 재구조화 후 새 파일을 **링크로** 걸면 그때부터 검사 대상 |
| **layer dependency (upward ref)** (`architecture/dependency.go:39-91,154-238`) | `plugins/*/commands/*.md`, `plugins/*/skills/*/SKILL.md`, `plugins/*/skills/SKILL.md`, `plugins/*/agents/*.md` — **references/ 미포함** (`architecture/validator.go:95-100`) | SKILL.md 안에서 `agents/<name>`, `commands/<name>`, `/plugin:command`, `Task\s*\(.*subagent`, `subagent_type\s*[=:]`, `(에이전트\|agent)\s+(호출\|실행\|call\|invoke\|spawn\|launch)` 매칭 시 **Failed(error)** | **error → CI 실패** | **최대 위험.** 라우터 SKILL.md 에 위임 예시·`Task(...subagent_type=...)` 스니펫을 코드펜스 **밖** 산문으로 옮기면 CI 실패. 회피 수단: 코드펜스 안, 표 행(`\|...\|`), `→`/박스문자 포함 줄, `<!-- arch-ignore -->`, frontmatter `arch-ignore: true` |
| **content-similarity** (`architecture/similarity.go`) | 위와 동일 glob — **references/ 미포함** | 단어 3-gram Jaccard, 임계 **0.30**, 최소 20 n-gram. **같은 plugin 안 + 서로 다른 layer** 쌍만 비교 (`similarity.go:246`). 코드펜스·10자 미만 줄·빈줄 제외 | **warning** | orchestrator SKILL.md 는 skill layer 뿐이므로 **다른 skill 의 SKILL.md 와는 비교되지 않는다** (같은 layer). agent/command 와만 비교 — 실질 위험 낮음 |
| **responsibility (God Skill)** (`architecture/responsibility.go:20-34,59-108`) | 위와 동일 glob — **references/ 미포함** | SKILL.md 에 orchestration 패턴 `(?i)Task\s*\(` · `subagent_type` · `(spawn\|launch\|delegate)\s+(agent\|에이전트\|worker)` · `병렬\s*(실행\|처리\|에이전트)` · `parallel\s+(execution\|agents?\|workers?)`, userInteraction 패턴 `(사용자\|user)\s*(입력\|input\|확인\|confirm)` · `(ask\|prompt)\s+(the\s+)?user` · `argument-hint` · `Magic\s+Keyword` 가 있으면 위반 | **warning** (CI 안 깨짐) | **reference 파일이 검사 대상이 아니기 때문에** 현재 orchestrator references 의 `Task(`·`subagent_type` 이 안 걸린다 (`advisory-consult.md` 1건, `delegation-patterns.md` 1건). SKILL.md 본문에는 현재 0건. 라우터 SKILL.md 에 이 어휘를 산문으로 올리면 warning 발생(치명적이진 않음) |
| **skill-reference (agent frontmatter)** (`architecture/reference.go:10-68`) | `plugins/*/agents/*.md` 만 | agent frontmatter `skills:` 에 선언된 skill 이 같은 plugin 에 `skills/<name>/SKILL.md` 로 실존하는가 | error | **orchestrator 를 `skills:` 로 선언한 agent 없음** → 영향 없음. 단 skill **디렉터리 이름**(`orchestrator`)은 유지해야 안전 |
| **extraction invariant** (`extraction/validator.go` + `tools/validate/extraction-invariants.json`) | search_roots = `plugins/atelier/skills`, `plugins/atelier/commands` 아래 **모든 `.md` (references/ 포함)** 를 하나의 코퍼스로 이어붙여 리터럴 substring 검사 | 아래 10개 토큰이 코퍼스 어딘가에 **최소 1회** 존재해야 함. 없으면 **Failed(error)** | **error → CI 실패** | orchestrator 문서에는 해당 토큰이 없어 직접 영향 없음. 다만 검사 코퍼스에 orchestrator references 가 포함되므로, 토큰을 **우연히 담고 있는 유일한 파일**을 지우면 실패 (아래 목록으로 확인) |

### refactor-invariant 토큰 10개 (`tools/validate/extraction-invariants.json`)

| # | domain | token (리터럴) | reason |
|---|---|---|---|
| 1 | spec-write/write | `## OCP 확장점` | write DESIGN.md 출력 구조 섹션 |
| 2 | spec-write/write | `## 미결정 사항` | write DESIGN.md 출력 구조 섹션 |
| 3 | spec-write/write | `## 관심사 분리` | write DESIGN.md 출력 구조 섹션 |
| 4 | spec-write/write-detail | `## 핵심 로직` | write-detail concerns 출력 구조 |
| 5 | spec-write/write-detail | `## 흐름 다이어그램` | write-detail flows 출력 구조 |
| 6 | spec-write/write-detail | `spec/concerns/` | write-detail 컴포넌트 저장 경로 |
| 7 | spec-write/write-detail | `spec/flows/` | write-detail 플로우 저장 경로 |
| 8 | spec/annotate | `## 에러 처리` | annotate 에러 처리 (매칭 0건/retry) |
| 9 | git/resolve | `diff-filter=U` | rebase --continue 미해결 충돌 가드 |
| 10 | git/resolve | `rebase --abort` | resolve --abort 인자 처리 |

→ 10개 모두 spec-write / git 도메인. **orchestrator 재구조화로 깨질 토큰은 없다.**
→ 단, 새 구조에서 orchestrator 문서를 지울 때 위 토큰을 담고 있지 않은지만 확인하면 됨.

### reference 파일(`references/*.md`)의 검사 대상 여부 — 결론

| 검사 | references/*.md 대상? | 근거 |
|---|---|---|
| skill frontmatter / 줄수 / 민감데이터 | **아니오** | `spec/validator.go:81` glob `**/skills/*/SKILL.md` |
| layer dependency / similarity / responsibility / skill-reference | **아니오** | `architecture/validator.go:95-100` 의 patterns 맵에 references 없음 |
| markdown 링크 실존 | **예** | `path/validator.go:111` glob `plugins/**/*.md` |
| extraction invariant 코퍼스 | **예** (읽기만) | `extraction/validator.go:98-121` 은 search_root 아래 모든 `.md` walk |
| skill reference 경로 실존 | 대상이었어야 하나 **dead code** | `path/validator.go:147-152` |

→ **references 파일은 줄수 상한도, 500줄 제한도, responsibility 정규식도 적용받지 않는다.** 현재 `autonomous-driving.md` 564줄 / `delegation-patterns.md` 461줄이 경고 없이 통과하는 이유.

---

## 3. rules 제약

### `.claude/rules/plugin-skill.md` (paths: `**/skills/**/SKILL.md` — SKILL.md 만, references 미적용)

| 행 | 규정 |
|---|---|
| `:2-3` | `paths: - "**/skills/**/SKILL.md"` — 이 rule 은 SKILL.md 편집 시 자동 주입 |
| `:12` | **Frontmatter 필수**: `name`, `description`, **`version`** 을 반드시 설정 (validator 는 version 을 요구하지 않음 — rules 가 더 엄격) |
| `:13` | **섹션 체계**: 원칙 → 프로세스 → **예시** → 출력 형식 순서로 구조화 |
| `:17-47` | DO 예시 — frontmatter + 번호 있는 섹션 + 표 + `## 출력 형식` |
| `:49-58` | DON'T — frontmatter 누락, 섹션 체계 없는 나열 |
| `:60-65` | validate 요구사항 재서술: name/description 필수, **콘텐츠 최소 50줄 이상 최대 500줄 이하**, 민감 데이터 금지 |
| `:69-73` | 체크리스트 — version 포함 3키 / 단일 도메인 / 동적 정보 없음 / **50~500줄** / 민감 데이터 없음 |

→ **mermaid 언급 없음.** reference 분리에 대한 규정도 **없음** (이 rule 은 SKILL.md 형식만 다룬다).
→ 재구조화 충돌 지점: `:13` "원칙 → 프로세스 → 예시 → 출력 형식" 섹션 순서와 `:72` "50줄 이상" 이 **라우터형 SKILL.md** 와 정면 충돌 가능.

### `.claude/rules/agent-design-principles.md` (paths: `**/plugin.json`)

| 행 | 규정 |
|---|---|
| `:8` | 설계 원리의 단일 출처는 `agent-design-principles` **skill**, rule 은 참조만 |
| `:12` | 새 Command/Agent/Skill 은 skill 의 레이어 매핑·안티패턴 체크리스트·진입점 분류(**§3.6**)·지식 자리(**§3.5**) 를 따른다 |
| `:13` | 명세 형식 규칙은 `plugin-skill.md`/`plugin-command.md`/`plugin-agent.md` |

### `plugins/atelier/rules/agent-design-principles.md` (paths: `{commands,agents,skills}/**/*.md` — **references/*.md 도 매칭**)

| 행 | 규정 |
|---|---|
| `:3-4` | `paths: ".claude/{commands,agents,skills}/**/*.md"`, `"{commands,agents,skills}/**/*.md"` — orchestrator references 편집 시에도 주입됨 |
| `:14-18` | 명세 편집 시 `agent-design-principles` skill 로드 / 레이어 매핑·안티패턴 통과 / 진입점 분류 **§3.6** / 항상 적용되는 지식은 skill 아닌 CLAUDE.md·rule **§3.5** |

### `plugins/atelier/skills/agent-design-principles/SKILL.md`

| 행 | 규정 |
|---|---|
| `:88` | `### 3. Skill = SRP (단일 책임 원칙)` |
| `:92-96` | `#### Skill 폭발 경고` — 과도 분리 시 Context 낭비 (Class Explosion 유비) |
| `:98-108` | `#### 적정 분리 판단 기준` — "2개 이상 Command/Agent 에서 참조되는가" / "관심사가 둘 이상인가" |
| `:110` | **"줄 수 자체는 분리 판단 기준이 아니다 — 관심사가 하나면 길어도 유지한다. SKILL.md 파일 자체의 길이 제약(50~500줄)은 CI `validate` 가 별도로 강제한다."** |
| `:114-134` | `### 3.5 Skill vs Rule 경계` — skill=암묵지, rule=컨벤션. rule 은 skill 을 참조만 하고 재서술 금지 |
| `:136` | `### 3.6 진입점: Command vs user-invocable Skill` |
| `:141` | user-invocable Skill 목록에 orchestrator |
| `:142` | **Protocol Skill** = "특정 커맨드/상위 skill 이 내부 디스패치하는 절차 본문. 예: **orchestrator 의 `references/` 프로토콜 문서**" — orchestrator references 의 성격 규정 |
| `:143` | Reference Skill 정의 (단독으로 읽어도 의미 있는 durable 지식) |
| `:155-173` | `## 토큰 관리` — 전략 1 Glob 경로만 수집→sub-agent 가 읽기, 전략 2 결정적 로직은 스크립트, 전략 3 CLAUDE.md 는 정적 원칙만 |

→ **mermaid 언급 없음.** "예시/다이어그램 형식"에 대한 규정 없음.
→ `:110` 은 재구조화에 **우호적** — "결정 1개 = 파일 1개" 분리 근거로 §98-110 의 "관심사가 둘 이상인가" 기준을 쓸 수 있으나, 동시에 "줄 수 때문에 쪼개지 말 것"을 명시하므로 **분할 근거를 관심사로 서술해야 한다.**

---

## 4. mermaid 지원

### 저장소 내 mermaid 블록 실태

- **` ```mermaid ` 코드펜스: 저장소 전체 0건** (`grep -rl '```mermaid' --include=*.md .` → 0 파일)
- "mermaid" 단어 언급 4건:
  - `plugins/atelier/skills/grill/references/design-generation.md:155` — "모듈 구조·의존성·흐름 다이어그램 → markdown 표 / ASCII / mermaid"
  - `plugins/atelier/templates/spec/spec-common.md:37` — "동작 흐름은 ASCII 다이어그램 또는 mermaid 로"
  - `plugins/atelier/templates/spec/spec-common.md:48` — "트리거/처리/응답 흐름 | ASCII flow 또는 mermaid sequence"
  - `plugins/atelier/templates/spec/spec-common.md:50` — "컴포넌트 간 관계 | ASCII 박스 또는 mermaid graph"
- 기존 다이어그램 관행: **graphviz DOT 를 코드펜스 안에** (`grill/references/design-generation.md:52,68`), **ASCII 박스문자** (`orchestrator/SKILL.md` 토폴로지 등)

### validator 의 코드펜스 처리 — 4곳 모두 skip 함

| 검사 | 코드펜스 skip? | 근거 |
|---|---|---|
| responsibility | **예** | `responsibility.go:64-68` (skill), `:118-123` (agent), `:174-181` (command) — ` ``` ` 토글, 내부 continue |
| similarity | **예** | `similarity.go:294-305` `cleanForSimilarity` — ` ``` ` 토글, 내부 continue. 추가로 빈줄·`---`·10자 미만 줄도 제외, 헤더 `#` 는 텍스트만 남김 |
| dependency | **예** | `dependency.go:102-118` — ` ``` ` 토글. 게다가 펜스 라벨에 `example`/`예시` 가 있으면 `inExampleBlock` 로 추가 보호 |
| markdown-link | **예** | `path/validator.go:471-478` — ` ``` ` 토글 |
| extraction invariant | **아니오** | `extraction/validator.go:109-114` — 파일 전체 바이트를 코퍼스에 append. 리터럴 substring 검사라 펜스 무관 |
| path `${CLAUDE_PLUGIN_ROOT}` 추출 | 함수에 따라 다름 | `parser/markdown.go:93-101` — `ExtractPluginRootPathsSkipCode` 만 skip. skill 검사는 skip 하지 않는 쪽을 씀(단 dead code) |

**결론: ` ```mermaid ` 블록은 responsibility·similarity·dependency·markdown-link 검사에서 전부 무시된다.** mermaid 안에 `Task(`, `subagent_type`, `병렬 실행`, `agents/foo` 를 써도 CI 위반이 되지 않는다 — 오히려 재구조화에 **안전한 그릇**이다.
단, mermaid 로 옮긴 텍스트는 similarity 계산에서도 빠지므로 중복 탐지 효력이 사라지는 부수효과가 있다.

---

## 5. skilleval 도구

`tools/skilleval/` — `main.go` + `internal/{evalset,runner,scoring,skill}`, `examples/communicate.json`

| 항목 | 내용 (근거) |
|---|---|
| 무엇을 측정하나 | **스킬 description 이 실제 발동을 만들어내는가, 경쟁 스킬 중 어느 쪽이 앞서는가** (`README.md:3`). `claude -p` 를 실제 호출 (`README.md:5`) |
| 입력 | eval set JSON — `query` / `expect`(true·false) / `files`(프롬프트 전 프로젝트 루트에 배치) (`README.md:56-68`) |
| 읽는 대상 | **SKILL.md frontmatter 의 `name`·`description` 만** (`internal/skill/skill.go:29-31` 주석 "reads name and description straight out of the frontmatter"). **본문·references 는 읽지 않는다** |
| 출력 지표 | `should fire` / `should not fire` 비율, `led X:n`, `with-others n`, `no-fire n`, `timeout n` (`README.md:39-49`) |
| orchestrator 트리거 케이스 등록? | **아니오.** `examples/` 에는 `communicate.json` 한 개뿐. orchestrator 는 `README.md:26` 의 **경쟁자(competitor) 예시**로만 등장 — 전용 eval set 없음 |
| CI 연동 | **없음** — "수동 실행 전용, CI 에 넣지 않는다" (`README.md:5-6`). `Makefile:1,18` 에 `skilleval` 타깃만 존재, `validate.yml` 에는 없음 |
| 재구조화 회귀 검증에 쓸 수 있나 | **부분적으로만.** description 을 바꾸지 않으면 결과 불변(본문 미참조) → **본문 재구조화의 회귀는 잡지 못한다.** description 을 함께 손보는 경우엔 유효한 before/after 측정 수단. 쓰려면 orchestrator 용 eval set(`expect:false` 케이스 포함 필수, `README.md:67`)을 새로 만들어야 함. 실행마다 흔들리므로 `--runs 15` 이상 권고 (`README.md:78-79`) |

---

## 6. "깨지면 안 되는 것" 요약

### 하드 (CI 를 실제로 깨뜨림 — error)

1. **SKILL.md frontmatter `name`, `description`** — `spec/validator.go:347`. `name: orchestrator` 값도 유지 (skilleval·agent `skills:` 참조 관례).
2. **SKILL.md 안 upward reference 금지** — `dependency.go:154-238`. 라우터 SKILL.md 산문에 `agents/<name>`, `commands/<name>`, `/atelier:xxx`, `Task(...subagent`, `subagent_type=`, `에이전트 spawn/호출` 을 **코드펜스·표 밖**에 두지 말 것. 필요하면 ` ```mermaid ` / 표 행 / `<!-- arch-ignore -->` 안으로.
3. **extraction invariant 토큰 10개** — 전부 spec-write·git 도메인이라 orchestrator 만 건드리면 안전. 파일 삭제 시 `## OCP 확장점` 등 10개 토큰을 담고 있지 않은지만 확인.
4. **마크다운 링크로 새 파일을 걸 경우 그 경로가 실존해야 함** — `path/validator.go:459`. (현재 링크 참조 0건)
5. **skill 디렉터리 이름 `orchestrator` 유지** — `plugins/atelier/skills/orchestrator/SKILL.md` 경로가 marketplace keyword(`.claude-plugin/marketplace.json:17`), skilleval README, `/atelier:orchestrator` 슬래시(`README.md:47`, `plugins/atelier/README.md:35`)의 전제.

### 소프트 (CI 는 통과하나 계약이 조용히 깨짐 — 가장 위험)

6. **§절 이름 4건** — 재구조화로 이 절 이름이 사라지면 **CI 는 잡지 않고** 참조만 죽는다:
   - `references/autonomous-driving.md §spec 확정 게이트` ← `grill/SKILL.md:50`
   - `references/architect-council.md §대화 스킬의 자율 어댑테이션` ← `grill/SKILL.md:52`
   - `references/architect-council.md §설계 승인 마커` ← `grill/SKILL.md:70`
   - `references/merge-coordinator.md §머지 대상` ← `git/SKILL.md:124`
7. **파일 경로 3건** — `references/merge-coordinator.md` 를 `git/references/conflict-resolution.md:3`, `git/SKILL.md:184` 가 "canonical" 로 지목. 파일명을 바꾸면 두 곳을 함께 고쳐야 함. `references/` **디렉터리 존재 자체**를 `agent-design-principles/SKILL.md:142` 가 Protocol Skill 예시로 지목.
8. **용어 계약** — 다음 용어는 외부에서 그대로 쓰이므로 새 구조에서도 검색 가능해야 한다: **설계 승인 마커**(`grill:70`), **spec 확정 게이트 / hard stop**(`grill:50`, `templates/spec/spec-concern.md:45`, `rules/policies/spec-writing.md:14`), **자율 어댑테이션**(`grill:52`), **에스컬레이션 (조건)**(`templates/claude-md/CLAUDE.md:15`), **머지 대상: epic 브랜치가 canonical**(`git:124`), **기본 자율 주행 / HITL opt-out**(`plugins/atelier/README.md:35,101`), **아키텍트 협의체**(`templates/claude-md/CLAUDE.md:15`), **모델 배정 기준·재평가 절차 canonical**(`agent-design-principles/SKILL.md:80`), **필수 포함 요소**(`references/delegation-patterns.md:354` 자체 참조).
9. **`references/` 파일 실존을 검증하는 CI 가 없다** (`path/validator.go:147-152` dead code) — 파일 이름 변경·삭제는 **수동 grep 으로만** 안전을 확보할 수 있다.

### 권고 (제약은 아니나 마찰)

10. **SKILL.md 50줄 하한** (`spec/validator.go:352`, `.claude/rules/plugin-skill.md:64,72`) — 라우터화로 50줄 미만이 되면 warning(CI 통과) + rule 체크리스트 위반. 라우터에 트리거 케이스·References 표를 남겨 50줄 이상 유지 권장.
11. **`.claude/rules/plugin-skill.md:13`** 의 "원칙 → 프로세스 → 예시 → 출력 형식" 섹션 순서 — 순수 라우터 구조와 충돌. rules 를 함께 갱신하거나 라우터에 최소 원칙 절을 남길지 결정 필요.
12. **`.claude/rules/plugin-skill.md:12`** 는 `version` frontmatter 를 필수로 요구 (validator 는 미요구). orchestrator SKILL.md 는 현재 `version: 0.1.0` 보유 — 유지.
13. **references 파일에는 줄수·responsibility 제약이 없다** — "결정 1개 = 파일 1개" 로 잘게 쪼개도 CI 상 페널티 없음. 반대로 references 를 SKILL.md 로 끌어올리면 그 순간 responsibility·dependency·줄수 검사 대상이 된다.
14. **mermaid 는 안전** — 저장소에 선례는 없으나(0건) validator 4종이 코드펜스를 skip 하므로 오히려 위험 어휘를 담기에 가장 안전한 그릇. spec 템플릿(`templates/spec/spec-common.md:37,48,50`)이 이미 mermaid 를 권장 표현으로 인정.
15. **skilleval 로는 본문 재구조화 회귀를 못 잡는다** (description 만 읽음). description 을 손대지 않는다면 트리거 회귀 위험도 없다 — 반대로 description 을 손대면 orchestrator 전용 eval set 을 새로 만들어 before/after 를 재야 한다.

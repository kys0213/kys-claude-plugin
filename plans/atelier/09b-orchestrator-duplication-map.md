# orchestrator 스킬 — 중복·참조 지도 (조사 B)

> **09 설계의 입력 자료.** 2026-09-07 시점 orchestrator 스킬(commit `ccd6b94` 기준)의 스냅샷 기록이며 이후 갱신하지 않는다.
> 기계 파서 입력으로 쓰지 않는다 — 근거는 [`09` §9.2](09-orchestrator-graph-restructure.md) 참조.
> **조사 관점**: 같은 내용이 여러 파일에 흩어진 중복과 파일 간 `§` 참조를 기계 추출해 중복 지도 · 모순(X1–X13) · 깨진 참조를 만들었다.
> 본문은 조사 원문 그대로다.

- 대상: `plugins/atelier/skills/orchestrator/` (SKILL.md + references/ 10개, 총 2,867줄)
- 브랜치: `claude/atelier-plugin-github-issue-tazv6a` (읽기 전용 조사, 수정 없음)
- 방법: 전체 11개 파일 통독 + `§` 참조 기계 추출(`xref.py`) + heading 대조 검증
- 모든 위치는 `파일:행` (파일 경로는 `plugins/atelier/skills/orchestrator/` 기준 상대)

## 0. 규모 요약

| 지표 | 값 |
|---|---|
| 파일 수 | 11 (SKILL.md + reference 10) |
| 총 줄 수 | 2,867 |
| `§` 절 참조 총수 | **369** (교차 파일 233 / 같은 파일 자기참조 136) |
| "단일 출처"/"단일 소유" 문구 출현 | **100** (96 + 4) |
| 절 이름이 대상 heading과 문자열로 안 맞는 참조 | **83** (아래 §1.3에서 분류 — 진짜 깨짐 5, 번호 프리픽스 드리프트 26, 외부 스킬 참조 14, 파서 잡음 38) |
| 파일 간 참조 엣지(방향쌍) | 58 |

---

## 1. 교차 참조 그래프

### 1.1 파일 단위 집계 (절 참조 기준)

| 파일 | 나가는 참조(out) | 들어오는 참조(in) | 자기참조(self) | 줄수 |
|---|---|---|---|---|
| SKILL.md | 56 | 55 | 19 | 230 |
| delegation-patterns.md | 31 | **62** | 30 | 461 |
| autonomous-driving.md | 45 | 42 | 12 | 564 |
| branch-strategy.md | 13 | 26 | 19 | 226 |
| merge-coordinator.md | 15 | 10 | 7 | 286 |
| architect-council.md | 16 | 4 | 6 | 174 |
| advisory-consult.md | 14 | 8 | 13 | 283 |
| agent-monitor.md | 8 | 8 | 17 | 266 |
| model-routing.md | 9 | 9 | 1 | 71 |
| worktree-lifecycle.md | 12 | 8 | 4 | 166 |
| spec-driven-review.md | 14 | **1** | 6 | 140 |

읽는 법:
- **delegation-patterns.md(in 62) + SKILL.md(in 55) + autonomous-driving.md(in 42)** 셋이 전체 in-degree의 68%를 먹는다 — 사실상 이 3개가 허브다.
- **spec-driven-review.md는 in=1**(autonomous-driving.md:220에서만 지목) — 거의 잎(leaf) 노드. 반대로 out=14로 자기가 참조만 뿌린다.
- **model-routing.md는 71줄인데 in=9 out=9** — 문서 크기 대비 참조 밀도가 가장 높다(줄당 0.25). 사실상 "포인터 문서".
- SKILL.md ↔ delegation-patterns.md 는 **양방향 44회**(28 + 16)로 최대 결합. 두 파일이 하나의 규칙 집합을 요지/본문으로 쪼개 갖고 있다는 신호.

### 1.2 주요 엣지 (src → dst, 5회 이상)

```
SKILL.md               -> delegation-patterns.md    28
autonomous-driving.md  -> SKILL.md                  16
delegation-patterns.md -> SKILL.md                  16
SKILL.md               -> autonomous-driving.md     10
autonomous-driving.md  -> delegation-patterns.md     9
spec-driven-review.md  -> autonomous-driving.md      9
merge-coordinator.md   -> branch-strategy.md         8
architect-council.md   -> autonomous-driving.md      7
delegation-patterns.md -> autonomous-driving.md      6
SKILL.md               -> agent-monitor.md           5
SKILL.md               -> branch-strategy.md         5
architect-council.md   -> delegation-patterns.md     5
model-routing.md       -> SKILL.md                   5
worktree-lifecycle.md  -> branch-strategy.md         5
```

전체 58개 엣지는 §부록 A, 참조 369건 전량은 §부록 B.

### 1.3 깨진 / 위험한 참조

문자열 대조로 걸린 83건을 성격별로 분류했다.

#### (a) 진짜 깨진 참조 — 대상 파일에 그 절이 없다 (5건)

| # | 위치 | 참조 | 실제 |
|---|---|---|---|
| B1 | SKILL.md:183 | `autonomous-driving.md §자율 계약`이 단일 출처 | autonomous-driving.md에 `자율 계약` heading 없음. 내용은 `§기본 동작과 opt-out`(autonomous-driving.md:16) 안의 코드블록(:22–48) |
| B2 | advisory-consult.md:70 | `autonomous-driving.md §자율 계약` | 동일 (B1과 같은 없는 절을 두 파일이 "단일 출처"로 지목) |
| B3 | SKILL.md:219 | 안티패턴 8 — ``(`references/advisory-consult.md §안티패턴` / §team mode 강제 등급)`` | 뒤쪽 `§team mode 강제 등급`은 advisory-consult.md에 없다. 실제 위치는 delegation-patterns.md:318. 파일 표기 생략으로 앞의 파일에 붙어 읽힌다 |
| B4 | model-routing.md:17 | 판정 트리 안 `§자문 조회의 네 트리거` | model-routing.md `§자문 조회 — 상위 tier 예외`(:42)에는 트리거 4개가 없다. 실제 표는 advisory-consult.md:76–81 `§트리거` |
| B5 | delegation-patterns.md:411 | spawn 확인 트리 안 `§진입 시 체크 4의 0·1차를 다시` | 파일 표기 없이 `§`만 써서 delegation-patterns 자기 절로 읽히지만, `진입 시 체크`는 SKILL.md:47/68에만 있다 |

#### (b) 번호 프리픽스 드리프트 — branch-strategy.md 참조 26건

branch-strategy.md의 heading은 전부 번호가 붙어 있는데(`## 2. hot-spot 파일 — disjoint 판정에서 떼어낸다`), 다른 파일들은 번호를 떼고 부른다(`§hot-spot 파일`). 문자열로는 안 맞고 사람만 이어 붙일 수 있다.

- `§hot-spot 파일` → 실제 `2. hot-spot 파일 — …` : SKILL.md:152, SKILL.md:157, delegation-patterns.md:60, delegation-patterns.md:155, worktree-lifecycle.md:55, worktree-lifecycle.md:154
- `§base drift 전파` → 실제 `3. base drift 전파 (in-flight 재동기화)` : delegation-patterns.md:57, worktree-lifecycle.md:26, worktree-lifecycle.md:107, merge-coordinator.md:54, merge-coordinator.md:85, merge-coordinator.md:253, autonomous-driving.md:239, SKILL.md:227
- `§브랜치 네이밍` → 실제 `1. 브랜치 네이밍 (단일 출처)` : delegation-patterns.md:57, worktree-lifecycle.md:27, merge-coordinator.md:58
- `§머지 방식` → 실제 `4. 머지 방식 — rebase 후 fast-forward (확정)` : merge-coordinator.md:72, autonomous-driving.md:239
- `§충돌 반복` → 실제 `6. 충돌 반복 = 재분해 신호` : merge-coordinator.md:78, autonomous-driving.md:239, autonomous-driving.md:545
- `§epic ← main 역방향 drift` → 실제 `5. epic ← main 역방향 drift` : SKILL.md:45, merge-coordinator.md:145, merge-coordinator.md:254
- 자기 문서 안에서도 이름이 흔들린다: branch-strategy.md:18 `§2 hot-spot 분리` vs heading `2. hot-spot 파일` / branch-strategy.md:19–21 `§3 base drift 전파`·`§4 rebase 후 ff`·`§5 역방향 흡수` — 표(:16–21)의 이름과 heading 이름이 3개 다르다

#### (c) 디렉토리 밖(다른 스킬/CLAUDE.md) 참조 — 14건

이 재설계 범위 밖이지만 그래프에 잡히므로 기록한다.

- `git` skill `SKILL.md §열린 PR 최신화 원칙` : SKILL.md:207, autonomous-driving.md:170, :464, :495, :564 (5)
- `git` skill `SKILL.md §PR 단위 원칙` : merge-coordinator.md:24
- `git` skill `§force-push 정책` : branch-strategy.md:155
- `git` skill `references/conflict-resolution.md` : branch-strategy.md:10, :149, :172, merge-coordinator.md:170
- `grill` skill (`skills/grill/references/design-generation.md`, `SKILL.md §미결·의문 해소 게이트`, `§종료와 핸드오프`) : architect-council.md:51, :52, :65, :124
- `CLAUDE.md §Debugging` / `§Debugging 고착 탈출` : delegation-patterns.md:279, :308 — **이 레포의 CLAUDE.md에는 `Debugging` 절이 없다**(현 CLAUDE.md 목차: 설계 최우선 / 문서 계층 / 책임 경계 / 구현 원칙 / 코드 품질 게이트 / PR 타이틀 / CI·CD). 스킬 밖 파일이라 열람하지 않았으므로 "깨짐 유력"으로만 표시

#### (d) 파서 잡음 38건

`(위 §…)` 형태의 자기참조인데 같은 줄 앞쪽 다른 파일명에 붙어 잡힌 것들(예: SKILL.md:193, architect-council.md:22, merge-coordinator.md:103, model-routing.md:67, spec-driven-review.md:59, delegation-patterns.md:344), 그리고 `§` 뒤 문장이 길게 이어져 절 이름 경계가 없는 것들(agent-monitor.md:147/:214, autonomous-driving.md:386/:397 등). **참조 자체는 유효하지만, "위/아래 §"처럼 상대 지시로만 쓰인 참조가 38건**이라는 사실은 파일을 쪼갤 때 전부 깨진다는 뜻이다.

---

## 2. 재요약 중복 (같은 규칙이 둘 이상의 파일에 실질적으로 같은 내용)

`요지 ↔ 본문` 구조가 의도된 것(SKILL.md의 "(요지)" 3절)뿐 아니라, 안티패턴 목록·체크리스트가 같은 규칙을 3~6곳에서 다시 진술한다. 아래 **31쌍/군**. 각 항목은 "같은 규칙"이며 표현만 다르다.

| # | 규칙 (한 줄) | 위치들 |
|---|---|---|
| D1 | team 비가용 = 자문 경로 차단, 무엇으로도 대체하지 않고 원래 에스컬레이션으로 | SKILL.md:183 ↔ advisory-consult.md:24–26 ↔ advisory-consult.md:44–63(게이트 0 트리) ↔ model-routing.md:61 ↔ delegation-patterns.md:331 ↔ autonomous-driving.md:32–34 |
| D2 | 자문은 team **필수** 등급 — 가용인데 단발 subagent로 대체하면 위반 | SKILL.md:177 ↔ delegation-patterns.md:331 ↔ advisory-consult.md:17–19 ↔ advisory-consult.md:236–239(안티1) ↔ model-routing.md:52 ↔ model-routing.md:57 ↔ SKILL.md:219(안티8) |
| D3 | 협의체도 필수 등급 — 폴백 없이 즉시 에스컬레이션 | architect-council.md:22 ↔ architect-council.md:73 ↔ architect-council.md:152(안티3) ↔ architect-council.md:168(체크) ↔ delegation-patterns.md:332 ↔ SKILL.md:177 |
| D4 | team 가용 판정의 권위 신호 = `SendMessage`로 재지목 가능한가, `name` 없으면 `agentId`, `printenv`는 보조 | SKILL.md:73–95 ↔ SKILL.md:222(안티11) ↔ delegation-patterns.md:344–346 ↔ delegation-patterns.md:382 ↔ architect-council.md:23 ↔ architect-council.md:152 ↔ advisory-consult.md:256–260(안티9) ↔ agent-monitor.md:37 ↔ autonomous-driving.md:29–31 ↔ autonomous-driving.md:533 |
| D5 | 집행 위임 tier ≤ 메인 tier / 매 dispatch `model` 명시(상속 금지) / 문서에 모델명 안 박음 | SKILL.md:182 ↔ model-routing.md:12–38 ↔ delegation-patterns.md:424 ↔ delegation-patterns.md:436 ↔ delegation-patterns.md:456 ↔ autonomous-driving.md:121 ↔ autonomous-driving.md:513(안티11) ↔ model-routing.md:70 |
| D6 | 작업유형→시작 tier 표는 delegation-patterns §모델 선택이 단일 출처(여기서 중복 정의 안 함) | SKILL.md:184 ↔ model-routing.md:40 ↔ architect-council.md:144 ↔ spec-driven-review.md:104 ↔ autonomous-driving.md:121 |
| D7 | teammate는 공유 checkout — 편집·격리는 `isolation:"worktree"` subagent가 보장 | SKILL.md:171 ↔ delegation-patterns.md:350 ↔ delegation-patterns.md:387 ↔ agent-monitor.md:237(안티4) ↔ autonomous-driving.md:145 ↔ autonomous-driving.md:156 ↔ autonomous-driving.md:515(안티13) ↔ spec-driven-review.md:61 ↔ spec-driven-review.md:112(안티3) ↔ advisory-consult.md:93 |
| D8 | 보고 채널: agent의 plain text는 메인에 안 온다 → prompt에 `SendMessage({to:"main"})` 필수 | SKILL.md:225(안티13) ↔ delegation-patterns.md:59(필수요소 11) ↔ delegation-patterns.md:65–74(표) ↔ delegation-patterns.md:385 ↔ agent-monitor.md:39 ↔ agent-monitor.md:247(체크) ↔ autonomous-driving.md:541 |
| D9 | `Bash sleep` 폴링 금지 — 완료 알림 사용 | SKILL.md:217(안티5) ↔ agent-monitor.md:35 ↔ agent-monitor.md:234(안티1) ↔ agent-monitor.md:249(체크) ↔ worktree-lifecycle.md:72 ↔ worktree-lifecycle.md:163(체크) ↔ autonomous-driving.md:507(안티5) |
| D10 | 메인은 편집하지 않는다 / 실패해도 편집권 회수 금지 | SKILL.md:35 ↔ SKILL.md:213(안티1) ↔ merge-coordinator.md:10 ↔ merge-coordinator.md:186 ↔ merge-coordinator.md:245(안티1) ↔ agent-monitor.md:183 ↔ autonomous-driving.md:478 ↔ spec-driven-review.md:112 |
| D11 | 검토·QA(+DBA)는 AND 게이트, 전부 pass여야 머지, 게이트는 구현자와 다른 agent | SKILL.md:138 ↔ autonomous-driving.md:214 ↔ autonomous-driving.md:218 ↔ autonomous-driving.md:511(안티9) ↔ spec-driven-review.md:44–45 ↔ spec-driven-review.md:111(안티2) ↔ spec-driven-review.md:113(안티4) ↔ spec-driven-review.md:133(체크) |
| D12 | QA의 테스트 추가도 편집이므로 격리 subagent에 위임 | SKILL.md:139 ↔ autonomous-driving.md:216 ↔ spec-driven-review.md:134(체크) |
| D13 | 게이트 거부는 `max_redispatch_per_task`를 소모(게이트 전용 예산 없음) | autonomous-driving.md:222 ↔ autonomous-driving.md:504(안티2) ↔ spec-driven-review.md:75 ↔ spec-driven-review.md:115(안티6) ↔ spec-driven-review.md:137(체크) ↔ delegation-patterns.md:99 |
| D14 | spec 미결(TBD)이 하나라도 있으면 구현 dispatch 금지 — hard stop, 가정·stub·협의체 확정 금지 | SKILL.md:128 ↔ SKILL.md:231(안티19) ↔ autonomous-driving.md:352 ↔ autonomous-driving.md:383–387 ↔ autonomous-driving.md:517(안티15) ↔ autonomous-driving.md:397(seam 선행조건) ↔ spec-driven-review.md:26 ↔ spec-driven-review.md:52 ↔ spec-driven-review.md:118(안티9) ↔ architect-council.md:43 ↔ architect-council.md:64 ↔ architect-council.md:172(체크) |
| D15 | 병렬/순차: disjoint+의존성 없음 → 병렬, 의심스러우면 순차 | SKILL.md:157 ↔ SKILL.md:214(안티2) ↔ delegation-patterns.md:146–164 ↔ worktree-lifecycle.md:54 ↔ worktree-lifecycle.md:139(안티2) ↔ worktree-lifecycle.md:155(체크) |
| D16 | 겹치는 파일이 전부 hot-spot이면 병렬 유지 + 통합 task 1개로 분리 | SKILL.md:152 ↔ SKILL.md:157 ↔ branch-strategy.md:68–79 ↔ branch-strategy.md:199(안티2) ↔ delegation-patterns.md:154 ↔ worktree-lifecycle.md:54 ↔ worktree-lifecycle.md:154(체크) |
| D17 | hot-spot 편집 금지 계약 + 보고 출구를 prompt에 넣는다 | branch-strategy.md:83–89 ↔ delegation-patterns.md:60(필수요소 12) ↔ delegation-patterns.md:454(체크) ↔ branch-strategy.md:200(안티3) ↔ branch-strategy.md:213(체크) |
| D18 | read-only 조사·감사의 기본값은 관점별 병렬 fan-out(근거 없으면 3관점 이상), 메인 순차 탐색 금지 | SKILL.md:161 ↔ SKILL.md:230(안티18) ↔ delegation-patterns.md:168–176 |
| D19 | 조사형 dispatch에는 탐색 예산을 숫자로 명시, 소진 시 중간 산출물 3종 | SKILL.md:132 ↔ SKILL.md:161 ↔ delegation-patterns.md:104–111 ↔ delegation-patterns.md:175 |
| D20 | 증거 계약 — 명령 원문 + `file:line`, 못 찾으면 `NOT FOUND`, 부재 주장은 교차 검증 | SKILL.md:131 ↔ delegation-patterns.md:61(필수요소 13) ↔ delegation-patterns.md:95–102 ↔ delegation-patterns.md:123 ↔ delegation-patterns.md:294–298(swarm 반환 계약) |
| D21 | 토폴로지 가드: branch == epic + working tree clean, 위반이면 복구 후 hard stop/에스컬레이션 | SKILL.md:218(안티6) ↔ merge-coordinator.md:101–121 ↔ merge-coordinator.md:251(안티7) ↔ worktree-lifecycle.md:102 ↔ worktree-lifecycle.md:143(안티6) ↔ worktree-lifecycle.md:164(체크) ↔ autonomous-driving.md:241–247 ↔ autonomous-driving.md:509(안티7) ↔ advisory-consult.md:101 ↔ advisory-consult.md:276(체크) |
| D22 | 기본은 배치 머지 — 즉시 머지했으면 in-flight 전부에 rebase 전파 | SKILL.md:228(안티16) ↔ branch-strategy.md:97–132 ↔ branch-strategy.md:201(안티4) ↔ merge-coordinator.md:53 ↔ merge-coordinator.md:85 ↔ merge-coordinator.md:253(안티9) ↔ merge-coordinator.md:276(체크) ↔ worktree-lifecycle.md:107 ↔ autonomous-driving.md:239 |
| D23 | 통합은 rebase 후 `merge --ff-only` (ff 실패 = rebase 누락 신호) | SKILL.md:44 ↔ branch-strategy.md:136–152 ↔ branch-strategy.md:202(안티5) ↔ branch-strategy.md:220(체크) ↔ merge-coordinator.md:72 ↔ merge-coordinator.md:273(체크) |
| D24 | 같은 파일 충돌 2회 → hot-spot 재분류/직렬화, 3회 → 에스컬레이션 (파일 단위 카운터, task 예산과 별개) | branch-strategy.md:179–192 ↔ merge-coordinator.md:77–78 ↔ merge-coordinator.md:275(체크) ↔ autonomous-driving.md:235–239 ↔ autonomous-driving.md:545(체크) |
| D25 | 완료 선언 전 epic ← main 역방향 흡수(merge, rebase 아님) 후 스위트 재실행 | branch-strategy.md:160–175 ↔ merge-coordinator.md:145–150 ↔ merge-coordinator.md:254(안티10) ↔ merge-coordinator.md:283(체크) ↔ branch-strategy.md:226(체크) ↔ SKILL.md:45 |
| D26 | worktree dispatch는 한 메시지에 하나씩 + 매 dispatch 직후 생성 가드 | worktree-lifecycle.md:77–96 ↔ worktree-lifecycle.md:144(안티7) ↔ worktree-lifecycle.md:158·162(체크) ↔ autonomous-driving.md:74–76 ↔ autonomous-driving.md:243 ↔ autonomous-driving.md:509(안티7) ↔ autonomous-driving.md:546(체크) |
| D27 | 메인 컨텍스트 격리 — 전문 통독 금지, verdict + 압축 요약만 수령, 근거는 경로로 | SKILL.md:16 ↔ SKILL.md:28 ↔ autonomous-driving.md:127–135 ↔ autonomous-driving.md:514(안티12) ↔ autonomous-driving.md:551(체크) ↔ spec-driven-review.md:65 ↔ spec-driven-review.md:117(안티8) ↔ architect-council.md:97 ↔ architect-council.md:150(안티1) ↔ advisory-consult.md:187 ↔ advisory-consult.md:230 ↔ advisory-consult.md:246(안티5) ↔ delegation-patterns.md:298 |
| D28 | 일감의 Task 분리·상태 관리는 메인 핵심 룰, 단발 1회만 예외 | SKILL.md:30 ↔ SKILL.md:122 ↔ agent-monitor.md:61 ↔ autonomous-driving.md:539(체크) |
| D29 | team은 session 종료 시 자동 정리(`TeamCreate`/`TeamDelete` 제거, `team_name` 무시) | SKILL.md:29 ↔ agent-monitor.md:228 ↔ agent-monitor.md:265(체크) ↔ delegation-patterns.md:344 ↔ delegation-patterns.md:386 ↔ advisory-consult.md:229 |
| D30 | 필수 등급 가드 2개 — spawn 확인 + decision log `실행 형태`·`판정 근거` 필드 | delegation-patterns.md:336 ↔ delegation-patterns.md:390–418 ↔ autonomous-driving.md:310–317 ↔ advisory-consult.md:146–160 ↔ advisory-consult.md:274(체크) ↔ architect-council.md:24 ↔ architect-council.md:91–92 ↔ architect-council.md:169(체크) ↔ SKILL.md:178 |
| D31 | 금지에는 반드시 "대신 무엇을 하라" 출구를 짝짓는다 | SKILL.md:224(안티12) ↔ delegation-patterns.md:58(필수요소 10) ↔ delegation-patterns.md:110 ↔ delegation-patterns.md:123 ↔ delegation-patterns.md:220 ↔ branch-strategy.md:83 |

### 2.1 가장 많이 중복된 규칙 (재진술 지점 수 상위)

1. **D14 spec 미결 → hard stop** — 12지점 / 4파일 (SKILL, autonomous-driving, spec-driven-review, architect-council)
2. **D27 메인 컨텍스트 격리** — 13지점 / 6파일
3. **D21 토폴로지 가드** — 10지점 / 5파일
4. **D4 team 가용 판정 신호** — 10지점 / 6파일
5. **D7 teammate는 편집 안 함 / 격리는 subagent** — 10지점 / 6파일
6. (동률) **D22 배치 머지·전파**, **D30 필수 등급 가드 2개** — 각 9지점

### 2.2 중복이 생기는 구조적 자리 3곳

- **안티패턴 목록**: SKILL.md 19개 + autonomous-driving 15개 + merge-coordinator 11개 + advisory-consult 10개 + architect-council 8개 + branch-strategy 7개 + agent-monitor 5개 + spec-driven-review 9개 + worktree-lifecycle 7개 = **91개 항목**. 대부분이 본문 규칙의 부정형 재진술이다 (예: D9는 4곳의 안티패턴에 각각 등장).
- **체크리스트**: 9개 reference가 말미에 체크리스트를 갖고 **총 142개 `- [ ]` 항목**(autonomous-driving 34, merge-coordinator 18, advisory-consult 15, agent-monitor 15, delegation-patterns 15, architect-council 12, spec-driven-review 12, worktree-lifecycle 12, branch-strategy 9). 항목 대부분이 본문 + 안티패턴의 3차 재진술이다 (D21은 본문 3곳 + 안티패턴 3곳 + 체크리스트 4곳).
- **"(요지)" 절**: SKILL.md:155(병렬 vs 순차), :175(team mode 강제 등급), :180(모델 라우팅) — 각각 delegation-patterns.md:142/:318, model-routing.md 전체와 짝. 여기에 SKILL.md:143(fan-out 복원력 요지 ↔ agent-monitor.md:160), SKILL.md:124–133(디스패치 게이트 6개 요지 ↔ 6개 서로 다른 파일의 절)까지 사실상 "요지 절"은 3개가 아니라 **5개**다.

---

## 3. 모순·불일치 (같은 주제를 두 곳이 다르게 말함)

| # | 주제 | 한쪽 | 다른 쪽 | 성격 |
|---|---|---|---|---|
| X1 | 에스컬레이션 조건 개수 | autonomous-driving.md:421 "아래 **8개** 중 하나라도" (목록 8개, 8번 = spec 미결) | autonomous-driving.md:452 "위 **7개** 조건과 별개로" | 같은 문서 안 숫자 불일치 — 8번 추가 시 아래 문장 미갱신 |
| X2 | epic 브랜치 rebase | branch-strategy.md:156 "**epic 브랜치 자체는 rebase하지 않는다**", :203(안티6) "epic 브랜치 rebase/force push → in-flight 전부 깨짐" | merge-coordinator.md:115 복구 절차가 `git pull --rebase origin epic/<name>` | 정면 충돌. 복구 절차가 금지된 연산을 지시 |
| X3 | 자문 왕복의 예산 소모 | advisory-consult.md:139 "반문 — **왕복도 예산을 소모**" | advisory-consult.md:162 "예산은 **소집 1회당** `max_advisory_consults` 1을 소모" | 같은 문서 23줄 간격 모순 (반문마다 소모 vs 소집당 1) |
| X4 | 자동 재시도 예산 | agent-monitor.md:214 "일반 실패는 **1회까지만**" (이 절은 :12에서 "두 모드 공통"으로 선언) | autonomous-driving.md:180 자율 모드 재위임 = `max_redispatch_per_task` 한도 내 반복, :341 "보통 **2~3**" | 공통 규칙으로 선언된 값과 자율 모드 값이 다름 |
| X5 | 리뷰·QA 게이트 조건성 | SKILL.md:135–139 "작업 케이스마다 **필수**", autonomous-driving.md:204 "**모든 작업에 예외 없이**" (무조건형) | SKILL.md:66 / delegation-patterns.md:205 경량 경로는 "**쓰기 전 검토 1회로 축소**"(조건부 유지) | 무조건형 ↔ 조건부. 무조건형 문장에 경로 단서 없음 |
| X6 | Task 룰 예외 범위 | SKILL.md:30·:122, agent-monitor.md:61 = "**단발 1회 작업만** 예외" | SKILL.md:139 "예외는 Task 룰과 동일하게 **단발 1회·read-only 작업만**" | "Task 룰과 동일하게"라면서 read-only를 더 얹음 — 인용이 원본보다 넓다 |
| X7 | 집행 위임의 실행 형태 | model-routing.md:52 표: 집행 위임 실행 형태 = `isolation:"worktree"` subagent (여집합 전부) | SKILL.md:72 "읽기 전용 조사·분석은 격리하지 않고 현재 브랜치에서 실행", delegation-patterns.md:133 동일 | 표가 집행 위임 = 항상 worktree로 읽히게 서술. 조사·리뷰(집행 위임 포함)는 비격리 |
| X8 | 메인이 실행해도 되는 Bash 범위 | SKILL.md:28 "`Bash(git status / git log / git diff --stat)` — 결정적 사실 확인에 **한정**" | merge-coordinator.md:74·:141–143 메인이 `git merge --ff-only` 수행, autonomous-driving.md:254 메인이 `integration_verify` 명령 직접 실행, merge-coordinator.md:200 메인이 worktree 삭제 | 허용 목록이 실제 절차보다 좁다 |
| X9 | HITL 자동 개입 금지의 서술 톤 | agent-monitor.md:10 "**자동 개입은 하지 않는다** — 결정은 사용자가 한다", :20·:235(안티2) 무조건형 | agent-monitor.md:12 스코프 단서(HITL 한정), autonomous-driving.md:12·:176–181 자율(기본)에서는 정체 해소용 SendMessage 허용 | 문서 첫 문단과 안티패턴은 무조건형, 스코프 단서는 :12 한 줄뿐 |
| X10 | worktree 정리 결정 주체 | merge-coordinator.md:90 "폐기된 worktree도 **사용자 확인 후** 삭제", :196–198 표가 "사용자가 보류/폐기 결정" | merge-coordinator.md:14 "기본 동작은 자율 주행 … HITL로 opt-out 한 경우에만 보고 후 결정" | 문서 상단은 자율 기본, 하단 절차는 HITL 전제로 서술 |
| X11 | branch-strategy 절 이름 | branch-strategy.md:18–21 표: `§2 hot-spot 분리` / `§4 rebase 후 ff` / `§5 역방향 흡수` | 실제 heading: `2. hot-spot 파일 …` / `4. 머지 방식 — rebase 후 fast-forward` / `5. epic ← main 역방향 drift` | 자기 문서 목차와 heading 이름 불일치 |
| X12 | 자문 트리거의 소유 | model-routing.md:17 판정 트리가 "§자문 조회의 네 트리거"를 자기 문서 안에서 가리킴 | advisory-consult.md:37 "언제 소집하는가 (트리거 — **단일 출처**)" | 단일 출처 선언과 실제 참조 방향 어긋남(B4와 동일 건) |
| X13 | 조사 리포트 산출물의 경로 판정 | delegation-patterns.md:213 "repo 밖·gitignore면 경량, tracked면 무거운" | SKILL.md:60 예시는 "read-only fan-out 조사"를 경량으로 단정 | 단정형 예시가 조건부 판정보다 넓게 읽힘 (경계 케이스로 위임됨을 SKILL.md:65가 보완하나 예시 문장 자체는 무조건형) |

---

## 4. 파일별 역할 (실제로 무엇을 소유하는가)

| 파일 | 소유하는 것 (한 줄) | 다른 파일과 나눠 가진 주제 |
|---|---|---|
| `SKILL.md` (230) | 진입 절차 + 경로 판정 게이트 + "reference 안 읽어도 성립해야 하는" 게이트·요지 모음. 사실상 목차 + 6개 요지 절 | 거의 전부. 특히 병렬/순차(→delegation §병렬 vs 순차 결정 트리), team 등급(→delegation §team mode 강제 등급), 모델 라우팅(→model-routing), 리뷰·QA(→autonomous-driving §리뷰어·QA 게이트), fan-out 복원력(→agent-monitor §fan-out 복원력) |
| `delegation-patterns.md` (461) | prompt 필수 포함 요소 13개, 증거 계약, 탐색 예산, 테스트 인프라 발견, 병렬/순차 트리, 경로 판정 경계·전환, 위임 깊이, 계획 우선 게이트, 근본원인 swarm, **team mode 강제 등급**, Agent team 패턴·spawn 확인, 작업유형→tier 표 | **9개 서로 다른 주제의 단일 출처를 한 파일이 가짐** — 실질적으로 5~6개 문서의 합본. 경로 판정은 SKILL(트리) ↔ 여기(경계·표)로 쪼개져 있고, tier는 여기(표) ↔ model-routing(원칙)으로 쪼개져 있다 |
| `autonomous-driving.md` (564) | 자율 계약·루프 의사코드, 모델 분배, 메인 컨텍스트 격리, 자동 개입 규칙, **리뷰어·QA·DBA 게이트**, 의사결정 기록 형식, 가드레일, **spec 확정 게이트**, 모호한 seam, 에스컬레이션 8조건, 종료 핸드오프 | 리뷰·QA 게이트를 spec-driven-review.md와 일반/spec으로 분할. 토폴로지 가드는 merge-coordinator(명령·복구) ↔ 여기(시점). 머지 정책은 branch-strategy로 전량 위임하면서 :235–239에 사다리를 재진술 |
| `branch-strategy.md` (226) | 충돌을 브랜치 운영으로 줄이는 6규칙: 네이밍, hot-spot, base drift 전파, rebase 후 ff, epic←main 흡수, 충돌 반복 사다리 | hot-spot 분해 원칙은 SKILL.md §분해는 충돌 경계로 쪼갠다와 분할(:23이 명시). 충돌 *해결*은 git skill로 위임 |
| `merge-coordinator.md` (286) | 머지 후보 수집·순서 결정, **토폴로지 가드 명령·복구 절차**, authorship 정정, 최종 통합 검증 게이트, 충돌 위임, worktree 정리, 보고 형식 | "언제·어떻게 머지"는 branch-strategy 소유(:22가 명시)라 순서/절차만 남았는데도 :53·:72·:85·:145에서 그 정책을 다시 서술 |
| `worktree-lifecycle.md` (166) | 격리 토폴로지 다이어그램, 병렬 dispatch 패턴, **dispatch 생성 가드**, 충돌 위험 사전 분석 | 결과 수령 이후는 merge-coordinator로 넘김(:110 명시). disjoint 판정은 delegation(트리) ↔ branch-strategy(hot-spot) ↔ 여기(파일 집합 추정)로 3분할 |
| `agent-monitor.md` (266) | 백그라운드 추적, **Task 시스템 사용법**, SendMessage 방향 규율, 정체/실패 감지(중복 보고·idle·no-op), **fan-out 복원력 4규칙**, 재위임 판단 기준 표, team 진행 추적 | HITL vs 자율 모드 구분을 autonomous-driving과 나눠 가짐(:12) — 그래서 X4·X9가 생김 |
| `advisory-consult.md` (283) | 자문 소집 트리거 4개 + 게이트 0, 패킷 입력·출력 계약, spawn 확인·기록 가드, 메인 처리 의무, 수명 | tier 예외 *원칙*은 model-routing 소유(:11–13 명시), *절차*만 여기. 등급 기준은 delegation 소유. 실질 3분할 |
| `architect-council.md` (174) | 협의체 메커니즘 5개/정책 분리, 두 자세(grill 어댑테이션), 협의체 루프, Task 도출 계약, **설계 승인 마커** | 분해 1단계를 SKILL.md 표준 절차와 나눠 가짐. team 필수 등급 근거는 delegation과 중복 서술 |
| `model-routing.md` (71) | 역할 기준 원칙(집행 tier 상한), 자문 tier 예외 대비 표, 역할별 모델 제약 | **71줄 중 실질 원칙은 :12–38뿐**이고 나머지는 포인터. tier 표(delegation), 자문 절차(advisory), 자율 배분(autonomous)로 전부 위임 — SKILL.md §모델 라우팅(요지)와 합치면 같은 규칙이 3중 |
| `spec-driven-review.md` (140) | 일반 게이트의 **spec 특수화**: 두 역할의 검증 질문, `spec-unresolved` verdict, continuous review 루프 | 예산·재위임·기록·에스컬레이션은 autonomous-driving 소유(:17 명시). 결과적으로 **고유 내용은 두 게이트의 검증 질문 표(:36–39)와 spec-unresolved(:48–53)뿐**이고 나머지 100줄은 재진술 |

### 4.1 "두 파일이 같은 주제를 나눠 가진" 목록 (재설계 시 병합 후보)

| 주제 | 조각 A | 조각 B | (조각 C) |
|---|---|---|---|
| 경로 판정 | SKILL.md:53–67 (트리) | delegation-patterns.md:193–221 (유지·생략 표 + 경계) | SKILL.md:99–101 / delegation-patterns.md:223–235 (전환 요지/절차) |
| team 강제 등급 | SKILL.md:175–178 (요지) | delegation-patterns.md:318–338 (기준·표) | advisory-consult.md:15–33 · architect-council.md:22 (경로별 재진술) |
| 모델 tier | SKILL.md:180–184 (요지) | model-routing.md:12–38 (원칙) | delegation-patterns.md:422–439 (표) · autonomous-driving.md:113–123 (자율 배분) |
| 병렬 vs 순차 | SKILL.md:155–161 (요지) | delegation-patterns.md:142–176 (트리) | branch-strategy.md:64–79 (hot-spot 분기) · worktree-lifecycle.md:50–55 (파일 집합 추정) |
| 토폴로지 가드 | merge-coordinator.md:101–121 (명령·복구) | autonomous-driving.md:241–247 (시점·자율) | worktree-lifecycle.md:77–96 (dispatch 생성 가드) · advisory-consult.md:101 (자문 전후) |
| 리뷰·QA 게이트 | SKILL.md:135–141 (요지) | autonomous-driving.md:202–225 (일반) | spec-driven-review.md 전체 (spec 특수화) |
| 머지 정책 | branch-strategy.md:97–157 (정책) | merge-coordinator.md:49–97 (절차) | autonomous-driving.md:227–239 (자율 재진술) |
| 자문 | model-routing.md:42–61 (원칙) | advisory-consult.md 전체 (절차) | SKILL.md:180–184 (요지) · agent-monitor.md:98 (SendMessage 예외) |
| fan-out 복원력 | SKILL.md:143–145 (요지) | agent-monitor.md:160–196 (절차) | — |
| Task 시스템 | SKILL.md:30·:120–122 (핵심 룰) | agent-monitor.md:59–81 (사용법) | — |

---

## 부록 A — 파일 간 참조 엣지 전체 (58)

```
SKILL.md                   -> delegation-patterns.md         28
autonomous-driving.md      -> SKILL.md                       16
delegation-patterns.md     -> SKILL.md                       16
SKILL.md                   -> autonomous-driving.md          10
autonomous-driving.md      -> delegation-patterns.md         9
spec-driven-review.md      -> autonomous-driving.md          9
merge-coordinator.md       -> branch-strategy.md             8
architect-council.md       -> autonomous-driving.md          7
delegation-patterns.md     -> autonomous-driving.md          6
SKILL.md                   -> agent-monitor.md               5
SKILL.md                   -> branch-strategy.md             5
architect-council.md       -> delegation-patterns.md         5
model-routing.md           -> SKILL.md                       5
worktree-lifecycle.md      -> branch-strategy.md             5
SKILL.md                   -> advisory-consult.md            4
advisory-consult.md        -> SKILL.md                       4
advisory-consult.md        -> autonomous-driving.md          4
agent-monitor.md           -> delegation-patterns.md         4
autonomous-driving.md      -> branch-strategy.md             4
autonomous-driving.md      -> worktree-lifecycle.md          4
branch-strategy.md         -> SKILL.md                       4
delegation-patterns.md     -> branch-strategy.md             4
spec-driven-review.md      -> delegation-patterns.md         4
advisory-consult.md        -> delegation-patterns.md         3
agent-monitor.md           -> SKILL.md                       3
architect-council.md       -> SKILL.md                       3
autonomous-driving.md      -> advisory-consult.md            3
branch-strategy.md         -> delegation-patterns.md         3
branch-strategy.md         -> merge-coordinator.md           3
delegation-patterns.md     -> model-routing.md               3
model-routing.md           -> delegation-patterns.md         3
worktree-lifecycle.md      -> merge-coordinator.md           3
SKILL.md                   -> model-routing.md               2
autonomous-driving.md      -> agent-monitor.md               2
autonomous-driving.md      -> architect-council.md           2
autonomous-driving.md      -> merge-coordinator.md           2
autonomous-driving.md      -> model-routing.md               2
branch-strategy.md         -> autonomous-driving.md          2
delegation-patterns.md     -> CLAUDE.md                      2
merge-coordinator.md       -> SKILL.md                       2
merge-coordinator.md       -> autonomous-driving.md          2
merge-coordinator.md       -> worktree-lifecycle.md          2
worktree-lifecycle.md      -> delegation-patterns.md         2
SKILL.md                   -> architect-council.md           1
SKILL.md                   -> worktree-lifecycle.md          1
advisory-consult.md        -> architect-council.md           1
advisory-consult.md        -> merge-coordinator.md           1
advisory-consult.md        -> model-routing.md               1
agent-monitor.md           -> advisory-consult.md            1
architect-council.md       -> model-routing.md               1
autonomous-driving.md      -> spec-driven-review.md          1
branch-strategy.md         -> worktree-lifecycle.md          1
delegation-patterns.md     -> agent-monitor.md               1
delegation-patterns.md     -> merge-coordinator.md           1
merge-coordinator.md       -> delegation-patterns.md         1
model-routing.md           -> autonomous-driving.md          1
spec-driven-review.md      -> SKILL.md                       1
worktree-lifecycle.md      -> SKILL.md                       1
worktree-lifecycle.md      -> autonomous-driving.md          1
```

## 부록 B — 절 참조 전량 (369건, `출발:행 → 대상 §절`)

```

# ---- SKILL.md ----
SKILL.md:16 -> autonomous-driving.md §메인 컨텍스트 격리
SKILL.md:28 -> autonomous-driving.md §메인 컨텍스트 격리
SKILL.md:29 -> SKILL.md (자기/상대참조) §진입 시 체크 4
SKILL.md:30 -> agent-monitor.md §Task 시스템
SKILL.md:32 -> SKILL.md (자기/상대참조) §진입 시 체크 0
SKILL.md:40 -> SKILL.md (자기/상대참조) §경로 판정 게이트가 먼저 정하고
SKILL.md:44 -> worktree-lifecycle.md §토폴로지
SKILL.md:45 -> branch-strategy.md §epic ← main 역방향 drift
SKILL.md:64 -> SKILL.md (자기/상대참조) §경로 전환. 판정 결과 + 근거를 진입 보고 1줄과 decision log에 남긴다 — **생략은 판정이 아니다.**
SKILL.md:65 -> delegation-patterns.md §경로 판정 경계 케이스
SKILL.md:66 -> delegation-patterns.md §경로 판정 경계 케이스
SKILL.md:72 -> delegation-patterns.md §경로 판정 경계 케이스
SKILL.md:72 -> delegation-patterns.md §Prompt 작성 원칙 필수 포함 요소
SKILL.md:73 -> delegation-patterns.md §team mode 강제 등급
SKILL.md:78 -> SKILL.md (자기/상대참조) §진입 시 체크 0
SKILL.md:92 -> SKILL.md (자기/상대참조) §team mode 강제 등급
SKILL.md:95 -> delegation-patterns.md §Agent team 사용 패턴
SKILL.md:97 -> delegation-patterns.md §공유 전제 preflight
SKILL.md:101 -> delegation-patterns.md §경로 전환
SKILL.md:106 -> SKILL.md (자기/상대참조) §진입 시 체크
SKILL.md:118 -> SKILL.md (자기/상대참조) §경로 판정 게이트의 요지와
SKILL.md:118 -> delegation-patterns.md §경로 판정 경계 케이스
SKILL.md:122 -> agent-monitor.md §Task 시스템
SKILL.md:128 -> autonomous-driving.md §spec 확정 게이트
SKILL.md:129 -> architect-council.md §설계 승인 마커
SKILL.md:130 -> delegation-patterns.md §테스트 인프라 발견
SKILL.md:131 -> delegation-patterns.md §증거 계약
SKILL.md:132 -> delegation-patterns.md §탐색 예산
SKILL.md:133 -> agent-monitor.md §중복 보고 감지
SKILL.md:133 -> agent-monitor.md §idle 판정
SKILL.md:141 -> autonomous-driving.md §리뷰어
SKILL.md:145 -> agent-monitor.md §fan-out 복원력
SKILL.md:152 -> branch-strategy.md §hot-spot 파일
SKILL.md:157 -> branch-strategy.md §hot-spot 파일
SKILL.md:157 -> delegation-patterns.md §병렬 vs 순차 결정 트리
SKILL.md:161 -> SKILL.md (자기/상대참조) §디스패치 전
SKILL.md:161 -> delegation-patterns.md §병렬 vs 순차 결정 트리
SKILL.md:168 -> SKILL.md (자기/상대참조) §진입 시 체크 4
SKILL.md:171 -> delegation-patterns.md §Agent team 사용 패턴
SKILL.md:178 -> delegation-patterns.md §team mode 강제 등급
SKILL.md:183 -> advisory-consult.md §게이트 0
SKILL.md:183 -> autonomous-driving.md §자율 계약
SKILL.md:184 -> delegation-patterns.md §모델 선택
SKILL.md:191 -> delegation-patterns.md §경로 판정 경계 케이스
SKILL.md:191 -> delegation-patterns.md §경로 전환
SKILL.md:191 -> delegation-patterns.md §공유 전제 preflight
SKILL.md:191 -> delegation-patterns.md §탐색 예산
SKILL.md:191 -> delegation-patterns.md §병렬 vs 순차 결정 트리가 단일 출처
SKILL.md:191 -> delegation-patterns.md §team mode 강제 등급이 단일 출처
SKILL.md:191 -> delegation-patterns.md §근본원인 swarm — 축 분해
SKILL.md:192 -> delegation-patterns.md §모델 선택
SKILL.md:193 -> branch-strategy.md §분해는 충돌 경계로 쪼갠다
SKILL.md:199 -> autonomous-driving.md §spec 확정 게이트
SKILL.md:206 -> autonomous-driving.md §에스컬레이션
SKILL.md:207 -> autonomous-driving.md §종료 조건
SKILL.md:207 -> SKILL.md §열린 PR 최신화 원칙
SKILL.md:207 -> autonomous-driving.md §종료 핸드오프
SKILL.md:214 -> delegation-patterns.md §Prompt 작성 원칙
SKILL.md:217 -> SKILL.md (자기/상대참조) §경로 판정 게이트
SKILL.md:218 -> SKILL.md (자기/상대참조) §경로 판정 게이트
SKILL.md:219 -> advisory-consult.md §안티패턴
SKILL.md:219 -> advisory-consult.md §team mode 강제 등급
SKILL.md:220 -> advisory-consult.md §안티패턴
SKILL.md:221 -> model-routing.md §역할 기준 원칙 /
SKILL.md:221 -> model-routing.md §역할별 모델 제약
SKILL.md:222 -> SKILL.md (자기/상대참조) §진입 시 체크 4
SKILL.md:223 -> delegation-patterns.md §필수 포함 요소
SKILL.md:224 -> delegation-patterns.md §필수 포함 요소
SKILL.md:225 -> SKILL.md (자기/상대참조) §진입 시 체크 0
SKILL.md:226 -> SKILL.md (자기/상대참조) §경로 판정 게이트
SKILL.md:227 -> branch-strategy.md §base drift 전파
SKILL.md:228 -> SKILL.md (자기/상대참조) §분해는 충돌 경계로 쪼갠다
SKILL.md:229 -> SKILL.md (자기/상대참조) §조사
SKILL.md:230 -> SKILL.md (자기/상대참조) §디스패치 전
SKILL.md:230 -> autonomous-driving.md §spec 확정 게이트

# ---- advisory-consult.md ----
advisory-consult.md:12 -> model-routing.md §역할 기준 원칙
advisory-consult.md:17 -> delegation-patterns.md §team mode 강제 등급
advisory-consult.md:18 -> advisory-consult.md (자기/상대참조) §왜 team member 전용인가가 이 경로에서의 근거를 소유한다
advisory-consult.md:19 -> advisory-consult.md (자기/상대참조) §안티패턴 1
advisory-consult.md:21 -> SKILL.md §진입 시 체크 4
advisory-consult.md:22 -> advisory-consult.md (자기/상대참조) §소집 절차의 **spawn 확인 실패 시 1회 재판정**뿐이다.
advisory-consult.md:33 -> delegation-patterns.md §Agent team 사용 패턴
advisory-consult.md:51 -> advisory-consult.md (자기/상대참조) §안티패턴 1
advisory-consult.md:62 -> advisory-consult.md (자기/상대참조) §소집 절차로
advisory-consult.md:70 -> autonomous-driving.md §자율 계약
advisory-consult.md:79 -> autonomous-driving.md §리뷰어
advisory-consult.md:101 -> merge-coordinator.md §토폴로지 가드
advisory-consult.md:101 -> SKILL.md §경로 판정 게이트
advisory-consult.md:107 -> architect-council.md §설계 원칙
advisory-consult.md:114 -> advisory-consult.md (자기/상대참조) §안티패턴 8
advisory-consult.md:133 -> advisory-consult.md (자기/상대참조) §spawn 확인
advisory-consult.md:143 -> advisory-consult.md (자기/상대참조) §기록
advisory-consult.md:148 -> delegation-patterns.md §spawn 확인
advisory-consult.md:150 -> advisory-consult.md (자기/상대참조) §안티패턴 1이다.
advisory-consult.md:154 -> autonomous-driving.md §의사결정 기록
advisory-consult.md:184 -> advisory-consult.md (자기/상대참조) §역할 제한은
advisory-consult.md:188 -> autonomous-driving.md §메인 컨텍스트 격리
advisory-consult.md:220 -> advisory-consult.md (자기/상대참조) §안티패턴 2
advisory-consult.md:224 -> SKILL.md §사고 모드
advisory-consult.md:238 -> advisory-consult.md (자기/상대참조) §spawn 확인(사전
advisory-consult.md:260 -> SKILL.md §진입 시 체크 4
advisory-consult.md:262 -> advisory-consult.md (자기/상대참조) §정책

# ---- agent-monitor.md ----
agent-monitor.md:39 -> delegation-patterns.md §필수 포함 요소
agent-monitor.md:61 -> SKILL.md §일감을 Task로 분리
agent-monitor.md:87 -> SKILL.md §진입 시 체크 0
agent-monitor.md:91 -> delegation-patterns.md §필수 포함 요소
agent-monitor.md:98 -> advisory-consult.md §소집 절차
agent-monitor.md:114 -> agent-monitor.md (자기/상대참조) §idle 판정
agent-monitor.md:139 -> agent-monitor.md (자기/상대참조) §재위임 판단 기준(prompt 결함
agent-monitor.md:147 -> agent-monitor.md (자기/상대참조) §fan-out 복원력의 폴백 규칙(read-only 조각은 메인 직접 분석
agent-monitor.md:147 -> agent-monitor.md (자기/상대참조) §보고 형식으로 사용자에게 보고하고 결정을 받는다.
agent-monitor.md:149 -> agent-monitor.md (자기/상대참조) §중복 보고 감지와 동일.
agent-monitor.md:156 -> agent-monitor.md (자기/상대참조) §감지 신호의 "worktree에 변경이 없는데 작업이 끝남"을 확정 신호로 보고
agent-monitor.md:156 -> agent-monitor.md (자기/상대참조) §재위임 판단 기준으로 회부한다 — 분류는 **prompt 결함** (검증 기준
agent-monitor.md:174 -> agent-monitor.md (자기/상대참조) §재위임 판단 기준의 isolation 행과 동일
agent-monitor.md:175 -> agent-monitor.md (자기/상대참조) §재위임 판단 기준 표를 따른다 (prompt 수정 또는 보고
agent-monitor.md:210 -> agent-monitor.md (자기/상대참조) §no-op run 감지
agent-monitor.md:211 -> agent-monitor.md (자기/상대참조) §idle 판정 — HITL이면 사용자 보고
agent-monitor.md:214 -> agent-monitor.md (자기/상대참조) §fan-out 복원력의 재시도 규칙(agent당 기본 3회
agent-monitor.md:223 -> SKILL.md §진입 시 체크 4
agent-monitor.md:223 -> delegation-patterns.md §Agent team 사용 패턴
agent-monitor.md:237 -> delegation-patterns.md §Agent team 사용 패턴
agent-monitor.md:247 -> agent-monitor.md (자기/상대참조) §백그라운드 위임 기본형 — 빠지면 agent가 답할 수단이 없다
agent-monitor.md:256 -> agent-monitor.md (자기/상대참조) §idle 판정 — 없으면 idle을 판정할 기준이 없다
agent-monitor.md:257 -> agent-monitor.md (자기/상대참조) §중복 보고 감지
agent-monitor.md:260 -> agent-monitor.md (자기/상대참조) §no-op run 감지
agent-monitor.md:264 -> agent-monitor.md (자기/상대참조) §idle 판정

# ---- architect-council.md ----
architect-council.md:22 -> delegation-patterns.md §team mode 강제 등급
architect-council.md:22 -> delegation-patterns.md §두 자세
architect-council.md:23 -> SKILL.md §진입 시 체크 4
architect-council.md:24 -> delegation-patterns.md §spawn 확인
architect-council.md:24 -> autonomous-driving.md §의사결정 기록
architect-council.md:30 -> architect-council.md (자기/상대참조) §모델 정책의 시작 기준을 참고하되 문제 난이도에 맞춰 정한다.
architect-council.md:43 -> autonomous-driving.md §spec 확정 게이트
architect-council.md:63 -> autonomous-driving.md §에스컬레이션
architect-council.md:64 -> autonomous-driving.md §spec 확정 게이트
architect-council.md:65 -> SKILL.md §미결
architect-council.md:97 -> autonomous-driving.md §메인 컨텍스트 격리
architect-council.md:98 -> delegation-patterns.md §Agent team 사용 패턴
architect-council.md:113 -> autonomous-driving.md §리뷰어
architect-council.md:120 -> SKILL.md §디스패치 전
architect-council.md:123 -> architect-council.md (자기/상대참조) §협의체 루프
architect-council.md:124 -> architect-council.md (자기/상대참조) §종료와 핸드오프
architect-council.md:126 -> autonomous-driving.md §의사결정 기록
architect-council.md:138 -> architect-council.md (자기/상대참조) §언제 쓰는가
architect-council.md:144 -> delegation-patterns.md §모델 선택
architect-council.md:144 -> model-routing.md §역할 기준 원칙
architect-council.md:154 -> architect-council.md (자기/상대참조) §언제 쓰는가
architect-council.md:172 -> architect-council.md (자기/상대참조) §자율 어댑테이션 — 미결은 hard stop

# ---- autonomous-driving.md ----
autonomous-driving.md:29 -> SKILL.md §진입 시 체크 4
autonomous-driving.md:34 -> advisory-consult.md §게이트 0
autonomous-driving.md:35 -> SKILL.md §경로 판정 게이트
autonomous-driving.md:37 -> SKILL.md §경로 전환으로 무거운 경로로 올린다
autonomous-driving.md:40 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트 — hard stop
autonomous-driving.md:58 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트
autonomous-driving.md:60 -> SKILL.md §경로 판정 게이트
autonomous-driving.md:76 -> worktree-lifecycle.md §dispatch 생성 가드
autonomous-driving.md:90 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트 (예산과 무관하게 멈춘다
autonomous-driving.md:95 -> SKILL.md §경로 판정 게이트
autonomous-driving.md:121 -> delegation-patterns.md §모델 선택
autonomous-driving.md:121 -> model-routing.md §역할 기준 원칙
autonomous-driving.md:143 -> delegation-patterns.md §Agent team 사용 패턴
autonomous-driving.md:146 -> delegation-patterns.md §team mode 강제 등급
autonomous-driving.md:156 -> delegation-patterns.md §Agent team 사용 패턴
autonomous-driving.md:170 -> SKILL.md §열린 PR 최신화 원칙이 단일 출처다.
autonomous-driving.md:187 -> agent-monitor.md §SendMessage
autonomous-driving.md:204 -> SKILL.md §작업 케이스마다 검토 에이전트
autonomous-driving.md:215 -> delegation-patterns.md §테스트 인프라 발견
autonomous-driving.md:217 -> architect-council.md §Task 도출 계약
autonomous-driving.md:239 -> branch-strategy.md §충돌 반복 /
autonomous-driving.md:239 -> branch-strategy.md §base drift 전파 /
autonomous-driving.md:239 -> branch-strategy.md §머지 방식
autonomous-driving.md:243 -> merge-coordinator.md §토폴로지 가드
autonomous-driving.md:243 -> worktree-lifecycle.md §dispatch 생성 가드
autonomous-driving.md:245 -> SKILL.md §경로 판정 게이트
autonomous-driving.md:245 -> delegation-patterns.md §경로 전환
autonomous-driving.md:294 -> advisory-consult.md §메인의 처리 의무
autonomous-driving.md:310 -> delegation-patterns.md §team mode 강제 등급
autonomous-driving.md:356 -> autonomous-driving.md (자기/상대참조) §범위
autonomous-driving.md:379 -> spec-driven-review.md §게이트 중 spec 미결 발견
autonomous-driving.md:386 -> autonomous-driving.md (자기/상대참조) §모호한 seam은 spec이 침묵하는 엣지에만 적용된다. spec이 미결로 **선언**한 항목은 seam이 아니라 결정 대기 상태다.
autonomous-driving.md:387 -> architect-council.md §언제 쓰는가
autonomous-driving.md:391 -> autonomous-driving.md (자기/상대참조) §종료 핸드오프
autonomous-driving.md:397 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트가 먼저 hard stop한다 — stub으로 격리해 전진하지 않는다.
autonomous-driving.md:439 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트
autonomous-driving.md:450 -> advisory-consult.md §게이트 0
autonomous-driving.md:464 -> SKILL.md §열린 PR 최신화 원칙
autonomous-driving.md:478 -> SKILL.md §안티패턴
autonomous-driving.md:495 -> SKILL.md §열린 PR 최신화 원칙
autonomous-driving.md:509 -> worktree-lifecycle.md §dispatch 생성 가드
autonomous-driving.md:509 -> SKILL.md §안티패턴 15
autonomous-driving.md:513 -> model-routing.md §역할 기준 원칙
autonomous-driving.md:517 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트
autonomous-driving.md:526 -> SKILL.md §진입 시 체크 0
autonomous-driving.md:527 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트 — 미결이 하나라도 있으면 계약을 세우지 않고 hard stop
autonomous-driving.md:532 -> SKILL.md §진입 시 체크
autonomous-driving.md:539 -> agent-monitor.md §Task 시스템
autonomous-driving.md:541 -> delegation-patterns.md §필수 포함 요소
autonomous-driving.md:542 -> delegation-patterns.md §계획 우선 게이트
autonomous-driving.md:545 -> branch-strategy.md §충돌 반복
autonomous-driving.md:546 -> worktree-lifecycle.md §dispatch 생성 가드
autonomous-driving.md:547 -> merge-coordinator.md §토폴로지 가드
autonomous-driving.md:548 -> SKILL.md §경로 판정 게이트
autonomous-driving.md:555 -> autonomous-driving.md (자기/상대참조) §spec 확정 게이트
autonomous-driving.md:561 -> autonomous-driving.md (자기/상대참조) §종료 핸드오프
autonomous-driving.md:564 -> SKILL.md §열린 PR 최신화 원칙

# ---- branch-strategy.md ----
branch-strategy.md:12 -> SKILL.md §경로 판정 게이트
branch-strategy.md:18 -> branch-strategy.md (자기/상대참조) §2 hot-spot 분리 |
branch-strategy.md:19 -> branch-strategy.md (자기/상대참조) §3 base drift 전파 |
branch-strategy.md:20 -> branch-strategy.md (자기/상대참조) §4 rebase 후 ff |
branch-strategy.md:21 -> branch-strategy.md (자기/상대참조) §5 역방향 흡수 |
branch-strategy.md:23 -> SKILL.md §분해는 충돌 경계로 쪼갠다
branch-strategy.md:66 -> worktree-lifecycle.md §충돌 위험 사전 분석
branch-strategy.md:76 -> delegation-patterns.md §병렬 vs 순차 결정 트리의 순차 규칙 그대로
branch-strategy.md:83 -> delegation-patterns.md §필수 포함 요소
branch-strategy.md:99 -> delegation-patterns.md §필수 포함 요소
branch-strategy.md:126 -> SKILL.md §진입 시 체크 4
branch-strategy.md:155 -> branch-strategy.md (자기/상대참조) §force-push 정책
branch-strategy.md:156 -> branch-strategy.md (자기/상대참조) §5도 같은 이유로 merge다
branch-strategy.md:162 -> SKILL.md §토폴로지
branch-strategy.md:169 -> merge-coordinator.md §최종 통합 검증 게이트
branch-strategy.md:170 -> branch-strategy.md (자기/상대참조) §4 주의의 두 번째 항목과 같은 이유
branch-strategy.md:172 -> branch-strategy.md (자기/상대참조) §4의 것을 그대로 쓰지 않는다.** 통합 경로의 기본 위임(
branch-strategy.md:172 -> merge-coordinator.md §충돌 시 위임
branch-strategy.md:173 -> autonomous-driving.md §에스컬레이션
branch-strategy.md:175 -> branch-strategy.md (자기/상대참조) §3의 전파가 필요해져 비용이 곱해진다. 런이 길어 중간 흡수가 불가피하면 **배치 경계**(
branch-strategy.md:175 -> branch-strategy.md (자기/상대참조) §3 A안의 머지 시점
branch-strategy.md:184 -> merge-coordinator.md §충돌 시 위임
branch-strategy.md:185 -> branch-strategy.md (자기/상대참조) §2
branch-strategy.md:192 -> autonomous-driving.md §의사결정 기록
branch-strategy.md:198 -> branch-strategy.md (자기/상대참조) §1
branch-strategy.md:199 -> branch-strategy.md (자기/상대참조) §2
branch-strategy.md:200 -> branch-strategy.md (자기/상대참조) §2
branch-strategy.md:201 -> branch-strategy.md (자기/상대참조) §3
branch-strategy.md:202 -> branch-strategy.md (자기/상대참조) §4
branch-strategy.md:203 -> branch-strategy.md (자기/상대참조) §4
branch-strategy.md:203 -> branch-strategy.md (자기/상대참조) §5
branch-strategy.md:204 -> branch-strategy.md (자기/상대참조) §6

# ---- delegation-patterns.md ----
delegation-patterns.md:39 -> autonomous-driving.md §위임 형태
delegation-patterns.md:51 -> delegation-patterns.md (자기/상대참조) §경로 판정 경계 케이스
delegation-patterns.md:56 -> delegation-patterns.md (자기/상대참조) §위임 깊이 제한 참조
delegation-patterns.md:57 -> branch-strategy.md §브랜치 네이밍
delegation-patterns.md:57 -> branch-strategy.md §base drift 전파
delegation-patterns.md:60 -> branch-strategy.md §hot-spot 파일
delegation-patterns.md:61 -> delegation-patterns.md (자기/상대참조) §증거 계약이 단일 출처다.
delegation-patterns.md:97 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소 13번
delegation-patterns.md:99 -> autonomous-driving.md §리뷰어
delegation-patterns.md:102 -> delegation-patterns.md (자기/상대참조) §근본원인 swarm 2번
delegation-patterns.md:106 -> SKILL.md §디스패치 전
delegation-patterns.md:110 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소 10번
delegation-patterns.md:111 -> delegation-patterns.md (자기/상대참조) §Prompt 작성 원칙
delegation-patterns.md:123 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소 10번
delegation-patterns.md:123 -> delegation-patterns.md (자기/상대참조) §증거 계약의 교차 검증을 거친다.
delegation-patterns.md:125 -> autonomous-driving.md §리뷰어
delegation-patterns.md:134 -> delegation-patterns.md (자기/상대참조) §Prompt 작성 원칙 필수 포함 요소 9번
delegation-patterns.md:136 -> delegation-patterns.md (자기/상대참조) §경로 판정 경계 케이스의 판정을 따른다
delegation-patterns.md:138 -> delegation-patterns.md (자기/상대참조) §Agent team 사용 패턴
delegation-patterns.md:144 -> SKILL.md §병렬 vs 순차 판정
delegation-patterns.md:155 -> branch-strategy.md §hot-spot 파일
delegation-patterns.md:166 -> delegation-patterns.md (자기/상대참조) §경로 판정 경계 케이스의 유지
delegation-patterns.md:166 -> delegation-patterns.md (자기/상대참조) §조사
delegation-patterns.md:172 -> SKILL.md §안티패턴
delegation-patterns.md:174 -> SKILL.md §메인 에이전트가 해도 되는 일
delegation-patterns.md:175 -> delegation-patterns.md (자기/상대참조) §탐색 예산 — 발동은
delegation-patterns.md:175 -> SKILL.md §디스패치 전
delegation-patterns.md:176 -> SKILL.md §When to use
delegation-patterns.md:182 -> SKILL.md §진입 시 체크
delegation-patterns.md:186 -> SKILL.md §병렬 fan-out 복원력
delegation-patterns.md:195 -> SKILL.md §경로 판정 게이트
delegation-patterns.md:202 -> SKILL.md §안티패턴
delegation-patterns.md:214 -> delegation-patterns.md (자기/상대참조) §경로 전환
delegation-patterns.md:220 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소의 3번(base 브랜치
delegation-patterns.md:225 -> SKILL.md §경로 전환
delegation-patterns.md:231 -> delegation-patterns.md (자기/상대참조) §경계 케이스 판정
delegation-patterns.md:241 -> delegation-patterns.md (자기/상대참조) §Agent team 사용 패턴이 의도한 설계다(teammate는 공유 checkout이라 편집을 직접 하지 않고 isolated subagent
delegation-patterns.md:243 -> agent-monitor.md §재위임 판단 기준
delegation-patterns.md:279 -> CLAUDE.md §Debugging 고착 탈출. 여기는 그 오케스트레이션 레이어다
delegation-patterns.md:298 -> autonomous-driving.md §메인 컨텍스트 격리
delegation-patterns.md:308 -> CLAUDE.md §Debugging
delegation-patterns.md:320 -> SKILL.md §team mode 강제 등급
delegation-patterns.md:336 -> delegation-patterns.md (자기/상대참조) §spawn 확인
delegation-patterns.md:336 -> autonomous-driving.md §의사결정 기록
delegation-patterns.md:344 -> SKILL.md §진입 시 체크 4
delegation-patterns.md:344 -> SKILL.md §team mode 강제 등급이 단일 출처다. 판정은 진입 시 1회 확정한다 — 트리거 시점에 확인하면 이미 다른 경로를 다 태운 뒤라 늦다. 과
delegation-patterns.md:348 -> delegation-patterns.md (자기/상대참조) §spawn 확인을 실행한다.
delegation-patterns.md:354 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소 11번
delegation-patterns.md:383 -> SKILL.md §진입 시 체크 0
delegation-patterns.md:385 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소 11번
delegation-patterns.md:388 -> delegation-patterns.md (자기/상대참조) §spawn 확인
delegation-patterns.md:388 -> merge-coordinator.md §토폴로지 가드
delegation-patterns.md:392 -> delegation-patterns.md (자기/상대참조) §team mode 강제 등급
delegation-patterns.md:408 -> SKILL.md §진입 시 체크 0
delegation-patterns.md:411 -> delegation-patterns.md (자기/상대참조) §진입 시 체크 4의 0
delegation-patterns.md:414 -> delegation-patterns.md (자기/상대참조) §team mode 강제 등급 표
delegation-patterns.md:424 -> model-routing.md §역할 기준 원칙
delegation-patterns.md:437 -> autonomous-driving.md §모델 분배
delegation-patterns.md:439 -> model-routing.md §자문 조회 — 상위 tier 예외
delegation-patterns.md:453 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소 9번
delegation-patterns.md:454 -> delegation-patterns.md (자기/상대참조) §필수 포함 요소 12번
delegation-patterns.md:455 -> delegation-patterns.md (자기/상대참조) §위임 깊이 제한
delegation-patterns.md:458 -> model-routing.md §자문 조회 — 상위 tier 예외

# ---- merge-coordinator.md ----
merge-coordinator.md:12 -> SKILL.md §경로 판정 게이트
merge-coordinator.md:14 -> autonomous-driving.md §머지/충돌
merge-coordinator.md:24 -> SKILL.md §PR 단위 원칙
merge-coordinator.md:54 -> branch-strategy.md §base drift 전파
merge-coordinator.md:58 -> branch-strategy.md §브랜치 네이밍
merge-coordinator.md:62 -> delegation-patterns.md §Prompt 작성 원칙 필수
merge-coordinator.md:72 -> branch-strategy.md §머지 방식
merge-coordinator.md:75 -> merge-coordinator.md (자기/상대참조) §머지 대상: epic 브랜치
merge-coordinator.md:78 -> branch-strategy.md §충돌 반복
merge-coordinator.md:83 -> merge-coordinator.md (자기/상대참조) §토폴로지 가드
merge-coordinator.md:84 -> merge-coordinator.md (자기/상대참조) §Authorship 확인
merge-coordinator.md:85 -> branch-strategy.md §base drift 전파
merge-coordinator.md:103 -> worktree-lifecycle.md §표준 절차 5 머지 직후 가드
merge-coordinator.md:103 -> worktree-lifecycle.md §dispatch 생성 가드
merge-coordinator.md:134 -> merge-coordinator.md (자기/상대참조) §표준 절차 5(머지 직후 가드
merge-coordinator.md:145 -> branch-strategy.md §epic ← main 역방향 drift
merge-coordinator.md:159 -> autonomous-driving.md §통합 검증
merge-coordinator.md:178 -> merge-coordinator.md (자기/상대참조) §보고 형식과 동일하다(단일 출처
merge-coordinator.md:184 -> merge-coordinator.md (자기/상대참조) §보고 형식으로 사용자에게 보고한 뒤 결정에 따라 진행한다.
merge-coordinator.md:253 -> branch-strategy.md §base drift 전파
merge-coordinator.md:254 -> branch-strategy.md §epic ← main 역방향 drift
merge-coordinator.md:255 -> merge-coordinator.md (자기/상대참조) §Authorship 확인

# ---- model-routing.md ----
model-routing.md:10 -> SKILL.md §모델 라우팅
model-routing.md:17 -> model-routing.md (자기/상대참조) §자문 조회의 네 트리거에 해당하는가?
model-routing.md:40 -> delegation-patterns.md §모델 선택
model-routing.md:40 -> autonomous-driving.md §모델 분배
model-routing.md:52 -> delegation-patterns.md §team mode 강제 등급
model-routing.md:57 -> SKILL.md §진입 시 체크 4
model-routing.md:57 -> delegation-patterns.md §team mode 강제 등급
model-routing.md:61 -> SKILL.md §모델 라우팅
model-routing.md:67 -> SKILL.md §안티패턴
model-routing.md:67 -> SKILL.md §역할 기준 원칙

# ---- spec-driven-review.md ----
spec-driven-review.md:15 -> SKILL.md §경로 판정 게이트
spec-driven-review.md:17 -> autonomous-driving.md §리뷰어
spec-driven-review.md:26 -> autonomous-driving.md §spec 확정 게이트
spec-driven-review.md:38 -> spec-driven-review.md (자기/상대참조) §게이트 중 spec 미결 발견
spec-driven-review.md:39 -> spec-driven-review.md (자기/상대참조) §게이트 중 spec 미결 발견
spec-driven-review.md:44 -> autonomous-driving.md §리뷰어
spec-driven-review.md:46 -> autonomous-driving.md §리뷰어
spec-driven-review.md:52 -> autonomous-driving.md §spec 확정 게이트
spec-driven-review.md:59 -> delegation-patterns.md §team mode 강제 등급
spec-driven-review.md:59 -> delegation-patterns.md §폴백
spec-driven-review.md:61 -> delegation-patterns.md §Agent team 사용 패턴
spec-driven-review.md:63 -> spec-driven-review.md (자기/상대참조) §두 게이트의 책임 분리 표에 있다. 구현 자체는 격리 subagent(
spec-driven-review.md:65 -> autonomous-driving.md §메인 컨텍스트 격리
spec-driven-review.md:81 -> spec-driven-review.md (자기/상대참조) §게이트 중 spec 미결 발견
spec-driven-review.md:91 -> autonomous-driving.md §의사결정 기록
spec-driven-review.md:100 -> autonomous-driving.md §모델 분배
spec-driven-review.md:104 -> delegation-patterns.md §모델 선택
spec-driven-review.md:118 -> spec-driven-review.md (자기/상대참조) §게이트 중 spec 미결 발견
spec-driven-review.md:127 -> autonomous-driving.md §spec 확정 게이트
spec-driven-review.md:136 -> spec-driven-review.md (자기/상대참조) §게이트 중 spec 미결 발견

# ---- worktree-lifecycle.md ----
worktree-lifecycle.md:12 -> SKILL.md §경로 판정 게이트
worktree-lifecycle.md:26 -> delegation-patterns.md §Prompt 작성 원칙 필수 포함 요소
worktree-lifecycle.md:26 -> branch-strategy.md §base drift 전파
worktree-lifecycle.md:27 -> branch-strategy.md §브랜치 네이밍
worktree-lifecycle.md:34 -> delegation-patterns.md §Prompt 작성 원칙 필수 포함 요소
worktree-lifecycle.md:55 -> branch-strategy.md §hot-spot 파일
worktree-lifecycle.md:58 -> worktree-lifecycle.md (자기/상대참조) §dispatch 생성 가드. base는 자동 보장되지 않음 —
worktree-lifecycle.md:92 -> autonomous-driving.md §에스컬레이션
worktree-lifecycle.md:93 -> merge-coordinator.md §토폴로지 가드
worktree-lifecycle.md:102 -> merge-coordinator.md §토폴로지 가드
worktree-lifecycle.md:107 -> branch-strategy.md §base drift 전파
worktree-lifecycle.md:144 -> worktree-lifecycle.md (자기/상대참조) §dispatch 생성 가드
worktree-lifecycle.md:154 -> branch-strategy.md §hot-spot 파일
worktree-lifecycle.md:158 -> worktree-lifecycle.md (자기/상대참조) §dispatch 생성 가드
worktree-lifecycle.md:162 -> worktree-lifecycle.md (자기/상대참조) §dispatch 생성 가드
worktree-lifecycle.md:164 -> merge-coordinator.md §토폴로지 가드
```

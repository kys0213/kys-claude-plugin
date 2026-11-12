# 칸반 대리 답변 — 자율 런 확정 설계 (협의체 pass)

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다. 현재 정책은 `plugins/atelier/spec/concerns/kanban-delegate.md`·`session-kanban.md` 와 코드가 정한다.
> 날짜: 2026-11-11 · 브랜치(epic): claude/magical-lovelace-bnfj4j · 이슈: 없음 · 승인 출처: council pass (생성 2라운드 · 검증 verdict pass, 조건: 아래 §구현 지시 K1~K10 을 해당 task 위임에 싣는다)
> 선행 기록: `plans/2026-11-11-atelier-kanban-delegate.md` (미승인 초안 — 이 문서가 대체), `plans/2026-11-10-atelier-kanban-card-actions.md`·`-spike.md`
> 사용자 요구·결정: 사용자는 메인 세션과만 대화하고, 메인이 칸반 상태를 보고 worker 의 입력 대기를 **사용자가 직접 결정한 사례 기반으로** 대신 처리한다(처음부터 다 위임하지 않음). 확신이 없으면 메인 대화에서 묻는다. 대리 이력은 메인 대화로 요청 시 + 매일 1회 보고한다. 성향 근거: suggest-workflow insight·칸반 답변 이력·Claude Code 메모리·지난 대화 기록. CLI 로 상태별 조회·질문 답하기. "스펙이 나오면 자율주행해줘" — 이 런.

## spike 결과 (Claude Code 2.1.322, tmux 대화형 세션, 로그 모드)

| # | 확인한 것 | 결과 |
|---|---|---|
| 1 | 권한 창에서 사람이 허용 | `classic.PermissionRequest` 의 `next(e)` 는 창보다 먼저 `{}` 로 끝난다(사람 답 미포함). 이후 같은 `tool_use_id` 의 `classic.PostToolUse` → `tool.call` 정상 결과 |
| 2 | 사람이 거부 | `classic.PermissionDenied`·`PostToolUse`·`PostToolUseFailure` 모두 없음. `tool.call` 이 `isError` + "The user doesn't want to proceed…" 로 끝나고 턴이 중단된다 |
| 3 | 허용 후 도구 실패 | `classic.PostToolUseFailure` 뒤 `tool.call` `isError` → 허용/거부를 사후 이벤트로 구분할 수 있다 |
| 4 | 턴 중 `$.prompt.submit` | 턴이 끝날 때까지 기다렸다 별도 턴으로 실행(자동 대기열). 입력 칸 draft 는 남는다. `$.prompt.read()` 로 draft 를 읽는다 |
| 5 | 보드 계산 비용 | 세션 20개(약 145KB) 1회 약 4ms (release) |

## 자문·검증 경과

- Fable 자문 검토(초안 대상): 출처 인증·대상 잘림·결정 답 묶기·그림자 모드 권고.
- 협의체 r1 → 검증 revise (critical 3: 선례 답 내용 미검증·읽기성 목록 구멍·relay 표지 위조) → r2 → 검증 pass.
- 자문 델타(E1~E4 사용자 판정 여부): 네 기본값 모두 자율을 좁히는 쪽이라 묻지 않고 진행. E2 는 "승격 기준은 권고, 사람이 입력한 `act` 는 언제나 받음" 으로 수정.

---

# 부록 A — 설계 r1 의 유지 부분 (사실 F1~F15, 접근 선택, 역할 나눔, r1 가정)

## 0. 탐색으로 확정한 사실 (설계 전제)

| # | 사실 | 근거 |
|---|---|---|
| F1 | 세션 기록은 모드의 `$.store` 에만 있다. `$.store` 는 "플러그인 자신의 JSON 파일" 이고 위치·형식은 엔진 내부다. 외부 프로세스(터미널의 CLI)는 읽을 공식 경로가 없다 | hooks/kanban.ts:132-137 (`snapshot` 이 `$.store.keys/get`), 엔진 타입 문서 `store:` 설명 (claude-code.d.ts:3548-3555) |
| F2 | `atelier kanban board` 는 세션 기록을 stdin 으로만 받는다. 상태 필터 인자가 없다 | cli/src/kanban/mod.rs:40-57, 360 |
| F3 | 툴 `kanban_board` 는 `repos` 만 주고 세션마다 `column·cwd·permission_mode·actions·subtitle·last_line·delivery` 뿐이다. 질문 내용·권한 id·결정 선택지는 카드(`cards`)에만 있다 | hooks/kanban.ts:206-227, cli/src/kanban/mod.rs:216-235, core/repos.rs:6-15 |
| F4 | 질문 답은 pane 이 `$.store` 의 `kanban:answer:<sid>:<tool_use_id>` 에 쓰고 묻는 세션이 2초 tick 에 읽는다 — CLI 경로 없음 | hooks/kanban-ask.ts:145, 85-99 |
| F5 | 권한 답 `permit` 은 `--session·--request·--decision` 만 맞으면 통과. 누가 보냈는지 없다 | cli/src/kanban/mod.rs:80-89, commands/requests.rs:55-65 |
| F6 | 결정 요청 답은 일반 `send`(프롬프트). take 는 세션이 쉬면 60초 안 프롬프트를 대상 확인 없이 전달 | hooks/kanban-detail.ts:190-195, core/request.rs:331-346 |
| F7 | 권한 대상은 필드마다 200자로 잘려 기록, 결정 ask 도 200자, 질문 문장 1000자·선택지 라벨 200자 | hooks/kanban.ts:74-83, 471; kanban-record.ts:6; kanban-ask.ts:7, 39-51 |
| F8 | 권한 대기는 2초 × 900회 ≈ 30분에서 멈춘다 | hooks/kanban-inbox.ts:7-9, 125-161 |
| F9 | 사람이 묻는 세션 화면에서 직접 고른 질문 답은 `won.local` 로 이미 손에 있지만 기록하지 않는다 | hooks/kanban.ts:546-553 |
| F10 | 모드는 `$.fs.write`(쓰기·디렉터리 생성), `$.prompt.submit`, `$.prompt.read`, `prompt.submit` 훅의 `context`, `$.config`(userConfig 읽기)를 쓸 수 있다. `$.fs` 에 rename 은 없다 | claude-code.d.ts:3447-3453, 3108-3150, 2049 |
| F11 | 모드는 이벤트↔CLI JSON 변환만, 판정은 CLI (fail-open) | .claude/rules/tool-layer-boundary.md §모드 모듈 |
| F12 | atelier CLI 는 sha2 등 해시 크레이트가 없다(`regex`·`serde_json` 은 있음) | plugins/atelier/cli/Cargo.toml:17-19 |
| F13 | orchestrator 정지 조건 4 의 "되돌리기 어려운 행위": 강제 push·기본 브랜치 머지·배포·외부 서비스 호출·데이터 삭제 | plugins/atelier/skills/orchestrator/SKILL.md:39-46 |
| F14 | 다른 플러그인 skill·command 를 부르면 `dependencies` 선언 | .claude/rules/plugin-governance.md |
| F15 | spike: 권한 직접 허용 → PostToolUse(또는 PostToolUseFailure) 가 같은 tool_use_id 로 옴, 직접 거부 → 둘 다 없이 tool.call isError 로 끝남(Esc 와 구별 불가). `$.prompt.submit` 은 턴 중이면 턴 끝까지 자동 대기, draft 유지 | inputs.md §spike 1-4 |

---

### 2.1 검토한 접근과 선택

**(a) 터미널 CLI 의 상태 조회 원천** (F1 때문에 필요)

| 안 | 장단 | 판정 |
|---|---|---|
| A. 각 세션 모드가 자기 기록의 **사본 파일**을 남기고, CLI 가 사본 디렉터리를 읽는다 | 엔진 내부와 무관, 판정 코어는 그대로 재사용, 사본이 깨지면 그 카드만 빠지는 기존 정책 재사용 | **채택** |
| B. CLI 가 `$.store` 의 JSON 파일을 직접 읽는다 | 엔진 내부 위치·형식에 묶임(초안의 기각 사유와 같음) | 기각 |
| C. 조회는 모드 툴로만 | 요구사항 3(터미널 CLI) 위반 | 기각 |

**(b) 대리 답 검증 위치**

| 안 | 판정 |
|---|---|
| A. **받는 세션**(원본 입력 보유)이 take·wait-permit 때 CLI 로 최종 판정, 보내는 CLI 는 구조 검사 | **채택** — 원본으로 판정, 받는 세션이 오염돼도 피해가 자기 자신에 한정 |
| B. 보내는 CLI 가 사본으로 판정 | 기각 — 사본은 잘린 값(F7) |
| C. skill 이 판정 | 기각 — 결정적 변환을 지능에 묻음 |

**(c) 메인 깨우기**

| 안 | 판정 |
|---|---|
| A. 위임 세션 모드의 2초 tick → CLI `delegate wake` 가 깨울지·무엇을 낼지 판정 → `$.prompt.submit` 또는 다음 프롬프트 `context` | **채택** |
| B. `/loop`·cron 폴링 | 기각(초안 사유 그대로: 지연·빈 턴 비용) |
| C. 원격 예약 작업 | 기각(이력·worker 가 이 머신에만 있음) |

### 2.2 역할 나눔

| 층 | 소유 | 하는 일 | 하지 않는 일 |
|---|---|---|---|
| CLI `atelier kanban` | 결정적 변환 | 보드·상태 판정, `wait` 도출, match_key·가드·잘림 판정, 답 전달(inbox), 받는 쪽 검증, 장부 쓰기(마스킹·출처 유도)·걸러 읽기·선례 집계, 그림자 일치율·승격 가능 판정, 위임 세션 판정, 깨우기·일일 보고 시각 판정, Bash 가드 판정 | 대리할지 결정, 추천안 작성, 자연어 해석 |
| 모드 (각 세션) | 사실 기록·전달 | 기록 사본 쓰기, 사람이 직접 한 답과 받은 답의 사실을 CLI `ledger record` 로 넘김, 위임 세션이면 tick 마다 CLI `delegate wake` 결과대로 프롬프트 제출·맥락 첨부, `/kanban delegate …` 명령 처리, Bash 툴 호출을 CLI `guard-bash` 결과대로 거부 | 판정(정규식·임계값·비교) |
| skill `kanban-delegate` | 판단 | 대기마다 대리/묻기 결정, 사용자에게 물을 질문·추천안 작성, 그림자 판정 기록, 뒤집기 기록, 보고 작성, 성향 프로필 변경 제안 | 같은 결정인지 판정(=키), 출처 판정, 가드 |

쓰기 행동은 전부 Bash 로 부르는 CLI 다(모드 툴은 권한 단계를 건너뜀 — inputs.md spike 7). 모드 툴 `kanban_board` 는 읽기 전용으로 남는다.

## 5. 가정과 에스컬레이션 후보

### 가정 (탐색으로 답할 수 없어 닫은 것)

| # | 가정 | 근거 |
|---|---|---|
| A1 | `$.fs.write` 로 쓴 사본을 CLI 가 쓰는 도중에 읽을 수 있다 — 깨진 사본은 그 세션만 빼고 다음 호출에 회복 | `$.fs` 에 rename 이 없음(F10). 기존 "깨진 기록 → 그 카드만 빼요" 정책과 같은 처리 |
| A2 | 사본 쓰기 비용(이벤트마다 수 KB)은 무시할 수 있다 | 기록은 이미 이벤트마다 `$.store` 에 쓰고, store 전체 4MiB 상한(kanban-record.ts:4) |
| A3 | 문장 정규화는 공백·대소문자·끝 문장부호만. 유니코드 정규화(NFC)는 하지 않음 | 새 의존 없이 결정적(F12). 한국어 입력은 대부분 완성형이라 영향 작음 |
| A4 | 권한 키의 Bash 대상은 앞 두 토큰 | 자문 제안 그대로. 보수적(파일 경로가 키에 들어가 사례가 덜 쌓임)이며, 바꾸려면 키 버전을 올림 |
| A5 | 대리 수치: 직접 2 / 승인 포함 3 / 90일 / 충돌 0 / 재활성 2 | 자문 제안 그대로 |
| A6 | 승격 수치: 30일·10건·불일치 ≤1·최근 5 일치, 자동 강등 7일 2회 | 자문 "일치율 기준" 의 구체화. 사용자가 "처음부터 다 위임하지 않음" 이라 보수적으로 |
| A7 | v1 은 단일 선택 질문만 대리 | 복수 선택 답 집합 일치 규칙은 사례 뒤에 정함 |
| A8 | 깨우기 수치: 모음 10초, 사용자 유예 30초, seen 최대 200 | 초안 10초 유지 + 자문 "유예". 30초는 사용자가 이어서 입력할 짧은 창 |
| A9 | 지켜보기 중 선례가 있는 대기는 추천 없이 묻는다 | 추천 노출이 일치율을 부풀리는 편향 방지 |
| A10 | 읽기성 허용 목록(§2.7)과 되돌리기 어려움 표지 목록 | F13 정지 조건 4 를 문자열 표지로 옮김. 목록 변경은 spec 수준이 아니라 CLI 규칙(키 버전 무관) |
| A11 | 보고 시각 설정은 userConfig `kanban_report_at`, 모드가 `$.config` 로 읽음 | F10, plugin.json 에 userConfig 선례 존재 |
| A12 | 결정 요청의 "사람이 직접 답함" = 결정 카드가 떠 있는 동안 사람이 친 다음 프롬프트 | spec 정책 3 의 결정 요청 수명(다음 턴 시작에 사라짐) |
| A13 | 장부 위치는 inbox 와 같은 뿌리(`$CLAUDE_CONFIG_DIR` 또는 `~/.claude` 아래 atelier/kanban), 프로필도 같은 뿌리 | commands/inbox.rs:56-74 관례 |
| A14 | suggest-workflow CLI 를 skill 이 부르는 것도 의존 선언 대상 | plugin-governance 는 skill·command·agent 호출을 명시. CLI 호출은 명시 밖이나 같은 결합으로 봄 |
| A15 | 옛 `permit`(from 없음)은 pane 으로 읽는다 | 배포 중 버전 섞임. 옛 경로는 pane 만 썼음(kanban-detail.ts) |


---

# 부록 B — 설계 r2 (확정 결정)

## 0. 추가로 확정한 사실

| # | 사실 | 근거 |
|---|---|---|
| F16 | atelier CLI 의 의존에는 해시 크레이트가 없다. 저장소의 다른 Cargo 프로젝트에도 `sha2`·`blake3` 가 없다 | plugins/atelier/cli/Cargo.toml:14-19 (`clap env_logger regex serde serde_json`), `grep -rln --include=Cargo.toml -E "^(sha2\|blake3)" .` → 결과 없음 |
| F17 | 툴체인은 1.93.1 이다. 따라서 `std::fs::File::lock` 을 새 의존 없이 쓸 수 있다 (1.89 에서 안정화) | /home/user/kys-claude-plugin/rust-toolchain.toml:2 |
| F18 | 프롬프트 출처 `origin.kind` 의 값: 사람 경로는 `composer`(터미널 입력 칸의 Enter·대기열·클릭)와 `bridge`(원격 제어로 사람이 보낸 메시지)이다. 그 밖의 값은 `sdk`·`task-notification`·`scheduled-trigger`·`peer`·`peer-send-message`·`projects-relay`·`channel`·`coordinator`·`observer`·`observer-activity`·`auto-continuation`·`unclassified`, 그리고 플러그인 제출 `plugin` 이다 | claude-code.d.ts:9630-9725, 3110-3116. `command.run` 도 `origin` 을 싣는다 (verify-r1 T:1846-1852) |
| F19 | `tool.call` 훅은 호출을 다시 쓸 수 있다(`next({...e, …})`) | claude-code.d.ts:14341 "(`tool.call` rewrites a call)" |
| F20 | 지금의 `/kanban` 명령 핸들러는 인자와 출처를 보지 않고 토글만 한다 | hooks/kanban.ts:366 |
| F21 | 지금의 `tool.call` 기록 훅은 서브에이전트 호출(`agentId` 있음)을 건너뛴다 | hooks/kanban.ts:443-458 |
| F22 | `send` 는 이미 `--from pane\|agent` 를 받는다 | cli/src/kanban/mod.rs:66-78, 297-305 |

---

## 1. 처리표

### 1.1 verify-r1 findings

| id | finding | 처리 | 해소 내용 (본문 위치) |
|---|---|---|---|
| C1 | 선례 대리가 답 내용을 검증하지 않음 | **해소** | 대리 조건 7 "보낸 답 == 선례 답" 을 추가했다. 질문은 질문마다 정규화 라벨의 지문이 선례 답 지문과 같고, 그 라벨이 원본 선택지 중 하나여야 한다. 권한은 선례 답과 보낸 답이 모두 `allow` 여야 한다. 마스킹이 답 라벨을 바꾼 건은 지지 건으로 세지 않는다. 보내는 쪽과 받는 쪽이 같은 함수로 검사한다 (§2.6, spec 정책 5) |
| C2 | "읽기성" 허용 목록에 쓰기·실행·외부 전송이 섞임 | **해소** | 명령별로 허용한 옵션과 위치 인자 수만 받는 허용 목록으로 바꿨다. 판정은 앞 두 토큰이 아니라 argv 전체로 하고, 모르는 플래그·초과 위치 인자·확장·리다이렉션·복합 명령은 `proxyable=false` 다. 경로 인자는 레포 안이어야 한다. WebFetch·WebSearch 는 v1 대리에서 뺐다. verify·자문이 든 거부 사례는 T1b 의 블랙박스 테스트 표로 고정했다 (§2.7) |
| C3 | relay 표지를 worker 글로 위조할 수 있음 | **해소** | (1) 표지는 질문 첫 줄에 하나만 허용하고, 위임 세션 모드가 그 질문을 띄우기 전에 CLI 가 만든 고정 머리말로 바꿔 쓴다. 머리말에는 대상의 원문(도구·대상 또는 원 질문·선택지)이 CLI 가 기록 사본에서 읽은 그대로 들어간다. (2) 본문의 같은 표지는 무효로 만든다. (3) `status`·`board` 는 worker 글 안의 표지 문자열을 무력화해 내보낸다. (4) relay 건은 그 tool_use_id 에 모드가 기억한 대상(`target_session`·`target_wait`)으로 묶는다. 텍스트를 다시 파싱해 묶지 않는다 (§2.5, spec 정책 8) |
| I1 | 그림자 "would" 를 skill 이 씀 → 경계 위반·승격 지연 | **해소** | `ledger shadow` 를 없앴다. `ledger stats` 가 장부를 시간순으로 재생해, 사람 결정마다 그 시점에 대리 조건을 만족했는지와 선례 답이 사람 답과 같았는지를 계산한다. 답이 어느 경로(session·pane·main)로 들어왔든 모두 비교에 들어간다. 승격 창은 "최근 90일 안 비교 10건" 이다. `promotable` 은 권고이고 사람의 `act` 명령은 기준에 못 미쳐도 받는다 (§2.8). E2 의 근거 문장도 고쳤다 (§5) |
| I2 | 결정 요청 영구 비대리가 가정·에스컬레이션에 없음 | **해소** | 가정 A16 으로 두고 종료 보고 항목에 넣었다. 결정 요청도 장부에 `decision` 종류로 쌓고 재생 비교를 해서 나중 승격의 근거로 남긴다. v1 에서는 `act decision` 명령을 거부한다 (§2.8, §5) |
| I3 | 여러 질문 대리와 `--precedent` 표면이 맞지 않음 | **해소** | `--precedent <질문 순번>:<id>` 를 반복해서 받는다(질문마다 1~5개). 검사는 "질문마다 그 질문의 키와 같음" 이다 (§2.3) |
| I4 | relay id 조회가 비결정적이고 relay 필드 뜻이 없음 | **해소** | relay 건에 `target_session`·`target_wait` 를 둔다. `status`·`board` 의 `wait.relays` 가 그 대기의 미사용 relay 를 직접 준다. `ledger list --target-session --target-wait` 도 추가했다. `--limit 1` 로 추정하지 않는다. relay 는 위임 세션 화면 직접 답(`won.local`) 또는 pane 답에서만 만든다. 표지가 있는 질문은 일반 `question` 선례로 기록하지 않는다 (§2.4·§2.5) |
| I5 | context 경로에서 대기가 무기한 묻힘 | **해소** | `seen` 은 실제로 전달된 뒤(제출 성공 또는 맥락 첨부 성공)에만 표시한다. 보류 상한 3분이 지나면 draft 가 있어도, 위임 세션 턴이 쉬고 있으면 제출한다. draft 는 남는다(spike 4) (§2.9) |
| I6 | 사람 전용 명령이라는 주장을 엔진 수단으로 보장하지 않음 | **해소** | 사람 입력은 `origin.kind ∈ {composer, bridge}` 일 때만 인정한다(F18). `/kanban delegate …`, 결정 요청의 사람 답 추론, 마지막 사람 입력 시각에 모두 이 규칙을 쓴다. 판정은 CLI 가 하고, 모드는 origin 을 사실로 넘긴다. spec 정책 1·3·9 에 넣었다 (§2.5) |
| I7 | `send --from pane` 이 Bash 가드 밖 | **해소** | Bash 가드 대상에 넣었다. 결정 카드가 떠 있는 세션에 `send` 가 와도 그것은 결정 답이 아니다. 결정 답은 `decide` 로만 가고, `send` 는 일반 프롬프트로 다룬다. 사람 결정 추론은 사람 origin 프롬프트에서만 하므로 `send --from agent` 가 결정 선례가 되지 않는다 (§2.5, spec 정책 13 변경) |
| I8 | FNV-1a 64 는 표적 충돌에 약함 | **해소** | 결정 키는 SHA-256 앞 128비트(32 hex)로 바꿨다. `sha2` 크레이트 의존을 추가하고, 근거는 F16 이다 (§2.6, A17) |
| I9 | T3 가 한 번 위임으로 끝날 크기가 아님 | **해소** | T3a(장부 쓰기·읽기)와 T3b(선례·인증)로 나눴다. `--from agent` 는 T3b 끝에서만 열린다. T1 도 T1a·T1b 로 나눠 task 는 7개다 (§4) |
| m1 | 정책 2 의 `/kanban` 토글 문장 | **해소** | spec 변경표에 넣었다. T4 할 일에 "인자 분기" 를 명시했다 |
| m2 | "어떤 명령도 사람 결정을 인자로 받지 않음" 이 `--from pane` 과 충돌 | **해소** | 문장을 "`--from pane` 은 사람 경로로 쓰는 표시이고, 모델의 셸 호출에서는 막아요" 로 사실대로 고쳤다 |
| m3 | store 답 키 → inbox 이관 중 버전 섞임 | **해소** | 받는 쪽 모드는 한 릴리스 동안 옛 store 답 키와 inbox `answer` 를 둘 다 읽는다. 새 pane 이 옛 모드 세션에 답하는 경우는 버려지며, 이것을 제약 조건에 적었다 (A15·A18) |
| m4 | 검증 기준에 비결정 항목 | **해소** | 성능은 `#[ignore]` 테스트로 두고 통과 판정에서 뺐다. tmux e2e 는 참고 기록만 남긴다 |
| m5 | 잠금 수단 미정 | **해소** | `std::fs::File::lock` 을 쓴다 (F17) |
| m6 | 가드가 서브에이전트 Bash 에도 적용돼야 하고 fail-open 을 명시해야 함 | **해소** | `agentId` 와 상관없이 적용한다. fail-open 은 spec 제약 조건에 적었다 |
| m7 | 자동 메모리 포함 여부 | **해소(가정)** | A19: 메인 레포의 자동 메모리는 추천 근거로만 포함한다 |
| m8 | `forget` 무인증 | **해소** | `forget` 은 모드 전용이고 Bash 가드 대상이다 |
| m9 | 모드가 truncated·draft 를 비교 판정함 | **해소** | 모드는 원래 길이·저장 길이·draft 길이를 사실로 넘기고, 판정은 CLI 가 한다 |

### 1.2 advisor-delta1 (메인 결정 — 그대로 반영)

| 항목 | 반영 |
|---|---|
| E1 파일 직접 위조 수용 | spec 제약 조건 첫 줄과 종료 보고 항목 R2 (§5) |
| E2 그림자 시작 · promotable 은 권고 · 사람의 act 는 기준 미달이어도 받음 · 진행도 표시 · 종료 보고 첫 항목 | §2.8, spec 정책 6, `status.delegate.progress`, 일일 보고 ⑥, 종료 보고 R1 |
| E3 읽기성 허용만, 목록 재설계, Web 제외, 쓰기형 제외 사례 테스트 | §2.7, T1b 검증 표 |
| E4 worker 레포 CLAUDE.md 제외 | spec 정책 11, 종료 보고 R3 |
| medium: Bash 가드는 `kanban` 포함 시에만 CLI 호출, 실패는 통과 | §2.5 |
| medium: relay 는 `won.local` 또는 pane 답에서만 | §2.5 |

### 1.3 메인 라운드 2 지시

| # | 지시 | 반영 |
|---|---|---|
| 1 | promotable 권고, 사람 act 수용, E2 근거 수정, would 는 CLI 재생 | §2.8, §5 E2 |
| 2 | 허용 목록 방식, argv 전체 판정, Web 제외, 쓰기형 사례를 T1b 테스트로 | §2.7, §4 T1b |
| 3 | 받는 쪽이 답을 선례 답과 대조 | §2.6 조건 7 |
| 4 | relay 생성 조건, 대상 묶기, 결정적 조회, 고정 머리말 표지 규칙 | §2.5 |
| 5 | 결정 요청은 늘 묻되 장부에 `decision` 으로 축적, 가정과 종료 보고 | A16, R4 |
| 6 | Bash 가드 `kanban` 포함 시만 호출, fail-open, `send --from pane` 포함 | §2.5 |
| 7 | 사람 origin 목록을 spec 문장으로 | spec 정책 3 |
| 8 | SHA-256 앞부분, 의존 근거 | §2.6, F16, A17 |
| 9 | T3 분할 | §4 |
| 10 | 깨우기 보류 상한, seen 은 전달 후 | §2.9 |

---

## 2. 확정 결정

r1 §2.1(접근 선택: 기록 사본 / 받는 쪽 재검증 / 위임 세션 tick 깨우기)과 §2.2(역할 나눔)는 유지한다. 다만 아래 변경을 반영한다.
- skill 의 책임에서 "그림자 판정 기록" 이 빠졌다.
- CLI 의 책임에 장부 재생(그림자 비교), relay 머리말 만들기, 사람 origin 판정이 더해졌다.

### 2.2' 역할 나눔 (갱신)

| 층 | 하는 일 | 하지 않는 일 |
|---|---|---|
| CLI `atelier kanban` | 보드·상태 판정, `wait` 도출, 결정 키·허용 목록·가드·잘림 판정, worker 글의 표지 무력화, relay 머리말 만들기, 답 전달(inbox), 받는 쪽 재검증(답 대조 포함), 장부 쓰기(마스킹·출처·origin 판정)·읽기·선례 집계·재생 비교·승격 권고, 위임 세션·깨우기·보고 시각 판정, Bash 가드 판정 | 대리할지 결정, 추천안 작성, 자연어 해석 |
| 모드 (각 세션) | 기록과 기록 사본 쓰기(원래 길이 사실 포함), 사람·받은 답의 사실을 `ledger record` 로 넘김(origin 포함), 위임 세션이면 `delegate wake` 결과대로 제출·맥락 첨부, 질문 첫 줄 표지가 있으면 `relay-frame` 결과로 질문을 다시 쓰고 tool_use_id↔대상을 기억, Bash 호출 가드 | 판정 |
| skill `kanban-delegate` | 대기마다 대리·묻기 결정, 질문 문장·추천안 작성, 뒤집기 기록, 보고 작성, 프로필 변경 제안 | 같은 결정 판정, 그림자 계산, 출처 판정, 가드 |

### 2.3 CLI 명령 표면 (갱신)

공통 규칙은 r1 과 같다. 모든 하위 명령은 JSON 한 줄을 내고 exit 0 이며, 거부는 `{"error": …}` 로 낸다.

#### 읽기

| 명령 | 인자 | 출력 · 거부 |
|---|---|---|
| `board` | 기존 인자 + `--column <id>` (반복 가능) | `repos[].sessions[]` 에 `wait`, 최상위에 `delegate` 를 더한다. 모르는 컬럼은 거부 |
| `status` | `[--column]… [--session] [--now <ms>](없으면 시스템 시각) [--format json\|text]` | `{now, delegate, sessions}`. 사본 디렉터리를 찾지 못하면 거부 |
| `record-path` | `--session` | `{path}` |
| `forget` | `--session` | `{forgot}`. **모드 전용**(Bash 가드 대상) |
| `relay-frame` | `--now` · stdin `{questions:[원본]}` | `{questions:[다시 쓴 질문], target:{session, wait, kind}\|null}`. 첫 줄 표지가 없으면 `target:null` 이고 질문은 그대로 낸다. 표지가 가리키는 대기가 사본에 없으면 `{error}` 를 내고, 모드는 원래 질문 그대로 진행한다(fail-open, relay 없음) |

`WAIT` 는 r1 모양에 다음을 바꾸고 더한다.
- worker 가 쓴 텍스트 필드(`request`·`context`·`questions[].question/header/options[].label/description`·`ask`·`recap`·`options`·`target`)에서는 표지 문자열 `[kanban-wait` 을 `［kanban-wait`(전각 괄호)로 바꿔 내보낸다. 결정 키는 바꾸기 전 원문으로 계산한다.
- `relays: [{id, answer, at}]` 를 더한다. 이 대기를 대상으로 하는, 쓰이지 않은 30분 안의 relay 건이다.
- `proxyable`·`blocked` 의 사유 목록은 `multi_select`·`truncated`·`not_allowlisted:<토큰>`·`outside_repo`·`compound`·`expansion`·`guard:irreversible`·`decision`·`input`·`no_key` 다.

`delegate` 는 `{session_id, since, conflicts, modes:{question, permission, decision}, progress:{question:"n/10", permission:"n/10", decision:"n/10"}}` 모양이다.

#### 답하기

| 명령 | 인자 |
|---|---|
| `reply` | `--session --ask <tool_use_id> --answer <text>…(질문 순서) --from pane\|agent` [인증] |
| `permit` | `--session --request --decision allow\|deny --from pane\|agent` [인증] |
| `decide` | `--session --at <결정 id> --text <답> --from pane\|agent` [인증] |
| `send`·`stop`·`spawn` | 변경 없음 |

[인증]은 `--from agent` 일 때만 쓰고, 아래 둘 중 정확히 하나여야 한다.
- `--relay <id>` (+ 선택 `--recommended <label>`)
- `--precedent <질문 순번>:<id>` 반복(권한·결정은 순번 `1`, 질문마다 1~5개) + `--reason <200자 이하>`

보내는 CLI 의 거부 조건:
1. 인증 형태가 틀림.
2. relay 문제: 장부에 없음 · relay 아님 · 이미 쓰임 · 30분 지남 · `target_session`/`target_wait` 가 이 `--session` 과 대상 id 와 다름 · 답이 relay 답과 다름. 권한 relay 의 답은 `허용`→allow, `거부`→deny 로 고정 대응한다.
3. precedent 문제: 장부에 없음 · `human` 아님 · 그 질문 순번의 다른 선례와 키가 다름 · **보낸 답이 그 선례들의 답과 다름**(C1) · 권한인데 `deny` 를 보냄.
4. 답 개수·빈 답·길이가 틀림.

#### 받는 쪽

`inbox take` 와 `inbox wait-permit` 은 r1 과 같이 stdin 으로 원본 사실을 받는다. 거기에 `--from agent` 재검증으로 "원본 키 == 선례 키", 대리 조건 1~7(§2.6), relay 대상·답 일치를 다시 판정한다. 통과하지 못하면 `refused` 다.

#### 장부

| 명령 | 인자 | 비고 |
|---|---|---|
| `ledger record` | stdin 사실 | **모드 전용**. 출처·origin·키·마스킹·relay 를 판정한다 |
| `ledger list` | r1 인자 + `--target-session <id>` `--target-wait <id>` `--unused` | 최신순 |
| `ledger match` | `--key --now` | `{key, eligible, answer, answer_fp, support:{direct, approved}, conflicts, disabled, mode, reasons}` |
| `ledger revoke` | `--entry --reason` | 인증 없음(자율을 줄이는 쓰기) |
| `ledger stats` | `--now` | 종류별 `{mode, compared, agreed, recent5_agreed, promotable, progress, demoted_reason}` + 꺼진 키. **재생 계산** |
| `ledger prune` | `--now` | 180일 · 50MB |
| ~~`ledger shadow`~~ | — | 삭제(I1) |

#### 위임·가드

| 명령 | 변경 |
|---|---|
| `delegate wake` | stdin `state` 에 `pending`(대기별 처음 본 시각), `delivered`(지난 제출·첨부가 성공한 대기 id), `last_human_at`(사람 origin 만), `draft_len`, `turn_running` 을 넣는다. 출력은 `{submit, context, deliver_ids, state, report_due}` 이고, 모드는 제출·첨부가 성공했을 때만 `deliver_ids` 를 다음 `delivered` 로 넘긴다 |
| `guard-bash` | stdin `{command}` → `{deny}` |

### 2.4 기록·inbox·장부 형식 (갱신)

**기록 사본과 기록의 새 사실**
- `permission.input_len:{필드: 원래 길이}`, `asks[id].orig_len:{question, labels:[…]}`, `decision.orig_len` 을 기록한다. 잘림 여부는 CLI 가 저장 길이와 비교해 판정한다(m9).
- `delegate_since`, `delegate_state` 를 기록한다.
- `relay_frames: {tool_use_id: {target_session, target_wait, kind}}` 를 기록한다. 위임 세션 모드가 다시 쓴 질문의 대상이고, 질문이 끝나면 지운다.

**inbox 요청** 은 r1 과 같다(`answer`·`decide`·`permit` + `from`·`auth`). 다만 `auth.precedents` 는 `[{q:1, id:"L…"}]` 모양이다.

**장부 한 건** — r1 형식에서 다음을 바꾸고 더한다.

```json
{
  "...": "r1 필드",
  "origin": "composer|bridge|plugin|…" | null,
  "answer": ["<마스킹된 표시용>"],
  "answer_fp": ["<질문마다 정규화 라벨의 SHA-256 앞 32hex>"] | null,
  "answer_masked": false,
  "target_session": "<relay 의 대상>" | null,
  "target_wait": "<relay 의 대상 대기 id>" | null,
  "used_by": "<relay 를 쓴 장부 건 id>" | null
}
```

- relay 건에서 `session`·`wait_id` 는 relay 를 만든 위임 세션과 그 AskUserQuestion 의 tool_use_id 를 뜻한다. 대상은 `target_*` 에 둔다.
- relay 를 썼다는 표시는 append-only 로 남긴다. `relay-used` 건을 덧붙이고, 읽을 때 합친다.
- `setting` 건은 `{scope:"question|permission|decision|delegate", value:"act|shadow|on|off", origin}` 모양이다. origin 이 사람 경로가 아니면 CLI 가 기록하지 않고 거부한다.

### 2.5 출처 인증 (갱신)

**사람 입력 origin** 은 `composer` 와 `bridge` 다. 판정은 CLI 가 한다(F18). 이 규칙은 세 군데에 쓴다.
- `/kanban delegate …` 의 모든 하위 명령
- 결정 요청에 대한 사람 답 추론(결정 카드가 떠 있는 동안 사람 origin 프롬프트)
- 깨우기의 `last_human_at`

| 경로 | decided_by · via | 조건 |
|---|---|---|
| 묻는 세션 화면 직접 답(질문 `won.local` / 권한 Post·Failure 또는 isError 추론 / 결정 뒤 사람 origin 프롬프트) | human · session | 결정 추론은 사람 origin 일 때만 |
| `--from pane` 으로 받은 답 | human · pane | |
| `--from agent --relay` 로 받은 답 | human · main (+approved) | 재검증 통과 |
| `--from agent --precedent` 로 받은 답 | proxy · main | 재검증 통과 |
| 위임 세션에서 사람이 답한 relay 질문 | `relay` 건만 만든다(`question` 선례가 아님) | 질문이 `relay_frames` 에 있고, 답이 `won.local` 또는 pane 경로 |

**relay 표지 규칙 (C3)**
1. skill 은 질문 문장의 **첫 줄**을 `[kanban-wait <session_id> <wait_id>]` 로 쓴다. 표지는 그 첫 줄 하나만 유효하다.
2. 위임 세션 모드의 AskUserQuestion 훅은 질문을 `relay-frame` 에 넘긴다. CLI 는 다음을 한다.
   - 첫 줄 표지를 고정 머리말 `칸반 대기 · <레포> · <세션 짧은 id> · <종류>` 로 바꾸고, 그 아래에 기록 사본에서 읽은 대상 원문을 붙인다. 권한은 `도구: 대상`, 질문은 원 질문과 선택지, 결정은 ask 와 선택지다.
   - 나머지 본문·선택지·설명에 나오는 `[kanban-wait` 는 전부 무력화한다.
   - 대상을 `target` 으로 돌려준다.
   모드는 `next({...e, questions})` 로 다시 쓴 질문을 띄우고(F19), tool_use_id 와 `target` 을 `relay_frames` 에 기억한다.
3. 사람이 답하면 모드가 `ledger record` 에 `relay_frames[tool_use_id]` 를 사실로 넘긴다. relay 의 대상은 이 사실로 정한다. 질문 텍스트를 다시 파싱하지 않는다.
4. `status`·`board` 는 worker 글의 표지를 무력화해서 내보낸다. skill 이 worker 글을 옮겨 적어도 유효한 표지가 되지 않는다.
5. 사람이 보는 머리말의 대상은 worker 글이 아니라 기록 사본의 원문이다. 사람은 실제 대상을 보고 답한다.

**Bash 가드**
- 모든 atelier 세션의 모드는 Bash 툴 입력 `command` 에 `kanban` 이라는 문자열이 있을 때만 `guard-bash` 를 부른다. 서브에이전트 호출도 포함한다(m6).
- CLI 가 없거나 실패하면 통과시킨다(fail-open).
- CLI 가 막는 것:
  - `atelier kanban` 의 `ledger record` · `forget` · `relay-frame`
  - `reply`·`permit`·`decide`·`send` 에 `--from pane` 이 붙은 호출(I7)
- 판정은 셸 토큰화한 argv 로 한다. 변수·eval·스크립트 파일로 감싼 호출은 막지 못하며, 이것은 제약 조건이다.

**자율을 늘리는 쓰기와 줄이는 쓰기**
- 늘리는 쓰기에는 인증을 요구한다: agent 답, `act`, `delegate on`.
- 줄이는 쓰기에는 인증이 없다: `revoke`, `shadow`, `delegate off`.
- 다만 `act`·`on`·`off`·`shadow` 는 모두 사람 origin 의 슬래시 명령으로만 바뀐다(설정 변경의 출처를 하나로 두기 위해).

### 2.6 선례 규칙 (갱신)

**결정 키** = `<종류><규칙 버전>:` + SHA-256(정규화 부분을 `\x1f` 로 이은 것)의 앞 32 hex. 예: `q1:…`·`p1:…`·`d1:…`. 정규화 부분은 r1 과 같다.
- 권한의 Bash 대상은 키에서 앞 두 토큰을 쓴다(A4).
- 허용 판정은 §2.7 에서 argv 전체로 한다.

**대리 조건**(전부 충족). `ledger match`, 보내는 쪽 검사, 받는 쪽 재검증이 같은 함수를 쓴다.
1. 지지: 같은 키에서 `human`·`used`·`free_text=false`·`answer_masked=false`·`inferred=false` 인 건이 직접 결정 2건 이상이거나, 직접 결정과 추천 승인을 합쳐 3건 이상.
2. 창은 90일이다.
3. 충돌 0: 창 안 같은 키의 human 건(추론·자유 입력 포함)의 `answer_fp` 가 모두 같다.
4. 키가 있고 대기가 `proxyable=true` 다.
5. 키가 꺼져 있지 않다(revoke 뒤 직접 결정 2건이 같은 답이면 다시 켜짐).
6. 종류가 `act` 다.
7. **(신규, C1)** 보낸 답이 선례 답과 같다.
   - 질문: 질문마다 보낸 라벨의 정규화 지문 == 지지 건의 `answer_fp`, 그리고 그 라벨이 원본 선택지 라벨 중 하나.
   - 권한: 선례 답 == 보낸 답 == `allow`.

대리 답(`proxy`)은 1·3 어디에도 세지 않는다.

### 2.7 대리 범위 (v1) — 허용 목록 (C2)

**공통 규칙** (CLI, argv 전체로 판정)
- 명령 문자열을 셸 규칙(작은따옴표·큰따옴표·백슬래시)으로 토큰화한다.
- 다음 중 하나라도 있으면 `proxyable=false` 다.
  - 따옴표 밖의 `; & | < > ( ) ` 줄바꿈 → `compound`
  - 큰따옴표 안이나 밖의 `$`, 백틱, 첫 토큰의 `VAR=값` → `expansion`
  - 허용 목록에 없는 명령·하위 명령·옵션 → `not_allowlisted:<토큰>`
  - 위치 인자 수 초과 → `not_allowlisted:<토큰>`
  - 경로 인자가 그 세션 레포 루트 밖(정규화 뒤, `..` 해석) → `outside_repo`
- 옵션 값은 허용 옵션이 값을 받는다고 정의된 경우에만 다음 토큰을 값으로 먹는다. `--opt=값` 형태도 같은 목록으로 판정한다.

| 명령 | 허용 옵션 (그 밖은 거부) | 위치 인자 |
|---|---|---|
| `ls` | `-l -a -A -h -R -1 -t -r -S -d -F` | 경로 0~5 |
| `cat` | `-n -b -s -A -E -T -v` | 경로 1~5 |
| `head`·`tail` | `-n <N>` `-c <N>` `-q -v` (`-f`·`-F` 거부) | 경로 1~5 |
| `wc` | `-l -w -c -m -L` | 경로 1~5 |
| `pwd` | 없음 | 0 |
| `stat` | `-L` | 경로 1~5 |
| `file` | `-b -i -L` | 경로 1~5 |
| `du` | `-s -h -a -c -d <N>` `--max-depth=<N>` | 경로 0~5 |
| `tree` | `-a -d -f -L <N>` `-I <패턴>` `--noreport` (`-o -H -R` 거부) | 경로 0~1 |
| `grep` | `-r -R -n -i -l -L -c -v -w -x -E -F -H -h -o -s -q` `-A/-B/-C <N>` `-e <패턴>` `--include=` `--exclude=` `--exclude-dir=` | 패턴 1(없으면 `-e` 필수) + 경로 0~5 |
| `rg` | `-n -i -S -l -c -v -w -F -o -u --hidden --no-ignore --files` `-e <패턴>` `-g <glob>` `-t/-T <타입>` `-A/-B/-C <N>` (`--pre --pre-glob -z --search-zip` 거부) | 패턴 0~1 + 경로 0~5 |
| `jq` | `-r -c -S -e` | 필터 1 + 경로 0~5 |
| `sort` | `-n -r -u -f -h -V` `-k <키>` `-t <문자>` (`-o --output -T` 거부) | 경로 0~5 |
| `uniq` | `-c -d -u -i` | 경로 **0~1** (둘째 위치 인자는 출력 파일이므로 거부) |
| `find` | 앞의 시작 경로 0~3 뒤 표현식으로 `-name -iname -path -ipath <패턴>` `-type <c>` `-maxdepth -mindepth <N>` `-mtime -mmin -size <값>` `-newer <경로>` `-empty -print -print0 -ls -prune -not ! -o -a -or -and ( )`(따옴표 안의 괄호) | (표현식 밖 위치 인자 없음) |
| `git status` | `-s -b --short --branch --porcelain` `-u<mode>` | 경로 0~5 |
| `git log` | `--oneline --graph --decorate --stat --name-only --name-status --no-color --all -p` `-n <N>` `-<N>` `--format=` `--pretty=` `--since=` `--until=` `--author=` `--grep=` | rev 0~2 + `--` 뒤 경로 0~5 |
| `git diff` | `--stat --name-only --name-status --cached --staged --no-color -w` `-U<N>` | rev 0~2 + `--` 뒤 경로 0~5 |
| `git show` | `--stat --name-only --name-status --oneline --no-color` `--format=` `--pretty=` | rev 0~3 + `--` 뒤 경로 0~5 |
| `git rev-parse` | `--show-toplevel --git-dir --abbrev-ref --short --verify` | rev 0~2 |
| `git ls-files` | `-c -m -o -d -z --others --exclude-standard` | 경로 0~5 |
| `git branch` | `--list -a -r -v -vv --show-current` | **0** (이름 인자는 브랜치를 만드므로 거부) |

- `git` 은 `git <하위 명령>` 형태만 받는다. 하위 명령 앞의 `-c -C --git-dir --work-tree --exec-path` 등 전역 옵션은 거부한다.
- 도구 `Read`·`Glob`·`Grep` 은 대상 경로가 레포 안일 때만 받는다.
- `WebFetch`·`WebSearch`, MCP 도구, 그 밖 도구는 v1 에서 대리하지 않는다.
- 질문: 단일 선택 질문을 선택지 라벨로 답하는 것만 대리한다. 여러 질문은 질문마다 조건을 충족해야 한다.
- 결정 요청·복수 선택·권한 거부·허용 목록 밖은 늘 묻는다.
- 되돌리기 어려움 가드(`guard:irreversible`)는 r1 §2.7 과 같다.

### 2.8 그림자와 승격 (갱신)

- 처음에는 질문·권한·결정 모두 `shadow` 다.
- **재생 비교**(`ledger stats`, CLI 결정적 계산): 창(90일) 안 `human` 건마다 그 건 이전의 장부만으로 대리 조건 1~5 를 만족했는지 계산한다. 만족했다면 비교 1건이고, 그때의 선례 답이 그 사람 답과 같으면 일치다. 경로(session·pane·main)는 따지지 않는다.
- **승격 권고** `promotable`: 비교 10건 이상, 불일치 1건 이하, 최근 5건 모두 일치.
- 진행도 `progress = "<비교 수>/10"` 를 `status.delegate` 와 일일 보고에 싣는다.
- **승격**: 사람이 `/kanban delegate act question|permission` 을 입력하면 기준에 못 미쳐도 받는다. 모드는 실행 전 현재 통계 한 줄(비교·일치·권고 여부)을 결과 문구로 보여 준다.
- `act decision` 은 v1 에서 거부한다(`결정 요청은 아직 대신 답하지 않아요`). 결정 요청의 비교는 계속 쌓인다(A16).
- **강등**: `act` 인 종류에서 7일 안 revoke 2건이 나오면 자동으로 `shadow` 로 본다. 사람의 `/kanban delegate shadow <kind>` 로도 내릴 수 있다.
- `shadow` 동안 skill 은 `ledger match` 가 `eligible` 인 대기를 추천 없이 묻는다(A9). 그렇지 않은 대기에는 추천안을 붙인다.

### 2.9 위임 세션과 깨우기 (갱신)

`/kanban delegate on|off` 는 사람 origin 일 때만 받는다. `/kanban` 만 입력하면 지금처럼 pane 을 토글하고, 인자가 있으면 토글하지 않는다(m1).

CLI `delegate wake` 판정:
1. 새 대기: 다른 세션의 `wait` 중 `delivered` 에 없는 것을 `pending` 에 처음 본 시각과 함께 넣는다.
2. 첫 pending 이 생긴 뒤 10초 모은다.
3. 바로 제출하는 조건: 턴이 쉼 · `draft_len == 0` · 사람의 마지막 입력 후 30초가 지남 → `submit`.
4. 조건 3 이 안 되면 → `context`. 모드는 사람 origin 의 다음 프롬프트에 맥락을 붙인다.
5. **보류 상한**: 한 pending 이 처음 본 뒤 3분을 넘으면 draft·30초 조건을 무시하고, 턴이 쉬기만 하면 `submit` 한다. draft 는 남는다(spike 4).
6. `deliver_ids` 는 이번 출력이 담은 대기다. 모드는 제출(`$.prompt.submit` 이 받아들여짐) 또는 맥락 첨부(사람 프롬프트의 훅이 `context` 를 붙임)가 성공했을 때만 다음 `state.delivered` 에 넣는다. 실패하면 pending 으로 남아 다음 tick 에 다시 판정된다.
7. 끝난 대기는 `pending`·`delivered` 에서 지운다(최대 200).
8. 전달 실패(`dropped·refused·expired·stale`)는 2번의 모음 없이 3~5번 규칙을 따른다.

### 2.10 skill 의 대기 처리 순서 (갱신)

1. `status --column needs_input` 또는 `kanban_board`(`column`) 로 대기를 읽는다.
2. `proxyable=false` 면 묻기로 간다.
3. `ledger match` 를 부른다.
   - `eligible` 이고 `mode=act` → `--from agent --precedent <q>:<id>… --reason …` 로 보내고, 답은 `match.answer` 그대로 쓴다.
   - `eligible` 이고 `mode=shadow` → 추천 없이 묻는다.
4. 묻기: AskUserQuestion 의 질문 첫 줄을 `[kanban-wait <session> <wait>]` 로 쓰고, 그 아래에 맥락을 재구성한다. 권한 질문의 선택지는 `허용`/`거부` 다. 답을 받으면 `status --session <대상>` 의 `wait.relays` 에서 relay id 를 얻고, `--from agent --relay <id> [--recommended]` 로 전달한다.
5. 사용자가 뒤집기를 말하면 `ledger revoke` 를 남긴다.

### 2.11 ~ 2.14

일일 보고·성향 프로필·보존·마스킹·실패 처리는 r1 과 같고, 아래만 바뀐다.
- 일일 보고 ⑥: 그림자 진행도 `n/10`, `promotable`, 강등 사유를 싣는다. 결정 요청 비교 현황을 따로 싣는다.
- 성향 프로필 입력: 사용자 전역 CLAUDE.md·rules, 메인 레포 CLAUDE.md·rules, 메인 레포의 자동 메모리(A19), suggest-workflow, 지난 대화. worker 레포 지침은 제외한다.
- 장부 잠금은 `std::fs::File::lock`(F17) 으로 한다.
- 마스킹이 답 라벨을 바꾼 건에는 `answer_masked=true` 를 붙인다. 이런 건은 지지에서 제외된다(조건 1).

---

## 5. 가정·에스컬레이션·종료 보고

### 가정 (r1 A1~A15 유지, 아래 변경·추가)

| # | 가정 | 근거 |
|---|---|---|
| A6' | 승격 권고: 90일 창 비교 10·불일치 ≤1·최근 5 일치. 자동 강등: 7일 revoke 2. 권고는 강제가 아님 | 메인 결정 1, advisor-delta1 E2 |
| A10' | 허용 목록은 §2.7 표가 전부이고 기본 거부다. 표를 넓히는 것은 CLI 규칙 변경이며 테스트 표를 함께 갱신한다 | 메인 결정 2 |
| A15' | 옛 `permit` 은 pane 으로 본다. 옛 store 답 키는 한 릴리스 동안 받는 쪽이 계속 읽는다 | m3 |
| A16 | **결정 요청은 v1 에서 대신 답하지 않는다.** 늘 메인 대화에서 묻고, 장부에는 `decision` 으로 쌓고 재생 비교도 계산한다. `act decision` 은 v1 에서 거부한다 | 메인 결정 5, 자문 R9 |
| A17 | `sha2 = "0.10"`(RustCrypto, 순수 Rust, 시스템 의존 없음)을 추가한다. 키는 앞 128비트. 크레이트는 기본 허용된 패키지 레지스트리에서 받는다 | F16, 메인 결정 8 |
| A18 | 새 pane → 옛 모드 세션의 질문 답은 버려진다(spec 실패 처리에 명시) | m3 |
| A19 | 성향 근거 "Claude Code 메모리" 에 메인 레포의 자동 메모리를 넣는다. 추천에만 쓴다 | m7, 사용자 결정 4 "CLAUDE.md 등" |
| A20 | `tool.call` 훅의 `next({...e, questions})` 로 AskUserQuestion 질문을 다시 쓸 수 있다. 안 되면 T3b 는 "첫 줄이 정확히 표지 하나이고 본문에 표지가 없을 때만 relay" 를 검증만 하는 형태로 내려가고, 머리말의 대상 원문 표시는 skill 템플릿 책임이 된다 | F19 |
| A21 | 사람 입력 origin 은 `composer`·`bridge` 둘이다. `sdk`(claude -p·SDK 호스트)는 사람 입력으로 보지 않는다 | F18 |
| A22 | 보류 상한은 3분이다. 권한 대기 수명(약 30분, F8)보다 충분히 짧다 | 메인 결정 10 |

### 에스컬레이션 후보

없음. r1 의 E1~E4 는 advisor-delta1 의 메인 결정으로 닫혔다.
- E1 수용
- E2 부분채택(그림자 시작 + 사람 act 수용)
- E3 채택(재설계)
- E4 채택

E2 의 근거 문장도 바로잡는다. 요구 5 "자율주행해줘" 는 스펙이 나온 뒤 **구현 런을 자율로 진행하라**는 뜻이고, 런타임의 대리 자율 수준을 말한 것이 아니다. 그림자 시작은 사용자 결정 "처음부터 다 위임하기보다" 에 근거한다.

### 종료 보고에 넣을 항목 (orchestrator 최종 보고용)

- **R1 (첫 항목)**: 대리는 꺼진 채(지켜보기) 시작한다. 켜는 법은 메인 세션에서 `/kanban delegate on`, 대리를 켤 때는 `/kanban delegate act question` 또는 `/kanban delegate act permission` 이다. 진행도는 `atelier kanban status` 의 `delegate.progress` 와 일일 보고에서 본다.
- **R2**: 같은 사용자 권한 프로세스가 장부·요청 파일을 직접 고치는 것은 막지 않는다. 명령 인자로 꾸미는 경로만 닫았다.
- **R3**: 성향 근거에서 worker 레포의 CLAUDE.md·규칙은 뺐다. worker 가 고칠 수 있는 오염 경로이기 때문이다.
- **R4**: 결정 요청은 v1 에서 대신 답하지 않는다. 장부에 쌓고 비교를 계산해 다음 승격의 근거로 남긴다.
- **R5**: 권한 대리는 기본 거부 허용 목록의 읽기 명령에 대한 허용만이다. 웹·외부 호출은 제외했다.

미결: 없음

---

# 부록 C — 검증 r2 의 구현 지시 (pass 조건)

## 구현 지시 (pass 조건 — 해당 task 의 dispatch 패킷에 그대로 싣는다)

| # | task | 지시 | 결정적 검증 |
|---|---|---|---|
| K1 | T3a | `answer_fp` 를 모든 종류(질문 라벨·권한 allow/deny·결정 답)에 채운다. 조건 3 은 종류 무관 지문 일치로 판정 (N1) | `cargo test --test kanban_precedent`: 직접 허용 2 + 추론 거부 1 → `eligible=false`; pane 거부 1 → `false` |
| K2 | T3a | 질문 대기는 질문마다 장부 건 하나(`q_index`·`q_count`·공통 `wait_id`) 또는 `keys[]`/`answer_fp[]` 중 하나로 고정 (N2) | 질문 2개 대기 기록 → 질문별 `ledger match --key` 가 각각 지지 1건씩 |
| K3 | T3a | 장부 건에 기록 당시 `proxyable`·`blocked` 저장 (m-a) | 재생 테스트에서 proxyable=false 건은 비교에서 빠짐 |
| K4 | T3b-2 | `relay-frame` 머리말에 잘림 표시 `(잘림 — 원래 N자 중 앞 M자)` (N3) | 잘린 대상 입력 → 머리말 문자열에 잘림 표시 |
| K5 | T3b-2 · T5 | AskUserQuestion 한 번 = 대상 하나, 표지는 `questions[0]` 첫 줄만, 질문 수는 대상 질문 수와 같음. 선택지는 원래 라벨 또는 `허용`/`거부` 로 CLI 가 덮어씀 (N4, m-e) | 질문 수 불일치 → `target:null`; 둘째 질문 표지 → 무효; 출력 선택지 == 원래 라벨 |
| K6 | T3b-2 | 다시 쓰기가 안 될 때는 `$.ui.ask` 경로 또는 정지(spec drift). skill 템플릿으로 내려가지 않음. 스키마 거부 시 원래 질문 + relay 없음 (N5) | `claude plugin test`: `next` 가 다시 쓴 `questions` 를 받음; CLI 실패·스키마 거부 → 원래 질문, `relay_frames` 없음 |
| K7 | dispatch | T3b 를 T3b-1(선례)·T3b-2(relay)·T3b-3(재생, T4 와 묶어도 됨)으로 나눈다. `--from agent` 는 인증 형태별로 해당 하위 task 끝에서 연다 (N6) | 각 하위 task 의 `cargo test --test …` 목록을 D2:558-563 에서 나눠 배정 |
| K8 | T1b · T3b-1 | 경로 판정은 받는 쪽에서 canonicalize(심볼릭 링크 해석) 후 레포 안 여부. `$` 는 작은따옴표 안도 `expansion` (m-c, m-i) | 레포 안 링크→밖 대상: `outside_repo`; `jq '.a \| $x'` → `expansion` |
| K9 | spec 커밋 | 제약 조건에 "레포의 git 설정이 실행하는 명령", "따옴표로 쪼갠 낱말" 추가. 정책 13·5 의 구현 세부 문장 다듬기 (m-b, m-d, m-f) | spec 커밋 diff 리뷰 |
| K10 | T3a | 동시 추가 테스트를 결정적으로 만들거나 `#[ignore]` 처리 (m-g) | 통과 판정 목록에 확률적 테스트 없음 |

# atelier

> Epic 1 (consolidation, [#765](https://github.com/kys0213/kys-claude-plugin/issues/765)) + Epic 2 (skill extraction, [#766](https://github.com/kys0213/kys-claude-plugin/issues/766)) 완료.
> 설계: [`plans/atelier/`](../../plans/atelier/) · 상위 epic: [#738](https://github.com/kys0213/kys-claude-plugin/issues/738)

**atelier**(공방)는 개발 워크플로우를 처음부터 끝까지 책임지는 단일 큐레이션 plugin입니다.
spec 설계 → 리뷰 → 구현 → PR 머지까지의 전체 흐름을 하나의 책임 경계 안에서 제공합니다.

흩어져 있던 6개 plugin을 흡수해, 묵시적 의존과 중복 책임을 명시적 단일 namespace로 정리합니다.

## 흡수 매핑 (6 → 1)

| 기존 plugin | 동결 버전 | atelier 내 위치 |
|---|---|---|
| `git-utils` | 2.4.2 | `skills/git/`(+references), `cli/` (Rust 포팅) |
| `github-autopilot` | 0.30.1 | **제거됨** — 에이전트 스웜이 클로드만으로 동작하게 되어 GitHub 이슈 구동 autopilot 루프를 걷어내고, 자율 개발은 `skills/orchestrator/`(기본 자율 주행)가 담당 |
| `spec-kit` | 0.7.1 | `agents/spec/*`, `skills/spec-write/`·`skills/spec-review/`(+issue-report·spec-criteria→references), `templates/spec/` |
| `workflow-guide` | 0.6.0 | `agents/workflow/*`, `skills/{workflow,agent-design-principles}/`, `rules/` |
| `coding-style` | 0.3.0 | `templates/claude-md/`, `hooks/suggest-simplify.sh` |
| `orchestrator` | 0.2.0 | `skills/orchestrator/` |

흡수된 6개 plugin은 **snapshot freeze** 됩니다 — 삭제하지 않고 동결 상태로 보존하며, 후속 개발은 atelier에서만 진행합니다. 마이그레이션 절차는 [`plans/atelier/03-migration.md`](../../plans/atelier/03-migration.md)를 참조하세요.

## 슬래시 표면 (관심사 단위)

Epic 2 ([#766](https://github.com/kys0213/kys-claude-plugin/issues/766))에서 capability 슬래시(35개)를 **관심사 단위**로 수렴했습니다. skill 이 `user-invocable` 이라 슬래시 호출과 모델 자동 호출을 모두 지원하며, 세부 동작은 skill 의 `references/` 로 progressive disclosure 합니다.

### 관심사 skill (슬래시 + 모델 자동 호출)

```
/atelier:spec        # 스펙 설계/리뷰/갭분석/주석/품질평가 — 자연어 의도로 디스패치
/atelier:git         # git 워크플로우 (커밋·push·PR·충돌 해결·리뷰 정리·이슈 우선순위)
/atelier:workflow    # 컨벤션 scaffold·.claude/rules 설계·설계 원칙 룰 설치·워크플로우 리뷰
/atelier:orchestrator # 위임/병렬 분해·worktree 격리·머지 조정 (기본 자율 주행, HITL opt-out)
/atelier:grill       # 이미 있는 계획·설계를 대화로 심문 (빈틈·가정 드러내기)
/atelier:brainstorm  # 무에서 설계를 대화로 생성 (발산→수렴)
```

### 유지 command (deliberate 진입점)

```
/atelier:setup       # 통합 setup (git / style / workflow 모듈 + hook 관리)
```

자율 개발 루프는 별도 진입점 없이 `/atelier:orchestrator` 가 기본 자율 주행으로 수행합니다.

capability 슬래시(commit-and-pr, prioritize-issues, hook-config, scaffold-conventions 등)는
모두 위 관심사 진입점으로 흡수되었습니다 — 슬래시 없이 자연어로 요청해도 해당 skill 이 자동 트리거됩니다.

### 기계적 호출만 CLI

`atelier git <reviews|guard|hook>` 등 hook·구조화 read 처럼 **기계적 호출이 꼭 필요한** 연산은 슬래시도 skill 도 아닌 Rust CLI 가 담당합니다 (CLAUDE.md 책임 경계). 커밋·브랜치·PR 은 git/gh 가 이미 결정적이라 CLI 로 감싸지 않고, skill 이 컨벤션을 적용해 plain git/gh 로 실행합니다.

## CLI

atelier는 단일 Rust crate(`cli/`)로 빌드되며, 바이너리 `atelier` 하나가 subcommand로 라우팅합니다.

```
atelier git <reviews|guard|hook>   # git-utils 의 기계적 호출 표면 (TypeScript → Rust 포팅)
atelier notify ask-question        # AskUserQuestion hook 페이로드를 메시지 채널로 전달
atelier notify notification        # Notification hook (권한 요청·유휴 대기)을 채널로 전달
```

기존 `git-utils` 호출 호환을 위한 alias는 `/atelier:setup`이 안내합니다.

## 대기 알림 (notify)

자율주행/백그라운드 세션이 사용자 입력을 기다리기 시작하는 순간을 사전 설정된 채널로
전달합니다 — 자리에 없거나 딴 작업 중일 때 "지금 세션이 나를 기다린다"를 빠르게 캐치하는 게 목적입니다.

| 상황 | 맞는 채널 |
|---|---|
| 자리에 없음 (폰으로 알아야 함) | `slack` (push) |
| 같은 머신에서 딴 작업 중 (다른 창/프로젝트) | `desktop` (OS 알림 배너) |
| 워처 세션/자동화가 반응해야 함 | `file` + Monitor (poll) |
| 자체 수신 서버·릴레이 보유 | `webhook` (push) |

| hook 이벤트 | 시점 | 전달 내용 |
|---|---|---|
| `PreToolUse` (matcher `AskUserQuestion`) | Claude 가 질문 도구를 호출 | 질문·선택지 전문 |
| `Notification` | 도구 **권한 요청**, 유휴 대기 | 알림 메시지 (예: "Claude needs your permission to use Bash") |

- **훅**: 둘 다 plugin 번들 `hooks/hooks.json` 으로 자동 선언됩니다 (별도 setup 불필요).
  shim(`notify-relay.sh`)은 부트스트랩만, 파싱·채널 결정·전송은 CLI 담당.
- **advisory 계약**: 어떤 경우에도 exit 0 — 채널 미설정이면 무음 no-op, 전송 실패는
  JSON 리포트로만 남고 도구 호출을 절대 차단하지 않습니다 (curl `--max-time 5`).

### 채널 설정

`/atelier:setup` 의 `notify` 모듈로 대화형 설정하거나 직접 작성합니다.
해석 순서: **env → 프로젝트 config → 글로벌 config** (env 가 하나라도 있으면 env 만 사용):

```bash
# 1순위: env
export ATELIER_NOTIFY_SLACK_WEBHOOK_URL="https://hooks.slack.com/services/..."
export ATELIER_NOTIFY_WEBHOOK_URL="https://my-relay.example/hook"   # 범용 JSON POST
export ATELIER_NOTIFY_FILE="~/.claude/atelier-notify/events.jsonl"  # 로컬 JSONL 싱크
export ATELIER_NOTIFY_DESKTOP=1                                     # OS 알림 배너
```

```jsonc
// 2순위: <project>/.claude/atelier-notify.json (프로젝트별 — webhook URL 은 시크릿이므로 gitignore 권장)
// 3순위: ~/.claude/atelier-notify.json (글로벌 — 한 번 설정하면 모든 프로젝트 세션에 적용)
{
  "channels": [
    { "type": "desktop" },
    { "type": "slack", "webhookUrl": "https://hooks.slack.com/services/..." },
    { "type": "webhook", "url": "https://my-relay.example/hook" },
    { "type": "file", "path": "~/.claude/atelier-notify/events.jsonl" }
  ]
}
```

- `slack`: Incoming Webhook 으로 사람이 읽는 `{"text": ...}` 메시지 전송 (push).
- `desktop`: 같은 머신에 OS 알림 배너 (macOS `osascript`, Linux `notify-send`) —
  네트워크·시크릿 불필요, 딴 창에서 작업 중일 때 가장 빠른 캐치.
- `webhook`: 구조화 이벤트(`{"event":"ask_user_question"|"notification", ...}`)를
  임의 URL 로 POST (push) — Discord relay·메일 브리지·자체 서버 등 수신 서버가 있을 때.
  이메일 등 새 채널은 `Channel` enum 확장 지점으로 남겨두었습니다.
- `file`: 같은 구조화 이벤트를 **한 줄 JSONL 로 append** (poll) — 수신 서버가 없는
  로컬 소비자용. `~/` 는 `$HOME` 으로 확장되고 디렉토리는 자동 생성됩니다.

### Monitor 도구로 소비 (polling)

[Monitor 도구](https://code.claude.com/docs/en/tools-reference#monitor-tool)는 webhook 을
수신하는 게 아니라 **백그라운드 명령의 stdout 라인 = 이벤트**로 받는 폴링 모델입니다.
`file` 채널이 그 짝입니다 — 워처 세션에서 이벤트 파일을 tail 하면 다른 세션이 입력을
기다리기 시작하는 순간 라인 단위로 반응할 수 있습니다:

```
# 워처 세션에서 Claude 에게:
"~/.claude/atelier-notify/events.jsonl 을 tail -F 로 모니터링하다가
 이벤트가 오면 어떤 세션이 뭘 기다리는지 알려줘"
```

전 세션 공용 싱크(`~/.claude/...`)로 두면 모든 프로젝트의 대기 이벤트가 한 파일에
모입니다. 플러그인 자동 monitor 선언(`monitors/monitors.json`)은 모든 세션이 서로의
이벤트에 반응해 노이즈가 되므로 넣지 않았습니다 — 워처 세션에서만 opt-in 하세요.

설정 확인은 페이로드를 직접 흘려보면 됩니다:

```bash
echo '{"tool_input":{"questions":[{"question":"test?"}]}}' | atelier notify ask-question
```

## 상태

| Phase | 내용 | 상태 |
|---|---|---|
| Phase 0 | 사전 검증 | ✅ |
| Phase 1 | 골격 (plugin.json · README · marketplace WIP entry) | ✅ |
| Phase 2 | CLI 통합 (Rust 단일 바이너리 — autopilot 흡수 + git-utils 포팅) | ✅ |
| Phase 3 | commands / agents / skills / hooks 이동 + namespace 치환 | ✅ |
| Phase 4 | CI 인프라 (validate · rust-binary · frozen 게이트 · bumpversion 제외) | ✅ |
| Phase 5 | 흡수 6개 freeze | ✅ |

> **현재 상태**: Epic 1 (consolidation) + Epic 2 (skill extraction) 완료.
> 에이전트 스웜이 클로드만으로 동작하게 되어 GitHub 이슈 구동 autopilot 서브시스템(skill·agents·commands·CLI 모듈)을 제거하고,
> 자율 개발은 `orchestrator` skill 의 **기본 자율 주행**(HITL opt-out)으로 통합했습니다.
> 단일 `atelier` 바이너리는 `atelier git <...>` 를 제공하며,
> 슬래시 표면은 capability 35개 → 관심사 단위로 수렴되었습니다.
>
> ⚠️ `gh` CLI 의존 git 명령(reviews, guard pr)은 mock 단위 테스트만 완료 —
> 실제 `gh`/네트워크 라이브 검증은 정식 릴리스 전 별도 수행이 필요합니다.

# agent team 이 꺼져 있으면 SessionStart 에서 사용자 settings 에 켜 둔다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-09-30 · 브랜치: claude/adoring-babbage-6rkjv8 · 이슈: 없음 (계기: #904 검토 중 파생) · 승인 출처: 사용자 승인 (2026-09-30 "setting.json 에 env 로 팀모드를 활성화하는 스크립트" · 기본값 "켜야 제대로 쓸 수 있으니")

## 요구사항

1. orchestrator 의 아키텍트 협의체·자문단은 team 등급이 필수라 agent team 이 꺼진 환경에서는 폴백 없이 에스컬레이션된다. 사용자가 매번 개입하지 않아도 team 이 켜진 상태로 돌게 한다.
2. 켜는 방법은 사용자 `settings.json` 의 `env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS="1"` 이다. 이를 스크립트로 넣는다.
3. 기본값은 켜짐이다 (사용자 결정).

## 현재 상태 (사이드이펙트 조사)

- Claude Code 문서 기준으로 plugin 이 env 를 켤 수 있는 공식 통로는 없다.
  - `userConfig` 값은 `settings.json` 의 `pluginConfigs` 에 저장될 뿐 `env` 를 바꾸지 않는다. hook 에는 `CLAUDE_PLUGIN_OPTION_<KEY>` 로 넘어온다.
  - plugin 루트 `settings.json` 은 `agent`·`subagentStatusLine` 만 지원한다.
  - SessionStart 의 `CLAUDE_ENV_FILE` 은 세션 안 Bash 명령에만 적용된다.
- hook 이 실행하는 스크립트는 파일을 직접 쓸 수 있다. env 는 Claude Code 시작 시점에 읽으므로 **쓴 다음 세션부터** 적용된다.
- agent team 은 실험 기능이다. user scope `env` 라 atelier 밖의 세션에도 켜진다. `-p`·Agent SDK 에서는 켜도 teammate 가 뜨지 않는다.
- 기존 번들 hook 은 전부 "비차단·출력만" 이라 게이트 없이 모든 세션에 둘 수 있었다. 설정을 쓰는 hook 은 이 전제 밖이다.
- `serde_json` 기본 Map 은 키를 정렬해 다시 쓴다. 사용자 전역 설정을 무인 편집하면서 키 순서를 바꾸는 것은 불필요한 변경이다. `preserve_order` 를 켜도 기존 테스트 277개는 그대로 통과했다 (setup 의 `hook register` 출력도 순서 보존으로 바뀐다).

## 결정

1. **판정·쓰기는 CLI**: `atelier session ensure-env --settings <file> --key <K> --value <V>`. 키가 없을 때만 추가하고, 이미 있으면 값과 무관하게(`"0"` 포함) 쓰지 않는다. JSON 이 깨졌거나 루트·`env` 가 객체가 아니면 쓰지 않고 이유를 한 줄 출력한다. 쓰기 전 `.bak-<timestamp>` 백업, temp → rename 교체. 심링크면 대상 파일에 쓰고(링크를 일반 파일로 바꾸지 않는다) 기존 파일 권한을 유지한다. 항상 exit 0.
2. **shim 은 self-gate 만**: `hooks/ensure-agent-teams.sh` 가 동의 값이 `false`/`0` 이거나, 프로세스 env 에 이미 `1` 이 있거나, CLI 가 없으면 무음 종료한다. 경로는 `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json`.
3. **동의는 `userConfig.agent_teams`** (boolean, 기본 `true`). 미설정도 켜짐으로 읽는다.
4. **`serde_json` `preserve_order` 활성화**.
5. `tool-layer-boundary` 룰에 "설정을 쓰는 hook 은 userConfig 동의로 self-gate, 없는 키만 추가" 를 더한다.

## 버린 선택지

- **`/atelier:setup` 에서 한 번만 켜기**: 사용자가 setup 을 다시 돌려야 해서 "개입 없이" 요구에 맞지 않는다.
- **값이 `"1"` 이 아니면 덮어쓰기**: 사용자가 끌 방법이 사라진다.
- **동의 게이트 없이 켜기**: 전역·실험 기능을 사용자가 모르는 채 켜게 된다. 기본 켜짐 + 설치 시 대화상자로 한 번은 보게 한다.
- **shim 에서 jq 로 직접 편집**: 결정적 로직을 테스트 불가능한 bash 에 두게 된다.

## 범위 밖

- #904 (자율 모드에서 협의체 없이 plan·task 확정)의 수정. team 이 켜져도 메인이 협의체를 소집하지 않는 문제는 그대로라 별도 변경으로 다룬다.

## 확인 방법

- `cargo test --test session_ensure_env` — 추가·멱등·기존 값 보존·키 순서·깨진 파일 거부·백업·심링크·권한·CLI exit 0.
- 임시 `HOME` 으로 shim 실행: 동의 false → 무변경 / 첫 실행 → 추가 + 백업 / 재실행 → 무음 / 값 `"0"` → 유지 / CLI 미설치 → 무음.
- `claude plugin validate plugins/atelier` (manifest `userConfig` 포함) · `make validate-ci`.

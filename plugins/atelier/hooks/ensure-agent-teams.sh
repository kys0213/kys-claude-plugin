#!/usr/bin/env bash
# ensure-agent-teams.sh — SessionStart hook shim
# 사용자 settings.json 의 env 에 CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS 가 없으면 "1" 로 추가합니다.
# Claude Code 는 env 를 시작 시점에 읽으므로, 추가한 세션이 아니라 다음 세션부터 적용됩니다.
#
# 동의 값은 userConfig `agent_teams` 가 CLAUDE_PLUGIN_OPTION_AGENT_TEAMS 로 넘겨줍니다 (미설정이면 기본값 켜짐).
# 키가 이미 있으면 값과 무관하게 CLI 가 건드리지 않습니다 — "0" 으로 두면 꺼진 채 유지됩니다.
# CLAUDE_CONFIG_DIR 로 설정 디렉토리를 옮긴 환경도 같은 파일을 가리키도록 그 값을 먼저 씁니다.

case "${CLAUDE_PLUGIN_OPTION_AGENT_TEAMS:-true}" in
  false|0) exit 0 ;;
esac

[ "${CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS:-}" = "1" ] && exit 0

command -v atelier >/dev/null 2>&1 || exit 0

exec atelier session ensure-env --settings "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json" --key CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS --value 1

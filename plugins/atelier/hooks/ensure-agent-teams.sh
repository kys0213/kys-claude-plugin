#!/usr/bin/env bash
# ensure-agent-teams.sh — SessionStart hook shim
# Claude Code 는 env 를 시작 시점에 읽으므로, 여기서 추가한 값은 다음 세션부터 적용됩니다.
# 동의 값은 userConfig `agent_teams` 가 CLAUDE_PLUGIN_OPTION_AGENT_TEAMS 로 넘겨줍니다.

case "${CLAUDE_PLUGIN_OPTION_AGENT_TEAMS:-true}" in
  false|0) exit 0 ;;
esac

[ "${CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS:-}" = "1" ] && exit 0

command -v atelier >/dev/null 2>&1 || exit 0

exec atelier session ensure-env --settings "${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json" --key CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS --value 1

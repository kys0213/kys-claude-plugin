# drift-common.sh — check-drift.sh / sync-artifact.sh 공용 상수
#
# 마커 문자열과 산출물 경로의 단일 출처. 두 스크립트가 각자 정의하면
# 한쪽만 수정됐을 때 판정(check)과 쓰기(sync)가 조용히 어긋난다.
# shellcheck shell=bash

# 마커는 전체 라인 일치로만 인식한다 — 본문의 부분 문자열 언급에 오반응하지 않도록.
# shellcheck disable=SC2034  # 소비자는 source 하는 쪽
BEGIN_MARKER='<!-- [coding-style:begin] DO NOT REMOVE THIS LINE -->'
# shellcheck disable=SC2034
END_MARKER='<!-- [coding-style:end] DO NOT REMOVE THIS LINE -->'

# 플러그인 루트 기준 상대 경로 (호출자가 $PLUGIN_DIR 을 앞에 붙인다)
# shellcheck disable=SC2034
TEMPLATE_CLAUDE_MD_REL="templates/claude-md/CLAUDE.md"
# shellcheck disable=SC2034
TEMPLATE_RULES_REL="rules/agent-design-principles.md"
# 프로젝트 루트 기준 rules 복사본 상대 경로
# shellcheck disable=SC2034
RULES_COPY_REL=".claude/rules/agent-design-principles.md"

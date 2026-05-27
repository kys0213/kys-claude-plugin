#!/usr/bin/env bash
#
# render-template — placeholder 치환 기반 결정적 템플릿 렌더러
#
# 사용법:
#   render-template <template> <output> KEY=VAL [KEY=VAL ...]
#
# 동작:
#   - <template> 의 {KEY} placeholder 를 VAL 로 치환하여 <output> 에 작성
#   - placeholder 문법: ASCII 영숫자/언더스코어 ({var}, {snake_case})
#   - 한국어/유니코드 시작 패턴 (예: {컴포넌트명}) 은 placeholder 가 아니며 원본 유지
#   - 출력 디렉토리 부재 시 자동 mkdir -p, 출력 파일 존재 시 덮어쓰기 (멱등)
#
# Exit codes:
#   0   성공
#   1   템플릿 부재, 또는 템플릿에 있는 placeholder 가 인자로 안 들어옴 (strict)
#   2   인자 부족, 또는 KEY=VAL 형식 오류

set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage: render-template <template> <output> KEY=VAL [KEY=VAL ...]

Renders <template> with {KEY} placeholders replaced by VAL,
writes result to <output> (mkdir -p on parent, overwrite).

Exits 1 if template references {KEY} but no matching KEY=VAL given.
EOF
  exit 2
}

[[ $# -lt 3 ]] && usage

template="$1"
output="$2"
shift 2

if [[ ! -f "$template" ]]; then
  echo "render-template: template not found: $template" >&2
  exit 1
fi

declare -A vars
for kv in "$@"; do
  if [[ "$kv" != *=* ]]; then
    echo "render-template: expected KEY=VAL, got: $kv" >&2
    exit 2
  fi
  key="${kv%%=*}"
  val="${kv#*=}"
  if [[ -z "$key" ]]; then
    echo "render-template: empty key in argument: $kv" >&2
    exit 2
  fi
  vars["$key"]="$val"
done

content=$(<"$template")

placeholders=$(printf '%s' "$content" \
  | grep -oE '\{[a-zA-Z_][a-zA-Z0-9_]*\}' \
  | sort -u \
  | sed 's/^{//; s/}$//' || true)

missing=()
for p in $placeholders; do
  if [[ -z "${vars[$p]+set}" ]]; then
    missing+=("$p")
  fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "render-template: missing values for placeholder(s): ${missing[*]}" >&2
  exit 1
fi

for key in "${!vars[@]}"; do
  val="${vars[$key]}"
  content="${content//\{$key\}/$val}"
done

mkdir -p "$(dirname "$output")"
printf '%s' "$content" > "$output"
echo "rendered: $output"

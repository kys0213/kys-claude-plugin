#!/usr/bin/env bash
#
# test-render-template — render-template.sh 블랙박스 테스트
#
# 실행: bash common/scripts/test-render-template.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RENDER="$SCRIPT_DIR/render-template.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

PASS=0
FAIL=0
FAILURES=()

assert_eq() {
  local name="$1" expected="$2" actual="$3"
  if [[ "$expected" == "$actual" ]]; then
    PASS=$((PASS + 1))
    printf '  ok    %s\n' "$name"
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("$name")
    printf '  FAIL  %s\n        expected: %q\n        actual:   %q\n' "$name" "$expected" "$actual"
  fi
}

assert_contains() {
  local name="$1" needle="$2" haystack="$3"
  if [[ "$haystack" == *"$needle"* ]]; then
    PASS=$((PASS + 1))
    printf '  ok    %s\n' "$name"
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("$name")
    printf '  FAIL  %s\n        needle: %q\n        haystack: %q\n' "$name" "$needle" "$haystack"
  fi
}

echo "[case 1] 정상 치환 — 모든 변수 제공"
cat > "$TMP/tpl1.md" <<'EOF'
project: {project}
root: {spec_root}
EOF
"$RENDER" "$TMP/tpl1.md" "$TMP/out1.md" project=my-app spec_root=docs/spec > /dev/null
out=$(<"$TMP/out1.md")
assert_eq "case1.content" "project: my-app
root: docs/spec" "$out"

echo "[case 2] 누락 변수 → exit 1 + 누락 키 메시지"
cat > "$TMP/tpl2.md" <<'EOF'
{project} / {spec_root} / {missing_key}
EOF
set +e
err=$("$RENDER" "$TMP/tpl2.md" "$TMP/out2.md" project=x spec_root=y 2>&1 >/dev/null)
rc=$?
set -e
assert_eq "case2.exit" "1" "$rc"
assert_contains "case2.message" "missing_key" "$err"

echo "[case 3] 출력 디렉토리 자동 생성"
cat > "$TMP/tpl3.md" <<'EOF'
hello {name}
EOF
"$RENDER" "$TMP/tpl3.md" "$TMP/deep/nested/dir/out3.md" name=world > /dev/null
[[ -f "$TMP/deep/nested/dir/out3.md" ]] && rc=0 || rc=1
assert_eq "case3.file_created" "0" "$rc"

echo "[case 4] 인자 부족 → exit 2"
set +e
"$RENDER" only_one_arg 2>/dev/null
rc=$?
set -e
assert_eq "case4.exit" "2" "$rc"

echo "[case 5] 잘못된 KEY=VAL 형식 → exit 2"
cat > "$TMP/tpl5.md" <<'EOF'
{x}
EOF
set +e
err=$("$RENDER" "$TMP/tpl5.md" "$TMP/out5.md" no_equals_here 2>&1 >/dev/null)
rc=$?
set -e
assert_eq "case5.exit" "2" "$rc"
assert_contains "case5.message" "KEY=VAL" "$err"

echo "[case 6] 템플릿 부재 → exit 1"
set +e
err=$("$RENDER" "$TMP/nonexistent.md" "$TMP/out6.md" k=v 2>&1 >/dev/null)
rc=$?
set -e
assert_eq "case6.exit" "1" "$rc"
assert_contains "case6.message" "not found" "$err"

echo "[case 7] 값에 슬래시 포함 (경로) — 안전 치환"
cat > "$TMP/tpl7.md" <<'EOF'
path: {spec_root}
nested: {concerns_path}
EOF
"$RENDER" "$TMP/tpl7.md" "$TMP/out7.md" spec_root=docs/spec concerns_path=docs/spec/concerns > /dev/null
out=$(<"$TMP/out7.md")
assert_eq "case7.content" "path: docs/spec
nested: docs/spec/concerns" "$out"

echo "[case 8] 한국어 {컴포넌트명} 은 placeholder 비매칭 — 원본 유지"
cat > "$TMP/tpl8.md" <<'EOF'
# {project} — {컴포넌트명}
> Flow: {유스케이스명}
EOF
"$RENDER" "$TMP/tpl8.md" "$TMP/out8.md" project=acme > /dev/null
out=$(<"$TMP/out8.md")
assert_eq "case8.content" "# acme — {컴포넌트명}
> Flow: {유스케이스명}" "$out"

echo "[case 9] 초과 인자 (템플릿엔 없는 KEY) — 무시하고 exit 0"
cat > "$TMP/tpl9.md" <<'EOF'
only: {used}
EOF
set +e
"$RENDER" "$TMP/tpl9.md" "$TMP/out9.md" used=A unused=B 2>/dev/null
rc=$?
set -e
assert_eq "case9.exit" "0" "$rc"
out=$(<"$TMP/out9.md")
assert_eq "case9.content" "only: A" "$out"

echo "[case 10] 같은 placeholder 가 여러 번 등장 — 모두 치환"
cat > "$TMP/tpl10.md" <<'EOF'
{name} and {name} again, with {name}.
EOF
"$RENDER" "$TMP/tpl10.md" "$TMP/out10.md" name=Alice > /dev/null
out=$(<"$TMP/out10.md")
assert_eq "case10.content" "Alice and Alice again, with Alice." "$out"

echo "[case 11] 멱등 — 같은 입력 두 번 호출, 동일 결과"
cat > "$TMP/tpl11.md" <<'EOF'
{a}/{b}
EOF
"$RENDER" "$TMP/tpl11.md" "$TMP/out11.md" a=x b=y > /dev/null
first=$(<"$TMP/out11.md")
"$RENDER" "$TMP/tpl11.md" "$TMP/out11.md" a=x b=y > /dev/null
second=$(<"$TMP/out11.md")
assert_eq "case11.idempotent" "$first" "$second"

echo ""
echo "==================================="
echo "PASS: $PASS  FAIL: $FAIL"
if [[ $FAIL -gt 0 ]]; then
  printf '실패한 케이스:\n'
  for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
  exit 1
fi

#!/usr/bin/env bash
# check-decision-graph.sh — orchestrator 축소판 스킬 문서의 상시 검사 8종
#
# 설계: plans/atelier/11-orchestrator-lean.md
#   §2 목표 구조 / 파일별·총합 줄수 상한 → 검사 (5)
#   §4 불변식 표 "금지 → 대신"      → 검사 (12)
#   §5 정지 조건 표 신호·행동·출구  → 검사 (13)
#   §9-b 보존 heading 4개           → 검사 (11)
#   §9-e 검사 스크립트 10종 → 8종   → 이 파일 전체
# 규약: .claude/rules/decision-graph.md (검사 번호 = 어긴 규칙 번호의 대응표)
#
# 입력은 인자로 받은 스킬 디렉토리 안의 *.md 뿐이다 (plan·별도 데이터 파일을
# 파서 입력으로 삼지 않는다 — 순수 함수).
#
# 사용법: check-decision-graph.sh [--verbose] [스킬 디렉토리]
# 종료 코드: 실패 검사 수 > 0 이면 1, 아니면 0
#
# 의존: bash / grep / sed / awk 만. GNU 전용 옵션 미사용 (macOS·Linux 공통).

set -u

# ---------------------------------------------------------------------------
# 설정 — 설계와 함께 고쳐야 하는 값들
# ---------------------------------------------------------------------------

# 구조 heading 화이트리스트 (설계 §2 목표 구조 · 규약 §1).
# 축소판 3파일의 절 이름 전수이며 **완전 일치**로 판정한다. 항목 id 체계가
# 폐지돼(규약 §8) `## D##.` 같은 예외 통로는 없다 — 새 절은 반드시 여기에
# 등재돼야 하고, 그 마찰이 화이트리스트가 산문 재증식의 배출구가 되는 것을 막는다.
#
# `###` 이름(표 1~7, SKILL.md §기본값 표 소속)도 등재해 둔다 — 레벨만 바뀌어도 통과하도록.
# 검사가 강제하는 대상은 `##` 뿐이다.
STRUCT_HEADING_WHITELIST='불변식
정지 조건
표준 절차
기본값 표
계약과 절차
도구 확보
왕복 조율 가용 판정과 spawn 확인
브랜치 규약
산출 경로 계약
prompt 필수 포함 요소
보고 채널
기록 위치와 형식
종료 핸드오프
plan 기록
예산과 가드레일 값
Task 시스템
task 도출 계약
설계 승인 마커
대화 스킬의 자율 어댑테이션
자문 계약
역할별 모델 제약
Authorship
사용자 보고 형식
머지 대상: epic 브랜치
표 1 — 집행 tier
표 2 — team 등급
표 3 — 병렬 · 순차
표 4 — 위임 형태
표 5 — 격리 여부
표 6 — 검토 게이트
표 7 — 모드 · 등록 · 경로
진입 절차
자율 실행 루프
직렬 dispatch 와 토폴로지
토폴로지 복구
worktree 정리
머지 표준 절차
최종 통합 검증 게이트
아키텍트 협의체
자문 소집'

# 크기 상한 (설계 §2 · 규약 §5). 값은 "넘으면 실패"하는 최대 허용치.
# 상한 값의 소유처는 이 파일 하나다 — 규약 문서에 복사하지 않는다.
LIMIT_SKILL=200             # SKILL.md (기본값 표 7개 포함). 201줄+ 실패
LIMIT_CONTRACTS=250         # references/contracts.md. 251줄+ 실패
LIMIT_PROCEDURES=160        # references/procedures.md. 161줄+ 실패
LIMIT_TOTAL=600             # 위 세 파일 총합. 601줄+ 실패

# 예산·가드레일 값의 유일한 소유 절 (규약 §6). 이 절 **밖**에서 아래 이름 옆에
# 숫자 리터럴이 나오면 검사 (9) 실패 — 다른 절은 값 대신 이름만 쓴다.
BUDGET_SECTION='예산과 가드레일 값'
BUDGET_SECTION_FILE='references/contracts.md'

# `§예산과 가드레일 값` 표의 행 이름 13종.
BUDGET_NAMES='done_when
path
spec_resolved
integration_verify
hard_stops
log_dir
max_loops
max_redispatch_per_task
max_council_rounds
max_advisory_consults
탐색 예산
충돌 카운터
no-progress'

# 검사 (9) 에서 "옆"으로 보는 범위 — 이름 뒤 공백 구분 토큰 N개.
# 값 대입 형태(`name 5` · `name 은 5` · `` `name` = 5 `` · `name: 2~3`)는 전부
# 3토큰 안에 들어오고, 이름이 경로 템플릿(`<log_dir>/HANDOFF.md`)으로 쓰인 뒤
# 멀리서 나오는 숫자는 들어오지 않는다.
BUDGET_WINDOW_TOKENS=3

# 외부 계약 표면 (설계 §9-a·§9-b · 규약 §7). 사라지면 CI 는 통과하나
# 외부 스킬의 `§절 이름` 참조가 조용히 죽는다.
#
# `spec 확정 게이트` 만 heading 이 아니라 SKILL.md `## 정지 조건` 표의
# 조건 셀이다 (grill SKILL.md 가 "정지 조건의 그 행"으로 지목한다).
EXTERNAL_ROW_TABLE='정지 조건'
EXTERNAL_ROW_COLUMN='조건'
EXTERNAL_ROW='spec 확정 게이트'
# 나머지 셋은 `## ` heading 으로 세 파일 합쳐 정확히 1곳.
EXTERNAL_SECTIONS='대화 스킬의 자율 어댑테이션
설계 승인 마커
머지 대상: epic 브랜치'
EXTERNAL_FILES='SKILL.md
references/contracts.md
references/procedures.md'

TOTAL_CHECKS=8

# ---------------------------------------------------------------------------
# 인자
# ---------------------------------------------------------------------------

VERBOSE=0
ROOT=""
for arg in "$@"; do
  case "$arg" in
    --verbose|-v) VERBOSE=1 ;;
    -h|--help)
      sed -n '2,24p' "$0"
      exit 0
      ;;
    -*)
      echo "unknown option: $arg" >&2
      exit 2
      ;;
    *)
      if [ -n "$ROOT" ]; then
        echo "too many arguments" >&2
        exit 2
      fi
      ROOT="$arg"
      ;;
  esac
done
[ -n "$ROOT" ] || ROOT="plugins/atelier/skills/orchestrator"
ROOT="${ROOT%/}"

if [ ! -d "$ROOT" ]; then
  echo "[FAIL] (0) 입력: $ROOT — 스킬 디렉토리가 없다"
  echo "checks: $TOTAL_CHECKS, failed: $TOTAL_CHECKS"
  exit 1
fi

TMPDIR_CDG=$(mktemp -d "${TMPDIR:-/tmp}/cdg.XXXXXX") || exit 2
trap 'rm -rf "$TMPDIR_CDG"' EXIT INT TERM

FILES="$TMPDIR_CDG/files"
find "$ROOT" -name '*.md' -type f | LC_ALL=C sort > "$FILES"
if [ ! -s "$FILES" ]; then
  echo "[FAIL] (0) 입력: $ROOT — md 파일이 없다"
  echo "checks: $TOTAL_CHECKS, failed: $TOTAL_CHECKS"
  exit 1
fi

SKILL_FILE="$ROOT/SKILL.md"
BUDGET_FILE="$ROOT/$BUDGET_SECTION_FILE"

FAILED_CHECKS=0
CUR_FAILS=0

fail() { # fail <n> <검사명> <위치> <사유>
  printf '[FAIL] (%s) %s: %s — %s\n' "$1" "$2" "$3" "$4"
  CUR_FAILS=$((CUR_FAILS + 1))
}
begin_check() { CUR_FAILS=0; }
end_check() { # end_check <n> <검사명>
  if [ "$CUR_FAILS" -gt 0 ]; then
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
  elif [ "$VERBOSE" -eq 1 ]; then
    printf '[PASS] (%s) %s\n' "$1" "$2"
  fi
}

# ---------------------------------------------------------------------------
# 스캔 — 모든 파일을 한 번 훑어 구조 레코드를 만든다.
#
# 산출 레코드 (탭 구분):
#   HEAD    file line level text   # 코드펜스 밖 heading (frontmatter 제외)
#   MERMAID file line              # mermaid 코드펜스의 여는 줄
#   FILELEN file lines
# ---------------------------------------------------------------------------

SCAN="$TMPDIR_CDG/scan"
: > "$SCAN"
while IFS= read -r f; do
  awk -v FILE="$f" '
    BEGIN { infm = 0; infence = 0 }
    {
      line = $0
      sub(/\r$/, "", line)

      # frontmatter (파일 최상단 --- ... ---)
      if (NR == 1 && line == "---") { infm = 1; next }
      if (infm) { if (line == "---") infm = 0; next }

      # 코드펜스 토글
      if (line ~ /^```/) {
        n = 0
        while (substr(line, n + 1, 1) == "`") n++
        if (!infence) {
          infence = 1; fencelen = n
          lang = substr(line, n + 1); gsub(/[ \t]/, "", lang)
          if (lang == "mermaid") printf "MERMAID\t%s\t%d\n", FILE, NR
        } else {
          rest = substr(line, n + 1)
          if (n >= fencelen && rest ~ /^[ \t]*$/) infence = 0
        }
        next
      }
      if (infence) next

      # heading
      if (line ~ /^#+[ \t]/) {
        lvl = 0
        while (substr(line, lvl + 1, 1) == "#") lvl++
        text = substr(line, lvl + 1)
        sub(/^[ \t]+/, "", text)
        sub(/[ \t]+$/, "", text)
        printf "HEAD\t%s\t%d\t%d\t%s\n", FILE, NR, lvl, text
      }
    }
    END { printf "FILELEN\t%s\t%d\n", FILE, NR }
  ' "$f" >> "$SCAN"
done < "$FILES"

# ---------------------------------------------------------------------------
# 표 파서 — 지정한 `## <heading>` 절의 **첫 표**를 읽는다.
#
# table_missing <file> <heading> <col> [col] [col]
#   → 필수 열이 빠졌거나 데이터 행의 그 셀이 비었으면 `행\t열\t사유` 를 낸다.
# table_column <file> <heading> <col>
#   → 데이터 행마다 `행\t셀값` 을 낸다 (강조·인라인코드 마커 제거 후).
# ---------------------------------------------------------------------------

TABLE_AWK='
  function trim(s) { sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s); return s }
  function cell(i,   v) { v = trim(c[i + 1]); gsub(/\001/, "|", v); return v }
  BEGIN {
    nreq = split(CN, req, "\\|")
    fence = 0; inh = 0; state = 0; found = 0
  }
  /^```/ { fence = !fence; next }
  fence { next }
  /^#+[ \t]/ {
    t = $0; sub(/^#+[ \t]+/, "", t); t = trim(t)
    lvl = 0; while (substr($0, lvl + 1, 1) == "#") lvl++
    if (state >= 1) state = 3
    inh = (lvl == 2 && t == H) ? 1 : 0
    next
  }
  !inh { next }
  state == 3 { next }
  /^[ \t]*\|/ {
    line = $0
    gsub(/\\\|/, "\001", line)
    nc = split(line, c, "|")
    ncell = nc - 2
    if (ncell < 1) { state = 3; next }

    if (state == 0) {           # 머리행
      found = 1
      for (i = 1; i <= ncell; i++) colname[cell(i)] = i
      for (k = 1; k <= nreq; k++) {
        if (req[k] == "") continue
        if (!(req[k] in colname)) { print 0 "\t" req[k] "\t__NOCOL__"; state = 3 }
      }
      if (state != 3) state = 1
      next
    }
    if (state == 1) {           # 구분행
      issep = 1
      for (i = 1; i <= ncell; i++) if (cell(i) !~ /^:?-+:?$/) issep = 0
      state = issep ? 2 : 3
      next
    }
                                # 데이터 행
    for (k = 1; k <= nreq; k++) {
      if (req[k] == "") continue
      v = cell(colname[req[k]])
      if (MODE == "column") {
        gsub(/[*`]/, "", v)
        print NR "\t" trim(v)
      } else if (v == "") {
        print NR "\t" req[k] "\t__EMPTY__"
      }
    }
    next
  }
  { if (state >= 1 && $0 !~ /^[ \t]*$/) state = 3 }
  END { if (!found) print 0 "\t" H "\t__NOTABLE__" }
'

table_missing() { # table_missing <file> <heading> <col>...
  _f=$1; _h=$2; shift 2
  _cn=""
  for _c in "$@"; do _cn="${_cn:+$_cn|}$_c"; done
  [ -f "$_f" ] || return 0
  awk -v H="$_h" -v CN="$_cn" -v MODE="missing" "$TABLE_AWK" "$_f"
}

table_column() { # table_column <file> <heading> <col>
  [ -f "$1" ] || return 0
  awk -v H="$2" -v CN="$3" -v MODE="column" "$TABLE_AWK" "$1"
}

# ---------------------------------------------------------------------------
# (2) 구조 heading 화이트리스트 (설계 §2 · 규약 §1)
# ---------------------------------------------------------------------------
check_2_struct_whitelist() {
  begin_check
  printf '%s\n' "$STRUCT_HEADING_WHITELIST" > "$TMPDIR_CDG/wl"
  awk -F'\t' -v wlfile="$TMPDIR_CDG/wl" '
    BEGIN { while ((getline l < wlfile) > 0) if (l != "") wl[l] = 1 }
    $1 == "HEAD" && $4 == 2 && !($5 in wl) { print $2 "\t" $3 "\t" $5 }
  ' "$SCAN" > "$TMPDIR_CDG/out2"
  while IFS="$(printf '\t')" read -r f l t; do
    [ -n "${f:-}" ] || continue
    fail 2 "구조 heading 화이트리스트" "$f:$l" "화이트리스트 밖의 heading '## $t' — 절을 늘리려면 설계와 이 목록을 함께 고친다"
  done < "$TMPDIR_CDG/out2"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out2")
  end_check 2 "구조 heading 화이트리스트"
}

# ---------------------------------------------------------------------------
# (5) 크기 상한 — 파일별 3개 + 총합 (설계 §2 · 규약 §5)
# ---------------------------------------------------------------------------
check_5_limits() {
  begin_check
  : > "$TMPDIR_CDG/out5"
  total=0
  for spec in "SKILL.md:$LIMIT_SKILL" \
              "references/contracts.md:$LIMIT_CONTRACTS" \
              "references/procedures.md:$LIMIT_PROCEDURES"; do
    rel="${spec%:*}"
    lim="${spec##*:}"
    fpath="$ROOT/$rel"
    [ -f "$fpath" ] || continue
    n=$(awk 'END { print NR }' "$fpath")
    total=$((total + n))
    if [ "$n" -gt "$lim" ]; then
      printf '%s\t1\t파일 %s줄 (상한 %s)\n' "$fpath" "$n" "$lim" >> "$TMPDIR_CDG/out5"
    fi
  done
  if [ "$total" -gt "$LIMIT_TOTAL" ]; then
    printf '%s\t0\t세 파일 총합 %s줄 (상한 %s)\n' "$ROOT" "$total" "$LIMIT_TOTAL" >> "$TMPDIR_CDG/out5"
  fi
  while IFS="$(printf '\t')" read -r f l msg; do
    [ -n "${f:-}" ] || continue
    fail 5 "크기 상한" "$f:$l" "$msg — 상한을 늘리지 말고 판단 대본이 섞였는지부터 본다"
  done < "$TMPDIR_CDG/out5"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out5")
  end_check 5 "크기 상한"
}

# ---------------------------------------------------------------------------
# (6) 금지 heading — 안티패턴 / 체크리스트 (규약 §1 판단 대본 금지)
# ---------------------------------------------------------------------------
check_6_forbidden_headings() {
  begin_check
  awk -F'\t' '
    $1 == "HEAD" && ($4 == 2 || $4 == 3) {
      if (index($5, "안티패턴") == 1 || index($5, "체크리스트") == 1)
        print $2 "\t" $3 "\t" $5
    }
  ' "$SCAN" > "$TMPDIR_CDG/out6"
  while IFS="$(printf '\t')" read -r f l t; do
    [ -n "${f:-}" ] || continue
    fail 6 "금지 heading" "$f:$l" "'$t' — 안티패턴·체크리스트 절은 전면 삭제 대상이다"
  done < "$TMPDIR_CDG/out6"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out6")
  end_check 6 "금지 heading"
}

# ---------------------------------------------------------------------------
# (7) mermaid 부재 — 그래프는 0개다 (설계 §2 · 규약 §4)
# ---------------------------------------------------------------------------
check_7_no_mermaid() {
  begin_check
  awk -F'\t' '$1 == "MERMAID" { print $2 "\t" $3 }' "$SCAN" > "$TMPDIR_CDG/out7"
  while IFS="$(printf '\t')" read -r f l; do
    [ -n "${f:-}" ] || continue
    fail 7 "mermaid 부재" "$f:$l" "mermaid 펜스 — 순서를 보여야 하면 번호 있는 줄 목록으로 적는다"
  done < "$TMPDIR_CDG/out7"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out7")
  end_check 7 "mermaid 부재"
}

# ---------------------------------------------------------------------------
# (9) 예산 값 유출 — `§예산과 가드레일 값` 절 밖에서 예산 이름 옆의 숫자 리터럴
#     (규약 §6 "값은 예산 값 표에만, 이름은 어디서나")
# ---------------------------------------------------------------------------
check_9_budget_leak() {
  begin_check
  printf '%s\n' "$BUDGET_NAMES" > "$TMPDIR_CDG/budgets"

  # 소유 절의 행 범위를 구한다 (없으면 0 0 — 전 범위를 스캔한다).
  BSTART=0
  BEND=0
  if [ -f "$BUDGET_FILE" ]; then
    range=$(awk -v H="$BUDGET_SECTION" '
      function trim(s) { sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s); return s }
      BEGIN { fence = 0; s = 0; e = 0 }
      /^```/ { fence = !fence; next }
      fence { next }
      /^#+[ \t]/ {
        t = $0; sub(/^#+[ \t]+/, "", t); t = trim(t)
        if (s > 0 && e == 0 && NR > s) e = NR - 1
        if (t == H) s = NR
      }
      END { if (s > 0 && e == 0) e = NR; print s "\t" e }
    ' "$BUDGET_FILE")
    BSTART=$(printf '%s' "$range" | awk -F'\t' '{ print $1 }')
    BEND=$(printf '%s' "$range" | awk -F'\t' '{ print $2 }')
  fi

  : > "$TMPDIR_CDG/out9"
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    if [ "$f" = "$BUDGET_FILE" ]; then xs=$BSTART; xe=$BEND; else xs=0; xe=0; fi
    awk -v FILE="$f" -v XS="$xs" -v XE="$xe" -v NT="$BUDGET_WINDOW_TOKENS" \
        -v bfile="$TMPDIR_CDG/budgets" '
      function isword(ch) { return (ch ~ /^[A-Za-z0-9_-]$/) }
      BEGIN { while ((getline l < bfile) > 0) if (l != "") b[++nb] = l }
      XS > 0 && NR >= XS && NR <= XE { next }
      {
        for (i = 1; i <= nb; i++) {
          name = b[i]
          pos = 1
          while ((p = index(substr($0, pos), name)) > 0) {
            abs = pos + p - 1
            after = abs + length(name)
            before = (abs > 1) ? substr($0, abs - 1, 1) : ""
            if (!isword(before) && !isword(substr($0, after, 1))) {
              nt = split(substr($0, after), tk, /[ \t]+/)
              seen = 0
              for (j = 1; j <= nt && seen < NT; j++) {
                if (tk[j] == "") continue
                seen++
                if (tk[j] ~ /[0-9]/) {
                  print FILE "\t" NR "\t" name "\t" tk[j]
                  break
                }
              }
            }
            pos = after
          }
        }
      }
    ' "$f" >> "$TMPDIR_CDG/out9"
  done < "$FILES"

  while IFS="$(printf '\t')" read -r f l name val; do
    [ -n "${f:-}" ] || continue
    fail 9 "예산 값 유출" "$f:$l" "예산 이름 '$name' 옆에 숫자 리터럴 '$val' — 값은 \`$BUDGET_SECTION_FILE §$BUDGET_SECTION\` 에만 두고 다른 절은 이름만 쓴다"
  done < "$TMPDIR_CDG/out9"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out9")
  end_check 9 "예산 값 유출"
}

# ---------------------------------------------------------------------------
# (11) 외부 계약 표면 실존 (설계 §9-a·§9-b · 규약 §7)
#      절 이름 3 — `## ` heading 으로 세 파일 합쳐 정확히 1곳
#      행 이름 1 — SKILL.md `## 정지 조건` 표의 조건 셀로 정확히 1회 (heading 아님)
#      파일 3   — 스킬 디렉토리 안에 실존
# ---------------------------------------------------------------------------
check_11_external_surface() {
  begin_check
  : > "$TMPDIR_CDG/out11"

  printf '%s\n' "$EXTERNAL_SECTIONS" | while IFS= read -r sec; do
    [ -n "$sec" ] || continue
    cnt=$(awk -F'\t' -v s="$sec" '$1 == "HEAD" && $4 == 2 && $5 == s { n++ } END { print n + 0 }' "$SCAN")
    if [ "$cnt" -ne 1 ]; then
      printf '%s\t0\t외부가 지목하는 절 이름 `§%s` 이 `## ` heading 으로 %s곳 — 정확히 1곳이어야 한다\n' \
        "$ROOT" "$sec" "$cnt" >> "$TMPDIR_CDG/out11"
    fi
  done

  # `spec 확정 게이트` — 정지 조건 표의 조건 셀로 정확히 1회, heading 으로는 0회.
  rowcnt=$(table_column "$SKILL_FILE" "$EXTERNAL_ROW_TABLE" "$EXTERNAL_ROW_COLUMN" \
    | awk -F'\t' -v s="$EXTERNAL_ROW" '$2 == s { n++ } END { print n + 0 }')
  if [ "$rowcnt" -ne 1 ]; then
    printf '%s\t0\t외부가 지목하는 행 이름 `%s` 이 SKILL.md `## %s` 표의 %s 셀로 %s회 — 정확히 1회여야 한다\n' \
      "$SKILL_FILE" "$EXTERNAL_ROW" "$EXTERNAL_ROW_TABLE" "$EXTERNAL_ROW_COLUMN" "$rowcnt" >> "$TMPDIR_CDG/out11"
  fi
  headcnt=$(awk -F'\t' -v s="$EXTERNAL_ROW" '$1 == "HEAD" && $5 == s { n++ } END { print n + 0 }' "$SCAN")
  if [ "$headcnt" -ne 0 ]; then
    printf '%s\t0\t`%s` 이 heading 으로도 %s곳에 있다 — 정지 조건 표의 행으로만 둔다\n' \
      "$ROOT" "$EXTERNAL_ROW" "$headcnt" >> "$TMPDIR_CDG/out11"
  fi

  printf '%s\n' "$EXTERNAL_FILES" | while IFS= read -r fn; do
    [ -n "$fn" ] || continue
    if [ ! -f "$ROOT/$fn" ]; then
      printf '%s\t0\t외부가 canonical 로 지목하는 파일 `%s` 이 없다\n' "$ROOT" "$fn" >> "$TMPDIR_CDG/out11"
    fi
  done

  while IFS="$(printf '\t')" read -r f l msg; do
    [ -n "${f:-}" ] || continue
    fail 11 "외부 계약 표면 실존" "$f:$l" "$msg"
  done < "$TMPDIR_CDG/out11"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out11")
  end_check 11 "외부 계약 표면 실존"
}

# ---------------------------------------------------------------------------
# (12) 불변식 행 형식 — 각 행이 "금지" 와 "대신" 을 모두 갖는가 (설계 §4 · 규약 §2)
# ---------------------------------------------------------------------------
check_12_invariant_rows() {
  begin_check
  table_missing "$SKILL_FILE" "불변식" "금지" "대신" > "$TMPDIR_CDG/out12"
  while IFS="$(printf '\t')" read -r l col why; do
    [ -n "${l:-}" ] || continue
    case "$why" in
      __NOCOL__)  msg="불변식 표에 '$col' 열이 없다" ;;
      __NOTABLE__) msg="SKILL.md 에 '## 불변식' 표가 없다" ;;
      *)          msg="'$col' 셀이 비었다 — 한 행 = 금지 → 대신. 출구 없는 금지는 두지 않는다" ;;
    esac
    fail 12 "불변식 행 형식" "$SKILL_FILE:$l" "$msg"
  done < "$TMPDIR_CDG/out12"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out12")
  end_check 12 "불변식 행 형식"
}

# ---------------------------------------------------------------------------
# (13) 정지 조건 3열 채움 — 신호 · 행동 · 출구 (설계 §5 · 규약 §3)
# ---------------------------------------------------------------------------
check_13_stop_rows() {
  begin_check
  table_missing "$SKILL_FILE" "정지 조건" "신호" "행동" "출구" > "$TMPDIR_CDG/out13"
  while IFS="$(printf '\t')" read -r l col why; do
    [ -n "${l:-}" ] || continue
    case "$why" in
      __NOCOL__)  msg="정지 조건 표에 '$col' 열이 없다" ;;
      __NOTABLE__) msg="SKILL.md 에 '## 정지 조건' 표가 없다" ;;
      *)          msg="'$col' 셀이 비었다 — 셋 중 하나를 채울 수 없으면 정지 조건이 아니다" ;;
    esac
    fail 13 "정지 조건 3열 채움" "$SKILL_FILE:$l" "$msg"
  done < "$TMPDIR_CDG/out13"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out13")
  end_check 13 "정지 조건 3열 채움"
}

# ---------------------------------------------------------------------------
# 실행
# ---------------------------------------------------------------------------
check_2_struct_whitelist
check_5_limits
check_6_forbidden_headings
check_7_no_mermaid
check_9_budget_leak
check_11_external_surface
check_12_invariant_rows
check_13_stop_rows

printf 'checks: %d, failed: %d\n' "$TOTAL_CHECKS" "$FAILED_CHECKS"
[ "$FAILED_CHECKS" -gt 0 ] && exit 1
exit 0

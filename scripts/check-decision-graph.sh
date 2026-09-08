#!/usr/bin/env bash
# check-decision-graph.sh — orchestrator 결정 그래프 문서의 상시 검사 10종
#
# 설계: plans/atelier/09-orchestrator-graph-restructure.md
#   §3 그래프 규약 / §3.4 크기 상한 / §3.6 구조 heading 화이트리스트(W1~W6)
#   §4.2 결정 라우팅 표 / §9.2 (A) 상시 검사
#   부록 09d §1·§6 외부 계약 표면 → 검사 (11)
#
# 입력은 인자로 받은 스킬 디렉토리 안의 *.md 뿐이다 (설계 §9.2 R-2:
# plan·별도 데이터 파일을 파서 입력으로 삼지 않는다 — 순수 함수).
#
# 사용법: check-decision-graph.sh [--verbose] [스킬 디렉토리]
# 종료 코드: 실패 검사 수 > 0 이면 1, 아니면 0
#
# 의존: bash / grep / sed / awk 만. GNU 전용 옵션 미사용 (macOS·Linux 공통).

set -u

# ---------------------------------------------------------------------------
# 설정 — 설계와 함께 고쳐야 하는 값들
# ---------------------------------------------------------------------------

# 구조 heading 화이트리스트 W1~W6 (설계 §3.6 R-1).
# 열거 기반이며 개수 상한이 아니다. heading 텍스트가 아래 항목 중 하나로
# **시작**하면 통과한다 (W6 처럼 뒤에 수식어가 붙는 것을 허용).
#
# !! 여기에 항목을 추가하려면 설계 §8.1-3 절차를 밟는다 —
#    이 목록과 설계 §3.6 표를 함께 고치고, 왜 id 블록(`## D##.`/`## C##.`/`## P##.`)
#    으로 표현할 수 없는지를 근거로 남긴다. 이 마찰이 화이트리스트가
#    산문 재증식의 배출구가 되는 것을 막는다.
STRUCT_HEADING_WHITELIST='## 필수 로드
## 디스패치 전 게이트 인덱스
## 결정 라우팅
## 계약
## 절차
## 머지 대상'

# 크기 상한 (설계 §3.4 · §9.2 검사 (5)). 값은 "넘으면 실패"하는 최대 허용치.
LIMIT_MERMAID_LINES=15      # mermaid 블록 내용 줄수 (펜스 제외). 16줄+ 실패
LIMIT_QUESTION_NODES=6      # 판정 질문 노드 D##_qN. 7+ 실패
LIMIT_TERMINALS=6           # 종단 D##_tN + D##_nN. 7+ 실패
LIMIT_EDGES=12              # 엣지 (-->). 13+ 실패
LIMIT_DECISION_BLOCK=35     # 결정 블록 전체 줄수. 36+ 실패
LIMIT_CONTRACT_BLOCK=40     # 계약·절차 블록 전체 줄수 (R-5). 41+ 실패
LIMIT_FILE_LINES=260        # 파일 하나. 261+ 실패

# C04 가 유일하게 소유하는 예산·가드레일 이름 (설계 §3.3 예외 1 "값은 계약에서만,
# 이름은 어디서나" / §2.2 C04 / 현행 autonomous-driving.md 자율 계약·가드레일 표).
# 결정 블록 안에서 이 이름 옆에 숫자 리터럴이 나오면 검사 (9) 실패.
BUDGET_NAMES='max_loops
max_redispatch_per_task
max_advisory_consults
max_council_rounds
탐색 예산
재시도 예산
자문 예산
충돌 카운터
no-progress'

# 검사 (9) 에서 "옆"으로 보는 창 크기 (이름 끝에서 이후 N 바이트).
BUDGET_WINDOW=40

# 외부 계약 표면 (설계 부록 09d §1-A·§6 소프트 6·7). 사라지면 CI 는 통과하나
# 외부 참조가 조용히 죽는다.
EXTERNAL_SECTIONS='spec 확정 게이트
대화 스킬의 자율 어댑테이션
설계 승인 마커
머지 대상'
EXTERNAL_FILES='merge-coordinator.md
architect-council.md
autonomous-driving.md'

TOTAL_CHECKS=10

# ---------------------------------------------------------------------------
# 인자
# ---------------------------------------------------------------------------

VERBOSE=0
ROOT=""
for arg in "$@"; do
  case "$arg" in
    --verbose|-v) VERBOSE=1 ;;
    -h|--help)
      sed -n '2,20p' "$0"
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
#   HEAD    file line level text          # 코드펜스 밖 heading (frontmatter 제외)
#   BLOCK   file id kind start end lines mermaids
#                                         # `## ` 로 열리는 블록. kind = D|C|P|STRUCT
#   MERMAID file start lines q t edges    # mermaid 코드펜스 1개
#                                         # q/t 는 줄수가 아니라 출현 횟수(한 줄
#                                         # 인라인 다중 선언 허용, 설계 §3.4)
#   FILELEN file lines
# ---------------------------------------------------------------------------

SCAN="$TMPDIR_CDG/scan"
: > "$SCAN"
while IFS= read -r f; do
  awk -v FILE="$f" '
    function flush_block(   kind) {
      if (blk_start == 0) return
      kind = "STRUCT"
      if (blk_id != "-") kind = substr(blk_id, 1, 1)
      printf "BLOCK\t%s\t%s\t%s\t%d\t%d\n", FILE, blk_id, kind, blk_start, blk_mermaid
      blk_start = 0
    }
    BEGIN { infm = 0; infence = 0; blk_start = 0; blk_id = "-" }
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
          infence = 1; fencelen = n; fstart = NR; flines = 0
          lang = substr(line, n + 1); gsub(/[ \t]/, "", lang)
          fq = 0; ft = 0; fe = 0
        } else {
          rest = substr(line, n + 1)
          if (n >= fencelen && rest ~ /^[ \t]*$/) {
            infence = 0
            if (lang == "mermaid") {
              printf "MERMAID\t%s\t%d\t%d\t%d\t%d\t%d\n", FILE, fstart, flines, fq, ft, fe
              if (blk_start > 0) blk_mermaid++
            }
          } else {
            flines++
          }
        }
        next
      }
      if (infence) {
        flines++
        if (lang == "mermaid") {
          # 판정 노드·종단은 줄당 1회가 아니라 출현 횟수로 센다 — 인라인
          # 스타일(예: `D30_q1{"…"} -->|Yes| D30_q2{"…"}`)은 규약상 허용되며
          # 한 줄에 선언이 둘 이상일 수 있다. 선언(뒤에 `{`/`[`/`(`)만 세므로
          # 같은 id 가 엣지에서 재사용되는 줄은 자연히 제외된다.
          s = line
          while (match(s, /[DCP][0-9][0-9]_q[0-9]+[ \t]*\{/)) { fq++; s = substr(s, RSTART + RLENGTH) }
          s = line
          while (match(s, /[DCP][0-9][0-9]_t[0-9]+[ \t]*\[/)) { ft++; s = substr(s, RSTART + RLENGTH) }
          s = line
          while (match(s, /[DCP][0-9][0-9]_n[0-9]+[ \t]*\(/)) { ft++; s = substr(s, RSTART + RLENGTH) }
          if (index(line, "-->") > 0) fe++
        }
        next
      }

      # heading
      if (line ~ /^#+[ \t]/) {
        lvl = 0
        while (substr(line, lvl + 1, 1) == "#") lvl++
        text = substr(line, lvl + 1)
        sub(/^[ \t]+/, "", text)
        sub(/[ \t]+$/, "", text)
        printf "HEAD\t%s\t%d\t%d\t%s\n", FILE, NR, lvl, text
        if (lvl == 2) {
          flush_block()
          blk_start = NR
          blk_mermaid = 0
          if (text ~ /^[DCP][0-9][0-9]\./) {
            blk_id = substr(text, 1, 3)
          } else {
            blk_id = "-"
          }
        }
      }
    }
    END {
      flush_block()
      printf "FILELEN\t%s\t%d\n", FILE, NR
    }
  ' "$f" >> "$SCAN"
done < "$FILES"

# awk 안에서 이전 블록의 마지막 non-blank 행을 추적하기 어려워, 블록 끝 행은
# 아래에서 다시 계산한다 (블록 시작 목록 + 파일 길이로 결정).
BLOCKS="$TMPDIR_CDG/blocks"
awk -F'\t' '
  $1 == "BLOCK"   { n++; bf[n] = $2; bid[n] = $3; bkind[n] = $4; bstart[n] = $5; bmer[n] = $6 }
  $1 == "FILELEN" { flen[$2] = $3 }
  END {
    for (i = 1; i <= n; i++) {
      end = (i < n && bf[i + 1] == bf[i]) ? bstart[i + 1] - 1 : flen[bf[i]]
      print bf[i] "\t" bid[i] "\t" bkind[i] "\t" bstart[i] "\t" end "\t" bmer[i]
    }
  }
' "$SCAN" > "$BLOCKS"
# BLOCKS 열: file, id, kind, start, end(다음 ## 직전 / EOF), mermaid_count

# 블록의 "실질 줄수" = 시작행 ~ 블록 안 마지막 non-blank 행.
BLOCKLEN="$TMPDIR_CDG/blocklen"
: > "$BLOCKLEN"
while IFS="$(printf '\t')" read -r bf bid bkind bstart bend bmer; do
  real_end=$(awk -v s="$bstart" -v e="$bend" 'NR>=s && NR<=e && /[^ \t]/ { last=NR } END { print (last?last:s) }' "$bf")
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$bf" "$bid" "$bkind" "$bstart" "$real_end" "$((real_end - bstart + 1))" "$bmer" >> "$BLOCKLEN"
done < "$BLOCKS"
# BLOCKLEN 열: file, id, kind, start, real_end, len, mermaid_count

# heading id 집합
HEADIDS="$TMPDIR_CDG/headids"
awk -F'\t' '$3 == "D" || $3 == "C" || $3 == "P" { print $2 }' "$BLOCKLEN" | LC_ALL=C sort > "$HEADIDS"
HEADIDS_UNIQ="$TMPDIR_CDG/headids_uniq"
LC_ALL=C sort -u "$HEADIDS" > "$HEADIDS_UNIQ"

has_id() { LC_ALL=C grep -qx -- "$1" "$HEADIDS_UNIQ"; }

# ---------------------------------------------------------------------------
# (1) heading 유일성
# ---------------------------------------------------------------------------
check_1_heading_unique() {
  begin_check
  LC_ALL=C uniq -c "$HEADIDS" | awk '$1 != 1 { print $2 "\t" $1 }' | while IFS="$(printf '\t')" read -r id cnt; do
    locs=$(awk -F'\t' -v id="$id" '$2 == id { printf "%s:%s ", $1, $4 }' "$BLOCKLEN")
    fail 1 "heading 유일성" "${locs% }" "id $id 가 ${cnt}회 선언됐다 (소유자는 정확히 1곳)"
  done > "$TMPDIR_CDG/out1"
  cat "$TMPDIR_CDG/out1"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out1")
  end_check 1 "heading 유일성"
}

# ---------------------------------------------------------------------------
# (2) 구조 heading 화이트리스트 (설계 §3.6 W1~W6)
# ---------------------------------------------------------------------------
check_2_struct_whitelist() {
  begin_check
  printf '%s\n' "$STRUCT_HEADING_WHITELIST" | sed 's/^## //' > "$TMPDIR_CDG/wl"
  awk -F'\t' -v wlfile="$TMPDIR_CDG/wl" '
    BEGIN { while ((getline l < wlfile) > 0) if (l != "") wl[++nw] = l }
    $1 == "HEAD" && $4 == 2 {
      text = $5
      if (text ~ /^[DCP][0-9][0-9]\./) next
      for (i = 1; i <= nw; i++)
        if (substr(text, 1, length(wl[i])) == wl[i]) next
      print $2 "\t" $3 "\t" text
    }
  ' "$SCAN" > "$TMPDIR_CDG/out2"
  while IFS="$(printf '\t')" read -r f l t; do
    [ -n "${f:-}" ] || continue
    fail 2 "구조 heading 화이트리스트" "$f:$l" "화이트리스트(W1~W6) 밖의 id 없는 heading '## $t'"
  done < "$TMPDIR_CDG/out2"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out2")
  end_check 2 "구조 heading 화이트리스트"
}

# ---------------------------------------------------------------------------
# (3a) needs: id 실존
# ---------------------------------------------------------------------------
check_3a_needs_exist() {
  begin_check
  : > "$TMPDIR_CDG/out3a"
  while IFS= read -r f; do
    awk -v FILE="$f" '
      /<!--[ \t]*needs:/ {
        s = $0
        sub(/^.*<!--[ \t]*needs:/, "", s)
        sub(/-->.*$/, "", s)
        n = split(s, toks, /[^A-Za-z0-9]+/)
        for (i = 1; i <= n; i++)
          if (toks[i] ~ /^[DCP][0-9][0-9]$/) print FILE "\t" NR "\t" toks[i]
      }
    ' "$f"
  done < "$FILES" > "$TMPDIR_CDG/needs"
  while IFS="$(printf '\t')" read -r f l id; do
    [ -n "${id:-}" ] || continue
    has_id "$id" || printf '%s\t%s\t%s\n' "$f" "$l" "$id" >> "$TMPDIR_CDG/out3a"
  done < "$TMPDIR_CDG/needs"
  while IFS="$(printf '\t')" read -r f l id; do
    [ -n "${id:-}" ] || continue
    fail 3a "needs: id 실존" "$f:$l" "needs: 가 존재하지 않는 항목 $id 를 가리킨다"
  done < "$TMPDIR_CDG/out3a"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out3a")
  end_check 3a "needs: id 실존"
}

# ---------------------------------------------------------------------------
# (4) 참조 실존 — `→ D##` / `→ C##` / `→ P##`
#     mermaid 종단(`D##_nN(["→ D31"])`)도 참조이므로 코드펜스를 제외하지 않는다.
# ---------------------------------------------------------------------------
check_4_refs_exist() {
  begin_check
  : > "$TMPDIR_CDG/refs"
  while IFS= read -r f; do
    awk -v FILE="$f" '
      {
        s = $0
        while (match(s, /→[ \t]*[DCP][0-9][0-9]/)) {
          tok = substr(s, RSTART, RLENGTH)
          sub(/^→[ \t]*/, "", tok)
          print FILE "\t" NR "\t" tok
          s = substr(s, RSTART + RLENGTH)
        }
      }
    ' "$f"
  done < "$FILES" > "$TMPDIR_CDG/refs"
  : > "$TMPDIR_CDG/out4"
  LC_ALL=C sort -u "$TMPDIR_CDG/refs" | while IFS="$(printf '\t')" read -r f l id; do
    [ -n "${id:-}" ] || continue
    has_id "$id" || printf '%s\t%s\t%s\n' "$f" "$l" "$id"
  done > "$TMPDIR_CDG/out4"
  while IFS="$(printf '\t')" read -r f l id; do
    [ -n "${id:-}" ] || continue
    fail 4 "참조 실존" "$f:$l" "→ $id 가 존재하지 않는 항목을 가리킨다"
  done < "$TMPDIR_CDG/out4"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out4")
  end_check 4 "참조 실존"
}

# ---------------------------------------------------------------------------
# (5) 상한 (설계 §3.4)
# ---------------------------------------------------------------------------
check_5_limits() {
  begin_check
  {
    awk -F'\t' -v ml="$LIMIT_MERMAID_LINES" -v mq="$LIMIT_QUESTION_NODES" \
               -v mt="$LIMIT_TERMINALS" -v me="$LIMIT_EDGES" '
      $1 == "MERMAID" {
        if ($4 > ml) print $2 "\t" $3 "\tmermaid 블록 " $4 "줄 (상한 " ml ")"
        if ($5 > mq) print $2 "\t" $3 "\t판정 질문 노드 " $5 "개 (상한 " mq ")"
        if ($6 > mt) print $2 "\t" $3 "\t종단 " $6 "개 (상한 " mt ")"
        if ($7 > me) print $2 "\t" $3 "\t엣지 " $7 "개 (상한 " me ")"
      }
    ' "$SCAN"
    awk -F'\t' -v dl="$LIMIT_DECISION_BLOCK" -v cl="$LIMIT_CONTRACT_BLOCK" '
      $3 == "D" && $6 > dl { print $1 "\t" $4 "\t결정 블록 " $2 " 가 " $6 "줄 (상한 " dl ")" }
      ($3 == "C" || $3 == "P") && $6 > cl { print $1 "\t" $4 "\t계약·절차 블록 " $2 " 가 " $6 "줄 (상한 " cl ")" }
    ' "$BLOCKLEN"
    awk -F'\t' -v fl="$LIMIT_FILE_LINES" '
      $1 == "FILELEN" && $3 > fl { print $2 "\t1\t파일 " $3 "줄 (상한 " fl ")" }
    ' "$SCAN"
  } | LC_ALL=C sort -t"$(printf '\t')" -k1,1 -k2,2n > "$TMPDIR_CDG/out5"
  while IFS="$(printf '\t')" read -r f l msg; do
    [ -n "${f:-}" ] || continue
    fail 5 "상한" "$f:$l" "$msg"
  done < "$TMPDIR_CDG/out5"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out5")
  end_check 5 "상한"
}

# ---------------------------------------------------------------------------
# (6) 금지 heading — 안티패턴 / 체크리스트 (설계 §6.1·§6.2, F2-2)
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
# (7) 그래프 개수 — D 블록 정확히 1개, C·P 블록 0개 (예외: P02 순서 그래프 1개)
# ---------------------------------------------------------------------------
check_7_graph_count() {
  begin_check
  awk -F'\t' '
    $3 == "D" { if ($7 != 1) print $1 "\t" $4 "\t결정 블록 " $2 " 의 mermaid 펜스가 " $7 "개 (정확히 1개여야 한다)"; next }
    # 설계 §3.6 예외: P02 표준 절차만 순서 그래프 1개를 갖는다
    $2 == "P02" { if ($7 != 1) print $1 "\t" $4 "\tP02 는 순서 그래프 1개를 가져야 한다 (현재 " $7 "개)"; next }
    ($3 == "C" || $3 == "P") { if ($7 != 0) print $1 "\t" $4 "\t" $3 " 블록 " $2 " 에 mermaid 펜스 " $7 "개 (계약·절차는 그래프를 갖지 않는다)" }
  ' "$BLOCKLEN" > "$TMPDIR_CDG/out7"
  while IFS="$(printf '\t')" read -r f l msg; do
    [ -n "${f:-}" ] || continue
    fail 7 "그래프 개수" "$f:$l" "$msg"
  done < "$TMPDIR_CDG/out7"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out7")
  end_check 7 "그래프 개수"
}

# ---------------------------------------------------------------------------
# (9) 예산 값 유출 — 결정 블록 안, C04 소유 예산 이름 옆의 숫자 리터럴
#     (설계 §3.3 예외 1 "값은 계약에서만, 이름은 어디서나")
# ---------------------------------------------------------------------------
check_9_budget_leak() {
  begin_check
  printf '%s\n' "$BUDGET_NAMES" > "$TMPDIR_CDG/budgets"
  : > "$TMPDIR_CDG/out9"
  awk -F'\t' '$3 == "D" { print $1 "\t" $4 "\t" $5 "\t" $2 }' "$BLOCKLEN" > "$TMPDIR_CDG/dblocks"
  while IFS="$(printf '\t')" read -r f s e id; do
    [ -n "${f:-}" ] || continue
    awk -v FILE="$f" -v S="$s" -v E="$e" -v ID="$id" -v W="$BUDGET_WINDOW" \
        -v bfile="$TMPDIR_CDG/budgets" '
      BEGIN { while ((getline l < bfile) > 0) if (l != "") b[++nb] = l }
      NR < S || NR > E { next }
      {
        for (i = 1; i <= nb; i++) {
          p = index($0, b[i])
          if (p == 0) continue
          tail = substr($0, p + length(b[i]), W)
          # 항목 id (D14 / C04 / #c04-...) 와 노드 첨자(_q1,_t2,_n3)는 숫자가 아니다
          gsub(/[DCPdcp][0-9][0-9]/, "", tail)
          gsub(/_[qtn][0-9]+/, "", tail)
          if (tail ~ /[0-9]/) {
            v = tail; sub(/^[^0-9]*/, "", v); sub(/[^0-9].*$/, "", v)
            print FILE "\t" NR "\t" ID "\t" b[i] "\t" v
            break
          }
        }
      }
    ' "$f" >> "$TMPDIR_CDG/out9"
  done < "$TMPDIR_CDG/dblocks"
  while IFS="$(printf '\t')" read -r f l id name val; do
    [ -n "${f:-}" ] || continue
    fail 9 "예산 값 유출" "$f:$l" "결정 블록 $id 안에서 예산 이름 '$name' 옆에 숫자 리터럴 '$val' — 값은 C04 에만 둔다 (→ C04)"
  done < "$TMPDIR_CDG/out9"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out9")
  end_check 9 "예산 값 유출"
}

# ---------------------------------------------------------------------------
# (10) owns ↔ heading ↔ 라우팅 표 3자 정합 (설계 §9.2 (10), §4.2)
# ---------------------------------------------------------------------------
check_10_three_way() {
  begin_check
  # owns 집합
  : > "$TMPDIR_CDG/owns"
  while IFS= read -r f; do
    awk -v FILE="$f" '
      /<!--[ \t]*owns:/ {
        s = $0
        sub(/^.*<!--[ \t]*owns:/, "", s)
        sub(/-->.*$/, "", s)
        n = split(s, toks, /[^A-Za-z0-9]+/)
        for (i = 1; i <= n; i++)
          if (toks[i] ~ /^[DCP][0-9][0-9]$/) print toks[i] "\t" FILE "\t" NR
      }
    ' "$f"
  done < "$FILES" > "$TMPDIR_CDG/owns"
  awk -F'\t' '{ print $1 }' "$TMPDIR_CDG/owns" | LC_ALL=C sort -u > "$TMPDIR_CDG/owns_ids"

  # 라우팅 표 집합 — SKILL.md 의 `## 결정 라우팅` 절 안 표 셀 토큰
  ROUTER=""
  while IFS= read -r f; do
    case "$f" in */SKILL.md|SKILL.md) ROUTER="$f" ;;
    esac
  done < "$FILES"
  : > "$TMPDIR_CDG/route_ids"
  ROUTE_SRC="(없음)"
  if [ -n "$ROUTER" ]; then
    ROUTE_SRC="$ROUTER"
    awk '
      /^##[ \t]/ { inrt = (index($0, "결정 라우팅") > 0) ? 1 : 0; next }
      inrt && /^[ \t]*\|/ {
        s = $0
        n = split(s, toks, /[^A-Za-z0-9]+/)
        for (i = 1; i <= n; i++)
          if (toks[i] ~ /^[DCP][0-9][0-9]$/) print toks[i]
      }
    ' "$ROUTER" | LC_ALL=C sort -u > "$TMPDIR_CDG/route_ids"
  fi

  : > "$TMPDIR_CDG/out10"
  if [ -z "$ROUTER" ]; then
    printf '%s\t%s\t%s\n' "$ROOT" "0" "SKILL.md 가 없어 라우팅 표를 읽을 수 없다" >> "$TMPDIR_CDG/out10"
  elif [ ! -s "$TMPDIR_CDG/route_ids" ]; then
    printf '%s\t%s\t%s\n' "$ROUTER" "0" "'## 결정 라우팅' 표에서 항목 id 를 하나도 찾지 못했다 (설계 §4.2)" >> "$TMPDIR_CDG/out10"
  fi
  awk -v ownsf="$TMPDIR_CDG/owns_ids" -v headf="$HEADIDS_UNIQ" -v routef="$TMPDIR_CDG/route_ids" \
      -v ownsloc="$TMPDIR_CDG/owns" -v blocks="$BLOCKLEN" -v routesrc="$ROUTE_SRC" '
    BEGIN {
      while ((getline l < ownsf) > 0) if (l != "") { o[l] = 1; all[l] = 1 }
      while ((getline l < headf) > 0) if (l != "") { h[l] = 1; all[l] = 1 }
      while ((getline l < routef) > 0) if (l != "") { r[l] = 1; all[l] = 1 }
      while ((getline l < ownsloc) > 0) { split(l, a, "\t"); ownwhere[a[1]] = a[2] ":" a[3] }
      while ((getline l < blocks) > 0) { split(l, a, "\t"); headwhere[a[2]] = a[1] ":" a[4] }
      n = 0
      for (k in all) keys[++n] = k
      for (i = 1; i < n; i++) for (j = i + 1; j <= n; j++) if (keys[i] > keys[j]) { t = keys[i]; keys[i] = keys[j]; keys[j] = t }
      for (i = 1; i <= n; i++) {
        k = keys[i]
        if (o[k] && h[k] && r[k]) continue
        miss = ""
        if (!o[k]) miss = miss "owns "
        if (!h[k]) miss = miss "heading "
        if (!r[k]) miss = miss "라우팅표 "
        sub(/ $/, "", miss)
        where = (k in ownwhere) ? ownwhere[k] : ((k in headwhere) ? headwhere[k] : routesrc ":0")
        split(where, w, ":")
        print w[1] "\t" w[2] "\t" k " 가 " miss " 집합에 없다 (owns ∪ = heading = 라우팅 표 여야 한다)"
      }
    }
  ' >> "$TMPDIR_CDG/out10"
  while IFS="$(printf '\t')" read -r f l msg; do
    [ -n "${f:-}" ] || continue
    fail 10 "3자 정합" "$f:$l" "$msg"
  done < "$TMPDIR_CDG/out10"
  CUR_FAILS=$(awk 'END { print NR }' "$TMPDIR_CDG/out10")
  end_check 10 "3자 정합"
}

# ---------------------------------------------------------------------------
# (11) 외부 계약 표면 실존 (부록 09d §1-A·§6)
#      절 이름 4 — 어떤 heading 텍스트에든 부분 문자열로 존재 (번호·id 접두 허용)
#      파일명 3 — 스킬 디렉토리 안에 실존
# ---------------------------------------------------------------------------
check_11_external_surface() {
  begin_check
  : > "$TMPDIR_CDG/out11"
  printf '%s\n' "$EXTERNAL_SECTIONS" | while IFS= read -r sec; do
    [ -n "$sec" ] || continue
    if ! awk -F'\t' -v s="$sec" '$1 == "HEAD" && index($5, s) > 0 { found = 1; exit } END { exit !found }' "$SCAN"; then
      printf '%s\t%s\t%s\n' "$ROOT" "0" "외부가 참조하는 절 이름 '§$sec' 을 어떤 heading 에서도 찾을 수 없다" >> "$TMPDIR_CDG/out11"
    fi
  done
  printf '%s\n' "$EXTERNAL_FILES" | while IFS= read -r fn; do
    [ -n "$fn" ] || continue
    if ! LC_ALL=C grep -q "/$fn\$" "$FILES"; then
      printf '%s\t%s\t%s\n' "$ROOT" "0" "외부가 canonical 로 지목하는 파일 '$fn' 이 없다" >> "$TMPDIR_CDG/out11"
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
# 실행
# ---------------------------------------------------------------------------
check_1_heading_unique
check_2_struct_whitelist
check_3a_needs_exist
check_4_refs_exist
check_5_limits
check_6_forbidden_headings
check_7_graph_count
check_9_budget_leak
check_10_three_way
check_11_external_surface

printf 'checks: %d, failed: %d\n' "$TOTAL_CHECKS" "$FAILED_CHECKS"
[ "$FAILED_CHECKS" -gt 0 ] && exit 1
exit 0

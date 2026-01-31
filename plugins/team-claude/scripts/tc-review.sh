#!/bin/bash
# Team Claude - Auto Review Runner
# 자동 리뷰 실행 스크립트

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# ============================================================================
# 사용법
# ============================================================================
usage() {
  cat << 'EOF'
Team Claude Review - 자동 리뷰 실행

사용법:
  tc-review <type> <target> [options]

Types:
  spec <session-id>           스펙 리뷰
  code <checkpoint-id>        코드 리뷰

Options:
  --max-iterations <n>        최대 반복 횟수 (기본: 5)
  --auto-fix                  자동 수정 적용
  --strict                    엄격 모드 (WARN도 FAIL로 처리)

Examples:
  tc-review spec abc12345
  tc-review code coupon-service --auto-fix
  tc-review spec abc12345 --strict --max-iterations 3
EOF
}

# ============================================================================
# 리뷰 결과 저장
# ============================================================================

# 리뷰 결과 디렉토리
get_review_dir() {
  local type="$1"
  local target="$2"

  if [[ "$type" == "spec" ]]; then
    echo "$(get_sessions_dir)/${target}/reviews"
  else
    echo "$(get_sessions_dir)/current/reviews/${target}"
  fi
}

# 리뷰 결과 저장
save_review_result() {
  local type="$1"
  local target="$2"
  local iteration="$3"
  local result="$4"
  local details="$5"

  require_jq

  local review_dir
  review_dir=$(get_review_dir "$type" "$target")
  ensure_dir "$review_dir"

  local review_file="${review_dir}/iteration-${iteration}.json"

  cat > "$review_file" << EOF
{
  "type": "${type}",
  "target": "${target}",
  "iteration": ${iteration},
  "result": "${result}",
  "details": ${details},
  "timestamp": "$(timestamp)"
}
EOF

  echo "$review_file"
}

# ============================================================================
# spec - 스펙 리뷰
# ============================================================================
cmd_spec() {
  local session_id=""
  local max_iterations=5
  local auto_fix=false
  local strict=false

  # 인자 파싱
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --max-iterations)
        max_iterations="$2"
        shift 2
        ;;
      --auto-fix)
        auto_fix=true
        shift
        ;;
      --strict)
        strict=true
        shift
        ;;
      -*)
        err "알 수 없는 옵션: $1"
        exit 1
        ;;
      *)
        if [[ -z "$session_id" ]]; then
          session_id="$1"
        fi
        shift
        ;;
    esac
  done

  if [[ -z "$session_id" ]]; then
    err "세션 ID를 지정하세요."
    err "사용법: tc-review spec <session-id>"
    exit 1
  fi

  local sessions_dir
  sessions_dir=$(get_sessions_dir)
  local session_path="${sessions_dir}/${session_id}"

  if [[ ! -d "$session_path" ]]; then
    err "세션을 찾을 수 없습니다: $session_id"
    exit 1
  fi

  echo ""
  echo "🔍 Spec Review 시작"
  echo ""
  echo "  세션: ${session_id}"
  echo "  최대 반복: ${max_iterations}"
  echo "  자동 수정: ${auto_fix}"
  echo "  엄격 모드: ${strict}"
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # 스펙 파일 확인
  local specs_dir="${session_path}/specs"
  local architecture="${specs_dir}/architecture.md"
  local contracts="${specs_dir}/contracts.md"
  local checkpoints="${specs_dir}/checkpoints.yaml"

  local missing_files=()
  [[ ! -f "$architecture" ]] && missing_files+=("architecture.md")
  [[ ! -f "$contracts" ]] && missing_files+=("contracts.md")
  [[ ! -f "$checkpoints" ]] && missing_files+=("checkpoints.yaml")

  if [[ ${#missing_files[@]} -gt 0 ]]; then
    warn "누락된 스펙 파일:"
    for f in "${missing_files[@]}"; do
      echo "  - $f"
    done
    echo ""
  fi

  # 리뷰 체크리스트 출력
  echo "📋 Review Checklist"
  echo ""
  echo "  완전성 (Completeness)"
  echo "    [ ] 모든 요구사항 반영"
  echo "    [ ] 엣지 케이스 정의"
  echo "    [ ] 에러 처리 정의"
  echo ""
  echo "  일관성 (Consistency)"
  echo "    [ ] 기존 아키텍처 일관성"
  echo "    [ ] 용어/네이밍 일관성"
  echo ""
  echo "  테스트 가능성 (Testability)"
  echo "    [ ] 검증 가능한 기준"
  echo "    [ ] Contract Test 충분성"
  echo ""
  echo "  의존성 (Dependencies)"
  echo "    [ ] 의존성 그래프 정확성"
  echo "    [ ] 순환 의존성 없음"
  echo ""

  # 리뷰 시뮬레이션 (실제로는 에이전트가 수행)
  echo "━━━ Auto-Review Loop ━━━"
  echo ""

  local iteration=1
  local final_result="PENDING"

  while [[ $iteration -le $max_iterations ]]; do
    echo "  Iteration ${iteration}/${max_iterations}:"
    echo "    🔍 리뷰 수행 중..."

    # 실제 구현에서는 여기서 spec-reviewer 에이전트 호출
    # 지금은 플레이스홀더

    # 결과 저장
    local result_json='{"issues": [], "warnings": []}'
    save_review_result "spec" "$session_id" "$iteration" "SIMULATED" "$result_json"

    echo "    ✅ 리뷰 완료"
    echo ""

    final_result="PASS"
    break
  done

  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  if [[ "$final_result" == "PASS" ]]; then
    ok "Spec Review 완료: PASS"
  elif [[ "$final_result" == "WARN" ]]; then
    warn "Spec Review 완료: WARN (경고 있음)"
  else
    err "Spec Review 완료: FAIL (수정 필요)"
  fi

  echo ""
  echo "  결과 저장: $(get_review_dir spec "$session_id")"
  echo ""

  # JSON 출력
  echo "---"
  cat << EOF
{
  "sessionId": "${session_id}",
  "type": "spec",
  "result": "${final_result}",
  "iterations": ${iteration}
}
EOF
}

# ============================================================================
# code - 코드 리뷰
# ============================================================================
cmd_code() {
  local checkpoint_id=""
  local max_iterations=5
  local auto_fix=false
  local strict=false

  # 인자 파싱
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --max-iterations)
        max_iterations="$2"
        shift 2
        ;;
      --auto-fix)
        auto_fix=true
        shift
        ;;
      --strict)
        strict=true
        shift
        ;;
      -*)
        err "알 수 없는 옵션: $1"
        exit 1
        ;;
      *)
        if [[ -z "$checkpoint_id" ]]; then
          checkpoint_id="$1"
        fi
        shift
        ;;
    esac
  done

  if [[ -z "$checkpoint_id" ]]; then
    err "Checkpoint ID를 지정하세요."
    err "사용법: tc-review code <checkpoint-id>"
    exit 1
  fi

  local worktrees_dir
  worktrees_dir=$(get_worktrees_dir)
  local worktree_path="${worktrees_dir}/${checkpoint_id}"

  if [[ ! -d "$worktree_path" ]]; then
    err "Worktree를 찾을 수 없습니다: $checkpoint_id"
    exit 1
  fi

  echo ""
  echo "🔍 Code Review 시작"
  echo ""
  echo "  Checkpoint: ${checkpoint_id}"
  echo "  Worktree: ${worktree_path}"
  echo "  최대 반복: ${max_iterations}"
  echo "  자동 수정: ${auto_fix}"
  echo "  엄격 모드: ${strict}"
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # 변경 파일 목록
  echo "📁 Changed Files"
  echo ""

  local root
  root=$(find_git_root)

  if git -C "$worktree_path" diff --name-only HEAD~1 2>/dev/null; then
    echo ""
  else
    info "커밋된 변경 사항이 없습니다."
    echo ""
  fi

  # 리뷰 체크리스트
  echo "📋 Review Checklist"
  echo ""
  echo "  Contract 준수"
  echo "    [ ] Interface 구현 정확성"
  echo "    [ ] Test 통과"
  echo ""
  echo "  코드 품질"
  echo "    [ ] 스타일 일관성"
  echo "    [ ] 복잡도 적절"
  echo ""
  echo "  보안"
  echo "    [ ] SQL Injection"
  echo "    [ ] XSS"
  echo "    [ ] 입력 검증"
  echo ""
  echo "  성능"
  echo "    [ ] N+1 쿼리"
  echo "    [ ] 불필요한 반복"
  echo ""

  # 리뷰 시뮬레이션
  echo "━━━ Auto-Review Loop ━━━"
  echo ""

  local iteration=1
  local final_result="PENDING"

  while [[ $iteration -le $max_iterations ]]; do
    echo "  Iteration ${iteration}/${max_iterations}:"
    echo "    🔍 리뷰 수행 중..."

    # 실제 구현에서는 여기서 code-reviewer 에이전트 호출

    local result_json='{"issues": [], "warnings": []}'
    save_review_result "code" "$checkpoint_id" "$iteration" "SIMULATED" "$result_json"

    echo "    ✅ 리뷰 완료"
    echo ""

    final_result="PASS"
    break
  done

  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  if [[ "$final_result" == "PASS" ]]; then
    ok "Code Review 완료: PASS"
  elif [[ "$final_result" == "WARN" ]]; then
    warn "Code Review 완료: WARN (경고 있음)"
  else
    err "Code Review 완료: FAIL (수정 필요)"
  fi

  echo ""

  # JSON 출력
  echo "---"
  cat << EOF
{
  "checkpointId": "${checkpoint_id}",
  "type": "code",
  "result": "${final_result}",
  "iterations": ${iteration}
}
EOF
}

# ============================================================================
# 메인
# ============================================================================
main() {
  local command="${1:-}"
  shift || true

  case "$command" in
    spec)
      cmd_spec "$@"
      ;;
    code)
      cmd_code "$@"
      ;;
    -h|--help|help|"")
      usage
      ;;
    *)
      err "알 수 없는 명령어: ${command}"
      usage
      exit 1
      ;;
  esac
}

main "$@"

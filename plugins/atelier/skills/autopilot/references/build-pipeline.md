# 이슈 구현 파이프라인 (build-issues)

ready 이슈를 의존성 분석 → 병렬 구현 → 승격(PR) 까지 처리하는 파이프라인. 전처리(base 동기화·idle/capacity·throttling)는 `pipeline-control.md`, 병렬 dispatch·worktree·머지 메커니즘은 `orchestrator` skill 에 위임한다.

## Skip 이슈 알림 (선택)

`notification` 설정이 있을 때만. autopilot 분석 코멘트는 있으나 `:ready` 라벨이 없는(이전 skip 판정) 이슈를 조회해, `<!-- notified -->` 마커가 없는 건에 대해 알림 발송 후 마커 코멘트를 남긴다.

## 대상 이슈 수집

```bash
autopilot issue list --stage ready --label-prefix "{label_prefix}" --limit 20
```

- 없으면 `autopilot check mark build-issues --status idle` 후 종료.
- **재작업 감지**: `autopilot issue list --stage rework` 로 코멘트 기반 재작업 요청 이슈를 찾아 `{label_prefix}ready` 재부여 + `<!-- autopilot:rework-detected -->` 마커.

## 의존성 분석 + 유사도 검사

1. `issue-dependency-analyzer` 에이전트 (background=false) 로 병렬 실행 가능한 배치 목록 산출.
2. 첫 배치 내 이슈 텍스트 유사도 측정:
   ```bash
   echo '${BATCH_ISSUES_JSON}' | autopilot issue detect-overlap --threshold 15
   ```
   `review_required` 가 비어있지 않으면, 해당 쌍의 제목·본문·distance 를 dependency-analyzer 에 추가 context 로 전달해 병렬/순차 재판단 → 필요 시 배치 재구성.

## WIP 라벨 + Gap 이슈 사전 검증

- 현재 배치 이슈에 `{label_prefix}wip` 추가 (중복 작업 방지).
- gap-fingerprint 포함 이슈는 스펙 실존 확인:
  ```bash
  echo "${ISSUE_BODY}" | autopilot issue extract-fingerprint
  # found:false 또는 spec_exists:true → 진행 / spec_exists:false → false positive close
  ```
  false positive 는 `<!-- autopilot:false-positive -->` 코멘트와 함께 close + ready/wip 라벨 제거, 배치에서 제외.

## 구현 (Agent Team — orchestrator 위임)

구현 시작 전 idle count 리셋: `autopilot check mark build-issues --status active`.

첫 배치부터 순서대로, 배치 내 이슈를 `max_parallel_agents` 단위 서브그룹으로 분할해 실행한다. **병렬 dispatch·worktree 격리·서브그룹 순차 진행의 메커니즘은 `orchestrator` skill (`references/delegation-patterns.md`·`worktree-lifecycle.md`) 에 위임**하고, 여기서는 무엇을 전달할지만 정의한다.

- 서브그룹 병렬 실행 → 완료 대기 → **rate limit(429) 체크**: 있으면 60초 대기 후 다음 서브그룹.
- 각 이슈마다 `issue-implementer` 에이전트 (`isolation: "worktree"`, `run_in_background: true`).
- 코멘트 필터링: `echo '${COMMENTS_JSON}' | autopilot issue filter-comments` → `comments`(정제) + `failure_analysis.recommended_persona`.

전달 정보 (**모두 필수**): issue_number, issue_title, issue_body, issue_comments(filter-comments 출력), recommended_persona(null 시 생략), draft_branch=`draft/issue-{number}`, base_branch, quality_gate_command.

> **Worktree origin freshness**: implementer 가 worktree 진입 시 `origin/{base_branch}` 를 fetch 후 draft 생성/rebase (`agents/issue-implementer.md` Phase 1 Step 0). MainAgent 로컬 base 가 stale 일 수 있으므로 base_branch 만 전달하고 freshness 는 implementer 책임.

## 결과 수집 + 에스컬레이션

성공: quality gate 통과 + draft 커밋. 실패: wip 라벨 제거.

에스컬레이션 체크 (실패 이슈별 연속 실패 횟수):

```bash
gh issue view ${N} --json comments --jq '.comments[].body' | grep -o '<!-- autopilot:failure:[0-9]* -->' | tail -1
```

- **rate limit(429) 실패**: failure count 증가 없이 wip 만 제거, 다음 cycle 자동 재시도, 에스컬레이션 제외.
- **N+1 ≥ `max_consecutive_failures` (기본 3)**: `## Autopilot Escalation Report` 코멘트(실패 이력 테이블 + 권장 조치 + `<!-- autopilot:escalated -->`) 게시, ready 라벨 제거, notification 알림.
- **N+1 < threshold**: 실패 코멘트(`<!-- autopilot:failure:{N+1} -->`) 게시, ready 라벨 유지(재시도).

## 승격 (Agent Team — orchestrator 위임)

성공 이슈마다 `branch-promoter` 에이전트. 전달: draft_branch, issue_number, issue_title, base_branch(work_branch > branch_strategy), label_prefix, pr_type="auto". Step 구현과 동일한 `max_parallel_agents` 서브그룹 방식 (orchestrator 위임).

라벨 정리: 승격 성공 → wip+ready 제거 / 승격 실패 → wip 제거(재시도).

## 결과 보고 + 세션 통계

cycle 종료 시 통계 업데이트:

```bash
autopilot stats update --command build-issues \
  --processed ${P} --success ${S} --failed ${F} --false-positive ${FP}
autopilot stats show --command build-issues
```

## 원칙

- 한 cycle 에서 첫 배치만 처리 (순차 의존 후속 배치는 다음 cycle).
- wip 라벨로 중복 방지, draft 브랜치는 로컬 only (remote push 안 함).
- MainAgent 는 이슈 목록 조회·라벨 관리만, 구현은 모두 Agent 위임 (토큰 최적화).

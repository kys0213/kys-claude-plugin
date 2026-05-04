---
description: (내부용) 갭 분석 리포트를 파싱하여 GitHub issue를 생성하는 에이전트
model: haiku
tools: ["Bash"]
skills: ["issue-label", "resilience"]
---

# Gap Issue Creator

갭 분석 결과를 받아 각 갭 항목을 GitHub issue로 등록합니다.

## 입력

프롬프트로 전달받는 정보:
- 갭 분석 리포트 (마크다운)
- label_prefix (기본값: "autopilot:")
- **(선택) ledger_epic**: autopilot ledger의 epic 이름 (예: `"gap-backlog"`). 비어있거나 미전달 시 ledger 쓰기를 skip한다 (GitHub issue 흐름은 그대로 진행).
- **(선택) stagnation 컨텍스트**: 유사 이슈 목록 + persona 가이드

## 프로세스

### 1. 이슈 생성 (중복 확인 내장)

갭 리포트에서 ❌ Missing 또는 ⚠️ Partial 항목을 추출하여, autopilot CLI로 이슈를 생성합니다. CLI가 fingerprint 중복 확인과 이슈 생성을 한 번에 처리합니다.

#### WARNING 항목 제외

`⚠️ WARNING (spec-not-found)` 항목은 이슈를 생성하지 않습니다. `skipped_warnings` 목록에 추가하고 결과 보고에 포함합니다.

#### 스펙 파일 실존 검증

각 갭 항목의 스펙 경로가 실제 파일로 존재하는지 확인합니다:

```bash
if [ ! -f "${SPEC_PATH}" ]; then
  echo "SKIP: 스펙 파일 미존재 — ${SPEC_PATH}"
  # skipped_missing에 추가하고 이슈 생성을 건너뜀
  continue
fi
```

스펙 파일이 존재하지 않는 갭은 `skipped_missing` 목록에 추가하고 이슈를 생성하지 않습니다.

#### 일반 이슈 (stagnation 미감지 시)

```bash
# fingerprint 형식: gap:{spec_path}:{requirement_keyword}
autopilot issue create \
  --title "feat(scope): implement [requirement description]" \
  --label "{label_prefix}ready" \
  --fingerprint "gap:${SPEC_PATH}:${REQUIREMENT_KEYWORD}" \
  --simhash "${SIMHASH}" \
  --body "$(cat <<'EOF'
## 요구사항

[갭 분석에서 추출한 요구사항 설명]

## 관련 스펙

- 스펙 파일: [경로]
- 분석 결과: [Missing/Partial]

## 영향 범위

- 관련 파일: [entry point, call chain에서 파악된 파일들]

## 구현 가이드

[갭 분석에서 제안된 구현 방향]
EOF
)"
```

#### Stagnation 이슈 (유사 세대진화 감지 시)

stagnation 컨텍스트가 전달된 경우, **resilience** 스킬을 참조하여 새 관점의 이슈를 생성합니다.

1. **`recommended_persona`가 있으면 그대로 사용한다** (CLI가 패턴 기반으로 결정적 추천)
2. `recommended_persona`가 없으면 하위 호환: candidates의 distance 분포로 판단
3. 유사 이슈 목록에서 distance가 작은 순서대로 이슈 내용을 확인한다
4. 과거 이슈에서 시도된 접근과 실패 사유를 파악한다
5. 과거 이슈에서 이미 사용된 persona가 있으면 다음 순위 persona를 사용한다
6. 해당 persona의 관점에서 새로운 구현 방향을 작성한다

```bash
autopilot issue create \
  --title "feat(scope): implement [requirement] — [persona] approach" \
  --label "{label_prefix}ready" \
  --fingerprint "gap:${SPEC_PATH}:${REQUIREMENT_KEYWORD}" \
  --simhash "${SIMHASH}" \
  --body "$(cat <<'EOF'
## 요구사항

[기존과 동일한 gap 설명]

## 과거 시도 이력

- #42 (CLOSED): [접근 요약] — [실패 사유]
- #58 (CLOSED): [접근 요약] — [실패 사유]

## 새로운 접근 ({Persona} Persona)

이전 시도들의 공통 실패 패턴: [분석]

[선택된 persona의 관점에서 작성한 새로운 구현 방향]

### 탐색 질문
- [persona별 질문 중 이 상황에 적합한 것 2-3개]

## 관련 스펙

- 스펙 파일: [경로]

## 영향 범위

- 관련 파일: [entry point, call chain에서 파악된 파일들]
EOF
)"
```

> **참고**: fingerprint HTML 주석과 simhash는 CLI가 body 하단에 자동 삽입합니다. exit 1이면 중복(skip).

### 2. Ledger 동기 기록 (observer)

`ledger_epic`이 전달되었고, GitHub issue 생성이 성공한 경우(중복 skip 제외)에만 동일 fingerprint로 ledger task를 기록합니다. 실패해도 GitHub issue 흐름은 절대 막지 않습니다.

```bash
if [ -n "${LEDGER_EPIC:-}" ]; then
  # task id는 fingerprint의 sha256 앞 12자리(hex). 동일 fingerprint → 동일 id (idempotent).
  TASK_ID=$(printf '%s' "gap:${SPEC_PATH}:${REQUIREMENT_KEYWORD}" | shasum -a 256 | cut -c1-12)
  autopilot task add \
    --epic "$LEDGER_EPIC" \
    --id "$TASK_ID" \
    --title "$ISSUE_TITLE" \
    --fingerprint "gap:${SPEC_PATH}:${REQUIREMENT_KEYWORD}" \
    --source gap-watch \
    || echo "WARN: ledger task add 실패 (issue #${ISSUE_NUMBER}는 정상 생성됨) — 계속 진행"
fi
```

> CLI 동작:
> - 신규 fingerprint: `inserted task <id>` (exit 0)
> - 이미 등록된 fingerprint: `duplicate of task <id>` (exit 0, no-op)
> - epic 미존재 / 환경 오류: exit 2 → WARN 로그 후 무시 (GitHub issue는 이미 생성됨)
>
> 결과 보고의 `created`/`created_with_persona` 항목에 `ledger_task_id` 필드를 추가합니다 (ledger 쓰기 skip 시 `null`).

### 3. 결과 보고

생성된 이슈 목록을 JSON 형태로 출력합니다:

```json
{
  "created": [
    {"number": 42, "title": "feat(auth): implement token refresh", "fingerprint": "gap:spec/auth.md:token-refresh", "persona": null, "ledger_task_id": "a1b2c3d4e5f6"}
  ],
  "created_with_persona": [
    {"number": 73, "title": "feat(auth): implement token refresh — hacker approach", "fingerprint": "gap:spec/auth.md:token-refresh", "persona": "hacker", "related_issues": [42, 58], "ledger_task_id": "a1b2c3d4e5f6"}
  ],
  "skipped_duplicates": [
    {"fingerprint": "gap:spec/api.md:rate-limiting", "existing_issue": 38}
  ],
  "skipped_missing": [
    {"spec_path": "spec-pipeline-create", "requirement": "Pipeline Issue Creation", "reason": "spec file not found"}
  ],
  "skipped_warnings": [
    {"requirement_id": "R-008", "keyword": "spec-gap", "reason": "spec-not-found in spec_paths"}
  ]
}
```

> `ledger_task_id`는 ledger task가 성공적으로 기록되었거나 동일 fingerprint의 기존 task가 있을 때 12-hex-char id를 담습니다. `ledger_epic`이 전달되지 않았거나 ledger 쓰기가 실패하면 `null`입니다.

## 주의사항

- issue-label 스킬의 라벨 필수 규칙과 fingerprint 규칙을 반드시 따른다
- 하나의 갭 = 하나의 이슈 (원자적 단위)
- 이슈 제목은 conventional commit 형식을 따른다
- stagnation 이슈의 제목에 persona 접근법을 포함한다 (예: `— hacker approach`)
- 모든 persona가 소진된 경우 이슈 본문에 "모든 자동 접근이 소진됨 — 사람의 검토 필요"를 명시하고 `{label_prefix}ready` 라벨을 부여하지 않는다
- `--simhash` 옵션은 항상 포함하여 이후 stagnation 추적에 활용한다
- ledger 쓰기는 GitHub issue 흐름의 보조 observer다. ledger 실패가 issue 생성 결과를 무효화하지 않도록 `|| echo WARN ...` 패턴으로 격리한다

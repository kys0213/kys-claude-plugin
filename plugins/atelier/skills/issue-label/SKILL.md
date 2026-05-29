---
name: issue-label
description: GitHub 이슈/PR 생성 시 라벨 지정 규칙과 fingerprint 기반 중복 방지 컨벤션. 이슈를 생성하거나 라벨을 변경하는 모든 컴포넌트가 참조
version: 1.1.0
---

# Issue Label & Dedup Convention

## 라벨 체계

| 라벨 | 용도 | 부여 주체 |
|------|------|-----------|
| `{label_prefix}ready` | 구현 대상 이슈 | ci-watch, test-watch |
| `{label_prefix}wip` | 구현 진행 중 | build-issues |
| `{label_prefix}ci-failure` | CI 실패 이슈 (ready와 함께 부여) | ci-watch |
| `{label_prefix}auto` | autopilot 생성 PR | branch-promoter |
| `{label_prefix}qa-suggestion` | QA 테스트 제안 (사용자 검토 후 ready로 전환) | qa-boost |

> **참고**: gap-watch는 ledger-only writer로 전환되었으므로 GitHub 라벨을 사용하지 않습니다 (정방향/역방향 모두 `gap-backlog` ledger epic에 task로 기록). 과거의 `{label_prefix}spec-needed` 라벨은 더 이상 부여되지 않습니다 — 역방향 갭은 `rev-gap:*` fingerprint로 ledger에서 식별합니다.

`label_prefix`는 `github-autopilot.local.md`에서 로딩한다 (기본값: `autopilot:`).

## 라벨 필수 규칙

**이슈/PR 생성 시 `--label` 플래그는 필수이다.**

라벨 없이 생성하면 autopilot 워크플로우에서 추적되지 않는다.

### 이슈 생성

```bash
# 새 이슈 생성 — 반드시 --label 포함
gh issue create \
  --title "..." \
  --label "{label_prefix}{label_name}" \
  --body "..."
```

### 기존 이슈에 라벨 추가

```bash
# 분석 후 라벨 부여 — 반드시 --add-label 포함
gh issue edit ${ISSUE_NUMBER} --add-label "{label_prefix}{label_name}"
```

### PR 생성

```bash
# PR 생성 — 반드시 --label 포함
gh pr create \
  --title "..." \
  --label "{label_prefix}{pr_type}" \
  --body "..."
```

### 컴포넌트별 필수 라벨

| 컴포넌트 | 명령어 | 필수 라벨 |
|----------|--------|-----------|
| qa-boost | `gh issue create` | `{label_prefix}qa-suggestion` |
| ci-watch | `gh issue create` | `{label_prefix}ci-failure` + `{label_prefix}ready` |
| test-watch | `gh issue create` | `{label_prefix}ready` |
| build-issues (구현 시작) | `gh issue edit --add-label` | `{label_prefix}wip` |
| branch-promoter | `gh pr create` | `{label_prefix}auto` |

## Fingerprint 기반 중복 방지

이슈 생성 전 기존 이슈와의 중복을 **fingerprint**로 판단한다. LLM의 유사도 판단에 의존하지 않는다.

### Fingerprint 형식

| 컴포넌트 | fingerprint 형식 | 예시 |
|----------|-----------------|------|
| gap-ledger-writer (정방향) | `gap:{spec_path}:{requirement_keyword}` | `gap:spec/auth.md:token-refresh` |
| gap-ledger-writer (역방향) | `rev-gap:{file_path}:{entry_point}` | `rev-gap:src/auth/oauth.rs:handle_callback` |
| qa-boost | `qa:{source_file_path}:{test_type}` | `qa:src/auth/refresh.rs:unit` |
| ci-watch | `ci:{workflow}:{branch}:{failure_type}` | `ci:validate.yml:main:test-failure` |
| test-watch | `test:{test_name}:{failure_hash}` | `test:e2e:a1b2c3d4` |

규칙:
- 항상 소문자, 공백 없음
- requirement_keyword는 핵심 키워드를 kebab-case로 변환 (2~4단어)
- failure_type은 `test-failure`, `build-error`, `lint-error` 등 카테고리

### autopilot CLI를 사용한 이슈 생성 (권장)

`autopilot issue create` 명령이 중복 확인 + 이슈 생성 + fingerprint 삽입을 한 번에 처리한다:

```bash
autopilot issue create \
  --title "feat(auth): implement token refresh" \
  --label "{label_prefix}ready" \
  --fingerprint "gap:spec/auth.md:token-refresh" \
  --body "## 요구사항 ..."
```

Exit codes:
- `0`: 이슈 생성됨 (JSON: `{"created": true, "issue_number": 42, "url": "..."}`)
- `1`: 중복 존재, skip (JSON: `{"created": false, "duplicate": true, "issue_number": 42, ...}`)
- `2`: 오류

중복 확인만 필요한 경우:

```bash
autopilot issue check-dup --fingerprint "gap:spec/auth.md:token-refresh"
```

### CI failure 이슈 자동 정리

관련 PR이 머지된 CI failure 이슈를 자동 close:

```bash
autopilot issue close-resolved --label-prefix "{label_prefix}"
```

### Body fingerprint 삽입

`autopilot issue create` 사용 시 body 하단에 fingerprint가 **자동 삽입**된다:

```markdown
---
`fingerprint: {fingerprint_value}`
<!-- fingerprint: {fingerprint_value} -->
```

- **backtick code span**: GitHub `in:body` 검색으로 인덱싱되어 `find_duplicate()`가 중복을 탐지
- **HTML comment**: 구조적 추출 (simhash 등)에 사용

두 형태 모두 필수이며 **절대 생략하지 않는다**.

## 라벨 생성

모든 라벨은 setup 커맨드에서 일괄 생성된다.

| 라벨 | 색상 |
|------|------|
| `{label_prefix}ready` | `0E8A16` (green) |
| `{label_prefix}wip` | `FBCA04` (yellow) |
| `{label_prefix}ci-failure` | `D93F0B` (red) |
| `{label_prefix}auto` | `1D76DB` (blue) |
| `{label_prefix}qa-suggestion` | `C5DEF5` (light blue) |
| `{label_prefix}spec-needed` | `BFD4F2` (light green) |

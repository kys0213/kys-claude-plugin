---
name: issue-label
description: GitHub 이슈/PR 생성 시 라벨 지정 규칙과 fingerprint 기반 중복 방지 컨벤션. 이슈를 생성하거나 라벨을 변경하는 모든 컴포넌트가 참조
version: 1.1.0
---

# Issue Label & Dedup Convention

## 라벨 체계

| 라벨 | 용도 | 부여 주체 |
|------|------|-----------|
| `{label_prefix}ready` | 구현 대상 이슈 | gap-issue-creator, qa-boost, ci-watch, test-watch |
| `{label_prefix}wip` | 구현 진행 중 | build-issues |
| `{label_prefix}ci-failure` | CI 실패 이슈 (ready와 함께 부여) | ci-watch |
| `{label_prefix}auto` | autopilot 생성 PR | branch-promoter |

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
| gap-issue-creator | `gh issue create` | `{label_prefix}ready` |
| qa-boost | `gh issue create` | `{label_prefix}ready` |
| ci-watch | `gh issue create` | `{label_prefix}ci-failure` + `{label_prefix}ready` |
| test-watch | `gh issue create` | `{label_prefix}ready` |
| build-issues (구현 시작) | `gh issue edit --add-label` | `{label_prefix}wip` |
| branch-promoter | `gh pr create` | `{label_prefix}auto` |

## Fingerprint 기반 중복 방지

이슈 생성 전 기존 이슈와의 중복을 **fingerprint**로 판단한다. LLM의 유사도 판단에 의존하지 않는다.

### Fingerprint 형식

| 컴포넌트 | fingerprint 형식 | 예시 |
|----------|-----------------|------|
| gap-issue-creator | `gap:{spec_path}:{requirement_keyword}` | `gap:spec/auth.md:token-refresh` |
| qa-boost | `qa:{source_file_path}:{test_type}` | `qa:src/auth/refresh.rs:unit` |
| ci-watch | `ci:{workflow}:{branch}:{failure_type}` | `ci:validate.yml:main:test-failure` |
| test-watch | `test:{test_name}:{failure_hash}` | `test:e2e:a1b2c3d4` |

규칙:
- 항상 소문자, 공백 없음
- requirement_keyword는 핵심 키워드를 kebab-case로 변환 (2~4단어)
- failure_type은 `test-failure`, `build-error`, `lint-error` 등 카테고리

### 통합 이슈 생성 (권장)

`scripts/create-issue.sh`를 사용하면 중복 검사, 라벨 할당, fingerprint 삽입을 한 번에 처리한다:

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/create-issue.sh \
  --type gap \
  --title "feat(auth): implement token refresh" \
  --body "$BODY" \
  --fingerprint "gap:spec/auth.md:token-refresh" \
  --label-prefix "autopilot:"
```

| 옵션 | 설명 |
|------|------|
| `--type` | 이슈 타입: `gap`, `ci-failure`, `qa`, `test` — 타입에 따라 라벨 자동 결정 |
| `--title` | 이슈 제목 |
| `--body` | 이슈 본문 (fingerprint 주석은 자동 삽입됨) |
| `--fingerprint` | 중복 검사용 fingerprint |
| `--label-prefix` | 라벨 접두사 (기본값: `autopilot:`) |
| `--dry-run` | 실제 생성 없이 미리보기만 출력 |

Exit codes:
- `0`: 이슈 생성 성공
- `1`: 중복 이슈 존재 (skip)
- `2`: 사용법 오류

### 중복 검색 (저수준)

개별 중복 확인만 필요할 때는 `scripts/check-duplicate.sh`를 직접 사용할 수 있다:

```bash
# 중복 확인 — exit 0이면 생성 가능, exit 1이면 중복
bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-duplicate.sh "gap:spec/auth.md:token-refresh"
```

출력:
```json
{"duplicate": false}                                          # 생성 가능
{"duplicate": true, "issue_number": 42, "issue_title": "..."}  # 중복 → skip
```

### Body에 fingerprint 삽입

이슈 body 맨 하단에 HTML 주석으로 삽입한다:

```markdown
---
<!-- fingerprint: {fingerprint_value} -->
```

이 주석이 중복 검색의 유일한 기준이므로 **절대 생략하지 않는다**.

## CI Failure 이슈 자동 정리

PR 머지 후 불필요하게 남아있는 CI failure 이슈를 자동으로 close한다:

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/close-merged-ci-issues.sh "autopilot:"
```

- `autopilot:ci-failure` 라벨이 있는 open 이슈를 조회
- 이슈 제목에서 브랜치명을 추출 (`CI failure in {workflow} on {branch}` 형식)
- 해당 브랜치의 PR이 MERGED 상태이면 이슈를 자동 close

출력:
```json
{"closed": [{"number": 42, "title": "...", "branch": "..."}], "still_open": [...]}
```

## 라벨 생성

모든 라벨은 setup 커맨드에서 일괄 생성된다.

| 라벨 | 색상 |
|------|------|
| `{label_prefix}ready` | `0E8A16` (green) |
| `{label_prefix}wip` | `FBCA04` (yellow) |
| `{label_prefix}ci-failure` | `D93F0B` (red) |
| `{label_prefix}auto` | `1D76DB` (blue) |

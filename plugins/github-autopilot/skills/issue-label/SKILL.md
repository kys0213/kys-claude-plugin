---
name: issue-label
description: GitHub 이슈/PR 생성 시 라벨 지정 규칙과 fingerprint 기반 중복 방지 컨벤션. 이슈를 생성하거나 라벨을 변경하는 모든 컴포넌트가 참조
version: 1.1.0
---

# Issue Label & Dedup Convention

## 라벨 체계

| 라벨 | 용도 | 부여 주체 |
|------|------|-----------|
| `{label_prefix}ready` | 구현 대상 이슈 | gap-issue-creator, analyze-issue, ci-watch |
| `{label_prefix}wip` | 구현 진행 중 | build-issues |
| `{label_prefix}ci-failure` | CI 실패 이슈 (ready와 함께 부여) | ci-watch |
| `{label_prefix}auto` | autopilot 생성 PR | branch-promoter |
| `{label_prefix}qa` | QA 테스트 PR | branch-promoter (qa-boost 경유) |

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
| ci-watch | `gh issue create` | `{label_prefix}ci-failure` + `{label_prefix}ready` |
| analyze-issue | `gh issue edit --add-label` | `{label_prefix}ready` (ready 판정만) |
| build-issues (구현 시작) | `gh issue edit --add-label` | `{label_prefix}wip` |
| branch-promoter | `gh pr create` | `{label_prefix}auto` 또는 `{label_prefix}qa` |

## Fingerprint 기반 중복 방지

이슈 생성 전 기존 이슈와의 중복을 **fingerprint**로 판단한다. LLM의 유사도 판단에 의존하지 않는다.

### Fingerprint 형식

| 컴포넌트 | fingerprint 형식 | 예시 |
|----------|-----------------|------|
| gap-issue-creator | `gap:{spec_path}:{requirement_keyword}` | `gap:spec/auth.md:token-refresh` |
| ci-watch | `ci:{workflow}:{branch}:{failure_type}` | `ci:validate.yml:main:test-failure` |

규칙:
- 항상 소문자, 공백 없음
- requirement_keyword는 핵심 키워드를 kebab-case로 변환 (2~4단어)
- failure_type은 `test-failure`, `build-error`, `lint-error` 등 카테고리

### 중복 검색 (스크립트)

`scripts/check-duplicate.sh`를 사용하여 중복을 확인한다:

```bash
# 중복 확인 — exit 0이면 생성 가능, exit 1이면 중복
bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-duplicate.sh "gap:spec/auth.md:token-refresh"
```

출력:
```json
{"duplicate": false}                                          # 생성 가능
{"duplicate": true, "issue_number": 42, "issue_title": "..."}  # 중복 → skip
```

이슈를 생성하기 전에 **반드시 이 스크립트를 먼저 실행**한다.

### Body에 fingerprint 삽입

이슈 body 맨 하단에 HTML 주석으로 삽입한다:

```markdown
---
<!-- fingerprint: {fingerprint_value} -->
```

이 주석이 중복 검색의 유일한 기준이므로 **절대 생략하지 않는다**.

## 라벨 생성

모든 라벨은 `/github-autopilot:setup` Step 4에서 일괄 생성된다.

| 라벨 | 색상 |
|------|------|
| `{label_prefix}ready` | `0E8A16` (green) |
| `{label_prefix}wip` | `FBCA04` (yellow) |
| `{label_prefix}ci-failure` | `D93F0B` (red) |
| `{label_prefix}auto` | `1D76DB` (blue) |
| `{label_prefix}qa` | `0075CA` (teal) |

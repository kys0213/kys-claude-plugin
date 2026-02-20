---
description: "마켓플레이스 플러그인에 대한 이슈를 제보합니다"
argument-hint: "[description]"
allowed-tools:
  - Bash
  - AskUserQuestion
---

# Report Issue

마켓플레이스 플러그인 사용 중 발견한 버그나 개선 사항을 `kys0213/kys-claude-plugin` 리포지토리에 이슈로 제보합니다.

## 핵심 원칙

**맥락 우선 추론**: 대화 내역에서 최대한 정보를 추출하고, 부족한 부분만 사용자에게 확인합니다.

## 실행 흐름

### Step 1: 맥락 분석 및 정보 추론

현재 대화 맥락을 분석하여 다음을 추론합니다:

- **대상 플러그인**: 대화에서 언급된 플러그인명, 커맨드(`/develop`, `/create-issue` 등), 또는 관련 파일 경로에서 판단
- **이슈 카테고리**: 오류/문제 맥락이면 `bug`, 개선/제안 맥락이면 `enhancement`
- **이슈 내용**: 대화에서 논의된 문제점이나 제안 사항을 요약

**플러그인 매핑 참고**:

| 플러그인 | 관련 커맨드/키워드 |
|----------|-------------------|
| `develop-workflow` | `/develop`, `/outline`, `/design`, `/implement`, `/multi-review`, 설계, 리뷰, 구현 |
| `external-llm` | `/invoke-codex`, `/invoke-gemini`, Codex, Gemini, 외부 LLM |
| `git-utils` | `/git-sync`, `/git-branch`, `/commit-and-pr`, `/merge-pr`, `/create-issue`, `/prioritize-issues`, `/epic`, `/check-ci`, `/branch-status` |
| `workflow-guide` | 설계 원칙, 레이어드 아키텍처, 에이전트 설계 |
| `suggest-workflow` | `/suggest-analyze`, `/suggest-insight`, 세션 분석, 워크플로우 제안 |
| `autonomous` | `/auto`, `/auto-setup`, `/auto-dashboard`, 자율 개발, 이슈 모니터링 |
| `marketplace-issue` | `/report-issue`, 이슈 제보 |

### Step 2: 추론 결과 확인 및 보완

추론된 정보를 사용자에게 한 번에 확인합니다. 추론에 성공한 항목은 기본값으로 제시하고, 추론에 실패한 항목만 질문합니다.

**확인 항목**:

1. **대상 플러그인** (추론 실패 시에만 질문)
   - `AskUserQuestion`으로 플러그인 목록 중 선택
2. **카테고리** (추론 실패 시에만 질문)
   - bug / enhancement 중 선택
3. **제목** (항상 확인)
   - 추론된 제목을 제안하되 사용자 수정 허용
4. **상세 설명** (추론된 내용이 충분하면 확인만)
   - 대화에서 추출한 내용을 기반으로 구조화

> **중요**: 추론 가능한 항목이 많을수록 질문을 줄입니다. 모든 항목이 추론되면 최종 확인 1회만 진행합니다.

### Step 3: 마켓플레이스 리포 정보 확인

```bash
gh repo view kys0213/kys-claude-plugin --json name,owner 2>/dev/null
```

리포 접근 가능 여부를 확인합니다. 실패하면 사용자에게 `gh auth` 상태를 안내합니다.

### Step 4: 중복 이슈 확인

```bash
gh issue list -R kys0213/kys-claude-plugin --state open --json number,title,labels --limit 30
```

유사한 제목의 열린 이슈가 있는지 확인합니다:
- 유사 이슈 발견 시: 해당 이슈 번호와 제목을 보여주고 계속 진행할지 확인
- 유사 이슈 없음: 바로 생성 단계로 진행

### Step 5: 이슈 생성

```bash
gh issue create -R kys0213/kys-claude-plugin \
  --title "{제목}" \
  --body "{본문}" \
  --label "plugin:{플러그인명},{카테고리}"
```

**본문 템플릿**:

```markdown
## Plugin

**Name**: {plugin-name}
**Version**: {version}
**Category**: {bug | enhancement}

## Description

{사용자가 확인한 상세 설명}

## Expected Behavior

{enhancement 또는 bug인 경우 기대 동작}

## Current Behavior

{bug인 경우 현재 동작}

## Environment

- Plugin version: {marketplace.json에서 추출}
```

> **참고**: 카테고리가 `enhancement`인 경우 "Current Behavior" 섹션은 생략합니다.

### Step 6: 결과 안내

생성된 이슈의 URL과 번호를 사용자에게 안내합니다.

```
✓ 이슈가 생성되었습니다: {issue-url}
  - 플러그인: {plugin-name}
  - 카테고리: {category}
  - 번호: #{number}
```

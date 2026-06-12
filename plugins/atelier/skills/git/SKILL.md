---
name: git
description: ALWAYS use this skill for ANY git-related task (commit, push, branch, PR, status, diff, log, conflict resolution, unresolved review followup, issue prioritization). Provides automatic quality validation and enforces project conventions. Use atelier git CLI for all operations.
allowed-tools: Bash
---

# Git Workflow Skill

## Description

현재 프로젝트의 **형상관리 워크플로우**를 자동화합니다. 브랜치 생성, TODO별 커밋, push, PR 생성, **rebase conflict 해결**, 미해결 리뷰 정리, 이슈 우선순위 추천을 프로젝트 컨벤션에 맞게 처리합니다.

**이 스킬이 실행되는 경우:**

- "커밋 만들어줘", "변경사항 커밋해줘", "커밋하고 push 해줘"
- "PR 만들어줘", "Pull Request 생성", "커밋하고 PR 까지"
- "브랜치 만들어줘"
- "충돌 해결해줘", "conflict 해결"
- "미해결 리뷰 정리해줘", "리뷰 코멘트 봐줘"
- "뭐부터 작업할까?", "이슈 우선순위 추천해줘"
- 작업 완료 후 자동 커밋/PR 필요 시

---

## GitHub 환경 설정

`gh` CLI 명령 실행 전 환경변수를 로드합니다:

```bash
[ -f ~/.git-workflow-env ] && source ~/.git-workflow-env
```

- `/atelier:setup`으로 `~/.git-workflow-env` 생성 (GH_HOST 등)
- GitHub Enterprise 사용 시 필수

---

## atelier git CLI (결정적 연산)

모든 git 워크플로우 작업은 `atelier git` CLI로 처리합니다. setup 실행 시 단일 `atelier` 바이너리가 `~/.local/bin/atelier`에 설치됩니다.

| 서브커맨드 | 역할 |
|---|---|
| `atelier git branch <name> [--base=<b>]` | base 최신화 + 신규 브랜치 생성 |
| `atelier git commit <type> <desc> [--scope] [--body]` | Conventional/Jira 형식 커밋 생성 |
| `atelier git pr <title> [--description]` | push + PR 생성 (base 자동 감지) |
| `atelier git reviews [pr-number]` | 미해결 리뷰 쓰레드 조회 (JSON) |
| `atelier git guard <write\|commit\|pr>` | 기본 브랜치 보호 / PR 중복 차단 (hook 용) |
| `atelier git hook <register\|unregister\|list>` | settings.json hook 관리 |

인자 형식·출력 계약(JSON)·Jira/브랜치 명명 규칙·워크플로우 예시는 `references/cli-reference.md` 를 로드합니다.

---

## 커밋·push·PR 워크플로우 (판단 절차)

"커밋해줘 / push 해줘 / PR 만들어줘" 요청 시:

1. **변경사항 확인**: `git status` + `git diff --stat`. 변경이 없으면 안내 후 종료.
2. **안전 검사**: 현재 브랜치가 기본 브랜치(main/master)이면 경고하고 AskUserQuestion 으로 확인 — 거부 시 `atelier git branch` 로 새 브랜치 생성을 제안.
3. **커밋 메시지 판단**: 사용자가 메시지를 주면 그대로, 없으면 변경 내용을 분석해 type/scope/description 을 결정. 민감 파일(`.env`, `credentials*` 등)은 스테이징에서 제외.
4. **CLI 실행**: `atelier git commit ...` → (push 요청 시) `git push -u origin {브랜치}` → (PR 요청 시) `atelier git pr ...`.
5. **결과 안내**: 커밋 subject / PR URL 을 사용자에게 출력.

---

## 금지 사항

### 기본 브랜치 직접 수정 금지

```bash
git checkout main
git commit -m "..."        # 금지
git push origin main       # 금지
```

### 올바른 프로세스

```bash
atelier git branch feature/my-work
atelier git commit feat "my changes"
atelier git pr "My feature"
# → Merge 승인 대기 → atelier git sync (CLI) → git branch -d feature/my-work
```

---

## Default Branch Guard (PreToolUse Hook)

기본 브랜치에서 Write/Edit 도구 사용 또는 git commit 시도 시 **즉시 차단**하고 브랜치 생성을 제안합니다.

| Hook | Matcher | 차단 대상 |
|------|---------|----------|
| Write/Edit Guard | `Write\|Edit` | 파일 생성/수정 |
| Commit Guard | `Bash` | `git commit` 명령 |

1. PreToolUse hook → `atelier git guard write` 또는 `atelier git guard commit` 실행
2. 기본 브랜치이면 exit 2로 차단 → Claude가 `atelier git branch`로 새 브랜치 생성 → 재시도 시 pass
3. 네트워크 호출 없이 로컬 캐시만 사용. rebase/merge/detached HEAD 상태와 기본 브랜치 감지 실패 시에는 차단하지 않음 (안전)

hook 의 등록·비활성화·재설정은 `/atelier:setup` 의 hook 관리 모드가 담당합니다.

---

## references 로드 가이드

판단·프로토콜이 필요한 git 워크플로우는 아래 references 를 progressive disclosure 로 로드합니다.

| reference | 언제 로드 | 관련 흐름 |
|---|---|---|
| `references/cli-reference.md` | CLI 인자 형식·출력 계약·명명 규칙이 필요할 때 | 커밋/브랜치/PR 실행 |
| `references/conflict-resolution.md` | rebase 충돌 파일별 해결 전략 (Ours/Theirs/Manual, marker 의미, --continue/--abort/--skip) | "충돌 해결해줘" / rebase 중 |
| `references/issue-prioritization.md` | 이슈 우선순위 가중치·의존성 그래프·코드베이스 연관성 4단계 분석 | "뭐부터 작업할까" / 이슈 우선순위 |
| `references/review-followup.md` | 미해결 리뷰 필터링·파일별 그룹핑·추천 액션 형식 | "리뷰 정리해줘" |
| `references/sync-strategy.md` | 브랜치 동기화 인자 파싱·stash 정책·상태별 처리 매트릭스 | "동기화" / `atelier git sync` |

> 결정적 git 연산(commit, branch, guard, PR)은 `atelier git` CLI 로 위임합니다. references 는 **판단**이 필요한 부분만 담습니다.
> 여러 변경의 머지 조정(순서·worktree 통합)이 필요하면 `orchestrator` skill 의 `references/merge-coordinator.md` 가 canonical 입니다.

---

## 핵심 원칙

1. **작은 단위 커밋**: TODO별로 명확한 진행상황 추적
2. **PR Only**: 모든 병합은 반드시 Pull Request를 통해서만
3. **atelier git CLI 사용**: 복잡한 git 명령어 대신 `atelier git` CLI 활용
4. **Rebase 우선**: merge 대신 rebase로 깔끔한 히스토리 유지

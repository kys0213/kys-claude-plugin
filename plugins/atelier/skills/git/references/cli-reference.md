# atelier git — CLI 표면 + git 정책 레퍼런스

`atelier git` CLI 는 **기계적 호출이 꼭 필요한 연산**(hook·guard·구조화 read)만 담당한다.
커밋·브랜치·PR 생성은 결정적 래핑이 더할 게 없으므로 **에이전트가 plain `git`/`gh` 를 직접 호출**하되,
아래 정책(컨벤션)을 적용한다.

```bash
atelier git --version    # 버전 확인
atelier git --help       # 전체 도움말 (reviews / guard / hook / setup / pr-guard)
```

## 도구 경계 (이 연산엔 이 도구 하나)

| 분류 | 도구 | 예시 |
|---|---|---|
| 조회·plumbing (정책 없음, determinism 더할 것 없음) | **plain `git`** | `status`, `diff`, `log`, `push` |
| 컨벤션을 담은 쓰기 (정책 적용) | **plain `git` / `gh`** | 커밋(Jira·Conventional), 브랜치(명명·base), PR(`gh pr create`) |
| 바이너리 필수 (hook·구조화 read·hook 설치) | **`atelier git`** | `guard`, `pr-guard`, `hook`, `setup`, `reviews` |

리트머스: *"gh 출력을 구조화하거나, PreToolUse hook 이거나, settings.json 편집인가?"*
→ Yes 면 `atelier git`, No(순수 조회·컨벤션 쓰기)면 plain `git`/`gh`.

---

# A. `atelier git` CLI (기계적 호출)

## 1. 미해결 리뷰 조회

```bash
atelier git reviews [pr-number]
```

**출력 (JSON):** PR 제목, URL, 리뷰 쓰레드 목록.

> PR 번호 미지정 시 현재 브랜치의 PR 을 자동 감지한다. gh GraphQL 응답을 구조화 JSON 으로
> 변환하는 결정적 read 라 CLI 가 담당한다. 결과 해석·후속 액션(리뷰 정리) 제안은 git skill 이 판단한다.

## 2. Tool Guard (branch 보호 · PR 중복)

```bash
atelier git guard <write|commit|pr> --project-dir=<p> [--create-branch-script=<s>] [--default-branch=<b>] [--protected-branches=<csv>]
```

- `write`/`commit`: 보호 브랜치에서 차단 시 exit 2, 통과 시 exit 0. 차단 메시지의 브랜치 생성 안내는
  `--create-branch-script` 값(기본 `git switch -c`)을 출력한다.
- `pr`: 현재 브랜치에 열린 PR 이 있으면 `gh pr create` 차단 (exit 2). branch 옵션 불필요. legacy alias: `atelier git pr-guard`.
- `--default-branch` 미지정 시 guard 가 런타임에 readonly 감지(`origin/HEAD` → main/develop/master 추측)한다.
  이 값을 박는 것은 `atelier git setup guard` 의 책임이다 (§4).

> **이 명령은 hook 런타임이다.** stdin 으로 PreToolUse 페이로드를 받고 exit 2 로 차단을 신호한다.
> 등록·설치용으로 호출하지 않는다 — 그건 §4 다.

## 3. Hook 관리

```bash
atelier git hook register <hookType> <matcher> <command> [--timeout=<n>] [--project-dir=<p>]
atelier git hook unregister <hookType> <command> [--project-dir=<p>]
atelier git hook list [hookType] [--project-dir=<p>]
```

> settings.json 편집은 결정적 변환이라 CLI 가 담당한다 (LLM 이 직접 Write 하지 않음).
> `register` 는 command **완전 일치**로만 기존 항목을 지운다 — 옛 형식(예: 꼬리에 `--default-branch main`)을
> 정리하려면 §4 를 쓴다. 개별 hook(PR Guard 등) 등록에만 직접 사용한다.

## 4. Guard hook 설치 (`setup guard`)

```bash
atelier git setup guard --project-dir <PATH> --scope <user|project> [--dry-run]
```

Write/Edit·Commit guard 2종의 감지·마이그레이션·등록을 한 번에 수행하는 **설치** 명령이다. §2 의 guard
(런타임)와 별개 surface 인 이유: 런타임은 exit 2 가 "차단" 이라 설치 실패를 그 코드로 신호할 수 없다.
이 명령은 성공 시 JSON + exit 0, 실패 시 `Error: ...` + exit 1 이며 **절대 exit 2 를 내지 않는다.**

수행 순서:

1. `git remote set-head origin --auto` (project-dir 기준) — `origin/HEAD` warm-up. 실패해도 계속 진행하고
   `originHeadWarmed: false` 로 보고한다.
2. 기본 브랜치 감지 — `gh repo view --json defaultBranchRef` → 실패 시 readonly 감지. 값이 없거나 공백이면
   **`--default-branch` 플래그를 통째로 생략**한다 (값 없는 플래그는 hook 실행 시 clap 파싱 실패 → exit 2 →
   모든 편집 차단).
3. `atelier git guard write `/`atelier git guard commit ` **접두** 일치 기존 엔트리 제거 (마이그레이션).
4. `PreToolUse`/`Write|Edit` + `PreToolUse`/`Bash` 를 **단일 쓰기**로 등록.

- `--scope user` 는 `--default-branch` 를 박지 않는다 — 전역 pin 하나가 모든 프로젝트에 한 repo 의 기본
  브랜치를 강요하기 때문(#810). 1단계 warm-up 이 런타임 감지로 대체한다. pin 이 필요하면 `--scope project`.
- `--project-dir` 는 **보호 대상 repo** 다 (warm-up·감지의 앵커). settings.json 경로는 scope 가 정한다:
  `user` → `$HOME/.claude/settings.json`, `project` → `<project-dir>/.claude/settings.json`.
- settings.json 에는 `--project-dir "${CLAUDE_PROJECT_DIR:-.}"` 가 **리터럴로** 기록된다 (hook 실행 시점 expand).
- `--dry-run` 은 계획(등록될 command, 제거될 항목)만 출력하고 파일을 쓰지 않는다.
- 재실행은 멱등이다.

**출력 (JSON):**

```json
{
  "scope": "project",
  "settingsPath": "/work/my-repo/.claude/settings.json",
  "defaultBranch": "trunk",
  "originHeadWarmed": true,
  "commands": [
    "atelier git guard write --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch trunk",
    "atelier git guard commit --project-dir \"${CLAUDE_PROJECT_DIR:-.}\" --default-branch trunk"
  ],
  "removed": [],
  "dryRun": false
}
```

> `removed` 는 접두 매칭으로 정리된 옛 등록분이다 — 비어 있지 않으면 마이그레이션이 일어난 것이다.
> guard hook 의 비활성화·재설정 절차는 통합 setup 의 hook 관리 모드가 담당한다.

---

# B. git 정책 (에이전트가 plain git/gh 로 적용)

결정적 래핑이 git/gh 가 이미 주는 것 이상을 더하지 않으므로 CLI 로 감싸지 않는다.
에이전트가 아래 컨벤션을 적용해 직접 실행한다.

## 1. 브랜치 생성

```bash
git fetch origin --prune
git switch -c <branch-name> origin/<base>   # base 최신 기준으로 분기
```

- base 미지정 시 기본 브랜치(`gh repo view --json defaultBranchRef -q .defaultBranchRef.name`,
  실패 시 main/develop/master)에서 분기한다.
- 작업 전 uncommitted 변경이 있으면 먼저 커밋/stash 하도록 안내한다.

### 브랜치 명명 규칙

**Jira 티켓 브랜치** — 패턴 `WAD-0212`, `feat/WAD-0212`, `feat/wad-0212`(커밋 시 자동 대문자) → 커밋 형식 `[TICKET] type: description`.

**일반 브랜치**

| 타입     | 패턴         | 예시                | 커밋 형식                      |
| -------- | ------------ | ------------------- | ------------------------------ |
| 기능     | `feature/*`  | `feature/user-auth` | `feat(scope): description`     |
| 수정     | `fix/*`      | `fix/memory-leak`   | `fix(scope): description`      |
| 문서     | `docs/*`     | `docs/api-guide`    | `docs(scope): description`     |
| 리팩터링 | `refactor/*` | `refactor/cleanup`  | `refactor(scope): description` |
| 성능     | `perf/*`     | `perf/optimize`     | `perf(scope): description`     |
| 테스트   | `test/*`     | `test/unit`         | `test(scope): description`     |

## 2. 커밋 생성

```bash
git add -u                     # tracked 변경만 (민감파일 제외 확인 후 필요시 개별 add)
git commit -m "<subject>" [-m "<body>"]
```

**Subject 형식 (Conventional / Jira):**

- Jira 브랜치(`feat/wad-0212`) → `[WAD-0212] feat: implement user authentication`
- scope 지정 → `feat(auth): implement user authentication`
- type 은 `feat`·`fix`·`docs`·`style`·`refactor`·`test`·`chore`·`perf` 중 하나.

**적용 순서:** 브랜치명에서 Jira 티켓 감지 → 형식 선택 → 민감 파일(`.env`, `credentials*` 등) 스테이징 제외 → 커밋.

## 3. PR 생성

### PR pre-check (스코프 확정 전 필수)

diff 스코프를 확정하기 전에 브랜치·머지 상태를 검증한다. 3가지 확인 없이 스코프를 확정하지 않는다.

```bash
# 1. base 와의 관계 확인 — base 에 없는 커밋만 스코프에 담기는지
git fetch origin --prune
git log --oneline origin/<base>..HEAD

# 2. 동일 변경이 이미 base 에 머지되었는지 확인
gh pr list --state merged --search "<핵심 키워드>" --limit 10
git merge-base --is-ancestor HEAD origin/<base> && echo "HEAD 가 이미 base 에 포함됨 — PR 불필요"

# 3. 열린 동일 목적 PR 존재 여부 확인
gh pr list --state open --search "<핵심 키워드>"
gh pr list --head <branch>   # 현재 브랜치의 기존 PR
```

- 1번 출력이 비어 있으면 base 에 더할 커밋이 없다 — PR 을 만들지 않고 사용자에게 상태를 보고한다.
- back-merge/hotfix 세션에서 레포 상태 파악이 흔들렸다면 pre-check 를 처음부터 다시 수행한다.

### 실행

```bash
git push -u origin <branch>
gh pr create --base <default-branch> --title "<title>" --body "<body>"
```

- base 는 `gh` 가 자동 감지하므로 명시 안 해도 되지만, 비표준 기본 브랜치면 `--base` 로 지정한다.
- Jira 브랜치면 제목을 `[TICKET] <title>` 로 prefix 한다.
- **PR 본문 스타일:** 토스 PR 템플릿 4단 고정(왜 / 무엇을 / 어떻게 / 확인 방법) + 친근한 해요체 단문 + 개조식.
  약어/사내용어는 풀어쓴다. **섹션 4단 고정은 이 절이 단일 출처**이고, 그 안을 채우는 작문 기준
  (독자 수준·맥락 이전·용어 처리)은 `communicate` 스킬이 단일 출처다 — PR 은 그 기준의 채널 특수화다.

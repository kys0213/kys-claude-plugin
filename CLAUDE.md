# CLAUDE.md

## PR 타이틀 (필수 준수)

형식: `<type>(<scope>): <description>`

- type: `feat` | `fix` | `refactor` | `docs` | `ci` | `chore` | `test` | `perf`
- description: 영어 소문자 시작, 마침표 없음, 명령형 현재 시제 (add, fix, update)
- 50자 이내 권장

### plugins/ 또는 common/ 코드 변경이 포함된 PR

반드시 버전 범프 type만 사용:

- `feat` (minor), `fix` (patch), `refactor` (patch), `major` (major)
- `chore`, `docs`, `ci`, `test`, `perf` 등은 사용 불가 (CI 실패)

### Scope 목록

`team-claude` | `planning` | `review` | `external-llm` | `suggest-workflow` | `git-utils` | `cli` | `hooks` | `scripts`

### PR 생성 전 체크리스트

1. 변경 파일에 `plugins/` 또는 `common/`이 포함되는가?
2. 포함되면 → type이 `feat`, `fix`, `refactor`, `major` 중 하나인가?
3. description이 소문자로 시작하고 마침표 없이 끝나는가?
4. 명령형 현재 시제인가? (add, fix, update - added, fixed, updated 아님)

## 커밋 메시지

PR 타이틀과 동일한 형식을 따름. 상세 규칙은 `.claude/rules/git-workflow.md` 참조.

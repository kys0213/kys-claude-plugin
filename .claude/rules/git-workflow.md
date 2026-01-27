# Git Workflow Rules

## PR 타이틀 규칙

PR 타이틀은 반드시 Conventional Commits 형식을 따릅니다:

```
<type>(<scope>): <description>
```

### Type (필수)

| Type | 설명 | 버전 범프 |
|------|------|----------|
| `feat` | 새 기능 추가 | minor |
| `fix` | 버그 수정 | patch |
| `refactor` | 리팩토링 (기능 변경 없음) | patch |
| `docs` | 문서 변경 | - |
| `ci` | CI/CD 변경 | - |
| `chore` | 기타 (빌드, 설정 등) | - |
| `test` | 테스트 추가/수정 | - |
| `perf` | 성능 개선 | patch |

### Scope (권장)

이 프로젝트의 주요 scope:

- `team-claude` - Team Claude 플러그인
- `planning` - Planning 플러그인
- `review` - Review 플러그인
- `external-llm` - External LLM 플러그인
- `git-utils` - Git Utils 플러그인
- `cli` - tc CLI
- `hooks` - Hook 스크립트
- `scripts` - 셸 스크립트

### Description (필수)

- 영어 소문자로 시작
- 마침표 없이 끝냄
- 명령형 현재 시제 사용 (add, fix, update - added, fixed 아님)
- 50자 이내 권장

### 예시

```
feat(team-claude): add tc CLI for project testing
fix(hooks): resolve path issue in settings.local.json
refactor(scripts): simplify tc-config init logic
docs(planning): update architecture diagram
ci: add GitHub Actions for CLI build
chore(cli): update dependencies
```

### 잘못된 예시

```
❌ Update stuff                    # type 없음
❌ feat: Add new feature.          # 마침표, 대문자
❌ FEAT(team-claude): add feature  # type 대문자
❌ feat(team-claude) add feature   # 콜론 없음
```

## 커밋 메시지 규칙

커밋 메시지도 동일한 형식을 따르되, 본문에 상세 설명을 추가할 수 있습니다:

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Footer

- `Co-Authored-By:` - 공동 작업자
- `Closes #123` - 이슈 참조
- `BREAKING CHANGE:` - 호환성 깨지는 변경

## 브랜치 명명 규칙

```
<type>/<short-description>
```

예시:
- `feat/add-tc-cli`
- `fix/hook-path-issue`
- `refactor/simplify-config`

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

- **플러그인 변경**: `plugins/` 하위 디렉토리 이름을 scope로 사용
  - 예: `plugins/develop-workflow/` → `feat(develop-workflow): ...`
- **플러그인 외 변경**: `hooks`, `scripts`, `common` 등 해당 디렉토리 이름을 scope로 사용

### Description (필수)

- 영어 소문자로 시작
- 마침표 없이 끝냄
- 명령형 현재 시제 사용 (add, fix, update - added, fixed 아님)
- 50자 이내 권장

### 예시

```
feat(develop-workflow): add multi-LLM support
fix(hooks): resolve path issue in settings.local.json
refactor(scripts): simplify tc-config init logic
docs(develop-workflow): update architecture diagram
ci: add GitHub Actions for CLI build
chore(cli): update dependencies
```

### 코드 변경 시 PR 타이틀 제약 (CI 검증)

`plugins/` 또는 `common/` 디렉토리에 코드 변경이 포함된 PR은 **반드시 버전 범프를 트리거하는 type**을 사용해야 합니다:

| 허용 Type | 버전 범프 |
|-----------|----------|
| `feat` | minor |
| `fix` | patch |
| `refactor` | patch |
| `major` | major |

`docs`, `ci`, `chore`, `test`, `style`, `perf`, `build`, `revert` 등은 `plugins/` 또는 `common/` 변경이 **없을 때만** 사용 가능합니다.

```
# plugins/ 변경 포함 PR
✅ refactor(suggest-workflow): improve Rust logic
✅ fix(git-utils): resolve path issue
✅ feat(develop-workflow): add multi-LLM support

# plugins/ 변경 포함 PR — CI 실패
❌ chore(suggest-workflow): update dependencies  # 버전 범프 prefix 아님
❌ perf(git-utils): optimize startup              # perf는 버전 범프 미지원
❌ docs(develop-workflow): update README          # docs는 버전 범프 미지원
```

> **주의**: CI의 `validate.yml`에서 `Check version bump prefix for code changes` 단계로 검증됩니다. PR 생성 시 변경 대상 디렉토리를 확인하고 적절한 type을 선택하세요.

### 잘못된 예시

```
❌ Update stuff                    # type 없음
❌ feat: Add new feature.          # 마침표, 대문자
❌ FEAT(git-utils): add feature    # type 대문자
❌ feat(git-utils) add feature     # 콜론 없음
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

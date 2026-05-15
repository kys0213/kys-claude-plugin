---
paths:
  - "**/.github/**"
---

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
| `docs` | - (버전 범프 없음) |

`ci`, `chore`, `test`, `style`, `perf`, `build`, `revert` 등은 `plugins/` 또는 `common/` 변경이 **없을 때만** 사용 가능합니다.

```
# plugins/ 변경 포함 PR
✅ refactor(suggest-workflow): improve Rust logic
✅ fix(git-utils): resolve path issue
✅ feat(develop-workflow): add multi-LLM support
✅ docs(autodev): add code review report

# plugins/ 변경 포함 PR — CI 실패
❌ chore(suggest-workflow): update dependencies  # 버전 범프 prefix 아님
❌ perf(git-utils): optimize startup              # perf는 버전 범프 미지원
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

## PR 본문 작성 스타일

PR **본문(description)** 은 다음 규칙을 따릅니다. (타이틀/커밋 메시지는 위 영문 규칙 그대로 유지)

### 원칙

- **독자**: IT 특성화 고등학생도 이해할 수 있는 수준 — 약어/사내용어는 풀어쓰기
- **문체**: 친근한 해요체 단문 ("~했어요", "~해요"). 한 문장은 짧게
- **정보 전달**: 개조식 (불릿/번호 목록) 우선. 줄글 설명은 최소화
- **언어**: 한국어 (코드/명령/식별자는 원문 유지)

### 토스 PR 템플릿 (4단 고정)

```markdown
## 왜
- 이 작업이 왜 필요했는지 1~3줄로
- 배경, 문제 상황, 의사결정 근거

## 무엇을
- 어떤 변경을 했는지 개조식으로
- 파일/모듈 단위로 나눠서

## 어떻게
- 핵심 구현 방식, 선택한 접근법
- 대안이 있었다면 왜 이걸 골랐는지

## 확인 방법
- [ ] 리뷰어가 따라할 수 있는 검증 단계
- [ ] 자동 테스트 / 수동 시나리오
```

### 예시 (좋음)

```markdown
## 왜
- 기본 브랜치에서 실수로 커밋하는 사고가 자주 났어요.
- Pre-commit 훅으로 막으면 안전해요.

## 무엇을
- `git-utils guard commit` 명령을 추가했어요.
- PreToolUse 훅에 등록되도록 `/setup`을 수정했어요.

## 어떻게
- 네트워크 호출 없이 로컬 캐시만 봐요. 빠르거든요.
- rebase 중에는 건너뛰어요. 충돌 해결을 막으면 곤란하니까요.

## 확인 방법
- [ ] `main`에서 `git commit` 시도 → exit 2로 차단되는지 확인
- [ ] `feature/*` 브랜치에서 정상 통과되는지 확인
```

### 안티패턴

```
❌ "본 PR에서는 ~을 추가하였습니다"     # 격식체, 줄글
❌ "implements XYZ pattern with refactoring"  # 영문 본문
❌ ## 변경사항 / ## 테스트                # 섹션 이름 임의 변경 (4단 고정)
❌ ## 왜 / ## 어떻게                      # 섹션 생략 (4단 모두 필수)
```

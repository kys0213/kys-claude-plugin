---
name: template
description: Worker 템플릿 관리 - 조회, 생성, 수정, 삭제
argument-hint: "<action> [name]"
allowed-tools: ["Bash", "Read", "Write", "AskUserQuestion"]
---

# Team Claude Template Command

Worker Claude용 템플릿을 관리합니다.

## 사용법

```bash
/team-claude:template <action> [name]
```

## 액션

| Action | 설명 | 예시 |
|--------|------|------|
| `list` | 템플릿 목록 | `/team-claude:template list` |
| `show` | 템플릿 상세 | `/team-claude:template show standard` |
| `create` | 새 템플릿 생성 | `/team-claude:template create` |
| `edit` | 템플릿 수정 | `/team-claude:template edit custom-fe` |
| `delete` | 템플릿 삭제 | `/team-claude:template delete custom-fe` |

## API 연동

### list - 템플릿 목록

```bash
curl -s http://localhost:3847/config/templates | jq
```

**출력 형식:**
```
╔══════════════════════════════════════════════════════════════╗
║               Worker Templates                               ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  [Built-in]                                                  ║
║  ✓ minimal      최소 지시, 자유도 높음                         ║
║  ✓ standard     TDD + 커밋 컨벤션 (기본값)                     ║
║  ✓ strict       린트/테스트 통과 필수                          ║
║                                                              ║
║  [Custom]                                                    ║
║    custom-fe    프론트엔드 전용                                ║
║    custom-api   API 개발용                                    ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝
```

### show - 템플릿 상세

```bash
curl -s http://localhost:3847/config/templates/standard | jq
```

**출력:**
```markdown
# Template: standard

## 설명
TDD + 커밋 컨벤션 (기본값)

## 적용 규칙
- test-required
- conventional-commits

## CLAUDE.md 내용
---
# Worker Task

## Task
{{TASK_DESCRIPTION}}

## 작업 규칙
1. 구현 전 테스트 먼저 작성 (TDD)
2. 커밋은 conventional commits 형식
3. 완료 전 셀프 리뷰
...
---
```

### create - 새 템플릿 생성

대화형으로 템플릿 생성:

```
> /team-claude:template create

📝 새 Worker 템플릿 생성

템플릿 이름: custom-mobile
설명: React Native 모바일 개발용

기반 템플릿을 선택하세요:
  1. 없음 (처음부터)
  2. minimal
  3. standard (권장)
  4. strict
선택 [3]: 3

추가 규칙을 입력하세요 (빈 줄로 완료):
- iOS/Android 모두 빌드 확인
- 스크린샷 테스트 포함

CLAUDE.md에 추가할 내용:
---
## 모바일 특화 규칙
- Expo/React Native 빌드 확인
- iOS 시뮬레이터 테스트
- Android 에뮬레이터 테스트
---

✅ 템플릿 'custom-mobile' 생성됨
```

**API:**
```bash
curl -X POST http://localhost:3847/config/templates \
  -H "Content-Type: application/json" \
  -d '{
    "template": {
      "name": "custom-mobile",
      "description": "React Native 모바일 개발용",
      "baseTemplate": "standard",
      "claudeMd": "# Worker Task\n...",
      "rules": ["test-required", "build-check"]
    },
    "scope": "project"
  }'
```

### edit - 템플릿 수정

```
> /team-claude:template edit custom-mobile

📝 템플릿 수정: custom-mobile

현재 설명: React Native 모바일 개발용
새 설명 (Enter로 유지):

현재 규칙: test-required, build-check
규칙 수정:
  1. 유지
  2. 추가
  3. 제거
  4. 전체 교체
선택 [1]: 2

추가할 규칙: screenshot-test

✅ 템플릿 'custom-mobile' 수정됨
```

### delete - 템플릿 삭제

```
> /team-claude:template delete custom-mobile

⚠️  템플릿 삭제: custom-mobile

이 템플릿을 사용 중인 Worker가 있을 수 있습니다.
삭제하시겠습니까? [y/N]: y

✅ 템플릿 'custom-mobile' 삭제됨
```

## 내장 템플릿

### minimal
```markdown
# Worker Task

아래 Task를 구현하세요.

## Task
{{TASK_DESCRIPTION}}

## 완료 조건
- 기능 동작 확인
```

### standard (기본값)
```markdown
# Worker Task

## Task
{{TASK_DESCRIPTION}}

## 작업 규칙
1. 구현 전 테스트 먼저 작성 (TDD)
2. 커밋은 conventional commits 형식
3. 완료 전 셀프 리뷰

## 완료 조건
- [ ] 모든 테스트 통과
- [ ] 타입 에러 없음
- [ ] 기능 동작 확인

## 막히면
- 구체적인 blocker 설명과 함께 완료 보고
```

### strict
```markdown
# Worker Task

## Task
{{TASK_DESCRIPTION}}

## 필수 규칙
1. TDD 필수
2. 테스트 커버리지 80% 이상
3. ESLint/Prettier 통과 필수
4. TypeScript strict mode
5. 모든 exported 함수에 JSDoc
6. Conventional Commits

## 완료 전 체크리스트
- [ ] `npm run lint` 통과
- [ ] `npm run test` 통과
- [ ] `npm run type-check` 통과
- [ ] 커버리지 80% 이상

## 금지 사항
- console.log 사용 금지
- any 타입 사용 금지
- 주석 처리된 코드 커밋 금지
```

## 템플릿 변수

템플릿에서 사용 가능한 변수:

| 변수 | 설명 |
|------|------|
| `{{TASK_DESCRIPTION}}` | Task 설명 (spawn 시 전달) |
| `{{FEATURE_NAME}}` | 피처 이름 |
| `{{BRANCH_NAME}}` | 브랜치 이름 |
| `{{TIMESTAMP}}` | 생성 시간 |

## 관련 커맨드

- `/team-claude:config` - 기본 템플릿 설정
- `/team-claude:spawn` - Worker 생성 시 템플릿 지정
- `/team-claude:rules` - 템플릿에 적용할 규칙 관리

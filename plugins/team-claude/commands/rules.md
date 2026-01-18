---
name: rules
description: 리뷰 규칙 관리 - 코드 리뷰 시 적용할 규칙 설정
argument-hint: "<action> [name]"
allowed-tools: ["Bash", "Read", "Write", "AskUserQuestion"]
---

# Team Claude Rules Command

코드 리뷰 시 적용할 규칙을 관리합니다.

## 사용법

```bash
/team-claude:rules <action> [name]
```

## 액션

| Action | 설명 | 예시 |
|--------|------|------|
| `list` | 규칙 목록 | `/team-claude:rules list` |
| `show` | 규칙 상세 | `/team-claude:rules show no-any` |
| `add` | 규칙 추가 | `/team-claude:rules add` |
| `edit` | 규칙 수정 | `/team-claude:rules edit no-any` |
| `toggle` | 활성화/비활성화 | `/team-claude:rules toggle no-any` |
| `delete` | 규칙 삭제 | `/team-claude:rules delete no-any` |

## API 연동

### list - 규칙 목록

```bash
curl -s http://localhost:3847/config/rules | jq
```

**출력 형식:**
```
╔══════════════════════════════════════════════════════════════╗
║                   Review Rules                               ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  [Enabled]                                                   ║
║  ✓ test-coverage     80% 이상 커버리지 필수      [error]      ║
║  ✓ no-console        console.log 사용 금지      [warning]    ║
║  ✓ conventional-commits  커밋 메시지 규칙        [error]      ║
║                                                              ║
║  [Disabled]                                                  ║
║  ○ no-any            any 타입 사용 금지         [warning]    ║
║  ○ max-file-lines    파일당 300줄 제한          [info]       ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝
```

### add - 규칙 추가

대화형으로 규칙 생성:

```
> /team-claude:rules add

📏 새 리뷰 규칙 추가

규칙 이름: no-magic-numbers
설명: 매직 넘버 사용 금지

검사 타입을 선택하세요:
  1. lint - ESLint 등 도구 사용
  2. pattern - 정규식 매칭
  3. ai - AI 리뷰어 판단
선택 [1]: 1

ESLint 규칙: no-magic-numbers
린터 종류 [eslint]: eslint

심각도를 선택하세요:
  1. error - 반드시 수정
  2. warning - 수정 권장
  3. info - 참고 사항
선택 [2]: 2

✅ 규칙 'no-magic-numbers' 추가됨
```

**API:**
```bash
curl -X POST http://localhost:3847/config/rules \
  -H "Content-Type: application/json" \
  -d '{
    "rule": {
      "name": "no-magic-numbers",
      "description": "매직 넘버 사용 금지",
      "type": "lint",
      "config": {
        "rule": "no-magic-numbers",
        "linter": "eslint"
      },
      "severity": "warning",
      "enabled": true
    },
    "scope": "project"
  }'
```

## 규칙 타입

### 1. lint - 린터 규칙

ESLint, TSC 등 린터 도구 연동:

```json
{
  "name": "no-any",
  "type": "lint",
  "config": {
    "rule": "@typescript-eslint/no-explicit-any",
    "linter": "eslint"
  },
  "severity": "error"
}
```

### 2. pattern - 정규식 매칭

파일 내용 패턴 검사:

```json
{
  "name": "no-console",
  "type": "pattern",
  "config": {
    "pattern": "console\\.(log|error|warn)\\(",
    "action": "deny",
    "files": "**/*.ts"
  },
  "severity": "warning"
}
```

### 3. ai - AI 리뷰

AI 리뷰어에게 판단 위임:

```json
{
  "name": "code-quality",
  "type": "ai",
  "config": {
    "prompt": "코드가 SOLID 원칙을 따르는지 확인하세요. 특히 단일 책임 원칙과 의존성 역전 원칙에 주목하세요."
  },
  "severity": "info"
}
```

## 심각도 레벨

| Level | 설명 | 리뷰 영향 |
|-------|------|----------|
| `error` | 반드시 수정 필요 | Request Changes |
| `warning` | 수정 권장 | Comment |
| `info` | 참고 사항 | Comment (optional) |

## 내장 규칙

### test-required
- **타입**: ai
- **설명**: 테스트 코드 작성 필수
- **심각도**: error

### test-coverage-80
- **타입**: ai
- **설명**: 테스트 커버리지 80% 이상
- **심각도**: error

### conventional-commits
- **타입**: pattern
- **설명**: 커밋 메시지가 conventional commits 형식
- **심각도**: error

### no-console
- **타입**: pattern
- **설명**: console.log/error/warn 사용 금지
- **심각도**: warning

### no-any
- **타입**: lint
- **설명**: TypeScript any 타입 사용 금지
- **심각도**: warning

### lint-required
- **타입**: ai
- **설명**: ESLint/Prettier 통과 필수
- **심각도**: error

## 규칙 활성화/비활성화

```
> /team-claude:rules toggle no-any

규칙 'no-any' 상태: disabled → enabled

✅ 규칙 'no-any' 활성화됨
```

## 프로젝트별 규칙

규칙은 `.team-claude/config.json`의 `review.rules`에 저장됩니다:

```json
{
  "review": {
    "rules": [
      {
        "name": "no-any",
        "description": "any 타입 사용 금지",
        "type": "lint",
        "config": { "rule": "@typescript-eslint/no-explicit-any" },
        "severity": "warning",
        "enabled": true
      }
    ]
  }
}
```

## 템플릿과 규칙 연동

템플릿에 규칙을 연결하면 해당 템플릿 사용 시 자동 적용:

```json
{
  "templates": {
    "strict": {
      "rules": ["no-any", "no-console", "test-coverage-80"]
    }
  }
}
```

## 관련 커맨드

- `/team-claude:template` - 템플릿에 규칙 적용
- `/team-claude:review` - 규칙 기반 코드 리뷰
- `/team-claude:config` - 전체 설정 관리

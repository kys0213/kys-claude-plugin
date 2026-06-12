# 미해결 리뷰 정리 (review followup)

"리뷰 정리해줘", "미해결 코멘트 봐줘" 같은 요청 시 수행하는 절차. 조회는 CLI(결정적), 그룹핑·액션 제안은 이 reference 의 판단 규칙을 따른다.

## Step 1: 미해결 리뷰 조회

`atelier git reviews` CLI 를 실행하여 리뷰 쓰레드를 가져온다.

```bash
atelier git reviews [PR_NUMBER]
```

> PR 번호 미지정 시 현재 브랜치의 PR 을 자동 감지한다.

## Step 2: 결과 분석

JSON 결과에서 `isResolved: false`인 쓰레드만 필터링한다.

각 미해결 쓰레드에 대해 다음 정보를 정리하여 출력한다:

| 항목 | 설명 |
|------|------|
| 파일 경로 | `path` 필드 |
| 라인 번호 | `line` 필드 |
| 리뷰어 | 첫 번째 코멘트의 `author.login` |
| 코멘트 내용 | 쓰레드의 모든 코멘트 `body` |
| 링크 | 첫 번째 코멘트의 `url` |

## Step 3: 결과 출력

- **미해결 항목이 없으면**: "모든 리뷰가 해결되었습니다" 메시지 출력
- **미해결 항목이 있으면**: 각 항목을 파일별로 그룹핑하여 출력하고, 다음 액션을 제안한다:
  - 해당 파일 수정이 필요한 경우 → 수정 제안
  - 답변이 필요한 경우 → 답변 작성 제안

## 출력 형식 예시

```
## PR #123: Feature title
총 3개의 미해결 리뷰

### src/auth/login.ts (2건)

1. **Line 42** - @reviewer1
   > 이 부분에서 에러 핸들링이 필요합니다
   🔗 [링크](https://github.com/...)

2. **Line 78** - @reviewer2
   > 타입 체크를 추가해주세요
   🔗 [링크](https://github.com/...)

### src/utils/validate.ts (1건)

1. **Line 15** - @reviewer1
   > 유틸리티 함수로 분리하는 게 좋겠습니다
   🔗 [링크](https://github.com/...)

---
**추천 액션:**
- [ ] src/auth/login.ts: 에러 핸들링 추가 (Line 42)
- [ ] src/auth/login.ts: 타입 체크 추가 (Line 78)
- [ ] src/utils/validate.ts: 유틸리티 함수 분리 (Line 15)
```

## 에러 처리

- gh 인증 실패 → `gh auth login` 안내
- 현재 브랜치에 PR 없음 → PR 번호를 명시하도록 안내

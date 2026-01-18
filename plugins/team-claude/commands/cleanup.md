---
name: team-claude:cleanup
description: 작업 완료 후 회고 및 개선 - 작업 분석, 에이전트/스킬/문서 개선 제안, 리소스 정리
argument-hint: "[task-id] [--analyze | --improve | --all] [--dry-run]"
allowed-tools: ["Task", "Bash", "Read", "Write", "Glob", "Grep", "AskUserQuestion"]
---

# Team Claude Cleanup & Retrospective

작업 완료 후 **분석 → 개선 제안 → 정리**를 수행하는 회고 커맨드입니다.

## 핵심 철학

> "모든 완료된 작업은 시스템을 개선할 기회"

단순 정리가 아닌, 작업에서 학습하여 에이전트, 스킬, 문서를 지속적으로 개선합니다.

---

## 사용법

```bash
# 분석 및 개선 제안 (정리 없음)
/team-claude:cleanup task-id --analyze

# 분석 + 개선 적용 + 정리
/team-claude:cleanup task-id --improve

# 전체 완료된 작업 일괄 처리
/team-claude:cleanup --completed --improve

# 정리만 (기존 동작)
/team-claude:cleanup task-id

# 모든 것 정리
/team-claude:cleanup --all
```

## Arguments

| Argument | 설명 |
|----------|------|
| task-id | 특정 Task 대상 |
| --analyze | 분석 및 제안만 (정리 안함) |
| --improve | 분석 + 개선 적용 + 정리 |
| --completed | 완료/머지된 것만 대상 |
| --all | 모든 worktree 대상 |
| --dry-run | 실제 적용 없이 미리보기 |

---

## PHASE 1: 작업 분석 (Work Analysis)

### 분석 대상

완료된 작업에서 다음 항목을 분석합니다:

| 분석 항목 | 소스 | 추출 정보 |
|-----------|------|-----------|
| 커밋 히스토리 | git log | 작업 패턴, 변경 유형 |
| 변경된 파일 | git diff | 파일 유형, 모듈 분포 |
| PR 리뷰 | .team-claude/reviews/ | 리뷰 피드백, 반복 이슈 |
| 계획 문서 | .team-claude/plans/ | 요구사항, 계약 |
| 작업 로그 | .team-claude/state/ | 소요 시간, 반복 횟수 |
| Worker 피드백 | hooks 로그 | 병목, 에러 패턴 |

### 분석 실행

```bash
/team-claude:cleanup task-coupon-service --analyze
```

```
🔍 작업 분석: task-coupon-service

📊 작업 통계
  - 총 커밋: 23개
  - 변경 파일: 45개
  - 추가된 라인: 1,847
  - 삭제된 라인: 324
  - 작업 기간: 2시간 15분
  - 리뷰 반복: 3회

📁 변경 유형 분포
  - API/Controller: 35%
  - Service/Business: 28%
  - Repository/Data: 18%
  - Test: 15%
  - Config: 4%

🔄 반복 패턴 감지
  - 유효성 검증 로직: 8회 반복
  - 에러 핸들링 패턴: 6회 반복
  - API 응답 포맷팅: 5회 반복

⚠️ 리뷰에서 지적된 이슈
  - "null 체크 누락" - 3회
  - "로깅 추가 필요" - 2회
  - "테스트 케이스 부족" - 2회
```

---

## PHASE 2: 개선 제안 (Improvement Suggestions)

분석 결과를 바탕으로 시스템 개선을 제안합니다.

### 2.1 에이전트 제안

작업 패턴에서 새로운 에이전트 필요성을 감지합니다.

```
🤖 에이전트 제안

📌 신규 에이전트 추천

1. validation-specialist
   근거: 유효성 검증 로직이 8회 반복됨
   역할: 입력값 검증, DTO 유효성, 비즈니스 규칙 검증 전문

   제안 프롬프트:
   ┌─────────────────────────────────────────────────┐
   │ 당신은 유효성 검증 전문가입니다.              │
   │ 다음 원칙을 따릅니다:                         │
   │ - Fail-fast 원칙                              │
   │ - 명확한 에러 메시지                          │
   │ - 레이어별 검증 분리                          │
   └─────────────────────────────────────────────────┘

2. error-handling-expert
   근거: 에러 핸들링 패턴 6회 반복, 리뷰 피드백 반영
   역할: 예외 처리 전략, 에러 응답 표준화

📌 기존 에이전트 개선

1. code-reviewer 업데이트 제안
   추가할 체크리스트:
   - [ ] null 안전성 검사 (3회 지적됨)
   - [ ] 적절한 로깅 존재 여부 (2회 지적됨)

   overrides 설정:
   {
     "code-reviewer": {
       "additionalChecks": [
         "null-safety",
         "logging-coverage"
       ]
     }
   }
```

### 2.2 스킬 제안

반복 작업에서 자동화 가능한 스킬을 추천합니다.

```
⚡ 스킬 제안

📌 신규 스킬 추천

1. /generate-validation
   근거: DTO 유효성 검증 코드 반복 작성
   기능: DTO 클래스에서 자동으로 validation 로직 생성
   예상 절감: 작업당 15-20분

2. /api-response-wrapper
   근거: API 응답 포맷팅 5회 반복
   기능: 표준 응답 형식으로 자동 래핑

3. /add-logging
   근거: 로깅 추가 리뷰 피드백 반복
   기능: 메서드에 표준 로깅 자동 추가

📌 기존 스킬 활용 제안

1. /generate-tests 활용도 낮음
   현재 사용률: 작업의 20%
   제안: 테스트 부족 피드백 방지를 위해 적극 활용
```

### 2.3 문서 개선 제안

```
📚 문서 개선 제안

📌 신규 문서 추천

1. docs/validation-guide.md
   근거: 유효성 검증 패턴 반복
   내용: 프로젝트 유효성 검증 표준 가이드

2. docs/error-handling.md
   근거: 에러 핸들링 일관성 부족
   내용: 예외 처리 전략 및 표준 응답 형식

📌 기존 문서 업데이트

1. CONTRIBUTING.md 업데이트
   추가 내용:
   - 코드 리뷰 체크리스트 섹션
   - null 안전성 가이드라인

2. README.md 업데이트
   추가 내용:
   - API 응답 형식 설명
```

### 2.4 설정 최적화 제안

```
⚙️ 설정 최적화 제안

📌 config.json 업데이트 추천

{
  "agents": {
    "enabled": [
      "code-reviewer",
      "qa-agent",
      "validation-specialist",  // 추가 권장
      "error-handling-expert"   // 추가 권장
    ]
  },
  "planning": {
    "reviewers": {
      "mode": "multi",  // single → multi 권장
      "reason": "리뷰 반복 3회, 다중 관점 필요"
    }
  },
  "hooks": {
    "preCommit": {
      "nullCheck": true,  // 추가 권장
      "loggingCheck": true
    }
  }
}
```

---

## PHASE 3: 개선 적용 (Apply Improvements)

### 사용자 확인

```
🔧 개선 적용

적용할 개선 사항을 선택하세요:

에이전트:
  [x] validation-specialist 신규 생성
  [x] error-handling-expert 신규 생성
  [x] code-reviewer overrides 업데이트

스킬:
  [ ] /generate-validation 스킬 템플릿 생성
  [x] /add-logging 스킬 템플릿 생성

문서:
  [x] docs/validation-guide.md 생성
  [ ] docs/error-handling.md 생성
  [x] CONTRIBUTING.md 업데이트

설정:
  [x] config.json 업데이트

선택한 항목을 적용하시겠습니까? [Y/n]
```

### AskUserQuestion 활용

```markdown
## 개선 적용 확인

다음 중 적용할 항목을 선택해주세요:

### 에이전트
- [ ] validation-specialist 생성
- [ ] error-handling-expert 생성
- [ ] code-reviewer 설정 업데이트

### 스킬
- [ ] /generate-validation 템플릿
- [ ] /add-logging 템플릿

### 문서
- [ ] validation-guide.md
- [ ] CONTRIBUTING.md 업데이트

### 추가 옵션
- [ ] 모두 적용
- [ ] 분석 결과만 저장 (적용 안함)
```

### 적용 결과

```
✅ 개선 적용 완료

생성됨:
  📄 plugins/team-claude/agents/validation-specialist.md
  📄 plugins/team-claude/agents/error-handling-expert.md
  📄 plugins/team-claude/templates/skills/add-logging.md
  📄 docs/validation-guide.md

업데이트됨:
  📝 .team-claude/config.json
  📝 CONTRIBUTING.md
  📝 plugins/team-claude/agents/code-reviewer.md (overrides)

💾 분석 보고서 저장됨:
  .team-claude/retrospectives/task-coupon-service-20250118.md
```

---

## PHASE 4: 리소스 정리 (Cleanup)

### 정리 대상

| 리소스 | 설명 | 위치 |
|--------|------|------|
| Worktree | Git worktree 디렉토리 | ../worktrees/{task-id}/ |
| 브랜치 | Feature 브랜치 | feature/{task-id} |
| 상태 파일 | Worker 상태 기록 | .team-claude/state/ |
| 리뷰 파일 | 리뷰 결과 | .team-claude/reviews/{task-id}/ |

### 정리 실행

```
🧹 리소스 정리

정리할 리소스:
  📁 Worktree: ../worktrees/task-coupon-service
  🌿 브랜치: feature/task-coupon-service
  📋 상태 기록: .team-claude/state/task-coupon-service.json

보존할 리소스:
  📊 분석 보고서: .team-claude/retrospectives/
  📝 계획 문서: .team-claude/plans/ (archived)

계속하시겠습니까? [Y/n]
```

### 정리 결과

```
✅ task-coupon-service 정리 완료

정리됨:
  ✅ Worktree 제거됨
  ✅ 로컬 브랜치 삭제됨
  ✅ 원격 브랜치 삭제됨
  ✅ 상태 기록 아카이브됨

보존됨:
  📊 분석 보고서
  📝 계획 문서 (archived)
  🤖 생성된 에이전트
  ⚡ 생성된 스킬 템플릿

정리된 공간: 45MB
```

---

## 회고 보고서 (Retrospective Report)

### 저장 위치

```
.team-claude/retrospectives/
├── task-coupon-service-20250118.md
├── task-admin-ui-20250118.md
└── index.json
```

### 보고서 형식

```markdown
# Retrospective: task-coupon-service

## 작업 요약
- **기간**: 2025-01-18 10:00 ~ 12:15
- **커밋**: 23개
- **변경**: +1,847 / -324 lines

## 주요 성과
- 쿠폰 서비스 API 구현 완료
- 단위 테스트 85% 커버리지

## 발견된 패턴
1. 유효성 검증 로직 반복 (8회)
2. 에러 핸들링 패턴 반복 (6회)

## 리뷰 피드백 요약
- null 체크 누락 (3회)
- 로깅 추가 필요 (2회)

## 적용된 개선
- [x] validation-specialist 에이전트 생성
- [x] error-handling-expert 에이전트 생성
- [x] code-reviewer 체크리스트 업데이트

## 향후 권장사항
1. /generate-validation 스킬 활용
2. 다중 리뷰어 모드 고려
```

---

## 일괄 처리

### 완료된 모든 작업 분석 및 개선

```bash
/team-claude:cleanup --completed --improve
```

```
🔍 일괄 분석: 3개 작업

분석 중...
  ✅ task-coupon-service
  ✅ task-admin-ui
  ✅ task-coupon-repository

📊 통합 분석 결과

공통 패턴:
  - 유효성 검증: 18회 (3개 작업 합산)
  - 에러 핸들링: 12회
  - API 응답 포맷: 9회

공통 리뷰 피드백:
  - null 안전성: 7회
  - 테스트 커버리지: 5회

🤖 통합 에이전트 제안: 2개
⚡ 통합 스킬 제안: 3개
📚 통합 문서 제안: 2개

개선 사항을 적용하시겠습니까? [Y/n]
```

---

## Dry-run 모드

```bash
/team-claude:cleanup task-id --improve --dry-run
```

```
🔍 Dry-run 모드 (실제 적용 안함)

분석 결과:
  🤖 에이전트 2개 생성 예정
  ⚡ 스킬 1개 생성 예정
  📚 문서 2개 생성/업데이트 예정

정리 예정:
  📁 ../worktrees/task-coupon-service (45MB)
  🌿 feature/task-coupon-service

실제 적용: /team-claude:cleanup task-id --improve
```

---

## 분석 알고리즘

### 패턴 감지

```python
# 의사 코드
def detect_patterns(commits, reviews):
    patterns = {}

    # 1. 코드 유사도 분석
    for commit in commits:
        similar_blocks = find_similar_code_blocks(commit.diff)
        for block in similar_blocks:
            patterns[block.type] += 1

    # 2. 리뷰 피드백 분류
    for review in reviews:
        for comment in review.comments:
            category = classify_comment(comment)
            patterns[category] += 1

    # 3. 임계값 이상인 패턴 추출
    return {k: v for k, v in patterns.items() if v >= THRESHOLD}
```

### 에이전트 매칭

```python
# 패턴 → 에이전트 매핑
PATTERN_AGENT_MAP = {
    "validation": "validation-specialist",
    "error-handling": "error-handling-expert",
    "security": "security-auditor",
    "performance": "performance-optimizer",
    "testing": "qa-agent",
}

def suggest_agents(patterns):
    suggestions = []
    for pattern, count in patterns.items():
        if pattern in PATTERN_AGENT_MAP:
            agent = PATTERN_AGENT_MAP[pattern]
            if not agent_exists(agent):
                suggestions.append({
                    "agent": agent,
                    "reason": f"{pattern} 패턴 {count}회 감지",
                    "priority": count
                })
    return sorted(suggestions, key=lambda x: -x["priority"])
```

---

## 설정

### config.json 옵션

```json
{
  "cleanup": {
    "autoAnalyze": true,
    "suggestImprovements": true,
    "autoApply": false,
    "keepRetrospectives": true,
    "patternThreshold": 3,
    "retrospectivePath": ".team-claude/retrospectives/"
  }
}
```

| 옵션 | 기본값 | 설명 |
|------|--------|------|
| autoAnalyze | true | 정리 시 자동 분석 수행 |
| suggestImprovements | true | 개선 제안 표시 |
| autoApply | false | 개선 자동 적용 (확인 없이) |
| keepRetrospectives | true | 회고 보고서 보존 |
| patternThreshold | 3 | 패턴 감지 최소 횟수 |

---

## 에러 처리

### 분석 실패

```
⚠️ 분석 부분 실패

성공:
  ✅ 커밋 히스토리 분석
  ✅ 파일 변경 분석

실패:
  ❌ 리뷰 파일 없음 (.team-claude/reviews/)
  ❌ 계획 문서 없음 (.team-claude/plans/)

부분 결과로 계속하시겠습니까? [Y/n]
```

### 개선 적용 실패

```
⚠️ 일부 개선 적용 실패

성공:
  ✅ validation-specialist.md 생성
  ✅ config.json 업데이트

실패:
  ❌ CONTRIBUTING.md 업데이트 실패 (파일 없음)

실패한 항목은 수동으로 처리해주세요.
```

---

## 복구

### 적용된 개선 롤백

```bash
# 생성된 파일 확인
ls .team-claude/retrospectives/task-coupon-service-20250118.md

# 보고서에서 생성된 파일 목록 확인 후 수동 삭제
```

### Worktree 복구

```bash
# reflog에서 브랜치 복구
git reflog
git checkout -b feature/task-coupon-service abc1234

# Worktree 재생성
git worktree add ../worktrees/task-coupon-service feature/task-coupon-service
```

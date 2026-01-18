---
name: review
description: Worker의 작업 결과를 리뷰합니다 - Git diff를 분석하고 피드백을 생성합니다
argument-hint: "<feature-name>"
allowed-tools: ["Bash", "Read", "Glob", "Task"]
---

# Worker Review 커맨드

완료된 Worker의 작업 결과를 리뷰하고 피드백을 생성합니다.

## 핵심 워크플로우

```
1. Worker 상태 확인 (pending_review 상태인지)
    │
    ▼
2. Git Diff 분석
    ├── 변경 파일 목록
    ├── 라인 변경 통계
    └── 주요 변경 내용
    │
    ▼
3. 코드 리뷰 수행
    └── code-reviewer 에이전트 활용
    │
    ▼
4. 리뷰 결과 정리
    │
    ▼
5. 피드백 전달 (선택)
```

## 실행 단계

### 1. Worker 상태 확인

```bash
curl -s http://localhost:3847/status/<feature-name> | jq
```

### 2. Git Diff 조회

```bash
# Coordination Server를 통해 diff 조회
curl -s http://localhost:3847/diff/<worktree-name> | jq

# 또는 직접 조회
cd ../worktrees/feature-<name> && git diff origin/main...HEAD
```

### 3. 코드 리뷰 수행

`code-reviewer` 에이전트를 사용하여 변경사항을 분석합니다:

```
Task(subagent_type="code-reviewer", prompt="...")
```

### 4. 리뷰 결과 형식

```markdown
# Code Review: <feature-name>

## 변경 요약
- 파일 수: N개
- 추가: +X lines
- 삭제: -Y lines

## 리뷰 의견

### 좋은 점
- [칭찬할 점 1]
- [칭찬할 점 2]

### 개선 필요
- [CRITICAL] 파일.ts:123 - 보안 취약점
- [SUGGESTION] 파일.ts:456 - 성능 개선 제안

### 질문/확인 필요
- 파일.ts:789 - 이 로직의 의도가 무엇인가요?

## 결론
- [ ] 승인 (Approve)
- [x] 수정 요청 (Request Changes)
- [ ] 논의 필요 (Discuss)

## 다음 단계
1. [수정 요청 사항 1]
2. [수정 요청 사항 2]
```

## 사용 예시

```bash
# 특정 Worker 리뷰
/team-claude:review auth-feature

# 모든 pending_review Worker 리뷰
/team-claude:review --all

# 상세 diff와 함께 리뷰
/team-claude:review payment --verbose
```

## 리뷰 항목

### 필수 체크 항목

1. **기능 완성도**
   - Task spec의 모든 항목 구현 여부
   - 엣지 케이스 처리

2. **코드 품질**
   - 네이밍 컨벤션
   - 코드 구조
   - 중복 코드

3. **보안**
   - 입력 검증
   - 인증/인가 처리
   - 민감 정보 노출

4. **테스트**
   - 테스트 커버리지
   - 테스트 품질

5. **성능**
   - 불필요한 연산
   - N+1 쿼리
   - 메모리 사용

### 리뷰 심각도

| 레벨 | 설명 | 액션 |
|------|------|------|
| CRITICAL | 반드시 수정 필요 | 수정 전 머지 불가 |
| MAJOR | 중요한 개선 필요 | 수정 권장 |
| MINOR | 사소한 개선 제안 | 선택적 수정 |
| SUGGESTION | 제안 사항 | 참고용 |

## 리뷰 후 액션

리뷰 완료 후 `/team-claude:feedback` 커맨드로 Worker에게 피드백을 전달합니다:

```bash
# 수정 요청
/team-claude:feedback auth-feature --action revise "보안 취약점 수정 필요"

# 승인
/team-claude:feedback auth-feature --action complete "LGTM"
```

## 관련 커맨드

- `/team-claude:status` - Worker 상태 확인
- `/team-claude:feedback` - 피드백 전달
- `/team-claude:spawn` - 새 Worker 생성

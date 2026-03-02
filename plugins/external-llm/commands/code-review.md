---
description: Claude, Codex, Gemini 3개 LLM으로 코드 변경사항을 리뷰합니다
argument-hint: "[scope] [관점]"
allowed-tools: ["Task"]
---

# 코드 리뷰 커맨드 (/code-review)

Claude, OpenAI Codex, Google Gemini 3개 LLM을 사용하여 코드 변경사항을 종합적으로 리뷰합니다.

## 사용법

```bash
# 기본 (uncommitted 변경사항)
/code-review

# scope 지정
/code-review staged
/code-review pr
/code-review "branch main"

# 관점 지정
/code-review "security 관점으로 리뷰해줘"
/code-review "staged performance 관점"

# 복합
/code-review "pr 보안과 성능 관점에서 리뷰해줘"
```

## 핵심 워크플로우

**토큰 최적화**: MainAgent는 파일 내용/diff를 절대 읽지 않음. 경로만 수집하여 전달.

```
1. diff-collector 에이전트로 diff 파일 생성 (foreground)
2. diff 파일 경로만 수신
3. 3개 리뷰 에이전트에 경로 전달 (병렬)
4. 결과 취합 → 컨센서스 리포트
```

## 작업 프로세스

### Step 1: 사용자 요청 파싱

사용자 요청에서 추출:
- **scope**: uncommitted (기본), staged, pr, branch
- **target**: branch scope일 때 base 브랜치명
- **관점**: security, performance, architecture 등 (기본: 일반 코드 리뷰)

파싱 규칙:
- "staged" → scope=staged
- "pr" → scope=pr
- "branch main" / "branch develop" → scope=branch, target=main/develop
- 그 외 → scope=uncommitted
- "security" / "보안" → 관점=security
- "performance" / "성능" → 관점=performance
- "architecture" / "아키텍처" / "설계" → 관점=architecture

### Step 2: diff-collector 에이전트 실행 (foreground)

diff 수집을 에이전트에 위임합니다:

```
Task(subagent_type="diff-collector", prompt="scope: [scope], target: [target]", run_in_background=false)
```

에이전트가 `get-diff.sh` 실행 → diff 파일 경로만 반환.

**에러 시**: 변경사항이 없으면 에이전트가 에러 메시지를 반환합니다. 사용자에게 안내 후 종료:
```
변경사항이 없습니다. 다른 scope를 지정해보세요:
- /code-review staged
- /code-review pr
- /code-review "branch main"
```

### Step 3: 3개 리뷰 에이전트 병렬 실행

diff 파일 경로를 3개 에이전트에 병렬 전달합니다:

**Claude 에이전트** (`code-reviewer-claude`):
```
코드 리뷰를 수행해주세요.

Diff 파일: [diff 파일 경로]

관점: [관점]

사용자 요청:
[원래 요청]

위 diff 파일을 Read하여 코드 변경사항을 리뷰해주세요.
```

**Codex 에이전트** (`code-reviewer-codex`):
```
코드 리뷰를 수행해주세요.

Scope: [scope]
Target: [target]

관점: [관점]

사용자 요청:
[원래 요청]

call-codex-review.sh를 사용하여 코드 리뷰를 수행해주세요.
```

**Gemini 에이전트** (`code-reviewer-gemini`):
```
코드 리뷰를 수행해주세요.

대상 파일:
- [diff 파일의 상대 경로]

관점: [관점]

사용자 요청:
[원래 요청]

위 diff 파일의 코드 변경사항을 리뷰해주세요.
```

3개 모두 `run_in_background=true`로 실행합니다.

### Step 4: 결과 취합 및 컨센서스 리포트

3개 결과를 비교 분석합니다.

## 종합 리포트 구조

```markdown
# 3개 LLM 코드 리뷰 결과

## 요약

- **Scope**: [scope]
- **Claude 점수**: XX/100
- **Codex 점수**: XX/100
- **Gemini 점수**: XX/100
- **평균 점수**: XX/100

## 공통 강점 (3개 LLM 모두 동의)

1. [3개 모두 언급한 강점]

## 공통 약점 (Critical - 3개 LLM 모두 지적)

1. [3개 모두 지적한 문제]
   - **신뢰도**: 높음 (3/3 LLM 동의)

## 관점 차이

### Claude의 독특한 지적
### Codex의 독특한 지적
### Gemini의 독특한 지적

## 종합 권장사항

### Critical (3개 LLM 합의)
### Important (2개 이상 LLM 언급)
### 참고사항 (1개 LLM만 언급)
```

## 컨센서스 분석 기준

- **3/3 합의**: 높은 신뢰도 → 반드시 반영
- **2/3 동의**: 중간 신뢰도 → 사용자에게 제시
- **1/3 지적**: 참고 사항 → 정보 제공

## 주의사항

- **API 필요**: Codex, Gemini CLI 설치 필요
- **토큰 최적화**: MainAgent는 diff 파일 경로만 수집, SubAgent가 실제 내용 읽기
- **Codex/Gemini 불가 시**: 해당 LLM 결과는 "N/A"로 표시, 가용한 LLM으로만 리포트 생성

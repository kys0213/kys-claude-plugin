---
description: 현재 레포를 자율 개발 모니터링 대상으로 등록합니다
allowed-tools: ["AskUserQuestion", "Bash"]
---

# 자율 개발 모니터링 설정 위자드 (/auto-setup)

> **이 플러그인은 반드시 User Scope로 설치**해야 합니다.
> 사용자가 모니터링하려는 레포 디렉토리에서 이 커맨드를 호출합니다.

## 실행 흐름

### Step 1: 레포 감지

현재 디렉토리의 git remote에서 레포 URL을 자동 감지합니다:

```bash
git remote get-url origin
```

감지된 URL을 사용자에게 확인합니다.

### Step 2: 의존성 검증

다음 플러그인이 `kys-claude-plugin` 마켓플레이스에서 User Scope로 설치되어 있는지 확인하세요:

| 구분 | 플러그인 | 마켓플레이스 |
|------|---------|-------------|
| 필수 | `develop-workflow` | `kys-claude-plugin` |
| 필수 | `git-utils` | `kys-claude-plugin` |
| 권장 | `external-llm` | `kys-claude-plugin` |

미설치 시 안내:
- 필수 → 경고 + `/plugin install <name>@kys-claude-plugin`
- 권장 → multi-LLM 분석이 Claude 단일 모델로 fallback됨을 안내

설치 확인이 완료되지 않으면 다음 단계로 진행하지 마세요.

### Step 3: 감시 대상 설정

AskUserQuestion으로 질문:
- 감시 대상: Issues / Pull Requests / 둘 다

### Step 4: 스캔 주기

AskUserQuestion으로 질문:
- 1분 (빠른 반응)
- 5분 (권장)
- 15분 (저부하)
- 직접 입력

### Step 5: Consumer 처리량

AskUserQuestion으로 질문:
- Issue Consumer 동시 처리 수 (1~3)
- PR Consumer 동시 처리 수 (1~3)
- Merge Consumer 동시 처리 수 (1~2)

### Step 6: 워크플로우 선택

AskUserQuestion으로 질문:
- Issue 분석: multi-LLM (Claude+Codex+Gemini) / 단일 모델
- PR 리뷰: /multi-review / 단일 모델

### Step 7: 필터 설정

AskUserQuestion으로 질문:
- 전체 이슈/PR 감시
- 특정 라벨만 (라벨명 입력)
- 특정 작성자 제외 (기본: dependabot, renovate)

### Step 8: 등록

수집된 설정을 JSON으로 구성하여 CLI에 전달:

```bash
autonomous repo add <url> --config '<json>'
```

### Step 9: 셸 환경 등록

최초 설정 시 셸 프로필에 환경변수와 alias를 등록할지 확인:

```bash
# ~/.bashrc 또는 ~/.zshrc에 추가
export AUTONOMOUS_HOME="$HOME/.autonomous"
export PATH="$HOME/.local/bin:$PATH"

alias auto="autonomous"
alias auto-s="autonomous status"
alias auto-d="autonomous dashboard"
alias auto-q="autonomous queue list"
```

### Step 10: 설정 요약

최종 설정을 사용자에게 보기 좋게 출력합니다.

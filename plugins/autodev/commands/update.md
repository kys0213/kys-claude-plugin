---
description: autodev 업그레이드 후 설정 마이그레이션 및 환경 점검
allowed-tools: ["Bash", "Glob", "Grep", "Read", "Edit", "AskUserQuestion"]
---

# 업그레이드 점검 (/update)

autodev를 업그레이드한 뒤 설정 파일, CLI 버전, 플러그인 의존성을 점검하고 필요한 마이그레이션을 수행합니다.

## 실행 흐름

### Step 1: CLI 바이너리 버전 확인

플러그인의 `ensure-binary.sh`로 CLI 바이너리가 최신인지 확인합니다:

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
```

- 설치 버전 < 플러그인 버전 → 재빌드 안내
- 설치 버전 >= 플러그인 버전 → OK

### Step 2: 설정 파일 마이그레이션 점검

#### 2-1. 글로벌 설정 점검

`~/.develop-workflow.yaml`이 있으면 읽어서 deprecated 키를 점검합니다.

#### 2-2. 등록된 레포별 설정 점검

```bash
autodev repo list
```

등록된 레포마다 워크스페이스 설정 파일을 점검합니다:

```bash
# 각 레포의 워크스페이스 경로
ls ~/.autodev/workspaces/*/. develop-workflow.yaml 2>/dev/null
```

#### 2-3. 마이그레이션 규칙 적용

아래 규칙을 **순서대로** 점검합니다. 해당 사항이 없으면 건너뜁니다.

##### Migration 1: `consumer:` → `sources.github:` (v0.8.1 → v0.9.0)

YAML 파일에서 최상위 `consumer:` 키가 있으면 `sources.github:`로 변환해야 합니다.

**변경 전:**
```yaml
consumer:
  scan_interval_secs: 300
  gh_host: "git.example.com"
  model: opus
```

**변경 후:**
```yaml
sources:
  github:
    scan_interval_secs: 300
    gh_host: "git.example.com"
    model: opus
```

**점검 방법:**

각 YAML 파일을 Read 도구로 읽고, `consumer:` 최상위 키가 존재하는지 확인합니다.

**발견 시:**

1. 사용자에게 변경 내용을 명확히 보여줍니다
2. AskUserQuestion으로 자동 변환 여부를 확인합니다
3. 승인 시 Edit 도구로 YAML 파일을 직접 수정합니다:
   - `consumer:` → `sources:\n  github:` 로 키 변경
   - 하위 필드의 들여쓰기를 2칸 추가

### Step 3: 플러그인 의존성 점검

필수 플러그인이 설치되어 있는지 확인합니다:

```bash
ls ~/.claude/plugins/cache/*/commands/*.md 2>/dev/null
```

| 구분 | 플러그인 | 확인 대상 |
|------|---------|----------|
| 필수 | `develop-workflow` | `multi-review.md`, `develop-auto.md` 존재 여부 |
| 필수 | `git-utils` | `commit-and-pr.md` 존재 여부 |
| 권장 | `external-llm` | `invoke-codex.md`, `invoke-gemini.md` 존재 여부 |

미설치 플러그인이 있으면 설치 명령어를 안내합니다.

### Step 4: 결과 리포트

점검 결과를 테이블로 요약합니다:

```
## 점검 결과

| 항목                    | 상태 | 비고                         |
|------------------------|------|------------------------------|
| CLI 바이너리 버전       | OK   | v0.9.0                       |
| 글로벌 설정 마이그레이션 | OK   | consumer → sources.github 완료 |
| 레포별 설정 마이그레이션 | OK   | org/repo 변환 완료            |
| develop-workflow 플러그인| OK   | 설치됨                       |
| git-utils 플러그인      | OK   | 설치됨                       |
| external-llm 플러그인   | WARN | 미설치 (multi-LLM 사용 불가)  |
```

모든 항목이 OK면 "업그레이드 점검 완료" 메시지를 출력합니다.

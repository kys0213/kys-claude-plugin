# Config Schema v2: `.autodev.yaml`

> **Date**: 2026-03-02
> **Status**: Draft
> **Closes**: #162
> **Base**: [DESIGN-v3.md](./DESIGN-v3.md) — Issue-PR Workflow (Analysis Review + Label-Positive)

---

## 1. 변경 동기

### v1의 문제

- dead config가 live config보다 많음 — `commands` 섹션(6개), `develop.implement`(3개), `develop.pr`(1개) 등 선언만 있고 미사용
- `workflow.issue` / `workflow.pr`은 단순 문자열로, 커스터마이즈 여지가 없음
- `develop.review.max_iterations`만 유일하게 사용되나 위치가 직관적이지 않음
- autodev 파이프라인 단계(analyze, implement, review)가 스키마에 명시적으로 드러나지 않음

### v2 목표

1. **dead config 제거** — 사용되지 않는 필드를 모두 정리
2. **파이프라인 단계를 스키마에 명시** — `workflows` 섹션으로 analyze, implement, review를 1급 개념으로 표현
3. **계층화된 오버라이드** — 글로벌 기본값 + 레포별 오버라이드 (deep merge)
4. **OCP 확장성** — 새로운 task source나 workflow stage 추가 시 기존 스키마 수정 없이 확장 가능

---

## 2. 스키마 구조

### 전체 구조

```yaml
daemon:       # 데몬 루프 설정 (source 무관)
sources:      # task source별 스캔/연결 설정
workflows:    # autodev 파이프라인 단계별 실행 방식
```

세 섹션은 각각 독립된 관심사를 가진다:

| 섹션 | 관심사 | 변경 빈도 |
|------|--------|----------|
| `daemon` | 데몬 프로세스 운영 | 거의 변경 없음 |
| `sources` | 어디서 task를 수집할지 | source 추가 시 |
| `workflows` | 어떻게 task를 처리할지 | 레포별 커스터마이즈 |

### 계층 위계

`.claude/` 설정과 동일한 패턴으로, 글로벌 기본값 위에 레포별 오버라이드를 deep merge한다:

```
~/.autodev.yaml              ← 글로벌 기본값
  └── <repo>/.autodev.yaml   ← 레포별 오버라이드 (deep merge)
```

레포 설정에서 명시한 필드만 글로벌 값을 덮어쓴다. 명시하지 않은 필드는 글로벌 → default 순으로 적용된다.

---

## 3. `daemon` 섹션

데몬 프로세스의 전역 운영 설정. 모든 source에 공통으로 적용된다.

```yaml
daemon:
  tick_interval_secs: 10        # 메인 루프 주기 (초)
  daily_report_hour: 6          # 일간 리포트 생성 시각 (0-23)
  log_dir: logs                 # 로그 디렉토리 (상대경로: ~/.autodev/ 기준)
  log_level: info               # trace | debug | info | warn | error
  log_retention_days: 30        # 로그 보관 기간 (일)
  max_concurrent_tasks: 3       # 전체 동시 실행 파이프라인 상한
```

---

## 4. `sources` 섹션

task를 수집하는 외부 시스템별 설정. 각 source는 독립된 하위 키를 가진다.

### `sources.github`

```yaml
sources:
  github:
    # ── 스캔 ──
    scan_interval_secs: 300       # 스캔 주기 (초)
    scan_targets:                 # 스캔 대상
      - issues
      - pulls
    issue_concurrency: 1          # 이슈 동시 처리 수
    pr_concurrency: 1             # PR 동시 처리 수
    filter_labels: null           # 특정 라벨만 필터 (null = 전체)
    ignore_authors:               # 무시할 작성자
      - dependabot
      - renovate
    gh_host: null                 # GitHub Enterprise host (null = github.com)

    # ── 처리 ──
    model: sonnet                 # LLM 모델
    workspace_strategy: worktree  # 작업 공간 전략
    confidence_threshold: 0.7     # 분석 신뢰도 임계값
    knowledge_extraction: true    # 지식 추출 활성화
```

### Source 확장 (OCP)

새로운 task source(예: Jira, Linear)를 추가할 때는 `sources` 하위에 새 키를 추가한다.
기존 `sources.github` 설정이나 코드는 수정하지 않는다.

```yaml
# 향후 확장 예시
sources:
  github:
    # ... (기존 그대로)
  jira:
    base_url: https://company.atlassian.net
    project_key: DEV
    poll_interval_secs: 600
```

---

## 5. `workflows` 섹션

autodev 파이프라인의 각 단계별 실행 방식을 정의한다.

### 파이프라인 단계

```
analyze → implement → review
   │          │          │
   │          │          └─ PR 코드 리뷰 + 피드백 반영 루프
   │          └─ 분석 결과 기반 코드 구현 + PR 생성
   └─ 이슈 분석 + 리포트 게시
```

### 스키마

```yaml
workflows:
  analyze:
    command: null                        # 커스텀 슬래시 커맨드 (선택, 미지정 시 builtin)
  implement:
    command: null                        # 커스텀 슬래시 커맨드 (선택, 미지정 시 builtin)
  review:
    command: null                        # 커스텀 슬래시 커맨드 (선택, 미지정 시 builtin)
    max_iterations: 2                   # 리뷰-피드백 반영 최대 횟수
```

> **Note**: v2 초기 스키마에 있던 `agent` 필드는 제거되었다. YAML에 `agent` 키가 남아 있어도 파싱 시 무시된다 (`deny_unknown_fields` 미적용).

### 커스터마이즈

각 단계는 `command` 필드로 커스텀 슬래시 커맨드를 지정할 수 있다. 미지정 시 task_type별 기본 출력 스펙이 system prompt로 사용된다.

| 필드 | 의미 | 예시 |
|------|------|------|
| `command` | 커스텀 slash command 실행 | `/develop-workflow:multi-review` |

```yaml
# 예: 특정 레포에서 리뷰만 커스텀 slash command로 실행
workflows:
  review:
    command: /develop-workflow:multi-review
    max_iterations: 3
```

### Workflow 확장 (OCP)

새로운 파이프라인 단계(예: extract, deploy)를 추가할 때는 `workflows` 하위에 새 키를 추가한다.
기존 단계의 설정이나 코드는 수정하지 않는다.

```yaml
# 향후 확장 예시
workflows:
  analyze:    # 기존
  implement:  # 기존
  review:     # 기존
  extract:                                # 신규 단계
    command: /custom-extract
```

---

## 6. 레포별 오버라이드 예시

### 글로벌 설정 (`~/.autodev.yaml`)

```yaml
daemon:
  log_level: info
  max_concurrent_tasks: 3

sources:
  github:
    model: sonnet
    ignore_authors: [dependabot, renovate]

workflows:
  review:
    max_iterations: 2
```

### Frontend 레포 오버라이드 (`frontend-app/.autodev.yaml`)

```yaml
sources:
  github:
    model: opus           # 프론트엔드는 더 높은 모델 사용

workflows:
  review:
    max_iterations: 3     # UI 리뷰는 반복 횟수를 늘림
```

### OSS 레포 오버라이드 (`oss-lib/.autodev.yaml`)

```yaml
workflows:
  analyze:
    command: /external-llm:analyze              # 커스텀 분석 사용
  review:
    command: /develop-workflow:multi-review    # 커스텀 리뷰 사용
```

---

## 7. v1 → v2 마이그레이션

### 제거 대상

| v1 섹션 | 필드 수 | 사유 |
|---------|---------|------|
| `commands.*` | 6 | 전체 미사용 |
| `develop.implement.*` | 3 | 전체 미사용 |
| `develop.pr.*` | 1 | 전체 미사용 |
| `develop.review.multi_llm` | 1 | 미사용 |
| `develop.review.auto_feedback` | 1 | 미사용 |

### 이동 대상

| v1 | v2 |
|----|-----|
| `workflow.issue` | `workflows.implement` |
| `workflow.pr` | `workflows.review` |
| `develop.review.max_iterations` | `workflows.review.max_iterations` |
| (하드코딩) | `workflows.analyze` |

### 호환성

- v1 YAML에 `commands`, `develop`, `workflow` 키가 있으면 파싱 시 warn 로그 + default fallback
- `/update` 커맨드에서 구버전 키 감지 시 마이그레이션 안내 출력

---

## 8. 설계 원칙 요약

| 원칙 | 적용 |
|------|------|
| **SRP** | `daemon`, `sources`, `workflows`는 각각 독립된 관심사 |
| **OCP** | 새 source/stage 추가 시 기존 스키마 수정 없이 하위 키 추가 |
| **DRY** | 글로벌 기본값 + 레포별 오버라이드 = deep merge로 중복 제거 |
| **Explicit > Implicit** | 파이프라인 3단계를 스키마에 명시적으로 표현 |

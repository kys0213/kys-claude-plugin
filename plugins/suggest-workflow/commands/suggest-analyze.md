---
description: 세션 분석 기반 통합 워크플로우/스킬 제안 (Multi-query BM25)
---

# Suggest Analyze

프로젝트 또는 전체 세션 히스토리를 통합 분석하여 워크플로우 패턴과 암묵지를 발견합니다.
Multi-query BM25 기반으로 장문 프롬프트도 정밀하게 스코어링합니다.

## 사용법

```bash
# 바이너리가 없으면 자동 다운로드/빌드
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh

# 현재 프로젝트 통합 분석 (기본)
${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow \
  --project "$(pwd)"

# 전체 프로젝트 글로벌 분석
${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow \
  --scope global

# 깊이 조절
${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow \
  --project "$(pwd)" \
  --depth wide

# 특정 분석만
${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow \
  --project "$(pwd)" \
  --focus skill
```

## 옵션

| 옵션 | 값 | 기본값 | 설명 |
|------|----|--------|------|
| `--scope` | `project`, `global` | `project` | 분석 범위 |
| `--depth` | `narrow`, `normal`, `wide` | `normal` | 탐색 깊이 |
| `--focus` | `all`, `workflow`, `skill` | `all` | 분석 대상 |
| `--project` | PATH | cwd | 프로젝트 경로 |
| `--threshold` | N | 3 | 최소 반복 횟수 |
| `--top` | N | 10 | 상위 N개 결과 |
| `--format` | `text`, `json` | `text` | 출력 형식 |
| `--decay` | flag | off | 시간 감쇠 가중치 |

## 탐색 깊이 (--depth)

| depth | 문장분해 | IDF 추출 | 명사쿼리 | 서브쿼리 수 | 유사도 | 전략 |
|-------|---------|---------|---------|-----------|--------|------|
| `narrow` | 12토큰↑ | top 3 | off | max 2 | 0.8 | Max |
| `normal` | 8토큰↑ | top 5 | on | max 4 | 0.7 | WeightedAvg |
| `wide` | 5토큰↑ | top 8 | on | max 8 | 0.5 | Avg |

### 활용 팁

- `narrow`: 빠른 탐색, 확실한 패턴만. 첫 분석에 적합
- `normal`: 균형잡힌 기본값. 대부분의 경우 충분
- `wide`: 숨겨진 패턴까지 발견. narrow 결과가 부족할 때

## 분석 범위 (--scope)

### project (기본)
현재 프로젝트의 세션만 분석합니다.

### global
`~/.claude/projects/` 아래 모든 프로젝트를 순회하며 분석합니다.
프로젝트를 넘나들며 반복되는 패턴이 진정한 개인 습관입니다.

## 분석 대상 (--focus)

### all (기본)
워크플로우 + 암묵지 모두 분석합니다.

### workflow
- 프롬프트 빈도 분석
- 도구 사용 시퀀스 패턴
- 도구 사용 통계

### skill
- 암묵지 패턴 (directive, convention, preference, correction)
- Multi-query BM25 스코어링
- 클러스터링 기반 유사 패턴 그룹화

## 출력 예시

```
=== Global Analysis (12 projects, 847 prompts) ===
Depth: normal | Multi-query: WeightedAvg

--- Workflow Analysis ---

Total prompts: 847 | Unique: 412
Top Prompts:
  1. [23x] 한국어로 응답해줘
  2. [12x] conventional commit으로 커밋해줘

--- Tacit Knowledge Analysis ---

Detected patterns: 8

#    Pattern                        Type         Count    Confidence
----------------------------------------------------------------------
1    한국어로 응답                    preference   23       94%
2    conventional commit 사용        convention   12       87%
3    타입 명시                        directive    8        82%

--- Project Breakdown ---

  kys-claude-plugin: 89 prompts, 14 sessions
  my-api-server: 67 prompts, 8 sessions
```

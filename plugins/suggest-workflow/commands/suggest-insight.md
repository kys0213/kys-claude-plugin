---
description: 구조적 통계 캐시 기반 Claude 시맨틱 인사이트 분석
---

# Suggest Insight

Rust CLI가 추출한 구조적 통계(캐시)를 기반으로 Claude가 시맨틱 해석을 수행합니다.
Rust는 **순수 연산만** 수행하고, **의미 해석과 분류는 LLM이** 담당합니다.

## 2-Phase 아키텍처

```
Phase 1: Rust CLI (구조적 통계 추출 → 캐시, 룰 0개)
    ↓ index.json + session summaries + analysis-snapshot.json
Phase 2: Claude (통계 해석 + 분류 + 인사이트)  ← 이 커맨드
```

## 사용법

```bash
# 1. 캐시 생성 (자동 실행)
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
CACHE_DIR=$(${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow \
  --project "$(pwd)" --cache)

# 2. 캐시 기반 시맨틱 분석
# workflow-insight 에이전트가 캐시를 읽고 분석합니다.
```

## 캐시 구조

```
~/.claude/suggest-workflow-cache/{project-encoded}/
  ├── index.json                    # 세션 메타 인덱스
  ├── sessions/
  │   ├── {session-id}.summary.json # 세션별 통계 요약
  │   └── ...
  └── analysis-snapshot.json        # 전체 분석 스냅샷 (8개 분석 축)
```

### analysis-snapshot.json 분석 축

| 분석 | 키 | 설명 |
|------|-----|------|
| 프롬프트 빈도 | `promptAnalysis` | Top 반복 프롬프트 |
| 도구 시퀀스 | `workflowAnalysis` | 공통 시퀀스 패턴 |
| 프롬프트 클러스터 | `tacitKnowledge` | BM25 클러스터 (type=cluster, LLM이 분류) |
| **도구 전이 행렬** | `toolTransitions` | A→B 전이 확률 |
| **반복/이상치** | `repetitionStats` | 파일 편집 이상치 (mean±σ), 루프 감지 |
| **주간 트렌드** | `weeklyTrends` | 도구별 주간 카운트 + 선형 회귀 기울기 |
| **파일 핫스팟** | `fileAnalysis` | 파일별 편집 횟수, co-change 그룹 |
| **세션 연결** | `sessionLinks` | 파일 겹침 Jaccard + 시간 근접 |

### 세션 요약 필드 (summary.json)

| 필드 | 설명 |
|------|------|
| `prompts[]` | 사용자 프롬프트 + 타임스탬프 |
| `toolUseCount` | 총 도구 사용 횟수 |
| `toolSequences[]` | 도구 시퀀스 문자열 (예: `"Read → Edit → Bash:test"`) |
| `filesMutated[]` | 수정된 파일 경로 |
| `stats` | 순수 정량 통계 (프롬프트 수, 도구 수, 전이 횟수, 파일 편집 수 등) |

## 캐시 전략

캐시는 매 실행 시 **전체 재생성**됩니다.
코드 버전에 따라 분석 로직이 달라질 수 있으므로, 항상 최신 결과를 보장합니다.

## 옵션

| 옵션 | 값 | 기본값 | 설명 |
|------|----|--------|------|
| `--project` | PATH | cwd | 프로젝트 경로 |
| `--depth` | `narrow`, `normal`, `wide` | `normal` | 클러스터링 깊이 |
| `--threshold` | N | 3 | 최소 반복 횟수 |
| `--top` | N | 10 | 상위 N개 결과 |
| `--decay` | flag | off | 시간 감쇠 가중치 |
| `--exclude-words` | WORD,WORD,... | - | 분석에서 제외할 노이즈 단어 (쉼표 구분) |

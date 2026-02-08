---
description: 정적 분석 캐시 기반 Claude 시맨틱 인사이트 분석
---

# Suggest Insight

정적 분석 결과(캐시)를 기반으로 Claude가 세션을 선별적으로 심층 분석합니다.
단순 빈도 통계를 넘어 **의도 파악, 습관 변화 추적, 모순 감지** 등 시맨틱 분석을 수행합니다.

## 2-Phase 아키텍처

```
Phase 1: Rust CLI (빠른 정적 분석 → 캐시 생성)
    ↓ index.json + session summaries + analysis-snapshot.json
Phase 2: Claude (선별된 세션만 시맨틱 분석)  ← 이 커맨드
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
  │   ├── {session-id}.summary.json # 세션별 요약 (프롬프트, 도구, 시그널)
  │   └── ...
  └── analysis-snapshot.json        # 정적 분석 스냅샷
```

### index.json 필드

| 필드 | 설명 |
|------|------|
| `project` | 프로젝트 경로 |
| `lastUpdated` | 마지막 캐시 갱신 시각 |
| `totalPrompts` | 전체 프롬프트 수 |
| `totalSessions` | 전체 세션 수 |
| `sessions[]` | 세션별 메타데이터 배열 |

### 세션 메타데이터 필드

| 필드 | 설명 |
|------|------|
| `id` | 세션 ID |
| `fileSize` | 원본 JSONL 파일 크기 (캐시 무효화용) |
| `promptCount` | 프롬프트 수 |
| `toolUseCount` | 도구 사용 횟수 |
| `firstTimestamp` | 세션 시작 시각 |
| `lastTimestamp` | 세션 종료 시각 |
| `durationMinutes` | 세션 지속 시간(분) |
| `dominantTools` | 주요 사용 도구 Top 5 |
| `tags` | 세션 태그 (high-activity, has-directives 등) |

### 세션 요약 필드 (summary.json)

| 필드 | 설명 |
|------|------|
| `prompts[]` | 사용자 프롬프트 + 타임스탬프 + 타입 |
| `toolUseCount` | 총 도구 사용 횟수 |
| `toolSequences[]` | 도구 시퀀스 문자열 (예: `"Read → Edit → Bash:test"`) |
| `directives[]` | 지시사항 프롬프트 |
| `corrections[]` | 교정 프롬프트 |
| `filesMutated[]` | 수정된 파일 경로 |
| `staticSignals` | 정적 시그널 (밀도, 복잡도 등) |

## 증분 캐싱

JSONL은 append-only 특성을 가지므로, **파일 크기 비교**로 캐시 유효성을 판단합니다:

- 파일 크기 동일 → 캐시 유효 (파싱 스킵)
- 파일 크기 변경 → 캐시 무효 (재파싱 + 요약 재생성)

## 옵션

| 옵션 | 값 | 기본값 | 설명 |
|------|----|--------|------|
| `--project` | PATH | cwd | 프로젝트 경로 |
| `--depth` | `narrow`, `normal`, `wide` | `normal` | 정적 분석 깊이 |
| `--threshold` | N | 3 | 최소 반복 횟수 |
| `--top` | N | 10 | 상위 N개 결과 |
| `--decay` | flag | off | 시간 감쇠 가중치 |

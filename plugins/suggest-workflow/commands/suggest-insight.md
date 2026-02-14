---
description: v3 SQLite 인덱스 + perspective 쿼리 기반 Claude 시맨틱 인사이트 분석
---

# Suggest Insight

Rust CLI가 구축한 SQLite 인덱스를 쿼리하여 구조적 통계를 얻고, Claude가 시맨틱 해석을 수행합니다.
Rust는 **순수 연산만** 수행하고, **의미 해석과 분류는 LLM이** 담당합니다.

## 2-Phase 아키텍처

```
Phase 1: Rust CLI (JSONL → 인크리멘털 인덱싱 → SQLite DB, 룰 0개)
    ↓ query --perspective (JSON 배열) + analysis-snapshot.json (복잡한 분석)
Phase 2: Claude (통계 해석 + 분류 + 인사이트)  ← 이 커맨드
```

## 사용법

```bash
# 1. 바이너리 빌드 확인
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
CLI="${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow"

# 2. v3 인덱싱 (인크리멘털 — 변경된 세션만 파싱)
$CLI index --project "$(pwd)"

# 3. v3 perspective 쿼리 (필요한 관점만 선택적으로)
$CLI query --project "$(pwd)" --perspective tool-frequency --param top=20
$CLI query --project "$(pwd)" --perspective transitions --param tool=Edit

# 4. 캐시도 생성 (v2 호환 + 복잡한 분석용, DB도 자동 갱신)
CACHE_DIR=$($CLI --project "$(pwd)" --cache)
```

## v3 빌트인 perspectives

`query --perspective`로 조회 가능한 9개 빌트인 관점:

| Perspective | 파라미터 | 설명 |
|-------------|---------|------|
| `tool-frequency` | `top=10` | 도구 사용 빈도 (분류명 기준) |
| `transitions` | `tool` (필수) | 특정 도구 이후 전이 확률 |
| `trends` | `since=2020-01-01` | 주간 도구 사용 트렌드 |
| `hotfiles` | `top=20` | 자주 편집되는 파일 핫스팟 |
| `repetition` | `z_threshold=2.0` | 반복/이상치 탐지 (z-score² 기반) |
| `prompts` | `search` (필수), `top=20` | 프롬프트 키워드 검색 |
| `session-links` | `min_overlap=0.3` | 파일 공유 기반 세션 연결 |
| `sequences` | `min_count=3` | 자주 등장하는 도구 시퀀스 (2-gram) |
| `sessions` | `top=20` | 세션 목록 및 요약 |

결과는 항상 JSON 배열로 stdout에 출력됩니다.

## 캐시 구조 (v2 호환)

```
~/.claude/suggest-workflow-cache/{project-encoded}/
  ├── index.json                    # 세션 메타 인덱스
  ├── sessions/
  │   ├── {session-id}.summary.json # 세션별 통계 요약
  │   └── ...
  └── analysis-snapshot.json        # 전체 분석 스냅샷 (DB + 복잡한 분석)
```

### analysis-snapshot.json 분석 축

| 분석 | 키 | 데이터 소스 | 설명 |
|------|-----|-----------|------|
| 프롬프트 빈도 | `promptAnalysis` | in-memory | Top 반복 프롬프트 (BM25) |
| 도구 시퀀스 | `workflowAnalysis` | in-memory | 공통 시퀀스 패턴 |
| 프롬프트 클러스터 | `tacitKnowledge` | in-memory | BM25 클러스터 (LLM이 분류) |
| 의존성 그래프 | `dependencyGraph` | in-memory | 도구 의존성 그래프 |
| **도구 전이 행렬** | `toolTransitions` | **SQLite** | A→B 전이 확률 |
| **반복/이상치** | `repetitionStats` | **SQLite** | 세션별 도구 사용 카운트 |
| **주간 트렌드** | `weeklyTrends` | **SQLite** | 도구별 주간 카운트 |
| **파일 핫스팟** | `fileAnalysis` | **SQLite** | 파일별 편집 횟수 |
| **세션 연결** | `sessionLinks` | **SQLite** | 파일 겹침 + 시간 근접 |

### 세션 요약 필드 (summary.json)

| 필드 | 설명 |
|------|------|
| `prompts[]` | 사용자 프롬프트 + 타임스탬프 |
| `toolUseCount` | 총 도구 사용 횟수 |
| `toolSequences[]` | 도구 시퀀스 문자열 (예: `"Read → Edit → Bash:test"`) |
| `filesMutated[]` | 수정된 파일 경로 |
| `stats` | 순수 정량 통계 (프롬프트 수, 도구 수, 전이 횟수, 파일 편집 수 등) |

## 인덱스 전략 (v3)

- **인크리멘털 인덱싱**: 변경된 세션만 재파싱 (size + mtime 변경 감지)
- **`--full` 플래그**: 전체 재구축 (스키마 변경 시)
- **DB 위치**: `~/.claude/suggest-workflow-index/{project-encoded}/index.db`
- **`--cache` 실행 시**: DB도 자동으로 갱신됨

## 옵션

### v3 서브커맨드

| 커맨드 | 설명 |
|--------|------|
| `index --project PATH [--full]` | 인크리멘털 인덱싱 |
| `query --perspective NAME [--param K=V]...` | perspective 쿼리 |
| `query --sql-file PATH` | 커스텀 SQL 실행 (SELECT만) |
| `query --list-perspectives` | 사용 가능한 perspective 목록 |

### v2 호환 옵션

| 옵션 | 값 | 기본값 | 설명 |
|------|----|--------|------|
| `--project` | PATH | cwd | 프로젝트 경로 |
| `--cache` | flag | off | 캐시 생성 (v3 DB도 자동 갱신) |
| `--depth` | `narrow`, `normal`, `wide` | `normal` | 클러스터링 깊이 |
| `--threshold` | N | 3 | 최소 반복 횟수 |
| `--top` | N | 10 | 상위 N개 결과 |
| `--decay` | flag | off | 시간 감쇠 가중치 |
| `--exclude-words` | WORD,WORD,... | - | 분석에서 제외할 노이즈 단어 |
